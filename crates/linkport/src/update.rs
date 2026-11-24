//! Update checks and the in-place upgrade against the project's GitHub
//! releases (see AGENTS.md "In-app updates & installer").
//!
//! Linkport ships as a zip attached to GitHub Releases, so "is there a
//! newer version" is a `releases/latest` lookup: compare the newest
//! published tag with the running `CARGO_PKG_VERSION`. The check is
//! best-effort — every failure degrades to "no update known" and never
//! blocks or breaks the daemon. It only ever contacts GitHub, and only
//! when enabled (`config.check_updates`, on by default) or when the user
//! asks for it from the tray menu.
//!
//! Applying an update reuses the official `tools/install.ps1` (the README
//! one-liner) instead of a zip/HTTP dependency tree: the daemon spawns
//! PowerShell, which downloads and runs the installer; the installer stops
//! this daemon at its replace step (Windows locks a running exe) and
//! relaunches it. If the installer exits while we are still alive, it gave
//! up before that point and the tray points at `update.log`.

use crate::portal::AppState;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// `owner/repo` of the canonical public repository.
pub const REPO: &str = "butageek/linkport";
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

/// Numeric `major.minor.patch` comparison. Anything unparseable — garbage,
/// missing parts, prerelease suffixes — counts as "not newer", so a bad
/// tag can never nag the user into (or install) a bogus update.
pub fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(version: &str) -> Option<(u64, u64, u64)> {
        let numbers: Vec<&str> = version.trim().split('.').collect();
        if numbers.len() != 3 {
            return None;
        }
        Some((
            numbers[0].parse().ok()?,
            numbers[1].parse().ok()?,
            numbers[2].parse().ok()?,
        ))
    }
    match (parts(&strip_v(latest)), parts(&strip_v(current))) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// Tray menu label for the update action item: install when an update is
