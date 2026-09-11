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
const OPEN_COMMAND_PATH: &str = r"Software\Classes\Linkport.URL\shell\open\command";

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

/// The registered URL-handler command (`Linkport.URL\shell\open\command`),
/// i.e. what Windows actually runs when a link is clicked. `None` when no
/// command is registered.
pub fn registered_handler() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(OPEN_COMMAND_PATH).ok()?.get_value("").ok()
}

/// Whether the registered handler can still work: not registered, or
/// registered with a command whose executable exists on disk. A `false`
/// after moving/upgrading the install is exactly the state where Windows
/// shows "Application not found" on every clicked link.
pub fn handler_ok() -> bool {
    if !is_registered() {
        return true;
    }
    registered_handler()
        .and_then(|cmd| {
            let (exe, _) = linkport_core::launcher::parse_command_template(&cmd);
            Path::new(&exe).exists().then_some(exe)
        })
        .is_some()
}

/// Update the registered handler paths in place after the install moved
/// (e.g. upgraded to a new version folder). Only existing values are
/// overwritten — no key is created or deleted — so the `UserChoice` hash
/// (which covers the ProgId name, not its command) stays valid. This is
/// the same in-place update browsers do on every release.
pub fn repair_handler(open_command: &str) -> Result<(), String> {
    let icon = icon_source(open_command);
    let updates = [
        (OPEN_COMMAND_PATH.to_string(), open_command.to_string()),
        (
            format!(r"Software\Classes\{PROG_ID}\DefaultIcon"),
            icon.clone(),
        ),
        (
            format!(r"{APP_REG_PATH}\shell\open\command"),
            open_command.to_string(),
        ),
        (format!(r"{APP_REG_PATH}\DefaultIcon"), icon),
    ];
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (key_path, value) in &updates {
        hkcu.open_subkey_with_flags(key_path, KEY_SET_VALUE)
            .and_then(|k| k.set_value("", value))
            .map_err(|e| format!("handler repair failed for {key_path}: {e}"))?;
    }
    Ok(())
}

/// `<handler exe>,0` icon reference (Chrome-style) derived from the shell
/// open command, e.g. `"C:\...\linkport.exe" "%1"` →
/// `C:\...\linkport.exe,0`.
fn icon_source(open_command: &str) -> String {
    let (exe, _) = linkport_core::launcher::parse_command_template(open_command);
    format!("{exe},0")
}
