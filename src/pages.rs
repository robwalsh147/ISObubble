//! Screens: sidebar, library (launcher), scanner, "this app" and overlays.
//! The install wizard steps live in wizard.rs.

use super::*;
use egui::{pos2, vec2, Align, Align2, Color32, FontId, Layout, RichText, Sense, Stroke};

pub(crate) fn big(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, bold())).color(TEXT)
}

pub(crate) fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(MUTED)
}

impl App {
    pub(crate) fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
            disc_bubble(ui.painter(), r.center(), 22.0, PURPLE, self.time);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.label(big("ISObubble", 22.0));
                ui.label(RichText::new("disc installer & launcher").size(12.0).color(MUTED));
            });
        });
        ui.add_space(16.0);
        let mut items = vec![
            (Page::Library, "🏠", "Library"),
            (Page::Wizard, "💿", "Install a disc"),
            (Page::Scanner, "🛡", "Safety scanner"),
            (Page::Compat, "🔧", "Compatibility"),
            (Page::Logs, "📋", "Crash logs"),
        ];
        if self.settings.abandonware_addon {
            items.push((Page::Abandon, "🕹", "Abandonware"));
        }
        items.push((Page::ThisApp, "✨", "Settings & updates"));
        for (page, icon, label) in items {
            let selected = self.page == page;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
            let p = ui.painter();
            if selected {
                p.rect_filled(rect.expand(2.0), 24.0, alpha(PURPLE, 40));
                p.rect_filled(rect, 22.0, mix(PURPLE, PANEL, 0.25));
                p.rect_filled(
                    egui::Rect::from_min_max(rect.min + vec2(22.0, 3.0), pos2(rect.max.x - 22.0, rect.min.y + 9.0)),
                    3.0,
                    Color32::from_white_alpha(14),
                );
                p.circle_filled(rect.left_top() + vec2(14.0, 12.0), 3.0, Color32::from_white_alpha(70));
            } else if resp.hovered() {
                p.rect_filled(rect, 22.0, PANEL2);
            }
            p.text(rect.left_center() + vec2(18.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(18.0), if selected { Color32::WHITE } else { MUTED });
            p.text(rect.left_center() + vec2(48.0, 0.0), Align2::LEFT_CENTER, label, FontId::new(15.5, bold()), if selected { Color32::WHITE } else { TEXT });
            if page == Page::Wizard && !self.wiz.discs.is_empty() {
                let c = rect.right_center() - vec2(20.0, 0.0);
                p.circle_filled(c, 10.0, PINK);
                p.text(c, Align2::CENTER_CENTER, format!("{}", self.wiz.step + 1), FontId::new(11.0, bold()), Color32::WHITE);
            }
            if page == Page::Library && !self.lib.items.is_empty() {
                let c = rect.right_center() - vec2(20.0, 0.0);
                p.circle_filled(c, 10.0, TEAL);
                p.text(c, Align2::CENTER_CENTER, format!("{}", self.lib.items.len()), FontId::new(11.0, bold()), text_on(TEAL));
            }
            if page == Page::ThisApp && self.ext.update.is_some() {
                let c = rect.right_center() - vec2(20.0, 0.0);
                p.circle_filled(c, 10.0, PINK);
                p.text(c, Align2::CENTER_CENTER, "↑", FontId::new(12.0, bold()), Color32::WHITE);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.page = page;
            }
            ui.add_space(2.0);
        }

        let space = ui.available_height() - 190.0;
        if space > 0.0 {
            ui.add_space(space);
        }
        tinted_card(if self.defender { TEAL } else { AMBER }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(big("Protection", 13.5));
            status_dot(ui, self.defender, if self.defender { "Defender engine ready" } else { "Defender not found" });
            status_dot(ui, true, "Built-in malware checks");
            status_dot(ui, self.installed, if self.installed { "Installed on this PC" } else { "Not installed yet" });
        });
        ui.add_space(4.0);
        ui.label(RichText::new(format!("Version {}", system::APP_VERSION)).size(11.5).color(MUTED));
    }

    pub(crate) fn library_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                heading(ui, "Your library");
                sub(ui, "Everything you've installed from disc images. Press Play to launch it.");
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if bubble_button_ex(ui, "+  Install a disc", PINK, true, true).clicked() {
                    self.page = Page::Wizard;
                }
                if soft_button(ui, "+ Add a game").on_hover_text("Add any game already on this PC so it can be launched from ISObubble and the tray").clicked() {
                    self.add_existing_game();
                }
            });
        });
        ui.add_space(16.0);

        if self.lib.items.is_empty() {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.empty_library(ui));
            return;
        }

        let mut action: Option<(u64, &'static str)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(18.0, 18.0);
                for item in &self.lib.items {
                    let color = hue_color(item.hue);
                    let disc_key = item.discs.first().map(|d| key(Path::new(d))).unwrap_or_default();
                    let in_drive = self.mounted.get(&disc_key).copied();
                    let busy = self.busy.contains(&disc_key);
                    ui.allocate_ui_with_layout(vec2(344.0, 300.0), Layout::top_down(Align::LEFT), |ui| {
                        card().show(ui, |ui| {
                            ui.set_width(300.0);
                            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                                disc_bubble(ui.painter(), r.center(), 26.0, color, self.time + item.id as f64);
                                ui.painter().text(r.center(), Align2::CENTER_CENTER, &item.icon, FontId::proportional(13.0), Color32::WHITE);
                                ui.vertical(|ui| {
                                    ui.add(egui::Label::new(big(&item.name, 18.0)).truncate());
                                    ui.horizontal(|ui| {
                                        chip(ui, &item.kind, color);
                                        let vc = match item.verdict.as_str() {
                                            "Clean" => TEAL,
                                            "Caution" => AMBER,
                                            "Danger" => RED,
                                            _ => MUTED,
                                        };
                                        chip(ui, &format!("🛡 {}", item.verdict), vc);
                                    });
                                });
                            });
                            ui.add(egui::Label::new(RichText::new(&item.summary).size(13.0).color(MUTED)).truncate());
                            ui.horizontal(|ui| {
                                let n = item.discs.len();
                                ui.label(RichText::new(format!("💿 {n} disc{}", if n == 1 { "" } else { "s" })).size(12.5).color(MUTED));
                                if let Some(l) = in_drive {
                                    chip(ui, &format!("In drive {l}:"), BLUE);
                                } else if item.needs_disc {
                                    ui.label(RichText::new("· inserts its disc on Play").size(12.5).color(MUTED));
                                }
                                if item.window_mode != library::WindowMode::GameDefault {
                                    chip(ui, item.window_mode.label(), PURPLE);
                                }
                            });
                            if let Some(l) = &item.launcher {
                                let lname = Path::new(l).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                if soft_button(ui, &format!("🚀 Open launcher: {}", trim_name(&lname, 26))).on_hover_text(l.as_str()).clicked() {
                                    action = Some((item.id, "launcher"));
                                }
                            }
                            ui.horizontal(|ui| {
                                if item.program.is_some() {
                                    if bubble_button(ui, "▶  Play", color).clicked() {
                                        action = Some((item.id, "play"));
                                    }
                                } else if bubble_button(ui, "Choose program…", color).clicked() {
                                    action = Some((item.id, "choose"));
                                }
                                if busy {
                                    ui.add(egui::Spinner::new().color(TEXT));
                                } else if in_drive.is_some() {
                                    if soft_button(ui, "⏏ Eject").clicked() {
                                        action = Some((item.id, "eject"));
                                    }
                                } else if soft_button(ui, "💿 Insert").on_hover_text("Put this disc in a virtual drive").clicked() {
                                    action = Some((item.id, "insert"));
                                }
                                if soft_button(ui, "⚙").on_hover_text("Game options: launcher, window mode, settings files").clicked() {
                                    action = Some((item.id, "options"));
                                }
                                ui.menu_button(RichText::new("•••").color(TEXT), |ui| {
                                    for (label, what) in [
                                        ("Game options…", "options"),
                                        ("Change program…", "choose"),
                                        ("Open its folder", "folder"),
                                        ("Scan the disc again", "scan"),
                                        ("Run the disc's setup again", "reinstall"),
                                    ] {
                                        if ui.button(label).clicked() {
                                            action = Some((item.id, what));
                                            ui.close_menu();
                                        }
                                    }
                                    ui.separator();
                                    if ui.button(RichText::new("Remove from library").color(RED)).clicked() {
                                        action = Some((item.id, "remove"));
                                        ui.close_menu();
                                    }
                                });
                            });
                        });
                    });
                }
            });
        });

        if let Some((id, what)) = action {
            self.library_action(id, what);
        }
    }

    fn library_action(&mut self, id: u64, what: &str) {
        let Some(idx) = self.lib.items.iter().position(|i| i.id == id) else { return };
        let item = self.lib.items[idx].clone();
        let disc = item.discs.first().cloned().unwrap_or_default();
        let disc_exists = Path::new(&disc).exists();
        match what {
            "play" | "launcher" => {
                let use_launcher = what == "launcher";
                if item.needs_disc && !self.mounted.contains_key(&key(Path::new(&disc))) && disc_exists {
                    self.mount_item(disc, Some((item.clone(), use_launcher)));
                } else {
                    match start_game(&item, use_launcher) {
                        Ok(()) => self.toast(format!("Starting {}…", item.name), true),
                        Err(e) => self.toast(format!("Couldn't start it: {e}"), false),
                    }
                }
            }
            "options" => self.open_options(id),
            "choose" => {
                let start = item.folder.clone().unwrap_or_else(|| std::env::var("ProgramFiles").unwrap_or_default());
                if let Some(p) = rfd::FileDialog::new().set_directory(start).add_filter("Programs", &["exe", "lnk", "bat"]).pick_file() {
                    self.lib.items[idx].program = Some(p.to_string_lossy().to_string());
                    self.lib.items[idx].folder = p.parent().map(|x| x.to_string_lossy().to_string());
                    self.lib.save();
                }
            }
            "insert" if disc_exists => self.mount_item(disc, None),
            "eject" => self.eject_item(disc),
            "folder" => {
                if let Some(f) = item.folder.as_ref().filter(|f| Path::new(f).exists()) {
                    system::open_folder(Path::new(f));
                } else if let Some(p) = Path::new(&disc).parent() {
                    system::open_folder(p);
                }
            }
            "scan" if disc_exists => self.scanner_open(PathBuf::from(disc)),
            "reinstall" => {
                self.reset_wizard();
                for d in item.discs.iter().filter(|d| Path::new(d).exists()) {
                    self.add_disc(PathBuf::from(d));
                }
                self.page = Page::Wizard;
            }
            "remove" => self.confirm_remove = Some(id),
            _ => self.toast("The disc image file has been moved or deleted.", false),
        }
    }

    fn empty_library(&mut self, ui: &mut egui::Ui) {
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(6.0);
                let (r, _) = ui.allocate_exact_size(vec2(180.0, 170.0), Sense::hover());
                let bob = (self.time * 1.4).sin() as f32 * 5.0;
                disc_bubble(ui.painter(), r.center() + vec2(0.0, bob), 70.0, PURPLE, self.time);
                for (off, rad, c) in [(vec2(76.0, -56.0), 14.0, PINK), (vec2(-80.0, 42.0), 9.0, TEAL), (vec2(-70.0, -60.0), 6.0, BLUE)] {
                    let pt = r.center() + off + vec2(0.0, -bob);
                    ui.painter().circle_filled(pt, rad, alpha(c, 60));
                    ui.painter().circle_stroke(pt, rad, Stroke::new(1.2_f32, alpha(c, 150)));
                    ui.painter().circle_filled(pt + vec2(-rad * 0.35, -rad * 0.35), rad * 0.22, Color32::from_white_alpha(90));
                }
                ui.add_space(8.0);
                ui.label(big("No discs yet", 26.0));
                ui.label(
                    RichText::new("Pick an ISO disc image and ISObubble will read it, check it for viruses,\nthen install it exactly like the real disc in a drive.")
                        .size(15.0)
                        .color(MUTED),
                );
                ui.add_space(12.0);
                if bubble_button_ex(ui, "💿  Insert your first disc", PURPLE, true, true).clicked() {
                    self.browse_discs();
                }
                ui.add_space(4.0);
                ui.label(RichText::new("…or drag an .iso file anywhere onto this window").size(13.0).color(MUTED));
                ui.add_space(12.0);
            });
        });
        ui.add_space(18.0);
        let steps = [
            (PURPLE, "1", "Read the disc", "See what's on it and what will be installed — before anything runs."),
            (TEAL, "2", "Scan for threats", "Microsoft Defender plus built-in trojan & malware checks."),
            (PINK, "3", "Install like the real disc", "The image goes into a virtual drive and its own setup starts."),
            (BLUE, "4", "Launch from here", "Everything you install lands in your library with a Play button."),
        ];
        columns_plain(ui, 4, |i, ui| {
            {
                let (c, n, t, d) = &steps[i];
                tinted_card(*c).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 15.0, *c);
                        ui.painter().circle_filled(r.center() + vec2(-5.0, -6.0), 4.0, Color32::from_white_alpha(70));
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, *n, FontId::new(15.0, bold()), text_on(*c));
                        ui.label(big(*t, 15.0));
                    });
                    ui.label(RichText::new(*d).size(13.0).color(MUTED));
                });
            }
        });
    }

    pub(crate) fn scanner_page(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Safety scanner");
        sub(ui, "Check any disc image, program or folder for viruses, trojans and other malware before you open it.");
        ui.add_space(12.0);
        let mut pick: Option<bool> = None;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let busy = self.scanner.running || self.scanner_loading;
                if bubble_button_ex(ui, "🛡  Scan a file or disc image…", TEAL, !busy, false).clicked() {
                    pick = Some(false);
                }
                if bubble_button_ex(ui, "📁  Scan a folder…", BLUE, !busy, false).clicked() {
                    pick = Some(true);
                }
                ui.checkbox(&mut self.scanner_defender, "Use Microsoft Defender too");
            });
            ui.label(
                RichText::new("Two layers: Microsoft Defender's full virus database (built into Windows) and ISObubble's own checks for disguised programs, dangerous scripts, code injection, keyloggers, ransomware notes and known-bad fingerprints.")
                    .size(13.0)
                    .color(MUTED),
            );
        });
        if let Some(folder) = pick {
            self.scanner_pick(folder);
        }
        ui.add_space(12.0);
        if self.scanner_loading {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().color(TEAL));
                ui.label("Reading…");
            });
        }
        if let Some(s) = &self.scanner_source {
            ui.label(muted(format!("Scanning: {}", s.path.display())));
        }
        let rescan = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.scanner.running || self.scanner.report.is_some() {
                    crate::wizard::scan_view(ui, &self.scanner, self.time)
                } else {
                    tinted_card(BLUE).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(big("Tip", 15.0));
                        ui.label(muted("You can also drag a file onto this page to scan it."));
                    });
                    false
                }
            })
            .inner;
        if rescan {
            if let Some(s) = self.scanner_source.clone() {
                let d = self.scanner_defender && self.defender;
                self.start_scan(Target::Scanner, vec![s], d);
            }
        }
    }

    pub(crate) fn this_app_page(&mut self, ui: &mut egui::Ui) {
        heading(ui, "ISObubble on this computer");
        sub(ui, "Install the app so it lives in your Start menu like any other program.");
        ui.add_space(12.0);
        let from_install = system::running_from_install();
        let mut do_install = false;
        let mut do_uninstall = false;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(86.0, 86.0), Sense::hover());
                disc_bubble(ui.painter(), r.center(), 38.0, PURPLE, self.time);
                ui.vertical(|ui| {
                    ui.label(big("ISObubble", 22.0));
                    if self.installed {
                        chip(ui, "✔ Installed", TEAL);
                        ui.label(RichText::new(format!("Location: {}", system::install_dir().display())).size(13.0).color(MUTED));
                    } else {
                        chip(ui, "Not installed — running from its download folder", AMBER);
                    }
                });
            });
            ui.add_space(8.0);
            ui.checkbox(&mut self.desktop_shortcut, "Also put a shortcut on my desktop");
            ui.horizontal(|ui| {
                let label = if self.installed { "Reinstall / update" } else { "⬇  Install ISObubble on this PC" };
                if bubble_button_ex(ui, label, PURPLE, !self.self_busy, true).clicked() {
                    do_install = true;
                }
                if self.installed && soft_button(ui, "Uninstall").clicked() {
                    do_uninstall = true;
                }
                if self.self_busy {
                    ui.add(egui::Spinner::new().color(PURPLE));
                }
            });
            if let Some((m, ok)) = &self.self_msg {
                ui.label(RichText::new(m).color(if *ok { TEAL } else { RED }));
            }
            ui.label(
                RichText::new("Installs just for you (no admin needed): adds a Start-menu entry and an entry in Settings › Apps so it can be uninstalled cleanly. Your library is kept either way.")
                    .size(13.0)
                    .color(MUTED),
            );
        });
        if do_install {
            self.self_busy = true;
            let desk = self.desktop_shortcut;
            self.spawn(move |tx| {
                let _ = tx.send(Msg::SelfInstall(system::self_install(desk)));
            });
        }
        if do_uninstall {
            match system::self_uninstall() {
                Ok(()) => {
                    self.installed = false;
                    if from_install {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        self.self_msg = Some(("ISObubble was removed from this PC.".into(), true));
                    }
                }
                Err(e) => self.self_msg = Some((e, false)),
            }
        }
        ui.add_space(12.0);
        let panels = [
            (
                TEAL,
                "🛡  How it keeps you safe",
                [
                    "Every disc is read first — nothing runs until you've seen what it is.",
                    "Each file is checked for disguised programs, dangerous scripts, trojan behaviour and ransomware text.",
                    "Microsoft Defender scans the whole image with its up-to-date virus database.",
                    "Setup programs show whether they're digitally signed and by whom.",
                ],
            ),
            (
                BLUE,
                "💿  How installing works",
                [
                    "Your ISO goes into a Windows virtual DVD drive — programs see a real disc.",
                    "The disc's own install wizard runs, just like with the physical disc.",
                    "Multi-disc sets: swap discs from the disc tray when setup asks.",
                    "Games that check for their disc can have it inserted automatically on Play.",
                ],
            ),
        ];
        columns_plain(ui, 2, |i, ui| {
            let (c, title, lines) = &panels[i];
            tinted_card(*c).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(big(*title, 16.0));
                for t in lines {
                    ui.label(RichText::new(format!("•  {t}")).color(TEXT));
                }
            });
        });
        ui.add_space(8.0);
        ui.label(RichText::new(format!("Your library is saved in {}", system::data_dir().display())).size(12.5).color(MUTED));
        ui.add_space(12.0);
        self.settings_extras(ui);
    }

    pub(crate) fn overlays(&mut self, ctx: &egui::Context) {
        if let Some((msg, ok, t0)) = self.toast.clone() {
            if self.time - t0 > 5.0 {
                self.toast = None;
            } else {
                let c = if ok { TEAL } else { RED };
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(Align2::RIGHT_BOTTOM, vec2(-24.0, -24.0))
                    .order(egui::Order::Foreground)
                    .show(ctx, |ui| {
                        tinted_card(c).fill(mix(PANEL, c, 0.2)).show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.label(RichText::new(msg).color(TEXT));
                        });
                    });
            }
        }
        if let Some(id) = self.confirm_remove {
            let name = self.lib.items.iter().find(|i| i.id == id).map(|i| i.name.clone()).unwrap_or_default();
            let mut close = false;
            egui::Window::new("Remove from library?")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(format!("Remove “{name}” from your library? The installed program and the disc image file stay on your PC."));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if bubble_button(ui, "Remove", RED).clicked() {
                            self.lib.items.retain(|i| i.id != id);
                            self.lib.save();
                            close = true;
                        }
                        if soft_button(ui, "Keep it").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.confirm_remove = None;
            }
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering && self.page != Page::Wizard {
            egui::Area::new(egui::Id::new("drop"))
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    tinted_card(PINK).show(ui, |ui| {
                        ui.label(big(if self.page == Page::Scanner { "Drop to scan it" } else { "Drop to insert the disc" }, 22.0));
                    });
                });
        }
    }
}

pub(crate) fn status_dot(ui: &mut egui::Ui, ok: bool, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.5, if ok { TEAL } else { AMBER });
        ui.label(RichText::new(text).size(12.5).color(MUTED));
    });
}

fn trim_name(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}
