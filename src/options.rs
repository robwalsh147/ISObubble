//! "Game options" window for a library game: which program Play starts, the game's own
//! launcher, launch options, window mode (incl. forced borderless) and a settings-file editor.

use super::*;
use crate::pages::{big, muted};
use egui::{vec2, Align2, FontId, RichText, Sense, Stroke};
use library::WindowMode;

pub struct OptionsState {
    id: u64,
    tab: usize,
    root: PathBuf,
    exes: Vec<gamecfg::ExeInfo>,
    files: Vec<PathBuf>,
    window_keys: Vec<String>,
    cfg: Option<gamecfg::CfgFile>,
    filter: String,
    msg: Option<(String, bool)>,
    draft: library::Item,
    saved_mode: WindowMode,
    monitors: Vec<crate::display::Monitor>,
    hdr: (Option<bool>, String),
    compat: Vec<crate::compat::Check>,
}

const PRESETS: &[(&str, &str)] = &[
    ("-windowed", "Start in a window"),
    ("-noborder", "No window border (many engines)"),
    ("-fullscreen", "Start fullscreen"),
    ("-novid", "Skip intro videos (Source games)"),
    ("-nosplash", "Skip splash screen"),
    ("-skipintro", "Skip intro"),
    ("-dx11", "Use DirectX 11"),
    ("-safe", "Safe mode / default graphics"),
];

fn short(root: &Path, p: &Path) -> String {
    match p.strip_prefix(root) {
        Ok(r) => r.to_string_lossy().to_string(),
        Err(_) => p.to_string_lossy().to_string(),
    }
}

fn scan_window_keys(root: &Path, files: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for f in files {
        if let Ok(c) = gamecfg::load(f) {
            for e in c.window_keys() {
                if out.len() < 40 {
                    let sec = if e.section.is_empty() { String::new() } else { format!("[{}] ", e.section) };
                    out.push(format!("{} — {sec}{} = {}", short(root, f), e.key, e.value));
                }
            }
        }
    }
    out
}

impl OptionsState {
    pub fn set_tab(&mut self, t: usize) {
        self.tab = t.min(3);
        if self.tab == 2 && self.cfg.is_none() {
            if let Some(f) = self.files.first() {
                self.cfg = gamecfg::load(f).ok();
            }
        }
    }
}

impl App {
    pub(crate) fn open_options(&mut self, id: u64) {
        let Some(item) = self.lib.items.iter().find(|i| i.id == id).cloned() else { return };
        let root = item
            .program
            .as_ref()
            .map(|p| gamecfg::game_root(Path::new(p)))
            .or_else(|| item.folder.as_ref().map(PathBuf::from))
            .unwrap_or_default();
        let ok = root.as_os_str().len() > 3 && root.exists();
        let exes = if ok { gamecfg::find_exes(&root) } else { Vec::new() };
        let files = if ok { gamecfg::find_config_files(&root, &item.name) } else { Vec::new() };
        let window_keys = scan_window_keys(&root, &files);
        let files_for_hdr = files.clone();
        let root_for_compat = root.clone();
        let mut draft = item.clone();
        let mut msg = None;
        if draft.launcher.is_none() {
            let main = draft.program.clone().unwrap_or_default().to_lowercase();
            if let Some(l) = exes.iter().find(|e| e.is_launcher && e.path.to_string_lossy().to_lowercase() != main) {
                draft.launcher = Some(l.path.to_string_lossy().to_string());
                msg = Some((format!("Found the game's own launcher: “{}”. It's set as the Launcher — press Save to keep it.", l.name), true));
            }
        }
        if !ok {
            msg = Some(("Choose the game's program first (Launch tab) so ISObubble can find its folder and settings.".into(), false));
        }
        self.opts = Some(OptionsState {
            id,
            tab: 0,
            root,
            exes,
            files,
            window_keys,
            cfg: None,
            filter: String::new(),
            msg,
            saved_mode: item.window_mode,
            monitors: crate::display::monitors(),
            hdr: item
                .program
                .as_ref()
                .map(|p| crate::display::game_hdr_support(Path::new(p), &files_for_hdr))
                .unwrap_or((None, String::new())),
            compat: item
                .program
                .as_ref()
                .map(|p| crate::compat::check_game(Path::new(p), &root_for_compat, item.needs_disc))
                .unwrap_or_default(),
            draft,
        });
    }

