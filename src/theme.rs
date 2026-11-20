//! The bubbly dark theme: colours, fonts, glossy pill buttons, cards and the floating bubble background.

use egui::{
    pos2, vec2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, Pos2, Rect,
    Response, Sense, Shape, Stroke, StrokeKind, TextStyle, Ui,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x12, 0x10, 0x1e);
pub const BG2: Color32 = Color32::from_rgb(0x1b, 0x15, 0x33);
pub const SIDEBAR: Color32 = Color32::from_rgba_premultiplied(0x18, 0x15, 0x2a, 0xee);
pub const PANEL: Color32 = Color32::from_rgb(0x20, 0x1c, 0x34);
pub const PANEL2: Color32 = Color32::from_rgb(0x2b, 0x26, 0x45);
pub const BORDER: Color32 = Color32::from_rgb(0x3a, 0x33, 0x5c);
pub const TEXT: Color32 = Color32::from_rgb(0xee, 0xea, 0xff);
pub const MUTED: Color32 = Color32::from_rgb(0xa6, 0x9f, 0xc8);
pub const PURPLE: Color32 = Color32::from_rgb(0xa2, 0x7b, 0xff);
pub const PINK: Color32 = Color32::from_rgb(0xff, 0x7a, 0xc6);
pub const TEAL: Color32 = Color32::from_rgb(0x4f, 0xe3, 0xc1);
pub const BLUE: Color32 = Color32::from_rgb(0x6c, 0xb8, 0xff);
pub const AMBER: Color32 = Color32::from_rgb(0xff, 0xc8, 0x57);
pub const RED: Color32 = Color32::from_rgb(0xff, 0x5c, 0x7a);

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

pub fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut bold_list = prop.clone();
    let fdir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()) + "\\Fonts\\";
    if let Ok(b) = std::fs::read(format!("{fdir}segoeui.ttf")) {
        fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(b)));
        prop.insert(0, "segoe".into());
        bold_list.insert(0, "segoe".into());
    }
    if let Ok(b) = std::fs::read(format!("{fdir}seguisb.ttf")).or_else(|_| std::fs::read(format!("{fdir}segoeuib.ttf"))) {
        fonts.font_data.insert("segoe_bold".into(), Arc::new(FontData::from_owned(b)));
        bold_list.insert(0, "segoe_bold".into());
    }
    if let Ok(b) = std::fs::read(format!("{fdir}seguisym.ttf")) {
        fonts.font_data.insert("symbols".into(), Arc::new(FontData::from_owned(b)));
        prop.push("symbols".into());
        bold_list.push("symbols".into());
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(bold(), bold_list);
    ctx.set_fonts(fonts);

    ctx.style_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, FontId::new(26.0, bold())),
            (TextStyle::Body, FontId::proportional(15.0)),
            (TextStyle::Button, FontId::proportional(15.0)),
            (TextStyle::Small, FontId::proportional(12.5)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        s.spacing.item_spacing = vec2(10.0, 9.0);
        s.spacing.button_padding = vec2(14.0, 7.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.scroll.bar_width = 8.0;
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = PANEL;
        v.extreme_bg_color = Color32::from_rgb(0x15, 0x12, 0x24);
        v.faint_bg_color = PANEL2;
        v.window_corner_radius = CornerRadius::same(22);
        v.menu_corner_radius = CornerRadius::same(16);
        v.window_stroke = Stroke::new(1.0_f32, BORDER);
        v.selection.bg_fill = alpha(PURPLE, 150);
        v.selection.stroke = Stroke::new(1.0_f32, TEXT);
        v.hyperlink_color = BLUE;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(15);
        }
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        v.widgets.inactive.bg_fill = PANEL2;
        v.widgets.inactive.weak_bg_fill = PANEL2;
        v.widgets.inactive.fg_stroke = Stroke::new(1.5_f32, TEXT);
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x5a, 0x50, 0x88));
        v.widgets.hovered.bg_fill = Color32::from_rgb(0x3b, 0x33, 0x62);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x3b, 0x33, 0x62);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, PURPLE);
        v.widgets.active.bg_fill = PURPLE;
        v.widgets.active.weak_bg_fill = PURPLE;
    });
}

