//! Game tweaks: finds a game's programs (main game + its own launcher), its settings
//! files (.ini/.cfg), edits them safely (with a backup) and can force borderless windowed mode.

use crate::library::WindowMode;
use std::path::{Path, PathBuf};

// ---------- finding programs ----------

#[derive(Clone, Debug)]
pub struct ExeInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub is_launcher: bool,
}

const LAUNCHER_WORDS: &[&str] = &["launcher", "edition", "gamer", "config", "settings", "setup_game", "options", "launch", "start"];
const SKIP_WORDS: &[&str] = &["unins", "redist", "vcredist", "dxsetup", "dotnet", "crash", "report", "prereq", "directx", "physx", "easyanticheat_setup", "cleanup", "update"];

/// The game's main folder: climbs out of typical "bin" sub-folders.
pub fn game_root(program: &Path) -> PathBuf {
    let mut dir = program.parent().unwrap_or(Path::new(".")).to_path_buf();
    for _ in 0..2 {
        let name = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ["bin", "bin32", "bin64", "binaries", "win64", "win32", "x64", "x86", "system", "exe"].contains(&name.as_str()) {
            if let Some(p) = dir.parent() {
                dir = p.to_path_buf();
                continue;
            }
        }
        break;
    }
    dir
}

pub fn find_exes(root: &Path) -> Vec<ExeInfo> {
    let mut out = Vec::new();
    walk(root, 0, 4, &mut |p, size| {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext != "exe" {
            return;
        }
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let l = name.to_lowercase();
        if SKIP_WORDS.iter().any(|w| l.contains(w)) {
            return;
        }
        let is_launcher = LAUNCHER_WORDS.iter().any(|w| l.contains(w));
        out.push(ExeInfo { path: p.to_path_buf(), name, size, is_launcher });
    });
    out.sort_by(|a, b| b.is_launcher.cmp(&a.is_launcher).then(b.size.cmp(&a.size)));
    out.truncate(60);
    out
}

/// Picks the game's own launcher (e.g. "Game - Gamer Edition.exe" or "Launcher.exe"),
/// never the main program itself.
pub fn detect_launcher(program: &Path) -> Option<String> {
    let root = game_root(program);
    find_exes(&root)
        .into_iter()
        .find(|e| e.is_launcher && !e.path.as_os_str().eq_ignore_ascii_case(program.as_os_str()))
        .map(|e| e.path.to_string_lossy().to_string())
}