    pub(crate) fn options_window(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.opts.take() else { return };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new(RichText::new(format!("⚙  Game options — {}", st.draft.name)).font(FontId::new(18.0, bold())))
            .id(egui::Id::new("game_options"))
            .collapsible(false)
            .resizable(true)
            .default_size([820.0, 600.0])
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (i, t) in ["🚀  Launch", "🖥  Display & window", "🔧  Compatibility", "📝  Settings files"].iter().enumerate() {
                        let r = if st.tab == i { bubble_button(ui, t, PURPLE) } else { soft_button(ui, t) };
                        if r.clicked() {
                            st.tab = i;
                        }
                    }
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height((ui.available_height() - 70.0).max(200.0)).auto_shrink([false, true]).show(ui, |ui| match st.tab {
                    0 => launch_tab(ui, &mut st),
                    1 => display_tab(ui, &mut st),
                    2 => compat_tab(ui, &mut st),
                    _ => files_tab(ui, &mut st),
                });
                ui.add_space(6.0);
                if let Some((m, ok)) = &st.msg {
                    ui.label(RichText::new(m).color(if *ok { TEAL } else { AMBER }));
                }
                ui.horizontal(|ui| {
                    if bubble_button(ui, "✔  Save", PINK).clicked() {
                        save = true;
                    }
                    if soft_button(ui, "Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if save {
            let mut note = Vec::new();
            if let Some(c) = st.cfg.as_mut() {
                if c.dirty() {
                    match c.save() {
                        Ok(m) => note.push(m),
                        Err(e) => note.push(e),
                    }
                }
            }
            if st.draft.window_mode != st.saved_mode && st.draft.window_mode != WindowMode::GameDefault {
                note.push(gamecfg::apply_mode_to_files(&st.files, st.draft.window_mode));
            }
            if let Some(i) = self.lib.items.iter().position(|x| x.id == st.id) {
                if st.draft.program != self.lib.items[i].program {
                    st.draft.folder = st.draft.program.as_ref().and_then(|p| Path::new(p).parent()).map(|p| p.to_string_lossy().to_string());
                }
                self.lib.items[i] = st.draft.clone();
                self.lib.save();
            }
            note.insert(0, format!("Saved options for {}.", st.draft.name));
            self.toast(note.join(" "), true);
        } else if open && !cancel {
            self.opts = Some(st);
        }
    }
}

fn exe_label(root: &Path, e: &gamecfg::ExeInfo) -> String {
    format!("{}{}", short(root, &e.path), if e.is_launcher { "   (launcher / settings)" } else { "" })
}

fn pick_exe(ui: &mut egui::Ui, id: &str, st_root: &Path, exes: &[gamecfg::ExeInfo], value: &mut Option<String>, allow_none: bool) {
    let current = value
        .as_ref()
        .map(|v| short(st_root, Path::new(v)))
        .unwrap_or_else(|| if allow_none { "None".into() } else { "Choose…".into() });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id).width(440.0).selected_text(current).show_ui(ui, |ui| {
            if allow_none && ui.selectable_label(value.is_none(), "None").clicked() {
                *value = None;
            }
            for e in exes {
                let p = e.path.to_string_lossy().to_string();
                let sel = value.as_deref().map(|v| v.eq_ignore_ascii_case(&p)).unwrap_or(false);
                let mut text = RichText::new(exe_label(st_root, e));
                if e.is_launcher {
                    text = text.color(PINK);
                }
                if ui.selectable_label(sel, text).clicked() {
                    *value = Some(p);
                }
            }
        });
        if soft_button(ui, "Browse…").clicked() {
            let mut d = rfd::FileDialog::new().add_filter("Programs", &["exe", "bat", "lnk"]);
            if st_root.exists() {
                d = d.set_directory(st_root);
            }
            if let Some(p) = d.pick_file() {
                *value = Some(p.to_string_lossy().to_string());
            }
        }
    });
}

