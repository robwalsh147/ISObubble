//! Reads disc images (ISO 9660 + Joliet) without mounting them, and can also
//! treat a folder or a mounted drive as a "disc" so the rest of the app works the same way.

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const SECTOR: u64 = 2048;
const MAX_ENTRIES: usize = 300_000;

#[derive(Clone, Debug)]
pub struct Entry {
    /// Path inside the disc, with forward slashes ("FOLDER/FILE.EXE").
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub lba: u32,
    pub hidden: bool,
    /// Set when the entry lives on a real drive/folder instead of inside an image file.
    pub disk_path: Option<PathBuf>,
}

impl Entry {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
    pub fn ext(&self) -> String {
        let n = self.name();
        match n.rfind('.') {
            Some(i) if i > 0 => n[i + 1..].to_ascii_lowercase(),
            _ => String::new(),
        }
    }
    pub fn depth(&self) -> usize {
        self.path.matches('/').count()
    }
}

#[derive(Clone, Debug, Default)]
pub struct VolumeInfo {
    pub label: String,
    pub system: String,
    pub publisher: String,
    pub preparer: String,
    pub application: String,
    pub created: String,
    pub bootable: bool,
    pub joliet: bool,
    pub udf: bool,
    pub iso9660: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    DiscImage,
    Folder,
    SingleFile,
}

#[derive(Debug)]
pub struct Source {
    pub path: PathBuf,
    pub kind: SourceKind,
    pub file_size: u64,
    pub info: VolumeInfo,
    pub entries: Vec<Entry>,
    /// Drive letter if the image had to be mounted to be read (UDF-only discs).
    pub mounted_letter: Option<char>,
}

