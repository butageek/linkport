//! The portal daemon: axum HTTP server on 127.0.0.1 serving the JSON API
//! and the embedded SPA, with token auth and DNS-rebinding protection.

use crate::{api, open, paths};
use anyhow::{Context, Result};
use axum::extract::{Request, State};
use axum::http::{header, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use linkport_core::engine::{self, Outcome};
use rust_embed::RustEmbed;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

// Resolved relative to this crate's manifest (crates/linkport/).
#[derive(RustEmbed)]
#[folder = "../../frontend/out"]
struct Assets;

#[derive(Clone)]
pub struct AppState {
    pub token: String,
    pub port: u16,
    /// Result of the latest update check (startup or tray), shared with
    /// the API status endpoint and the tray menu. `None` = not checked yet.
    pub update: Arc<Mutex<Option<crate::update::UpdateCheck>>>,
    /// Shutdown signal: `true` = quit (tray Quit/Restart, `/api/quit` from
    /// `linkport-cli stop`). `serve()` holds the receiver; every clone may
    /// send.
    pub quit: tokio::sync::watch::Sender<bool>,
}

impl AppState {
    pub fn new(token: String, port: u16) -> Self {
        Self {
            token,
            port,
            update: Arc::new(Mutex::new(None)),
            quit: tokio::sync::watch::channel(false).0,
        }
    }

    /// A copy of the latest update-check result, if any (the lock is held
    /// only for the clone).
    pub fn snapshot_update(&self) -> Option<crate::update::UpdateCheck> {
        self.update.lock().ok().and_then(|s| s.clone())
    }

    /// Publish a check result to the shared state (best-effort: a poisoned
    /// lock just drops the write).
    pub fn store_update(&self, result: crate::update::UpdateCheck) {
        if let Ok(mut slot) = self.update.lock() {
            *slot = Some(result);
        }
    }
}

pub fn serve(port_override: Option<u16>, open_browser: bool) -> Result<()> {
    let cfg = paths::load_or_default();
    let port = port_override.unwrap_or(cfg.portal.port);
    // Only read on Windows (the first-run portal pop below is Windows-only);
    // must be captured before `ensure_token` creates the file.
    #[cfg(windows)]
    let token_existed = paths::token_path().exists();
    let token = paths::ensure_token()?;
    let state = AppState::new(token, port);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    runtime.block_on(async move {
        let public = Router::new().route("/api/ping", get(api::ping));

        let protected = Router::new()
            .route("/api/status", get(api::status))
            .route("/api/config", get(api::get_config).put(api::put_config))
            .route("/api/browsers", get(api::browsers))
            .route("/api/test", post(api::test_url))
            .route("/api/events", get(api::events).delete(api::clear_events))
            .route("/api/register", post(api::register))
            .route("/api/unregister", post(api::unregister))
            .route("/api/update/check", post(api::check_update))
            .route("/api/update/install", post(api::install_update))
            .route("/api/quit", post(api::quit))
            .with_state(state.clone())
            .route_layer(middleware::from_fn_with_state(state.clone(), auth));

        let app = public.merge(protected).fallback(static_handler);
        // Dev only: let the Vite dev server (localhost:3000) call the API
        // (`cargo run -- serve` is a debug build). The production SPA is
        // same-origin, so release builds ship no CORS layer at all.
        #[cfg(debug_assertions)]
        let app = {
            use axum::http::HeaderValue;
            app.layer(
                tower_http::cors::CorsLayer::new()
                    .allow_origin([
                        "http://localhost:3000".parse::<HeaderValue>().unwrap(),
                        "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
                    ])
                    .allow_methods(tower_http::cors::Any)
                    .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
            )
        };

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("failed to bind {addr}"))?;

        println!("Linkport portal listening on http://127.0.0.1:{port}");
        println!("Manage your rules at {}", portal_url_string(&state));

        if open_browser {
            open_portal_url(&state);
        }

        // System tray presence (Windows). Quit in the tray menu — or
        // `/api/quit` from `linkport-cli stop` — sends on the state's watch
        // channel, gracefully shutting the HTTP server down.
        let mut quit_rx = state.quit.subscribe();
        #[cfg(windows)]
        let tray = crate::tray::spawn(state.clone());

        // Update check at startup (config-gated; the tray's manual check
        // always works). Runs off the async runtime — a slow or offline
        // network must never delay or block the portal.
        if cfg.check_updates {
            let upd_state = state.clone();
            tokio::task::spawn_blocking(move || {
                let result = crate::update::check();
                if result.available {
                    println!(
                        "linkport: update available: v{} (running v{}) — see the \
                         tray menu or the portal",
                        result.latest.as_deref().unwrap_or("?"),
                        result.current
                    );
                }
                upd_state.store_update(result);
                // Refresh the tray menu text if a tray is up (no-op without one).
                #[cfg(windows)]
                crate::tray::notify_update_checked();
            });
        }

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
            heal_windows_registration();
            if let Err(e) = linkport_win::shortcut::ensure_start_menu_shortcut(&main_exe) {
                eprintln!("linkport: start menu shortcut: {e}");
            }
        }

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = quit_rx.changed().await;
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

/// Outcome of [`request_quit`].
#[derive(Debug, PartialEq, Eq)]
pub enum QuitOutcome {
    /// A daemon was listening and has shut down its listener.
    Stopped,
    /// Nothing listens on the portal port.
    NotRunning,
}

/// Ask the running daemon to quit gracefully — the tray's Quit, over HTTP
/// — from another process (`linkport-cli stop`, which the installer runs
/// before replacing the exes). `NotRunning` when nothing listens; an
/// error when a daemon listens but does not go away.
pub fn request_quit() -> Result<QuitOutcome> {
    let port = paths::load_or_default().portal.port;
    request_quit_at(
        SocketAddr::from(([127, 0, 0, 1], port)),
        &paths::token_path(),
    )
}

/// [`request_quit`] against an explicit listener and token file (so the
/// client logic is testable off the real daemon): POST `/api/quit` with the
/// bearer token, then wait for the listener to go away.
fn request_quit_at(addr: SocketAddr, token_path: &std::path::Path) -> Result<QuitOutcome> {
    if std::net::TcpStream::connect(addr).is_err() {
        return Ok(QuitOutcome::NotRunning);
    }
    let token = std::fs::read_to_string(token_path).with_context(|| {
        format!(
            "a daemon listens on {addr} but {} holds no token",
            token_path.display()
        )
    })?;
    match ureq::post(&format!("http://{addr}/api/quit"))
        .set("Authorization", &format!("Bearer {}", token.trim()))
        .call()
    {
        Ok(_) => {}
        Err(ureq::Error::Status(code, _)) => {
            anyhow::bail!("the daemon answered {code} to the stop request")
        }
        Err(e) => anyhow::bail!("cannot reach the daemon: {e}"),
    }
    // The handler answers first and shuts down after; wait for the listener
    // to disappear so the caller knows the port is free.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::net::TcpStream::connect(addr).is_ok() {
        if std::time::Instant::now() > deadline {
            anyhow::bail!("the daemon on {addr} is still listening 5s after the stop request");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(QuitOutcome::Stopped)
}

/// Self-heal the Windows integration after the install moved (the release
/// zip extracts to a version-named folder, so an upgrade-by-extract leaves
/// the registered handler pointing at the deleted old copy — links then
/// fail with "Application not found" and nothing reaches the event log).
///
/// - URL handler: when the registered command's exe is missing *or* is a
///   different copy of Linkport than the one running, rewrite the command
///   values in place. No key is created or deleted, so the user's
///   default-browser `UserChoice` (hashed over the ProgId name, not its
///   contents) survives — the same in-place update browsers ship on every
///   release. A deliberately unregistered install is left alone.
/// - Login auto-start: repair only a stale Run value (one whose exe is
///   gone); "disabled" has no value and stays off.
#[cfg(windows)]
pub fn heal_windows_registration() {
    let handler = crate::default_handler_command();
    if linkport_win::is_registered() {
        let current_exe = linkport_core::launcher::parse_command_template(&handler).0;
        let stale = linkport_win::registered_handler()
            .map(|cmd| {
                let (exe, _) = linkport_core::launcher::parse_command_template(&cmd);
                !same_windows_path(&exe, &current_exe)
            })
            .unwrap_or(true);
        if stale {
            match linkport_win::repair_handler(&handler) {
                Ok(()) => println!(
                    "linkport: registered handler pointed at a different install — \
                     updated to {handler}"
                ),
                Err(e) => eprintln!("linkport: {e} — re-register via the portal's Settings"),
            }
        }
    }
    match linkport_win::autostart::repair_stale(&crate::autostart_command()) {
        Ok(true) => println!("linkport: repaired stale login auto-start entry"),
        Ok(false) => {}
        Err(e) => eprintln!("linkport: auto-start repair: {e}"),
    }
}

/// Case-insensitive Windows path comparison, slash-style agnostic.
#[cfg(windows)]
fn same_windows_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
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
            // The portal URL is not a wrapper; resolve/block/no-match all
            // fall through to the direct-launch chain below.
            Outcome::Blocked { .. } | Outcome::NoMatch | Outcome::Resolve { .. } => false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// One token file per test: the tests run concurrently and one of them
    /// deletes its file.
    fn token_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("linkport-token-{name}-{}.txt", std::process::id()))
    }

    /// Accept connections until one carries a request, answer it 200 and
    /// go away — the shape of a daemon that quits on `/api/quit`. The
    /// client's liveness probe connects first and sends nothing.
    fn fake_daemon(token_path: &std::path::Path) -> (SocketAddr, std::thread::JoinHandle<String>) {
        std::fs::write(token_path, "test-token\n").unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut request = String::new();
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    continue; // the probe: connected and closed
                }
                request = String::from_utf8_lossy(&buf[..n]).to_string();
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                    )
                    .unwrap();
                break;
            }
            drop(listener); // the daemon goes away
            request
        });
        (addr, handle)
    }

    #[test]
    fn request_quit_without_a_daemon_is_not_running() {
        // An ephemeral port we just released: nothing listens there.
        let addr = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let token = token_file("without");
        let _ = std::fs::remove_file(&token);
        assert_eq!(
            request_quit_at(addr, &token).unwrap(),
            QuitOutcome::NotRunning
        );
    }

    #[test]
    fn request_quit_sends_the_bearer_token_and_waits_for_the_listener() {
        let token = token_file("with");
        let (addr, server) = fake_daemon(&token);
        assert_eq!(request_quit_at(addr, &token).unwrap(), QuitOutcome::Stopped);
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /api/quit "), "{request}");
        assert!(
            request.contains("Authorization: Bearer test-token"),
            "{request}"
        );
        let _ = std::fs::remove_file(&token);
    }
}
