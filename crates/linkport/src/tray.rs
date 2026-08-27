//! System tray presence for the portal daemon (Windows only).
//!
//! Shows a tray icon while `linkport serve` runs:
//! - left click      → open the web portal
//! - right click     → menu: Open portal / Version / Portal URL / Quit
//!
//! Quit performs a graceful axum shutdown via the watch channel, then posts
//! WM_QUIT to end the tray's message loop.

#![cfg(windows)]

use std::sync::atomic::{AtomicU32, Ordering};

use tokio::sync::watch::Sender as WatchSender;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::portal::AppState;

static TRAY_THREAD_ID: AtomicU32 = AtomicU32::new(0);

const ICON_RGBA: &[u8] = include_bytes!("../../../resources/icon32.rgba");
const ICON_SIZE: u32 = 32;

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

    // Build the menu. Items with enabled=false are informational.
    let open = MenuItem::with_id("open", "Open Linkport portal", true, None);
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
    let quit_item = MenuItem::with_id("quit", "Quit Linkport", true, None);

    let menu = Menu::new();
    menu.append(&open)?;
    menu.append(&sep1)?;
    menu.append(&version)?;
    menu.append(&addr)?;
    menu.append(&sep2)?;
    menu.append(&quit_item)?;

    let icon = Icon::from_rgba(ICON_RGBA.to_vec(), ICON_SIZE, ICON_SIZE)?;
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(format!(
            "Linkport {} — rule-based browser router",
            env!("CARGO_PKG_VERSION")
        ))
        .with_icon(icon)
        .build()?;

    // Menu events (right-click menu) — own thread, blocking recv.
    let menu_state = state.clone();
    let menu_quit = quit.clone();
    std::thread::spawn(move || {
        while let Ok(ev) = MenuEvent::receiver().recv() {
            match ev.id().as_ref() {
                "open" => crate::portal::open_portal_url(&menu_state),
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
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
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
