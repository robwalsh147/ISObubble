//! The launcher library: everything installed through ISObubble, saved as JSON.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WindowMode {
    #[default]
    GameDefault,
    Windowed,
    Borderless,
    Fullscreen,
}

impl WindowMode {
    pub fn label(&self) -> &'static str {
        match self {
            WindowMode::GameDefault => "Game's own setting",
            WindowMode::Windowed => "Windowed",
            WindowMode::Borderless => "Borderless windowed",
            WindowMode::Fullscreen => "Fullscreen",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub name: String,
    pub kind: String,
    pub icon: String,
    pub summary: String,
    pub discs: Vec<String>,
    pub program: Option<String>,
    pub needs_disc: bool,
    pub folder: Option<String>,
    pub verdict: String,
    pub added: u64,
    pub hue: f32,
    /// The game's own launcher / settings program (e.g. a "... Edition" launcher).
    #[serde(default)]
    pub launcher: Option<String>,
    /// Extra command-line options passed when pressing Play.
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub window_mode: WindowMode,
    /// Monitor to play on (Windows device name, e.g. \\.\DISPLAY2). None = main monitor.
    #[serde(default)]
    pub monitor: Option<String>,
    /// Windows compatibility settings, e.g. "WINXPSP3 RUNASADMIN".
    #[serde(default)]
    pub compat: String,
    /// Program that runs the game (DOSBox for DOS games).
    #[serde(default)]
    pub runner: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Library {
    pub items: Vec<Item>,
}

fn file() -> PathBuf {
    crate::system::data_dir().join("library.json")
}

impl Library {
    pub fn load() -> Library {
        // Carry over the library from the app's earlier name.
        if !file().exists() {
            let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
            let old = std::path::Path::new(&base).join("DiscBubble").join("library.json");
            if old.exists() {
                let _ = std::fs::create_dir_all(crate::system::data_dir());
                let _ = std::fs::copy(&old, file());
            }
        }
        std::fs::read_to_string(file())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(crate::system::data_dir());
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(file(), s);
        }
    }
    pub fn next_id(&self) -> u64 {
        self.items.iter().map(|i| i.id).max().unwrap_or(0) + 1
    }
}