/// known, check otherwise.
pub fn tray_action_text(checked: Option<&UpdateCheck>) -> String {
    match checked {
        Some(c) if c.available => {
            format!("Install update v{}…", c.latest.as_deref().unwrap_or("?"))
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

/// Tray labels (action, status) while the installer is running.
pub fn tray_installing_text(checked: Option<&UpdateCheck>) -> (String, String) {
    let latest = checked.and_then(|c| c.latest.as_deref()).unwrap_or("?");
    (
        format!("Installing v{latest}…"),
        format!("Downloading and installing v{latest} — Linkport restarts when done"),
    )
}

/// Tray labels (action, status) after the installer exited without
/// reaching the replace step (download failed, …): the action goes back to
/// "Install update v…", the status line points at the log.
pub fn tray_failed_text(checked: Option<&UpdateCheck>) -> (String, String) {
    (
        tray_action_text(checked),
        "Update did not complete — see update.log in the config folder".to_string(),
    )
}

/// PowerShell that downloads the official installer (the README one-liner's
/// script) and runs it. Download-then-run-as-file because `irm … | iex`
/// cannot pass a script with a `param()` block through Windows PowerShell
/// 5.1; `Stop` makes a failed download abort instead of running nothing.
pub fn installer_script() -> String {
    format!(
        "$ErrorActionPreference = 'Stop'; \
         irm https://raw.githubusercontent.com/{REPO}/main/tools/install.ps1 \
         -OutFile \"$env:TEMP\\linkport-install.ps1\"; \
         & \"$env:TEMP\\linkport-install.ps1\""
    )
}

/// Spawn the installer hidden, with its output appended to `log` (the
/// daemon is gone by the time it finishes, so piping would break — a file
/// the user can open afterwards is the useful transcript).
#[cfg(windows)]
pub fn spawn_installer(log: &std::path::Path) -> std::io::Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::fs::File::create(log)?;
    let err = out.try_clone()?;
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &installer_script(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
}

/// Kick off the in-place upgrade: spawn the official installer, which
/// stops this daemon at its replace step and relaunches it — this process
/// is expected to disappear before the installer exits. If the installer
/// exits while we are still alive, it gave up before that point (download
/// failed, …): the tray then says so and points at the log. Only the spawn
/// happens on the caller's thread; the wait runs on its own.
pub fn install_in_background(state: &AppState) -> Result<(), String> {
    if !state.snapshot_update().is_some_and(|c| c.available) {
        return Err("no update is available".to_string());
    }
    #[cfg(windows)]
    {
        INSTALLING.store(true, Ordering::SeqCst);
        crate::tray::notify_update_installing();
        let log = crate::paths::update_log_path();
        std::thread::spawn(move || {
            let outcome = spawn_installer(&log);
            match outcome {
                Ok(mut child) => {
                    let status = child.wait();
                    eprintln!(
                        "linkport: update installer exited early ({status:?}) — see {}",
                        log.display()
                    );
                }
                Err(e) => eprintln!("linkport: cannot start the update installer: {e}"),
            }
            // Still alive: the installer never reached its replace step.
            INSTALLING.store(false, Ordering::SeqCst);
            crate::tray::notify_update_install_failed();
        });
        Ok(())
    }
    #[cfg(not(windows))]
    Err("in-place updates are installed by tools/install.ps1 on Windows".to_string())
}

static INSTALLING: AtomicBool = AtomicBool::new(false);

/// True while the installer is running (cleared when it exits without
/// having stopped us) — lets the portal tell "still installing" from "the
/// install gave up" while it waits for the daemon to come back.
pub fn is_installing() -> bool {
    INSTALLING.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_are_detected() {
        assert!(is_newer("v0.5.0", "0.4.9"));
        assert!(is_newer("0.4.2", "v0.4.1"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.10.0", "v0.9.0")); // numeric, not lexicographic
        assert!(is_newer("0.2.10", "0.2.9"));
    }

    #[test]
    fn same_older_and_unparseable_are_not_newer() {
        assert!(!is_newer("v0.4.0", "0.4.0"));
        assert!(!is_newer("v0.3.9", "0.4.0"));
        assert!(!is_newer("garbage", "0.4.0"));
        assert!(!is_newer("v0.5.0-beta", "0.4.0")); // prerelease: never nag
        assert!(!is_newer("", "0.4.0"));
        assert!(!is_newer("0.5", "0.4.9")); // missing part: never install a guess
        assert!(!is_newer("0.5.0", "0.4")); // nor upgrade onto an unknown current
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
        assert_eq!(tray_action_text(Some(&available)), "Install update v0.3.0…");
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

    #[test]
    fn tray_labels_while_installing_and_after_a_failed_install() {
        let available = UpdateCheck::succeeded("0.2.1", "0.3.0".into());
        let (action, status) = tray_installing_text(Some(&available));
        assert_eq!(action, "Installing v0.3.0…");
        assert!(status.contains("restarts when done"), "{status}");

        // Back to the install action, with the log pointer as the status.
        let (action, status) = tray_failed_text(Some(&available));
        assert_eq!(action, "Install update v0.3.0…");
        assert!(status.contains("update.log"), "{status}");

        // Without a known version the labels still make sense.
        let (action, _) = tray_installing_text(None);
        assert_eq!(action, "Installing v?…");
        assert_eq!(tray_failed_text(None).0, "Check for updates…");
    }

    #[test]
    fn installer_script_downloads_then_runs_the_official_installer() {
        let script = installer_script();
        // Runs as a file: `irm | iex` cannot carry a param() block on PS 5.1.
        assert!(script.contains(&format!(
            "irm https://raw.githubusercontent.com/{REPO}/main/tools/install.ps1 -OutFile"
        )));
        assert!(script.contains("& \"$env:TEMP\\linkport-install.ps1\""));
        assert!(script.starts_with("$ErrorActionPreference = 'Stop';"));
    }
}
