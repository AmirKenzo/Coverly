pub mod custom;
pub mod draw;
pub mod text;

use draw::*;
use image::{imageops, imageops::FilterType, RgbaImage};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Original,
    OriginalShadow,
    Plain,
    Sharp,
    AppIcon,
    Square,
    Circle,
    Hexagon,
    Polaroid,
    Framed,
    Case,
    Steelbook,
    Vhs,
    FolderTucked,
    FolderCard,
    FolderFront,
    FolderFrontTall,
    FolderWhole,
    FolderTinted,
    DiscCase,
    Stack,
    FilmStrip,
    Tv,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Builtin {
        style: Style,
        a: C,
        b: C,
        label: &'static str,
    },
    Custom(Arc<custom::Custom>),
}

#[derive(Clone, Debug)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub family: String,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LabelStyle {
    #[default]
    Bar,
    Pill,
    Outline,
    Accent,
}

impl LabelStyle {
    pub const ALL: [LabelStyle; 4] = [
        LabelStyle::Bar,
        LabelStyle::Pill,
        LabelStyle::Outline,
        LabelStyle::Accent,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LabelStyle::Bar => "Bar",
            LabelStyle::Pill => "Pill",
            LabelStyle::Outline => "Outline",
            LabelStyle::Accent => "Accent",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adjust {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

impl Default for Adjust {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            x: 0.0,
            y: 0.0,
        }
    }
}

impl Adjust {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    pub rating: Option<f32>,
    pub name: Option<String>,
    pub style: LabelStyle,
    pub adjust: Adjust,
}

pub(crate) fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn push(v: &mut Vec<Template>, family: &str, name: &str, style: Style, a: C, b: C, label: &'static str) {
    v.push(Template {
        id: slug(&format!("{family} {name}")),
        name: format!("{family} – {name}"),
        family: family.to_string(),
        kind: Kind::Builtin { style, a, b, label },
    });
}

const FOLDER_COLORS: [(&str, u32, u32); 12] = [
    ("Yellow", 0xf6c445, 0xe0a326),
    ("Blue", 0x4f95f0, 0x2f6fd0),
    ("Sky", 0x6cc7f5, 0x3ea6dc),
    ("Teal", 0x3cc2ae, 0x22a08d),
    ("Green", 0x5cc46a, 0x3a9e48),
    ("Orange", 0xf59a42, 0xd97a22),
    ("Red", 0xef5a5a, 0xc93a3a),
    ("Pink", 0xf27fb5, 0xd25a93),
    ("Purple", 0xa27cf0, 0x7f58d0),
    ("Gray", 0xa3a8b3, 0x7d838f),
    ("Graphite", 0x4a4d55, 0x34363c),
    ("White", 0xf4f4f4, 0xd2d2d2),
];

const RINGS: [(&str, u32); 8] = [
    ("White", 0xffffff),
    ("Black", 0x141414),
    ("Gold", 0xd4af37),
    ("Silver", 0xc0c4cc),
    ("Red", 0xd94040),
    ("Blue", 0x3b7be0),
    ("Green", 0x3aa45a),
    ("Purple", 0x8a5cf0),
];

const CASES: [(&str, &str, u32, u8, u32); 8] = [
    ("DVD Black", "DVD", 0x161616, 255, 0x2c2c2c),
    ("DVD Clear", "DVD", 0xdfe3e8, 120, 0x9aa1ab),
    ("Blu-ray", "BLU-RAY", 0x1c4fb0, 255, 0x2a66d6),
    ("4K Ultra HD", "4K ULTRA HD", 0x111111, 255, 0x2a2a2a),
    ("Collector Red", "COLLECTOR'S EDITION", 0x8e1b1b, 255, 0xb02a2a),
    ("Emerald", "SPECIAL EDITION", 0x135c3b, 255, 0x1d7a50),
    ("Royal Purple", "LIMITED EDITION", 0x3e2470, 255, 0x55339a),
    ("Anime Orange", "ANIME", 0xc4561a, 255, 0xe06a2a),
];

const POLAROIDS: [(&str, u32); 5] = [
    ("White", 0xfafafa),
    ("Cream", 0xf3ead7),
    ("Black", 0x1a1a1a),
    ("Pink", 0xf6d3e0),
    ("Mint", 0xd3f1e4),
];

const FRAMES: [(&str, u32, u32); 6] = [
    ("Gold", 0xc9a227, 0x8a6d12),
    ("Wood", 0x7a4a24, 0x4e2e14),
    ("Walnut", 0x5a3a22, 0x3a2414),
    ("Black", 0x1b1b1b, 0x3a3a3a),
    ("White", 0xf1f1f1, 0xc9c9c9),
    ("Silver", 0xb9bec7, 0x8a9099),
];

const STEEL: [(&str, u32, u32); 4] = [
    ("Silver", 0xd7dbe0, 0x8e949c),
    ("Gunmetal", 0x5b6068, 0x2c2f34),
    ("Gold", 0xe6c766, 0x9c7c22),
    ("Rose", 0xe7b6a8, 0x9e6a5c),
];

const VHS_BANDS: [(&str, u32); 4] = [
    ("Red", 0xc0392b),
    ("Blue", 0x2e6fd6),
    ("Yellow", 0xd4a20c),
    ("Green", 0x27ae60),
];

const DISCS: [(&str, u32, u32); 4] = [
    ("Silver disc", 0x161616, 0xc9ccd2),
    ("Gold disc", 0x161616, 0xd9b44a),
    ("Blu-ray", 0x1c4fb0, 0x8fb4ee),
    ("Black disc", 0xe8e8e8, 0x2a2a2a),
];

const STACKS: [(&str, u32); 6] = [
    ("Graphite", 0x4a4d55),
    ("Blue", 0x3b7be0),
    ("Red", 0xd94040),
    ("Purple", 0x8a5cf0),
    ("Green", 0x3aa45a),
    ("Gold", 0xd4af37),
];

const FILMS: [(&str, u32); 3] = [("Black", 0x111111), ("Sepia", 0x3b2a1a), ("Navy", 0x14213d)];

const TVS: [(&str, u32); 4] = [
    ("Black", 0x1a1a1a),
    ("Silver", 0xbfc3ca),
    ("Retro wood", 0x6b4423),
    ("White", 0xeeeeee),
];

pub fn catalog() -> Vec<Template> {
    let mut v = Vec::new();
    let black = hex(0x000000, 255);

    push(&mut v, "Original", "As it is", Style::Original, black, black, "");
    push(
        &mut v,
        "Original",
        "With shadow",
        Style::OriginalShadow,
        black,
        black,
        "",
    );

    for (n, f, b) in FOLDER_COLORS {
        push(&mut v, "Folder", n, Style::FolderTucked, hex(f, 255), hex(b, 255), "");
    }
    for (n, f, b) in FOLDER_COLORS {
        push(
            &mut v,
            "Folder card",
            n,
            Style::FolderCard,
            hex(f, 255),
            hex(b, 255),
            "",
        );
    }
    for (n, f, b) in FOLDER_COLORS {
        push(
            &mut v,
            "Folder · picture front",
            n,
            Style::FolderFront,
            hex(f, 255),
            hex(b, 255),
            "",
        );
    }
    for (n, f, b) in FOLDER_COLORS {
        push(
            &mut v,
            "Folder · tall picture",
            n,
            Style::FolderFrontTall,
            hex(f, 255),
            hex(b, 255),
            "",
        );
    }
    let tints: [(&str, u32, u8); 4] = [
        ("Natural", 0x000000, 70),
        ("Deep", 0x000000, 130),
        ("Blue night", 0x0b2a5c, 120),
        ("Warm", 0x5c3a0b, 110),
    ];
    for (n, tint, alpha) in tints {
        push(
            &mut v,
            "Folder · whole picture",
            n,
            Style::FolderWhole,
            hex(tint, alpha),
            hex(tint, alpha),
            "",
        );
    }
    for (n, f, b) in FOLDER_COLORS.iter().take(9).copied() {
        push(
            &mut v,
            "Folder · tinted",
            n,
            Style::FolderTinted,
            hex(f, 255),
            hex(b, 255),
            "",
        );
    }
    push(&mut v, "Poster", "Rounded", Style::Plain, black, black, "");
    push(&mut v, "Poster", "Sharp", Style::Sharp, black, black, "");
    push(&mut v, "Poster", "App icon", Style::AppIcon, black, black, "");
    push(&mut v, "Poster", "Rounded square", Style::Square, black, black, "");
    for (n, label, case, alpha, band) in CASES {
        push(&mut v, "Case", n, Style::Case, hex(case, alpha), hex(band, 255), label);
    }
    for (n, a, b) in STEEL {
        push(&mut v, "Steelbook", n, Style::Steelbook, hex(a, 255), hex(b, 255), "");
    }
    for (n, d) in DISCS.iter().map(|(n, c, d)| (*n, (*c, *d))) {
        push(
            &mut v,
            "Case + disc",
            n,
            Style::DiscCase,
            hex(d.0, 255),
            hex(d.1, 255),
            "",
        );
    }
    for (n, band) in VHS_BANDS {
        push(&mut v, "VHS", n, Style::Vhs, hex(band, 255), black, "VHS");
    }
    for (n, col) in RINGS {
        push(&mut v, "Circle", n, Style::Circle, hex(col, 255), black, "");
    }
    for (n, col) in RINGS {
        push(&mut v, "Hexagon", n, Style::Hexagon, hex(col, 255), black, "");
    }
    for (n, col) in POLAROIDS {
        push(&mut v, "Polaroid", n, Style::Polaroid, hex(col, 255), black, "");
    }
    for (n, a, b) in FRAMES {
        push(&mut v, "Frame", n, Style::Framed, hex(a, 255), hex(b, 255), "");
    }
    for (n, col) in STACKS {
        push(&mut v, "Stack", n, Style::Stack, hex(col, 255), black, "");
    }
    for (n, col) in FILMS {
        push(&mut v, "Film strip", n, Style::FilmStrip, hex(col, 255), black, "");
    }
    for (n, col) in TVS {
        push(&mut v, "TV", n, Style::Tv, hex(col, 255), black, "");
    }

    v.extend(custom::load_all());
    v
}

pub fn families(t: &[Template]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for x in t {
        if !out.contains(&x.family) {
            out.push(x.family.clone());
        }
    }
    out
}

pub fn render(t: &Template, poster: &RgbaImage, ov: &Overlay, size: u32) -> RgbaImage {
    let canvas = (size.saturating_mul(2)).clamp(64, 512).max(size.min(512));
    draw::set_scale(canvas as f32 / 512.0);
    draw::set_adjust(ov.adjust.zoom, ov.adjust.x, ov.adjust.y);
    let mut img = RgbaImage::new(canvas, canvas);
    let name_used = match &t.kind {
        Kind::Builtin { style, a, b, label } => builtin(&mut img, *style, *a, *b, label, poster, ov),
        Kind::Custom(c) => {
            c.draw(&mut img, poster);
            false
        }
    };
    overlays(&mut img, ov, name_used);
    draw::set_scale(1.0);
    draw::set_adjust(1.0, 0.0, 0.0);
    if size == canvas {
        img
    } else {
        imageops::resize(&img, size, size, FilterType::Lanczos3)
    }
}

fn folder_back(img: &mut RgbaImage, b: C) {
    soft_shadow(img, 40.0, 92.0, 432.0, 376.0, 26.0, 0.55);
    fill_rrect(img, 40.0, 60.0, 172.0, 76.0, 20.0, b);
    fill_poly(
        img,
        &[
            (186.0, 60.0),
            (196.0, 62.0),
            (204.0, 68.0),
            (226.0, 96.0),
            (236.0, 104.0),
            (186.0, 104.0),
        ],
        b,
    );
    fill_rrect(img, 40.0, 92.0, 432.0, 376.0, 26.0, b);
    vgrad_rrect(
        img,
        52.0,
        104.0,
        408.0,
        120.0,
        18.0,
        with_alpha(shade(b, 0.9), 0),
        with_alpha(shade(b, 0.82), 120),
    );
}

fn photo(img: &mut RgbaImage, p: &RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32) {
    soft_shadow(img, x, y, w, h, r, 0.6);
    fill_rrect(img, x, y, w, h, r, c(250, 250, 250, 255));
    poster_rrect(img, p, x + 7.0, y + 7.0, w - 14.0, h - 14.0, (r - 5.0).max(2.0), 0.45);
}

const TAB: (f32, f32, f32, f32, f32) = (40.0, 60.0, 172.0, 76.0, 20.0);
const SLOPE: [(f32, f32); 6] = [
    (186.0, 60.0),
    (196.0, 62.0),
    (204.0, 68.0),
    (226.0, 96.0),
    (236.0, 104.0),
    (186.0, 104.0),
];
const BODY: (f32, f32, f32, f32, f32) = (40.0, 92.0, 432.0, 376.0, 26.0);

fn in_back(x: f32, y: f32) -> bool {
    in_rrect(x, y, TAB.0, TAB.1, TAB.2, TAB.3, TAB.4)
        || point_in_poly(x, y, &SLOPE)
        || in_rrect(x, y, BODY.0, BODY.1, BODY.2, BODY.3, BODY.4)
}

fn in_front(x: f32, y: f32, top: f32) -> bool {
    in_rrect(x, y, 40.0, top, 432.0, 468.0 - top, 26.0)
}

fn folder_front(img: &mut RgbaImage, a: C, y: f32) {
    let h = 468.0 - y;
    fill_rrect(img, 40.0, y - 8.0, 432.0, h + 8.0, 26.0, c(0, 0, 0, 40));
    vgrad_rrect(img, 40.0, y, 432.0, h, 26.0, shade(a, 1.12), shade(a, 0.95));
    fill_rrect(img, 62.0, y, 388.0, 4.0, 2.0, with_alpha(shade(a, 1.4), 210));
}

fn builtin(img: &mut RgbaImage, style: Style, a: C, b: C, label: &str, p: &RgbaImage, ov: &Overlay) -> bool {
    let white = c(255, 255, 255, 255);
    match style {
        Style::Original => {
            picture_contain(img, p, 16.0, 16.0, 480.0, 480.0);
        }
        Style::OriginalShadow => {
            let mut layer = RgbaImage::new(img.width(), img.height());
            picture_contain(&mut layer, p, 28.0, 20.0, 456.0, 456.0);
            let off = (8.0 * draw::k()).round() as i32;
            let mut shadow = layer.clone();
            for px in shadow.pixels_mut() {
                *px = c(0, 0, 0, (px[3] as f32 * 0.45) as u8);
            }
            let shadow = imageops::blur(&shadow, 6.0 * draw::k());
            paste_masked(img, &shadow, 0, off, |_, _| true);
            paste_masked(img, &layer, 0, 0, |_, _| true);
        }
        Style::Plain | Style::Sharp => {
            let r = if style == Style::Plain { 18.0 } else { 3.0 };
            soft_shadow(img, 96.0, 16.0, 320.0, 480.0, r, 1.0);
            poster_rrect(img, p, 96.0, 16.0, 320.0, 480.0, r, 0.5);
        }
        Style::AppIcon | Style::Square => {
            let r = if style == Style::AppIcon { 100.0 } else { 28.0 };
            soft_shadow(img, 32.0, 32.0, 448.0, 448.0, r, 1.0);
            poster_rrect(img, p, 32.0, 32.0, 448.0, 448.0, r, 0.3);
        }
        Style::Circle => {
            soft_shadow(img, 16.0, 16.0, 480.0, 480.0, 240.0, 1.0);
            fill_circle(img, 256.0, 256.0, 240.0, a);
            poster_circle(img, p, 256.0, 256.0, 222.0);
        }
        Style::Hexagon => {
            let shadow: Vec<(f32, f32)> = hexagon(256.0, 256.0, 252.0)
                .into_iter()
                .map(|(x, y)| (x, y + 8.0))
                .collect();
            fill_poly(img, &shadow, c(0, 0, 0, 70));
            fill_poly(img, &hexagon(256.0, 256.0, 252.0), a);
            poster_poly(img, p, &hexagon(256.0, 256.0, 232.0));
        }
        Style::Polaroid => {
            soft_shadow(img, 72.0, 16.0, 368.0, 480.0, 6.0, 1.0);
            fill_rrect(img, 72.0, 16.0, 368.0, 480.0, 6.0, a);
            poster_rrect(img, p, 92.0, 36.0, 328.0, 372.0, 2.0, 0.4);
            if let Some(n) = &ov.name {
                let ink = if luma(a) > 140.0 {
                    c(30, 30, 30, 255)
                } else {
                    c(240, 240, 240, 255)
                };
                text::centered(img, n, 256.0, 452.0, 320.0, 40.0, ink);
                return true;
            }
        }
        Style::Framed => {
            soft_shadow(img, 68.0, 8.0, 376.0, 496.0, 8.0, 1.0);
            fill_rrect(img, 68.0, 8.0, 376.0, 496.0, 8.0, a);
            fill_rrect(img, 86.0, 26.0, 340.0, 460.0, 4.0, b);
            fill_rrect(img, 92.0, 32.0, 328.0, 448.0, 3.0, shade(a, 1.25));
            poster_rrect(img, p, 98.0, 38.0, 316.0, 436.0, 2.0, 0.4);
        }
        Style::Case => {
            let (x, y, w, h) = (84.0, 16.0, 344.0, 480.0);
            soft_shadow(img, x, y, w, h, 12.0, 1.0);
            fill_rrect(img, x, y, w, h, 12.0, a);
            let spine = shade(with_alpha(a, 255), 0.65);
            fill_rrect(img, x, y, 24.0, h, 12.0, spine);
            fill_rrect(img, x + 12.0, y, 12.0, h, 0.0, spine);
            fill_rrect(img, x + 30.0, y + 8.0, w - 38.0, 44.0, 6.0, b);
            text::centered(img, label, x + 30.0 + (w - 38.0) / 2.0, y + 30.0, w - 60.0, 26.0, white);
            poster_rrect(img, p, x + 30.0, y + 58.0, w - 38.0, h - 66.0, 3.0, 0.4);
            fill_poly(
                img,
                &[(x + 24.0, y), (x + 150.0, y), (x + 24.0, y + 200.0)],
                c(255, 255, 255, 22),
            );
        }
        Style::Steelbook => {
            soft_shadow(img, 80.0, 12.0, 352.0, 488.0, 22.0, 1.0);
            vgrad_rrect(img, 80.0, 12.0, 352.0, 488.0, 22.0, shade(a, 1.2), b);
            poster_rrect(img, p, 98.0, 30.0, 316.0, 452.0, 12.0, 0.4);
            fill_poly(img, &[(98.0, 30.0), (300.0, 30.0), (98.0, 250.0)], c(255, 255, 255, 34));
        }
        Style::Vhs => {
            soft_shadow(img, 88.0, 12.0, 336.0, 488.0, 8.0, 1.0);
            fill_rrect(img, 88.0, 12.0, 336.0, 488.0, 8.0, hex(0x121212, 255));
            poster_rrect(img, p, 102.0, 26.0, 308.0, 368.0, 3.0, 0.35);
            fill_rrect(img, 102.0, 406.0, 308.0, 80.0, 4.0, a);
            for i in 0..3 {
                fill_rrect(
                    img,
                    112.0,
                    418.0 + i as f32 * 20.0,
                    30.0,
                    8.0,
                    2.0,
                    c(255, 255, 255, 90),
                );
            }
            text::centered(
                img,
                if label.is_empty() { "VHS" } else { label },
                266.0,
                446.0,
                220.0,
                46.0,
                white,
            );
        }
        Style::FolderTucked => {
            folder_back(img, b);
            photo(img, p, 140.0, 30.0, 232.0, 346.0, 16.0);
            folder_front(img, a, 226.0);
        }
        Style::FolderCard => {
            folder_back(img, b);
            folder_front(img, a, 120.0);
            photo(img, p, 156.0, 138.0, 200.0, 302.0, 14.0);
        }
        Style::FolderFront | Style::FolderFrontTall => {
            let top = if style == Style::FolderFront { 226.0 } else { 120.0 };
            folder_back(img, b);
            let h = 468.0 - top;
            fill_rrect(img, 40.0, top - 8.0, 432.0, h + 8.0, 26.0, c(0, 0, 0, 40));
            picture_in(img, p, (40.0, top, 432.0, h), 0.3, |x, y| in_front(x, y, top), 0);
            fill_rrect(img, 62.0, top, 388.0, 4.0, 2.0, with_alpha(shade(a, 1.4), 220));
        }
        Style::FolderWhole => {
            soft_shadow(img, 40.0, 92.0, 432.0, 376.0, 26.0, 0.55);
            let area = (40.0, 60.0, 432.0, 408.0);
            picture_in(img, p, area, 0.35, in_back, 0);
            fill_rrect(img, TAB.0, TAB.1, TAB.2, TAB.3, TAB.4, a);
            fill_poly(img, &SLOPE, a);
            fill_rrect(img, BODY.0, BODY.1, BODY.2, BODY.3, BODY.4, a);
            let top = 214.0;
            fill_rrect(img, 40.0, top - 8.0, 432.0, 12.0, 6.0, c(0, 0, 0, 60));
            picture_in(img, p, area, 0.35, |x, y| in_front(x, y, top), 0);
            fill_rrect(img, 62.0, top, 388.0, 4.0, 2.0, c(255, 255, 255, 150));
        }
        Style::FolderTinted => {
            folder_back(img, b);
            let top = 196.0;
            let h = 468.0 - top;
            fill_rrect(img, 40.0, top - 8.0, 432.0, h + 8.0, 26.0, c(0, 0, 0, 40));
            picture_in(img, p, (40.0, top, 432.0, h), 0.3, |x, y| in_front(x, y, top), 0);
            vgrad_rrect(
                img,
                40.0,
                top,
                432.0,
                h,
                26.0,
                with_alpha(a, 50),
                with_alpha(shade(a, 0.8), 150),
            );
            fill_rrect(img, 62.0, top, 388.0, 4.0, 2.0, with_alpha(shade(a, 1.4), 220));
        }
        Style::DiscCase => {
            fill_circle(img, 338.0, 262.0, 178.0, c(0, 0, 0, 60));
            fill_circle(img, 334.0, 256.0, 176.0, b);
            fill_ring(img, 334.0, 256.0, 176.0, 168.0, shade(b, 0.8));
            poster_circle(img, p, 334.0, 256.0, 150.0);
            fill_circle(img, 334.0, 256.0, 56.0, shade(b, 1.15));
            fill_ring(img, 334.0, 256.0, 56.0, 50.0, shade(b, 0.85));
            clear_circle(img, 334.0, 256.0, 22.0);
            soft_shadow(img, 30.0, 40.0, 300.0, 432.0, 10.0, 1.0);
            fill_rrect(img, 30.0, 40.0, 300.0, 432.0, 10.0, a);
            poster_rrect(img, p, 42.0, 52.0, 276.0, 408.0, 4.0, 0.4);
        }
        Style::Stack => {
            fill_rrect(img, 160.0, 8.0, 280.0, 420.0, 18.0, shade(a, 0.6));
            fill_rrect(img, 130.0, 26.0, 280.0, 420.0, 18.0, shade(a, 0.85));
            soft_shadow(img, 96.0, 48.0, 288.0, 432.0, 18.0, 1.0);
            poster_rrect(img, p, 96.0, 48.0, 288.0, 432.0, 18.0, 0.4);
        }
        Style::FilmStrip => {
            soft_shadow(img, 64.0, 4.0, 384.0, 504.0, 10.0, 0.8);
            fill_rrect(img, 64.0, 4.0, 384.0, 504.0, 10.0, a);
            for i in 0..13 {
                let y = 14.0 + i as f32 * 38.0;
                fill_rrect(img, 76.0, y, 20.0, 24.0, 4.0, c(235, 235, 235, 235));
                fill_rrect(img, 416.0, y, 20.0, 24.0, 4.0, c(235, 235, 235, 235));
            }
            poster_rrect(img, p, 108.0, 28.0, 296.0, 456.0, 4.0, 0.4);
        }
        Style::Tv => {
            soft_shadow(img, 16.0, 64.0, 480.0, 320.0, 26.0, 1.0);
            fill_rrect(img, 16.0, 64.0, 480.0, 320.0, 26.0, a);
            poster_rrect(img, p, 36.0, 84.0, 440.0, 280.0, 10.0, 0.22);
            fill_poly(img, &[(36.0, 84.0), (270.0, 84.0), (36.0, 250.0)], c(255, 255, 255, 24));
            fill_rrect(img, 228.0, 384.0, 56.0, 44.0, 0.0, shade(a, 0.8));
            fill_rrect(img, 150.0, 424.0, 212.0, 24.0, 12.0, shade(a, 0.9));
        }
    }
    false
}

fn overlays(img: &mut RgbaImage, ov: &Overlay, name_used: bool) {
    if !name_used {
        if let Some(n) = ov.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            draw_label(img, n, ov.style);
        }
    }
    if let Some(r) = ov.rating {
        let (x, y, w, h) = (338.0, 10.0, 164.0, 66.0);
        fill_rrect(img, x + 2.0, y + 4.0, w, h, 33.0, c(0, 0, 0, 80));
        fill_rrect(img, x, y, w, h, 33.0, hex(0xf2b84b, 255));
        let ink = hex(0x1a1a1a, 255);
        fill_poly(img, &star(x + 38.0, y + 33.0, 22.0, 9.5), ink);
        text::left_mid(img, &format!("{r:.1}"), x + 66.0, y + 33.0, 42.0, ink);
    }
}

