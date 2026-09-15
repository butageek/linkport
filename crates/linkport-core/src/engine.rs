//! The URL rule engine: evaluates a URL against the ordered rule list and
//! produces a decision with a full trace for explainability.

use crate::config::{Config, Rule, TARGET_BLOCK, TARGET_RESOLVE};
use globset::Glob;
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

/// What the engine decided to do with a URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outcome {
    /// A rule matched and names the browser to open.
    RuleMatched {
        rule: String,
        target: String,
        incognito: bool,
    },
    /// No rule matched; the configured default browser is used.
    Default { target: String },
    /// A rule matched and blocked the URL.
    Blocked { rule: String },
    /// A rule matched with the `resolve` target: the caller should follow
    /// the link's redirects and evaluate the rules against the final
    /// destination. Intermediate only — never logged or returned by
    /// `open::decide`, which always resolves it to a terminal outcome.
    Resolve { rule: String },
    /// No rule matched and no usable default browser is configured.
    NoMatch,
}

/// Per-rule explanation of why it did or did not match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleTrace {
    pub name: String,
    pub matched: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub url: String,
    pub host: Option<String>,
    pub scheme: Option<String>,
    pub outcome: Outcome,
    pub trace: Vec<RuleTrace>,
    /// Set when the link went through redirect resolution: the final URL
    /// the outcome was evaluated against (`url` stays the clicked link).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_url: Option<String>,
}

enum MatchResult {
    Match,
    No(String),
}

fn traced(rule: &Rule, matched: bool, reason: String) -> RuleTrace {
    RuleTrace {
        name: rule.name.clone(),
        matched,
        reason,
    }
}

pub fn evaluate(cfg: &Config, raw_url: &str) -> Decision {
    // Scheme-less input ("www.example.com") is normalized like a browser's
    // address bar: https:// is assumed, so rules match on the host and the
    // launched URL is a valid one.
    let raw_url = raw_url.trim();
    let has_scheme = raw_url.contains("://");
    let direct = Url::parse(raw_url).ok();
    // Candidate parse of the assumed https:// form; never tried for full
    // URLs, so they can only ever resolve to themselves.
    let prefixed = if has_scheme {
        None
    } else {
        Url::parse(&format!("https://{raw_url}")).ok()
    };
    let parsed = direct.as_ref().or(prefixed.as_ref());
    // `decision.url` carries the normalized form: input without a ://
    // scheme that parses keeps the assumed https:// prefix; full URLs and
    // unparseable input stay as typed.
    let url = if parsed.is_some() && !has_scheme {
        format!("https://{raw_url}")
    } else {
        raw_url.to_string()
    };
    let host = parsed.and_then(|u| u.host_str()).map(|h| h.to_lowercase());
    let scheme = parsed.map(|u| u.scheme().to_ascii_lowercase());

    let mut trace = Vec::new();
    for rule in &cfg.rules {
        if !rule.enabled {
            trace.push(traced(rule, false, "disabled".to_string()));
            continue;
        }
        match matches_rule(rule, raw_url, host.as_deref(), scheme.as_deref()) {
            MatchResult::Match => {
                trace.push(traced(rule, true, "matched".to_string()));
                // `block`/`resolve` are special targets; anything else is a
                // browser id.
                let outcome = match rule.target.as_str() {
                    TARGET_BLOCK => Outcome::Blocked {
                        rule: rule.name.clone(),
                    },
                    TARGET_RESOLVE => Outcome::Resolve {
                        rule: rule.name.clone(),
                    },
                    _ => Outcome::RuleMatched {
                        rule: rule.name.clone(),
                        target: rule.target.clone(),
                        incognito: rule.incognito,
                    },
                };
                return Decision {
                    url: url.clone(),
                    host,
                    scheme,
                    outcome,
                    trace,
                    resolved_url: None,
                };
            }
            MatchResult::No(reason) => trace.push(traced(rule, false, reason)),
        }
    }

    let outcome = default_outcome(cfg);
    Decision {
        url,
        host,
        scheme,
        outcome,
        trace,
        resolved_url: None,
    }
}

/// What to do when no rule matched: the configured default browser, or
/// `NoMatch` when none is usable. Also the terminal fallback for a
/// redirect chain that ends up on another `resolve` rule.
pub fn default_outcome(cfg: &Config) -> Outcome {
    match &cfg.default_browser {
        Some(t) if cfg.browsers.contains_key(t) => Outcome::Default { target: t.clone() },
        _ => Outcome::NoMatch,
    }
}

/// Cookie-domain host matching shared by rule matching and redirect-host
/// lookups: the pattern (with an optional `*.` prefix stripped) matches the
/// host itself and any subdomain at any depth, dot-boundary respected.
/// Patterns containing other wildcards fall back to glob matching.
pub fn host_pattern_matches(pattern: &str, host: &str) -> bool {
    let host = host.to_lowercase();
    let pat_lower = pattern.to_lowercase();
    let base = pat_lower.strip_prefix("*.").unwrap_or(&pat_lower);
    if base.chars().any(|c| matches!(c, '*' | '?' | '[')) {
        glob_matches(pattern, &host) || glob_matches(base, &host)
    } else {
        host == base || host.ends_with(&format!(".{base}"))
    }
}

