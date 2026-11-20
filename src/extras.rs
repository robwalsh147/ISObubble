//! Extra screens and background jobs: compatibility checker, crash logs, My Abandonware
//! add-on, GitHub updates, tray / start-with-Windows settings and display info.

use super::*;
use crate::pages::{big, muted};
use egui::{vec2, Align, Color32, Layout, RichText};

pub enum ExtMsg {
    Monitors(Vec<display::Monitor>),
    UpdateChecked(Result<Option<update::UpdateInfo>, String>, bool),
    UpdateProgress(f32, String),
    UpdateDone(Result<String, String>),
    Logs(Vec<crashlog::LogEntry>),
    AbList(Result<(Vec<abandon::Game>, bool), String>, bool),
    AbDetails(String, Result<abandon::GameDetails, String>),
    Unpacked(Result<PathBuf, String>),
}

#[derive(Default)]
pub struct Extras {
    pub monitors: Vec<display::Monitor>,
    pub update: Option<update::UpdateInfo>,
    pub update_msg: Option<(String, bool)>,
    pub update_busy: bool,
    pub update_progress: Option<(f32, String)>,
    pub update_ready: bool,
    pub repo_input: String,
    pub last_auto_check: f64,
    pub start_with_windows: bool,

    pub logs: Vec<crashlog::LogEntry>,
    pub logs_loading: bool,
    pub logs_for: Option<u64>,
    pub logs_sel: usize,
    pub logs_loaded_once: bool,

    pub compat_for: Option<u64>,
    pub compat_exe: Option<PathBuf>,
    pub compat: Vec<compat::Check>,
    pub compat_checked: bool,

    pub ab_games: Vec<abandon::Game>,
    pub ab_title: String,
    pub ab_path: String,
    pub ab_page: u32,
    pub ab_more: bool,
    pub ab_loading: bool,
    pub ab_error: Option<String>,
    pub ab_filter: String,
    pub ab_search: String,
    pub ab_year: u32,
    pub ab_sel: Option<abandon::Game>,
    pub ab_details: Option<(String, abandon::GameDetails)>,
    pub ab_details_loading: bool,
    pub ab_watch_since: u64,
    pub ab_found: Vec<PathBuf>,
    pub ab_dismissed: Vec<PathBuf>,
    pub ab_unpacking: bool,
    pub last_dl_check: f64,
}

fn level_color(l: compat::Level) -> Color32 {
    match l {
        compat::Level::Good => TEAL,
        compat::Level::Info => BLUE,
        compat::Level::Warn => AMBER,
        compat::Level::Bad => RED,
    }
}

impl App {
    fn ext_send(tx: &Sender<Msg>, m: ExtMsg) {
        let _ = tx.send(Msg::Ext(m));
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.wiz.installing || self.wiz.scan.running || self.scanner.running || self.ext.update_busy || self.ext.ab_unpacking
    }

    pub(crate) fn startup_extras(&mut self) {
        self.ext.repo_input = self.settings.github_repo.clone();
        self.ext.ab_watch_since = system::now_secs();
        self.ext.ab_year = 1998;
        self.ext.start_with_windows = system::starts_with_windows();
        self.refresh_monitors();
        let due = system::now_secs().saturating_sub(self.settings.last_check) > 6 * 3600;
        if self.settings.auto_check && !self.settings.github_repo.is_empty() && due && self.shot.is_none() {
            self.check_updates(false);
        }
    }

    pub(crate) fn periodic_extras(&mut self) {
        // Check GitHub every 6 hours while ISObubble keeps running (e.g. in the tray).
        if self.settings.auto_check && !self.settings.github_repo.is_empty() && !self.ext.update_busy && self.shot.is_none() {
            if self.ext.last_auto_check == 0.0 {
                self.ext.last_auto_check = self.time;
            } else if self.time - self.ext.last_auto_check > 6.0 * 3600.0 {
                self.ext.last_auto_check = self.time;
                self.check_updates(false);
            }
        }
        // Watch Downloads for new game downloads (abandonware add-on).
        if self.settings.abandonware_addon && self.settings.watch_downloads && self.time - self.ext.last_dl_check > 4.0 {
            self.ext.last_dl_check = self.time;
            for p in abandon::new_downloads(self.ext.ab_watch_since) {
                if !self.ext.ab_found.contains(&p) && !self.ext.ab_dismissed.contains(&p) {
                    // Skip files still being written by the browser.
                    let s1 = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                    if s1 > 0 {
                        self.toast(format!("New download: {} — open Abandonware to install it.", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), true);
                        self.ext.ab_found.push(p);
                    }
                }
            }
        }
    }

    pub(crate) fn refresh_monitors(&mut self) {
        self.spawn(|tx| Self::ext_send(&tx, ExtMsg::Monitors(display::monitors())));
    }

    pub(crate) fn check_updates(&mut self, manual: bool) {
        let Some(repo) = update::parse_repo(&self.settings.github_repo) else {
            if manual {
                self.ext.update_msg = Some(("Link your GitHub project first (owner/repo or its web address).".into(), false));
            }
            return;
        };
        self.ext.update_busy = true;
        self.ext.update_msg = None;
        self.settings.last_check = system::now_secs();
        self.settings.save();
        self.spawn(move |tx| Self::ext_send(&tx, ExtMsg::UpdateChecked(update::check(&repo), manual)));
    }

