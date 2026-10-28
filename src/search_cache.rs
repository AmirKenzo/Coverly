use crate::{
    config::cache_dir,
    sources::{ImageHit, Match, SourceKind},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const KEEP: Duration = Duration::from_secs(3 * 24 * 3600);
const IN_MEMORY: usize = 300;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub key: String,
    pub saved_at: u64,
    pub hits: Vec<ImageHit>,
    pub matches: Vec<Match>,
    pub done: Vec<SourceKind>,
    pub bing_next: usize,
}

pub fn age(saved_at: u64) -> String {
    let secs = now().saturating_sub(saved_at);
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        86_400..=172_799 => "yesterday".into(),
        _ => format!("{} days ago", secs / 86_400),
    }
}

pub struct SearchCache {
    mem: HashMap<String, Saved>,
    order: VecDeque<String>,
    dir: PathBuf,
}

impl SearchCache {
    pub fn new() -> Self {
        let dir = cache_dir().join("search");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let old = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > KEEP);
                if old {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        Self {
            mem: HashMap::new(),
            order: VecDeque::new(),
            dir,
        }
    }

    pub fn get(&mut self, key: &str) -> Option<Saved> {
        let fresh = |s: &Saved| now().saturating_sub(s.saved_at) < KEEP.as_secs();
        if let Some(s) = self.mem.get(key) {
            return fresh(s).then(|| s.clone());
        }
        let text = std::fs::read_to_string(self.file(key)).ok()?;
        let s: Saved = serde_json::from_str(&text).ok()?;
        if s.key != key || !fresh(&s) {
            return None;
        }
        self.remember(s.clone());
        Some(s)
    }

    pub fn put(&mut self, s: Saved) {
        if let Ok(text) = serde_json::to_string(&s) {
            let _ = std::fs::write(self.file(&s.key), text);
        }
        self.remember(s);
    }

    pub fn forget(&mut self, key: &str) {
        self.mem.remove(key);
        self.order.retain(|k| k != key);
        let _ = std::fs::remove_file(self.file(key));
    }

    fn remember(&mut self, s: Saved) {
        let key = s.key.clone();
        if self.mem.insert(key.clone(), s).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > IN_MEMORY {
            if let Some(k) = self.order.pop_front() {
                self.mem.remove(&k);
            }
        }
    }

    fn file(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{:016x}.json", fnv(key)))
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_names() {
        assert_eq!(fnv("a"), fnv("a"));
        assert_ne!(fnv("a"), fnv("b"));
    }

    #[test]
    fn ages() {
        assert_eq!(age(now()), "just now");
        assert_eq!(age(now() - 7200), "2 h ago");
    }
}
