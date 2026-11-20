//! Updates from the person's GitHub project: reads the latest Release, downloads the attached
//! ISObubble program, safety-scans it and swaps it in.

use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

const AGENT: &str = "ISObubble-updater";

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub name: String,
    pub notes: String,
    pub page: String,
    pub asset_url: Option<String>,
    pub asset_name: String,
    pub asset_size: u64,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Accepts "owner/repo", "github.com/owner/repo" or a full https URL.
pub fn parse_repo(input: &str) -> Option<String> {
    let s = input.trim().trim_end_matches('/').trim_end_matches(".git");
    let s = s.split("github.com/").last().unwrap_or(s);
    let mut parts = s.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    let ok = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (ok(owner) && ok(repo)).then(|| format!("{owner}/{repo}"))
}

fn version_parts(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .take(4)
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// True if `remote` is a newer version than `local` ("v1.3.0" > "1.2.0").
pub fn is_newer(remote: &str, local: &str) -> bool {
    let (mut a, mut b) = (version_parts(remote), version_parts(local));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

/// Checks the latest release. Ok(None) means ISObubble is up to date.
pub fn check(repo: &str) -> Result<Option<UpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases");
    let resp = ureq::get(&url)
        .set("User-Agent", AGENT)
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(20))
        .call();
    let releases: Vec<Release> = match resp {
        Ok(r) => r.into_json().map_err(|e| format!("GitHub sent something unexpected: {e}"))?,
        Err(ureq::Error::Status(404, _)) => return Err(format!("GitHub project “{repo}” wasn't found (or it's private).")),
        Err(ureq::Error::Status(403, _)) => return Err("GitHub is limiting checks right now — try again in an hour.".into()),
        Err(e) => return Err(format!("Couldn't reach GitHub: {e}")),
    };
    let Some(r) = releases.into_iter().filter(|r| !r.draft).max_by(|a, b| {
        let (x, y) = (version_parts(&a.tag_name), version_parts(&b.tag_name));
        x.cmp(&y)
    }) else {
        return Err("No releases on GitHub yet. To publish an update, create a Release (tag like v1.3.0) and attach ISObubble.exe or a .zip containing it.".into());
    };
    if !is_newer(&r.tag_name, crate::system::APP_VERSION) {
        return Ok(None);
    }
    let asset = r
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(".exe") && a.name.to_lowercase().contains("isobubble"))
        .or_else(|| r.assets.iter().find(|a| a.name.to_lowercase().ends_with(".exe")))
        .or_else(|| r.assets.iter().find(|a| a.name.to_lowercase().ends_with(".zip")));
    Ok(Some(UpdateInfo {
        version: r.tag_name.clone(),
        name: r.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| r.tag_name.clone()),
        notes: r.body.unwrap_or_default(),
        page: r.html_url,
        asset_url: asset.map(|a| a.browser_download_url.clone()),
        asset_name: asset.map(|a| a.name.clone()).unwrap_or_default(),
        asset_size: asset.map(|a| a.size).unwrap_or(0),
    }))
}

/// Downloads the update, checks it with Microsoft Defender and replaces this program.
/// The new version starts the next time ISObubble opens.
pub fn download_and_install(info: &UpdateInfo, progress: &dyn Fn(f32, String)) -> Result<String, String> {
    let url = info.asset_url.as_ref().ok_or("This release has no ISObubble program attached (.exe or .zip).")?;
    let dir = std::env::temp_dir().join("isobubble_update");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(&info.asset_name);
    let resp = ureq::get(url).set("User-Agent", AGENT).call().map_err(|e| format!("Download failed: {e}"))?;
    let total = resp.header("Content-Length").and_then(|s| s.parse::<u64>().ok()).unwrap_or(info.asset_size).max(1);
    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(&file).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut out, &buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress((done as f32 / total as f32).min(1.0) * 0.8, format!("Downloading {} ({})", info.version, crate::iso::fmt_size(done)));
    }
    drop(out);

    let exe = if file.extension().map(|e| e.eq_ignore_ascii_case("zip")).unwrap_or(false) {
        progress(0.82, "Unpacking…".into());
        extract_exe_from_zip(&file, &dir)?
    } else {
        file.clone()
    };
    let head = std::fs::read(&exe).map_err(|e| e.to_string())?;
    if head.len() < 2 || &head[0..2] != b"MZ" {
        return Err("The downloaded file isn't a Windows program.".into());
    }

    progress(0.88, "Checking the update with Microsoft Defender…".into());
    if let crate::scan::DefenderResult::Threats(t) = crate::scan::defender_scan(&exe) {
        return Err(format!("Microsoft Defender flagged the download — update cancelled. {}", t.join(" ")));
    }

    progress(0.95, "Installing the update…".into());
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = me.with_extension("old.exe");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&me, &old).map_err(|e| format!("Couldn't replace ISObubble: {e}"))?;
    if let Err(e) = std::fs::copy(&exe, &me) {
        let _ = std::fs::rename(&old, &me);
        return Err(format!("Couldn't copy the new version in: {e}"));
    }
    crate::applog::log(format!("Updated to {} from GitHub", info.version));
    progress(1.0, "Done".into());
    Ok(format!("ISObubble {} is installed. Restart ISObubble to use it.", info.version))
}

fn extract_exe_from_zip(zip_path: &Path, dir: &Path) -> Result<PathBuf, String> {
    let f = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("Couldn't open the zip: {e}"))?;
    let mut best: Option<(usize, bool)> = None;
    for i in 0..z.len() {
        let Ok(e) = z.by_index(i) else { continue };
        let n = e.name().to_lowercase();
        if n.ends_with(".exe") {
            let preferred = n.contains("isobubble");
            if best.map(|b| !b.1 && preferred).unwrap_or(true) {
                best = Some((i, preferred));
            }
        }
    }
    let (i, _) = best.ok_or("The zip doesn't contain a program (.exe).")?;
    let mut e = z.by_index(i).map_err(|e| e.to_string())?;
    let out = dir.join("ISObubble_new.exe");
    let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    std::io::copy(&mut e, &mut f).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Removes the previous version left behind by an update.
pub fn cleanup_old() {
    if let Ok(me) = std::env::current_exe() {
        let _ = std::fs::remove_file(me.with_extension("old.exe"));
    }
}

/// Restarts ISObubble (after an update).
pub fn restart() {
    if let Ok(me) = std::env::current_exe() {
        let _ = std::process::Command::new(me).spawn();
        std::process::exit(0);
    }
}