/// All specified constraints must match (AND). A rule with no constraints
/// matches everything.
fn matches_rule(rule: &Rule, url: &str, host: Option<&str>, scheme: Option<&str>) -> MatchResult {
    if let Some(want) = &rule.scheme {
        match scheme {
            Some(s) if s.eq_ignore_ascii_case(want) => {}
            _ => return MatchResult::No(format!("scheme {want:?} != {scheme:?}")),
        }
    }
    if let Some(pat) = &rule.host_glob {
        let matched = host.is_some_and(|h| host_pattern_matches(pat, h));
        if !matched {
            return MatchResult::No(format!("host_glob {pat:?} did not match host {host:?}"));
        }
    }
    if let Some(needle) = &rule.url_contains {
        if !url.to_lowercase().contains(&needle.to_lowercase()) {
            return MatchResult::No(format!("url_contains {needle:?} not in url"));
        }
    }
    if let Some(pat) = &rule.url_regex {
        let re = match Regex::new(pat) {
            Ok(r) => r,
            Err(e) => return MatchResult::No(format!("invalid url_regex: {e}")),
        };
        if !re.is_match(url) {
            return MatchResult::No(format!("url_regex {pat:?} did not match url"));
        }
    }
    MatchResult::Match
}

fn glob_matches(pattern: &str, host: &str) -> bool {
    Glob::new(pattern)
        .map(|g| g.compile_matcher().is_match(host))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Browser, PortalConfig, Rule};
    use std::collections::BTreeMap;

    fn browser(exe: &str) -> Browser {
        Browser {
            display_name: exe.to_string(),
            exe: exe.to_string(),
            args: vec!["{url}".into()],
            incognito_args: None,
        }
    }

    fn cfg(rules: Vec<Rule>, default: Option<&str>) -> Config {
        let mut c = Config {
            version: 1,
            portal: PortalConfig::default(),
            default_browser: default.map(String::from),
            browsers: BTreeMap::new(),
            rules,
            redirect_hosts: Vec::new(),
            check_updates: true,
        };
        c.browsers.insert("ff".into(), browser("firefox"));
        c.browsers.insert("chrome".into(), browser("chrome"));
        c
    }

    fn rule(name: &str, host_glob: Option<&str>, target: &str) -> Rule {
        Rule {
            name: name.into(),
            enabled: true,
            host_glob: host_glob.map(String::from),
            url_contains: None,
            url_regex: None,
            scheme: None,
            target: target.into(),
            incognito: false,
        }
    }

    #[test]
    fn scheme_less_input_assumes_https() {
        let c = cfg(
            vec![rule("li", Some("www.linkedin.com"), "ff")],
            Some("chrome"),
        );
        let d = evaluate(&c, "  www.linkedin.com  ");
        assert!(matches!(d.outcome, Outcome::RuleMatched { .. }));
        assert_eq!(d.host.as_deref(), Some("www.linkedin.com"));
        assert_eq!(d.url, "https://www.linkedin.com");
        // full URLs pass through untouched
        let d2 = evaluate(&c, "https://www.linkedin.com/feed/");
        assert_eq!(d2.url, "https://www.linkedin.com/feed/");
    }

    #[test]
    fn resolve_target_yields_resolve_outcome() {
        let c = cfg(
            vec![
                rule("tracker", Some("track.example.com"), TARGET_RESOLVE),
                rule("portal", Some("portal.example.com"), "ff"),
            ],
            Some("chrome"),
        );
        let d = evaluate(&c, "http://track.example.com/c?p=1");
        assert_eq!(
            d.outcome,
            Outcome::Resolve {
                rule: "tracker".into()
            }
        );
        // the destination rule matches normally on the final URL
        assert!(matches!(
            evaluate(&c, "https://portal.example.com/x").outcome,
            Outcome::RuleMatched { .. }
        ));
    }

    #[test]
    fn url_contains_is_case_insensitive_and_anded_with_host() {
        let mut r = rule("sso", Some("login.example.com"), "ff");
        r.url_contains = Some("redirect_uri=https%3A%2F%2Fsecurity.example.com".into());
        let c = cfg(vec![r], None);
        assert!(matches!(
            evaluate(
                &c,
                "https://login.example.com/auth?redirect_uri=https%3A%2F%2FSECURITY.example.com%2F"
            )
            .outcome,
            Outcome::RuleMatched { .. }
        ));
        // substring absent -> no match
        assert_eq!(
            evaluate(
                &c,
                "https://login.example.com/auth?redirect_uri=https%3A%2F%2Fmail.example.com%2F"
            )
            .outcome,
            Outcome::NoMatch
        );
        // substring present but host differs -> no match
        assert_eq!(
            evaluate(
                &c,
                "https://other.com/auth?redirect_uri=https%3A%2F%2Fsecurity.example.com%2F"
            )
            .outcome,
            Outcome::NoMatch
        );
    }

    #[test]
    fn host_pattern_matches_cookie_domain() {
        assert!(host_pattern_matches("example.com", "example.com"));
        assert!(host_pattern_matches("example.com", "a.example.com"));
        assert!(host_pattern_matches("*.example.com", "a.example.com"));
        assert!(!host_pattern_matches("example.com", "notexample.com"));
        assert!(!host_pattern_matches(
            "example.com",
            "a.example.com.evil.io"
        ));
    }

    #[test]
    fn first_matching_rule_wins() {
        let c = cfg(
            vec![
                rule("first", Some("*.example.com"), "ff"),
                rule("second", Some("*"), "chrome"),
            ],
            Some("chrome"),
        );
        let d = evaluate(&c, "https://api.example.com/x");
        assert_eq!(
            d.outcome,
            Outcome::RuleMatched {
                rule: "first".into(),
                target: "ff".into(),
                incognito: false
            }
        );
    }

    #[test]
    fn disabled_rules_are_skipped_but_traced() {
        let mut r = rule("off", Some("*.example.com"), "ff");
        r.enabled = false;
        let c = cfg(vec![r], None);
        let d = evaluate(&c, "https://a.example.com/");
        assert_eq!(d.outcome, Outcome::NoMatch);
        assert_eq!(d.trace[0].reason, "disabled");
    }

    #[test]
    fn host_glob_semantics() {
        let c = cfg(vec![rule("g", Some("*.example.com"), "ff")], None);
        assert!(matches!(
            evaluate(&c, "https://a.example.com/").outcome,
            Outcome::RuleMatched { .. }
        ));
        // cookie-style: the bare domain matches its own `*.` pattern
        assert!(matches!(
            evaluate(&c, "https://example.com/").outcome,
            Outcome::RuleMatched { .. }
        ));
        // other domains do not match
        assert_eq!(
            evaluate(&c, "https://notexample.com/").outcome,
            Outcome::NoMatch
        );
        // exact hosts still match exactly
        let c2 = cfg(vec![rule("loopback", Some("127.0.0.1"), "ff")], None);
        assert!(matches!(
            evaluate(&c2, "http://127.0.0.1:14200/").outcome,
            Outcome::RuleMatched { .. }
        ));
        // a bare host covers itself and every subdomain (cookie-domain style)
        let c3 = cfg(vec![rule("d", Some("example.com"), "ff")], None);
        for url in [
            "https://example.com/",
            "https://a.example.com/",
            "https://deep.a.example.com/",
        ] {
            assert!(
                matches!(evaluate(&c3, url).outcome, Outcome::RuleMatched { .. }),
                "bare host should match {url}"
            );
        }
        // the suffix must respect the dot boundary
        assert_eq!(
            evaluate(&c3, "https://notexample.com/").outcome,
            Outcome::NoMatch
        );
    }

    #[test]
    fn combined_matchers_are_anded() {
        let mut r = rule("docs", Some("*.google.com"), "ff");
        r.url_regex = Some(r"docs\.google\.com".into());
        let c = cfg(vec![r], None);
        assert!(matches!(
            evaluate(&c, "https://docs.google.com/document").outcome,
            Outcome::RuleMatched { .. }
        ));
        assert_eq!(
            evaluate(&c, "https://mail.google.com/").outcome,
            Outcome::NoMatch
        );
    }

    #[test]
    fn scheme_matcher() {
        let mut r = rule("zoom", None, "ff");
        r.scheme = Some("zoommtg".into());
        let c = cfg(vec![r], None);
        assert!(matches!(
            evaluate(&c, "zoommtg://zoom.us/join?conf=1").outcome,
            Outcome::RuleMatched { .. }
        ));
        assert_eq!(
            evaluate(&c, "https://zoom.us/join").outcome,
            Outcome::NoMatch
        );
    }

    #[test]
    fn block_target() {
        let c = cfg(
            vec![rule("nope", Some("*.tracker.io"), TARGET_BLOCK)],
            Some("ff"),
        );
        let d = evaluate(&c, "https://pixel.tracker.io/x");
        assert_eq!(
            d.outcome,
            Outcome::Blocked {
                rule: "nope".into()
            }
        );
    }

    #[test]
    fn default_used_when_no_rule_matches() {
        let c = cfg(vec![rule("g", Some("*.example.com"), "ff")], Some("chrome"));
        let d = evaluate(&c, "https://other.org/");
        assert_eq!(
            d.outcome,
            Outcome::Default {
                target: "chrome".into()
            }
        );
    }

    #[test]
    fn unknown_default_falls_back_to_no_match() {
        let c = cfg(vec![], Some("ghost"));
        assert_eq!(evaluate(&c, "https://a.b/").outcome, Outcome::NoMatch);
    }
}
