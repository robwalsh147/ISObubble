//! Steam-style tray icon: lives in the notification area, its tooltip shows what ISObubble is
//! doing ("Installing King Kong – 45%"), clicking opens the window, and right-clicking lists your
//! games so any of them can be launched straight from the tray. Closing the window keeps
//! ISObubble running there.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, Ordering};
use std::sync::Mutex;

static HWND: AtomicIsize = AtomicIsize::new(0);
static HIDDEN: AtomicBool = AtomicBool::new(false);
static QUIT: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static MENU_DIRTY: AtomicBool = AtomicBool::new(true);
/// Page the window should open on next time it's shown (0 = keep).
static PENDING_PAGE: AtomicU8 = AtomicU8::new(0);
static STATUS: Mutex<String> = Mutex::new(String::new());
static GAMES: Mutex<Vec<crate::library::Item>> = Mutex::new(Vec::new());
static FINGERPRINT: Mutex<String> = Mutex::new(String::new());

pub const PAGE_LIBRARY: u8 = 1;
pub const PAGE_INSTALL: u8 = 2;
pub const PAGE_ABANDONWARE: u8 = 3;
pub const PAGE_SETTINGS: u8 = 4;

pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}
pub fn quitting() -> bool {
    QUIT.load(Ordering::Relaxed)
}
pub fn is_hidden() -> bool {
    HIDDEN.load(Ordering::Relaxed)
}
pub fn take_pending_page() -> u8 {
    PENDING_PAGE.swap(0, Ordering::Relaxed)
}

pub fn set_hwnd(h: isize) {
    HWND.store(h, Ordering::Relaxed);
}

/// What the tooltip says, e.g. "Installing King Kong – 45%". Empty = idle.
pub fn set_status(s: impl Into<String>) {
    if let Ok(mut g) = STATUS.lock() {
        *g = s.into();
    }
}

/// Keeps the tray's game list in step with the library.
pub fn sync_games(items: &[crate::library::Item]) {
    let fp: String = items.iter().map(|i| format!("{}:{}:{:?};", i.id, i.name, i.program)).collect();
    if let Ok(mut f) = FINGERPRINT.lock() {
        if *f != fp {
            *f = fp;
            if let Ok(mut g) = GAMES.lock() {
                *g = items.to_vec();
            }
            MENU_DIRTY.store(true, Ordering::Relaxed);
        }
    }
}

pub fn hide() {
    #[cfg(windows)]
    unsafe {
        let h = HWND.load(Ordering::Relaxed);
        if h != 0 && ACTIVE.load(Ordering::Relaxed) {
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(h as _, windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE);
            HIDDEN.store(true, Ordering::Relaxed);
        }
    }
}

