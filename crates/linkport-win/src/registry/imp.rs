//! Registers Linkport as a default-browser *candidate* for the current user.
//!
//! Windows 10/11 deliberately prevents programs from silently setting
//! themselves as the default browser. The supported flow is:
//!
//! 1. This module writes the `StartMenuInternet` client registration,
//!    a URL ProgId and a `RegisteredApplications` entry (all under HKCU).
//! 2. The user picks "Linkport" in
//!    Settings > Apps > Default apps > Linkport > set default for HTTP/HTTPS.
//!
//! After that, Windows invokes the registered shell open command (the
//! console-less `linkport.exe` handler) for every clicked link.

use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

const APP_REG_PATH: &str = r"Software\Clients\StartMenuInternet\Linkport";
const PROG_ID: &str = "Linkport.URL";

pub fn register(_exe: &Path, open_command: &str) -> Result<(), String> {
    let icon = icon_source(open_command);
    let run = || -> std::io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        // 1. Protocol ProgId: HKCU\Software\Classes\Linkport.URL.
        //    The shell open command points at the console-less handler
        //    (linkport.exe) so no console window flashes on link clicks.
        let (prog, _) = hkcu.create_subkey(format!(r"Software\Classes\{PROG_ID}"))?;
        prog.set_value("URL Protocol", &"")?;
        let (cmd, _) =
            hkcu.create_subkey(format!(r"Software\Classes\{PROG_ID}\shell\open\command"))?;
        cmd.set_value("", &open_command)?;
        let (prog_icon, _) =
            hkcu.create_subkey(format!(r"Software\Classes\{PROG_ID}\DefaultIcon"))?;
        prog_icon.set_value("", &icon)?;

        // 2. StartMenuInternet client with URL capabilities.
        let (client, _) = hkcu.create_subkey(APP_REG_PATH)?;
        client.set_value("", &"Linkport")?;
        let (client_cmd, _) = hkcu.create_subkey(format!(r"{APP_REG_PATH}\shell\open\command"))?;
        client_cmd.set_value("", &open_command)?;
        // Default apps / Start menu take the entry's icon from here (and fall
        // back to the embedded exe icon — both point at linkport.exe).
        let (client_icon, _) = hkcu.create_subkey(format!(r"{APP_REG_PATH}\DefaultIcon"))?;
        client_icon.set_value("", &icon)?;
        let (caps, _) = hkcu.create_subkey(format!(r"{APP_REG_PATH}\Capabilities"))?;
        caps.set_value("ApplicationName", &"Linkport")?;
        caps.set_value(
            "ApplicationDescription",
            &"Rule-based browser router with a web portal",
        )?;
        let (url_assoc, _) =
            hkcu.create_subkey(format!(r"{APP_REG_PATH}\Capabilities\URLAssociations"))?;
        url_assoc.set_value("http", &PROG_ID)?;
        url_assoc.set_value("https", &PROG_ID)?;

        // 3. Announce ourselves in RegisteredApplications.
        let (regapps, _) = hkcu.create_subkey(r"Software\RegisteredApplications")?;
        regapps.set_value("Linkport", &format!(r"{APP_REG_PATH}\Capabilities"))?;
        Ok(())
    };
    run().map_err(|e| format!("registry write failed: {e}"))
}

pub fn unregister() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    // RegDeleteTree removes each subtree in one call.
    let _ = hkcu.delete_subkey_all(APP_REG_PATH);
    let _ = hkcu.delete_subkey_all(format!(r"Software\Classes\{PROG_ID}"));
    if let Ok(regapps) =
        hkcu.open_subkey_with_flags(r"Software\RegisteredApplications", KEY_SET_VALUE)
    {
        let _ = regapps.delete_value("Linkport");
    }
    Ok(())
}

pub fn is_registered() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(format!(r"{APP_REG_PATH}\Capabilities\URLAssociations"))
        .is_ok()
}

/// `<handler exe>,0` icon reference (Chrome-style) derived from the shell
/// open command, e.g. `"C:\...\linkport.exe" "%1"` →
/// `C:\...\linkport.exe,0`.
fn icon_source(open_command: &str) -> String {
    let cmd = open_command.trim();
    let exe = cmd
        .strip_prefix('"')
        .and_then(|s| s.split_once('"').map(|(e, _)| e))
        .or_else(|| cmd.split_once(' ').map(|(e, _)| e))
        .unwrap_or(cmd);
    format!("{exe},0")
}