    pub(crate) fn install_update(&mut self) {
        let Some(info) = self.ext.update.clone() else { return };
        self.ext.update_busy = true;
        self.ext.update_progress = Some((0.0, "Starting download…".into()));
        tray::set_status(format!("Downloading update {}", info.version));
        self.spawn(move |tx| {
            let r = update::download_and_install(&info, &|f, s| {
                tray::set_status(format!("Downloading update {} – {:.0}%", info.version, f * 100.0));
                Self::ext_send(&tx, ExtMsg::UpdateProgress(f, s));
            });
            tray::set_status(if r.is_ok() { "Update ready — restart ISObubble" } else { "" });
            Self::ext_send(&tx, ExtMsg::UpdateDone(r));
        });
    }

    pub(crate) fn load_logs(&mut self) {
        let games: Vec<library::Item> = match self.ext.logs_for {
            Some(0) => vec![],
            Some(id) => self.lib.items.iter().filter(|i| i.id == id).cloned().collect(),
            None => self.lib.items.clone(),
        };
        let include_app = matches!(self.ext.logs_for, None | Some(0));
        let mut exes = Vec::new();
        let mut roots = Vec::new();
        for g in &games {
            for p in [g.program.as_ref(), g.launcher.as_ref()].into_iter().flatten() {
                if let Some(n) = Path::new(p).file_name() {
                    exes.push(n.to_string_lossy().to_lowercase());
                }
            }
            if let Some(p) = &g.program {
                roots.push(gamecfg::game_root(Path::new(p)));
            }
        }
        self.ext.logs_loading = true;
        self.ext.logs_loaded_once = true;
        self.spawn(move |tx| Self::ext_send(&tx, ExtMsg::Logs(crashlog::collect(exes, roots, include_app))));
    }

    fn ab_load(&mut self, title: &str, path: &str, page: u32) {
        self.ext.ab_title = title.to_string();
        self.ext.ab_path = path.to_string();
        self.ext.ab_page = page;
        self.ext.ab_loading = true;
        self.ext.ab_error = None;
        let append = page > 1;
        if !append {
            self.ext.ab_games.clear();
        }
        let path = path.to_string();
        self.spawn(move |tx| {
            let r = if path == "recent" { abandon::recent().map(|v| (v, false)) } else { abandon::browse(&path, page) };
            Self::ext_send(&tx, ExtMsg::AbList(r, append));
        });
    }

    fn ab_open(&mut self, g: abandon::Game) {
        self.ext.ab_details = None;
        self.ext.ab_details_loading = true;
        let url = g.url.clone();
        self.ext.ab_sel = Some(g);
        self.spawn(move |tx| {
            let r = abandon::details(&url);
            Self::ext_send(&tx, ExtMsg::AbDetails(url, r));
        });
    }

