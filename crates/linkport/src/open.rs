//! The hot path: read config, evaluate rules, launch the browser, log, exit.
//! Daemon-independent by design so links keep working when the portal is down.

use anyhow::{Context, Result};
use linkport_core::engine::{self, Outcome};
use linkport_core::event;
use linkport_core::{config, launcher};

/// Route a URL and open it. Always records an event (best-effort), including
/// an `error` field when routing or launching failed.
pub fn open_url(url: &str) -> Result<()> {
    let cfg = paths::load_or_default();
    let decision = engine::evaluate(&cfg, url);
    let outcome = decision.outcome.clone();

    let result: Result<()> = match &outcome {
        Outcome::RuleMatched {
            target, incognito, ..
        } => launch_browser(&cfg, target, url, *incognito),
        Outcome::Default { target } => launch_browser(&cfg, target, url, false),
        Outcome::Blocked { .. } => Ok(()),
        Outcome::NoMatch => Err(anyhow::anyhow!(
            "no rule matched and no default browser is configured"
        )),
    };

    let error = result.as_ref().err().map(|e| e.to_string());
    let _ = event::append(
        &paths::events_path(),
        &event::Event {
            error,
            ..event::Event::new(url, decision.host.clone(), outcome)
        },
    );

    result
}

pub fn launch_browser(
    cfg: &config::Config,
    target: &str,
    url: &str,
    incognito: bool,
) -> Result<()> {
    let browser = cfg
        .browsers
        .get(target)
        .with_context(|| format!("browser '{target}' is not configured"))?;
    launcher::launch(browser, url, incognito)
        .map(|_| ())
        .with_context(|| format!("failed to launch '{}'", browser.exe))
}

pub fn test_url(url: &str) -> Result<()> {
    let cfg = paths::load_or_default();
    let decision = engine::evaluate(&cfg, url);
    println!("{}", serde_json::to_string_pretty(&decision)?);
    Ok(())
}

pub fn list_browsers() -> Result<()> {
    let cfg = paths::load_or_default();
    if cfg.browsers.is_empty() {
        println!("no browsers configured — run `linkport serve` and use the portal");
    } else {
        println!("configured:");
        for (id, b) in &cfg.browsers {
            println!("  {id:<16} {} -> {}", b.display_name, b.exe);
        }
    }
    let discovered = linkport_win::discover_browsers();
    if !discovered.is_empty() {
        println!("discovered on this system:");
        for d in discovered {
            println!("  {:<16} {}", d.name, d.command);
        }
    }
    match &cfg.default_browser {
        Some(def) => println!("default: {def}"),
        None if !cfg.browsers.is_empty() => println!("default: (none set)"),
        _ => {}
    }
    Ok(())
}

use crate::paths;
