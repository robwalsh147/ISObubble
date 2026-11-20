//! Compatibility checker: looks at a game's program and folder and explains what may stop it
//! running on this PC, with one-click fixes (Windows compatibility modes, DOSBox, downloads).

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Good,
    Info,
    Warn,
    Bad,
}

#[derive(Clone, Debug)]
pub enum Fix {
    /// Add a Windows compatibility setting (e.g. WINXPSP3, RUNASADMIN).
    Layer(&'static str),
    UseDosbox,
    NeedsDisc,
    Url(&'static str),
}

#[derive(Clone, Debug)]
pub struct Check {
    pub level: Level,
    pub title: String,
    pub detail: String,
    pub fix: Option<(String, Fix)>,
}

/// Windows compatibility modes the person can pick.
pub const MODES: &[(&str, &str)] = &[
    ("", "None"),
    ("WIN95", "Windows 95"),
    ("WIN98", "Windows 98 / ME"),
    ("WINXPSP3", "Windows XP (Service Pack 3)"),
    ("VISTASP2", "Windows Vista (Service Pack 2)"),
    ("WIN7RTM", "Windows 7"),
    ("WIN8RTM", "Windows 8"),
];

/// Extra compatibility switches.
pub const FLAGS: &[(&str, &str)] = &[
    ("RUNASADMIN", "Run as administrator"),
    ("DISABLEDXMAXIMIZEDWINDOWEDMODE", "Disable fullscreen optimisations"),
    ("HIGHDPIAWARE", "Fix blurry / wrongly-sized window (high DPI)"),
    ("16BITCOLOR", "Reduced colour mode (16-bit)"),
    ("640X480", "Run in 640×480"),
];

fn has(h: &[u8], n: &str) -> bool {
    crate::pe::find(h, n.as_bytes()).is_some()
}

fn has_ci(lower: &[u8], n: &str) -> bool {
    crate::pe::find(lower, n.to_lowercase().as_bytes()).is_some()
}

fn system_has(dll: &str, is_32bit: bool) -> bool {
    let win = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let dir = if is_32bit && Path::new(&win).join("SysWOW64").exists() { "SysWOW64" } else { "System32" };
    Path::new(&win).join(dir).join(dll).exists()
}

/// Finds an installed DOSBox (Staging, X or classic).
pub fn find_dosbox() -> Option<PathBuf> {
    let mut bases = Vec::new();
    for v in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Ok(b) = std::env::var(v) {
            bases.push(PathBuf::from(&b));
            bases.push(Path::new(&b).join("Programs"));
        }
    }
    for b in bases {
        let Ok(rd) = std::fs::read_dir(&b) else { continue };
        for d in rd.flatten() {
            if !d.file_name().to_string_lossy().to_lowercase().contains("dosbox") {
                continue;
            }
            for exe in ["dosbox.exe", "dosbox-x.exe", "dosbox-staging.exe"] {
                let p = d.path().join(exe);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

pub fn check_game(program: &Path, root: &Path, needs_disc: bool) -> Vec<Check> {
    let mut out = Vec::new();
    let mut push = |level, title: &str, detail: &str, fix: Option<(&str, Fix)>| {
        out.push(Check { level, title: title.into(), detail: detail.into(), fix: fix.map(|(l, f)| (l.to_string(), f)) });
    };
    let Ok(bytes) = std::fs::read(program) else {
        push(Level::Bad, "Program not found", "The game's program file couldn't be opened. Choose it again in the Launch tab.", None);
        return out;
    };
    let ext = program.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext == "bat" || ext == "com" {
        push(Level::Bad, "DOS program", "This is a DOS program. Windows 10/11 can't run DOS games directly — DOSBox can.", Some(("Run with DOSBox", Fix::UseDosbox)));
        return out;
    }
    if bytes.len() < 64 || &bytes[0..2] != b"MZ" {
        push(Level::Warn, "Not a normal Windows program", "This file isn't a Windows program. If it's a shortcut or script it may still work.", None);
        return out;
    }
    let lfanew = u32::from_le_bytes([bytes[0x3c], bytes[0x3d], bytes[0x3e], bytes[0x3f]]) as usize;
    let sig = bytes.get(lfanew..lfanew + 2).unwrap_or(&[]);
    if sig == b"NE" {
        push(
            Level::Bad,
            "16-bit Windows program (Windows 3.1 era)",
            "64-bit Windows can't run 16-bit programs at all. Options: the free “otvdm/winevdm” add-on, DOSBox-X with Windows 3.1, or a virtual machine.",
            Some(("Get otvdm", Fix::Url("https://github.com/otya128/winevdm/releases"))),
        );
        return out;
    }
    if bytes.get(lfanew..lfanew + 4) != Some(b"PE\0\0") {
        push(Level::Bad, "DOS program", "This is a DOS program. Windows 10/11 can't run DOS games directly — DOSBox can.", Some(("Run with DOSBox", Fix::UseDosbox)));
        return out;
    }

    let info = crate::pe::parse(&bytes);
    let is_32 = info.as_ref().map(|i| i.machine.starts_with("32")).unwrap_or(true);
    let opt = lfanew + 24;
    let os_major = u16::from_le_bytes([*bytes.get(opt + 40).unwrap_or(&0), *bytes.get(opt + 41).unwrap_or(&0)]);
    let sub_major = u16::from_le_bytes([*bytes.get(opt + 48).unwrap_or(&0), *bytes.get(opt + 49).unwrap_or(&0)]);
    let era = match sub_major.max(os_major) {
        0..=4 => "Windows 95/98",
        5 => "Windows 2000/XP",
        6 => "Windows Vista/7/8",
        _ => "Windows 10 or newer",
    };
    push(
        Level::Good,
        &format!("{} Windows program", if is_32 { "32-bit" } else { "64-bit" }),
        &format!("Built for {era}. {}", if is_32 { "32-bit games run fine on 64-bit Windows." } else { "" }),
        None,
    );
    let old = sub_major.max(os_major) <= 4;
    if old {
        push(
            Level::Warn,
            "Made for Windows 95/98",
            "Very old games often run better in Windows XP compatibility mode.",
            Some(("Use Windows XP mode", Fix::Layer("WINXPSP3"))),
        );
    }

    let region = &bytes[..bytes.len().min(64 * 1024 * 1024)];
    let lower: Vec<u8> = region.iter().map(|c| c.to_ascii_lowercase()).collect();

    // DirectX
    if has_ci(&lower, "ddraw.dll") || has_ci(&lower, "d3d8.dll") || has_ci(&lower, "d3dim.dll") || has_ci(&lower, "d3drm.dll") {
        push(
            Level::Warn,
            "Uses old DirectX (DirectDraw / Direct3D 7–8)",
            "Modern Windows emulates these slowly; colours, speed or alt-tab can glitch. The free dgVoodoo2 wrapper usually fixes it (copy its files next to the game).",
            Some(("Get dgVoodoo2", Fix::Url("https://github.com/dege-diosg/dgVoodoo2/releases"))),
        );
    }
    let mut missing_dx = Vec::new();
    for dll in ["d3dx9_24.dll", "d3dx9_30.dll", "d3dx9_31.dll", "d3dx9_34.dll", "d3dx9_35.dll", "d3dx9_36.dll", "d3dx9_39.dll", "d3dx9_40.dll", "d3dx9_41.dll", "d3dx9_42.dll", "d3dx9_43.dll", "xinput1_3.dll", "xinput1_2.dll", "x3daudio1_7.dll", "xaudio2_7.dll", "d3dx10_43.dll", "d3dx11_43.dll"] {
        if has_ci(&lower, dll) && !system_has(dll, is_32) && !root.join(dll).exists() && !program.with_file_name(dll).exists() {
            missing_dx.push(dll);
        }
    }
    if !missing_dx.is_empty() {
        push(
            Level::Bad,
            "Missing DirectX 9 parts",
            &format!("The game needs {} which isn't on this PC. Install Microsoft's “DirectX End-User Runtime” (June 2010).", missing_dx.join(", ")),
            Some(("Get DirectX runtime", Fix::Url("https://www.microsoft.com/en-us/download/details.aspx?id=8109"))),
        );
    }
    // Visual C++ runtimes
    let mut missing_vc = Vec::new();
    for (dll, name) in [
        ("msvcr71.dll", "2003"),
        ("msvcp71.dll", "2003"),
        ("msvcr80.dll", "2005"),
        ("msvcr90.dll", "2008"),
        ("msvcr100.dll", "2010"),
        ("msvcp100.dll", "2010"),
        ("msvcr110.dll", "2012"),
        ("msvcp110.dll", "2012"),
        ("msvcr120.dll", "2013"),
        ("msvcp120.dll", "2013"),
        ("vcruntime140.dll", "2015–2022"),
        ("msvcp140.dll", "2015–2022"),
    ] {
        if has_ci(&lower, dll) && !system_has(dll, is_32) && !root.join(dll).exists() && !program.with_file_name(dll).exists() {
            // msvcr80/90 live in WinSxS, so only report them as "may be needed".
            if !missing_vc.contains(&name) {
                missing_vc.push(name);
            }
        }
    }
    if !missing_vc.is_empty() {
        push(
            Level::Bad,
            "Missing Visual C++ runtime",
            &format!("The game needs the Microsoft Visual C++ {} runtime ({}). Most games include it in a “redist” folder on the disc; otherwise get the all-in-one package.", missing_vc.join(" / "), if is_32 { "x86" } else { "x64" }),
            Some(("Get Visual C++ runtimes", Fix::Url("https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist"))),
        );
    }
    if has_ci(&lower, "mscoree.dll") {
        push(Level::Info, ".NET program", "Needs the .NET Framework. Windows usually installs the right version automatically when the game first starts.", None);
    }
    if has(region, "PhysXLoader.dll") && !system_has("PhysXLoader.dll", is_32) && !program.with_file_name("PhysXLoader.dll").exists() {
        push(
            Level::Warn,
            "Needs NVIDIA PhysX (legacy)",
            "Install the PhysX legacy system software if the game says PhysXLoader.dll is missing.",
            Some(("Get PhysX legacy", Fix::Url("https://www.nvidia.com/en-us/drivers/physx/physx-9-13-0604-legacy-driver/"))),
        );
    }

    // Disc copy protection
    let protected_files = ["secdrv.sys", "drvmgt.dll", "00000001.tmp", "clcd32.dll", "clokspl.exe", "dplayerx.dll"];
    let found_file = protected_files.iter().find(|f| root.join(f).exists() || program.with_file_name(f).exists());
    if found_file.is_some() || has(region, "SecuROM") || has(region, "BoG_ *90.0&!!  Yy>") || has(region, ".securom") || has(region, "SafeDisc") || has(region, "StarForce") {
        push(
            Level::Bad,
            "Old disc copy-protection (SafeDisc / SecuROM / StarForce)",
            "Windows 10/11 blocks the driver these use, so the game may refuse to start even with the disc inserted. The usual fix is the publisher's official no-CD patch or a re-release (e.g. GOG).",
            None,
        );
    } else if !needs_disc && (has_ci(&lower, "please insert") || has_ci(&lower, "insert the cd") || has_ci(&lower, "insert disc") || has_ci(&lower, "cd-rom")) {
        push(
            Level::Info,
            "May check for its disc",
            "The game mentions inserting its CD. If it asks for the disc, let ISObubble insert it automatically when you press Play.",
            Some(("Insert disc on Play", Fix::NeedsDisc)),
        );
    }

    // Program Files + old game = saving problems
    let pf = program.to_string_lossy().to_lowercase();
    if old && (pf.contains("\\program files\\") || pf.contains("\\program files (x86)\\")) {
        push(
            Level::Warn,
            "Installed in Program Files",
            "Old games try to save inside their own folder, which Windows protects there. Running as administrator usually fixes lost saves or settings.",
            Some(("Run as administrator", Fix::Layer("RUNASADMIN"))),
        );
    }
    if old {
        push(
            Level::Info,
            "Fullscreen tips for old games",
            "If the screen flickers or the game window is tiny/blurry, try “Disable fullscreen optimisations” and the high-DPI fix.",
            Some(("Apply both", Fix::Layer("DISABLEDXMAXIMIZEDWINDOWEDMODE HIGHDPIAWARE"))),
        );
    }

    // Space
    if let Some(sig) = info.as_ref().map(|i| i.signed) {
        if !sig {
            push(Level::Info, "Not digitally signed", "Normal for older games. Windows SmartScreen may ask before it starts the first time.", None);
        }
    }
    out.sort_by(|a, b| b.level.cmp(&a.level));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn checks_a_windows_program() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/game/Bin/MyGame.exe");
        let v = super::check_game(&p, p.parent().unwrap(), false);
        assert!(v.iter().any(|c| c.title.contains("64-bit")));
        let tmp = std::env::temp_dir().join("isobubble_dos_test.exe");
        std::fs::write(&tmp, b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff\x00\x00\xb8\x00\x00\x00\x00\x00\x00\x00\x40\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00").unwrap();
        let v = super::check_game(&tmp, &std::env::temp_dir(), false);
        assert!(v.iter().any(|c| c.title == "DOS program"), "{v:?}");
    }
}
