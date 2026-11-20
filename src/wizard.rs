//! The six-step install wizard and the shared scan-results view.

use super::*;
use crate::pages::{big, muted};
use egui::{pos2, vec2, Align, Align2, Color32, FontId, Layout, RichText, Sense, Stroke};

impl App {
    fn step_bar(&self, ui: &mut egui::Ui) {
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(w, 66.0), Sense::hover());
        let p = ui.painter();
        let n = STEPS.len();
        let gap = w / n as f32;
        let step = self.wiz.step;
        for i in 0..n {
            let cx = rect.left() + gap * (i as f32 + 0.5);
            let c = pos2(cx, rect.top() + 22.0);
            if i + 1 < n {
                let c2 = pos2(cx + gap, c.y);
                p.line_segment([c + vec2(22.0, 0.0), c2 - vec2(22.0, 0.0)], Stroke::new(3.0_f32, if i < step { TEAL } else { BORDER }));
            }
            let (fill, txt) = if i < step {
                (TEAL, "✔".to_string())
            } else if i == step {
                (PURPLE, format!("{}", i + 1))
            } else {
                (PANEL2, format!("{}", i + 1))
            };
            if i == step {
                let pulse = 1.0 + (self.time * 3.0).sin() as f32 * 0.08;
                p.circle_filled(c, 24.0 * pulse, alpha(PURPLE, 50));
            }
            p.circle_filled(c, 18.0, fill);
            p.circle_stroke(c, 18.0, Stroke::new(1.0_f32, Color32::from_white_alpha(40)));
            p.circle_filled(c + vec2(-6.0, -7.0), 4.5, Color32::from_white_alpha(60));
            p.text(c, Align2::CENTER_CENTER, txt, FontId::new(15.0, bold()), if i > step { MUTED } else { text_on(fill) });
            p.text(pos2(cx, rect.top() + 56.0), Align2::CENTER_CENTER, STEPS[i], FontId::new(12.5, bold()), if i == step { TEXT } else { MUTED });
        }
    }

    pub(crate) fn wizard_page(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Install from a disc image");
        sub(ui, "Works just like putting the real disc in the drive — but every disc is read and checked first.");
        ui.add_space(10.0);
        self.step_bar(ui);
        ui.add_space(8.0);

        let body_h = (ui.available_height() - 64.0).max(100.0);
        egui::ScrollArea::vertical()
            .id_salt(("wiz", self.wiz.step))
            .max_height(body_h)
            .auto_shrink([false, false])
            .show(ui, |ui| match self.wiz.step {
                0 => self.step_insert(ui),
                1 => self.step_inside(ui),
                2 => self.step_scan(ui),
                3 => self.step_options(ui),
                4 => self.step_install(ui),
                _ => self.step_finish(ui),
            });

        ui.add_space(8.0);
        let step = self.wiz.step;
        let mut go: Option<&str> = None;
        ui.horizontal(|ui| {
            if step > 0 && !self.wiz.installing && soft_button(ui, "↺  Start over").clicked() {
                go = Some("reset");
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (label, enabled) = match step {
                    0 => ("Read the disc  ›", self.wiz.all_read()),
                    1 => ("Scan for threats  ›", true),
                    2 => (
                        "Continue  ›",
                        !self.wiz.scan.running
                            && self.wiz.scan.report.as_ref().map(|r| r.verdict() != scan::Verdict::Danger || self.wiz.accept_risk).unwrap_or(false),
                    ),
                    3 => ("Install  ›", self.wiz.method == Method::Mount || !self.wiz.dest.trim().is_empty()),
                    4 => ("Finish  ›", self.wiz.install_done),
                    _ => ("Add to library  ✔", true),
                };
                if bubble_button_ex(ui, label, if step == 3 { PINK } else { PURPLE }, enabled, true).clicked() {
                    go = Some("next");
                }
                if step == 5 && soft_button(ui, "▶  Add & play now").clicked() {
                    go = Some("play");
                }
                if step > 0 && step < 4 && soft_button(ui, "‹  Back").clicked() {
                    go = Some("back");
                }
            });
        });
        match go {
            Some("reset") => self.reset_wizard(),
            Some("back") => self.wiz.step -= 1,
            Some("play") => self.finish_wizard(true),
            Some("next") => match step {
                0 => self.wiz.step = 1,
                1 => {
                    self.wiz.step = 2;
                    if self.wiz.scan.report.is_none() && !self.wiz.scan.running {
                        self.wizard_scan();
                    }
                }
                2 => self.wiz.step = 3,
                3 => {
                    self.wiz.step = 4;
                    self.start_install();
                }
                4 => self.enter_finish(),
                _ => self.finish_wizard(false),
            },
            _ => {}
        }
    }

    fn step_insert(&mut self, ui: &mut egui::Ui) {
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 220.0), Sense::click());
        {
            let p = ui.painter();
            let col = if hovering { PINK } else if resp.hovered() { PURPLE } else { BORDER };
            p.rect_filled(rect, 30.0, alpha(PANEL, 220));
            let inner = rect.shrink(8.0);
            let per = 2.0 * (inner.width() + inner.height());
            let count = (per / 16.0) as usize;
            let t = self.time as f32 * 12.0;
            for k in 0..count {
                let d = (k as f32 * 16.0 + t).rem_euclid(per);
                p.circle_filled(perimeter_point(inner, d / per), 2.6, alpha(col, 200));
            }
            let bob = (self.time * 1.6).sin() as f32 * 4.0;
            disc_bubble(p, pos2(rect.center().x, rect.top() + 76.0 + bob), 46.0, if hovering { PINK } else { PURPLE }, self.time);
            p.text(
                pos2(rect.center().x, rect.top() + 150.0),
                Align2::CENTER_CENTER,
                if hovering { "Let go to insert the disc" } else { "Drop your ISO disc image here" },
                FontId::new(20.0, bold()),
                TEXT,
            );
            p.text(
                pos2(rect.center().x, rect.top() + 180.0),
                Align2::CENTER_CENTER,
                "or click to browse — you can add several discs for multi-disc sets",
                FontId::proportional(14.0),
                MUTED,
            );
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.browse_discs();
        }
        ui.add_space(14.0);

        if self.wiz.discs.is_empty() {
            tinted_card(BLUE).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(big("💡  Nothing runs yet", 15.0));
                ui.label(muted("ISObubble only reads the disc image at this point. Nothing is installed or started until you've seen what's inside and the safety scan is done."));
            });
            return;
        }
        ui.label(big("Disc tray", 17.0));
        let mut remove = None;
        let mut move_up = None;
        for (i, d) in self.wiz.discs.iter().enumerate() {
            card().inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                    let c = if d.error.is_some() { RED } else if d.source.is_some() { TEAL } else { PURPLE };
                    disc_bubble(ui.painter(), r.center(), 18.0, c, self.time + i as f64);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(big(format!("Disc {}", i + 1), 15.0));
                            ui.label(muted(d.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()));
                        });
                        if let Some(e) = &d.error {
                            ui.label(RichText::new(e).color(RED));
                        } else if let (Some(s), Some(desc)) = (&d.source, &d.desc) {
                            ui.horizontal(|ui| {
                                chip(ui, &format!("✔ {}", s.info.label), TEAL);
                                ui.label(RichText::new(format!("{} · {} files · {}", desc.kind.label(), desc.file_count, fmt_size(s.file_size))).size(13.0).color(MUTED));
                            });
                        } else {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().color(PURPLE));
                                ui.label(muted("Reading the disc…"));
                            });
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if soft_button(ui, "✕").on_hover_text("Take this disc out").clicked() {
                            remove = Some(i);
                        }
                        if i > 0 && soft_button(ui, "▲").on_hover_text("Move up (Disc 1 should be first)").clicked() {
                            move_up = Some(i);
                        }
                    });
                });
            });
        }
        if let Some(i) = remove {
            let d = self.wiz.discs.remove(i);
            if self.mounted.remove(&key(&d.path)).is_some() {
                self.spawn(move |_| {
                    let _ = system::dismount_iso(&d.path);
                });
            }
            self.wiz.dest.clear();
            self.prepare_options();
        }
        if let Some(i) = move_up {
            self.wiz.discs.swap(i, i - 1);
            self.wiz.dest.clear();
            self.prepare_options();
        }
        ui.horizontal(|ui| {
            if soft_button(ui, "+ Add another disc").clicked() {
                self.browse_discs();
            }
            ui.label(RichText::new("Multi-disc set? Add every disc in order, Disc 1 first.").size(13.0).color(MUTED));
        });
    }

    fn step_inside(&mut self, ui: &mut egui::Ui) {
        let Some((src, d)) = self.wiz.primary().map(|(s, d)| (s.clone(), d.clone())) else {
            ui.label("Waiting for the disc to be read…");
            return;
        };
        let kc = kind_color(d.kind);
        tinted_card(kc).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(70.0, 70.0), Sense::hover());
                disc_bubble(ui.painter(), r.center(), 32.0, kc, self.time);
                ui.painter().text(r.center(), Align2::CENTER_CENTER, d.kind.icon(), FontId::proportional(15.0), Color32::WHITE);
                ui.vertical(|ui| {
                    ui.label(RichText::new("What is this?").size(13.0).color(kc));
                    ui.add(egui::Label::new(big(&d.headline, 21.0)).wrap());
                });
            });
            ui.add_space(4.0);
            for s in &d.sentences {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("•").color(kc));
                    ui.label(RichText::new(s).size(15.0).color(TEXT));
                });
            }
        });
        ui.add_space(12.0);

        columns_plain(ui, 2, |col, ui| {
            if col == 0 {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(big("Disc details", 16.0));
                egui::Grid::new("facts").num_columns(2).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
                    for (k, v) in &d.facts {
                        ui.label(muted(k));
                        ui.add(egui::Label::new(RichText::new(v).color(TEXT)).wrap());
                        ui.end_row();
                    }
                });
            });
            } else {
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(big("What's taking up the space", 16.0));
                let total = d.categories.iter().map(|c| c.2).sum::<u64>().max(1);
                let colors = [PURPLE, PINK, TEAL, BLUE, AMBER, MUTED];
                for (i, (name, count, bytes)) in d.categories.iter().take(6).enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).size(13.5).color(TEXT));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{} · {count} files", fmt_size(*bytes))).size(12.5).color(MUTED));
                        });
                    });
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
                    ui.painter().rect_filled(r, 5.0, PANEL2);
                    let mut fr = r;
                    fr.set_width(r.width() * (*bytes as f32 / total as f32).max(0.02));
                    ui.painter().rect_filled(fr, 5.0, colors[i % colors.len()]);
                }
            });
            }
        });
        ui.add_space(12.0);

        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(big("Setup programs found", 16.0));
            if d.installers.is_empty() {
                ui.label(muted("No setup program on this disc. You can still copy its files to a folder."));
            }
            for (i, inst) in d.installers.iter().take(5).enumerate() {
                ui.horizontal_wrapped(|ui| {
                    if i == 0 && inst.score >= 30 {
                        chip(ui, "Main setup", PINK);
                    }
                    ui.label(big(inst.path.replace('/', "\\"), 14.0));
                    if let Some(p) = inst.product.as_ref().or(inst.file_desc.as_ref()) {
                        ui.label(muted(p));
                    }
                    if let Some(v) = &inst.version {
                        ui.label(muted(format!("v{v}")));
                    }
                    match inst.signed {
                        Some(true) => {
                            chip(ui, "Signed", TEAL);
                        }
                        Some(false) => {
                            chip(ui, "Not signed", AMBER);
                        }
                        None => {}
                    }
                    if let Some(a) = &inst.arch {
                        chip(ui, a, BLUE);
                    }
                });
            }
        });

        for (i, slot) in self.wiz.discs.iter().enumerate().skip(1) {
            if let (Some(s), Some(dd)) = (&slot.source, &slot.desc) {
                ui.add_space(8.0);
                card().inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(big(format!("Disc {} — {}", i + 1, s.info.label), 15.0));
                    ui.label(muted(format!("{} files in {} folders, {}. {}", dd.file_count, dd.folder_count, fmt_size(s.total_bytes()), dd.kind.label())));
                });
            }
        }

        ui.add_space(8.0);
        egui::CollapsingHeader::new(big(format!("Browse all {} files on disc 1", d.file_count), 15.0))
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.add(egui::TextEdit::singleline(&mut self.wiz.file_filter).desired_width(260.0).hint_text("e.g. .exe"));
                });
                let f = self.wiz.file_filter.to_lowercase();
                let rows: Vec<_> = src.entries.iter().filter(|e| f.is_empty() || e.path.to_lowercase().contains(&f)).take(3000).collect();
                egui::Grid::new("files").striped(true).num_columns(2).spacing(vec2(20.0, 3.0)).show(ui, |ui| {
                    for e in rows {
                        let icon = if e.is_dir { "📁" } else { "📄" };
                        ui.label(RichText::new(format!("{icon} {}", e.path.replace('/', "\\"))).size(13.0).color(if e.is_dir { BLUE } else { TEXT }));
                        ui.label(RichText::new(if e.is_dir { String::new() } else { fmt_size(e.size) }).size(12.5).color(MUTED));
                        ui.end_row();
                    }
                });
            });
    }

    fn step_scan(&mut self, ui: &mut egui::Ui) {
        if self.wiz.scan.report.is_none() && !self.wiz.scan.running {
            let mut start = false;
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(big("Ready to scan", 18.0));
                ui.checkbox(&mut self.wiz.use_defender, "Also use Microsoft Defender's virus database (recommended)");
                start = bubble_button(ui, "🛡  Start the scan", TEAL).clicked();
            });
            if start {
                self.wizard_scan();
            }
            return;
        }
        let rescan = scan_view(ui, &self.wiz.scan, self.time);
        if let Some(r) = &self.wiz.scan.report {
            if r.verdict() == scan::Verdict::Danger {
                ui.add_space(8.0);
                tinted_card(RED).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("We strongly suggest not installing this disc.").font(FontId::new(15.0, bold())).color(RED));
                    ui.checkbox(&mut self.wiz.accept_risk, "I trust where this disc came from and want to continue anyway");
                });
            }
        }
        if rescan {
            self.wizard_scan();
        }
    }

    fn step_options(&mut self, ui: &mut egui::Ui) {
        let Some(d) = self.wiz.primary().map(|(_, d)| d.clone()) else { return };
        match d.kind {
            DiscKind::WindowsSetup => {
                tinted_card(AMBER).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("⚠  This is Windows itself").font(FontId::new(15.0, bold())).color(AMBER));
                    ui.label(RichText::new("Running its setup from here upgrades or reinstalls Windows on this PC. Back up your files first. For a clean install, write it to a USB stick instead.").color(TEXT));
                });
                ui.add_space(10.0);
            }
            DiscKind::Linux => {
                tinted_card(BLUE).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Linux discs can't be installed inside Windows").font(FontId::new(15.0, bold())).color(BLUE));
                    ui.label(RichText::new("You can copy its files to a folder, or insert it as a virtual disc to look around.").color(TEXT));
                });
                ui.add_space(10.0);
            }
            _ => {}
        }
        ui.label(big("How should it be installed?", 17.0));
        let options = [
            (Method::Mount, "💿", "Insert as a virtual disc", "Recommended", "Windows puts the image in a virtual DVD drive and the disc's own setup runs — exactly as if you had the physical disc in the drive. Multi-disc sets can be swapped during setup."),
            (Method::Copy, "📁", "Copy the disc to a folder", "", "Every file is copied into a folder on your PC first, then setup runs from there (if you pick one). Good for discs with no setup program, or games that run straight from their folder."),
        ];
        for (m, icon, title, badge, text) in options {
            let selected = self.wiz.method == m;
            let c = if m == Method::Mount { PURPLE } else { TEAL };
            let frame = if selected { tinted_card(c) } else { card().inner_margin(egui::Margin::same(16)) };
            let resp = frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                        ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(2.0_f32, if selected { c } else { MUTED }));
                        if selected {
                            ui.painter().circle_filled(r.center(), 6.5, c);
                        }
                        ui.label(big(format!("{icon}  {title}"), 16.0));
                        if !badge.is_empty() {
                            chip(ui, badge, PINK);
                        }
                    });
                    ui.label(muted(text));
                })
                .response;
            if resp.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.wiz.method = m;
            }
            ui.add_space(6.0);
        }
        ui.add_space(6.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("opts").num_columns(2).spacing(vec2(16.0, 12.0)).show(ui, |ui| {
                ui.label(muted("Program to run"));
                let none = "Don't run anything — just make the files available";
                let sel = self.wiz.installer.and_then(|i| d.installers.get(i)).map(|i| i.title()).unwrap_or_else(|| none.into());
                egui::ComboBox::from_id_salt("installer").width(480.0).selected_text(sel).show_ui(ui, |ui| {
                    for (i, inst) in d.installers.iter().enumerate() {
                        ui.selectable_value(&mut self.wiz.installer, Some(i), inst.title());
                    }
                    ui.selectable_value(&mut self.wiz.installer, None, none);
                });
                ui.end_row();
                if self.wiz.method == Method::Copy {
                    ui.label(muted("Copy to folder"));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.wiz.dest).desired_width(380.0));
                        if soft_button(ui, "Browse…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().set_title("Where should the disc be copied?").pick_folder() {
                                self.wiz.dest = p.to_string_lossy().to_string();
                            }
                        }
                    });
                    ui.end_row();
                    let need: u64 = self.wiz.discs.iter().filter_map(|x| x.source.as_ref()).map(|s| s.total_bytes()).sum();
                    ui.label(muted("Space needed"));
                    ui.label(fmt_size(need));
                    ui.end_row();
                } else {
                    ui.label(muted("After setup"));
                    ui.checkbox(&mut self.wiz.keep_mounted, "Leave the disc in the virtual drive (some games check for their disc when they start)");
                    ui.end_row();
                }
            });
        });
        if self.wiz.discs.len() > 1 {
            ui.add_space(8.0);
            let n = self.wiz.discs.len();
            if self.wiz.method == Method::Copy {
                ui.label(muted(format!("This set has {n} discs. All of them are copied into the same folder before setup starts, so setup finds every disc's files there.")));
            } else {
                ui.label(big(format!("This game comes on {n} discs — how should the disc swaps work?"), 16.0));
                let modes = [
                    (
                        MultiMode::Changer,
                        "🔄  Automatic disc changer — all discs mounted",
                        "Recommended",
                        "Every disc goes into its own virtual drive at the same time. Setup runs from disc 1 like normal; when it pauses and asks for the next disc, ISObubble moves that disc into setup's drive and presses OK for you — setup never closes and just carries on, exactly like swapping real discs.",
                    ),
                    (
                        MultiMode::Combine,
                        "🔗  Combine all discs into one",
                        "Skips swapping",
                        "ISObubble merges every disc into a single virtual disc first, so setup finds disc 2's files straight away and never asks. Needs free space about the size of all the discs together (removed afterwards). If setup still asks for a disc, use the disc changer instead.",
                    ),
                ];
                for (m, title, badge, text) in modes {
                    let selected = self.wiz.multi == m;
                    let c = if m == MultiMode::Changer { TEAL } else { BLUE };
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
                                chip(ui, badge, c);
                            });
                            ui.label(muted(text));
                        })
                        .response;
                    if resp.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.wiz.multi = m;
                    }
                    ui.add_space(4.0);
                }
            }
        }
    }

    fn step_install(&mut self, ui: &mut egui::Ui) {
        let w = &self.wiz;
        let (title, color) = if w.installing {
            ("Installing…", PURPLE)
        } else if w.install_error.is_some() {
            ("Something went wrong", RED)
        } else {
            ("Setup is done", TEAL)
        };
        let mut swap = None;
        let mut force_done = false;
        tinted_card(color).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                let spin = if w.installing { self.time * 3.0 } else { self.time * 0.3 };
                disc_bubble(ui.painter(), r.center(), 28.0, color, spin);
                ui.vertical(|ui| {
                    ui.label(big(title, 21.0));
                    if let Some(l) = w.letter {
                        ui.label(muted(format!("Disc {} is in virtual drive {l}:", w.current_disc + 1)));
                    } else if w.method == Method::Copy {
                        ui.label(muted(format!("Copying to {}", w.dest)));
                    }
                });
            });
            ui.add_space(6.0);
            if w.method == Method::Copy || !w.installing {
                ui.add(egui::ProgressBar::new(w.progress).show_percentage().corner_radius(8.0));
            } else {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::hover());
                ui.painter().rect_filled(r, 6.0, PANEL2);
                let x = ((self.time * 0.6).fract() as f32) * (r.width() + 160.0) - 160.0;
                let left = r.left() + x.max(0.0);
                let right = (r.left() + x + 160.0).min(r.right());
                if right > left {
                    ui.painter().rect_filled(egui::Rect::from_min_max(pos2(left, r.top()), pos2(right, r.bottom())), 6.0, PURPLE);
                }
            }
            if let Some(e) = &w.install_error {
                ui.label(RichText::new(e).color(RED));
            }
            ui.horizontal(|ui| {
                if let Some(l) = w.letter {
                    if soft_button(ui, &format!("Open drive {l}:")).clicked() {
                        system::open_folder(Path::new(&format!("{l}:\\")));
                    }
                }
                if w.method == Method::Copy && !w.installing && soft_button(ui, "Open the folder").clicked() {
                    system::open_folder(Path::new(w.dest.trim()));
                }
                if w.installing
                    && w.method == Method::Mount
                    && soft_button(ui, "Setup is finished — continue").on_hover_text("Use this if setup closed but ISObubble is still waiting").clicked()
                {
                    force_done = true;
                }
            });
        });
        if w.discs.len() > 1 && w.method == Method::Mount && w.combined.is_some() {
            ui.add_space(10.0);
            tinted_card(BLUE).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(big("🔗  All discs combined", 16.0));
                ui.label(muted(format!(
                    "Every disc's files are on one virtual disc{} — setup shouldn't need to ask for another disc.",
                    w.letter.map(|l| format!(" in drive {l}:")).unwrap_or_default()
                )));
            });
        } else if w.discs.len() > 1 && w.method == Method::Mount {
            ui.add_space(10.0);
            card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(big("Disc drives", 16.0));
                    if w.multi == MultiMode::Changer && w.installing {
                        let pulse = ((self.time * 2.0).sin() * 0.5 + 0.5) as f32;
                        chip(ui, "🔄 Disc changer watching for setup's prompts", mix(TEAL, BLUE, pulse));
                    }
                });
                ui.label(muted("All discs are mounted. When setup asks for another disc, ISObubble swaps it into setup's drive and presses OK. You can also move a disc there yourself."));
                for (i, d) in w.discs.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let current = w.letter.is_some() && w.current_disc == i;
                        let letter = w.letters.iter().find(|x| x.0 == i).map(|x| x.1);
                        let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                        disc_bubble(ui.painter(), r.center(), 13.0, if current { TEAL } else if letter.is_some() { BLUE } else { MUTED }, self.time);
                        ui.label(big(format!("Disc {}", i + 1), 14.0));
                        ui.label(muted(d.source.as_ref().map(|s| s.info.label.clone()).unwrap_or_default()));
                        match (current, letter) {
                            (true, Some(l)) => {
                                chip(ui, &format!("In setup's drive {l}:"), TEAL);
                            }
                            (_, Some(l)) => {
                                chip(ui, &format!("Mounted in {l}:"), BLUE);
                            }
                            _ => {}
                        }
                        if !current && w.letter.is_some() && soft_button(ui, "Move to setup's drive").clicked() {
                            swap = Some(i);
                        }
                    });
                }
            });
        }
        ui.add_space(10.0);
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(big("What's happening", 16.0));
            for l in &w.log {
                ui.label(RichText::new(l).size(13.5).color(TEXT));
            }
        });
        if let Some(i) = swap {
            self.swap_disc(i);
        }
        if force_done {
            self.wiz.installing = false;
            self.wiz.install_done = true;
            self.wiz.log.push("Continuing — setup was marked as finished.".into());
        }
    }

    fn step_finish(&mut self, ui: &mut egui::Ui) {
        tinted_card(TEAL).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(big("🎉  All set! Add it to your library", 21.0));
            ui.label(muted("Pick the program that starts what you installed, so you can launch it from ISObubble with one click."));
        });
        ui.add_space(10.0);
        let mut pick = None;
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("finish").num_columns(2).spacing(vec2(16.0, 12.0)).show(ui, |ui| {
                ui.label(muted("Name"));
                ui.add(egui::TextEdit::singleline(&mut self.wiz.name).desired_width(420.0));
                ui.end_row();
                ui.label(muted("Program to launch"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.wiz.program).desired_width(420.0).hint_text("C:\\Program Files\\…\\program.exe"));
                    if soft_button(ui, "Browse…").clicked() {
                        let start = if self.wiz.method == Method::Copy { self.wiz.dest.clone() } else { std::env::var("ProgramFiles").unwrap_or_default() };
                        if let Some(p) = rfd::FileDialog::new().set_directory(start).add_filter("Programs", &["exe", "lnk", "bat"]).pick_file() {
                            self.wiz.program = p.to_string_lossy().to_string();
                        }
                    }
                });
                ui.end_row();
                ui.label(muted("Disc"));
                ui.checkbox(&mut self.wiz.needs_disc, "Insert the disc automatically when I press Play (for games that check for their disc)");
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(muted("Found:"));
                if self.wiz.searching {
                    ui.add(egui::Spinner::new().color(PURPLE));
                    ui.label(muted("looking for newly installed programs…"));
                } else if self.wiz.suggestions.is_empty() {
                    ui.label(muted("no new shortcuts — use Browse to pick the program"));
                }
                for (n, t) in &self.wiz.suggestions {
                    let sel = self.wiz.program.eq_ignore_ascii_case(t);
                    if bubble_button(ui, n, if sel { TEAL } else { PANEL2 }).on_hover_text(t).clicked() {
                        pick = Some(t.clone());
                    }
                }
            });
        });
        if let Some(t) = pick {
            self.wiz.program = t;
        }
    }
}

