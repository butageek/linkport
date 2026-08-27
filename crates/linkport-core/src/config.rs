//! Configuration model, loading/saving and validation.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

pub const CONFIG_VERSION: u32 = 1;

/// Special rule target that swallows the URL without opening anything.
pub const TARGET_BLOCK: &str = "block";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub portal: PortalConfig,
    pub default_browser: Option<String>,
    /// Browser definitions keyed by id. Rule targets and `default_browser`
    /// reference these ids.
    pub browsers: BTreeMap<String, Browser>,
    /// Ordered rules; the first matching rule wins.
    pub rules: Vec<Rule>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            portal: PortalConfig::default(),
            default_browser: None,
            browsers: BTreeMap::new(),
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PortalConfig {
    /// Port the portal daemon binds on 127.0.0.1.
    pub port: u16,
}

impl Default for PortalConfig {
    fn default() -> Self {
        Self { port: 14200 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Browser {
    pub display_name: String,
    /// Executable to run.
    pub exe: String,
    /// Argument template; `{url}` is substituted with the captured URL.
    /// If no argument contains `{url}`, the URL is appended.
    #[serde(default)]
    pub args: Vec<String>,
    /// Alternative argument template used when a rule requests incognito mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incognito_args: Option<Vec<String>>,
}

impl Browser {
    pub fn resolved_args(&self, url: &str, incognito: bool) -> Vec<String> {
        let template = if incognito {
            self.incognito_args.as_deref().unwrap_or(&self.args)
        } else {
            &self.args
        };
        let mut out: Vec<String> = template.iter().map(|a| a.replace("{url}", url)).collect();
        if !template.iter().any(|a| a.contains("{url}")) {
            out.push(url.to_string());
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Glob matched against the URL host, e.g. `*.example.com`.
    /// Cookie-style: `*.example.com` also matches `example.com` itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_glob: Option<String>,
    /// Regex matched against the full URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_regex: Option<String>,
    /// Exact scheme match (e.g. `https`, or deep-link schemes like `zoommtg`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    /// Browser id to open with, or `block` to swallow the URL.
    pub target: String,
    /// Open in the browser's incognito/private mode when supported.
    #[serde(default)]
    pub incognito: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
}

pub fn load(path: &Path) -> Result<Config, LoadError> {
    let raw = fs::read_to_string(path)?;
    Ok(toml::from_str(&raw)?)
}

/// Atomically write the config (temp file + rename).
pub fn save(path: &Path, cfg: &Config) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(cfg).map_err(io::Error::other)?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, raw)?;
    fs::rename(&tmp, path).or_else(|e| {
        // Windows: rename over an existing file fails; remove and retry.
        let _ = fs::remove_file(path);
        fs::rename(&tmp, path).map_err(|_| e)
    })
}

/// Returns human-readable warnings for problems that do not prevent saving.
pub fn validate(cfg: &Config) -> Vec<String> {
    let mut warnings = Vec::new();

    if cfg.version != CONFIG_VERSION {
        warnings.push(format!(
            "config version is {}, expected {}",
            cfg.version, CONFIG_VERSION
        ));
    }
    for (i, rule) in cfg.rules.iter().enumerate() {
        if rule.target != TARGET_BLOCK && !cfg.browsers.contains_key(&rule.target) {
            warnings.push(format!(
                "rule[{}] {:?} targets unknown browser {:?}",
                i, rule.name, rule.target
            ));
        }
        if let Some(p) = &rule.host_glob {
            if let Err(e) = globset::Glob::new(p) {
                warnings.push(format!(
                    "rule {:?} has invalid host_glob {:?}: {e}",
                    rule.name, p
                ));
            }
        }
        if let Some(p) = &rule.url_regex {
            if regex::Regex::new(p).is_err() {
                warnings.push(format!(
                    "rule {:?} has invalid url_regex {:?}",
                    rule.name, p
                ));
            }
        }
        if rule.host_glob.is_none() && rule.url_regex.is_none() && rule.scheme.is_none() {
            warnings.push(format!(
                "rule {:?} has no matcher and will match every URL",
                rule.name
            ));
        }
    }
    if let Some(d) = &cfg.default_browser {
        if !cfg.browsers.contains_key(d) {
            warnings.push(format!(
                "default_browser {d:?} is not defined in [browsers]"
            ));
        }
    }
    if cfg.browsers.is_empty() {
        warnings.push("no browsers configured; add one on the Browsers page".to_string());
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("linkport-test-{name}-{}.toml", std::process::id()))
    }

    #[test]
    fn config_roundtrip() {
        let mut cfg = Config::default();
        cfg.browsers.insert(
            "firefox".into(),
            Browser {
                display_name: "Firefox".into(),
                exe: "/usr/bin/firefox".into(),
                args: vec!["{url}".into()],
                incognito_args: Some(vec!["--private-window".into(), "{url}".into()]),
            },
        );
        cfg.rules.push(Rule {
            name: "work".into(),
            enabled: true,
            host_glob: Some("*.corp.example".into()),
            url_regex: None,
            scheme: None,
            target: "firefox".into(),
            incognito: false,
        });
        cfg.default_browser = Some("firefox".into());

        let path = tmp_path("roundtrip");
        save(&path, &cfg).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(cfg, loaded);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let toml = r#"
[browsers.firefox]
display_name = "Firefox"
exe = "firefox"

[[rules]]
name = "all"
target = "firefox"
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(cfg.rules[0].enabled, "enabled should default to true");
        assert!(cfg.rules[0].host_glob.is_none());
        assert!(!cfg.rules[0].incognito, "incognito should default to false");
    }

    #[test]
    fn validate_flags_unknown_target() {
        let cfg: Config = toml::from_str(
            r#"
[[rules]]
name = "r"
target = "ghost"
"#,
        )
        .unwrap();
        let warnings = validate(&cfg);
        assert!(warnings.iter().any(|w| w.contains("unknown browser")));
        assert!(warnings
            .iter()
            .any(|w| w.contains("no browsers configured")));
    }

    #[test]
    fn resolved_args_substitutes_and_appends() {
        let b = Browser {
            display_name: "X".into(),
            exe: "x".into(),
            args: vec!["--new-window".into(), "{url}".into()],
            incognito_args: Some(vec!["--private".into()]),
        };
        assert_eq!(
            b.resolved_args("https://a.b", false),
            vec!["--new-window", "https://a.b"]
        );
        // incognito template without {url} -> url appended
        assert_eq!(
            b.resolved_args("https://a.b", true),
            vec!["--private", "https://a.b"]
        );

        let plain = Browser {
            args: vec![],
            exe: "x".into(),
            display_name: "X".into(),
            incognito_args: None,
        };
        assert_eq!(plain.resolved_args("u://1", false), vec!["u://1"]);
    }
}
