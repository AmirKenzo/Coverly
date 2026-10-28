use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontFamily, FontId, Id, Layout, Margin, Pos2, Rect, Response, RichText,
    Sense, Shape, Stroke, Ui, Vec2, Widget,
};

pub const ACCENT: Color32 = Color32::from_rgb(242, 184, 75);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(250, 200, 105);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(62, 50, 26);
pub const BG: Color32 = Color32::from_rgb(21, 21, 24);
pub const PANEL: Color32 = Color32::from_rgb(28, 28, 32);
pub const TITLEBAR: Color32 = Color32::from_rgb(17, 17, 20);
pub const FIELD: Color32 = Color32::from_rgb(16, 16, 19);
pub const CHIP: Color32 = Color32::from_rgb(38, 38, 44);
pub const CHIP_HOVER: Color32 = Color32::from_rgb(50, 50, 58);
pub const LINE: Color32 = Color32::from_rgb(46, 46, 53);
pub const TEXT: Color32 = Color32::from_rgb(236, 236, 240);
pub const MUTED: Color32 = Color32::from_rgb(150, 150, 162);
pub const DANGER: Color32 = Color32::from_rgb(240, 120, 120);
pub const OK: Color32 = Color32::from_rgb(120, 220, 150);
const CLOSE_RED: Color32 = Color32::from_rgb(196, 43, 28);
const ON_ACCENT: Color32 = Color32::from_rgb(24, 20, 10);

pub const BOLD: &str = "semibold";

pub fn bold() -> FontFamily {
    FontFamily::Name(BOLD.into())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Folder,
    Pencil,
    Search,
    Paste,
    Gear,
    Globe,
    ChevronDown,
    ChevronRight,
    Skip,
    Dots,
    Close,
    Minimize,
    Maximize,
    Restore,
    Refresh,
    Check,
    Film,
    Dot,
    Alert,
    Circle,
    Trash,
    Layers,
    Download,
}