/// Deep gradient plus slowly rising translucent bubbles.
pub fn paint_background(p: &egui::Painter, r: Rect, t: f64) {
    let mut mesh = egui::Mesh::default();
    let top = BG;
    let bottom = BG2;
    mesh.colored_vertex(r.left_top(), top);
    mesh.colored_vertex(r.right_top(), Color32::from_rgb(0x16, 0x12, 0x28));
    mesh.colored_vertex(r.right_bottom(), bottom);
    mesh.colored_vertex(r.left_bottom(), Color32::from_rgb(0x14, 0x1a, 0x2e));
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));

    let colors = [PURPLE, PINK, TEAL, BLUE];
    let h = r.height() + 300.0;
    for i in 0..26u32 {
        let seed = (i.wrapping_mul(2654435761) >> 8) as f32 / 16_777_216.0;
        let seed2 = (i.wrapping_mul(40503).wrapping_add(977) % 1000) as f32 / 1000.0;
        let radius = 14.0 + seed2 * 70.0;
        let speed = 6.0 + seed * 14.0;
        let x = r.left() + (seed * 1.7 + seed2 * 0.9).fract() * r.width() + ((t * 0.3 + i as f64).sin() as f32) * 18.0;
        let y = r.bottom() + 150.0 - ((t as f32 * speed + seed2 * h) % h);
        let c = colors[(i % 4) as usize];
        let center = pos2(x, y);
        p.circle_filled(center, radius, alpha(c, 14));
        p.circle_stroke(center, radius, Stroke::new(1.2_f32, alpha(c, 40)));
        p.circle_filled(center + vec2(-radius * 0.38, -radius * 0.38), radius * 0.16, Color32::from_white_alpha(22));
    }
}

/// A glossy, pill-shaped "bubble" button.
pub fn bubble_button(ui: &mut Ui, text: &str, color: Color32) -> Response {
    bubble_button_ex(ui, text, color, true, false)
}

pub fn text_on(fill: Color32) -> Color32 {
    let lum = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if lum > 150.0 {
        Color32::from_rgb(0x1a, 0x14, 0x2e)
    } else {
        Color32::WHITE
    }
}

