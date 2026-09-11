//! Update checks against the project's GitHub releases.
//!
//! Linkport ships as a zip attached to GitHub Releases, so "is there a
//! newer version" is a `releases/latest` lookup: compare the newest
//! published tag with the running `CARGO_PKG_VERSION`. The check is
//! best-effort — every failure degrades to "no update known" and never
//! blocks or breaks the daemon. It only ever contacts GitHub, and only
//! when enabled (`config.check_updates`, on by default) or when the user
//! asks for it from the tray menu.

use serde::Serialize;
use std::time::Duration;

/// `owner/repo` of the canonical public repository.
const REPO: &str = "butageek/linkport";
const TIMEOUT: Duration = Duration::from_secs(10);

/// The result of one update check, surfaced through `/api/status` and the
/// tray menu. `latest`/`error` describe how the check went; `available`
/// is the derived answer.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheck {
    pub current: String,
    /// Newest published version (`v` prefix stripped); `None` when the
    /// check failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<String>,
    /// True when `latest` is newer than `current`.
    pub available: bool,
    /// The releases page (opens the newest release; used by the tray's
    /// download action).
    pub url: String,
    /// Failure reason when the check did not succeed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl UpdateCheck {
    pub(crate) fn succeeded(current: &str, latest: String) -> Self {
        Self {
            current: current.to_string(),
            available: is_newer(&latest, current),
            url: releases_url(),
            latest: Some(latest),
            error: None,
        }
    }

    pub(crate) fn failed(current: &str, error: String) -> Self {
        Self {
            current: current.to_string(),
            available: false,
            url: releases_url(),
            latest: None,
            error: Some(error),
        }
    }
}

/// Perform one check (blocking, network-bound — run off the hot paths).
pub fn check() -> UpdateCheck {
    let current = env!("CARGO_PKG_VERSION");
    match fetch_latest_tag() {
        Ok(tag) => UpdateCheck::succeeded(current, strip_v(&tag)),
        Err(e) => UpdateCheck::failed(current, e),
    }
}

fn releases_url() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

/// Ask GitHub for the newest published (non-draft, non-prerelease)
/// release tag. Reads only the small JSON document of `releases/latest`.
fn fetch_latest_tag() -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
    }

    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
    let resp = agent
        .get(&format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        // GitHub requires a User-Agent on API requests.
        .set(
            "User-Agent",
            &format!(
                "linkport/{} (+https://github.com/{REPO})",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("request failed: {e}"))?;
    let body = resp
        .into_string()
        .map_err(|e| format!("response read failed: {e}"))?;
    let release: Release =
        serde_json::from_str(&body).map_err(|e| format!("unexpected response: {e}"))?;
    let tag = release.tag_name.trim().to_string();
    if tag.is_empty() {
        return Err("empty tag_name".to_string());
    }
    Ok(tag)
}

fn strip_v(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).trim().to_string()
}

/// Tray menu label for the update action item: download when an update is
/// known, check otherwise.
pub fn tray_action_text(checked: Option<&UpdateCheck>) -> String {
    match checked {
        Some(c) if c.available => {
            format!("Download Linkport v{}…", c.latest.as_deref().unwrap_or("?"))
        }
        _ => "Check for updates…".to_string(),
    }
}

/// Tray menu label for the (disabled) status line under the action item.
pub fn tray_status_text(checked: Option<&UpdateCheck>) -> String {
    match checked {
        None => "No update check yet".to_string(),
        Some(c) if c.available => format!(
            "v{} available (running v{})",
            c.latest.as_deref().unwrap_or("?"),
            c.current
        ),
        Some(c) if c.error.is_some() => "Update check failed".to_string(),
        Some(c) => format!("Up to date (v{})", c.current),
    }
}

/// Numeric `major.minor.patch` comparison; missing parts count as 0 and
/// non-numeric parts degrade to 0 (this project tags plain `x.y.z`).
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    let (l, c) = (parts(latest), parts(current));
    for i in 0..l.len().max(c.len()) {
        let a = l.get(i).copied().unwrap_or(0);
        let b = c.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.3.0", "0.2.1"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.2.10", "0.2.9"));
        assert!(is_newer("0.3", "0.2.9"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
        assert!(!is_newer("0.2", "0.2.0"));
    }

    #[test]
    fn strips_v_prefix() {
        assert_eq!(strip_v("v0.3.0"), "0.3.0");
        assert_eq!(strip_v("0.3.0"), "0.3.0");
        assert_eq!(strip_v(" V1.2.3 "), "1.2.3");
    }

    #[test]
    fn check_result_shapes() {
        let ok = UpdateCheck::succeeded("0.2.1", "0.3.0".into());
        assert!(ok.available);
        assert_eq!(ok.latest.as_deref(), Some("0.3.0"));
        assert!(ok.error.is_none());

        let same = UpdateCheck::succeeded("0.2.1", "0.2.1".into());
        assert!(!same.available);

        let err = UpdateCheck::failed("0.2.1", "offline".into());
        assert!(!err.available);
        assert!(err.latest.is_none());
        assert_eq!(err.error.as_deref(), Some("offline"));
    }

    #[test]
    fn tray_labels_follow_the_check_result() {
        assert_eq!(tray_action_text(None), "Check for updates…");
        assert_eq!(tray_status_text(None), "No update check yet");

        let available = UpdateCheck::succeeded("0.2.1", "0.3.0".into());
        assert_eq!(
            tray_action_text(Some(&available)),
            "Download Linkport v0.3.0…"
        );
        assert_eq!(
            tray_status_text(Some(&available)),
            "v0.3.0 available (running v0.2.1)"
        );

        let fresh = UpdateCheck::succeeded("0.2.1", "0.2.1".into());
        assert_eq!(tray_action_text(Some(&fresh)), "Check for updates…");
        assert_eq!(tray_status_text(Some(&fresh)), "Up to date (v0.2.1)");

        let failed = UpdateCheck::failed("0.2.1", "no dns".into());
        assert_eq!(tray_action_text(Some(&failed)), "Check for updates…");
        assert_eq!(tray_status_text(Some(&failed)), "Update check failed");
    }
}
