//! App-wide settings, saved as JSON next to the library.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "owner/repo" of the person's GitHub project for ISObubble.
    pub github_repo: String,
    pub auto_check: bool,
    pub auto_install: bool,
    pub last_check: u64,
    pub skipped_version: String,
    pub tray_while_busy: bool,
    pub abandonware_addon: bool,
    pub watch_downloads: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            github_repo: "robwalsh147/ISObubble".to_string(),
            auto_check: true,
            auto_install: false,
            last_check: 0,
            skipped_version: String::new(),
            tray_while_busy: true,
            abandonware_addon: false,
            watch_downloads: true,
        }
    }
}

fn file() -> std::path::PathBuf {
    crate::system::data_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s: Settings = std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        // ISObubble's home on GitHub, used for updates unless another project is linked.
        if s.github_repo.trim().is_empty() {
            s.github_repo = Settings::default().github_repo;
        }
        s
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(crate::system::data_dir());
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(file(), s);
        }
    }
}
