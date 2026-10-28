use image::{imageops, imageops::FilterType, Rgba, RgbaImage};
use std::cell::Cell;

pub type C = Rgba<u8>;

thread_local! {
    static SCALE: Cell<f32> = const { Cell::new(1.0) };
    static ADJUST: Cell<(f32, f32, f32)> = const { Cell::new((1.0, 0.0, 0.0)) };
}

pub fn set_adjust(zoom: f32, x: f32, y: f32) {
    ADJUST.with(|c| c.set((zoom.max(1.0), x.max(-1.0).min(1.0), y.max(-1.0).min(1.0))));
}

fn adjust() -> (f32, f32, f32) {
    ADJUST.with(|c| c.get())
}

pub fn set_scale(k: f32) {
    SCALE.with(|c| c.set(k));
}

#[inline]
pub(crate) fn k() -> f32 {
    SCALE.with(|c| c.get())
}

pub const fn c(r: u8, g: u8, b: u8, a: u8) -> C {
    Rgba([r, g, b, a])
}

pub fn hex(rgb: u32, a: u8) -> C {
    Rgba([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, a])
}

pub fn shade(col: C, f: f32) -> C {
    let ch = |v: u8| -> u8 {
        let v = v as f32;
        let out = if f < 1.0 { v * f } else { v + (255.0 - v) * (f - 1.0) };
        out.max(0.0).min(255.0) as u8
    };
    Rgba([ch(col[0]), ch(col[1]), ch(col[2]), col[3]])
}

pub fn with_alpha(col: C, a: u8) -> C {
    Rgba([col[0], col[1], col[2], a])
}

pub fn luma(col: C) -> f32 {
    0.299 * col[0] as f32 + 0.587 * col[1] as f32 + 0.114 * col[2] as f32
}

#[inline]
pub fn blend(dst: &mut C, src: C, cov: f32) {
    let sa = src[3] as f32 / 255.0 * cov.max(0.0).min(1.0);
    if sa <= 0.0 {
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let oa = sa + da * (1.0 - sa);
    for i in 0..3 {
        let v = (src[i] as f32 * sa + dst[i] as f32 * da * (1.0 - sa)) / oa.max(1e-6);
        dst[i] = v.round().max(0.0).min(255.0) as u8;
    }
    dst[3] = (oa * 255.0).round().max(0.0).min(255.0) as u8;
}

fn bbox(img: &RgbaImage, x0: f32, y0: f32, x1: f32, y1: f32) -> (u32, u32, u32, u32) {
    let w = img.width() as f32;
    let h = img.height() as f32;
    let fix = |v: f32, max: f32| if v.is_nan() { 0.0 } else { v.max(0.0).min(max) };
    (
        fix(x0, w) as u32,
        fix(y0, h) as u32,
        fix(x1.ceil(), w) as u32,
        fix(y1.ceil(), h) as u32,
    )
}

pub fn in_rrect(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> bool {
    if px < x || py < y || px > x + w || py > y + h {
        return false;
    }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let cx = px.max(x + r).min(x + w - r);
    let cy = py.max(y + r).min(y + h - r);
    (px - cx) * (px - cx) + (py - cy) * (py - cy) <= r * r
}

fn raw_rrect(img: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, col: C) {
    let (x0, y0, x1, y1) = bbox(img, x, y, x + w, y + h);
    for py in y0..y1 {
        for px in x0..x1 {
            if in_rrect(px as f32 + 0.5, py as f32 + 0.5, x, y, w, h, r) {
                blend(img.get_pixel_mut(px, py), col, 1.0);
            }
        }
    }
}

pub fn fill_rrect(img: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, col: C) {
    let s = k();
    raw_rrect(img, x * s, y * s, w * s, h * s, r * s, col);
}

pub fn vgrad_rrect(img: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, top: C, bottom: C) {
    let s = k();
    let (x, y, w, h, r) = (x * s, y * s, w * s, h * s, r * s);
    let (x0, y0, x1, y1) = bbox(img, x, y, x + w, y + h);
    for py in y0..y1 {
        let t = ((py as f32 + 0.5 - y) / h.max(1e-3)).max(0.0).min(1.0);
        let mut col = top;
        for i in 0..4 {
            col[i] = (top[i] as f32 + (bottom[i] as f32 - top[i] as f32) * t) as u8;
        }
        for px in x0..x1 {
            if in_rrect(px as f32 + 0.5, py as f32 + 0.5, x, y, w, h, r) {
                blend(img.get_pixel_mut(px, py), col, 1.0);
            }
        }
    }
}

pub fn fill_circle(img: &mut RgbaImage, cx: f32, cy: f32, r: f32, col: C) {
    fill_rrect(img, cx - r, cy - r, r * 2.0, r * 2.0, r, col);
}

pub fn fill_ring(img: &mut RgbaImage, cx: f32, cy: f32, r_out: f32, r_in: f32, col: C) {
    let s = k();
    let (cx, cy, r_out, r_in) = (cx * s, cy * s, r_out * s, r_in * s);
    let (x0, y0, x1, y1) = bbox(img, cx - r_out, cy - r_out, cx + r_out, cy + r_out);
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            let d2 = dx * dx + dy * dy;
            if d2 <= r_out * r_out && d2 >= r_in * r_in {
                blend(img.get_pixel_mut(px, py), col, 1.0);
            }
        }
    }
}