fn kind_color(k: DiscKind) -> Color32 {
    match k {
        DiscKind::Software => PURPLE,
        DiscKind::Game => PINK,
        DiscKind::WindowsSetup => BLUE,
        DiscKind::Linux => AMBER,
        DiscKind::DvdVideo | DiscKind::Media => TEAL,
        _ => BLUE,
    }
}

fn level_color(l: scan::Level) -> Color32 {
    match l {
        scan::Level::Info => MUTED,
        scan::Level::Low => BLUE,
        scan::Level::Medium => AMBER,
        scan::Level::High | scan::Level::Threat => RED,
    }
}

fn perimeter_point(r: egui::Rect, t: f32) -> egui::Pos2 {
    let mut d = t * 2.0 * (r.width() + r.height());
    if d < r.width() {
        return pos2(r.left() + d, r.top());
    }
    d -= r.width();
    if d < r.height() {
        return pos2(r.right(), r.top() + d);
    }
    d -= r.height();
    if d < r.width() {
        return pos2(r.right() - d, r.bottom());
    }
    d -= r.width();
    pos2(r.left(), r.bottom() - d)
}

/// Shows a scan in progress or its results. Returns true if "Scan again" was pressed.
pub(crate) fn scan_view(ui: &mut egui::Ui, st: &ScanState, time: f64) -> bool {
    let mut again = false;
    if st.running {
        tinted_card(TEAL).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                let p = ui.painter();
                let c = r.center();
                p.circle_filled(c, 28.0, alpha(TEAL, 40));
                let a = time as f32 * 4.0;
                for (k, (rad, al)) in [(7.0, 255u8), (5.0, 160), (3.5, 100)].iter().enumerate() {
                    let ang = a - k as f32 * 0.6;
                    p.circle_filled(c + vec2(ang.cos(), ang.sin()) * 18.0, *rad, alpha(TEAL, *al));
                }
                ui.vertical(|ui| {
                    ui.label(big("Scanning for viruses, trojans and malware…", 18.0));
                    ui.add(egui::Label::new(RichText::new(&st.status).size(13.0).color(MUTED)).truncate());
                    if !st.findings.is_empty() {
                        ui.label(RichText::new(format!("{} finding(s) so far", st.findings.len())).size(13.0).color(AMBER));
                    }
                });
            });
            ui.add(egui::ProgressBar::new(st.progress).show_percentage().corner_radius(8.0));
            if soft_button(ui, "Stop scan").clicked() {
                st.cancel.store(true, Ordering::Relaxed);
            }
        });
    } else if let Some(r) = &st.report {
        let (c, title, text, icon) = match r.verdict() {
            scan::Verdict::Clean => (TEAL, "No threats found", "Nothing dangerous was found. It looks safe to install.", "✔"),
            scan::Verdict::Caution => (AMBER, "A few things to look at", "Nothing is confirmed as malware, but some files behave in ways worth knowing about. Check the list below.", "!"),
            scan::Verdict::Danger => (RED, "Danger — threats found", "This contains something dangerous. Don't install or open it unless you're completely sure where it came from.", "✖"),
        };
        tinted_card(c).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rr, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                let p = ui.painter();
                p.circle_filled(rr.center(), 28.0, c);
                p.circle_filled(rr.center() + vec2(-9.0, -10.0), 7.0, Color32::from_white_alpha(70));
                p.text(rr.center(), Align2::CENTER_CENTER, icon, FontId::new(26.0, bold()), text_on(c));
                ui.vertical(|ui| {
                    ui.label(big(title, 21.0));
                    ui.label(RichText::new(text).color(TEXT));
                });
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                chip(ui, &format!("{} files checked", r.files_scanned), BLUE);
                chip(ui, &format!("{} read", fmt_size(r.bytes_scanned)), BLUE);
                let (dt, dc) = match &r.defender {
                    scan::DefenderResult::Clean => ("Microsoft Defender: clean", TEAL),
                    scan::DefenderResult::Threats(_) => ("Microsoft Defender: THREATS", RED),
                    scan::DefenderResult::Unavailable(_) => ("Microsoft Defender: not used", MUTED),
                    scan::DefenderResult::Error(_) => ("Microsoft Defender: couldn't finish", AMBER),
                    scan::DefenderResult::NotRun => ("Microsoft Defender: not run", MUTED),
                };
                chip(ui, dt, dc);
                if r.cancelled {
                    chip(ui, "Scan was stopped early", AMBER);
                }
            });
            match &r.defender {
                scan::DefenderResult::Threats(t) => {
                    for l in t {
                        ui.label(RichText::new(l).size(13.0).color(RED));
                    }
                }
                scan::DefenderResult::Error(e) | scan::DefenderResult::Unavailable(e) => {
                    ui.label(RichText::new(e).size(12.5).color(MUTED));
                }
                _ => {}
            }
        });
        ui.add_space(10.0);
        let mut list = r.findings.clone();
        list.sort_by(|a, b| b.level.cmp(&a.level));
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(big(format!("Findings ({})", list.len()), 16.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if soft_button(ui, "↻ Scan again").clicked() {
                        again = true;
                    }
                });
            });
            if list.is_empty() {
                ui.label(muted("Nothing to report — every file passed our checks."));
            }
            for f in list.iter().take(400) {
                ui.horizontal_wrapped(|ui| {
                    chip(ui, f.level.label(), level_color(f.level));
                    ui.label(big(f.file.replace('/', "\\"), 13.5));
                    ui.label(RichText::new(&f.what).size(13.5).color(MUTED));
                });
            }
            for (n, h) in &r.fingerprints {
                ui.label(RichText::new(format!("Fingerprint (SHA-256) of {n}: {h}")).size(11.5).color(MUTED));
            }
        });
    }
    again
}
