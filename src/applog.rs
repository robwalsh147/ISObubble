//! ISObubble's own activity log (installs, launches, scans, updates, errors).

use std::io::Write;

pub fn path() -> std::path::PathBuf {
    crate::system::data_dir().join("isobubble.log")
}

pub fn log(msg: impl AsRef<str>) {
    let _ = std::fs::create_dir_all(crate::system::data_dir());
    let p = path();
    // Keep the log small: start over when it passes 1 MB.
    if std::fs::metadata(&p).map(|m| m.len() > 1024 * 1024).unwrap_or(false) {
        let _ = std::fs::rename(&p, p.with_extension("old.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{}  {}", crate::crashlog::fmt_time(crate::system::now_secs() as i64), msg.as_ref());
    }
}
