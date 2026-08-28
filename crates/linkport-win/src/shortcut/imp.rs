//! Creates a per-user Start Menu shortcut pointing at the windowless
//! `linkport.exe serve` (the console binary would flash a window).
//!
//! `.lnk` files are produced via the `WScript.Shell` COM object through a
//! one-shot `powershell.exe` run — no shell-link crate needed.

use std::path::Path;

const SHORTCUT_NAME: &str = "Linkport.lnk";

/// Ensure `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Linkport.lnk`
/// exists pointing at `target serve`. Succeeds immediately when the
/// shortcut is already present (no PowerShell spawned on the login path).
pub fn ensure_start_menu_shortcut(target: &Path) -> Result<(), String> {
    let programs = std::env::var("APPDATA").map_err(|e| format!("APPDATA not available: {e}"))?;
    let lnk = Path::new(&programs)
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join(SHORTCUT_NAME);
    if lnk.exists() {
        return Ok(());
    }
    if let Some(dir) = lnk.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create Start Menu dir: {e}"))?;
    }

    let workdir = target
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let script = format!(
        "$s = New-Object -ComObject WScript.Shell; \
         $l = $s.CreateShortcut({}); \
         $l.TargetPath = {}; \
         $l.Arguments = 'serve'; \
         $l.WorkingDirectory = {}; \
         $l.Description = 'Linkport portal daemon'; \
         $l.Save()",
        ps_str(&lnk.display().to_string()),
        ps_str(&target.display().to_string()),
        ps_str(&workdir),
    );

    // The daemon is a GUI-subsystem process: without CREATE_NO_WINDOW the
    // child powershell.exe would flash a console window at login.
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("run powershell: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "powershell exited with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    if !lnk.exists() {
        return Err("shortcut was not created".to_string());
    }
    Ok(())
}

/// Quote a value as a single-quoted PowerShell string literal.
fn ps_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
