//! Safety scanner. Two layers:
//! 1. Our own checks on every file on the disc (known-bad fingerprints, disguised programs,
//!    dangerous script commands, trojan/ransomware behaviour inside programs, packers).
//! 2. The Microsoft Defender engine built into Windows, which has the full, daily-updated
//!    virus signature database.

use crate::iso::{Entry, Source};
use crate::pe;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Info,
    Low,
    Medium,
    High,
    Threat,
}

impl Level {
    pub fn label(&self) -> &'static str {
        match self {
            Level::Info => "Note",
            Level::Low => "Low",
            Level::Medium => "Suspicious",
            Level::High => "Dangerous",
            Level::Threat => "Malware",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub level: Level,
    pub file: String,
    pub what: String,
}

#[derive(Clone, Debug)]
pub enum DefenderResult {
    NotRun,
    Unavailable(String),
    Clean,
    Threats(Vec<String>),
    Error(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    Clean,
    Caution,
    Danger,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub findings: Vec<Finding>,
    pub defender: DefenderResult,
    pub fingerprints: Vec<(String, String)>,
    pub cancelled: bool,
}

impl Report {
    pub fn verdict(&self) -> Verdict {
        let worst = self.findings.iter().map(|f| f.level).max().unwrap_or(Level::Info);
        if matches!(self.defender, DefenderResult::Threats(_)) || worst >= Level::High {
            Verdict::Danger
        } else if worst == Level::Medium || self.cancelled {
            Verdict::Caution
        } else {
            Verdict::Clean
        }
    }
}

pub enum Msg {
    Status(String, f32),
    Found(Finding),
    Done(Report),
}

/// SHA-256 fingerprints of known malicious/test files.
const BLOCKLIST: &[(&str, &str)] = &[
    ("275a021bbfb6489e54d471899f7db9d1663fc695ec2fe2a2c4538aabf651fd0f", "EICAR anti-virus test file"),
    ("2546dcffc5ad854d4ddc64fbf056871cd5a00f2471cb7a5bfd4ac23b6e9eedad", "EICAR anti-virus test file (zipped)"),
];

const EXEC_EXT: &[&str] = &["exe", "com", "scr", "pif", "dll", "cpl", "ocx", "sys", "msi", "bat", "cmd", "ps1", "vbs", "vbe", "js", "jse", "wsf", "hta", "lnk", "jar"];
const SCRIPT_EXT: &[&str] = &["bat", "cmd", "ps1", "psm1", "vbs", "vbe", "js", "jse", "wsf", "hta", "inf", "reg", "lnk", "url"];
const DECOY_EXT: &[&str] = &["pdf", "doc", "docx", "xls", "xlsx", "jpg", "jpeg", "png", "gif", "txt", "mp3", "mp4", "avi", "zip", "rar", "htm", "html"];

const SCRIPT_RULES: &[(&str, Level, &str)] = &[
    ("-encodedcommand", Level::High, "Runs a hidden, scrambled PowerShell command"),
    (" -enc ", Level::High, "Runs a hidden, scrambled PowerShell command"),
    ("vssadmin delete shadows", Level::Threat, "Deletes Windows' backup copies — typical ransomware behaviour"),
    ("shadowcopy delete", Level::Threat, "Deletes Windows' backup copies — typical ransomware behaviour"),
    ("recoveryenabled no", Level::Threat, "Disables Windows recovery — typical ransomware behaviour"),
    ("set-mppreference -disable", Level::High, "Tries to switch off Microsoft Defender"),
    ("add-mppreference -exclusion", Level::High, "Tries to hide files from Microsoft Defender"),
    ("advfirewall set allprofiles state off", Level::High, "Turns off the Windows firewall"),
    ("disabletaskmgr", Level::High, "Blocks Task Manager so it can't be stopped"),
    ("net user /add", Level::High, "Creates a new user account"),
    ("localgroup administrators", Level::High, "Gives an account administrator rights"),
    ("downloadstring(", Level::Medium, "Downloads and runs code from the internet"),
    ("downloadfile(", Level::Medium, "Downloads files from the internet"),
    ("invoke-webrequest", Level::Medium, "Downloads files from the internet"),
    ("start-bitstransfer", Level::Medium, "Downloads files from the internet"),
    ("bitsadmin /transfer", Level::Medium, "Downloads files from the internet"),
    ("certutil -urlcache", Level::High, "Misuses a Windows tool to download files (common trojan trick)"),
    ("certutil.exe -urlcache", Level::High, "Misuses a Windows tool to download files (common trojan trick)"),
    ("invoke-expression", Level::Medium, "Builds and runs code on the fly"),
    ("iex(", Level::Medium, "Builds and runs code on the fly"),
    ("iex (", Level::Medium, "Builds and runs code on the fly"),
    ("-windowstyle hidden", Level::Medium, "Runs in an invisible window"),
    ("frombase64string", Level::Low, "Unpacks hidden (base64-encoded) data"),
    ("currentversion\\run", Level::Medium, "Makes something start automatically with Windows"),
    ("schtasks /create", Level::Medium, "Creates a scheduled task (can be used to keep malware running)"),
    ("mshta http", Level::High, "Runs a web page as a program"),
    ("regsvr32 /s /n /u /i:http", Level::High, "Runs code from the internet through regsvr32 (known attack trick)"),
    ("rundll32 javascript:", Level::High, "Runs script through rundll32 (known attack trick)"),
];

const PACKERS: &[(&str, Level, &str)] = &[
    ("UPX0", Level::Low, "Compressed with the UPX packer (common for small tools, also used to hide malware)"),
    (".aspack", Level::Medium, "Wrapped with the ASPack protector, which hides what the program does"),
    (".adata", Level::Low, "Wrapped with a program protector"),
    (".petite", Level::Medium, "Wrapped with the Petite packer, which hides what the program does"),
    (".themida", Level::Medium, "Protected with Themida (used by some games, also by malware to hide)"),
    (".vmp0", Level::Medium, "Protected with VMProtect (used by some games, also by malware to hide)"),
    ("MPRESS1", Level::Medium, "Packed with MPRESS, which hides what the program does"),
    (".enigma1", Level::Medium, "Protected with Enigma (used by some programs, also by malware to hide)"),
];

const RANSOM_TEXT: &[&str] = &[
    "your files have been encrypted",
    "your files are encrypted",
    "all your files are encrypted",
    "to decrypt your files",
    "send bitcoin",
    "bitcoin wallet",
    "your personal files are encrypted",
];

fn has(h: &[u8], n: &str) -> bool {
    pe::find(h, n.as_bytes()).is_some()
}

fn lower_ascii(b: &[u8]) -> Vec<u8> {
    b.iter().map(|c| c.to_ascii_lowercase()).collect()
}

fn utf16_to_ascii_lossy(b: &[u8]) -> Vec<u8> {
    // Many Windows scripts/shortcuts are UTF-16; fold "a\0b\0" into "ab".
    if b.len() > 4 && b.iter().skip(1).step_by(2).take(64).filter(|&&c| c == 0).count() > 24 {
        b.iter().step_by(2).copied().collect()
    } else {
        b.to_vec()
    }
}

pub fn scan_entry(src: &Source, e: &Entry, out: &mut Vec<Finding>) -> u64 {
    let name = e.name();
    let lname = name.to_ascii_lowercase();
    let ext = e.ext();
    let is_exec = EXEC_EXT.contains(&ext.as_str());
    let f = |level, what: &str| Finding { level, file: e.path.clone(), what: what.to_string() };

    if name.contains('\u{202E}') {
        out.push(f(Level::High, "File name uses a hidden right-to-left character to disguise its real type"));
    }
    let parts: Vec<&str> = lname.split('.').collect();
    if parts.len() >= 3 && is_exec && DECOY_EXT.contains(&parts[parts.len() - 2]) {
        out.push(f(Level::High, &format!("Disguised program: looks like a .{} file but is really a .{} program", parts[parts.len() - 2], ext)));
    }
    if lname.contains(".") && name.contains("          ") && is_exec {
        out.push(f(Level::Medium, "File name is padded with spaces to hide its real ending"));
    }
    if matches!(ext.as_str(), "scr" | "pif") {
        out.push(f(Level::Medium, "Screensaver/PIF program files are a classic way to sneak in malware"));
    }
    if is_exec && e.hidden {
        out.push(f(Level::Low, "Program file is marked hidden on the disc"));
    }

    let interesting = is_exec || SCRIPT_EXT.contains(&ext.as_str()) || e.size <= 8 * 1024 * 1024;
    if !interesting || e.size == 0 {
        return 0;
    }
    let Ok(data) = src.read_file(e, 64 * 1024 * 1024) else {
        out.push(f(Level::Info, "Couldn't be read for scanning"));
        return 0;
    };
    let n = data.len() as u64;

    if has(&data, "EICAR-STANDARD-ANTIVIRUS-TEST-FILE") {
        out.push(f(Level::Threat, "Matches the EICAR anti-virus test signature (a harmless test file that every scanner treats as a virus)"));
    }
    if e.size <= 64 * 1024 * 1024 {
        let h = format!("{:x}", Sha256::digest(&data));
        if let Some((_, what)) = BLOCKLIST.iter().find(|(x, _)| *x == h) {
            out.push(f(Level::Threat, &format!("Known malicious fingerprint: {what}")));
        }
    }

    if SCRIPT_EXT.contains(&ext.as_str()) {
        let text = lower_ascii(&utf16_to_ascii_lossy(&data));
        let mut seen = Vec::new();
        for (pat, lvl, what) in SCRIPT_RULES {
            if has(&text, pat) && !seen.contains(what) {
                seen.push(*what);
                out.push(f(*lvl, what));
            }
        }
        if lname == "autorun.inf" {
            let t = String::from_utf8_lossy(&text).to_string();
            for line in t.lines() {
                let l = line.trim();
                if (l.starts_with("open=") || l.starts_with("shellexecute=") || l.contains("\\command=")) && [".vbs", ".js", ".bat", ".cmd", ".ps1", ".scr", ".hta", "cmd.exe", "powershell", "wscript", "mshta"].iter().any(|x| l.contains(x)) {
                    out.push(f(Level::High, "Autorun starts a script or command shell instead of a normal setup program"));
                    break;
                }
            }
        }
        if ext == "lnk" && (has(&text, "powershell") || has(&text, "cmd.exe") || has(&text, "mshta")) {
            out.push(f(Level::High, "Shortcut secretly runs a command shell or PowerShell"));
        }
    }

    if data.len() > 64 && &data[0..2] == b"MZ" {
        let ok_carrier = matches!(
            ext.as_str(),
            "exe" | "dll" | "sys" | "ocx" | "cpl" | "drv" | "ax" | "mui" | "bin" | "dat" | "com" | "scr" | "efi" | "tlb" | "acm" | "tmp" | "mod" | "flt" | "ime" | "rll" | "pyd" | "node" | "asi" | "" | "_ex" | "ex_" | "dl_" | "x86" | "x64" | "api" | "aex" | "8bi" | "plg" | "vst" | "xll"
        );
        if !ok_carrier && DECOY_EXT.contains(&ext.as_str()) {
            out.push(f(Level::High, &format!("A program is hiding inside a file pretending to be .{ext}")));
        }
        if let Some(info) = pe::parse(&data) {
            for s in &info.sections {
                if let Some((_, lvl, what)) = PACKERS.iter().find(|(p, _, _)| s.name.eq_ignore_ascii_case(p)) {
                    out.push(f(*lvl, what));
                }
            }
            if info.sections.iter().any(|s| s.executable && s.entropy > 7.6 && s.raw_size > 64 * 1024) {
                out.push(f(Level::Low, "Program code is heavily compressed or encrypted (can hide what it does)"));
            }
            let region = &data[..data.len().min(16 * 1024 * 1024)];
            let inject = has(region, "VirtualAllocEx") && has(region, "WriteProcessMemory") && has(region, "CreateRemoteThread");
            if inject {
                out.push(f(
                    if info.signed { Level::Low } else { Level::Medium },
                    "Can inject code into other running programs (used by trojans; also by some game tools)",
                ));
            }
            if (has(region, "SetWindowsHookExA") || has(region, "SetWindowsHookExW")) && has(region, "GetAsyncKeyState") && !info.signed {
                out.push(f(Level::Medium, "Watches everything typed on the keyboard (keylogger-like behaviour)"));
            }
            if has(region, "URLDownloadToFile") && !info.signed && !lname.contains("setup") && !lname.contains("install") {
                out.push(f(Level::Low, "Can download files from the internet"));
            }
            let low = lower_ascii(region);
            if RANSOM_TEXT.iter().any(|t| has(&low, t)) {
                out.push(f(Level::High, "Contains ransom-note text (asks for payment to unlock files)"));
            }
            if (ext == "exe" || ext == "scr") && e.depth() == 0 && !info.signed && info.gui && lname.starts_with("autorun") {
                out.push(f(Level::Info, "Autorun program is not digitally signed"));
            }
        }
    } else if !is_exec && DECOY_EXT.contains(&ext.as_str()) {
        if let Some(p) = pe::find(&data, b"This program cannot be run in DOS mode") {
            if p > 512 {
                out.push(f(Level::Medium, &format!("A hidden program is embedded inside this .{ext} file")));
            }
        }
    }
    n
}

pub fn find_defender() -> Option<PathBuf> {
    let pd = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    let plat = Path::new(&pd).join("Microsoft\\Windows Defender\\Platform");
    if let Ok(rd) = std::fs::read_dir(&plat) {
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|d| d.path()).filter(|p| p.join("MpCmdRun.exe").exists()).collect();
        dirs.sort();
        if let Some(d) = dirs.pop() {
            return Some(d.join("MpCmdRun.exe"));
        }
    }
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
    let p = Path::new(&pf).join("Windows Defender\\MpCmdRun.exe");
    p.exists().then_some(p)
}

pub fn defender_scan(target: &Path) -> DefenderResult {
    let Some(exe) = find_defender() else {
        return DefenderResult::Unavailable("Microsoft Defender's scanner wasn't found on this PC.".into());
    };
    let mut c = std::process::Command::new(exe);
    c.args(["-Scan", "-ScanType", "3", "-File"]).arg(target).arg("-DisableRemediation");
    crate::system::hide_window(&mut c);
    match c.output() {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout).replace('\0', "");
            let failed = text.contains("Failed with hr") || text.contains("CmdTool: Failed");
            let detected = text.contains("LIST OF DETECTED THREATS") || text.lines().any(|l| l.trim_start().starts_with("Threat ") && l.contains(':'));
            match o.status.code() {
                Some(0) => DefenderResult::Clean,
                Some(2) if failed && !detected => {
                    let hr = text.split("hr = ").nth(1).and_then(|s| s.split_whitespace().next()).unwrap_or("?").trim_end_matches('.').to_string();
                    DefenderResult::Error(format!("Defender couldn't read this file in place (error {hr})."))
                }
                Some(2) => {
                    let lines: Vec<String> = text
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| l.contains("Threat") || l.contains("Resources") || l.starts_with("file:") || l.starts_with("containerfile:"))
                        .take(20)
                        .collect();
                    DefenderResult::Threats(if lines.is_empty() { vec!["Threats were found.".into()] } else { lines })
                }
                code => {
                    let last: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
                    let tail = last.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join(" ");
                    DefenderResult::Error(format!(
                        "Defender couldn't finish (code {}). It may be switched off or another antivirus is in charge — scan with that one too. {}",
                        code.map(|c| c.to_string()).unwrap_or("?".into()),
                        tail
                    ))
                }
            }
        }
        Err(e) => DefenderResult::Error(format!("Couldn't start Defender: {e}")),
    }
}