fn walk(dir: &Path, depth: usize, max: usize, f: &mut dyn FnMut(&Path, u64)) {
    if depth > max {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for d in rd.flatten().take(3000) {
        let Ok(md) = d.metadata() else { continue };
        if md.is_dir() {
            walk(&d.path(), depth + 1, max, f);
        } else {
            f(&d.path(), md.len());
        }
    }
}

// ---------- settings files ----------

/// .ini / .cfg files in the game folder, plus matching folders in Documents\My Games,
/// Documents, AppData\Local and AppData\Roaming (where many games keep their settings).
pub fn find_config_files(root: &Path, game_name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut add = |p: &Path, size: u64| {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "ini" | "cfg" | "conf" | "prefs") && size < 1024 * 1024 && out.len() < 80 {
            out.push(p.to_path_buf());
        }
    };
    walk(root, 0, 4, &mut add);

    let words: Vec<String> = game_name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 4 && !["edition", "game", "games", "gold", "complete", "deluxe"].contains(w))
        .map(|w| w.to_string())
        .collect();
    if !words.is_empty() {
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        let mut bases = vec![
            Path::new(&home).join("Documents").join("My Games"),
            Path::new(&home).join("Documents"),
            Path::new(&home).join("OneDrive").join("Documents").join("My Games"),
            Path::new(&home).join("OneDrive").join("Documents"),
        ];
        for v in ["LOCALAPPDATA", "APPDATA"] {
            if let Ok(p) = std::env::var(v) {
                bases.push(PathBuf::from(p));
            }
        }
        for base in bases {
            let Ok(rd) = std::fs::read_dir(&base) else { continue };
            for d in rd.flatten() {
                let n = d.file_name().to_string_lossy().to_lowercase();
                if d.path().is_dir() && words.iter().any(|w| n.contains(w.as_str())) {
                    walk(&d.path(), 0, 3, &mut add);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub line: usize,
    pub section: String,
    pub key: String,
    pub value: String,
    /// Text of the line up to the start of the value (key, '=' and spacing), kept on save.
    prefix: String,
    original: String,
}

impl Entry {
    pub fn changed(&self) -> bool {
        self.value != self.original
    }
}

#[derive(Clone, Debug)]
pub struct CfgFile {
    pub path: PathBuf,
    lines: Vec<String>,
    pub entries: Vec<Entry>,
    utf16: bool,
    crlf: bool,
}

pub fn load(path: &Path) -> Result<CfgFile, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't open {}: {e}", path.display()))?;
    let (text, utf16) = if bytes.starts_with(&[0xFF, 0xFE]) {
        let u: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        (String::from_utf16_lossy(&u), true)
    } else {
        let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&bytes);
        (String::from_utf8_lossy(b).to_string(), false)
    };
    let crlf = text.contains("\r\n");
    let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let mut entries = Vec::new();
    let mut section = String::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') || t.starts_with("//") {
            continue;
        }
        if t.starts_with('[') && t.ends_with(']') {
            section = t[1..t.len() - 1].to_string();
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            if key.is_empty() {
                continue;
            }
            let after = &line[eq + 1..];
            let ws = after.len() - after.trim_start().len();
            let value = after.trim().to_string();
            entries.push(Entry {
                line: i,
                section: section.clone(),
                key,
                prefix: line[..eq + 1 + ws].to_string(),
                original: value.clone(),
                value,
            });
        }
    }
    Ok(CfgFile { path: path.to_path_buf(), lines, entries, utf16, crlf })
}

impl CfgFile {
    pub fn dirty(&self) -> bool {
        self.entries.iter().any(|e| e.changed())
    }

    /// Writes changes back, keeping a one-time backup next to the file.
    pub fn save(&mut self) -> Result<String, String> {
        let backup = PathBuf::from(format!("{}.isobubble-backup", self.path.to_string_lossy()));
        if !backup.exists() {
            std::fs::copy(&self.path, &backup).map_err(|e| format!("Couldn't make a backup: {e}"))?;
        }
        for e in &self.entries {
            if e.changed() {
                self.lines[e.line] = format!("{}{}", e.prefix, e.value);
            }
        }
        let nl = if self.crlf { "\r\n" } else { "\n" };
        let mut text = self.lines.join(nl);
        text.push_str(nl);
        let bytes: Vec<u8> = if self.utf16 {
            let mut b = vec![0xFF, 0xFE];
            b.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
            b
        } else {
            text.into_bytes()
        };
        std::fs::write(&self.path, bytes).map_err(|e| format!("Couldn't save {}: {e} (is the game running, or the file read-only?)", self.path.display()))?;
        let n = self.entries.iter().filter(|e| e.changed()).count();
        for e in &mut self.entries {
            e.original = e.value.clone();
        }
        Ok(format!("Saved {n} change(s). The original is kept as {}", backup.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()))
    }

    /// Sets the window-mode related keys this file has. Returns how many values changed.
    pub fn apply_window_mode(&mut self, mode: WindowMode) -> usize {
        if mode == WindowMode::GameDefault {
            return 0;
        }
        let mut n = 0;
        for e in &mut self.entries {
            let k: String = e.key.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
            let Some(style) = BoolStyle::of(&e.value) else { continue };
            let want = if k.contains("fullscreen") || k == "exclusivefullscreen" {
                Some(mode == WindowMode::Fullscreen)
            } else if k.contains("borderless") || k.contains("noborder") || k.contains("fullscreenwindow") {
                Some(mode == WindowMode::Borderless)
            } else if k == "windowed" || k.ends_with("windowed") || k == "window" || k == "windowmode" && style.numeric {
                Some(mode != WindowMode::Fullscreen)
            } else {
                None
            };
            if let Some(w) = want {
                let v = style.format(w);
                if v != e.value {
                    e.value = v;
                    n += 1;
                }
            }
        }
        n
    }

