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
//! console-less `linkport-open.exe` handler) for every clicked link.

use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

const APP_REG_PATH: &str = r"Software\Clients\StartMenuInternet\Linkport";
const PROG_ID: &str = "Linkport.URL";

pub fn register(exe: &Path, open_command: &str) -> Result<(), String> {
    let _exe = exe.display().to_string();
    let run = || -> std::io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        // 1. Protocol ProgId: HKCU\Software\Classes\Linkport.URL.
        //    The shell open command points at the console-less handler
        //    (linkport-open.exe) so no console window flashes on link clicks.
        let (prog, _) = hkcu.create_subkey(format!(r"Software\Classes\{PROG_ID}"))?;
        prog.set_value("URL Protocol", &"")?;
        let (cmd, _) =
            hkcu.create_subkey(format!(r"Software\Classes\{PROG_ID}\shell\open\command"))?;
        cmd.set_value("", &open_command)?;

        // 2. StartMenuInternet client with URL capabilities.
        let (client, _) = hkcu.create_subkey(APP_REG_PATH)?;
        client.set_value("", &"Linkport")?;
        let (client_cmd, _) = hkcu.create_subkey(format!(r"{APP_REG_PATH}\shell\open\command"))?;
        client_cmd.set_value("", &open_command)?;
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
