//! Turns the list of files on a disc into a plain-English description:
//! what kind of disc it is, what will be installed and which program starts the setup.

use crate::iso::{fmt_size, Source};
use crate::pe;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiscKind {
    Software,
    Game,
    WindowsSetup,
    Linux,
    DvdVideo,
    Media,
    Data,
    NotADisc,
}

impl DiscKind {
    pub fn label(&self) -> &'static str {
        match self {
            DiscKind::Software => "Software installer",
            DiscKind::Game => "Game installer",
            DiscKind::WindowsSetup => "Windows setup disc",
            DiscKind::Linux => "Linux disc",
            DiscKind::DvdVideo => "Video disc",
            DiscKind::Media => "Music / media disc",
            DiscKind::Data => "Data disc",
            DiscKind::NotADisc => "Single file",
        }
    }
    pub fn icon(&self) -> &'static str {
        match self {
            DiscKind::Software => "📦",
            DiscKind::Game => "🎮",
            DiscKind::WindowsSetup => "🪟",
            DiscKind::Linux => "🐧",
            DiscKind::DvdVideo => "🎬",
            DiscKind::Media => "🎵",
            DiscKind::Data => "🗂",
            DiscKind::NotADisc => "📄",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Installer {
    pub path: String,
    pub score: i32,
    pub is_msi: bool,
    pub product: Option<String>,
    pub company: Option<String>,
    pub version: Option<String>,
    pub file_desc: Option<String>,
    pub arch: Option<String>,
    pub signed: Option<bool>,
}

impl Installer {
    pub fn title(&self) -> String {
        let mut s = self.path.replace('/', "\\");
        if let Some(p) = self.product.as_ref().or(self.file_desc.as_ref()) {
            s = format!("{s}  —  {p}");
        }
        s
    }
}

#[derive(Clone, Debug)]
pub struct Description {
    pub kind: DiscKind,
    pub headline: String,
    pub sentences: Vec<String>,
    pub facts: Vec<(String, String)>,
    pub categories: Vec<(String, usize, u64)>,
    pub installers: Vec<Installer>,
    pub autorun: Vec<(String, String)>,
    pub top_level: Vec<(String, bool, u64)>,
    pub file_count: usize,
    pub folder_count: usize,
}

fn category(ext: &str) -> &'static str {
    match ext {
        "exe" | "msi" | "com" | "dll" | "sys" | "ocx" | "cpl" | "scr" | "msp" | "drv" => "Programs & system files",
        "bat" | "cmd" | "ps1" | "vbs" | "vbe" | "js" | "jse" | "wsf" | "hta" | "inf" | "reg" | "lnk" => "Scripts & setup instructions",
        "cab" | "zip" | "7z" | "rar" | "wim" | "esd" | "swm" | "gz" | "xz" | "squashfs" | "tar" | "msu" | "z" | "hdr" => "Compressed setup data",
        "pak" | "bik" | "bnk" | "wad" | "upk" | "pck" | "xnb" | "gcf" | "bsa" | "ba2" | "vpk" | "arc" | "big" | "dat" | "bin" | "sav" => "Game / program data",
        "txt" | "pdf" | "rtf" | "htm" | "html" | "doc" | "docx" | "chm" | "md" | "nfo" | "ini" | "xml" | "json" | "cfg" => "Documents & settings",
        "mp3" | "wav" | "ogg" | "flac" | "wma" | "m4a" | "aac" | "cda" => "Music & sound",
        "avi" | "mp4" | "mkv" | "mpg" | "mpeg" | "vob" | "ifo" | "bup" | "wmv" | "mov" | "m2ts" | "webm" => "Video",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "ico" | "tga" | "dds" | "svg" | "webp" => "Pictures & icons",
        "ttf" | "otf" | "fon" => "Fonts",
        _ => "Other files",
    }
}

fn parse_autorun(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_section = l.eq_ignore_ascii_case("[autorun]");
            continue;
        }
        if !in_section || l.starts_with(';') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            out.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    out
}

