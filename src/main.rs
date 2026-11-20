#![cfg_attr(windows, windows_subsystem = "windows")]

mod abandon;
mod applog;
mod compat;
mod crashlog;
mod describe;
mod discchanger;
mod display;
mod extras;
mod settings;
mod tray;
mod update;
mod gamecfg;
mod options;
mod iso;
mod library;
mod pages;
mod wizard;
mod pe;
mod scan;
mod system;
mod theme;

use describe::{Description, DiscKind};
use iso::{fmt_size, Source, SourceKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use theme::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Library,
    Wizard,
    Scanner,
    Compat,
    Logs,
    Abandon,
    ThisApp,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Wizard,
    Scanner,
}

enum Msg {
    Toast(String, bool),
    Mounted(String, char),
    Ejected(String),
    DiscRead(u64, Result<(Arc<Source>, Description), String>),
    ScannerSource(Result<Arc<Source>, String>),
    Scan(Target, scan::Msg),
    InstallLog(String),
    InstallProgress(f32),
    InstallMounted(usize, char),
    DiscLetters(Vec<(usize, char)>),
    Combined(PathBuf, char),
    InstallDone(Result<(), String>),
    Shortcuts(Vec<(String, String)>),
    SelfInstall(Result<String, String>),
    Ext(extras::ExtMsg),
}

struct DiscSlot {
    id: u64,
    path: PathBuf,
    source: Option<Arc<Source>>,
    desc: Option<Description>,
    error: Option<String>,
}

#[derive(Default)]
struct ScanState {
    running: bool,
    status: String,
    progress: f32,
    findings: Vec<scan::Finding>,
    report: Option<scan::Report>,
    cancel: Arc<AtomicBool>,
}

/// How a multi-disc set is installed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MultiMode {
    /// Every disc mounted; when setup asks, the wanted disc is moved into setup's drive.
    Changer,
    /// All discs merged into one virtual disc, so setup never asks.
    Combine,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Method {
    Mount,
    Copy,
}

struct Wizard {
    step: usize,
    discs: Vec<DiscSlot>,
    next_slot: u64,
    scan: ScanState,
    use_defender: bool,
    accept_risk: bool,
    method: Method,
    dest: String,
    installer: Option<usize>,
    keep_mounted: bool,
    installing: bool,
    install_done: bool,
    install_error: Option<String>,
    log: Vec<String>,
    progress: f32,
    letter: Option<char>,
    current_disc: usize,
    started_at: u64,
    name: String,
    program: String,
    needs_disc: bool,
    suggestions: Vec<(String, String)>,
    searching: bool,
    file_filter: String,
    multi: MultiMode,
    letters: Vec<(usize, char)>,
    combined: Option<PathBuf>,
}

impl Wizard {
    fn new() -> Self {
        Wizard {
            step: 0,
            discs: Vec::new(),
            next_slot: 1,
            scan: ScanState::default(),
            use_defender: true,
            accept_risk: false,
            method: Method::Mount,
            dest: String::new(),
            installer: None,
            keep_mounted: false,
            installing: false,
            install_done: false,
            install_error: None,
            log: Vec::new(),
            progress: 0.0,
            letter: None,
            current_disc: 0,
            started_at: 0,
            name: String::new(),
            program: String::new(),
            needs_disc: false,
            suggestions: Vec::new(),
            searching: false,
            file_filter: String::new(),
            multi: MultiMode::Changer,
            letters: Vec::new(),
            combined: None,
        }
    }
    fn primary(&self) -> Option<(&Arc<Source>, &Description)> {
        let d = self.discs.first()?;
        Some((d.source.as_ref()?, d.desc.as_ref()?))
    }
    fn all_read(&self) -> bool {
        !self.discs.is_empty() && self.discs.iter().all(|d| d.source.is_some())
    }
}

struct Shot {
    path: PathBuf,
    frames: u32,
    requested: bool,
}

struct App {
    page: Page,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    lib: library::Library,
    wiz: Wizard,
    scanner: ScanState,
    scanner_source: Option<Arc<Source>>,
    scanner_loading: bool,
    scanner_defender: bool,
    mounted: HashMap<String, char>,
    busy: HashSet<String>,
    toast: Option<(String, bool, f64)>,
    defender: bool,
    installed: bool,
    self_busy: bool,
    self_msg: Option<(String, bool)>,
    desktop_shortcut: bool,
    shot: Option<Shot>,
    confirm_remove: Option<u64>,
    time: f64,
    /// Test helper (ISOBUBBLE_DEMO_STEP): walk the wizard to this step once discs are read.
    demo_step: Option<usize>,
    /// The open "Game options" window, if any.
    opts: Option<options::OptionsState>,
    settings: settings::Settings,
    ext: extras::Extras,
    hwnd_set: bool,
    start_in_tray: bool,
}

const STEPS: [&str; 6] = ["Insert disc", "What's inside", "Safety scan", "Options", "Install", "Finish"];

