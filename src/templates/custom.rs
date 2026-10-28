use super::{draw::*, slug, Kind, Template};
use image::{imageops, RgbaImage};
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Deserialize)]
struct Spec {
    name: String,
    #[serde(default)]
    family: Option<String>,
    #[serde(default = "default_canvas")]
    canvas: u32,
    poster: PRect,
    #[serde(default)]
    below: Vec<String>,
    #[serde(default)]
    above: Vec<String>,
    #[serde(default)]
    shape: Option<String>,
    #[serde(default)]
    focus: Option<f32>,
}

fn default_canvas() -> u32 {
    512
}

#[derive(Deserialize)]
struct PRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    #[serde(default)]
    radius: f32,
}

#[derive(Debug)]
pub struct Custom {
    below: Vec<RgbaImage>,
    above: Vec<RgbaImage>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    circle: bool,
    focus: f32,
}

impl Custom {
    pub fn draw(&self, img: &mut RgbaImage, poster: &RgbaImage) {
        for l in &self.below {
            paste_layer(img, l);
        }
        if self.circle {
            let r = self.w.min(self.h) / 2.0;
            poster_circle(img, poster, self.x + self.w / 2.0, self.y + self.h / 2.0, r);
        } else {
            poster_rrect(img, poster, self.x, self.y, self.w, self.h, self.r, self.focus);
        }
        for l in &self.above {
            paste_layer(img, l);
        }
    }
}

pub fn dirs() -> Vec<PathBuf> {
    let mut v = vec![crate::config::Config::dir().join("templates")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.join("templates"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        v.push(cwd.join("templates"));
    }
    v
}

pub fn load_all() -> Vec<Template> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for d in dirs() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut entries: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("template.toml").is_file())
            .collect();
        entries.sort();
        for e in entries {
            match load(&e) {
                Ok(t) => {
                    if seen.insert(t.id.clone()) {
                        out.push(t);
                    }
                }
                Err(err) => eprintln!("template {}: {err:#}", e.display()),
            }
        }
    }
    out
}

fn load(dir: &Path) -> anyhow::Result<Template> {
    let spec: Spec = toml::from_str(&std::fs::read_to_string(dir.join("template.toml"))?)?;
    let k = 512.0 / spec.canvas.max(1) as f32;
    let layer = |f: &String| -> anyhow::Result<RgbaImage> {
        let im = image::open(dir.join(f))?.to_rgba8();
        Ok(if im.width() == 512 && im.height() == 512 {
            im
        } else {
            imageops::resize(&im, 512, 512, imageops::FilterType::Lanczos3)
        })
    };
    let below = spec.below.iter().map(&layer).collect::<anyhow::Result<Vec<_>>>()?;
    let above = spec.above.iter().map(&layer).collect::<anyhow::Result<Vec<_>>>()?;
    let folder_name = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(Template {
        id: format!("custom-{}", slug(&folder_name)),
        name: spec.name,
        family: spec.family.unwrap_or_else(|| "Custom".into()),
        kind: Kind::Custom(Arc::new(Custom {
            below,
            above,
            x: spec.poster.x * k,
            y: spec.poster.y * k,
            w: spec.poster.w * k,
            h: spec.poster.h * k,
            r: spec.poster.radius * k,
            circle: spec.shape.as_deref() == Some("circle"),
            focus: spec.focus.unwrap_or(0.4),
        })),
    })
}
