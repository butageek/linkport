//! Discovers installed browsers from the Windows registry.

use serde::Serialize;
use winreg::enums::*;
use winreg::RegKey;

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredBrowser {
    /// Registry key name — stable but often ugly (e.g. `Firefox-308046B0AF4A39CB`).
    pub name: String,
    /// Friendly name from the key's default value (e.g. `Mozilla Firefox`,
    /// `Brave`); falls back to the key name when absent.
    pub display_name: String,
    /// Raw `shell\open\command` template (e.g. `"C:\...\firefox.exe" -osint -url "%1"`).
    pub command: String,
}

pub fn discover_browsers() -> Vec<DiscoveredBrowser> {
    let mut out = Vec::new();
    let hives = [
        (HKEY_CURRENT_USER, r"Software\Clients\StartMenuInternet"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Clients\StartMenuInternet"),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Clients\StartMenuInternet",
        ),
    ];
    for (hive, path) in hives {
        let Ok(key) = RegKey::predef(hive).open_subkey(path) else {
            continue;
        };
        for name in key.enum_keys().filter_map(Result::ok) {
            if name.eq_ignore_ascii_case("Linkport") {
                continue; // never offer ourselves as a target
            }
            let Ok(cmd_key) = key.open_subkey(format!(r"{name}\shell\open\command")) else {
                continue;
            };
            let Ok(command) = cmd_key.get_value::<String, _>("") else {
                continue;
            };
            let display_name = key
                .open_subkey(&name)
                .and_then(|client| client.get_value::<String, _>(""))
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| name.clone());
            out.push(DiscoveredBrowser {
                name,
                display_name,
                command,
            });
        }
    }
    // HKLM, its WOW6432Node mirror and HKCU can all register the same browser.
    let mut seen = std::collections::HashSet::new();
    out.retain(|d| seen.insert((d.name.clone(), d.command.clone())));
    out
}

/// The executable of the current system default browser, resolved from the
/// per-user `http` protocol association (`UserChoice` ProgId → shell open
/// command). `None` when it cannot be resolved. When the system default is
/// Linkport itself, the returned exe is our own handler — callers fall back
/// sensibly since no configured browser will match it.
pub fn system_default_exe() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let prog_id: String = hkcu
        .open_subkey(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\UrlAssociations\http\UserChoice",
        )
        .ok()?
        .get_value("ProgId")
        .ok()?;
    let command: String = hkcu
        .open_subkey(format!(r"Software\Classes\{prog_id}\shell\open\command"))
        .or_else(|_| {
            RegKey::predef(HKEY_CLASSES_ROOT).open_subkey(format!(r"{prog_id}\shell\open\command"))
        })
        .ok()?
        .get_value("")
        .ok()?;
    let exe = command_exe(&command);
    (!exe.is_empty()).then_some(exe)
}

/// Executable (first token) of a registry command template: quoted or
/// space-separated.
fn command_exe(command: &str) -> String {
    let cmd = command.trim();
    if let Some(stripped) = cmd.strip_prefix('"') {
        match stripped.split_once('"') {
            Some((exe, _)) => exe.to_string(),
            None => cmd.trim_matches('"').to_string(),
        }
    } else {
        match cmd.split_once(' ') {
            Some((exe, _)) => exe.to_string(),
            None => cmd.to_string(),
        }
    }
}