    /// Sets resolution / refresh-rate keys to the given monitor mode. Returns how many changed.
    pub fn apply_resolution(&mut self, w: u32, h: u32, hz: u32) -> usize {
        let mut n = 0;
        for e in &mut self.entries {
            let k: String = e.key.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
            let numeric = !e.value.is_empty() && e.value.trim().chars().all(|c| c.is_ascii_digit());
            let new = if numeric && ["resx", "resolutionx", "resolutionwidth", "screenwidth", "width", "xres", "sizex", "displaywidth", "fullscreenwidth", "resolutionsizex"].contains(&k.as_str()) {
                Some(w.to_string())
            } else if numeric && ["resy", "resolutiony", "resolutionheight", "screenheight", "height", "yres", "sizey", "displayheight", "fullscreenheight", "resolutionsizey"].contains(&k.as_str()) {
                Some(h.to_string())
            } else if numeric && ["refreshrate", "refresh", "hz", "frequency", "displayfrequency", "monitorrefreshrate"].contains(&k.as_str()) {
                Some(hz.to_string())
            } else if k == "resolution" && e.value.to_lowercase().contains('x') {
                Some(format!("{w}x{h}"))
            } else {
                None
            };
            if let Some(v) = new {
                if v != e.value {
                    e.value = v;
                    n += 1;
                }
            }
        }
        n
    }

    /// Window-mode related keys found, to show the person.
    pub fn window_keys(&self) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| {
                let k = e.key.to_lowercase();
                k.contains("fullscreen") || k.contains("windowed") || k.contains("borderless") || k.contains("resolution") || k.contains("width") || k.contains("height") || k.contains("vsync") || k == "window"
            })
            .collect()
    }
}

struct BoolStyle {
    on: &'static str,
    off: &'static str,
    upper: bool,
    numeric: bool,
}

impl BoolStyle {
    fn of(v: &str) -> Option<BoolStyle> {
        let l = v.trim().trim_matches('"').to_lowercase();
        let upper = v.trim().trim_matches('"').chars().next().map(|c| c.is_uppercase()).unwrap_or(false);
        let (on, off, numeric) = match l.as_str() {
            "0" | "1" => ("1", "0", true),
            "true" | "false" => ("true", "false", false),
            "yes" | "no" => ("yes", "no", false),
            "on" | "off" => ("on", "off", false),
            _ => return None,
        };
        Some(BoolStyle { on, off, upper, numeric })
    }
    fn format(&self, b: bool) -> String {
        let s = if b { self.on } else { self.off };
        if self.upper {
            let mut c = s.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        } else {
            s.to_string()
        }
    }
}

/// Applies a window mode to every settings file of a game. Returns a short summary.
pub fn apply_mode_to_files(files: &[PathBuf], mode: WindowMode) -> String {
    let mut changed_files = 0;
    let mut changed = 0;
    let mut errors = Vec::new();
    for f in files {
        let Ok(mut c) = load(f) else { continue };
        let n = c.apply_window_mode(mode);
        if n > 0 {
            match c.save() {
                Ok(_) => {
                    changed += n;
                    changed_files += 1;
                }
                Err(e) => errors.push(e),
            }
        }
    }
    let mut s = if changed == 0 {
        "No window-mode settings needed changing in the game's settings files.".to_string()
    } else {
        format!("Changed {changed} setting(s) in {changed_files} file(s) to {} (backups kept).", mode.label().to_lowercase())
    };
    if let Some(e) = errors.first() {
        s.push_str(&format!(" {e}"));
    }
    s
}

// ---------- forcing borderless windowed ----------

/// Watches for the game's window (any program running from the game folder) and removes its
/// border, stretching it over the whole screen. Runs in the background for up to 15 minutes.
pub fn force_borderless(root: PathBuf, program: &Path, monitor: Option<String>) {
    // The chosen monitor's area (None = whichever monitor the game opens on).
    let area = monitor.and_then(|d| crate::display::by_device(&d)).map(|m| (m.x, m.y, m.width as i32, m.height as i32));
    let keep = program.to_string_lossy().to_lowercase();
    // Never stretch the game's own launcher / settings windows.
    let exclude: Vec<String> = find_exes(&root)
        .into_iter()
        .filter(|e| e.is_launcher)
        .map(|e| e.path.to_string_lossy().to_lowercase())
        .filter(|p| *p != keep)
        .collect();
    #[cfg(windows)]
    std::thread::spawn(move || borderless::watch(root, exclude, area));
    #[cfg(not(windows))]
    let _ = (root, exclude, area);
}

