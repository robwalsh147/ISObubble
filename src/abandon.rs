//! Optional "My Abandonware" add-on (www.myabandonware.com).
//! Lists games from the site's public pages (recently added feed, browse by year / letter /
//! platform), shows a game's download files, opens the download in the person's browser (the
//! site's own download page), then picks the finished file up from Downloads, unpacks it and
//! hands it to the install wizard (which reads and virus-scans it first).

use std::io::Read;
use std::path::{Path, PathBuf};

pub const SITE: &str = "https://www.myabandonware.com";
const AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ISObubble";

#[derive(Clone, Debug, Default)]
pub struct Game {
    pub name: String,
    pub url: String,
    pub year: String,
    pub platform: String,
    pub extra: String,
}

#[derive(Clone, Debug)]
pub struct DownloadFile {
    pub version: String,
    pub kind: String,
    pub language: String,
    pub size: String,
    pub url: String,
}

#[derive(Clone, Debug, Default)]
pub struct GameDetails {
    pub description: String,
    pub downloads: Vec<DownloadFile>,
}

fn get(url: &str) -> Result<String, String> {
    let r = ureq::get(url)
        .set("User-Agent", AGENT)
        .timeout(std::time::Duration::from_secs(25))
        .call()
        .map_err(|e| format!("Couldn't reach My Abandonware: {e}"))?;
    let mut s = String::new();
    r.into_reader().take(8 * 1024 * 1024).read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

pub fn decode(s: &str) -> String {
    let mut out = s
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");
    // numeric entities
    while let Some(i) = out.find("&#") {
        let Some(j) = out[i..].find(';') else { break };
        let num = &out[i + 2..i + j];
        let c = if let Some(h) = num.strip_prefix('x') { u32::from_str_radix(h, 16).ok() } else { num.parse().ok() };
        match c.and_then(char::from_u32) {
            Some(ch) => out.replace_range(i..i + j + 1, &ch.to_string()),
            None => break,
        }
    }
    out.trim().to_string()
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)?;
    Some(&s[i..i + j])
}

fn attr(tag: &str, name: &str) -> Option<String> {
    between(tag, &format!("{name}=\""), "\"").map(decode)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode(&out.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// The 100 most recently added games (the site's RSS feed).
pub fn recent() -> Result<Vec<Game>, String> {
    let xml = get(&format!("{SITE}/recent.rss"))?;
    let mut v = Vec::new();
    for item in xml.split("<item>").skip(1) {
        let name = between(item, "<title>", "</title>").map(decode).unwrap_or_default();
        let url = between(item, "<link>", "</link>").map(decode).unwrap_or_default();
        let desc = between(item, "<description>", "</description>").map(decode).unwrap_or_default();
        let mut parts = desc.split(" · ");
        let year = parts.next().unwrap_or("").to_string();
        // "2001 · Windows · Publisher. Name" or "2005 · Windows. Name" (no publisher)
        let p2 = parts.next().unwrap_or("");
        let platform = p2.split(". ").next().unwrap_or("").trim_end_matches('.').to_string();
        let publisher = parts.next().unwrap_or("").split(". ").next().unwrap_or("").trim_end_matches('.').to_string();
        let genres: Vec<String> = item
            .split("<category domain=\"https://www.myabandonware.com/browse/genre/\">")
            .skip(1)
            .filter_map(|c| c.split("</category>").next().map(decode))
            .collect();
        if !name.is_empty() && url.starts_with(SITE) {
            v.push(Game { name, url, year, platform, extra: [publisher, genres.join(", ")].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" · ") });
        }
    }
    Ok(v)
}

/// One page of a browse list, e.g. "browse/year/1998/" or "browse/name/A/".
/// Returns the games and whether there's another page.
pub fn browse(path: &str, page: u32) -> Result<(Vec<Game>, bool), String> {
    let path = path.trim_matches('/');
    let url = if page > 1 { format!("{SITE}/{path}/page/{page}/") } else { format!("{SITE}/{path}/") };
    let html = get(&url)?;
    let mut v: Vec<Game> = Vec::new();
    for chunk in html.split("data-fav-name=\"").skip(1) {
        let tag_end = chunk.find('>').unwrap_or(chunk.len());
        let tag = format!("data-fav-name=\"{}", &chunk[..tag_end]);
        let Some(name) = attr(&tag, "data-fav-name") else { continue };
        let Some(u) = attr(&tag, "data-fav-url") else { continue };
        let url = format!("{SITE}{u}");
        if v.iter().any(|g| g.url == url) {
            continue;
        }
        v.push(Game {
            name,
            url,
            year: attr(&tag, "data-fav-year").unwrap_or_default(),
            platform: attr(&tag, "data-fav-platforms").unwrap_or_default(),
            extra: String::new(),
        });
    }
    let has_next = html.contains(&format!("/{path}/page/{}/", page + 1));
    Ok((v, has_next))
}

/// A game's page: short description and its download files.
pub fn details(game_url: &str) -> Result<GameDetails, String> {
    let html = get(game_url)?;
    let description = between(&html, "<meta name=\"description\" content=\"", "\"").map(decode).unwrap_or_default();
    let mut downloads = Vec::new();
    let mut pos = 0;
    while let Some(i) = html[pos..].find("href=\"/download/") {
        let at = pos + i;
        pos = at + 16;
        let before = &html[..at];
        let version = before
            .rfind("game-downloads__platforms-title\">")
            .map(|j| strip_tags(&before[j + 33..].split("</").next().unwrap_or("")))
            .unwrap_or_default();
        let kind = before
            .rfind("game-downloads__platform-title\">")
            .map(|j| strip_tags(&before[j + 32..].split("</").next().unwrap_or("")))
            .unwrap_or_default();
        let href = between(&html[at..], "href=\"", "\"").unwrap_or("").to_string();
        if downloads.iter().any(|d: &DownloadFile| d.url.ends_with(&href)) {
            continue;
        }
        let tag_end = html[at..].find('>').map(|e| at + e).unwrap_or(at);
        let size = between(&html[at..tag_end + 1], "data-file-size=\"", "\"")
            .and_then(|s| s.parse::<u64>().ok())
            .map(crate::iso::fmt_size)
            .unwrap_or_default();
        let after = &html[at..html.len().min(at + 6000)];
        let language = after
            .split("game-downloads__platform-text\">")
            .nth(1)
            .map(|t| strip_tags(t.split("</div>").next().unwrap_or("")))
            .unwrap_or_default();
        downloads.push(DownloadFile { version, kind, language, size, url: format!("{SITE}{href}") });
    }
    Ok(GameDetails { description, downloads })
}

pub fn search_url(q: &str) -> String {
    let enc: String = q
        .trim()
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else if b == b' ' { "+".into() } else { format!("%{b:02X}") })
        .collect();
    format!("{SITE}/search/q/{enc}")
}