pub fn paint_icon(p: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 2.0;
    let w = (s / 5.0).clamp(1.1, 1.8);
    let st = Stroke::new(w, color);
    let at = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    let line = |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), st);
    let closed = |pts: &[(f32, f32)]| Shape::closed_line(pts.iter().map(|&(x, y)| at(x, y)).collect(), st);
    let rect_of = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(at(x0, y0), at(x1, y1));
    let arc = |r: f32, a0: f32, a1: f32, cx: f32, cy: f32| -> Vec<Pos2> {
        let n = 18;
        (0..=n)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
                at(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };

    match icon {
        Icon::Folder => {
            p.add(closed(&[
                (-0.8, -0.55),
                (-0.25, -0.55),
                (-0.08, -0.35),
                (0.8, -0.35),
                (0.8, 0.6),
                (-0.8, 0.6),
            ]));
        }
        Icon::Pencil => {
            p.add(closed(&[
                (0.35, -0.75),
                (0.75, -0.35),
                (-0.35, 0.75),
                (-0.75, 0.75),
                (-0.75, 0.35),
            ]));
            p.add(line(&[(0.15, -0.55), (0.55, -0.15)]));
        }
        Icon::Search => {
            p.circle_stroke(at(-0.15, -0.15), 0.52 * s, st);
            p.add(line(&[(0.25, 0.25), (0.75, 0.75)]));
        }
        Icon::Paste => {
            p.rect_stroke(rect_of(-0.6, -0.55, 0.6, 0.8), 2.0, st);
            p.rect_filled(rect_of(-0.28, -0.8, 0.28, -0.42), 1.5, color);
        }
        Icon::Gear => {
            for k in 0..8 {
                let a = (k as f32 * 45.0).to_radians();
                p.line_segment(
                    [at(0.5 * a.cos(), 0.5 * a.sin()), at(0.82 * a.cos(), 0.82 * a.sin())],
                    Stroke::new(w * 1.7, color),
                );
            }
            p.circle_stroke(c, 0.55 * s, st);
            p.circle_stroke(c, 0.2 * s, st);
        }
        Icon::Globe => {
            p.circle_stroke(c, 0.78 * s, st);
            let ell: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = (i as f32 * 15.0).to_radians();
                    at(0.34 * a.cos(), 0.78 * a.sin())
                })
                .collect();
            p.add(Shape::line(ell, st));
            p.add(line(&[(-0.78, 0.0), (0.78, 0.0)]));
        }
        Icon::ChevronDown => {
            p.add(line(&[(-0.5, -0.22), (0.0, 0.28), (0.5, -0.22)]));
        }
        Icon::ChevronRight => {
            p.add(line(&[(-0.22, -0.5), (0.28, 0.0), (-0.22, 0.5)]));
        }
        Icon::Skip => {
            p.add(line(&[(-0.75, 0.0), (0.3, 0.0)]));
            p.add(line(&[(-0.05, -0.38), (0.33, 0.0), (-0.05, 0.38)]));
            p.add(line(&[(0.7, -0.55), (0.7, 0.55)]));
        }
        Icon::Dots => {
            for x in [-0.6, 0.0, 0.6] {
                p.circle_filled(at(x, 0.0), (0.14 * s).max(1.4), color);
            }
        }
        Icon::Close => {
            p.add(line(&[(-0.55, -0.55), (0.55, 0.55)]));
            p.add(line(&[(0.55, -0.55), (-0.55, 0.55)]));
        }
        Icon::Minimize => {
            p.add(line(&[(-0.6, 0.0), (0.6, 0.0)]));
        }
        Icon::Maximize => {
            p.rect_stroke(rect_of(-0.55, -0.55, 0.55, 0.55), 1.5, st);
        }
        Icon::Restore => {
            p.rect_stroke(rect_of(-0.62, -0.3, 0.3, 0.62), 1.5, st);
            p.add(line(&[
                (-0.3, -0.3),
                (-0.3, -0.62),
                (0.62, -0.62),
                (0.62, 0.3),
                (0.3, 0.3),
            ]));
        }
        Icon::Refresh => {
            let (r, a0, a1) = (0.6, 40.0_f32, 320.0_f32);
            p.add(Shape::line(arc(r, a0, a1, 0.0, 0.0), st));
            let a = a1.to_radians();
            let end = at(r * a.cos(), r * a.sin());
            let t = vec2(-a.sin(), a.cos());
            let n = vec2(-t.y, t.x);
            let k = 0.32 * s;
            p.add(Shape::line(vec![end - t * k + n * k, end, end - t * k - n * k], st));
        }
        Icon::Check => {
            p.add(line(&[(-0.6, 0.02), (-0.18, 0.45), (0.65, -0.45)]));
        }
        Icon::Download => {
            p.add(line(&[(0.0, -0.75), (0.0, 0.3)]));
            p.add(line(&[(-0.4, -0.08), (0.0, 0.32), (0.4, -0.08)]));
            p.add(line(&[(-0.7, 0.72), (0.7, 0.72)]));
        }
        Icon::Film => {
            p.rect_stroke(rect_of(-0.8, -0.6, 0.8, 0.6), 2.0, st);
            p.add(line(&[(-0.42, -0.6), (-0.42, 0.6)]));
            p.add(line(&[(0.42, -0.6), (0.42, 0.6)]));
            p.add(line(&[(-0.42, 0.0), (0.42, 0.0)]));
        }
        Icon::Dot => {
            p.circle_filled(c, 0.32 * s, color);
        }
        Icon::Alert => {
            p.circle_stroke(c, 0.75 * s, st);
            p.add(line(&[(0.0, -0.4), (0.0, 0.12)]));
            p.circle_filled(at(0.0, 0.4), w * 0.8, color);
        }
        Icon::Circle => {
            p.circle_stroke(c, 0.42 * s, st);
        }
        Icon::Trash => {
            p.add(line(&[(-0.75, -0.5), (0.75, -0.5)]));
            p.add(line(&[(-0.25, -0.5), (-0.2, -0.78), (0.2, -0.78), (0.25, -0.5)]));
            p.add(closed(&[(-0.55, -0.5), (-0.45, 0.78), (0.45, 0.78), (0.55, -0.5)]));
        }
        Icon::Layers => {
            p.add(closed(&[(0.0, -0.75), (0.8, -0.3), (0.0, 0.15), (-0.8, -0.3)]));
            p.add(line(&[(-0.8, 0.15), (0.0, 0.6), (0.8, 0.15)]));
        }
    }
}