fn launch_tab(ui: &mut egui::Ui, st: &mut OptionsState) {
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(big("Program that ▶ Play starts", 15.0));
        ui.label(muted("Usually the game itself."));
        let root = st.root.clone();
        pick_exe(ui, "main_exe", &root, &st.exes, &mut st.draft.program, false);
        ui.add_space(8.0);
        ui.label(big("Game's own launcher", 15.0));
        ui.label(muted("Many games ship a launcher or settings program (for example a “… Edition” launcher). It gets its own 🚀 button in your library."));
        pick_exe(ui, "launcher_exe", &root, &st.exes, &mut st.draft.launcher, true);
        if st.draft.launcher.is_none() && !st.exes.iter().any(|e| e.is_launcher) && !st.exes.is_empty() {
            ui.label(RichText::new("No launcher was spotted by name — if the game has one, pick it from the list.").size(13.0).color(MUTED));
        }
    });
    ui.add_space(8.0);
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(big("Launch options", 15.0));
        ui.label(muted("Extra options added when ▶ Play starts the game. Not every game understands every option."));
        ui.add(egui::TextEdit::singleline(&mut st.draft.args).desired_width(f32::INFINITY).hint_text("e.g. -windowed -noborder"));
        ui.horizontal_wrapped(|ui| {
            for (opt, tip) in PRESETS {
                let on = st.draft.args.split_whitespace().any(|a| a.eq_ignore_ascii_case(opt));
                let r = if on { bubble_button(ui, opt, TEAL) } else { soft_button(ui, opt) };
                if r.on_hover_text(*tip).clicked() {
                    let mut parts: Vec<String> = st.draft.args.split_whitespace().map(|s| s.to_string()).collect();
                    if on {
                        parts.retain(|a| !a.eq_ignore_ascii_case(opt));
                    } else {
                        parts.push(opt.to_string());
                    }
                    st.draft.args = parts.join(" ");
                }
            }
        });
        ui.add_space(4.0);
        ui.checkbox(&mut st.draft.needs_disc, "Insert the disc automatically when I press Play");
    });
}

fn display_tab(ui: &mut egui::Ui, st: &mut OptionsState) {
    ui.label(big("How should the game's window look?", 16.0));
    let modes = [
        (WindowMode::GameDefault, "Leave it to the game", "ISObubble doesn't change anything."),
        (WindowMode::Windowed, "Windowed", "Switches the game's settings files to windowed mode."),
        (
            WindowMode::Borderless,
            "Borderless windowed (forced)",
            "Switches the game to windowed, then every time you press Play ISObubble removes the window border and stretches it over the whole screen — looks like fullscreen, but Alt-Tab is instant and other windows can sit on top.",
        ),
        (WindowMode::Fullscreen, "Fullscreen", "Switches the game's settings files back to fullscreen."),
    ];
    for (m, title, text) in modes {
        let selected = st.draft.window_mode == m;
        let c = if m == WindowMode::Borderless { PINK } else { PURPLE };
        let frame = if selected { tinted_card(c) } else { card().inner_margin(egui::Margin::same(14)) };
        let resp = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                    ui.painter().circle_stroke(r.center(), 10.0, Stroke::new(2.0_f32, if selected { c } else { MUTED }));
                    if selected {
                        ui.painter().circle_filled(r.center(), 6.0, c);
                    }
                    ui.label(big(title, 15.0));
                    if m == WindowMode::Borderless {
                        chip(ui, "Popular", PINK);
                    }
                });
                ui.label(muted(text));
            })
            .response;
        if resp.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.draft.window_mode = m;
        }
        ui.add_space(4.0);
    }
    if st.draft.window_mode == WindowMode::Borderless && !st.draft.args.to_lowercase().contains("-windowed") {
        ui.horizontal(|ui| {
            ui.label(muted("Tip: some games also need the -windowed launch option."));
            if soft_button(ui, "Add -windowed").clicked() {
                st.draft.args = format!("{} -windowed", st.draft.args).trim().to_string();
            }
        });
    }
    ui.add_space(6.0);
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(big("Window settings found in the game's files", 15.0));
        if st.window_keys.is_empty() {
            ui.label(muted("None found yet. Many games only create their settings file after the first time they run — play once, then come back. The forced borderless mode works either way."));
        }
        for k in &st.window_keys {
            ui.label(RichText::new(k).size(13.0).color(TEXT));
        }
        if st.draft.window_mode != WindowMode::GameDefault && !st.files.is_empty() && soft_button(ui, "Apply to the settings files now").clicked() {
            let m = gamecfg::apply_mode_to_files(&st.files, st.draft.window_mode);
            st.msg = Some((m, true));
            st.saved_mode = st.draft.window_mode;
            st.window_keys = scan_window_keys(&st.root, &st.files);
            st.cfg = None;
        }
    });
    ui.add_space(6.0);
    monitor_card(ui, st);
}

