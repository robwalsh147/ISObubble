//! Tiny Windows program (PE) reader: architecture, signature, sections and the
//! "version info" text (product name, company, version) shown in Explorer's Properties.

use std::collections::BTreeMap;

pub struct Section {
    pub name: String,
    pub entropy: f32,
    pub executable: bool,
    pub raw_size: u32,
}

pub struct PeInfo {
    pub machine: &'static str,
    pub is_dll: bool,
    pub signed: bool,
    pub gui: bool,
    pub sections: Vec<Section>,
    pub version: BTreeMap<String, String>,
}

fn u16le(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub fn entropy(data: &[u8]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u32; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let n = data.len() as f32;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f32 / n;
            -p * p.log2()
        })
        .sum()
}

pub fn parse(b: &[u8]) -> Option<PeInfo> {
    if b.len() < 0x40 || &b[0..2] != b"MZ" {
        return None;
    }
    let pe = u32le(b, 0x3C)? as usize;
    if b.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let coff = pe + 4;
    let machine = match u16le(b, coff)? {
        0x14c => "32-bit (x86)",
        0x8664 => "64-bit (x64)",
        0xaa64 => "ARM64",
        0x1c4 => "ARM",
        _ => "unknown",
    };
    let nsec = u16le(b, coff + 2)? as usize;
    let opt_size = u16le(b, coff + 16)? as usize;
    let chars = u16le(b, coff + 18)?;
    let opt = coff + 20;
    let magic = u16le(b, opt)?;
    let dd = match magic {
        0x10b => opt + 96,
        0x20b => opt + 112,
        _ => return None,
    };
    let subsystem = u16le(b, opt + 68).unwrap_or(0);
    let sec_size = u32le(b, dd + 4 * 8 + 4).unwrap_or(0);
    let mut sections = Vec::new();
    let st = opt + opt_size;
    for i in 0..nsec.min(96) {
        let s = st + i * 40;
        let Some(raw) = b.get(s..s + 40) else { break };
        let name = String::from_utf8_lossy(&raw[0..8]).trim_end_matches('\0').to_string();
        let raw_size = u32le(raw, 16).unwrap_or(0);
        let raw_ptr = u32le(raw, 20).unwrap_or(0) as usize;
        let ch = u32le(raw, 36).unwrap_or(0);
        let end = (raw_ptr + raw_size as usize).min(b.len());
        let ent = if raw_ptr < end { entropy(&b[raw_ptr..end]) } else { 0.0 };
        sections.push(Section {
            name,
            entropy: ent,
            executable: ch & 0x2000_0000 != 0,
            raw_size,
        });
    }
    Some(PeInfo {
        machine,
        is_dll: chars & 0x2000 != 0,
        signed: sec_size > 0,
        gui: subsystem == 2,
        sections,
        version: version_strings(b),
    })
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

pub fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    let first = n[0];
    let last = h.len() - n.len();
    let mut i = 0;
    while i <= last {
        match h[i..=last].iter().position(|&b| b == first) {
            Some(p) => {
                i += p;
                if &h[i..i + n.len()] == n {
                    return Some(i);
                }
                i += 1;
            }
            None => return None,
        }
    }
    None
}

/// Reads the version-info strings by locating each key in the resource data.
fn version_strings(b: &[u8]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(sfi) = find(b, &utf16("StringFileInfo")) else {
        return out;
    };
    let region = &b[sfi..b.len().min(sfi + 64 * 1024)];
    for key in [
        "ProductName",
        "CompanyName",
        "FileDescription",
        "ProductVersion",
        "FileVersion",
        "LegalCopyright",
        "OriginalFilename",
    ] {
        let mut needle = utf16(key);
        needle.extend_from_slice(&[0, 0]);
        let Some(kp) = find(region, &needle) else { continue };
        let abs = sfi + kp;
        if abs < 6 {
            continue;
        }
        let start = abs - 6;
        let vlen = u16le(b, start + 2).unwrap_or(0) as usize;
        let mut p = abs + needle.len();
        p = (p + 3) & !3;
        let mut chars = Vec::new();
        while chars.len() < vlen.max(1).min(512) {
            let Some(c) = u16le(b, p) else { break };
            if c == 0 {
                break;
            }
            chars.push(c);
            p += 2;
        }
        let v = String::from_utf16_lossy(&chars).trim().to_string();
        if !v.is_empty() && v.chars().all(|c| !c.is_control()) {
            out.insert(key.to_string(), v);
        }
    }
    out
}