fn draw_label(img: &mut RgbaImage, t: &str, style: LabelStyle) {
    let white = c(255, 255, 255, 255);
    let cy = 462.0;
    match style {
        LabelStyle::Bar => {
            fill_rrect(img, 14.0, 424.0, 484.0, 76.0, 18.0, c(0, 0, 0, 200));
            text::centered(img, t, 256.0, cy, 452.0, 44.0, white);
        }
        LabelStyle::Pill | LabelStyle::Accent => {
            let size = text::fit_size(t, 452.0, 46.0);
            let w = (text::width512(t, size) + 56.0).max(110.0).min(488.0);
            let (bg, ink) = if style == LabelStyle::Accent {
                (hex(0xf2b84b, 255), hex(0x1a1a1a, 255))
            } else {
                (c(12, 12, 14, 215), white)
            };
            fill_rrect(img, 256.0 - w / 2.0 + 2.0, 428.0, w, 70.0, 35.0, c(0, 0, 0, 70));
            fill_rrect(img, 256.0 - w / 2.0, 424.0, w, 70.0, 35.0, bg);
            text::centered(img, t, 256.0, cy - 3.0, w - 40.0, size, ink);
        }
        LabelStyle::Outline => {
            let shadow = c(0, 0, 0, 150);
            for (dx, dy) in [
                (-3.0, 0.0),
                (3.0, 0.0),
                (0.0, -3.0),
                (0.0, 3.0),
                (-2.0, -2.0),
                (2.0, 2.0),
                (-2.0, 2.0),
                (2.0, -2.0),
                (0.0, 5.0),
            ] {
                text::centered(img, t, 256.0 + dx, cy + dy, 470.0, 50.0, shadow);
            }
            text::centered(img, t, 256.0, cy, 470.0, 50.0, white);
        }
    }
}

pub fn app_icon(size: u32) -> RgbaImage {
    const PNG: &[u8] = include_bytes!("../../assets/app.png");
    match image::load_from_memory(PNG) {
        Ok(img) => imageops::resize(&img.to_rgba8(), size, size, imageops::FilterType::Lanczos3),
        Err(_) => RgbaImage::new(size, size),
    }
}
