//! The URL rule engine: evaluates a URL against the ordered rule list and
//! produces a decision with a full trace for explainability.

use crate::config::{Config, Rule, TARGET_BLOCK};
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
}

enum MatchResult {
    Match,
    No(String),
}

pub fn evaluate(cfg: &Config, raw_url: &str) -> Decision {
    let parsed = Url::parse(raw_url).ok();
    let host = parsed
        .as_ref()
        .and_then(|u| u.host_str())
        .map(|h| h.to_lowercase());
    let scheme = parsed.map(|u| u.scheme().to_ascii_lowercase());

    let mut trace = Vec::new();
    for rule in &cfg.rules {
        if !rule.enabled {
            trace.push(RuleTrace {
                name: rule.name.clone(),
                matched: false,
                reason: "disabled".to_string(),
            });
            continue;
        }
        match matches_rule(rule, raw_url, host.as_deref(), scheme.as_deref()) {
            MatchResult::Match => {
                trace.push(RuleTrace {
                    name: rule.name.clone(),
                    matched: true,
                    reason: "matched".to_string(),
                });
                let outcome = if rule.target == TARGET_BLOCK {
                    Outcome::Blocked {
                        rule: rule.name.clone(),
                    }
                } else {
                    Outcome::RuleMatched {
                        rule: rule.name.clone(),
                        target: rule.target.clone(),
                        incognito: rule.incognito,
                    }
                };
                return Decision {
                    url: raw_url.to_string(),
                    host,
                    scheme,
                    outcome,
                    trace,
                };
            }
            MatchResult::No(reason) => trace.push(RuleTrace {
                name: rule.name.clone(),
                matched: false,
                reason,
            }),
        }
    }

    let outcome = match &cfg.default_browser {
        Some(t) if cfg.browsers.contains_key(t) => Outcome::Default { target: t.clone() },
        _ => Outcome::NoMatch,
    };
    Decision {
        url: raw_url.to_string(),
        host,
        scheme,
        outcome,
        trace,
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
        // Cookie-domain semantics: a host matcher covers the domain itself
        // and every subdomain at any depth — `example.com` matches
        // `a.example.com` and `x.a.example.com` too. The `*.` prefix is an
        // accepted alias (`*.example.com` behaves identically); patterns
        // with other wildcards fall back to glob matching.
        let pat_lower = pat.to_lowercase();
        let base = pat_lower
            .strip_prefix("*.")
            .unwrap_or(&pat_lower)
            .to_string();
        let matched = match host {
            Some(h) => {
                let h = h.to_lowercase();
                if base.chars().any(|c| matches!(c, '*' | '?' | '[')) {
                    glob_matches(pat, &h) || glob_matches(&base, &h)
                } else {
                    h == base || h.ends_with(&format!(".{base}"))
                }
            }
            None => false,
        };
        if !matched {
            return MatchResult::No(format!("host_glob {pat:?} did not match host {host:?}"));
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
            url_regex: None,
            scheme: None,
            target: target.into(),
            incognito: false,
        }
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