fn monitor_card(ui: &mut egui::Ui, st: &mut OptionsState) {
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(big("Monitor, resolution & HDR", 15.0));
        let current = st
            .draft
            .monitor
            .as_ref()
            .and_then(|d| st.monitors.iter().find(|m| m.device.eq_ignore_ascii_case(d)))
            .or_else(|| st.monitors.iter().find(|m| m.primary))
            .cloned();
        ui.horizontal(|ui| {
            ui.label("Play on:");
            let sel = current.as_ref().map(|m| m.title()).unwrap_or_else(|| "Main monitor".into());
            egui::ComboBox::from_id_salt("play_monitor").width(380.0).selected_text(sel).show_ui(ui, |ui| {
                for m in &st.monitors {
                    let on = current.as_ref().map(|c| c.device == m.device).unwrap_or(false);
                    if ui.selectable_label(on, format!("{}   ·   {}", m.title(), m.summary())).clicked() {
                        st.draft.monitor = if m.primary { None } else { Some(m.device.clone()) };
                    }
                }
            });
        });
        if let Some(m) = &current {
            ui.horizontal_wrapped(|ui| {
                chip(ui, &format!("{}×{}", m.width, m.height), BLUE);
                chip(ui, &format!("{} Hz", m.hz), TEAL);
                if m.max_hz > m.hz {
                    chip(ui, &format!("can do {} Hz", m.max_hz), AMBER);
                }
                let (t, c) = match (m.hdr_supported, m.hdr_on) {
                    (_, true) => ("HDR on", PINK),
                    (true, false) => ("HDR supported, off in Windows", AMBER),
                    _ => ("No HDR", MUTED),
                };
                chip(ui, t, c);
            });
            if st.draft.window_mode == WindowMode::Borderless {
                ui.label(RichText::new("Borderless mode stretches the game over this monitor.").size(12.5).color(MUTED));
            }
            ui.horizontal(|ui| {
                if !st.files.is_empty() && soft_button(ui, &format!("Set the game to {}×{} @ {} Hz", m.width, m.height, m.hz)).clicked() {
                    let mut n = 0;
                    for f in &st.files {
                        if let Ok(mut c) = gamecfg::load(f) {
                            let k = c.apply_resolution(m.width, m.height, m.hz);
                            if k > 0 && c.save().is_ok() {
                                n += k;
                            }
                        }
                    }
                    st.msg = Some((
                        if n == 0 { "No resolution / refresh-rate settings were found in the game's files — set it in the game's own options menu.".into() } else { format!("Changed {n} setting(s) to {}×{} @ {} Hz (backups kept).", m.width, m.height, m.hz) },
                        n > 0,
                    ));
                    st.window_keys = scan_window_keys(&st.root, &st.files);
                    st.cfg = None;
                }
                if soft_button(ui, "Windows HDR settings").clicked() {
                    system::open_url("ms-settings:display-hdr");
                }
            });
        }
        ui.add_space(4.0);
        let (sup, text) = &st.hdr;
        let (label, c) = match sup {
            Some(true) => ("Game HDR: supported", PINK),
            Some(false) => ("Game HDR: not supported", MUTED),
            None => ("Game HDR: unknown", MUTED),
        };
        ui.horizontal_wrapped(|ui| {
            chip(ui, label, c);
            ui.label(RichText::new(text).size(13.0).color(MUTED));
        });
        if *sup == Some(true) && current.as_ref().map(|m| m.hdr_supported && !m.hdr_on).unwrap_or(false) {
            ui.label(RichText::new("Tip: this monitor supports HDR but it's off in Windows — turn it on for HDR in this game.").size(13.0).color(AMBER));
        }
    });
}

fn compat_tab(ui: &mut egui::Ui, st: &mut OptionsState) {
    if st.compat.is_empty() {
        ui.label(muted("Choose the game's program in the Launch tab first."));
        return;
    }
    if let Some(fix) = crate::extras::compat_list(ui, &st.compat) {
        match fix {
            crate::compat::Fix::Url(u) => system::open_url(u),
            crate::compat::Fix::Layer(l) => {
                for t in l.split_whitespace() {
                    if !st.draft.compat.split_whitespace().any(|x| x == t) {
                        st.draft.compat = format!("{} {t}", st.draft.compat).trim().to_string();
                    }
                }
                st.msg = Some(("Compatibility setting added — press Save to keep it.".into(), true));
            }
            crate::compat::Fix::NeedsDisc => {
                st.draft.needs_disc = true;
                st.msg = Some(("The disc will be inserted on Play — press Save to keep it.".into(), true));
            }
            crate::compat::Fix::UseDosbox => match crate::compat::find_dosbox() {
                Some(d) => {
                    st.draft.runner = Some(d.to_string_lossy().to_string());
                    st.msg = Some(("This game will start inside DOSBox — press Save to keep it.".into(), true));
                }
                None => {
                    system::open_url("https://www.dosbox-staging.org/releases/windows/");
                    st.msg = Some(("Install DOSBox from the page that opened, then press the button again.".into(), false));
                }
            },
        }
    }
    ui.add_space(8.0);
    crate::extras::compat_settings(ui, &mut st.draft);
}

