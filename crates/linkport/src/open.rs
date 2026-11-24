//! The hot path: read config, evaluate rules, launch the browser, log, exit.
//! Daemon-independent by design so links keep working when the portal is down.

use crate::paths;
use anyhow::{Context, Result};
use linkport_core::engine::{self, Decision, Outcome};
use linkport_core::event;
use linkport_core::{config, launcher};

/// Route a URL and open it. Always records an event (best-effort), including
/// an `error` field when routing or launching failed.
pub fn open_url(url: &str) -> Result<()> {
    let cfg = paths::load_or_default();

    let (outcome, host, resolved, result) = if paths::is_paused() {
        // Routing paused: send everything to the configured default browser.
        let target = cfg
            .default_browser
            .clone()
            .filter(|t| cfg.browsers.contains_key(t));
        match target {
            Some(t) => {
                let result = launch_browser(&cfg, &t, url, false);
                (Outcome::Default { target: t }, None, None, result)
            }
            None => (
                Outcome::NoMatch,
                None,
                None,
                Err(anyhow::anyhow!(
                    "routing is paused but no default browser is configured"
                )),
            ),
        }
    } else {
        let decision = decide(&cfg, url);
        // Launch the engine's canonical URL (scheme-less input gets the
        // https:// it assumed) or the redirect-resolved destination.
        let final_url = decision.resolved_url.as_deref().unwrap_or(&decision.url);
        let result = match &decision.outcome {
            Outcome::RuleMatched {
                target, incognito, ..
            } => launch_browser(&cfg, target, final_url, *incognito),
            Outcome::Default { target } => launch_browser(&cfg, target, final_url, false),
            // Blocked URLs are swallowed without opening anything.
            Outcome::Blocked { .. } => Ok(()),
            // decide() always resolves this to a terminal outcome; defensive.
            Outcome::Resolve { .. } => Err(anyhow::anyhow!(
                "redirect was not resolved to a terminal outcome"
            )),
            Outcome::NoMatch => Err(anyhow::anyhow!(
                "no rule matched and no default browser is configured"
            )),
        };
        (
            decision.outcome,
            decision.host,
            decision.resolved_url,
            result,
        )
    };

    let error = result.as_ref().err().map(|e| e.to_string());
    // Event contract: url = what actually opened (the final destination),
    // origin_url = the clicked link when redirect resolution moved it.
    let (logged_url, origin_url) = match resolved {
        Some(final_url) => (final_url, Some(url.to_string())),
        None => (url.to_string(), None),
    };
    let _ = event::append(
        &paths::events_path(),
        &event::Event {
            error,
            origin_url,
            ..event::Event::new(logged_url, host, outcome)
        },
    );

    result
}

/// Evaluate the rules, following redirects for links marked as wrappers:
/// either a rule with the `resolve` target matched, or (legacy config) the
/// clicked host sits in `redirect_hosts` and no rule matched. The rules
/// then see the final destination. `decision.url` stays the clicked link;
/// `decision.resolved_url` carries the final URL.
///
/// When nothing is resolved (the link did not redirect, the fetch failed,
/// a non-web scheme) the clicked link itself is routed: a `resolve` target
/// is never terminal, so it degrades to the default browser. The outcome
/// returned here is therefore always terminal.
pub fn decide(cfg: &config::Config, url: &str) -> Decision {
    let mut decision = engine::evaluate(cfg, url);
    if should_resolve_redirects(cfg, &decision) {
        match crate::redirect::resolve(url) {
            Some(final_url) => {
                let mut redirected = engine::evaluate(cfg, &final_url);
                // A chain that lands on another wrapper (or loops back)
                // stops here: fall back to the default browser rather than
                // following forever.
                if matches!(redirected.outcome, Outcome::Resolve { .. }) {
                    redirected.outcome = engine::default_outcome(cfg);
                }
                redirected.url = url.to_string();
                redirected.resolved_url = Some(final_url);
                decision = redirected;
            }
            // For the legacy redirect_hosts path the outcome is already
            // terminal (Default/NoMatch) and this is a no-op.
            None => decision.outcome = engine::default_outcome(cfg),
        }
    }
    decision
}