pub fn clear_circle(img: &mut RgbaImage, cx: f32, cy: f32, r: f32) {
    let s = k();
    let (cx, cy, r) = (cx * s, cy * s, r * s);
    let (x0, y0, x1, y1) = bbox(img, cx - r, cy - r, cx + r, cy + r);
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r * r {
                img.put_pixel(px, py, c(0, 0, 0, 0));
            }
        }
    }
}

pub fn point_in_poly(x: f32, y: f32, pts: &[(f32, f32)]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn poly_bounds(pts: &[(f32, f32)]) -> (f32, f32, f32, f32) {
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in pts {
        b.0 = b.0.min(x);
        b.1 = b.1.min(y);
        b.2 = b.2.max(x);
        b.3 = b.3.max(y);
    }
    b
}

fn scaled(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let s = k();
    pts.iter().map(|(x, y)| (x * s, y * s)).collect()
}

pub fn fill_poly(img: &mut RgbaImage, pts: &[(f32, f32)], col: C) {
    if pts.len() < 3 {
        return;
    }
    let pts = scaled(pts);
    let (minx, miny, maxx, maxy) = poly_bounds(&pts);
    let (x0, y0, x1, y1) = bbox(img, minx, miny, maxx, maxy);
    for py in y0..y1 {
        for px in x0..x1 {
            if point_in_poly(px as f32 + 0.5, py as f32 + 0.5, &pts) {
                blend(img.get_pixel_mut(px, py), col, 1.0);
            }
        }
    }
}

pub fn hexagon(cx: f32, cy: f32, r: f32) -> Vec<(f32, f32)> {
    (0..6)
        .map(|i| {
            let a = (60.0 * i as f32 - 90.0).to_radians();
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

pub fn star(cx: f32, cy: f32, r_out: f32, r_in: f32) -> Vec<(f32, f32)> {
    (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { r_out } else { r_in };
            let a = (36.0 * i as f32 - 90.0).to_radians();
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

pub fn soft_shadow(img: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, strength: f32) {
    for i in (1..=10).rev() {
        let e = i as f32 * 2.2;
        let a = (strength * 26.0 / i as f32).max(0.0).min(255.0) as u8;
        fill_rrect(
            img,
            x - e * 0.5,
            y + 6.0 - e * 0.2,
            w + e,
            h + e * 0.9,
            r + e * 0.6,
            c(0, 0, 0, a),
        );
    }
}

pub fn fit_cover(src: &RgbaImage, w: u32, h: u32, focus_y: f32) -> RgbaImage {
    let (w, h) = (w.max(1), h.max(1));
    let (zoom, ax, ay) = adjust();
    let (sw, sh) = (src.width().max(1) as f32, src.height().max(1) as f32);
    let scale = (w as f32 / sw).max(h as f32 / sh) * zoom;
    let nw = ((sw * scale).ceil() as u32).max(w);
    let nh = ((sh * scale).ceil() as u32).max(h);
    let r = imageops::resize(src, nw, nh, FilterType::Triangle);
    let (free_x, free_y) = ((nw - w) as f32, (nh - h) as f32);
    let fx = (0.5 + ax * 0.5).max(0.0).min(1.0);
    let fy = if ay == 0.0 { focus_y } else { 0.5 + ay * 0.5 }.max(0.0).min(1.0);
    let x = (free_x * fx) as u32;
    let y = (free_y * fy) as u32;
    imageops::crop_imm(&r, x, y, w, h).to_image()
}

pub fn fit_contain(src: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (sw, sh) = (src.width().max(1) as f32, src.height().max(1) as f32);
    let scale = (w.max(1) as f32 / sw).min(h.max(1) as f32 / sh);
    let nw = ((sw * scale).round() as u32).max(1);
    let nh = ((sh * scale).round() as u32).max(1);
    imageops::resize(src, nw, nh, FilterType::Lanczos3)
}

pub fn paste_masked(dst: &mut RgbaImage, src: &RgbaImage, x: i32, y: i32, mask: impl Fn(f32, f32) -> bool) {
    for sy in 0..src.height() {
        let dy = y + sy as i32;
        if dy < 0 || dy >= dst.height() as i32 {
            continue;
        }
        for sx in 0..src.width() {
            let dx = x + sx as i32;
            if dx < 0 || dx >= dst.width() as i32 {
                continue;
            }
            if mask(dx as f32 + 0.5, dy as f32 + 0.5) {
                let p = *src.get_pixel(sx, sy);
                blend(dst.get_pixel_mut(dx as u32, dy as u32), p, 1.0);
            }
        }
    }
}

pub fn paste_layer(dst: &mut RgbaImage, layer: &RgbaImage) {
    if layer.width() == dst.width() && layer.height() == dst.height() {
        paste_masked(dst, layer, 0, 0, |_, _| true);
    } else {
        let l = imageops::resize(layer, dst.width(), dst.height(), FilterType::Triangle);
        paste_masked(dst, &l, 0, 0, |_, _| true);
    }
}

pub fn poster_rrect(dst: &mut RgbaImage, poster: &RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, focus_y: f32) {
    let s = k();
    let (x, y, w, h, r) = (x * s, y * s, w * s, h * s, r * s);
    let p = fit_cover(poster, w.round() as u32, h.round() as u32, focus_y);
    let (ox, oy) = (x.round(), y.round());
    paste_masked(dst, &p, ox as i32, oy as i32, |fx, fy| {
        in_rrect(fx, fy, ox, oy, w.round(), h.round(), r)
    });
}

pub fn poster_circle(dst: &mut RgbaImage, poster: &RgbaImage, cx: f32, cy: f32, r: f32) {
    let s = k();
    let (cx, cy, r) = (cx * s, cy * s, r * s);
    let d = (r * 2.0).round() as u32;
    let p = fit_cover(poster, d, d, 0.3);
    paste_masked(dst, &p, (cx - r).round() as i32, (cy - r).round() as i32, |fx, fy| {
        (fx - cx) * (fx - cx) + (fy - cy) * (fy - cy) <= r * r
    });
}

pub fn poster_poly(dst: &mut RgbaImage, poster: &RgbaImage, pts: &[(f32, f32)]) {
    let pts = scaled(pts);
    let (minx, miny, maxx, maxy) = poly_bounds(&pts);
    let p = fit_cover(poster, (maxx - minx).round() as u32, (maxy - miny).round() as u32, 0.35);
    paste_masked(dst, &p, minx.round() as i32, miny.round() as i32, |fx, fy| {
        point_in_poly(fx, fy, &pts)
    });
}

pub fn picture_contain(dst: &mut RgbaImage, pic: &RgbaImage, x: f32, y: f32, w: f32, h: f32) {
    let s = k();
    let (zoom, ax, ay) = adjust();
    let (x, y, w, h) = (x * s, y * s, w * s, h * s);
    let p = fit_contain(pic, (w * zoom).round() as u32, (h * zoom).round() as u32);
    let ox = x + (w - p.width() as f32) / 2.0 + ax * w * 0.25;
    let oy = y + (h - p.height() as f32) / 2.0 + ay * h * 0.25;
    paste_masked(dst, &p, ox.round() as i32, oy.round() as i32, |_, _| true);
}

pub fn picture_in(
    dst: &mut RgbaImage,
    pic: &RgbaImage,
    (x, y, w, h): (f32, f32, f32, f32),
    focus_y: f32,
    mask: impl Fn(f32, f32) -> bool,
    dim: u8,
) {
    let s = k();
    let p = fit_cover(pic, (w * s).round() as u32, (h * s).round() as u32, focus_y);
    let (ox, oy) = ((x * s).round() as i32, (y * s).round() as i32);
    let shade = c(0, 0, 0, dim);
    for sy in 0..p.height() {
        let dy = oy + sy as i32;
        if dy < 0 || dy >= dst.height() as i32 {
            continue;
        }
        for sx in 0..p.width() {
            let dx = ox + sx as i32;
            if dx < 0 || dx >= dst.width() as i32 {
                continue;
            }
            if mask((dx as f32 + 0.5) / s, (dy as f32 + 0.5) / s) {
                let d = dst.get_pixel_mut(dx as u32, dy as u32);
                blend(d, *p.get_pixel(sx, sy), 1.0);
                if dim > 0 {
                    blend(d, shade, 1.0);
                }
            }
        }
    }
}