#[cfg(windows)]
mod borderless {
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    struct Search {
        root: String,
        exclude: Vec<String>,
        found: Vec<HWND>,
    }

    unsafe fn process_path(pid: u32) -> Option<String> {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]).to_lowercase())
    }

    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let s = &mut *(lp as *mut Search);
        if IsWindowVisible(hwnd) == 0 || !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut r);
        if r.right - r.left < 400 || r.bottom - r.top < 300 {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if let Some(p) = process_path(pid) {
            if p.starts_with(&s.root) && !s.exclude.contains(&p) {
                s.found.push(hwnd);
            }
        }
        1
    }

    unsafe fn make_borderless(hwnd: HWND, area: Option<(i32, i32, i32, i32)>) -> bool {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let border = WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;
        let m = match area {
            Some((x, y, w, h)) => RECT { left: x, top: y, right: x + w, bottom: y + h },
            None => {
                let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                let mut mi: MONITORINFO = std::mem::zeroed();
                mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                GetMonitorInfoW(mon, &mut mi);
                mi.rcMonitor
            }
        };
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut r);
        let already = style & border == 0 && r.left == m.left && r.top == m.top && r.right == m.right && r.bottom == m.bottom;
        if already {
            return true;
        }
        SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !border) as isize);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex & !(WS_EX_DLGMODALFRAME | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE | WS_EX_WINDOWEDGE)) as isize);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            m.left,
            m.top,
            m.right - m.left,
            m.bottom - m.top,
            SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_SHOWWINDOW,
        );
        false
    }

    pub fn watch(root: PathBuf, exclude: Vec<String>, area: Option<(i32, i32, i32, i32)>) {
        let mut root = root.to_string_lossy().to_lowercase();
        if !root.ends_with('\\') {
            root.push('\\');
        }
        let start = std::time::Instant::now();
        let mut seen_once = false;
        let mut misses = 0;
        while start.elapsed().as_secs() < 15 * 60 {
            let mut s = Search { root: root.clone(), exclude: exclude.clone(), found: Vec::new() };
            unsafe {
                EnumWindows(Some(each), &mut s as *mut Search as LPARAM);
                for h in &s.found {
                    make_borderless(*h, area);
                }
            }
            if s.found.is_empty() {
                misses += 1;
                // The game has closed: stop watching.
                if seen_once && misses > 30 {
                    break;
                }
            } else {
                seen_once = true;
                misses = 0;
            }
            std::thread::sleep(std::time::Duration::from_millis(if seen_once { 2000 } else { 1000 }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_launcher_and_switches_to_borderless() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata").join("game");
        let program = base.join("Bin").join("MyGame.exe");
        assert_eq!(game_root(&program), base);
        let l = detect_launcher(&program).expect("launcher found");
        assert!(l.ends_with("MyGame - Gamer Edition.exe"), "{l}");
        assert!(!find_exes(&base).iter().any(|e| e.name.starts_with("unins")));

        let files = find_config_files(&base, "My Game");
        assert_eq!(files.len(), 1);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let tmp = std::env::temp_dir().join(format!("isobubble_test_{stamp}.ini"));
        std::fs::copy(&files[0], &tmp).unwrap();
        let mut c = load(&tmp).unwrap();
        assert_eq!(c.apply_window_mode(WindowMode::Borderless), 3);
        c.save().unwrap();
        let text = std::fs::read_to_string(&tmp).unwrap();
        assert!(text.contains("Fullscreen=0\r\n"), "{text}");
        assert!(text.contains("Windowed = 1\r\n"), "{text}");
        assert!(text.contains("Borderless=True\r\n"), "{text}");
        assert!(text.contains("ResX=1920") && text.contains("; comment") && text.contains("Volume=80"));
        assert!(Path::new(&format!("{}.isobubble-backup", tmp.display())).exists());
    }
}
