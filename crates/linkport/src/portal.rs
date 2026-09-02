//! The portal daemon: axum HTTP server on 127.0.0.1 serving the JSON API
//! and the embedded SPA, with token auth and DNS-rebinding protection.

use crate::{api, open, paths};
use anyhow::{Context, Result};
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use linkport_core::engine::{self, Outcome};
use rust_embed::RustEmbed;
use std::net::SocketAddr;

// Resolved relative to this crate's manifest (crates/linkport/).
#[derive(RustEmbed)]
#[folder = "../../frontend/out"]
struct Assets;

#[derive(Clone)]
pub struct AppState {
    pub token: String,
    pub port: u16,
}

pub fn serve(port_override: Option<u16>, open_browser: bool) -> Result<()> {
    let cfg = paths::load_or_default();
    let port = port_override.unwrap_or(cfg.portal.port);
    // Only read on Windows (the first-run portal pop below is Windows-only);
    // must be captured before `ensure_token` creates the file.
    #[cfg(windows)]
    let token_existed = paths::token_path().exists();
    let token = paths::ensure_token()?;
    let state = AppState { token, port };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    runtime.block_on(async move {
        // Allow the Vite dev server (localhost:3000) to call the API during
        // development. The production SPA is same-origin and unaffected.
        let cors = tower_http::cors::CorsLayer::new()
            .allow_origin([
                "http://localhost:3000".parse::<HeaderValue>().unwrap(),
                "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
            ])
            .allow_methods(tower_http::cors::Any)
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

        let public = Router::new().route("/api/ping", get(api::ping));

        let protected = Router::new()
            .route("/api/status", get(api::status))
            .route("/api/config", get(api::get_config).put(api::put_config))
            .route("/api/browsers", get(api::browsers))
            .route("/api/test", post(api::test_url))
            .route("/api/events", get(api::events).delete(api::clear_events))
            .route("/api/register", post(api::register))
            .route("/api/unregister", post(api::unregister))
            .with_state(state.clone())
            .route_layer(middleware::from_fn_with_state(state.clone(), auth));

        let app = public.merge(protected).fallback(static_handler).layer(cors);

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("failed to bind {addr}"))?;

        println!("Linkport portal listening on http://127.0.0.1:{port}");
        println!("Manage your rules at {}", portal_url_string(&state));

        if open_browser {
            open_portal_url(&state);
        }

        // System tray presence (Windows). Quit in the tray menu sends on this
        // watch channel, gracefully shutting the HTTP server down.
        let (quit_tx, quit_rx) = tokio::sync::watch::channel(false);
        #[cfg(windows)]
        let tray = crate::tray::spawn(state.clone(), quit_tx.clone());
        #[cfg(not(windows))]
        let _ = quit_tx;

        // First run ever (fresh install + auto-start): the token has just been
        // minted and the user has no other way to discover it — pop the portal
        // once so setup can begin.
        #[cfg(windows)]
        if !token_existed {
            open_portal_url(&state);
        }

        // Keep a Start Menu entry so the daemon can be restarted from Start
        // after a tray Quit (created once; cheap existence check otherwise).
        #[cfg(windows)]
        if let Some(main_exe) = crate::main_binary_path() {
            if let Err(e) = linkport_win::shortcut::ensure_start_menu_shortcut(&main_exe) {
                eprintln!("linkport: start menu shortcut: {e}");
            }
        }

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let mut rx = quit_rx;
                let _ = rx.changed().await;
            })
            .await
            .context("portal server error")?;

        #[cfg(windows)]
        tray.stop();
        anyhow::Ok(())
    })
}

/// The tokenized portal URL (opens straight into the dashboard, no gate).
pub fn portal_url_string(state: &AppState) -> String {
    format!("http://127.0.0.1:{}/?token={}", state.port, state.token)
}

/// Open the portal in a real browser.
///
/// The URL goes through the rule engine first, so users can pin the portal
/// to a browser with a rule (e.g. `*.127.0.0.1` → Chrome). When routing
/// can't decide (fresh install, blocked, no match, paused, launch failure)
/// it falls back to: configured default browser → any configured browser →
/// first registry-discovered browser → system handler. The fallback never
/// routes through Linkport itself, so it works even when Linkport is the
/// system default browser and nothing is configured yet.
pub fn open_portal_url(state: &AppState) {
    let url = portal_url_string(state);

    let cfg = paths::load_or_default();
    if !paths::is_paused() {
        let decision = engine::evaluate(&cfg, &url);
        let launched = match &decision.outcome {
            Outcome::RuleMatched {
                target, incognito, ..
            } => open::launch_browser(&cfg, target, &url, *incognito).is_ok(),
            Outcome::Default { target } => open::launch_browser(&cfg, target, &url, false).is_ok(),
            Outcome::Blocked { .. } | Outcome::NoMatch => false,
        };
        if launched {
            return;
        }
    }

    if let Some(browser) = portal_browser() {
        let launched = linkport_core::launcher::launch(&browser, &url, false).is_ok();
        if launched {
            return;
        }
    }
    open_in_system_browser(&url);
}

fn portal_browser() -> Option<linkport_core::Browser> {
    let cfg = paths::load_or_default();
    if let Some(b) = cfg
        .default_browser
        .as_deref()
        .and_then(|id| cfg.browsers.get(id))
    {
        return Some(b.clone());
    }
    if let Some((_, b)) = cfg.browsers.iter().next() {
        return Some(b.clone());
    }
    linkport_win::discover_browsers().first().map(|d| {
        let (exe, args) = linkport_core::launcher::parse_command_template(&d.command);
        linkport_core::Browser {
            display_name: d.display_name.clone(),
            exe,
            args,
            incognito_args: None,
        }
    })
}

/// Auth middleware for `/api/*` routes.
///
/// - Rejects requests whose Host header is not localhost (DNS rebinding).
/// - Requires the bearer token in the `Authorization` header or `?token=`.
async fn auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let host_ok = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| {
            let host = h.rsplit_once(':').map(|(h, _)| h).unwrap_or(h);
            host == "127.0.0.1" || host == "localhost" || host == "[::1]"
        })
        .unwrap_or(false);
    if !host_ok {
        return Err(StatusCode::FORBIDDEN);
    }

    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
        .or_else(|| {
            req.uri()
                .query()
                .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("token=")))
                .map(str::to_owned)
        })
        .unwrap_or_default();

    if !constant_time_eq(provided.as_bytes(), state.token.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Open a URL with the system handler (default browser) without flashing a
/// console window. Used by `linkport-cli serve --open`.
fn open_in_system_browser(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    serve_asset(if path.is_empty() { "index.html" } else { path })
}

fn serve_asset(path: &str) -> Response {
    if let Some(file) = Assets::get(path) {
        return asset_response(path, &file);
    }
    // Client-side routes: /rules -> rules.html (Next static export layout).
    let html = format!("{path}.html");
    if let Some(file) = Assets::get(&html) {
        return asset_response(&html, &file);
    }
    if let Some(file) = Assets::get("index.html") {
        return asset_response("index.html", &file);
    }
    (
        StatusCode::NOT_FOUND,
        "portal assets not built; run the frontend build first",
    )
        .into_response()
}

fn asset_response(path: &str, file: &rust_embed::EmbeddedFile) -> Response {
    let mime = match path.rsplit('.').next().unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "css" => "text/css",
        "json" | "map" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .body(axum::body::Body::from(file.data.clone()))
        .unwrap()
}