fn key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>, open: Vec<PathBuf>) -> Self {
        let mut app = Self::create(cc, shot);
        // Find the game's own launcher for games added before launcher detection existed.
        let mut found = false;
        for it in app.lib.items.iter_mut().filter(|i| i.launcher.is_none()) {
            if let Some(p) = it.program.clone().filter(|p| Path::new(p).exists()) {
                if let Some(l) = gamecfg::detect_launcher(Path::new(&p)) {
                    it.launcher = Some(l);
                    found = true;
                }
            }
        }
        if found {
            app.lib.save();
        }
        update::cleanup_old();
        if app.shot.is_none() {
            tray::create(cc.egui_ctx.clone());
            app.start_in_tray = std::env::args().any(|a| a == "--tray");
        }
        app.startup_extras();
        // Test helper: open on a given page.
        match std::env::var("ISOBUBBLE_DEMO_PAGE").as_deref() {
            Ok("compat") => app.page = Page::Compat,
            Ok("logs") => app.page = Page::Logs,
            Ok("abandon") => app.page = Page::Abandon,
            Ok("settings") => app.page = Page::ThisApp,
            _ => {}
        }
        app.demo_step = std::env::var("ISOBUBBLE_DEMO_STEP").ok().and_then(|s| s.parse().ok());
        for p in open {
            app.add_disc(p);
        }
        // Test helper: show a game (kept in memory only, never saved) and its options window.
        if let Ok(exe) = std::env::var("ISOBUBBLE_DEMO_GAME") {
            let id = app.lib.next_id();
            app.lib.items.push(library::Item {
                id,
                name: "My Game".into(),
                kind: "Game installer".into(),
                icon: "🎮".into(),
                summary: "Game installer for “My Game”".into(),
                discs: vec![],
                program: Some(exe.clone()),
                needs_disc: false,
                folder: None,
                verdict: "Clean".into(),
                added: 0,
                hue: 0.25,
                launcher: gamecfg::detect_launcher(Path::new(&exe)),
                args: String::new(),
                window_mode: library::WindowMode::Borderless,
                monitor: None,
                compat: String::new(),
                runner: None,
            });
            if let Ok(tab) = std::env::var("ISOBUBBLE_DEMO_TAB") {
                app.open_options(id);
                if let Some(o) = app.opts.as_mut() {
                    o.set_tab(tab.parse().unwrap_or(0));
                }
            }
        }
        app
    }

    fn create(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>) -> Self {
        theme::install(&cc.egui_ctx);
        let (tx, rx) = channel();
        App {
            demo_step: None,
            opts: None,
            settings: settings::Settings::load(),
            ext: extras::Extras::default(),
            hwnd_set: false,
            start_in_tray: false,
            page: Page::Library,
            tx,
            rx,
            lib: library::Library::load(),
            wiz: Wizard::new(),
            scanner: ScanState::default(),
            scanner_source: None,
            scanner_loading: false,
            scanner_defender: true,
            mounted: HashMap::new(),
            busy: HashSet::new(),
            toast: None,
            defender: scan::find_defender().is_some(),
            installed: system::is_installed(),
            self_busy: false,
            self_msg: None,
            desktop_shortcut: true,
            shot: shot.map(|path| Shot { path, frames: 0, requested: false }),
            confirm_remove: None,
            time: 0.0,
        }
    }

    fn spawn(&self, f: impl FnOnce(Sender<Msg>) + Send + 'static) {
        let tx = self.tx.clone();
        std::thread::spawn(move || f(tx));
    }

    fn toast(&mut self, s: impl Into<String>, ok: bool) {
        self.toast = Some((s.into(), ok, self.time));
    }

    // ---------- actions ----------

    fn add_disc(&mut self, path: PathBuf) {
        if self.wiz.step > 0 || self.wiz.installing {
            self.toast("Finish or restart the current install before adding discs.", false);
            return;
        }
        if self.wiz.discs.iter().any(|d| key(&d.path) == key(&path)) {
            self.toast("That disc is already in the tray.", false);
            return;
        }
        let path = std::path::absolute(&path).unwrap_or(path);
        let id = self.wiz.next_slot;
        self.wiz.next_slot += 1;
        self.wiz.discs.push(DiscSlot { id, path: path.clone(), source: None, desc: None, error: None });
        self.page = Page::Wizard;
        self.spawn(move |tx| {
            let r = (|| -> Result<(Arc<Source>, Description), String> {
                let mut s = Source::open(&path)?;
                if s.kind == SourceKind::SingleFile {
                    return Err("This file isn't a disc image. Pick an .iso file (or a folder copied from a disc).".into());
                }
                if s.needs_mount_listing() {
                    let l = system::mount_iso(&path)?;
                    s.fill_from_drive(l);
                }
                let d = describe::describe(&s);
                Ok((Arc::new(s), d))
            })();
            let _ = tx.send(Msg::DiscRead(id, r));
        });
    }

    fn browse_discs(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Choose disc image(s) — pick Disc 1 first")
            .add_filter("Disc images", &["iso", "img", "udf"])
            .add_filter("All files", &["*"])
            .pick_files()
        {
            for f in files {
                self.add_disc(f);
            }
        }
    }

    fn start_scan(&mut self, target: Target, sources: Vec<Arc<Source>>, use_defender: bool) {
        let cancel = Arc::new(AtomicBool::new(false));
        let st = ScanState {
            running: true,
            status: "Starting the scan…".into(),
            cancel: cancel.clone(),
            ..Default::default()
        };
        match target {
            Target::Wizard => self.wiz.scan = st,
            Target::Scanner => self.scanner = st,
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            scan::run(sources, use_defender, cancel, |m| {
                match &m {
                    scan::Msg::Status(_, f) => tray::set_status(format!("Scanning for viruses – {:.0}%", f * 100.0)),
                    scan::Msg::Done(_) => tray::set_status(""),
                    _ => {}
                }
                let _ = tx.send(Msg::Scan(target, m));
            })
        });
    }

    fn wizard_scan(&mut self) {
        let srcs = self.wiz.discs.iter().filter_map(|d| d.source.clone()).collect();
        let d = self.wiz.use_defender && self.defender;
        self.start_scan(Target::Wizard, srcs, d);
    }

    fn scanner_pick(&mut self, folder: bool) {
        let pick = if folder {
            rfd::FileDialog::new().set_title("Choose a folder to scan").pick_folder()
        } else {
            rfd::FileDialog::new().set_title("Choose a disc image or file to scan").pick_file()
        };
        if let Some(p) = pick {
            self.scanner_open(p);
        }
    }

    fn scanner_open(&mut self, p: PathBuf) {
        self.scanner_loading = true;
        self.scanner = ScanState::default();
        self.scanner_source = None;
        self.page = Page::Scanner;
        self.spawn(move |tx| {
            let r = Source::open(&p).and_then(|mut s| {
                if s.needs_mount_listing() {
                    let l = system::mount_iso(&p)?;
                    s.fill_from_drive(l);
                }
                Ok(Arc::new(s))
            });
            let _ = tx.send(Msg::ScannerSource(r));
        });
    }

    fn start_install(&mut self) {
        let w = &mut self.wiz;
        w.installing = true;
        w.install_done = false;
        w.install_error = None;
        w.log.clear();
        w.progress = 0.0;
        w.started_at = system::now_secs();
        let sources: Vec<(PathBuf, Arc<Source>)> = w
            .discs
            .iter()
            .filter_map(|d| d.source.clone().map(|s| (d.path.clone(), s)))
            .collect();
        let installer = w
            .installer
            .and_then(|i| w.discs.first()?.desc.as_ref()?.installers.get(i).map(|x| x.path.clone()));
        let method = w.method;
        let multi = w.multi;
        let paths: Vec<PathBuf> = sources.iter().map(|x| x.0.clone()).collect();
        let dest = PathBuf::from(w.dest.trim());
        let title = w.discs.first().and_then(|d| d.source.as_ref()).map(|s| s.info.label.clone()).unwrap_or_default();
        tray::set_status(format!("Installing {title}"));
        applog::log(format!("Install started: {title}"));
        self.spawn(move |tx| {
            let log = |s: String| {
                let _ = tx.send(Msg::InstallLog(s));
            };
            let r = (|| -> Result<(), String> {
                match method {
                    Method::Mount => {
                        let run_setup = |letter: char| -> Result<(), String> {
                            if let Some(inst) = &installer {
                                let prog = PathBuf::from(format!("{letter}:\\{}", inst.replace('/', "\\")));
                                log(format!("Starting the disc's setup: {}", prog.display()));
                                log("Follow the setup window. If Windows asks for permission, choose Yes.".into());
                                let code = system::run_and_wait(&prog, Path::new(&format!("{letter}:\\")))?;
                                match code {
                                    Some(0) | None => log("Setup has closed.".into()),
                                    Some(c) => log(format!("Setup closed with code {c} (it may have been cancelled or need a restart).")),
                                }
                            } else {
                                log(format!("Nothing to run — the disc stays in drive {letter}: so you can open it."));
                            }
                            Ok(())
                        };
                        if sources.len() > 1 && multi == MultiMode::Combine {
                            // Skip disc swapping entirely: one virtual disc holding every disc's files.
                            log(format!("Combining all {} discs into one virtual disc, so setup never has to ask for the next one…", sources.len()));
                            let srcs: Vec<Arc<Source>> = sources.iter().map(|x| x.1.clone()).collect();
                            let iso = discchanger::combine(&srcs, &title, &|f, msg| {
                                let _ = tx.send(Msg::InstallProgress(f * 0.4));
                                tray::set_status(format!("Installing {title} – {msg}"));
                            })?;
                            for (p, _) in &sources {
                                let _ = system::dismount_iso(p);
                            }
                            log(format!("Combined disc ready ({}).", fmt_size(std::fs::metadata(&iso).map(|m| m.len()).unwrap_or(0))));
                            let letter = system::mount_iso(&iso)?;
                            let _ = tx.send(Msg::Combined(iso.clone(), letter));
                            log(format!("All discs are now one disc in drive {letter}:."));
                            let _ = tx.send(Msg::InstallProgress(0.45));
                            tray::set_status(format!("Installing {title}"));
                            run_setup(letter)?;
                        } else if sources.len() > 1 {
                            // Automatic disc changer: every disc mounted, disc 1 in setup's drive.
                            log(format!("Putting all {} discs into virtual drives…", sources.len()));
                            let letters = discchanger::mount_all_with_first(&paths, 0)?;
                            let letter = letters[0].1;
                            for (i, l) in &letters {
                                log(format!("Disc {} → drive {l}:{}", i + 1, if *i == 0 { "  (setup runs from here)" } else { "" }));
                            }
                            let _ = tx.send(Msg::DiscLetters(letters));
                            let _ = tx.send(Msg::InstallProgress(0.35));
                            log("Disc changer is on: when setup asks for another disc, ISObubble swaps it in and presses OK for you.".into());
                            let stop = Arc::new(AtomicBool::new(false));
                            let (tx2, tx3, stop2, paths2) = (tx.clone(), tx.clone(), stop.clone(), paths.clone());
                            let watcher = std::thread::spawn(move || {
                                discchanger::watch(
                                    paths2,
                                    letter,
                                    0,
                                    stop2,
                                    move |v| {
                                        let _ = tx2.send(Msg::DiscLetters(v));
                                    },
                                    move |s| {
                                        applog::log(&s);
                                        let _ = tx3.send(Msg::InstallLog(s));
                                    },
                                )
                            });
                            let r = run_setup(letter);
                            stop.store(true, Ordering::Relaxed);
                            let _ = watcher.join();
                            r?;
                        } else {
                            let (p0, s0) = &sources[0];
                            log("Putting disc 1 into a virtual DVD drive…".into());
                            let letter = match s0.mounted_letter {
                                Some(l) => l,
                                None => system::mount_iso(p0)?,
                            };
                            let _ = tx.send(Msg::InstallMounted(0, letter));
                            log(format!("Disc 1 is now in drive {letter}: — Windows sees it exactly like a real disc."));
                            let _ = tx.send(Msg::InstallProgress(0.35));
                            run_setup(letter)?;
                        }
                    }
                    Method::Copy => {
                        std::fs::create_dir_all(&dest).map_err(|e| format!("Couldn't create {}: {e}", dest.display()))?;
                        let total: u64 = sources.iter().map(|(_, s)| s.total_bytes()).sum::<u64>().max(1);
                        let mut done = 0u64;
                        let mut last = 0u64;
                        for (i, (_, s)) in sources.iter().enumerate() {
                            log(format!("Copying disc {} ({}, {})…", i + 1, s.info.label, fmt_size(s.total_bytes())));
                            // A folder that already is the destination (e.g. an unpacked download) isn't copied again.
                            if s.kind == SourceKind::Folder && key(&s.path) == key(&dest) {
                                log("Files are already in place.".into());
                                continue;
                            }
                            for e in &s.entries {
                                let target = dest.join(e.path.replace('/', "\\"));
                                if e.is_dir {
                                    std::fs::create_dir_all(&target).map_err(|er| er.to_string())?;
                                    continue;
                                }
                                s.copy_out(e, &target, &mut |n| {
                                    done += n;
                                    if done - last > 8 << 20 {
                                        last = done;
                                        let _ = tx.send(Msg::InstallProgress(done as f32 / total as f32 * 0.9));
                                        tray::set_status(format!("Installing {title} – {:.0}%", done as f32 / total as f32 * 90.0));
                                    }
                                })
                                .map_err(|er| format!("Couldn't copy {}: {er}", e.path))?;
                            }
                        }
                        let _ = tx.send(Msg::InstallProgress(0.9));
                        log(format!("All files copied to {}.", dest.display()));
                        if let Some(inst) = installer {
                            let prog = dest.join(inst.replace('/', "\\"));
                            log(format!("Starting setup from the copied files: {}", prog.display()));
                            let code = system::run_and_wait(&prog, prog.parent().unwrap_or(&dest))?;
                            log(match code {
                                Some(0) | None => "Setup has closed.".to_string(),
                                Some(c) => format!("Setup closed with code {c}."),
                            });
                        }
                    }
                }
                Ok(())
            })();
            applog::log(match &r {
                Ok(()) => format!("Install finished: {title}"),
                Err(e) => format!("Install failed: {title}: {e}"),
            });
            tray::set_status(match &r {
                Ok(()) => format!("{title} is installed"),
                Err(_) => String::new(),
            });
            let _ = tx.send(Msg::InstallDone(r));
        });
    }

    fn swap_disc(&mut self, idx: usize) {
        let paths: Vec<PathBuf> = self.wiz.discs.iter().map(|d| d.path.clone()).collect();
        self.wiz.log.push(format!("Swapping to disc {}…", idx + 1));
        self.spawn(move |tx| {
            match discchanger::mount_all_with_first(&paths, idx) {
                Ok(v) => {
                    let l = v[0].1;
                    let _ = tx.send(Msg::DiscLetters(v));
                    let _ = tx.send(Msg::InstallLog(format!(
                        "Disc {} is now in drive {l}:. Go back to the setup window and press OK / Continue.",
                        idx + 1
                    )));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Toast(format!("Couldn't insert disc {}: {e}", idx + 1), false));
                }
            }
        });
    }

    fn enter_finish(&mut self) {
        self.wiz.step = 5;
        let w = &mut self.wiz;
        if let Some((s, d)) = w.discs.first().and_then(|x| Some((x.source.clone()?, x.desc.clone()?))) {
            if w.name.is_empty() {
                w.name = d
                    .installers
                    .first()
                    .and_then(|i| i.product.clone())
                    .filter(|p| p.len() < 60 && !p.to_lowercase().contains("setup"))
                    .unwrap_or_else(|| s.info.label.clone());
            }
            w.needs_disc = w.method == Method::Mount && w.keep_mounted;
            if w.method == Method::Copy {
                let dest = PathBuf::from(w.dest.trim());
                for e in s.files() {
                    let n = e.name().to_lowercase();
                    if e.ext() == "exe"
                        && e.depth() <= 1
                        && !n.contains("setup")
                        && !n.contains("install")
                        && !n.contains("unins")
                        && !n.contains("autorun")
                        && !n.contains("redist")
                    {
                        w.suggestions.push((e.name().to_string(), dest.join(e.path.replace('/', "\\")).to_string_lossy().to_string()));
                    }
                }
                w.suggestions.truncate(6);
                if w.program.is_empty() {
                    if let Some(s) = w.suggestions.first() {
                        w.program = s.1.clone();
                    }
                }
            }
        }
        self.wiz.searching = true;
        let since = self.wiz.started_at;
        self.spawn(move |tx| {
            let _ = tx.send(Msg::Shortcuts(system::find_new_shortcuts(since)));
        });
    }

    fn finish_wizard(&mut self, play: bool) {
        let w = &self.wiz;
        let Some((_, d)) = w.primary() else { return };
        let verdict = match w.scan.report.as_ref().map(|r| r.verdict()) {
            Some(scan::Verdict::Clean) => "Clean",
            Some(scan::Verdict::Caution) => "Caution",
            Some(scan::Verdict::Danger) => "Danger",
            None => "Not scanned",
        };
        let program = w.program.trim().to_string();
        let item = library::Item {
            id: self.lib.next_id(),
            name: if w.name.trim().is_empty() { "Untitled".into() } else { w.name.trim().to_string() },
            kind: d.kind.label().to_string(),
            icon: d.kind.icon().to_string(),
            summary: d.headline.clone(),
            discs: {
                let mut v: Vec<String> = w.discs.iter().map(|d| d.path.to_string_lossy().to_string()).collect();
                // A combined disc is kept for games that check for their disc when they start.
                if let Some(c) = w.combined.as_ref().filter(|_| w.needs_disc || w.keep_mounted) {
                    v.insert(0, c.to_string_lossy().to_string());
                }
                v
            },
            program: (!program.is_empty()).then_some(program.clone()),
            needs_disc: w.needs_disc,
            folder: if w.method == Method::Copy {
                Some(w.dest.trim().to_string())
            } else {
                Path::new(&program).parent().map(|p| p.to_string_lossy().to_string())
            },
            verdict: verdict.into(),
            added: system::now_secs(),
            hue: (self.lib.items.len() as f32 * 0.21 + 0.05).fract(),
            // The game's own launcher (e.g. a "Gamer Edition" launcher), found next to it.
            launcher: if program.is_empty() { None } else { gamecfg::detect_launcher(Path::new(&program)) },
            args: String::new(),
            window_mode: Default::default(),
            monitor: None,
            compat: String::new(),
            runner: None,
        };
        let item_copy = item.clone();
        let keep = (w.method == Method::Mount && w.keep_mounted) || w.needs_disc;
        let paths: Vec<PathBuf> = w.discs.iter().map(|d| d.path.clone()).collect();
        let current = w.current_disc;
        let combined = w.combined.clone();
        let name = item.name.clone();
        self.lib.items.push(item);
        self.lib.save();
        if let Some(c) = combined {
            if !keep {
                self.mounted.remove(&key(&c));
                self.spawn(move |_| {
                    let _ = system::dismount_iso(&c);
                    let _ = std::fs::remove_file(&c);
                });
            }
        }
        for (i, p) in paths.iter().enumerate() {
            if keep && i == current {
                continue;
            }
            if self.mounted.remove(&key(p)).is_some() {
                let p = p.clone();
                self.spawn(move |tx| {
                    let _ = system::dismount_iso(&p);
                    let _ = tx.send(Msg::Ejected(key(&p)));
                });
            }
        }
        if play && !program.is_empty() {
            match start_game(&item_copy, false) {
                Ok(()) => self.toast(format!("Starting {name}…"), true),
                Err(e) => self.toast(format!("Couldn't start it: {e}"), false),
            }
        } else {
            self.toast(format!("{name} was added to your library."), true);
        }
        self.wiz = Wizard::new();
        self.page = Page::Library;
    }

    fn reset_wizard(&mut self) {
        self.wiz.scan.cancel.store(true, Ordering::Relaxed);
        if let Some(c) = self.wiz.combined.take() {
            self.mounted.remove(&key(&c));
            self.spawn(move |_| {
                let _ = system::dismount_iso(&c);
                let _ = std::fs::remove_file(&c);
            });
        }
        for d in &self.wiz.discs {
            if self.mounted.remove(&key(&d.path)).is_some() {
                let p = d.path.clone();
                self.spawn(move |tx| {
                    let _ = system::dismount_iso(&p);
                    let _ = tx.send(Msg::Ejected(key(&p)));
                });
            }
        }
        self.wiz = Wizard::new();
    }

    fn mount_item(&mut self, disc: String, then_launch: Option<(library::Item, bool)>) {
        self.busy.insert(key(Path::new(&disc)));
        self.spawn(move |tx| {
            let p = PathBuf::from(&disc);
            match system::mount_iso(&p) {
                Ok(l) => {
                    let _ = tx.send(Msg::Mounted(key(&p), l));
                    let msg = match then_launch {
                        Some((item, use_launcher)) => match start_game(&item, use_launcher) {
                            Ok(()) => (format!("Disc inserted in {l}: — starting…"), true),
                            Err(e) => (format!("Couldn't start it: {e}"), false),
                        },
                        None => (format!("Disc inserted in drive {l}:"), true),
                    };
                    let _ = tx.send(Msg::Toast(msg.0, msg.1));
                }
                Err(e) => {
                    let _ = tx.send(Msg::Ejected(key(&p)));
                    let _ = tx.send(Msg::Toast(format!("Couldn't insert the disc: {e}"), false));
                }
            }
        });
    }

    fn eject_item(&mut self, disc: String) {
        self.busy.insert(key(Path::new(&disc)));
        self.spawn(move |tx| {
            let p = PathBuf::from(&disc);
            let r = system::dismount_iso(&p);
            let _ = tx.send(Msg::Ejected(key(&p)));
            if let Err(e) = r {
                let _ = tx.send(Msg::Toast(format!("Couldn't eject: {e}"), false));
            }
        });
    }

    fn handle(&mut self, m: Msg) {
        match m {
            Msg::Toast(s, ok) => self.toast(s, ok),
            Msg::Mounted(k, l) => {
                self.busy.remove(&k);
                self.mounted.insert(k, l);
            }
            Msg::Ejected(k) => {
                self.busy.remove(&k);
                self.mounted.remove(&k);
            }
            Msg::DiscRead(id, r) => {
                let mut mounted = None;
                if let Some(s) = self.wiz.discs.iter_mut().find(|d| d.id == id) {
                    match r {
                        Ok((src, d)) => {
                            if let Some(l) = src.mounted_letter {
                                mounted = Some((key(&s.path), l));
                            }
                            s.source = Some(src);
                            s.desc = Some(d);
                        }
                        Err(e) => s.error = Some(e),
                    }
                }
                if let Some((k, l)) = mounted {
                    self.mounted.insert(k, l);
                }
                self.prepare_options();
            }
            Msg::ScannerSource(r) => {
                self.scanner_loading = false;
                match r {
                    Ok(s) => {
                        self.scanner_source = Some(s.clone());
                        let d = self.scanner_defender && self.defender;
                        self.start_scan(Target::Scanner, vec![s], d);
                    }
                    Err(e) => self.toast(e, false),
                }
            }
            Msg::Scan(t, sm) => {
                let st = match t {
                    Target::Wizard => &mut self.wiz.scan,
                    Target::Scanner => &mut self.scanner,
                };
                match sm {
                    scan::Msg::Status(s, f) => {
                        st.status = s;
                        st.progress = f;
                    }
                    scan::Msg::Found(f) => st.findings.push(f),
                    scan::Msg::Done(r) => {
                        st.running = false;
                        st.progress = 1.0;
                        st.report = Some(r);
                    }
                }
            }
            Msg::InstallLog(s) => self.wiz.log.push(s),
            Msg::InstallProgress(f) => self.wiz.progress = f,
            Msg::DiscLetters(v) => {
                if let Some((i, l)) = v.first() {
                    self.wiz.letter = Some(*l);
                    self.wiz.current_disc = *i;
                }
                for d in &self.wiz.discs {
                    self.mounted.remove(&key(&d.path));
                }
                for (i, l) in &v {
                    if let Some(d) = self.wiz.discs.get(*i) {
                        self.mounted.insert(key(&d.path), *l);
                    }
                }
                self.wiz.letters = v;
            }
            Msg::Combined(p, l) => {
                self.wiz.letter = Some(l);
                self.wiz.current_disc = 0;
                self.mounted.insert(key(&p), l);
                self.wiz.combined = Some(p);
            }
            Msg::InstallMounted(i, l) => {
                self.wiz.letter = Some(l);
                self.wiz.current_disc = i;
                for (j, d) in self.wiz.discs.iter().enumerate() {
                    if j == i {
                        self.mounted.insert(key(&d.path), l);
                    } else {
                        self.mounted.remove(&key(&d.path));
                    }
                }
            }
            Msg::InstallDone(r) => {
                self.wiz.installing = false;
                self.wiz.install_done = true;
                self.wiz.progress = 1.0;
                match r {
                    Ok(()) => self.wiz.log.push("✔ Finished. Press Finish to add it to your library.".into()),
                    Err(e) => {
                        self.wiz.log.push(format!("✖ {e}"));
                        self.wiz.install_error = Some(e);
                    }
                }
            }
            Msg::Shortcuts(v) => {
                self.wiz.searching = false;
                for s in v {
                    if !self.wiz.suggestions.iter().any(|x| x.1.eq_ignore_ascii_case(&s.1)) {
                        self.wiz.suggestions.insert(0, s);
                    }
                }
                if self.wiz.program.is_empty() {
                    if let Some(s) = self.wiz.suggestions.first() {
                        self.wiz.program = s.1.clone();
                    }
                }
            }
            Msg::Ext(e) => self.handle_ext(e),
            Msg::SelfInstall(r) => {
                self.self_busy = false;
                self.installed = system::is_installed();
                self.self_msg = Some(match r {
                    Ok(s) => (s, true),
                    Err(e) => (e, false),
                });
            }
        }
    }

    /// Picks sensible defaults for the options step once disc 1 has been read.
    fn prepare_options(&mut self) {
        let Some((s, d)) = self.wiz.primary() else { return };
        let label = system::safe_folder_name(&s.info.label);
        let has_setup = d.installers.first().map(|i| i.score >= 30).unwrap_or(false);
        let kind = d.kind;
        let is_folder = s.kind == SourceKind::Folder;
        if self.wiz.dest.is_empty() {
            self.wiz.dest = system::default_install_root().join(label).to_string_lossy().to_string();
        }
        self.wiz.installer = if has_setup { Some(0) } else { None };
        self.wiz.method = if has_setup && kind != DiscKind::Linux && !is_folder { Method::Mount } else { Method::Copy };
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.time = ctx.input(|i| i.time);
        if !self.hwnd_set {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = frame.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    tray::set_hwnd(w.hwnd.get());
                    self.hwnd_set = true;
                    if self.start_in_tray {
                        tray::hide();
                    }
                }
            }
        }
        // Steam-style: closing the window keeps ISObubble running in the tray.
        if ctx.input(|i| i.viewport().close_requested()) && tray::is_active() && !tray::quitting() && (self.settings.tray_while_busy || self.is_busy()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            tray::hide();
        }
        match tray::take_pending_page() {
            tray::PAGE_LIBRARY => self.page = Page::Library,
            tray::PAGE_INSTALL => self.page = Page::Wizard,
            tray::PAGE_ABANDONWARE => self.page = Page::Abandon,
            tray::PAGE_SETTINGS => self.page = Page::ThisApp,
            _ => {}
        }
        tray::sync_games(&self.lib.items);
        // Never stay stuck at a tiny size (e.g. a size remembered while hidden in the tray).
        if self.time < 3.0 {
            if let Some(r) = ctx.input(|i| i.viewport().inner_rect) {
                if r.width() < 900.0 || r.height() < 600.0 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1240.0, 800.0)));
                }
            }
        }
        self.periodic_extras();
        let msgs: Vec<Msg> = self.rx.try_iter().collect();
        for m in msgs {
            self.handle(m);
        }
        if let Some(target) = self.demo_step {
            if self.wiz.all_read() && self.wiz.step < target.min(3) {
                self.wiz.step += 1;
                if self.wiz.step == 2 {
                    self.wizard_scan();
                }
            }
        }

        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(first) = dropped.first() {
            if self.page == Page::Scanner {
                self.scanner_open(first.clone());
            } else {
                for p in dropped {
                    self.add_disc(p);
                }
            }
        }

        paint_background(&ctx.layer_painter(egui::LayerId::background()), ctx.screen_rect(), self.time);

        egui::SidePanel::left("nav")
            .exact_width(240.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(SIDEBAR).inner_margin(egui::Margin::symmetric(16, 20)))
            .show(ctx, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(30, 24)))
            .show(ctx, |ui| match self.page {
                Page::Library => self.library_page(ui),
                Page::Wizard => self.wizard_page(ui),
                Page::Scanner => self.scanner_page(ui),
                Page::Compat => self.compat_page(ui),
                Page::Logs => self.logs_page(ui),
                Page::Abandon => self.abandon_page(ui),
                Page::ThisApp => {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.this_app_page(ui));
                }
            });
        self.overlays(ctx);
        self.options_window(ctx);

        // --screenshot support
        if let Some(shot) = &mut self.shot {
            shot.frames += 1;
            if shot.frames >= 120 && !shot.requested {
                shot.requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let images: Vec<Arc<egui::ColorImage>> = ctx.input(|i| {
                i.raw
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                    .collect()
            });
            if let Some(img) = images.first() {
                let [w, h] = img.size;
                let mut bytes = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    let [r, g, b, _] = p.to_srgba_unmultiplied();
                    bytes.extend_from_slice(&[r, g, b, 255]);
                }
                if let Some(parent) = shot.path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    let _ = buf.save(&shot.path);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if shot.frames > 900 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint();
    }
}