pub fn downloads_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    Path::new(&home).join("Downloads")
}

/// Game archives / disc images that appeared in Downloads after `since` (unix seconds).
pub fn new_downloads(since: u64) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(downloads_dir()) else { return Vec::new() };
    let mut v: Vec<(u64, PathBuf)> = rd
        .flatten()
        .filter_map(|d| {
            let p = d.path();
            let ext = p.extension()?.to_string_lossy().to_lowercase();
            if !matches!(ext.as_str(), "zip" | "iso" | "img" | "7z" | "rar") {
                return None;
            }
            let t = d.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            (t >= since).then_some((t, p))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|x| x.1).collect()
}

/// Unpacks a downloaded zip into `dest`, returning what to give the install wizard:
/// a disc image found inside, or the unpacked folder.
pub fn unpack(archive: &Path, dest: &Path, progress: &dyn Fn(f32)) -> Result<PathBuf, String> {
    let ext = archive.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext == "iso" || ext == "img" {
        return Ok(archive.to_path_buf());
    }
    if ext != "zip" {
        return Err("ISObubble can unpack .zip files. For .7z or .rar, unpack it with 7-Zip first, then use Install a disc on the folder.".into());
    }
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| format!("Couldn't open the zip: {e}"))?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let n = z.len().max(1);
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = e.enclosed_name() else { continue }; // refuses "../" tricks
        let out = dest.join(rel);
        if e.is_dir() {
            let _ = std::fs::create_dir_all(&out);
        } else {
            if let Some(p) = out.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let mut f = std::fs::File::create(&out).map_err(|er| format!("Couldn't write {}: {er}", out.display()))?;
            std::io::copy(&mut e, &mut f).map_err(|er| er.to_string())?;
        }
        progress(i as f32 / n as f32);
    }
    // Prefer a disc image inside (many Windows-era games come as an ISO).
    let mut found = None;
    fn find_iso(d: &Path, depth: usize, found: &mut Option<PathBuf>) {
        if depth > 3 || found.is_some() {
            return;
        }
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                find_iso(&p, depth + 1, found);
            } else if matches!(p.extension().map(|x| x.to_string_lossy().to_lowercase()).as_deref(), Some("iso") | Some("img")) {
                *found = Some(p);
                return;
            }
        }
    }
    find_iso(dest, 0, &mut found);
    if let Some(iso) = found {
        return Ok(iso);
    }
    // A zip with a single top folder: use that folder.
    let entries: Vec<_> = std::fs::read_dir(dest).map(|r| r.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    if entries.len() == 1 && entries[0].is_dir() {
        return Ok(entries[0].clone());
    }
    Ok(dest.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_browse_card_attributes() {
        let tag = r#"<i data-fav-name="101: The Airborne Invasion of Normandy" data-fav-url="/game/101-the-airborne-invasion-of-normandy-kh4" data-fav-year="1998" data-fav-platforms="Windows" aria-pressed="false">"#;
        assert_eq!(attr(tag, "data-fav-name").unwrap(), "101: The Airborne Invasion of Normandy");
        assert_eq!(attr(tag, "data-fav-year").unwrap(), "1998");
        assert_eq!(decode("Gil&#39;s &amp; Co"), "Gil's & Co");
        assert_eq!(search_url("doom 2"), "https://www.myabandonware.com/search/q/doom+2");
    }

    /// Network check against the live site: cargo test -- --ignored
    #[test]
    #[ignore]
    fn live_site() {
        let (v, more) = browse("browse/year/1998", 1).unwrap();
        assert!(v.len() > 10 && more, "{}", v.len());
        let r = recent().unwrap();
        assert!(r.len() > 20);
        let d = details("https://www.myabandonware.com/game/1483-anno-domini-tob").unwrap();
        assert!(!d.downloads.is_empty());
        println!("{:?}
{:?}", v[0], d.downloads);
    }
}