fn files_tab(ui: &mut egui::Ui, st: &mut OptionsState) {
    ui.label(muted("Change any setting the game keeps in its .ini / .cfg files. A backup of each file is kept the first time you save it."));
    let root = st.root.clone();
    let current = st.cfg.as_ref().map(|c| short(&root, &c.path)).unwrap_or_else(|| "Choose a settings file…".into());
    let mut load_path: Option<PathBuf> = None;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("cfg_file").width(460.0).selected_text(current).show_ui(ui, |ui| {
            for f in &st.files {
                if ui.selectable_label(st.cfg.as_ref().map(|c| &c.path == f).unwrap_or(false), short(&root, f)).clicked() {
                    load_path = Some(f.clone());
                }
            }
        });
        if soft_button(ui, "Add a file…").clicked() {
            let mut d = rfd::FileDialog::new().add_filter("Settings files", &["ini", "cfg", "conf", "txt", "prefs"]);
            if root.exists() {
                d = d.set_directory(&root);
            }
            if let Some(p) = d.pick_file() {
                if !st.files.contains(&p) {
                    st.files.push(p.clone());
                }
                load_path = Some(p);
            }
        }
    });
    if let Some(p) = load_path {
        match gamecfg::load(&p) {
            Ok(c) => st.cfg = Some(c),
            Err(e) => st.msg = Some((e, false)),
        }
    }
    if st.files.is_empty() && st.cfg.is_none() {
        tinted_card(BLUE).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(muted("No .ini or .cfg files were found in the game's folder, Documents\\My Games or AppData. Many games create them the first time they run — play once, then come back, or use “Add a file…”."));
        });
        return;
    }
    let Some(cfg) = st.cfg.as_mut() else { return };
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.add(egui::TextEdit::singleline(&mut st.filter).desired_width(220.0).hint_text("e.g. fullscreen"));
        if soft_button(ui, "Open in Notepad").clicked() {
            let mut c = std::process::Command::new("notepad.exe");
            c.arg(&cfg.path);
            let _ = c.spawn();
        }
        if soft_button(ui, "Open folder").clicked() {
            if let Some(p) = cfg.path.parent() {
                system::open_folder(p);
            }
        }
    });
    let f = st.filter.to_lowercase();
    egui::Grid::new("cfg_grid").striped(true).num_columns(3).spacing(vec2(14.0, 4.0)).show(ui, |ui| {
        ui.label(muted("Section"));
        ui.label(muted("Setting"));
        ui.label(muted("Value"));
        ui.end_row();
        for e in cfg.entries.iter_mut().filter(|e| f.is_empty() || e.key.to_lowercase().contains(&f) || e.section.to_lowercase().contains(&f)).take(1500) {
            ui.label(RichText::new(&e.section).size(12.5).color(MUTED));
            let changed = e.changed();
            ui.label(RichText::new(&e.key).size(13.5).color(if changed { PINK } else { TEXT }));
            ui.add(egui::TextEdit::singleline(&mut e.value).desired_width(260.0));
            ui.end_row();
        }
    });
    if cfg.entries.is_empty() {
        ui.label(muted("This file has no “setting = value” lines to edit. Use Open in Notepad instead."));
    }
    ui.horizontal(|ui| {
        if bubble_button_ex(ui, "Save this file", TEAL, cfg.dirty(), false).clicked() {
            match cfg.save() {
                Ok(m) => st.msg = Some((m, true)),
                Err(e) => st.msg = Some((e, false)),
            }
        }
        if cfg.dirty() && soft_button(ui, "Undo changes").clicked() {
            if let Ok(c) = gamecfg::load(&cfg.path.clone()) {
                *cfg = c;
            }
        }
    });
}
