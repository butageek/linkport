//! System tray presence for the portal daemon (Windows only).
//!
//! Shows a tray icon while the daemon (`linkport.exe serve`) runs:
//! - left click      → open the web portal
//! - right click     → menu: Open portal / Start-at-login toggle /
//!   Pause-routing toggle / Check-for-updates / Version / Portal URL /
//!   Quit
//!
//! Toggles mutate Windows/user state (Run registry key, pause flag file) and
//! are performed on this thread (menu items are not `Send`), driven by
//! `WM_APP` messages posted from the menu-event thread. Quit performs a
//! graceful axum shutdown via the watch channel, then posts WM_QUIT.

#![cfg(windows)]

use std::sync::atomic::{AtomicU32, Ordering};

use tokio::sync::watch::Sender as WatchSender;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::portal::AppState;

static TRAY_THREAD_ID: AtomicU32 = AtomicU32::new(0);

const ICON_RGBA: &[u8] = include_bytes!("../../../resources/icon32.rgba");
const ICON_GREY_RGBA: &[u8] = include_bytes!("../../../resources/icon32-grey.rgba");
const ICON_SIZE: u32 = 32;

/// WM_APP-based command posted to the tray thread (wParam payload).
const WM_TRAY_CMD: u32 = 0x8000 + 1;
const CMD_TOGGLE_AUTOSTART: usize = 1;
const CMD_TOGGLE_PAUSE: usize = 2;
const CMD_UPDATE_CHECKED: usize = 3;

/// Tell the tray thread an update check finished and the menu should
/// re-read the shared result (no-op when no tray thread is running).
pub fn notify_update_checked() {
    post_cmd(CMD_UPDATE_CHECKED);
}

pub struct TrayHandle {
    thread: std::thread::JoinHandle<()>,
}

/// Spawn the tray thread. Never panics; on failure the daemon simply runs
/// without a tray icon (the error is printed to stderr).
pub fn spawn(state: AppState, quit: WatchSender<bool>) -> TrayHandle {
    let thread = std::thread::Builder::new()
        .name("linkport-tray".into())
        .spawn(move || {
            if let Err(e) = run(state, quit) {
                eprintln!("linkport: tray unavailable: {e:#}");
            }
        })
        .expect("failed to spawn tray thread");
    TrayHandle { thread }
}

impl TrayHandle {
    /// End the tray message loop and wait for the thread to exit (removes the
    /// icon). Safe to call even if Quit already posted WM_QUIT.
    pub fn stop(self) {
        post_quit();
        let _ = self.thread.join();
    }
}

fn run(state: AppState, quit: WatchSender<bool>) -> anyhow::Result<()> {
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    unsafe {
        TRAY_THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);
    }

    // Build the menu. enabled=false items are informational.
    let open = MenuItem::with_id("open", "Open Linkport portal", true, None);
    let copy_url = MenuItem::with_id("copy", "Copy portal URL", true, None);
    let autostart = CheckMenuItem::with_id(
        "autostart",
        "Start Linkport when I sign in",
        true,
        linkport_win::autostart::is_enabled(),
        None,
    );
    let pause = CheckMenuItem::with_id(
        "pause",
        "Pause routing (all links to default browser)",
        true,
        crate::paths::is_paused(),
        None,
    );
    // Update items: an action plus a disabled status line, both re-texted
    // on this thread whenever a check completes (WM_APP), and seeded from
    // the shared state in case the startup check already finished.
    let checked = state.snapshot_update();
    let upd_action = MenuItem::with_id(
        "upd_action",
        crate::update::tray_action_text(checked.as_ref()),
        true,
        None,
    );
    let upd_status = MenuItem::with_id(
        "upd_status",
        crate::update::tray_status_text(checked.as_ref()),
        false,
        None,
    );
    let version = MenuItem::with_id(
        "version",
        format!("Version {}", env!("CARGO_PKG_VERSION")),
        false,
        None,
    );
    let addr = MenuItem::with_id(
        "addr",
        format!("Portal: http://127.0.0.1:{}", state.port),
        false,
        None,
    );
    let sep1 = PredefinedMenuItem::separator();
    let sep2 = PredefinedMenuItem::separator();
    let sep3 = PredefinedMenuItem::separator();
    let quit_item = MenuItem::with_id("quit", "Quit Linkport", true, None);

    let menu = Menu::new();
    menu.append(&open)?;
    menu.append(&copy_url)?;
    menu.append(&sep1)?;
    menu.append(&autostart)?;
    menu.append(&pause)?;
    menu.append(&upd_action)?;
    menu.append(&upd_status)?;
    menu.append(&sep2)?;
    menu.append(&version)?;
    menu.append(&addr)?;
    menu.append(&sep3)?;
    menu.append(&quit_item)?;

    let paused = crate::paths::is_paused();
    let icon = tray_icon(paused)?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(tooltip_text(paused))
        .with_icon(icon)
        .build()?;

    // Menu events (right-click menu). Item mutation must happen on the tray
    // thread, so toggles are forwarded as WM_APP commands.
    let menu_state = state.clone();
    let menu_quit = quit.clone();
    std::thread::spawn(move || {
        while let Ok(ev) = MenuEvent::receiver().recv() {
            match ev.id().as_ref() {
                "open" => crate::portal::open_portal_url(&menu_state),
                "copy" => {
                    let url = crate::portal::portal_url_string(&menu_state);
                    if set_clipboard_text(&url) {
                        eprintln!("linkport: portal URL copied to clipboard");
                    } else {
                        eprintln!("linkport: failed to copy portal URL");
                    }
                }
                "autostart" => post_cmd(CMD_TOGGLE_AUTOSTART),
                "pause" => post_cmd(CMD_TOGGLE_PAUSE),
                "upd_action" => {
                    // Network-bound: keep the menu-event loop responsive.
                    let st = menu_state.clone();
                    std::thread::spawn(move || check_for_updates(st));
                }
                "quit" => {
                    let _ = menu_quit.send(true);
                    post_quit();
                }
                _ => {}
            }
        }
    });

    // Tray events (left click opens the portal).
    let click_state = state.clone();
    std::thread::spawn(move || {
        while let Ok(ev) = TrayIconEvent::receiver().recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                crate::portal::open_portal_url(&click_state);
            }
        }
    });

    // Win32 message pump — required for the tray icon and menu to function.
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG,
    };
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if r <= 0 {
            // 0 = WM_QUIT, -1 = error: end the loop either way.
            break;
        }
        if msg.hwnd.is_null() && msg.message == WM_TRAY_CMD {
            match msg.wParam {
                CMD_TOGGLE_AUTOSTART => toggle_autostart(&autostart),
                CMD_TOGGLE_PAUSE => toggle_pause(&pause, &tray),
                CMD_UPDATE_CHECKED => refresh_update_items(&upd_action, &upd_status, &state),
                _ => {}
            }
            continue;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}

