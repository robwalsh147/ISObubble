//! Multi-disc installs.
//!
//! 1. Automatic disc changer: every disc is mounted in its own virtual drive. While setup runs,
//!    ISObubble watches for its "insert the next disc" window, moves the wanted disc into the
//!    drive setup is reading from (the others stay mounted) and presses OK / Retry, so setup
//!    simply continues — like swapping real discs, without touching anything.
//! 2. Combine: copies every disc into one folder and builds a single virtual disc from it, so
//!    setup finds disc 2's files straight away and never asks.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Mounts every disc, with disc `first` mounted first so it gets setup's drive letter.
/// Returns (disc index, drive letter) for each disc that mounted.
pub fn mount_all_with_first(paths: &[PathBuf], first: usize) -> Result<Vec<(usize, char)>, String> {
    for p in paths {
        let _ = crate::system::dismount_iso(p);
    }
    let mut out = Vec::new();
    let l = crate::system::mount_iso(&paths[first])?;
    out.push((first, l));
    for (i, p) in paths.iter().enumerate() {
        if i != first {
            if let Ok(l) = crate::system::mount_iso(p) {
                out.push((i, l));
            }
        }
    }
    Ok(out)
}

/// Which disc a prompt asks for ("Please insert Disk 2", "CD-ROM #3", "the second disc"…).
pub fn wanted_disc(text: &str) -> Option<usize> {
    let t = text.to_lowercase();
    for (w, n) in [("second", 2), ("third", 3), ("fourth", 4), ("fifth", 5), ("sixth", 6)] {
        if t.contains(&format!("{w} disc")) || t.contains(&format!("{w} disk")) || t.contains(&format!("{w} cd")) {
            return Some(n);
        }
    }
    for word in ["disk", "disc", "cd-rom", "cd", "dvd", "volume"] {
        let mut from = 0;
        while let Some(i) = t[from..].find(word) {
            let rest = &t[from + i + word.len()..];
            let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '#' || c == '_' || c == ':' || c == '-');
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse::<usize>() {
                if (1..=12).contains(&n) {
                    return Some(n);
                }
            }
            from += i + word.len();
        }
    }
    None
}

/// Does this window text look like a "please insert the next disc" prompt?
pub fn looks_like_disc_prompt(text: &str) -> bool {
    let t = text.to_lowercase();
    let disc = ["disk", "disc", "cd", "dvd", "volume", "media"].iter().any(|w| t.contains(w));
    let ask = [
        "insert",
        "next disk",
        "next disc",
        "needs the next",
        "need the next",
        "please put",
        "cannot find",
        "could not find",
        "locate",
        "required",
        "change disk",
        "change disc",
        "swap",
    ]
    .iter()
    .any(|w| t.contains(w));
    disc && ask
}

#[derive(Clone, Debug)]
pub struct Prompt {
    pub hwnd: isize,
    pub text: String,
}

/// Finds a visible "insert disc" window belonging to another program.
pub fn find_prompt() -> Option<Prompt> {
    #[cfg(windows)]
    return win::find();
    #[cfg(not(windows))]
    None
}

