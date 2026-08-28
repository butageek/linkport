//! JSON API handlers.

use crate::{paths, portal::AppState};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use linkport_core::engine::{self, Decision};
use linkport_core::event::{self, Event};
use linkport_core::{config, launcher};
use serde::Deserialize;
use serde_json::{json, Value};

/// Unauthenticated liveness probe (used by the SPA's token gate).
pub async fn ping() -> Json<Value> {
    Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

pub async fn status(State(state): State<AppState>) -> Json<Value> {
    let cfg = paths::load_or_default();
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "registered": linkport_win::is_registered(),
        "paused": paths::is_paused(),
        "autostart": linkport_win::autostart::is_enabled(),
        "config_path": paths::config_path().display().to_string(),
        "portal_url": format!("http://127.0.0.1:{}/?token={}", state.port, state.token),
        "default_browser": cfg.default_browser,
        "rules_count": cfg.rules.len(),
        "browsers_count": cfg.browsers.len(),
    }))
}

pub async fn get_config() -> Json<config::Config> {
    Json(paths::load_or_default())
}

pub async fn put_config(
    Json(mut cfg): Json<config::Config>,
) -> Result<Json<Value>, (StatusCode, String)> {
    cfg.default_browser = resolve_default(&cfg);
    let warnings = config::validate(&cfg);
    config::save(&paths::config_path(), &cfg)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(
        json!({ "ok": true, "warnings": warnings, "config": cfg }),
    ))
}

/// The default browser never dangles: keep a valid choice, else adopt the
/// system default browser when it is among the configured ones, else the
/// first configured browser. `None` only when no browsers are configured.
/// (On the first save this pre-selects the user's current system default.)
fn resolve_default(cfg: &config::Config) -> Option<String> {
    if let Some(d) = &cfg.default_browser {
        if cfg.browsers.contains_key(d) {
            return Some(d.clone());
        }
    }
    if let Some(sys) = linkport_win::system_default_exe() {
        if let Some((id, _)) = cfg.browsers.iter().find(|(_, b)| same_exe(&b.exe, &sys)) {
            return Some(id.clone());
        }
    }
    cfg.browsers.iter().next().map(|(id, _)| id.clone())
}

/// Case-insensitive Windows exe-path comparison, slash-style agnostic.
fn same_exe(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}

pub async fn browsers() -> Json<Value> {
    let cfg = paths::load_or_default();
    let configured: Vec<Value> = cfg
        .browsers
        .iter()
        .map(|(id, b)| json!({ "id": id, "browser": b }))
        .collect();
    let discovered: Vec<Value> = linkport_win::discover_browsers()
        .into_iter()
        .map(|d| {
            let (exe, args) = launcher::parse_command_template(&d.command);
            json!({
                "name": d.name,
                "display_name": d.display_name,
                "command": d.command,
                "suggested": { "exe": exe, "args": args },
            })
        })
        .collect();
    Json(json!({ "configured": configured, "discovered": discovered }))
}

#[derive(Deserialize)]
pub struct TestBody {
    pub url: String,
}

pub async fn test_url(Json(body): Json<TestBody>) -> Json<Decision> {
    let cfg = paths::load_or_default();
    Json(engine::evaluate(&cfg, &body.url))
}

#[derive(Deserialize)]
pub struct EventsParams {
    pub limit: Option<usize>,
}

pub async fn events(Query(p): Query<EventsParams>) -> Json<Vec<Event>> {
    Json(event::read_recent(
        &paths::events_path(),
        p.limit.unwrap_or(50).min(500),
    ))
}

/// Clear the history log. The next routed link recreates the file.
pub async fn clear_events() -> Json<Value> {
    match std::fs::remove_file(paths::events_path()) {
        Ok(()) => Json(json!({ "ok": true })),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Json(json!({ "ok": true })),
        Err(e) => Json(json!({ "ok": false, "detail": e.to_string() })),
    }
}

pub async fn register() -> Json<Value> {
    let handler = crate::default_handler_command();
    let result = match std::env::current_exe() {
        Ok(exe) => linkport_win::register(&exe, &handler).map(|()| {
            format!(
                "Registered with handler: {handler}. Now set Linkport as the \
                 default for HTTP/HTTPS in Windows Settings > Apps > Default apps > Linkport."
            )
        }),
        Err(e) => Err(e.to_string()),
    };
    let (ok, detail) = match result {
        Ok(detail) => (true, detail),
        Err(detail) => (false, detail),
    };
    Json(json!({ "ok": ok, "detail": detail }))
}

pub async fn unregister() -> Json<Value> {
    match linkport_win::unregister() {
        Ok(()) => Json(json!({ "ok": true, "detail": "Unregistered." })),
        Err(e) => Json(json!({ "ok": false, "detail": e })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browsers(ids: &[&str]) -> std::collections::BTreeMap<String, config::Browser> {
        ids.iter()
            .map(|id| {
                (
                    id.to_string(),
                    config::Browser {
                        display_name: id.to_string(),
                        exe: format!(r"C:\Apps\{id}.exe"),
                        args: vec!["{url}".into()],
                        incognito_args: None,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn resolve_keeps_valid_default() {
        let mut cfg = config::Config::default();
        cfg.browsers = browsers(&["brave", "chrome"]);
        cfg.default_browser = Some("chrome".into());
        assert_eq!(resolve_default(&cfg).as_deref(), Some("chrome"));
    }

    #[test]
    fn resolve_ghost_or_missing_default_falls_back_to_first() {
        let mut cfg = config::Config::default();
        cfg.browsers = browsers(&["brave", "chrome"]);
        cfg.default_browser = Some("ghost".into());
        assert_eq!(resolve_default(&cfg).as_deref(), Some("brave"));

        cfg.default_browser = None;
        assert_eq!(
            resolve_default(&cfg).as_deref(),
            Some("brave"),
            "no system default detectable on this host -> first configured"
        );
    }

    #[test]
    fn resolve_no_browsers_stays_none() {
        let cfg = config::Config::default();
        assert_eq!(resolve_default(&cfg), None);
    }

    #[test]
    fn same_exe_matches_case_and_slashes() {
        assert!(same_exe(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            "c:/program files/google/chrome/application/CHROME.exe "
        ));
        assert!(!same_exe(r"C:\a\chrome.exe", r"C:\a\msedge.exe"));
    }
}