/// Manual update action from the menu. When an update is already known,
/// the click means "take me there" — the release page opens through the
/// normal routing (rules apply, the open is logged like any link).
/// Otherwise run a check now, publish it to the shared state and let the
/// tray thread refresh the menu text.
fn check_for_updates(state: AppState) {
    let known_url = state
        .update
        .lock()
        .ok()
        .and_then(|g| g.as_ref().filter(|c| c.available).map(|c| c.url.clone()));
    if let Some(url) = known_url {
        let _ = crate::open::open_url(&url);
        return;
    }
    let result = crate::update::check();
    if result.available {
        println!(
            "linkport: update available: v{} (running v{})",
            result.latest.as_deref().unwrap_or("?"),
            result.current
        );
    }
    state.store_update(result);
    notify_update_checked();
}

/// Re-read the shared update state and re-text the two menu items. Runs on
/// the tray thread (items are not `Send`), triggered by `CMD_UPDATE_CHECKED`.
fn refresh_update_items(action: &MenuItem, status: &MenuItem, state: &AppState) {
    let checked = state.snapshot_update();
    let _ = action.set_text(crate::update::tray_action_text(checked.as_ref()));
    let _ = status.set_text(crate::update::tray_status_text(checked.as_ref()));
}

fn toggle_autostart(item: &CheckMenuItem) {
    let want = !linkport_win::autostart::is_enabled();
    let result = if want {
        linkport_win::autostart::enable(&crate::autostart_command())
    } else {
        linkport_win::autostart::disable()
    };
    match result {
        Ok(()) => item.set_checked(want),
        Err(e) => eprintln!("linkport: auto-start toggle failed: {e}"),
    }
}

fn toggle_pause(item: &CheckMenuItem, tray: &tray_icon::TrayIcon) {
    let want = !crate::paths::is_paused();
    match crate::paths::set_paused(want) {
        Ok(()) => {
            item.set_checked(want);
            if let Ok(icon) = tray_icon(want) {
                if let Err(e) = tray.set_icon(Some(icon)) {
                    eprintln!("linkport: pause icon swap failed: {e}");
                }
            }
            if let Err(e) = tray.set_tooltip(Some(tooltip_text(want))) {
                eprintln!("linkport: tooltip update failed: {e}");
            }
        }
        Err(e) => eprintln!("linkport: pause toggle failed: {e}"),
    }
}

/// The tray icon reflects routing state: colored when routing, grey when
/// paused.
fn tray_icon(paused: bool) -> anyhow::Result<Icon> {
    let rgba = if paused { ICON_GREY_RGBA } else { ICON_RGBA };
    Icon::from_rgba(rgba.to_vec(), ICON_SIZE, ICON_SIZE)
        .map_err(|e| anyhow::anyhow!("bad icon data: {e}"))
}

fn tooltip_text(paused: bool) -> String {
    if paused {
        format!(
            "Linkport {} — routing paused (all links to default browser)",
            env!("CARGO_PKG_VERSION")
        )
    } else {
        format!(
            "Linkport {} — rule-based browser router",
            env!("CARGO_PKG_VERSION")
        )
    }
}

fn post_cmd(cmd: usize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
    let tid = TRAY_THREAD_ID.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            PostThreadMessageW(tid, WM_TRAY_CMD, cmd, 0);
        }
    }
}

fn post_quit() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
    let tid = TRAY_THREAD_ID.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            PostThreadMessageW(tid, WM_QUIT, 0, 0);
        }
    }
}

/// Put UTF-16 text on the clipboard via Win32 (no extra crates).
fn set_clipboard_text(text: &str) -> bool {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    const CF_UNICODETEXT: u32 = 13;

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        let mut ok = false;
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let hglobal = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
        if hglobal != std::ptr::null_mut() {
            let dst = GlobalLock(hglobal) as *mut u16;
            if !dst.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
                GlobalUnlock(hglobal);
                if EmptyClipboard() != 0
                    && SetClipboardData(CF_UNICODETEXT, hglobal) != std::ptr::null_mut()
                {
                    ok = true; // system owns hglobal on success
                }
            }
        }
        CloseClipboard();
        ok
    }
}