fn read_at(f: &mut File, off: u64, len: usize) -> std::io::Result<Vec<u8>> {
    f.seek(SeekFrom::Start(off))?;
    let mut buf = vec![0u8; len];
    let mut got = 0;
    while got < len {
        let n = f.read(&mut buf[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    buf.truncate(got);
    Ok(buf)
}

fn ascii_field(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .trim_matches(|c: char| c == ' ' || c == '\0')
        .to_string()
}

fn utf16be_field(b: &[u8]) -> String {
    let u: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&u)
        .trim_matches(|c: char| c == ' ' || c == '\0')
        .to_string()
}

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn fmt_date(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b).to_string();
    if s.len() >= 8 && s[..8].chars().all(|c| c.is_ascii_digit()) && &s[..4] != "0000" {
        format!("{}-{}-{}", &s[..4], &s[4..6], &s[6..8])
    } else {
        String::new()
    }
}

pub fn is_disc_image_name(p: &Path) -> bool {
    matches!(
        p.extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .as_deref(),
        Some("iso") | Some("img") | Some("udf")
    )
}

impl Source {
    pub fn open(path: &Path) -> Result<Source, String> {
        let meta = std::fs::metadata(path).map_err(|e| format!("Couldn't open the file: {e}"))?;
        if meta.is_dir() {
            return Ok(Self::from_folder(path, None));
        }
        let mut f = File::open(path).map_err(|e| format!("Couldn't open the file: {e}"))?;
        let mut info = VolumeInfo::default();
        let mut pvd_root: Option<(u32, u32)> = None;
        let mut joliet_root: Option<(u32, u32)> = None;

        for s in 16..96u64 {
            let d = read_at(&mut f, s * SECTOR, SECTOR as usize).map_err(|e| e.to_string())?;
            if d.len() < SECTOR as usize {
                break;
            }
            let id = &d[1..6];
            if id == b"CD001" {
                match d[0] {
                    0 => {
                        if d[7..30].starts_with(b"EL TORITO") {
                            info.bootable = true;
                        }
                    }
                    1 => {
                        info.iso9660 = true;
                        info.system = ascii_field(&d[8..40]);
                        info.label = ascii_field(&d[40..72]);
                        info.publisher = ascii_field(&d[318..446]);
                        info.preparer = ascii_field(&d[446..574]);
                        info.application = ascii_field(&d[574..702]);
                        info.created = fmt_date(&d[813..830]);
                        let root = &d[156..190];
                        pvd_root = Some((le32(root, 2), le32(root, 10)));
                    }
                    2 => {
                        let esc = &d[88..91];
                        if esc[0] == 0x25 && esc[1] == 0x2F && matches!(esc[2], 0x40 | 0x43 | 0x45) {
                            info.joliet = true;
                            let root = &d[156..190];
                            joliet_root = Some((le32(root, 2), le32(root, 10)));
                            let l = utf16be_field(&d[40..72]);
                            if !l.is_empty() {
                                info.label = l;
                            }
                            let p = utf16be_field(&d[318..446]);
                            if !p.is_empty() {
                                info.publisher = p;
                            }
                            let a = utf16be_field(&d[574..702]);
                            if !a.is_empty() {
                                info.application = a;
                            }
                        }
                    }
                    _ => {}
                }
            } else if id == b"BEA01" || id == b"NSR02" || id == b"NSR03" {
                if id != b"BEA01" {
                    info.udf = true;
                }
            } else if id == b"TEA01" {
                break;
            } else {
                break;
            }
        }

        if !info.iso9660 && !info.udf {
            // Not a disc image: treat as a single file (used by the scanner page).
            return Ok(Source {
                path: path.to_path_buf(),
                kind: SourceKind::SingleFile,
                file_size: meta.len(),
                info: VolumeInfo {
                    label: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    ..Default::default()
                },
                entries: vec![Entry {
                    path: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    is_dir: false,
                    size: meta.len(),
                    lba: 0,
                    hidden: false,
                    disk_path: Some(path.to_path_buf()),
                }],
                mounted_letter: None,
            });
        }

        let mut entries = Vec::new();
        if let Some((lba, size)) = joliet_root.or(pvd_root) {
            let mut seen = HashSet::new();
            walk(&mut f, lba, size, joliet_root.is_some(), "", 0, &mut entries, &mut seen);
        }
        if info.label.is_empty() {
            info.label = path
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
        }
        Ok(Source {
            path: path.to_path_buf(),
            kind: SourceKind::DiscImage,
            file_size: meta.len(),
            info,
            entries,
            mounted_letter: None,
        })
    }

    /// True for discs whose files could only be listed after mounting (UDF-only images).
    pub fn needs_mount_listing(&self) -> bool {
        self.kind == SourceKind::DiscImage && self.entries.is_empty() && self.info.udf
    }

    /// Lists a UDF-only disc through a mounted drive letter.
    pub fn fill_from_drive(&mut self, letter: char) {
        let root = PathBuf::from(format!("{letter}:\\"));
        let mut entries = Vec::new();
        walk_folder(&root, "", 0, &mut entries);
        self.entries = entries;
        self.mounted_letter = Some(letter);
    }

    pub fn from_folder(path: &Path, letter: Option<char>) -> Source {
        let mut entries = Vec::new();
        walk_folder(path, "", 0, &mut entries);
        let size = entries.iter().map(|e| e.size).sum();
        Source {
            path: path.to_path_buf(),
            kind: SourceKind::Folder,
            file_size: size,
            info: VolumeInfo {
                label: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string()),
                ..Default::default()
            },
            entries,
            mounted_letter: letter,
        }
    }

    pub fn files(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| !e.is_dir)
    }

    pub fn total_bytes(&self) -> u64 {
        self.files().map(|e| e.size).sum()
    }

    pub fn find(&self, path_lower: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.path.to_ascii_lowercase() == path_lower)
    }

    /// Reads up to `max` bytes of a file on the disc.
    pub fn read_file(&self, e: &Entry, max: u64) -> std::io::Result<Vec<u8>> {
        let want = e.size.min(max) as usize;
        if let Some(p) = &e.disk_path {
            let mut f = File::open(p)?;
            return read_at(&mut f, 0, want);
        }
        let mut f = File::open(&self.path)?;
        read_at(&mut f, e.lba as u64 * SECTOR, want)
    }

    /// Copies one file off the disc, calling `on_bytes` as data is written.
    pub fn copy_out(&self, e: &Entry, dest: &Path, on_bytes: &mut dyn FnMut(u64)) -> std::io::Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(dest)?;
        let mut src = match &e.disk_path {
            Some(p) => File::open(p)?,
            None => {
                let mut f = File::open(&self.path)?;
                f.seek(SeekFrom::Start(e.lba as u64 * SECTOR))?;
                f
            }
        };
        let mut left = e.size;
        let mut buf = vec![0u8; 1 << 20];
        while left > 0 {
            let n = (left as usize).min(buf.len());
            let got = src.read(&mut buf[..n])?;
            if got == 0 {
                break;
            }
            out.write_all(&buf[..got])?;
            left -= got as u64;
            on_bytes(got as u64);
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn walk(
    f: &mut File,
    lba: u32,
    size: u32,
    joliet: bool,
    prefix: &str,
    depth: usize,
    out: &mut Vec<Entry>,
    seen: &mut HashSet<u32>,
) {
    if depth > 48 || !seen.insert(lba) || out.len() > MAX_ENTRIES {
        return;
    }
    let size = size.min(16 * 1024 * 1024);
    let Ok(buf) = read_at(f, lba as u64 * SECTOR, size as usize) else {
        return;
    };
    let mut pos = 0usize;
    let mut subdirs = Vec::new();
    while pos < buf.len() {
        let len = buf[pos] as usize;
        if len == 0 {
            pos = (pos / SECTOR as usize + 1) * SECTOR as usize;
            continue;
        }
        if pos + len > buf.len() || len < 34 {
            break;
        }
        let r = &buf[pos..pos + len];
        let extent = le32(r, 2);
        let dlen = le32(r, 10);
        let flags = r[25];
        let nl = r[32] as usize;
        if 33 + nl <= len {
            let raw = &r[33..33 + nl];
            if !(nl == 1 && (raw[0] == 0 || raw[0] == 1)) {
                let mut name = if joliet {
                    utf16be_field(raw)
                } else {
                    String::from_utf8_lossy(raw).to_string()
                };
                if let Some(i) = name.find(';') {
                    name.truncate(i);
                }
                if name.ends_with('.') {
                    name.pop();
                }
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                let is_dir = flags & 2 != 0;
                let merged = match out.last_mut() {
                    Some(last) if !is_dir && !last.is_dir && last.path == path => {
                        last.size += dlen as u64; // multi-extent file (> 4 GB)
                        true
                    }
                    _ => false,
                };
                if !merged {
                    out.push(Entry {
                        path: path.clone(),
                        is_dir,
                        size: if is_dir { 0 } else { dlen as u64 },
                        lba: extent,
                        hidden: flags & 1 != 0,
                        disk_path: None,
                    });
                    if is_dir {
                        subdirs.push((extent, dlen, path));
                    }
                }
            }
        }
        pos += len;
    }
    for (e, l, p) in subdirs {
        walk(f, e, l, joliet, &p, depth + 1, out, seen);
    }
}

fn walk_folder(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<Entry>) {
    if depth > 48 || out.len() > MAX_ENTRIES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut items: Vec<_> = rd.flatten().collect();
    items.sort_by_key(|d| d.file_name());
    let mut subdirs = Vec::new();
    for d in items {
        let name = d.file_name().to_string_lossy().to_string();
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        let Ok(md) = d.metadata() else { continue };
        if md.file_type().is_symlink() {
            continue;
        }
        let is_dir = md.is_dir();
        if is_dir && (name.eq_ignore_ascii_case("System Volume Information") || name.eq_ignore_ascii_case("$RECYCLE.BIN")) {
            continue;
        }
        out.push(Entry {
            path: path.clone(),
            is_dir,
            size: if is_dir { 0 } else { md.len() },
            lba: 0,
            hidden: false,
            disk_path: Some(d.path()),
        });
        if is_dir {
            subdirs.push((d.path(), path));
        }
    }
    for (p, rel) in subdirs {
        walk_folder(&p, &rel, depth + 1, out);
    }
}

pub fn fmt_size(b: u64) -> String {
    let b = b as f64;
    if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} GB", b / 1024.0 / 1024.0 / 1024.0)
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB", b / 1024.0 / 1024.0)
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{} bytes", b as u64)
    }
}