pub fn icon_label(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_icon(ui.painter(), rect, icon, color);
    resp
}

pub fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, max_w: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping {
        max_width: max_w.max(8.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts(|f| f.layout_job(job))
}

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).family(bold()).color(MUTED));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Ghost,
    Quiet,
    Chip(bool),
    Window,
    WindowClose,
}

#[must_use = "add it with ui.add(…)"]
pub struct Btn {
    text: String,
    icon: Option<Icon>,
    icon_color: Option<Color32>,
    trail: Option<Icon>,
    kind: Kind,
    h: f32,
    min_w: f32,
    max_w: f32,
    font: f32,
    strong: bool,
    rounding: f32,
    text_color: Option<Color32>,
    align_left: bool,
}

impl Btn {
    pub fn new(text: impl Into<String>, kind: Kind) -> Self {
        Self {
            text: text.into(),
            icon: None,
            icon_color: None,
            trail: None,
            kind,
            h: 28.0,
            min_w: 0.0,
            max_w: f32::INFINITY,
            font: 13.0,
            strong: matches!(kind, Kind::Primary),
            rounding: 6.0,
            text_color: None,
            align_left: false,
        }
    }

    pub fn icon(mut self, i: Icon) -> Self {
        self.icon = Some(i);
        self
    }

    pub fn icon_color(mut self, c: Color32) -> Self {
        self.icon_color = Some(c);
        self
    }

    pub fn trail(mut self, i: Icon) -> Self {
        self.trail = Some(i);
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.h = h;
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.min_w = w;
        self.max_w = w;
        self
    }

    pub fn max_width(mut self, w: f32) -> Self {
        self.max_w = w;
        self
    }

    pub fn font(mut self, size: f32) -> Self {
        self.font = size;
        self
    }

    pub fn rounding(mut self, r: f32) -> Self {
        self.rounding = r;
        self
    }

    pub fn color(mut self, c: Color32) -> Self {
        self.text_color = Some(c);
        self
    }

    pub fn left(mut self) -> Self {
        self.align_left = true;
        self
    }
}

