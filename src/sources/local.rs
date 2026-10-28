use super::{ImageHit, SourceKind};
use crate::folder::is_our_icon;
use std::path::{Path, PathBuf};

const EXTS: [&str; 7] = ["jpg", "jpeg", "png", "webp", "bmp", "gif", "ico"];
const POSTER_NAMES: [&str; 5] = ["poster", "folder", "cover", "movie", "show"];

pub fn is_image(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
        .unwrap_or(false)
}

pub fn hit_for(p: &Path) -> ImageHit {
    let url = format!("file:{}", p.to_string_lossy());
    ImageHit {
        source: SourceKind::Local,
        thumb_url: url.clone(),
        full_url: url,
        width: 0,
        height: 0,
        lang: None,
        title: p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
    }
}

fn poster_rank(p: &Path) -> usize {
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if POSTER_NAMES.contains(&stem.as_str()) {
        0
    } else if stem.contains("poster") || stem.contains("cover") {
        1
    } else {
        2
    }
}

fn images_in(folder: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_image(p))
        .filter(|p| {
            !p.file_name()
                .map(|n| is_our_icon(&n.to_string_lossy()))
                .unwrap_or(false)
        })
        .collect();
    v.sort_by_key(|p| (poster_rank(p), p.clone()));
    v
}

pub fn list(folder: &Path) -> Vec<ImageHit> {
    images_in(folder).iter().map(|p| hit_for(p)).collect()
}

pub fn best_poster(folder: &Path) -> Option<PathBuf> {
    images_in(folder).into_iter().find(|p| poster_rank(p) < 2)
}
