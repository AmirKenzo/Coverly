use super::draw::{blend, k, C};
use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};
use image::RgbaImage;
use std::sync::OnceLock;

const CANDIDATES: [&str; 7] = [
    r"C:\Windows\Fonts\segoeuib.ttf",
    r"C:\Windows\Fonts\arialbd.ttf",
    r"C:\Windows\Fonts\tahomabd.ttf",
    r"C:\Windows\Fonts\segoeui.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
];

fn font() -> Option<&'static FontVec> {
    static FONT: OnceLock<Option<FontVec>> = OnceLock::new();
    FONT.get_or_init(|| {
        CANDIDATES
            .iter()
            .find_map(|p| std::fs::read(p).ok().and_then(|b| FontVec::try_from_vec(b).ok()))
    })
    .as_ref()
}

pub fn width(text: &str, size: f32) -> f32 {
    let Some(f) = font() else { return 0.0 };
    let sf = f.as_scaled(PxScale::from(size));
    let mut w = 0.0;
    let mut prev = None;
    for ch in text.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            w += sf.kern(p, id);
        }
        w += sf.h_advance(id);
        prev = Some(id);
    }
    w
}

pub fn draw(img: &mut RgbaImage, text: &str, x: f32, y_top: f32, size: f32, col: C) {
    let Some(f) = font() else { return };
    let scale = PxScale::from(size);
    let sf = f.as_scaled(scale);
    let base = y_top + sf.ascent();
    let mut caret = x;
    let mut prev = None;
    for ch in text.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            caret += sf.kern(p, id);
        }
        let glyph = id.with_scale_and_position(scale, point(caret, base));
        caret += sf.h_advance(id);
        prev = Some(id);
        if let Some(o) = f.outline_glyph(glyph) {
            let b = o.px_bounds();
            o.draw(|gx, gy, cov| {
                let px = b.min.x as i32 + gx as i32;
                let py = b.min.y as i32 + gy as i32;
                if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                    blend(img.get_pixel_mut(px as u32, py as u32), col, cov);
                }
            });
        }
    }
}

fn line_height(size: f32) -> f32 {
    match font() {
        Some(f) => {
            let sf = f.as_scaled(PxScale::from(size));
            sf.ascent() - sf.descent()
        }
        None => size,
    }
}

pub fn left_mid(img: &mut RgbaImage, text: &str, x: f32, cy: f32, size: f32, col: C) {
    let s = k();
    let (x, cy, size) = (x * s, cy * s, size * s);
    draw(img, text, x, cy - line_height(size) / 2.0, size, col);
}

pub fn centered(img: &mut RgbaImage, text: &str, cx: f32, cy: f32, max_w: f32, size: f32, col: C) {
    let k = k();
    let (cx, cy, max_w, size) = (cx * k, cy * k, max_w * k, size * k);
    let min = 14.0 * k;
    let mut s = size;
    while s > min && width(text, s) > max_w {
        s -= 1.0;
    }
    let mut t = text.to_string();
    if width(&t, s) > max_w {
        while !t.is_empty() && width(&format!("{t}…"), s) > max_w {
            t.pop();
        }
        t.push('…');
    }
    let w = width(&t, s);
    draw(img, &t, cx - w / 2.0, cy - line_height(s) / 2.0, s, col);
}

pub fn width512(text: &str, size: f32) -> f32 {
    let k = k();
    width(text, size * k) / k.max(1e-3)
}

pub fn fit_size(text: &str, max_w: f32, size: f32) -> f32 {
    let mut s = size;
    while s > 14.0 && width512(text, s) > max_w {
        s -= 1.0;
    }
    s
}