impl Widget for Btn {
    fn ui(self, ui: &mut Ui) -> Response {
        let family = if self.strong { bold() } else { FontFamily::Proportional };
        let font = FontId::new(self.font, family);
        let icon_sz = (self.font + 1.0).round();
        let gap = 7.0;
        let pad = if self.text.is_empty() {
            ((self.h - icon_sz) / 2.0).max(4.0)
        } else {
            (self.h * 0.42).clamp(8.0, 14.0)
        };
        let icons_w = self.icon.map_or(0.0, |_| icon_sz) + self.trail.map_or(0.0, |_| icon_sz * 0.8);
        let gaps = if self.text.is_empty() {
            if self.icon.is_some() && self.trail.is_some() {
                gap * 0.6
            } else {
                0.0
            }
        } else {
            self.icon.map_or(0.0, |_| gap) + self.trail.map_or(0.0, |_| gap * 0.8)
        };
        let text_room = (self.max_w - pad * 2.0 - icons_w - gaps).max(10.0);

        let galley = one_line(ui, &self.text, font, Color32::PLACEHOLDER, text_room);
        let content_w = galley.size().x + icons_w + gaps;
        let w = (content_w + pad * 2.0).max(self.min_w).min(self.max_w.max(self.min_w));
        let (rect, resp) = ui.allocate_exact_size(vec2(w, self.h), Sense::click());
        if !ui.is_rect_visible(rect) {
            return resp;
        }

        let hovered = resp.hovered() && ui.is_enabled();
        let pressed = resp.is_pointer_button_down_on() && ui.is_enabled();
        let (bg, fg, stroke) = match self.kind {
            Kind::Primary => (
                if pressed {
                    Color32::from_rgb(222, 166, 60)
                } else if hovered {
                    ACCENT_HOVER
                } else {
                    ACCENT
                },
                ON_ACCENT,
                Stroke::NONE,
            ),
            Kind::Ghost => (
                if pressed {
                    Color32::from_rgb(60, 60, 70)
                } else if hovered {
                    CHIP_HOVER
                } else {
                    CHIP
                },
                TEXT,
                Stroke::NONE,
            ),
            Kind::Quiet => (
                if hovered { CHIP } else { Color32::TRANSPARENT },
                if hovered { TEXT } else { MUTED },
                Stroke::NONE,
            ),
            Kind::Chip(true) => (ACCENT_DIM, ACCENT, Stroke::new(1.0_f32, Color32::from_rgb(120, 92, 40))),
            Kind::Chip(false) => (
                if hovered { CHIP_HOVER } else { CHIP },
                if hovered {
                    TEXT
                } else {
                    Color32::from_rgb(205, 205, 212)
                },
                Stroke::NONE,
            ),
            Kind::Window => (
                if hovered { CHIP_HOVER } else { Color32::TRANSPARENT },
                if hovered { TEXT } else { MUTED },
                Stroke::NONE,
            ),
            Kind::WindowClose => (
                if hovered { CLOSE_RED } else { Color32::TRANSPARENT },
                if hovered { Color32::WHITE } else { MUTED },
                Stroke::NONE,
            ),
        };
        let fg = self
            .text_color
            .filter(|_| !hovered || self.kind != Kind::Quiet)
            .unwrap_or(fg);
        let p = ui.painter();
        let round = if matches!(self.kind, Kind::Window | Kind::WindowClose) {
            0.0
        } else {
            self.rounding
        };
        p.rect(rect, round, bg, stroke);

        let mut x = if self.align_left {
            rect.left() + pad
        } else {
            rect.center().x - content_w / 2.0
        };
        let cy = rect.center().y;
        if let Some(i) = self.icon {
            let r = Rect::from_center_size(pos2(x + icon_sz / 2.0, cy), Vec2::splat(icon_sz));
            paint_icon(p, r, i, self.icon_color.unwrap_or(fg));
            x += icon_sz + if self.text.is_empty() { 0.0 } else { gap };
        }
        if !self.text.is_empty() {
            let gs = galley.size();
            p.galley(pos2(x, cy - gs.y / 2.0), galley.clone(), fg);
            x += gs.x + if self.trail.is_some() { gap * 0.8 } else { 0.0 };
        } else if self.icon.is_some() && self.trail.is_some() {
            x += gap * 0.6;
        }
        if let Some(i) = self.trail {
            let sz = icon_sz * 0.8;
            let r = Rect::from_center_size(pos2(x + sz / 2.0, cy), Vec2::splat(sz));
            paint_icon(
                p,
                r,
                i,
                if self.kind == Kind::Primary {
                    fg
                } else {
                    fg.gamma_multiply(0.8)
                },
            );
        }
        if ui.is_enabled() {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            resp
        }
    }
}

pub fn primary_button(text: &str, h: f32) -> Btn {
    Btn::new(text, Kind::Primary).height(h)
}

pub fn ghost_button(text: &str) -> Btn {
    Btn::new(text, Kind::Ghost)
}

pub fn quiet_button(text: &str) -> Btn {
    Btn::new(text, Kind::Quiet)
}

pub fn chip(text: &str, selected: bool) -> Btn {
    Btn::new(text, Kind::Chip(selected))
        .height(26.0)
        .font(12.5)
        .rounding(13.0)
}

pub fn icon_button(icon: Icon, size: f32) -> Btn {
    Btn::new("", Kind::Quiet).icon(icon).height(size).width(size)
}