pub fn bubble_button_ex(ui: &mut Ui, text: &str, color: Color32, enabled: bool, big: bool) -> Response {
    let font = FontId::new(if big { 18.0 } else { 15.0 }, bold());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let pad = if big { vec2(28.0, 14.0) } else { vec2(18.0, 8.0) };
    let size = galley.size() + pad * 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let mut fill = if enabled { color } else { Color32::from_rgb(0x3a, 0x35, 0x52) };
        if enabled && resp.hovered() {
            fill = mix(fill, Color32::WHITE, 0.14);
        }
        if enabled && resp.is_pointer_button_down_on() {
            fill = mix(fill, Color32::BLACK, 0.12);
        }
        let r = rect.height() / 2.0;
        p.rect_filled(rect.translate(vec2(0.0, 3.0)), r, Color32::from_black_alpha(80));
        if enabled {
            p.rect_filled(rect.expand(2.0), r + 2.0, alpha(color, if resp.hovered() { 60 } else { 25 }));
        }
        p.rect_filled(rect, r, fill);
        let gloss = Rect::from_min_max(
            rect.min + vec2(r * 0.9, 3.0),
            pos2(rect.max.x - r * 0.9, rect.min.y + 3.0 + rect.height() * 0.22),
        );
        p.rect_filled(gloss, gloss.height() / 2.0, Color32::from_white_alpha(if enabled { 9 } else { 4 }));
        p.circle_filled(pos2(rect.min.x + r * 0.62, rect.min.y + r * 0.6), r * 0.13, Color32::from_white_alpha(if enabled { 60 } else { 12 }));
        let tc = if enabled { text_on(fill) } else { MUTED };
        p.galley(rect.center() - galley.size() / 2.0, galley, tc);
    }
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Soft outlined pill button for secondary actions.
pub fn soft_button(ui: &mut Ui, text: &str) -> Response {
    let font = FontId::proportional(14.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let size = galley.size() + vec2(28.0, 14.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let r = rect.height() / 2.0;
        let fill = if resp.hovered() { Color32::from_rgb(0x3b, 0x33, 0x62) } else { PANEL2 };
        p.rect_filled(rect, r, fill);
        p.rect_stroke(rect, r, Stroke::new(1.0_f32, if resp.hovered() { PURPLE } else { BORDER }), StrokeKind::Inside);
        p.galley(rect.center() - galley.size() / 2.0, galley, TEXT);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn chip(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = FontId::new(12.5, bold());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let size = galley.size() + vec2(18.0, 8.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, rect.height() / 2.0, alpha(color, 38));
    p.rect_stroke(rect, rect.height() / 2.0, Stroke::new(1.0_f32, alpha(color, 90)), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

pub fn card() -> Frame {
    Frame::NONE
        .fill(alpha(PANEL, 248))
        .corner_radius(CornerRadius::same(24))
        .inner_margin(Margin::same(20))
        .stroke(Stroke::new(1.0_f32, BORDER))
        .shadow(egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: Color32::from_black_alpha(70),
        })
}

pub fn tinted_card(c: Color32) -> Frame {
    Frame::NONE
        .fill(mix(PANEL, c, 0.12))
        .corner_radius(CornerRadius::same(22))
        .inner_margin(Margin::same(16))
        .stroke(Stroke::new(1.2_f32, alpha(c, 120)))
}

/// Side-by-side columns whose text is left-aligned (egui's own columns stretch wrapped text).
pub fn columns_plain(ui: &mut Ui, n: usize, mut f: impl FnMut(usize, &mut Ui)) {
    ui.columns(n, |cols| {
        for (i, c) in cols.iter_mut().enumerate() {
            c.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| f(i, ui));
        }
    });
}

pub fn heading(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(FontId::new(28.0, bold())).color(TEXT));
}

pub fn sub(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(15.0).color(MUTED));
}

/// A glossy disc-shaped bubble (used as logo and as card art).
pub fn disc_bubble(p: &egui::Painter, c: Pos2, r: f32, color: Color32, t: f64) {
    p.circle_filled(c + vec2(0.0, r * 0.12), r, Color32::from_black_alpha(60));
    p.circle_filled(c, r, mix(color, BG, 0.25));
    p.circle_filled(c, r * 0.86, mix(color, Color32::WHITE, 0.05));
    // rainbow-ish sheen rotating slowly, like a real disc
    let a = t as f32 * 0.6;
    for (k, col) in [PINK, TEAL, BLUE].iter().enumerate() {
        let ang = a + k as f32 * 2.1;
        let off = vec2(ang.cos(), ang.sin()) * r * 0.42;
        p.circle_filled(c + off, r * 0.32, alpha(*col, 46));
    }
    p.circle_filled(c, r * 0.28, mix(color, BG, 0.55));
    p.circle_filled(c, r * 0.12, BG);
    p.circle_stroke(c, r, Stroke::new(1.5_f32, Color32::from_white_alpha(60)));
    p.circle_filled(c + vec2(-r * 0.42, -r * 0.45), r * 0.17, Color32::from_white_alpha(110));
    p.circle_filled(c + vec2(-r * 0.22, -r * 0.62), r * 0.07, Color32::from_white_alpha(90));
}

pub fn hue_color(h: f32) -> Color32 {
    let palette = [PURPLE, PINK, TEAL, BLUE, AMBER];
    let x = (h.fract() * palette.len() as f32).floor() as usize % palette.len();
    palette[x]
}

pub fn app_icon() -> egui::IconData {
    egui::IconData { rgba: icon_pixels(64), width: 64, height: 64 }
}

/// Draws the ISObubble icon (a glossy disc bubble) at `n`×`n` pixels, anti-aliased, RGBA.
pub fn icon_pixels(n: usize) -> Vec<u8> {
    let s = n as f32;
    let c = s / 2.0;
    let mut out = vec![0u8; n * n * 4];
    let cover = |d: f32, r: f32| ((r - d) * s / 64.0 + 0.5).clamp(0.0, 1.0);
    for y in 0..n {
        for x in 0..n {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let (dx, dy) = (px - c, py - c);
            let d = (dx * dx + dy * dy).sqrt();
            let outer = 0.47 * s;
            let a_out = cover(d, outer);
            if a_out <= 0.0 {
                continue;
            }
            // body: purple → pink diagonal gradient
            let t = ((px + py) / (2.0 * s)).clamp(0.0, 1.0);
            let mut col = [
                (PURPLE.r() as f32 * (1.0 - t) + PINK.r() as f32 * t),
                (PURPLE.g() as f32 * (1.0 - t) + PINK.g() as f32 * t),
                (PURPLE.b() as f32 * (1.0 - t) + PINK.b() as f32 * t),
            ];
            let blend = |col: &mut [f32; 3], c2: Color32, a: f32| {
                col[0] += (c2.r() as f32 - col[0]) * a;
                col[1] += (c2.g() as f32 - col[1]) * a;
                col[2] += (c2.b() as f32 - col[2]) * a;
            };
            // disc sheen blobs
            for (ox, oy, cc) in [(0.16, 0.12, TEAL), (-0.12, 0.18, BLUE), (0.15, -0.16, Color32::WHITE)] {
                let (bx, by) = (c + ox * s, c + oy * s);
                let bd = ((px - bx).powi(2) + (py - by).powi(2)).sqrt();
                let a = (1.0 - bd / (0.17 * s)).clamp(0.0, 1.0) * 0.45;
                blend(&mut col, cc, a);
            }
            // rim
            let rim = (1.0 - ((d - outer + 0.02 * s).abs() / (0.025 * s))).clamp(0.0, 1.0);
            blend(&mut col, Color32::WHITE, rim * 0.35);
            // hub ring and hole
            let hub = cover(d, 0.15 * s);
            blend(&mut col, Color32::from_rgb(0x2a, 0x22, 0x48), hub);
            let hole = cover(d, 0.065 * s);
            // gloss highlight
            let (gx, gy) = (c - 0.2 * s, c - 0.22 * s);
            let gd = ((px - gx).powi(2) / 1.6 + (py - gy).powi(2)).sqrt();
            blend(&mut col, Color32::WHITE, cover(gd, 0.08 * s) * 0.75);
            let a = a_out * (1.0 - hole);
            let i = (y * n + x) * 4;
            out[i] = col[0].round() as u8;
            out[i + 1] = col[1].round() as u8;
            out[i + 2] = col[2].round() as u8;
            out[i + 3] = (a * 255.0).round() as u8;
        }
    }
    out
}

/// A Windows .ico file with 16–256 px images (PNG inside, supported since Windows Vista).
pub fn icon_ico_bytes() -> Vec<u8> {
    let sizes = [16usize, 24, 32, 48, 64, 128, 256];
    let mut pngs = Vec::new();
    for &n in &sizes {
        let px = icon_pixels(n);
        let mut buf = Vec::new();
        if n >= 256 {
            if let Some(img) = image::RgbaImage::from_raw(n as u32, n as u32, px) {
                let _ = img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png);
            }
        } else {
            // Classic 32-bit bitmap entry: header, BGRA rows bottom-up, then an empty AND mask.
            let mask_row = (n + 31) / 32 * 4;
            buf.extend(40u32.to_le_bytes());
            buf.extend((n as i32).to_le_bytes());
            buf.extend((2 * n as i32).to_le_bytes());
            buf.extend(1u16.to_le_bytes());
            buf.extend(32u16.to_le_bytes());
            buf.extend(0u32.to_le_bytes());
            buf.extend(((n * n * 4 + mask_row * n) as u32).to_le_bytes());
            buf.extend([0u8; 16]);
            for y in (0..n).rev() {
                for x in 0..n {
                    let i = (y * n + x) * 4;
                    buf.extend([px[i + 2], px[i + 1], px[i], px[i + 3]]);
                }
            }
            buf.extend(vec![0u8; mask_row * n]);
        }
        pngs.push(buf);
    }
    let mut out = vec![0, 0, 1, 0];
    out.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (i, &n) in sizes.iter().enumerate() {
        let dim = if n >= 256 { 0 } else { n as u8 };
        out.extend([dim, dim, 0, 0]);
        out.extend(1u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend((pngs[i].len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += pngs[i].len() as u32;
    }
    for p in pngs {
        out.extend(p);
    }
    out
}
