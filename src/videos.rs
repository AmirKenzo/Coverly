use crate::name::{self, clean_folder_name, NameFmt, ParsedName};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const EXTS: [&str; 15] = [
    "mkv", "mp4", "avi", "m4v", "mov", "wmv", "ts", "m2ts", "mts", "webm", "mpg", "mpeg", "flv", "vob", "iso",
];

pub fn is_video(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

pub struct Plan {
    pub video: PathBuf,
    pub parsed: ParsedName,
    pub folder: String,
    pub extras: Vec<PathBuf>,
    pub on: bool,
}

impl Plan {
    pub fn new(video: PathBuf, fmt: NameFmt) -> Self {
        let file = video
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let parsed = name::parse(&file);
        let extras = extras_for(&video);
        let mut p = Self {
            video,
            parsed,
            folder: String::new(),
            extras,
            on: true,
        };
        p.rename(fmt);
        p
    }

    pub fn rename(&mut self, fmt: NameFmt) {
        let year = if self.parsed.is_tv { None } else { self.parsed.year };
        self.folder = clean_folder_name(&fmt.apply(&self.parsed.title, year));
    }

    pub fn season_dir(&self) -> Option<String> {
        self.parsed
            .season
            .filter(|_| self.parsed.is_tv)
            .map(|n| format!("Season {n:02}"))
    }

    pub fn file_name(&self) -> String {
        self.video
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

pub fn extras_for(video: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (video.parent(), video.file_stem()) else {
        return Vec::new();
    };
    let stem = stem.to_string_lossy().to_lowercase();
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p != video)
                .filter(|p| {
                    let n = p
                        .file_name()
                        .map(|s| s.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    n.strip_prefix(&stem)
                        .and_then(|rest| rest.chars().next())
                        .is_some_and(|c| matches!(c, '.' | '-' | '_' | ' '))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn execute(p: &Plan, move_extras: bool, seasons: bool) -> Result<PathBuf> {
    let name = clean_folder_name(&p.folder);
    if name.is_empty() {
        bail!("the folder name is empty");
    }
    let parent = p.video.parent().context("the file has no folder")?;
    let season = p.season_dir().filter(|_| seasons);

    let here = parent
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if season.is_none() && here.eq_ignore_ascii_case(&name) {
        return Ok(parent.to_path_buf());
    }

    let root = parent.join(&name);
    let dest = match &season {
        Some(s) => root.join(s),
        None => root.clone(),
    };
    std::fs::create_dir_all(&dest).with_context(|| format!("couldn't create “{}”", dest.display()))?;
    move_into(&p.video, &dest)?;
    if move_extras {
        for e in &p.extras {
            if e.exists() {
                let _ = move_into(e, &dest);
            }
        }
    }
    Ok(root)
}

fn move_into(src: &Path, dir: &Path) -> Result<()> {
    let target = dir.join(src.file_name().context("no file name")?);
    if target.exists() {
        bail!("“{}” is already in that folder", target.display());
    }
    if std::fs::rename(src, &target).is_err() {
        std::fs::copy(src, &target).with_context(|| format!("couldn't move “{}”", src.display()))?;
        std::fs::remove_file(src).with_context(|| format!("couldn't remove “{}” after copying", src.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_and_moves() {
        let base = std::env::temp_dir().join(format!("coverly-videos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let video = base.join("Interstellar.2014.1080p.BluRay.mkv");
        for f in [
            "Interstellar.2014.1080p.BluRay.mkv",
            "Interstellar.2014.1080p.BluRay.en.srt",
            "Interstellar.2014.1080p.BluRay.nfo",
            "Interstellar.2014.1080p.BluRayX.txt",
        ] {
            std::fs::write(base.join(f), b"x").unwrap();
        }
        let plan = Plan::new(video.clone(), NameFmt::Paren);
        assert_eq!(plan.folder, "Interstellar (2014)");
        assert_eq!(plan.extras.len(), 2);

        let root = execute(&plan, true, true).unwrap();
        assert_eq!(root, base.join("Interstellar (2014)"));
        assert!(root.join("Interstellar.2014.1080p.BluRay.mkv").is_file());
        assert!(root.join("Interstellar.2014.1080p.BluRay.en.srt").is_file());
        assert!(base.join("Interstellar.2014.1080p.BluRayX.txt").is_file());
        assert!(!video.exists());

        let ep = base.join("Dark.S02E03.720p.mkv");
        std::fs::write(&ep, b"x").unwrap();
        let plan = Plan::new(ep, NameFmt::Paren);
        assert_eq!(plan.folder, "Dark");
        let root = execute(&plan, true, true).unwrap();
        assert!(root.join("Season 02").join("Dark.S02E03.720p.mkv").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn video_types() {
        assert!(is_video(Path::new("a.MKV")));
        assert!(!is_video(Path::new("a.srt")));
    }
}