pub fn show() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let h = HWND.load(Ordering::Relaxed);
        if h != 0 {
            ShowWindow(h as _, SW_SHOW);
            ShowWindow(h as _, SW_RESTORE);
            SetForegroundWindow(h as _);
            HIDDEN.store(false, Ordering::Relaxed);
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::cell::RefCell;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    thread_local! {
        static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
        static LAST_TIP: RefCell<String> = const { RefCell::new(String::new()) };
    }

    fn build_menu() -> Menu {
        let menu = Menu::new();
        let _ = menu.append(&MenuItem::with_id("open", "Open ISObubble", true, None));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let games = GAMES.lock().map(|g| g.clone()).unwrap_or_default();
        let playable: Vec<_> = games.iter().filter(|g| g.program.is_some()).collect();
        if playable.is_empty() {
            let _ = menu.append(&MenuItem::with_id("none", "No games yet", false, None));
        } else {
            // Most recently added first, like a "recent games" list.
            let mut list = playable.clone();
            list.sort_by(|a, b| b.added.cmp(&a.added));
            for g in list.iter().take(12) {
                let _ = menu.append(&MenuItem::with_id(format!("play:{}", g.id), format!("▶  {}", g.name), true, None));
            }
            let launchers: Vec<_> = list.iter().filter(|g| g.launcher.is_some()).collect();
            if !launchers.is_empty() {
                let sub = Submenu::new("Game launchers", true);
                for g in launchers.iter().take(12) {
                    let _ = sub.append(&MenuItem::with_id(format!("launch:{}", g.id), format!("🚀  {}", g.name), true, None));
                }
                let _ = menu.append(&sub);
            }
        }
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id("library", "Library", true, None));
        let _ = menu.append(&MenuItem::with_id("install", "Install a disc…", true, None));
        if crate::settings::Settings::load().abandonware_addon {
            let _ = menu.append(&MenuItem::with_id("abandon", "Abandonware", true, None));
        }
        let _ = menu.append(&MenuItem::with_id("settings", "Settings & updates", true, None));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id("quit", "Exit ISObubble", true, None));
        menu
    }

    fn play(id: u64, launcher: bool) {
        let item = GAMES.lock().ok().and_then(|g| g.iter().find(|i| i.id == id).cloned());
        if let Some(item) = item {
            std::thread::spawn(move || {
                if item.needs_disc {
                    if let Some(d) = item.discs.first().filter(|d| std::path::Path::new(d).exists()) {
                        let _ = crate::system::mount_iso(std::path::Path::new(d));
                    }
                }
                let r = crate::start_game(&item, launcher);
                crate::applog::log(match r {
                    Ok(()) => format!("Started {} from the tray", item.name),
                    Err(e) => format!("Couldn't start {} from the tray: {e}", item.name),
                });
            });
        }
    }

    unsafe extern "system" fn tick(_: windows_sys::Win32::Foundation::HWND, _: u32, _: usize, _: u32) {
        TRAY.with(|t| {
            let t = t.borrow();
            let Some(tray) = t.as_ref() else { return };
            let status = STATUS.lock().map(|s| s.clone()).unwrap_or_default();
            let tip = if status.is_empty() { "ISObubble".to_string() } else { format!("ISObubble – {status}") };
            LAST_TIP.with(|l| {
                if *l.borrow() != tip {
                    let _ = tray.set_tooltip(Some(&tip));
                    *l.borrow_mut() = tip;
                }
            });
            if MENU_DIRTY.swap(false, Ordering::Relaxed) {
                tray.set_menu(Some(Box::new(build_menu())));
            }
        });
    }

    pub fn create(ctx: egui::Context) {
        let icon = Icon::from_rgba(crate::theme::icon_pixels(32), 32, 32).ok();
        let mut b = TrayIconBuilder::new().with_tooltip("ISObubble").with_menu(Box::new(build_menu())).with_menu_on_left_click(false);
        if let Some(i) = icon {
            b = b.with_icon(i);
        }
        let Ok(tray) = b.build() else { return };
        MENU_DIRTY.store(false, Ordering::Relaxed);
        TRAY.with(|t| *t.borrow_mut() = Some(tray));
        ACTIVE.store(true, Ordering::Relaxed);

        let c2 = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| match e {
            TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                show();
                c2.request_repaint();
            }
            _ => {}
        }));
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let id = e.id.0.as_str();
            let page = match id {
                "open" => Some(0),
                "library" => Some(PAGE_LIBRARY),
                "install" => Some(PAGE_INSTALL),
                "abandon" => Some(PAGE_ABANDONWARE),
                "settings" => Some(PAGE_SETTINGS),
                _ => None,
            };
            if let Some(p) = page {
                if p != 0 {
                    PENDING_PAGE.store(p, Ordering::Relaxed);
                }
                show();
                ctx.request_repaint();
            } else if id == "quit" {
                QUIT.store(true, Ordering::Relaxed);
                show();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                ctx.request_repaint();
            } else if let Some(n) = id.strip_prefix("play:") {
                play(n.parse().unwrap_or(0), false);
            } else if let Some(n) = id.strip_prefix("launch:") {
                play(n.parse().unwrap_or(0), true);
            }
        }));
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(std::ptr::null_mut(), 0, 1000, Some(tick));
        }
    }
}

pub const WINDOW_TITLE: &str = "ISObubble — Disc Installer & Launcher";

/// Like Steam, only one ISObubble runs at a time. Returns false if another copy is already
/// running — that copy's window is brought back (even from the tray) and this one should exit.
pub fn claim_single_instance() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows_sys::Win32::System::Threading::CreateMutexW;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let name: Vec<u16> = "Local\\ISObubble_single_instance".encode_utf16().chain(std::iter::once(0)).collect();
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if !h.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            let title: Vec<u16> = WINDOW_TITLE.encode_utf16().chain(std::iter::once(0)).collect();
            let w = FindWindowW(std::ptr::null(), title.as_ptr());
            if !w.is_null() {
                ShowWindow(w, SW_SHOW);
                ShowWindow(w, SW_RESTORE);
                SetForegroundWindow(w);
            }
            return false;
        }
        // The mutex handle is kept for the life of the program on purpose.
    }
    true
}

/// Creates the tray icon (call once, on the UI thread).
pub fn create(ctx: egui::Context) {
    #[cfg(windows)]
    imp::create(ctx);
    #[cfg(not(windows))]
    let _ = ctx;
}