pub fn dropdown<R>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    btn: Btn,
    min_w: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> (Response, Option<R>) {
    let resp = ui.add(btn.trail(Icon::ChevronDown));
    let id = ui.make_persistent_id(id_salt);
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    let r = egui::popup_below_widget(ui, id, &resp, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
        ui.set_min_width(min_w.max(resp.rect.width()));
        ui.spacing_mut().item_spacing.y = 2.0;
        add(ui)
    });
    (resp, r)
}

pub fn menu_item(ui: &mut Ui, text: &str, selected: bool) -> bool {
    let w = ui.available_width().max(120.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        if hovered {
            ui.painter().rect_filled(rect, 5.0, CHIP_HOVER);
        }
        if selected {
            let r = Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), Vec2::splat(12.0));
            paint_icon(ui.painter(), r, Icon::Check, ACCENT);
        }
        let g = one_line(
            ui,
            text,
            FontId::proportional(13.0),
            if selected { ACCENT } else { TEXT },
            rect.width() - 34.0,
        );
        let y = rect.center().y - g.size().y / 2.0;
        ui.painter().galley(pos2(rect.left() + 26.0, y), g, TEXT);
    }
    let clicked = resp.clicked();
    if clicked {
        ui.memory_mut(|m| m.close_popup());
    }
    clicked
}

pub fn menu_action(ui: &mut Ui, icon: Icon, text: &str) -> bool {
    let w = ui.available_width().max(120.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter().rect_filled(rect, 5.0, CHIP_HOVER);
        }
        let r = Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), Vec2::splat(13.0));
        paint_icon(ui.painter(), r, icon, MUTED);
        let g = one_line(ui, text, FontId::proportional(13.0), TEXT, rect.width() - 36.0);
        let y = rect.center().y - g.size().y / 2.0;
        ui.painter().galley(pos2(rect.left() + 30.0, y), g, TEXT);
    }
    let clicked = resp.clicked();
    if clicked {
        ui.memory_mut(|m| m.close_popup());
    }
    clicked
}

pub fn text_field<'a>(text: &'a mut String, hint: &str, width: f32) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .hint_text(RichText::new(hint.to_string()).color(Color32::from_gray(105)))
        .font(FontId::proportional(13.0))
        .desired_width(width)
        .min_size(vec2(0.0, 28.0))
        .vertical_align(Align::Center)
        .margin(Margin::symmetric(8.0, 4.0))
}

pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    id_salt: &str,
    value: &mut T,
    items: &[(T, String)],
    h: f32,
) -> bool {
    let font = FontId::proportional(12.5);
    let galleys: Vec<_> = items
        .iter()
        .map(|(_, t)| {
            ui.painter()
                .layout_no_wrap(t.clone(), font.clone(), Color32::PLACEHOLDER)
        })
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 22.0).collect();
    let total = widths.iter().sum::<f32>() + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, h), Sense::hover());
    let mut changed = false;
    if !ui.is_rect_visible(rect) {
        return false;
    }
    ui.painter().rect(rect, 7.0, FIELD, Stroke::new(1.0_f32, LINE));
    let mut x = rect.left() + 2.0;
    for (i, ((v, _), g)) in items.iter().zip(galleys).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(widths[i], h - 4.0));
        let resp = ui
            .interact(r, Id::new((id_salt, i)), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let sel = *value == *v;
        if sel {
            ui.painter().rect_filled(r, 5.0, CHIP_HOVER);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 5.0, Color32::from_rgb(30, 30, 35));
        }
        let col = if sel {
            TEXT
        } else if resp.hovered() {
            Color32::from_rgb(210, 210, 216)
        } else {
            MUTED
        };
        let gs = g.size();
        ui.painter().galley(r.center() - gs / 2.0, g, col);
        if resp.clicked() && !sel {
            *value = *v;
            changed = true;
        }
        x += widths[i];
    }
    changed
}

pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let font = FontId::proportional(13.0);
    let g = ui
        .painter()
        .layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let sw = vec2(28.0, 16.0);
    let size = vec2(sw.x + 8.0 + g.size().x, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool(resp.id, *on);
        let track = Rect::from_min_size(pos2(rect.left(), rect.center().y - sw.y / 2.0), sw);
        let fill = if *on { ACCENT } else { Color32::from_rgb(58, 58, 66) };
        ui.painter().rect_filled(track, sw.y / 2.0, fill);
        let knob_x = egui::lerp(track.left() + 8.0..=track.right() - 8.0, t);
        ui.painter().circle_filled(
            pos2(knob_x, track.center().y),
            5.5,
            if *on { ON_ACCENT } else { Color32::from_gray(200) },
        );
        let col = if resp.hovered() || *on {
            TEXT
        } else {
            Color32::from_rgb(205, 205, 212)
        };
        ui.painter()
            .galley(pos2(track.right() + 8.0, rect.center().y - g.size().y / 2.0), g, col);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn slider_row(ui: &mut Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) -> Response {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(84.0, 20.0), Layout::left_to_right(Align::Center), |ui| {
            ui.label(RichText::new(label).size(12.5).color(MUTED));
        });
        ui.spacing_mut().slider_width = (ui.available_width() - 4.0).max(60.0);
        ui.spacing_mut().interact_size.y = 16.0;
        ui.add(egui::Slider::new(v, range).show_value(false).trailing_fill(true))
    })
    .inner
}

pub fn source_toggle(ui: &mut Ui, label: &str, dot: Color32, count: &str, shown: bool) -> Response {
    let f = FontId::proportional(12.0);
    let g1 = ui
        .painter()
        .layout_no_wrap(label.to_string(), f.clone(), Color32::PLACEHOLDER);
    let g2 = ui.painter().layout_no_wrap(count.to_string(), f, Color32::PLACEHOLDER);
    let w = 8.0 + 6.0 + 6.0 + g1.size().x + 5.0 + g2.size().x + 9.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        let bg = if hovered {
            CHIP_HOVER
        } else if shown {
            CHIP
        } else {
            Color32::TRANSPARENT
        };
        let stroke = if shown {
            Stroke::NONE
        } else {
            Stroke::new(1.0_f32, LINE)
        };
        ui.painter().rect(rect, 11.0, bg, stroke);
        let cy = rect.center().y;
        ui.painter().circle_filled(
            pos2(rect.left() + 11.0, cy),
            3.0,
            if shown { dot } else { Color32::from_gray(80) },
        );
        let x = rect.left() + 20.0;
        let (s1, s2) = (g1.size(), g2.size());
        ui.painter()
            .galley(pos2(x, cy - s1.y / 2.0), g1, if shown { TEXT } else { MUTED });
        ui.painter().galley(pos2(x + s1.x + 5.0, cy - s2.y / 2.0), g2, MUTED);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn tag(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    let g = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(11.0), fg);
    let size = g.size() + vec2(12.0, 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, size.y / 2.0, bg);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, fg);
    resp
}

pub fn checkerboard(ui: &Ui, rect: Rect) {
    let p = ui.painter();
    let s = 16.0;
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() {
        let mut x = rect.left() + if row % 2 == 0 { 0.0 } else { s };
        while x < rect.right() {
            let r = Rect::from_min_max(pos2(x, y), pos2((x + s).min(rect.right()), (y + s).min(rect.bottom())));
            p.rect_filled(r, 0.0, Color32::from_rgb(22, 22, 26));
            x += s * 2.0;
        }
        y += s;
        row += 1;
    }
}

pub fn placeholder(ui: &Ui, rect: Rect, text: &str) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(13.0),
        MUTED,
    );
}

pub fn filter_dropdown(ui: &mut Ui, id_salt: &str, label: &str, active: bool, add: impl FnOnce(&mut Ui)) {
    let btn = quiet_button(label).height(22.0).font(12.0).rounding(11.0);
    let btn = if active { btn.color(ACCENT) } else { btn };
    let _ = dropdown(ui, id_salt, btn, 200.0, add);
}
