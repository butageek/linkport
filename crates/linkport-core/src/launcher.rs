//! Browser process launching and Windows command-template parsing.

use crate::config::Browser;
use std::process::Command;

/// Spawn the browser with the resolved argument template.
///
/// On Windows the child is created with `CREATE_NO_WINDOW` so no console
/// window flashes when Linkport is invoked from GUI apps.
pub fn launch(
    browser: &Browser,
    url: &str,
    incognito: bool,
) -> std::io::Result<std::process::Child> {
    let mut cmd = Command::new(&browser.exe);
    cmd.args(browser.resolved_args(url, incognito));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn()
}

/// Parse a Windows registry `shell\open\command` template such as
/// `"C:\Program Files\Mozilla Firefox\firefox.exe" -osint -url "%1"` into an
/// executable and an argument list where `%1`-style tokens become `{url}`.
///
/// If no token references the URL, `{url}` is appended.
pub fn parse_command_template(command: &str) -> (String, Vec<String>) {
    let command = command.trim();

    let (exe, rest) = if let Some(stripped) = command.strip_prefix('"') {
        match stripped.split_once('"') {
            Some((exe, rest)) => (exe.to_string(), rest),
            None => (command.trim_matches('"').to_string(), ""),
        }
    } else {
        match command.split_once(' ') {
            Some((exe, rest)) => (exe.to_string(), rest),
            None => (command.to_string(), ""),
        }
    };

    let mut args: Vec<String> = rest
        .split_whitespace()
        .map(|t| {
            if t.contains('%') {
                "{url}".to_string()
            } else {
                t.to_string()
            }
        })
        .collect();
    args.dedup();
    if !args.iter().any(|a| a == "{url}") {
        args.push("{url}".to_string());
    }
    (exe, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_firefox_registry_command() {
        let (exe, args) = parse_command_template(
            r#""C:\Program Files\Mozilla Firefox\firefox.exe" -osint -url "%1""#,
        );
        assert_eq!(exe, r"C:\Program Files\Mozilla Firefox\firefox.exe");
        assert_eq!(args, vec!["-osint", "-url", "{url}"]);
    }

    #[test]
    fn parses_chromium_registry_command() {
        let (exe, args) = parse_command_template(
            r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --single-argument %1"#,
        );
        assert_eq!(
            exe,
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        );
        assert_eq!(args, vec!["--single-argument", "{url}"]);
    }

    #[test]
    fn appends_url_when_template_has_no_placeholder() {
        let (exe, args) = parse_command_template(r#""C:\b.exe""#);
        assert_eq!(exe, r"C:\b.exe");
        assert_eq!(args, vec!["{url}"]);
    }

    #[test]
    fn parses_unquoted_exe() {
        let (exe, args) = parse_command_template("/usr/bin/firefox %u");
        assert_eq!(exe, "/usr/bin/firefox");
        assert_eq!(args, vec!["{url}"]);
    }
}