pub fn describe(src: &Source) -> Description {
    let files: Vec<_> = src.files().collect();
    let file_count = files.len();
    let folder_count = src.entries.len() - file_count;
    let total = src.total_bytes();
    let lower: Vec<String> = src.entries.iter().map(|e| e.path.to_ascii_lowercase()).collect();
    let has = |p: &str| lower.iter().any(|l| l == p);
    let has_prefix = |p: &str| lower.iter().any(|l| l.starts_with(p));
    let has_ext = |exts: &[&str]| files.iter().any(|e| exts.contains(&e.ext().as_str()));

    // File categories
    let mut cats: Vec<(String, usize, u64)> = Vec::new();
    for e in &files {
        let c = category(&e.ext());
        match cats.iter_mut().find(|x| x.0 == c) {
            Some(x) => {
                x.1 += 1;
                x.2 += e.size;
            }
            None => cats.push((c.to_string(), 1, e.size)),
        }
    }
    cats.sort_by(|a, b| b.2.cmp(&a.2));

    // Top-level items with their total sizes
    let mut top: Vec<(String, bool, u64)> = Vec::new();
    for e in &src.entries {
        let first = e.path.split('/').next().unwrap_or("").to_string();
        let is_dir = e.depth() > 0 || e.is_dir;
        match top.iter_mut().find(|t| t.0 == first) {
            Some(t) => t.2 += e.size,
            None => top.push((first, is_dir, e.size)),
        }
    }
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));

    // autorun.inf
    let autorun = src
        .entries
        .iter()
        .find(|e| !e.is_dir && e.depth() == 0 && e.name().eq_ignore_ascii_case("autorun.inf"))
        .and_then(|e| src.read_file(e, 64 * 1024).ok())
        .map(|b| parse_autorun(&String::from_utf8_lossy(&b)))
        .unwrap_or_default();
    let autorun_open = autorun
        .iter()
        .find(|(k, _)| k == "open" || k == "shellexecute")
        .map(|(_, v)| {
            v.split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .replace('\\', "/")
                .trim_start_matches('/')
                .to_ascii_lowercase()
        });

    // Installer candidates
    let mut installers: Vec<Installer> = Vec::new();
    for e in &files {
        let ext = e.ext();
        if ext != "exe" && ext != "msi" {
            continue;
        }
        let d = e.depth() as i32;
        if d > 2 {
            continue;
        }
        let n = e.name().to_ascii_lowercase();
        let mut score = match n.as_str() {
            "setup.exe" => 100,
            "install.exe" | "installer.exe" | "setup64.exe" | "setup_x64.exe" => 90,
            "autorun.exe" | "autoplay.exe" | "launcher.exe" | "start.exe" => 60,
            _ if ext == "msi" && d <= 1 => 70,
            _ if n.contains("setup") || n.contains("install") => 55,
            _ if d == 0 => 40,
            _ => 10,
        };
        if n.contains("unins") || n.starts_with("vcredist") || n.starts_with("dxsetup") || n.contains("redist") || n.starts_with("dotnet") {
            score = 3;
        }
        score -= d * 12;
        if autorun_open.as_deref() == Some(e.path.to_ascii_lowercase().as_str()) {
            score += 50;
        }
        installers.push(Installer {
            path: e.path.clone(),
            score,
            is_msi: ext == "msi",
            product: None,
            company: None,
            version: None,
            file_desc: None,
            arch: None,
            signed: None,
        });
    }
    installers.sort_by(|a, b| b.score.cmp(&a.score));
    installers.truncate(8);
    for (i, inst) in installers.iter_mut().enumerate() {
        if i >= 3 || inst.is_msi {
            continue;
        }
        if let Some(e) = src.entries.iter().find(|e| e.path == inst.path) {
            if let Ok(bytes) = src.read_file(e, 48 * 1024 * 1024) {
                if let Some(info) = pe::parse(&bytes) {
                    inst.product = info.version.get("ProductName").cloned();
                    inst.company = info.version.get("CompanyName").cloned();
                    inst.version = info
                        .version
                        .get("ProductVersion")
                        .or(info.version.get("FileVersion"))
                        .cloned();
                    inst.file_desc = info.version.get("FileDescription").cloned();
                    inst.arch = Some(info.machine.to_string());
                    inst.signed = Some(info.signed);
                }
            }
        }
    }

    // What kind of disc is this?
    let kind = if src.kind == crate::iso::SourceKind::SingleFile {
        DiscKind::NotADisc
    } else if has("sources/install.wim") || has("sources/install.esd") || has("sources/install.swm") || (has("bootmgr") && has("sources/boot.wim")) {
        DiscKind::WindowsSetup
    } else if has_prefix("casper/") || has("live/filesystem.squashfs") || has_ext(&["squashfs"]) || (has_prefix("isolinux/") && !has_ext(&["exe", "msi"])) {
        DiscKind::Linux
    } else if has_prefix("video_ts/") || has_prefix("bdmv/") {
        DiscKind::DvdVideo
    } else if !installers.is_empty() && installers[0].score >= 30 {
        let gamey = has_ext(&["pak", "bik", "bnk", "wad", "upk", "pck", "xnb", "bsa", "ba2", "vpk", "big"])
            || lower.iter().any(|l| l.contains("directx") || l.contains("dxsetup") || l.contains("physx") || l.contains("punkbuster"));
        if gamey {
            DiscKind::Game
        } else {
            DiscKind::Software
        }
    } else if !files.is_empty()
        && files.iter().all(|e| {
            let c = category(&e.ext());
            c == "Music & sound" || c == "Video" || c == "Pictures & icons" || c == "Documents & settings"
        })
    {
        DiscKind::Media
    } else {
        DiscKind::Data
    };

    let label = if src.info.label.trim().is_empty() {
        "Untitled disc".to_string()
    } else {
        src.info.label.trim().to_string()
    };
    let main = installers.first().filter(|i| i.score >= 30);
    let product = main
        .and_then(|i| i.product.clone().or(i.file_desc.clone()))
        .filter(|p| !p.to_ascii_lowercase().contains("setup launcher") && p.len() < 80);
    let company = main.and_then(|i| i.company.clone());

    let headline = match (kind, &product) {
        (DiscKind::Software | DiscKind::Game, Some(p)) => match &company {
            Some(c) => format!("{} for “{}” by {}", kind.label(), p, c),
            None => format!("{} for “{}”", kind.label(), p),
        },
        _ => format!("{} — “{}”", kind.label(), label),
    };

    let mut s = Vec::new();
    s.push(format!(
        "This disc holds {} file{} in {} folder{}, {} in total.",
        file_count,
        if file_count == 1 { "" } else { "s" },
        folder_count,
        if folder_count == 1 { "" } else { "s" },
        fmt_size(total)
    ));
    match kind {
        DiscKind::Software | DiscKind::Game => {
            let what = if kind == DiscKind::Game { "a game" } else { "a program" };
            if let Some(m) = main {
                let mut line = format!(
                    "It is an installation disc for {what}. Installing will start “{}”",
                    m.path.replace('/', "\\")
                );
                if let Some(p) = &product {
                    line.push_str(&format!(", which installs {p}"));
                }
                if let Some(v) = &m.version {
                    line.push_str(&format!(" (version {v})"));
                }
                line.push('.');
                s.push(line);
                match m.signed {
                    Some(true) => s.push(format!(
                        "The installer carries a digital signature{} — a good sign it hasn't been tampered with.",
                        company.as_ref().map(|c| format!(" (publisher listed as {c})")).unwrap_or_default()
                    )),
                    Some(false) => s.push("The installer is not digitally signed. That is common on older discs, but means its publisher can't be confirmed.".into()),
                    None => {}
                }
                if let Some(a) = &m.arch {
                    s.push(format!("The setup program is built for {a} Windows."));
                }
            }
        }
        DiscKind::WindowsSetup => {
            s.push("This is Windows operating-system setup media. Running its setup upgrades or reinstalls Windows on this PC — for a clean install it is normally written to a USB stick and booted instead.".into());
        }
        DiscKind::Linux => {
            s.push("This is a Linux system disc. It is meant to be written to a USB stick or booted in a virtual machine; it can't be installed from inside Windows, but its files can be copied out.".into());
        }
        DiscKind::DvdVideo => s.push("It contains a video disc layout (DVD/Blu-ray). Nothing needs installing; open it in a media player.".into()),
        DiscKind::Media => s.push("It only contains media and documents. Nothing will be installed; the files can be copied to a folder.".into()),
        DiscKind::Data => s.push("No setup program was found, so it looks like a plain data disc. Its files can be copied to a folder and opened from there.".into()),
        DiscKind::NotADisc => s.push("This is not a disc image, just a single file.".into()),
    }
    if !autorun.is_empty() {
        if let Some((_, v)) = autorun.iter().find(|(k, _)| k == "open" || k == "shellexecute") {
            s.push(format!("When the real disc is inserted, Windows' autoplay would run “{v}”."));
        }
    }
    if src.info.bootable {
        s.push("The disc is bootable — a computer could start from it directly.".into());
    }
    if let Some(top_cat) = cats.first() {
        s.push(format!(
            "Most of the space is {} ({}).",
            top_cat.0.to_lowercase(),
            fmt_size(top_cat.2)
        ));
    }

    let mut facts = vec![("Disc label".to_string(), label)];
    if let Some(p) = &product {
        facts.push(("Product".into(), p.clone()));
    }
    if let Some(c) = &company {
        facts.push(("Publisher".into(), c.clone()));
    } else if !src.info.publisher.is_empty() {
        facts.push(("Publisher".into(), src.info.publisher.clone()));
    }
    if let Some(v) = main.and_then(|m| m.version.clone()) {
        facts.push(("Version".into(), v));
    }
    if !src.info.created.is_empty() {
        facts.push(("Disc made on".into(), src.info.created.clone()));
    }
    if !src.info.application.is_empty() {
        facts.push(("Made with".into(), src.info.application.clone()));
    }
    let fmt = match (src.info.iso9660, src.info.joliet, src.info.udf) {
        (_, _, true) if !src.info.iso9660 => "UDF",
        (true, true, true) => "ISO 9660 + Joliet + UDF",
        (true, true, false) => "ISO 9660 + Joliet",
        (true, false, true) => "ISO 9660 + UDF",
        (true, false, false) => "ISO 9660",
        _ => "Folder",
    };
    facts.push(("Disc format".into(), fmt.into()));
    facts.push(("Image size".into(), fmt_size(src.file_size)));
    facts.push(("Bootable".into(), if src.info.bootable { "Yes" } else { "No" }.into()));

    Description {
        kind,
        headline,
        sentences: s,
        facts,
        categories: cats,
        installers,
        autorun,
        top_level: top,
        file_count,
        folder_count,
    }
}