/// Starts a library game (or its own launcher) with its launch options, and keeps it
/// borderless if that window mode was chosen.
pub(crate) fn start_game(item: &library::Item, use_launcher: bool) -> Result<(), String> {
    let prog = if use_launcher { item.launcher.clone() } else { item.program.clone() }.ok_or("No program chosen yet.")?;
    let args = if use_launcher { "" } else { item.args.as_str() };
    match item.runner.as_ref().filter(|r| !use_launcher && Path::new(r).exists()) {
        // DOS games run inside DOSBox.
        Some(dosbox) => system::launch_full(Path::new(dosbox), &format!("\"{prog}\" -exit {args}"), "", Path::new(&prog).parent())?,
        None => system::launch_full(Path::new(&prog), args, &item.compat, None)?,
    }
    applog::log(format!("Started {}{}", item.name, if use_launcher { " (launcher)" } else { "" }));
    if item.window_mode == library::WindowMode::Borderless && !use_launcher {
        if let Some(main) = &item.program {
            gamecfg::force_borderless(gamecfg::game_root(Path::new(main)), Path::new(main), item.monitor.clone());
        }
    }
    Ok(())
}

fn text_report(input: &Path, defender: bool) -> String {
    let mut src = match Source::open(input) {
        Ok(s) => s,
        Err(e) => return format!("ERROR: {e}\n"),
    };
    let mut mounted_here = false;
    if src.needs_mount_listing() {
        match system::mount_iso(input) {
            Ok(l) => {
                src.fill_from_drive(l);
                mounted_here = true;
            }
            Err(e) => return format!("ERROR mounting: {e}\n"),
        }
    }
    let d = describe::describe(&src);
    let mut out = format!("{}\n\n", d.headline);
    for s in &d.sentences {
        out.push_str(&format!("- {s}\n"));
    }
    out.push('\n');
    for (k, v) in &d.facts {
        out.push_str(&format!("{k}: {v}\n"));
    }
    out.push_str("\nInstallers:\n");
    for i in &d.installers {
        out.push_str(&format!("  {} score={} product={:?} company={:?} version={:?} signed={:?} arch={:?}\n", i.path, i.score, i.product, i.company, i.version, i.signed, i.arch));
    }
    out.push_str("\nFiles:\n");
    for e in &src.entries {
        out.push_str(&format!("  {}{} {}\n", e.path, if e.is_dir { "/" } else { "" }, if e.is_dir { String::new() } else { fmt_size(e.size) }));
    }
    let report = std::sync::Mutex::new(None);
    scan::run(vec![Arc::new(src)], defender, Arc::new(AtomicBool::new(false)), |m| {
        if let scan::Msg::Done(r) = m {
            *report.lock().unwrap() = Some(r);
        }
    });
    if let Some(r) = report.into_inner().unwrap() {
        out.push_str(&format!("\nScan verdict: {:?}  files={} defender={:?}\n", r.verdict(), r.files_scanned, r.defender));
        for f in &r.findings {
            out.push_str(&format!("  [{}] {}: {}\n", f.level.label(), f.file, f.what));
        }
    }
    if mounted_here {
        let _ = system::dismount_iso(input);
    }
    out
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--write-icon") {
        if let Some(out) = args.get(i + 1) {
            let _ = std::fs::write(out, theme::icon_ico_bytes());
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--uninstall") {
        system::self_uninstall_interactive();
        return Ok(());
    }
    // Text report of what's on a disc + a scan, for testing without the window:
    // --report <disc.iso> <out.txt> [--defender]
    if let Some(i) = args.iter().position(|a| a == "--report") {
        if let (Some(input), Some(out)) = (args.get(i + 1), args.get(i + 2)) {
            let _ = std::fs::write(out, text_report(Path::new(input), args.iter().any(|a| a == "--defender")));
        }
        return Ok(());
    }
    let shot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    // Only one ISObubble at a time (like Steam): opening it again shows the running one.
    if shot.is_none() && !tray::claim_single_instance() {
        return Ok(());
    }
    let mut vp = egui::ViewportBuilder::default()
        .with_title(tray::WINDOW_TITLE)
        .with_inner_size([1240.0, 800.0])
        .with_min_inner_size([960.0, 640.0])
        .with_icon(Arc::new(theme::app_icon()));
    if shot.is_some() {
        vp = vp.with_active(false);
        #[cfg(windows)]
        {
            vp = vp.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    // Disc images passed on the command line ("Open with → ISObubble" in Explorer).
    let mut skip_next = false;
    let open: Vec<PathBuf> = args
        .iter()
        .skip(1)
        .filter(|a| {
            if skip_next {
                skip_next = false;
                return false;
            }
            if a.as_str() == "--screenshot" {
                skip_next = true;
                return false;
            }
            !a.starts_with("--") && Path::new(a).exists()
        })
        .map(PathBuf::from)
        .collect();
    // Don't restore a remembered window size: a size saved while the window was hidden in the
    // tray can be tiny. ISObubble always opens at its normal size.
    let opts = eframe::NativeOptions { viewport: vp, persist_window: false, ..Default::default() };
    eframe::run_native("ISObubble", opts, Box::new(move |cc| Ok(Box::new(App::new(cc, shot, open)))))
}