/// Presses OK / Retry / Continue in the prompt (if Windows allows it).
pub fn press_ok(hwnd: isize) -> bool {
    #[cfg(windows)]
    return win::press(hwnd);
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

#[cfg(windows)]
mod win {
    use super::Prompt;
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    fn text_of(h: HWND) -> String {
        let mut buf = [0u16; 1024];
        unsafe {
            let mut res: usize = 0;
            // WM_GETTEXT works for controls in other programs; time out if the program is busy.
            let ok = SendMessageTimeoutW(h, WM_GETTEXT, buf.len() as WPARAM, buf.as_mut_ptr() as LPARAM, SMTO_ABORTIFHUNG, 300, &mut res);
            if ok == 0 {
                let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
                return String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            }
            let n = buf.iter().position(|&c| c == 0).unwrap_or(0);
            String::from_utf16_lossy(&buf[..n])
        }
    }

    fn class_of(h: HWND) -> String {
        let mut buf = [0u16; 128];
        let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    unsafe extern "system" fn child(h: HWND, lp: LPARAM) -> BOOL {
        let v = &mut *(lp as *mut Vec<(HWND, String, String)>);
        if v.len() < 200 && IsWindowVisible(h) != 0 {
            v.push((h, class_of(h), text_of(h)));
        }
        1
    }

    fn children(h: HWND) -> Vec<(HWND, String, String)> {
        let mut v: Vec<(HWND, String, String)> = Vec::new();
        unsafe {
            EnumChildWindows(h, Some(child), &mut v as *mut _ as LPARAM);
        }
        v
    }

    unsafe extern "system" fn top(h: HWND, lp: LPARAM) -> BOOL {
        let out = &mut *(lp as *mut Option<Prompt>);
        if out.is_some() || IsWindowVisible(h) == 0 {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        if pid == std::process::id() {
            return 1;
        }
        let title = text_of(h);
        let class = class_of(h);
        let kids = children(h);
        // Only dialogs: a standard dialog box, or a window that has buttons.
        let has_button = kids.iter().any(|(_, c, _)| c.eq_ignore_ascii_case("Button"));
        if class != "#32770" && !has_button {
            return 1;
        }
        if kids.len() > 120 {
            return 1; // a big app window, not a prompt
        }
        let body: String = kids.iter().filter(|(_, c, _)| !c.eq_ignore_ascii_case("Button")).map(|(_, _, t)| t.as_str()).collect::<Vec<_>>().join(" ");
        let all = format!("{title} {body}");
        if super::looks_like_disc_prompt(&all) {
            *out = Some(Prompt { hwnd: h as isize, text: all.split_whitespace().collect::<Vec<_>>().join(" ") });
        }
        1
    }

    pub fn find() -> Option<Prompt> {
        let mut out: Option<Prompt> = None;
        unsafe {
            EnumWindows(Some(top), &mut out as *mut _ as LPARAM);
        }
        out
    }

    pub fn press(hwnd: isize) -> bool {
        let h = hwnd as HWND;
        let kids = children(h);
        let wanted = ["ok", "retry", "continue", "next >", "next", "yes", "try again"];
        for w in wanted {
            if let Some((b, _, _)) = kids.iter().find(|(_, c, t)| c.eq_ignore_ascii_case("Button") && t.replace('&', "").trim().eq_ignore_ascii_case(w)) {
                unsafe {
                    return PostMessageW(*b, BM_CLICK, 0, 0) != 0;
                }
            }
        }
        // No labelled button: press Enter on the dialog (its default button).
        unsafe { PostMessageW(h, WM_KEYDOWN, 0x0D, 0) != 0 && PostMessageW(h, WM_KEYUP, 0x0D, 0) != 0 }
    }
}

/// Runs while setup is open: whenever it asks for another disc, puts that disc into setup's
/// drive (keeping the others mounted) and presses OK. `report` gets (disc index → letter)
/// updates and log lines.
pub fn watch(
    paths: Vec<PathBuf>,
    setup_letter: char,
    mut current: usize,
    stop: Arc<AtomicBool>,
    on_letters: impl Fn(Vec<(usize, char)>),
    log: impl Fn(String),
) {
    let mut handled: Vec<(isize, std::time::Instant)> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(900));
        let Some(p) = find_prompt() else { continue };
        // Don't act on the same window twice within 12 seconds.
        handled.retain(|(_, t)| t.elapsed().as_secs() < 12);
        if handled.iter().any(|(h, _)| *h == p.hwnd) {
            continue;
        }
        handled.push((p.hwnd, std::time::Instant::now()));
        let wanted = wanted_disc(&p.text).map(|n| n - 1).filter(|n| *n < paths.len()).unwrap_or((current + 1) % paths.len());
        let short: String = p.text.chars().take(120).collect();
        log(format!("Setup asked: “{short}”"));
        if wanted != current {
            log(format!("Changing discs: putting disc {} into drive {setup_letter}: (the other discs stay mounted)…", wanted + 1));
            match mount_all_with_first(&paths, wanted) {
                Ok(letters) => {
                    let got = letters.first().map(|x| x.1).unwrap_or(setup_letter);
                    on_letters(letters);
                    current = wanted;
                    if got != setup_letter {
                        log(format!("Windows gave disc {} drive {got}: instead of {setup_letter}: — if setup still asks, browse to {got}: in its window.", wanted + 1));
                    } else {
                        log(format!("Disc {} is in drive {setup_letter}:.", wanted + 1));
                    }
                }
                Err(e) => {
                    log(format!("Couldn't change discs: {e}"));
                    continue;
                }
            }
            // Give Windows a moment to notice the "new" disc, like a real drive spinning up.
            std::thread::sleep(std::time::Duration::from_millis(2500));
        }
        if press_ok(p.hwnd) {
            log("Pressed OK for you — setup continues.".into());
        } else {
            log("Press OK / Retry in the setup window to continue.".into());
        }
    }
}

/// Builds one combined virtual disc from all discs. Files with the same name keep disc 1's copy.
/// Returns the path of the new disc image.
pub fn combine(
    sources: &[std::sync::Arc<crate::iso::Source>],
    label: &str,
    progress: &dyn Fn(f32, String),
) -> Result<PathBuf, String> {
    let base = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir()).join("ISObubble").join("combined");
    let name = crate::system::safe_folder_name(label);
    let staging = base.join(format!("{name}_files"));
    let iso = base.join(format!("{name} (all discs).iso"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let total: u64 = sources.iter().map(|s| s.total_bytes()).sum::<u64>().max(1);
    let mut done = 0u64;
    let mut last = 0u64;
    for (i, s) in sources.iter().enumerate() {
        progress(done as f32 / total as f32 * 0.5, format!("Copying disc {} into the combined disc…", i + 1));
        for e in &s.entries {
            let target = staging.join(e.path.replace('/', "\\"));
            if e.is_dir {
                let _ = std::fs::create_dir_all(&target);
                continue;
            }
            if target.exists() {
                done += e.size;
                continue; // keep disc 1's copy (setup.exe, autorun.inf…)
            }
            s.copy_out(e, &target, &mut |n| {
                done += n;
                if done - last > 16 << 20 {
                    last = done;
                    progress(done as f32 / total as f32 * 0.5, format!("Copying disc {}… {}", i + 1, crate::iso::fmt_size(done)));
                }
            })
            .map_err(|er| format!("Couldn't copy {}: {er}", e.path))?;
        }
    }
    progress(0.5, "Building the combined virtual disc…".into());
    let big_file = sources.iter().flat_map(|s| s.files()).any(|e| e.size >= 4 * 1024 * 1024 * 1024 - 1);
    let vol: String = label.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ' ').take(15).collect::<String>().trim().to_string();
    let vol = if vol.is_empty() { "COMBINED".to_string() } else { vol };
    let q = crate::system::q;
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $fsi = New-Object -ComObject IMAPI2FS.MsftFileSystemImage; \
         $fsi.ChooseImageDefaultsForMediaType(18); \
         $fsi.FileSystemsToCreate = {fs}; \
         $fsi.VolumeName = {vol}; \
         $fsi.Root.AddTree({src}, $false); \
         $res = $fsi.CreateResultImage(); \
         Add-Type -TypeDefinition 'using System; using System.IO; using System.Runtime.InteropServices.ComTypes; public static class IsoOut {{ public static void Write(object s, string path, int bs, int blocks) {{ IStream st = (IStream)s; using (var fs = File.Create(path)) {{ byte[] buf = new byte[bs * 64]; int left = blocks; int done = 0; int lastPct = -1; while (left > 0) {{ int n = Math.Min(64, left); st.Read(buf, n * bs, IntPtr.Zero); fs.Write(buf, 0, n * bs); left -= n; done += n; int pct = (int)((long)done * 100 / blocks); if (pct != lastPct) {{ lastPct = pct; Console.WriteLine(\"P \" + pct); }} }} }} }} }}'; \
         [IsoOut]::Write($res.ImageStream, {out}, $res.BlockSize, $res.TotalBlocks); 'done'",
        fs = if big_file { 4 } else { 7 },
        vol = q(&vol),
        src = q(&staging.to_string_lossy()),
        out = q(&iso.to_string_lossy()),
    );
    let r = crate::system::ps_stream(&script, &mut |line| {
        if let Some(p) = line.strip_prefix("P ").and_then(|x| x.trim().parse::<f32>().ok()) {
            progress(0.5 + p / 100.0 * 0.45, format!("Building the combined virtual disc… {p:.0}%"));
        }
    });
    let _ = std::fs::remove_dir_all(&staging);
    r?;
    if !iso.exists() {
        return Err("The combined disc couldn't be created.".into());
    }
    Ok(iso)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_disc_prompts() {
        assert!(looks_like_disc_prompt("Setup Needs The Next Disk Please insert disk 2 that contains the file data2.cab"));
        assert_eq!(wanted_disc("Please insert Disk 2 that contains data2.cab"), Some(2));
        assert_eq!(wanted_disc("Insert CD-ROM #3"), Some(3));
        assert_eq!(wanted_disc("Please insert the second disc"), Some(2));
        assert_eq!(wanted_disc("Insert the disk: GAME_DISC_2"), Some(2));
        assert!(!looks_like_disc_prompt("Welcome to the setup wizard. Click Next to continue."));
    }
}

