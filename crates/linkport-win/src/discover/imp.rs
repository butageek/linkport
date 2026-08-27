//! Discovers installed browsers from the Windows registry.

use serde::Serialize;
use winreg::enums::*;
use winreg::RegKey;

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredBrowser {
    /// Human-friendly name from the registry (e.g. "Firefox-308046B0AF4A39CB").
    pub name: String,
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
            out.push(DiscoveredBrowser { name, command });
        }
    }
    // HKLM, its WOW6432Node mirror and HKCU can all register the same browser.
    let mut seen = std::collections::HashSet::new();
    out.retain(|d| seen.insert((d.name.clone(), d.command.clone())));
    out
}