    fn ab_install_file(&mut self, file: PathBuf) {
        if self.wiz.installing {
            self.toast("Finish the current install first.", false);
            return;
        }
        let stem = system::safe_folder_name(&file.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Game".into()));
        let dest = system::default_install_root().join(stem);
        self.ext.ab_unpacking = true;
        tray::set_status(format!("Unpacking {}", file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
        self.spawn(move |tx| {
            let r = abandon::unpack(&file, &dest, &|f| tray::set_status(format!("Unpacking – {:.0}%", f * 100.0)));
            tray::set_status("");
            Self::ext_send(&tx, ExtMsg::Unpacked(r));
        });
    }

    pub(crate) fn handle_ext(&mut self, m: ExtMsg) {
        match m {
            ExtMsg::Monitors(v) => self.ext.monitors = v,
            ExtMsg::UpdateChecked(r, manual) => {
                self.ext.update_busy = false;
                match r {
                    Ok(Some(info)) => {
                        self.toast(format!("ISObubble {} is available.", info.version), true);
                        applog::log(format!("Update available: {}", info.version));
                        let auto = self.settings.auto_install && info.asset_url.is_some();
                        self.ext.update = Some(info);
                        if auto {
                            self.install_update();
                        }
                    }
                    Ok(None) => {
                        self.ext.update = None;
                        if manual {
                            self.ext.update_msg = Some((format!("You're up to date (version {}).", system::APP_VERSION), true));
                        }
                    }
                    Err(e) => {
                        if manual {
                            self.ext.update_msg = Some((e, false));
                        }
                    }
                }
            }
            ExtMsg::UpdateProgress(f, s) => self.ext.update_progress = Some((f, s)),
            ExtMsg::UpdateDone(r) => {
                self.ext.update_busy = false;
                self.ext.update_progress = None;
                match r {
                    Ok(s) => {
                        self.ext.update_ready = true;
                        self.ext.update = None;
                        self.ext.update_msg = Some((s, true));
                    }
                    Err(e) => self.ext.update_msg = Some((e, false)),
                }
            }
            ExtMsg::Logs(v) => {
                self.ext.logs_loading = false;
                self.ext.logs = v;
                self.ext.logs_sel = 0;
            }
            ExtMsg::AbList(r, append) => {
                self.ext.ab_loading = false;
                match r {
                    Ok((v, more)) => {
                        if !append {
                            self.ext.ab_games.clear();
                        }
                        self.ext.ab_games.extend(v);
                        self.ext.ab_more = more;
                    }
                    Err(e) => self.ext.ab_error = Some(e),
                }
            }
            ExtMsg::AbDetails(url, r) => {
                self.ext.ab_details_loading = false;
                match r {
                    Ok(d) => self.ext.ab_details = Some((url, d)),
                    Err(e) => self.ext.ab_error = Some(e),
                }
            }
            ExtMsg::Unpacked(r) => {
                self.ext.ab_unpacking = false;
                match r {
                    Ok(p) => {
                        applog::log(format!("Unpacked download to {}", p.display()));
                        self.reset_wizard();
                        let is_folder = p.is_dir();
                        self.add_disc(p.clone());
                        if is_folder {
                            // The unpacked folder is already where the game will live.
                            self.wiz.dest = p.to_string_lossy().to_string();
                        }
                        self.page = Page::Wizard;
                    }
                    Err(e) => self.toast(e, false),
                }
            }
        }
    }

    /// Adds a game that's already on this PC (not installed through ISObubble).
    pub(crate) fn add_existing_game(&mut self) {
        let start = std::env::var("ProgramFiles(x86)").or_else(|_| std::env::var("ProgramFiles")).unwrap_or_default();
        let Some(p) = rfd::FileDialog::new().set_title("Choose the game's program").set_directory(start).add_filter("Programs", &["exe", "bat", "com"]).pick_file() else {
            return;
        };
        let product = std::fs::read(&p).ok().and_then(|b| pe::parse(&b)).and_then(|i| i.version.get("ProductName").cloned()).filter(|n| n.len() < 60 && !n.contains("Windows"));
        let name = product.unwrap_or_else(|| p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        let program = p.to_string_lossy().to_string();
        let item = library::Item {
            id: self.lib.next_id(),
            name: name.clone(),
            kind: "Game".into(),
            icon: "🎮".into(),
            summary: format!("Added from {}", p.parent().map(|x| x.display().to_string()).unwrap_or_default()),
            discs: vec![],
            program: Some(program.clone()),
            needs_disc: false,
            folder: p.parent().map(|x| x.to_string_lossy().to_string()),
            verdict: "Not scanned".into(),
            added: system::now_secs(),
            hue: (self.lib.items.len() as f32 * 0.21 + 0.05).fract(),
            launcher: gamecfg::detect_launcher(&p),
            args: String::new(),
            window_mode: Default::default(),
            monitor: None,
            compat: String::new(),
            runner: None,
        };
        self.lib.items.push(item);
        self.lib.save();
        self.toast(format!("{name} was added — press Play, or launch it from the tray."), true);
    }

    // ---------- Compatibility page ----------

    pub(crate) fn compat_page(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Compatibility checker");
        sub(ui, "Finds what could stop a game running on this PC — and fixes it with one click.");
        ui.add_space(10.0);
        let mut run = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let sel = match (self.ext.compat_for, &self.ext.compat_exe) {
                    (Some(id), _) => self.lib.items.iter().find(|i| i.id == id).map(|i| i.name.clone()).unwrap_or_default(),
                    (None, Some(p)) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    _ => "Choose a game…".into(),
                };
                egui::ComboBox::from_id_salt("compat_game").width(320.0).selected_text(sel).show_ui(ui, |ui| {
                    for it in self.lib.items.iter().filter(|i| i.program.is_some()) {
                        if ui.selectable_label(self.ext.compat_for == Some(it.id), &it.name).clicked() {
                            self.ext.compat_for = Some(it.id);
                            self.ext.compat_exe = None;
                            run = true;
                        }
                    }
                });
                if soft_button(ui, "Check another program…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Programs", &["exe", "com", "bat"]).pick_file() {
                        self.ext.compat_for = None;
                        self.ext.compat_exe = Some(p);
                        run = true;
                    }
                }
                if (self.ext.compat_for.is_some() || self.ext.compat_exe.is_some()) && bubble_button(ui, "↻ Check again", TEAL).clicked() {
                    run = true;
                }
            });
        });
        if run {
            self.run_compat();
        }
        ui.add_space(10.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.ext.compat_checked {
                let fixes = compat_list(ui, &self.ext.compat);
                if let Some(f) = fixes {
                    self.apply_compat_fix(f);
                }
            }
            if let Some(id) = self.ext.compat_for {
                ui.add_space(10.0);
                if let Some(idx) = self.lib.items.iter().position(|i| i.id == id) {
                    let mut item = self.lib.items[idx].clone();
                    if compat_settings(ui, &mut item) {
                        self.lib.items[idx] = item;
                        self.lib.save();
                    }
                }
            }
            ui.add_space(10.0);
            self.displays_card(ui);
        });
    }

    pub(crate) fn run_compat(&mut self) {
        let (program, root, needs_disc) = match (self.ext.compat_for, &self.ext.compat_exe) {
            (Some(id), _) => {
                let Some(it) = self.lib.items.iter().find(|i| i.id == id) else { return };
                let Some(p) = it.program.clone() else { return };
                let p = PathBuf::from(p);
                (p.clone(), gamecfg::game_root(&p), it.needs_disc)
            }
            (None, Some(p)) => (p.clone(), gamecfg::game_root(p), false),
            _ => return,
        };
        self.ext.compat = compat::check_game(&program, &root, needs_disc);
        self.ext.compat_checked = true;
    }

    fn apply_compat_fix(&mut self, fix: compat::Fix) {
        if let compat::Fix::Url(u) = &fix {
            system::open_url(u);
            return;
        }
        let Some(id) = self.ext.compat_for else {
            self.toast("Add the game to your library first to apply fixes.", false);
            return;
        };
        let Some(idx) = self.lib.items.iter().position(|i| i.id == id) else { return };
        let it = &mut self.lib.items[idx];
        let msg = match fix {
            compat::Fix::Layer(l) => {
                for t in l.split_whitespace() {
                    if !it.compat.split_whitespace().any(|x| x == t) {
                        it.compat = format!("{} {t}", it.compat).trim().to_string();
                    }
                }
                "Compatibility setting added — it's used every time you press Play.".to_string()
            }
            compat::Fix::NeedsDisc => {
                it.needs_disc = true;
                "The disc will be inserted automatically when you press Play.".into()
            }
            compat::Fix::UseDosbox => match compat::find_dosbox() {
                Some(d) => {
                    it.runner = Some(d.to_string_lossy().to_string());
                    "This game will now start inside DOSBox.".into()
                }
                None => {
                    system::open_url("https://www.dosbox-staging.org/releases/windows/");
                    "DOSBox isn't installed yet — its download page is opening. Install it, then press the button again.".into()
                }
            },
            compat::Fix::Url(_) => String::new(),
        };
        self.lib.save();
        self.toast(msg, true);
        self.run_compat();
    }

    pub(crate) fn displays_card(&mut self, ui: &mut egui::Ui) {
        let mut refresh = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(big("🖥  Your displays", 16.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if soft_button(ui, "↻ Refresh").clicked() {
                        refresh = true;
                    }
                    if soft_button(ui, "Windows HDR settings").clicked() {
                        system::open_url("ms-settings:display-hdr");
                    }
                });
            });
            if self.ext.monitors.is_empty() {
                ui.label(muted("Reading your displays…"));
            }
            for m in &self.ext.monitors {
                ui.horizontal_wrapped(|ui| {
                    ui.label(big(m.title(), 14.0));
                    chip(ui, &format!("{}×{}", m.width, m.height), BLUE);
                    chip(ui, &format!("{} Hz", m.hz), TEAL);
                    if m.max_hz > m.hz {
                        chip(ui, &format!("can do {} Hz", m.max_hz), AMBER);
                    }
                    let (t, c) = match (m.hdr_supported, m.hdr_on) {
                        (_, true) => ("HDR on", PINK),
                        (true, false) => ("HDR supported (off)", AMBER),
                        _ => ("No HDR", MUTED),
                    };
                    chip(ui, t, c);
                });
            }
            if self.ext.monitors.iter().any(|m| m.max_hz > m.hz) {
                ui.label(RichText::new("A monitor is running below its best refresh rate — change it in Windows Settings › Display › Advanced display.").size(13.0).color(MUTED));
            }
        });
        if refresh {
            self.refresh_monitors();
        }
    }

    // ---------- Crash logs page ----------

    pub(crate) fn logs_page(&mut self, ui: &mut egui::Ui) {
        if !self.ext.logs_loaded_once {
            self.load_logs();
        }
        heading(ui, "Crash logs");
        sub(ui, "Crashes and hangs Windows recorded for your games, their crash dumps and log files, and ISObubble's own log.");
        ui.add_space(10.0);
        let mut reload = false;
        ui.horizontal(|ui| {
            let sel = match self.ext.logs_for {
                None => "All games + ISObubble".to_string(),
                Some(0) => "ISObubble only".to_string(),
                Some(id) => self.lib.items.iter().find(|i| i.id == id).map(|i| i.name.clone()).unwrap_or_default(),
            };
            egui::ComboBox::from_id_salt("logs_for").width(300.0).selected_text(sel).show_ui(ui, |ui| {
                if ui.selectable_label(self.ext.logs_for.is_none(), "All games + ISObubble").clicked() {
                    self.ext.logs_for = None;
                    reload = true;
                }
                if ui.selectable_label(self.ext.logs_for == Some(0), "ISObubble only").clicked() {
                    self.ext.logs_for = Some(0);
                    reload = true;
                }
                for it in &self.lib.items {
                    if ui.selectable_label(self.ext.logs_for == Some(it.id), &it.name).clicked() {
                        self.ext.logs_for = Some(it.id);
                        reload = true;
                    }
                }
            });
            if bubble_button(ui, "↻ Refresh", TEAL).clicked() {
                reload = true;
            }
            if soft_button(ui, "Open crash-report folder").clicked() {
                let p = std::env::var("ProgramData").map(|b| Path::new(&b).join("Microsoft\\Windows\\WER\\ReportArchive")).unwrap_or_default();
                system::open_folder(&p);
            }
            if self.ext.logs_loading {
                ui.add(egui::Spinner::new().color(TEAL));
            }
        });
        if reload {
            self.load_logs();
        }
        ui.add_space(8.0);
        if self.ext.logs.is_empty() && !self.ext.logs_loading {
            tinted_card(TEAL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(big("No crashes found 🎉", 16.0));
                ui.label(muted("Nothing was recorded in the last 90 days. If a game crashes, its report will show up here."));
            });
            return;
        }
        let h = ui.available_height();
        let w = ui.available_width();
        let _ = (w, h);
        columns_plain(ui, 2, |col, ui| {
            if col == 0 {
                egui::ScrollArea::vertical().id_salt("log_list").auto_shrink([false, false]).show(ui, |ui| {
                    for (i, e) in self.ext.logs.iter().enumerate() {
                        let sel = i == self.ext.logs_sel;
                        let c = match e.source {
                            "Windows" | "Windows report" | "Crash dump" | "Crash log" => RED,
                            "ISObubble" => PURPLE,
                            _ => BLUE,
                        };
                        let frame = if sel { tinted_card(c) } else { card().inner_margin(egui::Margin::same(12)) };
                        let r = frame
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    chip(ui, e.source, c);
                                    ui.label(RichText::new(crashlog::fmt_time(e.ts)).size(12.0).color(MUTED));
                                });
                                ui.add(egui::Label::new(RichText::new(&e.title).size(14.0).color(TEXT)).truncate());
                            })
                            .response;
                        if r.interact(egui::Sense::click()).clicked() {
                            self.ext.logs_sel = i;
                        }
                        ui.add_space(4.0);
                    }
                });
            } else {
                let Some(e) = self.ext.logs.get(self.ext.logs_sel).cloned() else { return };
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(big(&e.title, 16.0));
                    ui.horizontal(|ui| {
                        if let Some(p) = &e.path {
                            if soft_button(ui, "Open file").clicked() {
                                let mut c = std::process::Command::new("notepad.exe");
                                c.arg(p);
                                if p.extension().map(|x| x.eq_ignore_ascii_case("dmp") || x.eq_ignore_ascii_case("mdmp")).unwrap_or(false) {
                                    system::open_folder(p.parent().unwrap_or(p));
                                } else {
                                    let _ = c.spawn();
                                }
                            }
                            if soft_button(ui, "Open folder").clicked() {
                                system::open_folder(p.parent().unwrap_or(p));
                            }
                        }
                        if soft_button(ui, "Copy text").clicked() {
                            ui.ctx().copy_text(e.text.clone());
                            self.toast("Copied — paste it into a bug report or forum post.", true);
                        }
                    });
                    egui::ScrollArea::vertical().id_salt("log_text").auto_shrink([false, false]).show(ui, |ui| {
                        let mut t = e.text.clone();
                        ui.add(egui::TextEdit::multiline(&mut t).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).interactive(true));
                    });
                });
            }
        });
    }

    // ---------- My Abandonware add-on ----------

    pub(crate) fn abandon_page(&mut self, ui: &mut egui::Ui) {
        if self.ext.ab_games.is_empty() && !self.ext.ab_loading && self.ext.ab_error.is_none() && self.ext.ab_title.is_empty() {
            self.ab_load("Recently added", "recent", 1);
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                heading(ui, "My Abandonware");
                sub(ui, "Browse classic games from myabandonware.com, download them in your browser, and ISObubble scans and installs them.");
            });
        });
        ui.add_space(8.0);

        // Downloads waiting to be installed
        let mut install = None;
        let mut dismiss = None;
        if !self.ext.ab_found.is_empty() || self.ext.ab_unpacking {
            tinted_card(PINK).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(big("📥  Ready to install", 15.0));
                if self.ext.ab_unpacking {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().color(PINK));
                        ui.label(muted("Unpacking…"));
                    });
                }
                for p in &self.ext.ab_found {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).color(TEXT));
                        ui.label(muted(fmt_size(std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))));
                        if bubble_button_ex(ui, "Install with ISObubble", PINK, !self.ext.ab_unpacking, false).clicked() {
                            install = Some(p.clone());
                        }
                        if soft_button(ui, "Dismiss").clicked() {
                            dismiss = Some(p.clone());
                        }
                    });
                }
            });
            ui.add_space(8.0);
        }
        if let Some(p) = install {
            self.ext.ab_found.retain(|x| x != &p);
            self.ext.ab_dismissed.push(p.clone());
            self.ab_install_file(p);
        }
        if let Some(p) = dismiss {
            self.ext.ab_found.retain(|x| x != &p);
            self.ext.ab_dismissed.push(p);
        }

        // Source picker
        let mut load: Option<(String, String)> = None;
        card().inner_margin(egui::Margin::same(14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                for (t, p) in [("Recently added", "recent"), ("Windows games", "browse/platform/windows"), ("DOS games", "browse/platform/dos")] {
                    let on = self.ext.ab_path == p;
                    let r = if on { bubble_button(ui, t, PURPLE) } else { soft_button(ui, t) };
                    if r.clicked() {
                        load = Some((t.into(), p.into()));
                    }
                }
                ui.label(muted("Year:"));
                egui::ComboBox::from_id_salt("ab_year").width(80.0).selected_text(self.ext.ab_year.to_string()).show_ui(ui, |ui| {
                    for y in (1978..=2012).rev() {
                        if ui.selectable_label(self.ext.ab_year == y, y.to_string()).clicked() {
                            self.ext.ab_year = y;
                            load = Some((format!("Games from {y}"), format!("browse/year/{y}")));
                        }
                    }
                });
                if soft_button(ui, "Show year").clicked() {
                    load = Some((format!("Games from {}", self.ext.ab_year), format!("browse/year/{}", self.ext.ab_year)));
                }
                if soft_button(ui, "Install a downloaded file…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_directory(abandon::downloads_dir()).add_filter("Game downloads", &["zip", "iso", "img"]).pick_file() {
                        self.ab_install_file(p);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(muted("A–Z:"));
                for l in "0ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
                    let label = if l == '0' { "#".to_string() } else { l.to_string() };
                    if ui.add(egui::Button::new(RichText::new(&label).size(13.0)).min_size(vec2(24.0, 22.0))).clicked() {
                        load = Some((format!("Games starting with {label}"), format!("browse/name/{l}")));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.ext.ab_search).desired_width(260.0).hint_text("Search the whole site…"));
                if soft_button(ui, "Search on myabandonware.com").clicked() && !self.ext.ab_search.trim().is_empty() {
                    system::open_url(&abandon::search_url(&self.ext.ab_search));
                    self.ext.ab_watch_since = system::now_secs().saturating_sub(5);
                }
                ui.label(muted("Filter this list:"));
                ui.add(egui::TextEdit::singleline(&mut self.ext.ab_filter).desired_width(180.0));
            });
        });
        if let Some((t, p)) = load {
            self.ext.ab_sel = None;
            self.ext.ab_details = None;
            self.ab_load(&t, &p, 1);
        }
        ui.add_space(8.0);

        let h = ui.available_height();
        let w = ui.available_width();
        let mut open: Option<abandon::Game> = None;
        let mut more = false;
        let _ = (w, h);
        columns_plain(ui, 2, |col, ui| {
            if col == 0 {
                ui.horizontal(|ui| {
                    ui.label(big(&self.ext.ab_title, 15.0));
                    if self.ext.ab_loading {
                        ui.add(egui::Spinner::new().color(PURPLE));
                    }
                });
                if let Some(e) = &self.ext.ab_error {
                    ui.label(RichText::new(e).color(RED));
                }
                let f = self.ext.ab_filter.to_lowercase();
                egui::ScrollArea::vertical().id_salt("ab_list").auto_shrink([false, false]).show(ui, |ui| {
                    for g in self.ext.ab_games.iter().filter(|g| f.is_empty() || g.name.to_lowercase().contains(&f)) {
                        let sel = self.ext.ab_sel.as_ref().map(|s| s.url == g.url).unwrap_or(false);
                        let frame = if sel { tinted_card(PURPLE) } else { card().inner_margin(egui::Margin::same(10)) };
                        let r = frame
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(egui::Label::new(big(&g.name, 14.5)).truncate());
                                ui.horizontal(|ui| {
                                    if !g.year.is_empty() {
                                        chip(ui, &g.year, BLUE);
                                    }
                                    if !g.platform.is_empty() {
                                        chip(ui, &g.platform, if g.platform.contains("Windows") { TEAL } else { AMBER });
                                    }
                                    if !g.extra.is_empty() {
                                        ui.add(egui::Label::new(RichText::new(&g.extra).size(12.0).color(MUTED)).truncate());
                                    }
                                });
                            })
                            .response;
                        if r.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            open = Some(g.clone());
                        }
                        ui.add_space(3.0);
                    }
                    if self.ext.ab_more && !self.ext.ab_loading && soft_button(ui, "Load more…").clicked() {
                        more = true;
                    }
                });
            } else {
                egui::ScrollArea::vertical().id_salt("ab_detail").auto_shrink([false, false]).show(ui, |ui| self.ab_detail_panel(ui));
            }
        });
        if let Some(g) = open {
            self.ab_open(g);
        }
        if more {
            let (t, p, n) = (self.ext.ab_title.clone(), self.ext.ab_path.clone(), self.ext.ab_page + 1);
            self.ab_load(&t, &p, n);
        }
    }

    fn ab_detail_panel(&mut self, ui: &mut egui::Ui) {
        let Some(g) = self.ext.ab_sel.clone() else {
            tinted_card(BLUE).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(big("How it works", 15.0));
                for t in [
                    "1. Pick a game on the left to see its downloads.",
                    "2. Press Download — the site's download page opens in your browser.",
                    "3. When the file lands in Downloads, ISObubble offers to install it.",
                    "4. It's unpacked, virus-scanned and installed like a disc, then added to your library.",
                ] {
                    ui.label(RichText::new(t).color(TEXT));
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Games are hosted by myabandonware.com. Check that downloading them is allowed where you live.").size(12.5).color(MUTED));
            });
            return;
        };
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(big(&g.name, 18.0));
            ui.horizontal(|ui| {
                if !g.year.is_empty() {
                    chip(ui, &g.year, BLUE);
                }
                if !g.platform.is_empty() {
                    chip(ui, &g.platform, TEAL);
                }
                if soft_button(ui, "Open game page").clicked() {
                    system::open_url(&g.url);
                }
            });
            if self.ext.ab_details_loading {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().color(PURPLE));
                    ui.label(muted("Loading downloads…"));
                });
            }
            let details = self.ext.ab_details.as_ref().filter(|(u, _)| *u == g.url).map(|(_, d)| d.clone());
            if let Some(d) = details {
                if !d.description.is_empty() {
                    ui.label(RichText::new(&d.description).size(13.5).color(MUTED));
                }
                ui.add_space(4.0);
                if d.downloads.is_empty() {
                    ui.label(muted("No downloads listed on this page — open the game page to see why (some games are no longer offered)."));
                }
                for dl in &d.downloads {
                    ui.horizontal_wrapped(|ui| {
                        if bubble_button(ui, "⬇ Download", PINK).clicked() {
                            system::open_url(&dl.url);
                            self.ext.ab_watch_since = system::now_secs().saturating_sub(5);
                            self.toast("The download opens in your browser. ISObubble will offer to install it when it finishes.", true);
                        }
                        ui.label(big(format!("{} {}", dl.version, dl.kind).trim(), 13.5));
                        if !dl.language.is_empty() {
                            chip(ui, &dl.language, BLUE);
                        }
                        if !dl.size.is_empty() {
                            chip(ui, &dl.size, MUTED);
                        }
                    });
                }
                if g.platform.contains("DOS") {
                    ui.add_space(4.0);
                    ui.label(RichText::new("DOS game: after installing, the Compatibility checker can set it to run in DOSBox.").size(13.0).color(AMBER));
                }
            }
        });
    }

    // ---------- Settings additions ----------

    pub(crate) fn settings_extras(&mut self, ui: &mut egui::Ui) {
        let mut check = false;
        let mut install = false;
        let mut save = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(big("🔄  Updates from GitHub", 16.0));
            ui.label(muted("Link ISObubble to your GitHub project. It checks your Releases for a newer version and installs it."));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.ext.repo_input).desired_width(360.0).hint_text("https://github.com/you/isobubble  or  you/isobubble"));
                if bubble_button(ui, "🔗 Link", PURPLE).clicked() {
                    match update::parse_repo(&self.ext.repo_input) {
                        Some(r) => {
                            self.settings.github_repo = r.clone();
                            self.ext.repo_input = r.clone();
                            save = true;
                            check = true;
                            self.ext.update_msg = Some((format!("Linked to github.com/{r}."), true));
                        }
                        None => self.ext.update_msg = Some(("That doesn't look like a GitHub project address.".into(), false)),
                    }
                }
                if !self.settings.github_repo.is_empty() {
                    if soft_button(ui, "Open on GitHub").clicked() {
                        system::open_url(&format!("https://github.com/{}", self.settings.github_repo));
                    }
                    if soft_button(ui, "Unlink").clicked() {
                        self.settings.github_repo.clear();
                        self.ext.repo_input.clear();
                        self.ext.update = None;
                        save = true;
                    }
                }
            });
            if !self.settings.github_repo.is_empty() {
                ui.horizontal(|ui| {
                    chip(ui, &format!("Linked: {}", self.settings.github_repo), TEAL);
                    if self.settings.last_check > 0 {
                        ui.label(RichText::new(format!("Last checked {}", crashlog::fmt_time(self.settings.last_check as i64))).size(12.5).color(MUTED));
                    }
                });
                ui.horizontal(|ui| {
                    if bubble_button_ex(ui, "Check for updates now", TEAL, !self.ext.update_busy, false).clicked() {
                        check = true;
                    }
                    if self.ext.update_busy {
                        ui.add(egui::Spinner::new().color(TEAL));
                    }
                });
                if ui.checkbox(&mut self.settings.auto_check, "Check automatically (when ISObubble starts and every 6 hours)").changed() {
                    save = true;
                }
                if ui.checkbox(&mut self.settings.auto_install, "Download and install updates automatically").changed() {
                    save = true;
                }
            }
            if let Some(info) = &self.ext.update {
                tinted_card(PINK).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(big(format!("✨ {} is available (you have {})", info.name, system::APP_VERSION), 15.0));
                    if !info.notes.trim().is_empty() {
                        let notes: String = info.notes.chars().take(700).collect();
                        ui.label(RichText::new(notes).size(13.0).color(TEXT));
                    }
                    ui.horizontal(|ui| {
                        if info.asset_url.is_some() {
                            if bubble_button_ex(ui, "⬇ Download & install", PINK, !self.ext.update_busy, false).clicked() {
                                install = true;
                            }
                        } else {
                            ui.label(RichText::new("This release has no ISObubble.exe (or .zip) attached yet.").color(AMBER));
                        }
                        if soft_button(ui, "View on GitHub").clicked() {
                            system::open_url(&info.page);
                        }
                    });
                });
            }
            if let Some((f, s)) = &self.ext.update_progress {
                ui.add(egui::ProgressBar::new(*f).text(s.as_str()).corner_radius(8.0));
            }
            if let Some((m, ok)) = &self.ext.update_msg {
                ui.label(RichText::new(m).color(if *ok { TEAL } else { AMBER }));
            }
            if self.ext.update_ready && bubble_button(ui, "↻ Restart ISObubble now", PURPLE).clicked() {
                update::restart();
            }
            ui.label(
                RichText::new("To publish an update: on GitHub open Releases › Draft a new release, give it a tag higher than the current version (e.g. v1.3.0) and attach ISObubble.exe.")
                    .size(12.5)
                    .color(MUTED),
            );
        });
        if check {
            self.check_updates(true);
        }
        if install {
            self.install_update();
        }

        ui.add_space(10.0);
        let mut startup_change = None;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(big("🫧  Tray & background", 16.0));
            if ui.checkbox(&mut self.settings.tray_while_busy, "Closing the window keeps ISObubble running in the tray (like Steam)").changed() {
                save = true;
            }
            ui.label(RichText::new("Installs, scans and downloads always keep running in the tray if you close the window. Click the tray icon to open ISObubble; right-click it to launch any game in your library.").size(12.5).color(MUTED));
            let mut sw = self.ext.start_with_windows;
            if ui.checkbox(&mut sw, "Start ISObubble with Windows (quietly, in the tray)").changed() {
                startup_change = Some(sw);
            }
            if !tray::is_active() {
                ui.label(RichText::new("The tray icon isn't available in this session.").size(12.5).color(AMBER));
            }
        });
        if let Some(on) = startup_change {
            match system::set_start_with_windows(on) {
                Ok(()) => self.ext.start_with_windows = on,
                Err(e) => self.toast(e, false),
            }
        }

        ui.add_space(10.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(big("🔌  Add-ons", 16.0));
            if ui.checkbox(&mut self.settings.abandonware_addon, "My Abandonware browser — see, download and install classic games from myabandonware.com").changed() {
                save = true;
                if self.settings.abandonware_addon {
                    self.ext.ab_watch_since = system::now_secs();
                }
            }
            if self.settings.abandonware_addon && ui.checkbox(&mut self.settings.watch_downloads, "Watch my Downloads folder and offer to install new game downloads").changed() {
                save = true;
            }
        });
        if save {
            self.settings.save();
        }
        ui.add_space(10.0);
        self.displays_card(ui);
    }
}