/// Real end-to-end checks with the two test discs (mounts virtual drives):
/// cargo test --bin isobubble discchanger -- --ignored --nocapture --test-threads=1
#[cfg(test)]
mod live_tests {
    use super::*;
    use std::sync::Arc;

    fn discs() -> Vec<PathBuf> {
        let t = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        vec![t.join("multi_disc1.iso"), t.join("multi_disc2.iso")]
    }
    use std::path::Path;

    #[test]
    #[ignore]
    fn swap_keeps_setups_drive_letter() {
        let d = discs();
        let first = mount_all_with_first(&d, 0).unwrap();
        println!("disc 1 first: {first:?}");
        assert_eq!(first.len(), 2);
        let setup_letter = first[0].1;
        let swapped = mount_all_with_first(&d, 1).unwrap();
        println!("disc 2 first: {swapped:?}");
        assert_eq!(swapped[0], (1, setup_letter), "disc 2 should now be in setup's drive");
        assert!(Path::new(&format!(r"{setup_letter}:DATA2.CAB")).exists());
        assert!(swapped.iter().any(|x| x.0 == 0), "disc 1 stays mounted too");
        for p in &d {
            let _ = crate::system::dismount_iso(p);
        }
    }

    #[test]
    #[ignore]
    fn combines_both_discs() {
        let srcs: Vec<Arc<crate::iso::Source>> = discs().iter().map(|p| Arc::new(crate::iso::Source::open(p).unwrap())).collect();
        let iso = combine(&srcs, "TESTGAME", &|f, s| println!("{:.0}% {s}", f * 100.0)).unwrap();
        let c = crate::iso::Source::open(&iso).unwrap();
        let names: Vec<String> = c.files().map(|e| e.path.to_uppercase()).collect();
        println!("{names:?}");
        for f in ["SETUP.EXE", "DATA1.CAB", "DATA2.CAB", "DISK2.TAG"] {
            assert!(names.iter().any(|n| n == f), "missing {f}");
        }
        let _ = std::fs::remove_file(&iso);
    }

    #[test]
    #[ignore]
    fn spots_and_answers_a_disc_prompt() {
        // A stand-in for an installer's "insert the next disk" window.
        let mut c = std::process::Command::new("powershell.exe");
        c.args(["-NoProfile", "-Command", "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('Please insert Disk 2 that contains the file data2.cab.', 'Setup Needs The Next Disk', 'OKCancel') | Out-Null"]);
        let mut child = c.spawn().unwrap();
        let mut found = None;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            found = find_prompt();
            if found.is_some() {
                break;
            }
        }
        let p = found.expect("prompt spotted");
        println!("{p:?}");
        assert_eq!(wanted_disc(&p.text), Some(2));
        assert!(press_ok(p.hwnd));
        let mut closed = false;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            if child.try_wait().unwrap().is_some() {
                closed = true;
                break;
            }
        }
        if !closed {
            let _ = child.kill();
        }
        assert!(closed, "OK was pressed and the prompt closed");
    }
}
