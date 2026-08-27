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
    Json(cfg): Json<config::Config>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let warnings = config::validate(&cfg);
    config::save(&paths::config_path(), &cfg)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "ok": true, "warnings": warnings })))
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

pub async fn register() -> Json<Value> {
    match std::env::current_exe() {
        Ok(exe) => {
            let handler = crate::default_handler_command();
            match linkport_win::register(&exe, &handler) {
                Ok(()) => Json(json!({
                    "ok": true,
                    "detail": format!("Registered with handler: {handler}. Now set Linkport as the default for HTTP/HTTPS in Windows Settings > Apps > Default apps > Linkport.")
                })),
                Err(e) => Json(json!({ "ok": false, "detail": e })),
            }
        }
        Err(e) => Json(json!({ "ok": false, "detail": e.to_string() })),
    }
}

pub async fn unregister() -> Json<Value> {
    match linkport_win::unregister() {
        Ok(()) => Json(json!({ "ok": true, "detail": "Unregistered." })),
        Err(e) => Json(json!({ "ok": false, "detail": e })),
    }
}