/// Shows compatibility results; returns a fix the person clicked.
pub(crate) fn compat_list(ui: &mut egui::Ui, checks: &[compat::Check]) -> Option<compat::Fix> {
    let mut clicked = None;
    let worst = checks.iter().map(|c| c.level).max().unwrap_or(compat::Level::Good);
    let (c, t) = match worst {
        compat::Level::Good | compat::Level::Info => (TEAL, "Should run fine on this PC"),
        compat::Level::Warn => (AMBER, "Should run, with a few things to watch"),
        compat::Level::Bad => (RED, "Something will probably stop it running"),
    };
    tinted_card(c).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(big(t, 18.0));
        for ch in checks {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let label = match ch.level {
                    compat::Level::Good => "OK",
                    compat::Level::Info => "Tip",
                    compat::Level::Warn => "Check",
                    compat::Level::Bad => "Problem",
                };
                chip(ui, label, level_color(ch.level));
                ui.label(big(&ch.title, 14.0));
                if let Some((l, f)) = &ch.fix {
                    if soft_button(ui, l).clicked() {
                        clicked = Some(f.clone());
                    }
                }
            });
            ui.label(RichText::new(&ch.detail).size(13.0).color(MUTED));
        }
    });
    clicked
}

/// Windows compatibility mode + switches + DOSBox for a library game. Returns true if changed.
pub(crate) fn compat_settings(ui: &mut egui::Ui, item: &mut library::Item) -> bool {
    let mut changed = false;
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(big("Compatibility settings for this game", 15.0));
        ui.label(muted("Used every time you press Play — nothing is changed in Windows itself."));
        let tokens: Vec<String> = item.compat.split_whitespace().map(|s| s.to_string()).collect();
        let mode = compat::MODES.iter().skip(1).find(|(k, _)| tokens.iter().any(|t| t == k)).map(|m| m.0).unwrap_or("");
        let mut new_mode = mode;
        ui.horizontal(|ui| {
            ui.label("Run as if on:");
            egui::ComboBox::from_id_salt(("compat_mode", item.id)).width(260.0).selected_text(compat::MODES.iter().find(|m| m.0 == mode).map(|m| m.1).unwrap_or("None")).show_ui(ui, |ui| {
                for (k, l) in compat::MODES {
                    ui.selectable_value(&mut new_mode, *k, *l);
                }
            });
        });
        let mut flags: Vec<(&str, bool)> = compat::FLAGS.iter().map(|(k, _)| (*k, tokens.iter().any(|t| t == k))).collect();
        for (i, (_, label)) in compat::FLAGS.iter().enumerate() {
            ui.checkbox(&mut flags[i].1, *label);
        }
        let mut out: Vec<&str> = Vec::new();
        if !new_mode.is_empty() {
            out.push(new_mode);
        }
        out.extend(flags.iter().filter(|f| f.1).map(|f| f.0));
        let joined = out.join(" ");
        if joined != item.compat {
            item.compat = joined;
            changed = true;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut dos = item.runner.is_some();
            if ui.checkbox(&mut dos, "Run inside DOSBox (for DOS games)").changed() {
                if dos {
                    match compat::find_dosbox() {
                        Some(d) => item.runner = Some(d.to_string_lossy().to_string()),
                        None => system::open_url("https://www.dosbox-staging.org/releases/windows/"),
                    }
                } else {
                    item.runner = None;
                }
                changed = true;
            }
            if let Some(r) = &item.runner {
                ui.label(RichText::new(r).size(12.0).color(MUTED));
            }
        });
    });
    changed
}