/// Redirect resolution is deliberately gated (latency + privacy: matched
/// links go straight out, and no host is contacted unless the user marked
/// it a wrapper) via either of:
/// - a rule with the `resolve` target matching the clicked link, or
/// - the legacy `redirect_hosts` list when NO rule matched the link.
fn should_resolve_redirects(cfg: &config::Config, decision: &Decision) -> bool {
    match &decision.outcome {
        Outcome::Resolve { .. } => true,
        Outcome::Default { .. } | Outcome::NoMatch => decision.host.as_deref().is_some_and(|h| {
            cfg.redirect_hosts
                .iter()
                .any(|p| engine::host_pattern_matches(p, h))
        }),
        _ => false,
    }
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
    let decision = decide(&cfg, url);
    println!("{}", serde_json::to_string_pretty(&decision)?);
    Ok(())
}

pub fn list_browsers() -> Result<()> {
    let cfg = paths::load_or_default();
    if cfg.browsers.is_empty() {
        println!("no browsers configured — run `linkport-cli serve` and use the portal");
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
            println!("  {:<24} {}", d.display_name, d.command);
        }
    }
    match &cfg.default_browser {
        Some(def) => println!("default: {def}"),
        None if !cfg.browsers.is_empty() => println!("default: (none set)"),
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use linkport_core::config::{Browser, Rule, TARGET_RESOLVE};

    fn cfg(rules: Vec<Rule>, redirect_hosts: &[&str], default: Option<&str>) -> config::Config {
        let mut c = config::Config::default();
        c.browsers.insert(
            "ff".into(),
            Browser {
                display_name: "Firefox".into(),
                exe: "firefox".into(),
                args: vec!["{url}".into()],
                incognito_args: None,
            },
        );
        c.default_browser = default.map(String::from);
        c.rules = rules;
        c.redirect_hosts = redirect_hosts.iter().map(|s| s.to_string()).collect();
        c
    }

    fn resolve_rule() -> Rule {
        Rule {
            name: "tracker".into(),
            enabled: true,
            host_glob: Some("127.0.0.1".into()),
            url_contains: None,
            url_regex: None,
            scheme: None,
            target: TARGET_RESOLVE.into(),
            incognito: false,
        }
    }

    /// Nothing listens on port 1 of the loopback interface, so the redirect
    /// fetch is refused on the spot — no network, no timeout wait.
    const DEAD_LINK: &str = "http://127.0.0.1:1/x";

    #[test]
    fn resolve_rule_degrades_to_default_when_nothing_is_resolved() {
        let c = cfg(vec![resolve_rule()], &[], Some("ff"));
        let d = decide(&c, DEAD_LINK);
        assert_eq!(
            d.outcome,
            Outcome::Default {
                target: "ff".into()
            }
        );
        assert_eq!(d.url, DEAD_LINK, "the clicked link is what gets routed");
        assert!(d.resolved_url.is_none());
        assert!(
            d.trace[0].matched,
            "the resolve rule still traces as matched"
        );
    }

    #[test]
    fn resolve_rule_without_default_degrades_to_no_match() {
        let c = cfg(vec![resolve_rule()], &[], None);
        assert_eq!(decide(&c, DEAD_LINK).outcome, Outcome::NoMatch);
    }

    #[test]
    fn legacy_redirect_hosts_keep_their_terminal_outcome() {
        let c = cfg(vec![], &["127.0.0.1"], Some("ff"));
        let d = decide(&c, DEAD_LINK);
        assert_eq!(
            d.outcome,
            Outcome::Default {
                target: "ff".into()
            }
        );
        assert!(d.resolved_url.is_none());
    }
}