/// Fingerprints, scans every file and runs Defender. Sends progress through `send`.
pub fn run(sources: Vec<Arc<Source>>, use_defender: bool, cancel: Arc<AtomicBool>, send: impl Fn(Msg)) {
    let total_files: usize = sources.iter().map(|s| s.files().count()).sum::<usize>().max(1);
    let mut report = Report {
        files_scanned: 0,
        bytes_scanned: 0,
        findings: Vec::new(),
        defender: DefenderResult::NotRun,
        fingerprints: Vec::new(),
        cancelled: false,
    };
    let weight_hash = 0.25f32;
    let weight_files = if use_defender { 0.45 } else { 0.75 };

    // 1. Whole-image fingerprint
    for (i, s) in sources.iter().enumerate() {
        if s.kind == crate::iso::SourceKind::Folder {
            continue;
        }
        let Ok(mut f) = std::fs::File::open(&s.path) else { continue };
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 4 << 20];
        let mut done = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                break;
            }
            use std::io::Read;
            let Ok(n) = f.read(&mut buf) else { break };
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            done += n as u64;
            let frac = (i as f32 + done as f32 / s.file_size.max(1) as f32) / sources.len() as f32;
            send(Msg::Status(format!("Taking the disc's fingerprint… {}", crate::iso::fmt_size(done)), frac * weight_hash));
        }
        let hex = format!("{:x}", h.finalize());
        if let Some((_, what)) = BLOCKLIST.iter().find(|(x, _)| *x == hex) {
            let fnd = Finding { level: Level::Threat, file: s.info.label.clone(), what: format!("Known malicious fingerprint: {what}") };
            send(Msg::Found(fnd.clone()));
            report.findings.push(fnd);
        }
        report.fingerprints.push((s.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), hex));
    }

    // 2. Every file on the disc
    'outer: for s in &sources {
        for e in s.files() {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                break 'outer;
            }
            report.files_scanned += 1;
            if report.files_scanned % 7 == 1 || e.size > 4 << 20 {
                let frac = weight_hash + weight_files * report.files_scanned as f32 / total_files as f32;
                send(Msg::Status(format!("Checking {}", e.path.replace('/', "\\")), frac));
            }
            let mut found = Vec::new();
            report.bytes_scanned += scan_entry(s, e, &mut found);
            for fnd in found {
                send(Msg::Found(fnd.clone()));
                report.findings.push(fnd);
            }
        }
    }

    // 3. Microsoft Defender
    if use_defender && !report.cancelled {
        send(Msg::Status("Asking Microsoft Defender to scan the whole disc image…".into(), weight_hash + weight_files));
        let mut threats = Vec::new();
        let mut result = DefenderResult::Clean;
        for s in &sources {
            let target = match s.mounted_letter {
                Some(l) => PathBuf::from(format!("{l}:\\")),
                None => s.path.clone(),
            };
            let mut r = defender_scan(&target);
            // Fallback: if Defender can't read the image file where it is, put the disc in a
            // virtual drive (read-only, nothing runs) and let Defender scan the drive instead.
            if matches!(r, DefenderResult::Error(_)) && s.mounted_letter.is_none() && s.kind == crate::iso::SourceKind::DiscImage {
                send(Msg::Status("Defender is scanning the disc through a virtual drive…".into(), weight_hash + weight_files + 0.1));
                let was_in_drive = crate::system::is_mounted(&s.path);
                if let Ok(l) = crate::system::mount_iso(&s.path) {
                    let r2 = defender_scan(Path::new(&format!("{l}:\\")));
                    if !was_in_drive {
                        let _ = crate::system::dismount_iso(&s.path);
                    }
                    if !matches!(r2, DefenderResult::Error(_)) {
                        r = r2;
                    }
                }
            }
            match r {
                DefenderResult::Threats(t) => threats.extend(t),
                DefenderResult::Clean => {}
                other => {
                    result = other;
                    break;
                }
            }
        }
        if !threats.is_empty() {
            result = DefenderResult::Threats(threats);
        }
        report.defender = result;
    } else if !use_defender {
        report.defender = DefenderResult::Unavailable("Defender scan was switched off.".into());
    }
    send(Msg::Status("Scan finished".into(), 1.0));
    send(Msg::Done(report));
}
