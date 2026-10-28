use crate::{sources::SourceKind, templates::LabelStyle};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const APP_NAME: &str = "Coverly";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub sources: Vec<SourceKind>,
    pub tmdb_key: String,
    pub fanart_key: String,
    pub language: String,

    pub proxy_enabled: bool,
    pub proxy_url: String,
    pub proxy_sources: Vec<SourceKind>,

    pub query_extra: String,
    pub query_site: String,

    pub show_rating: bool,
    pub auto_next: bool,
    pub show_name: bool,
    pub label_style: LabelStyle,
    pub last_template: String,
    pub icon_sizes: Vec<u32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sources: vec![
                SourceKind::Google,
                SourceKind::Bing,
                SourceKind::Imdb,
                SourceKind::Fanart,
                SourceKind::Tmdb,
            ],
            tmdb_key: String::new(),
            fanart_key: String::new(),
            language: "en".into(),
            proxy_enabled: false,
            proxy_url: "socks5h://127.0.0.1:10808".into(),
            proxy_sources: vec![SourceKind::Imdb, SourceKind::Fanart, SourceKind::Tmdb],
            query_extra: String::new(),
            query_site: String::new(),
            show_rating: false,
            auto_next: true,
            show_name: false,
            label_style: LabelStyle::Bar,
            last_template: "folder-yellow".into(),
            icon_sizes: vec![16, 24, 32, 48, 64, 128, 256],
        }
    }
}

impl Config {
    pub fn dir() -> PathBuf {
        dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join(APP_NAME)
    }

    fn path() -> PathBuf {
        Self::dir().join("config.toml")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(Self::dir())?;
        std::fs::write(Self::path(), toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn proxy_address(&self) -> Option<String> {
        let p = self.proxy_url.trim();
        if p.is_empty() {
            return None;
        }
        Some(if p.contains("://") {
            p.to_string()
        } else {
            format!("http://{p}")
        })
    }

    pub fn proxy_for(&self, s: SourceKind) -> Option<String> {
        if self.proxy_enabled && self.proxy_sources.contains(&s) {
            self.proxy_address()
        } else {
            None
        }
    }

    pub fn routes_for(&self, s: SourceKind) -> Vec<Option<String>> {
        if s == SourceKind::Local {
            return vec![None];
        }
        let first = self.proxy_for(s);
        let mut v = vec![first.clone()];
        if self.proxy_enabled {
            if let Some(p) = self.proxy_address() {
                let other = if first.is_some() { None } else { Some(p) };
                v.push(other);
            }
        }
        v
    }

    pub fn has_key(&self, s: SourceKind) -> bool {
        match s {
            SourceKind::Tmdb => !self.tmdb_key.trim().is_empty(),
            SourceKind::Fanart => !self.fanart_key.trim().is_empty(),
            _ => true,
        }
    }

    pub fn active_sources(&self) -> Vec<SourceKind> {
        SourceKind::ONLINE
            .iter()
            .copied()
            .filter(|s| self.sources.contains(s) && self.has_key(*s))
            .collect()
    }
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join(APP_NAME)
}
