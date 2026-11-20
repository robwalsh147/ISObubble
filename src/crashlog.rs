//! Crash logs: Windows crash/hang reports for a game, Windows Error Reporting files,
//! crash dumps and log files in the game's folders, and ISObubble's own log.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub ts: i64,
    pub source: &'static str,
    pub title: String,
    pub path: Option<PathBuf>,
    pub text: String,
}

/// Minutes to add to UTC to get local time.
fn local_offset_secs() -> i64 {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
        let r = GetTimeZoneInformation(&mut tz);
        let mut bias = tz.Bias;
        if r == 2 {
            bias += tz.DaylightBias;
        }
        return -(bias as i64) * 60;
    }
    #[allow(unreachable_code)]
    0
}

/// "2026-11-19 14:05" in local time.
pub fn fmt_time(unix: i64) -> String {
    let t = unix + local_offset_secs();
    let days = t.div_euclid(86400);
    let secs = t.rem_euclid(86400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

fn mtime(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Reads the end of a text file (the newest part of a log).
pub fn read_tail(p: &Path, max: usize) -> String {
    let Ok(b) = std::fs::read(p) else { return String::new() };
    let start = b.len().saturating_sub(max);
    let slice = &b[start..];
    if slice.len() > 2 && slice.iter().skip(1).step_by(2).take(200).filter(|&&c| c == 0).count() > 80 {
        let u: Vec<u16> = slice.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&u);
    }
    String::from_utf8_lossy(slice).to_string()
}

/// Windows "Application Error" / "Application Hang" / WER events of the last 90 days.
fn windows_events(exe_names: &[String]) -> Vec<LogEntry> {
    let script = "Get-WinEvent -FilterHashtable @{LogName='Application'; Id=1000,1001,1002; StartTime=(Get-Date).AddDays(-90)} -MaxEvents 600 -ErrorAction SilentlyContinue | \
                  ForEach-Object { [DateTimeOffset]::new($_.TimeCreated).ToUnixTimeSeconds().ToString() + '|' + $_.Id + '|' + (($_.Message -replace \"`r?`n\", ' ¶ ') -replace '\\|', '/') }";
    let out = crate::system::ps(script).unwrap_or_default();
    let mut v = Vec::new();
    for line in out.lines() {
        let mut parts = line.splitn(3, '|');
        let (Some(ts), Some(id), Some(msg)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let lower = msg.to_lowercase();
        let Some(exe) = exe_names.iter().find(|n| lower.contains(n.as_str())) else { continue };
        let kind = match id {
            "1000" => "crashed",
            "1002" => "stopped responding",
            _ => "crash report sent to Windows",
        };
        v.push(LogEntry {
            ts: ts.parse().unwrap_or(0),
            source: "Windows",
            title: format!("{exe} {kind}"),
            path: None,
            text: msg.replace(" ¶ ", "\n"),
        });
    }
    v
}

fn wer_reports(exe_names: &[String]) -> Vec<LogEntry> {
    let mut bases = Vec::new();
    for var in ["ProgramData", "LOCALAPPDATA"] {
        if let Ok(b) = std::env::var(var) {
            for sub in ["ReportArchive", "ReportQueue"] {
                bases.push(Path::new(&b).join("Microsoft\\Windows\\WER").join(sub));
            }
        }
    }
    let mut v = Vec::new();
    for b in bases {
        let Ok(rd) = std::fs::read_dir(&b) else { continue };
        for d in rd.flatten() {
            let n = d.file_name().to_string_lossy().to_lowercase();
            let Some(exe) = exe_names.iter().find(|e| n.contains(e.as_str())) else { continue };
            let wer = d.path().join("Report.wer");
            v.push(LogEntry {
                ts: mtime(&d.path()),
                source: "Windows report",
                title: format!("{exe}: {}", d.file_name().to_string_lossy().split('_').next().unwrap_or("Report")),
                text: read_tail(&wer, 64 * 1024),
                path: Some(if wer.exists() { wer } else { d.path() }),
            });
        }
    }
    v
}

fn game_files(roots: &[PathBuf]) -> Vec<LogEntry> {
    let mut v = Vec::new();
    fn walk(dir: &Path, depth: usize, v: &mut Vec<LogEntry>) {
        if depth > 4 || v.len() > 80 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for d in rd.flatten().take(4000) {
            let p = d.path();
            if p.is_dir() {
                walk(&p, depth + 1, v);
                continue;
            }
            let n = d.file_name().to_string_lossy().to_lowercase();
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let in_crash_dir = dir.to_string_lossy().to_lowercase().contains("crash");
            let is_dump = matches!(ext.as_str(), "dmp" | "mdmp");
            let is_log = matches!(ext.as_str(), "log" | "rpt") || (ext == "txt" && (n.contains("crash") || n.contains("error") || n.contains("log")));
            if is_dump || is_log || (in_crash_dir && ext != "exe" && ext != "dll") {
                v.push(LogEntry {
                    ts: mtime(&p),
                    source: if is_dump { "Crash dump" } else if n.contains("crash") || in_crash_dir { "Crash log" } else { "Game log" },
                    title: d.file_name().to_string_lossy().to_string(),
                    text: if is_dump {
                        format!("Crash dump file ({}). It can be opened with WinDbg or sent to the game's support.", crate::iso::fmt_size(d.metadata().map(|m| m.len()).unwrap_or(0)))
                    } else {
                        read_tail(&p, 96 * 1024)
                    },
                    path: Some(p),
                });
            }
        }
    }
    for r in roots {
        if r.as_os_str().len() > 3 && r.exists() {
            walk(r, 0, &mut v);
        }
    }
    v
}

/// Everything for the given games (exe file names, lower-case) and folders.
pub fn collect(exe_names: Vec<String>, roots: Vec<PathBuf>, include_app_log: bool) -> Vec<LogEntry> {
    let mut v = Vec::new();
    if !exe_names.is_empty() {
        v.extend(windows_events(&exe_names));
        v.extend(wer_reports(&exe_names));
    }
    v.extend(game_files(&roots));
    if include_app_log {
        let p = crate::applog::path();
        if p.exists() {
            v.push(LogEntry {
                ts: mtime(&p),
                source: "ISObubble",
                title: "ISObubble activity log".into(),
                text: read_tail(&p, 128 * 1024),
                path: Some(p),
            });
        }
    }
    v.sort_by(|a, b| b.ts.cmp(&a.ts));
    v
}
