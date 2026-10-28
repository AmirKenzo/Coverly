pub mod bing;
pub mod fanart;
pub mod google;
pub mod google_web;
pub mod imdb;
pub mod local;
pub mod tmdb;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    Google,
    Bing,
    #[allow(dead_code)]
    DuckDuckGo,
    Imdb,
    Tmdb,
    Fanart,
    Local,
}

impl SourceKind {
    pub const ONLINE: [SourceKind; 5] = [
        SourceKind::Google,
        SourceKind::Bing,
        SourceKind::Imdb,
        SourceKind::Fanart,
        SourceKind::Tmdb,
    ];

    pub fn is_title_based(self) -> bool {
        matches!(self, SourceKind::Imdb | SourceKind::Tmdb | SourceKind::Fanart)
    }

    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Google => "Google",
            SourceKind::Bing => "Bing",
            SourceKind::DuckDuckGo => "DuckDuckGo",
            SourceKind::Imdb => "IMDb",
            SourceKind::Tmdb => "TMDB",
            SourceKind::Fanart => "fanart.tv",
            SourceKind::Local => "Folder",
        }
    }

    pub fn needs_key(self) -> bool {
        matches!(self, SourceKind::Tmdb | SourceKind::Fanart)
    }

    pub fn color(self) -> [u8; 3] {
        match self {
            SourceKind::Google => [92, 150, 255],
            SourceKind::Bing => [0, 190, 160],
            SourceKind::DuckDuckGo => [230, 100, 60],
            SourceKind::Imdb => [245, 197, 24],
            SourceKind::Tmdb => [1, 200, 240],
            SourceKind::Fanart => [190, 130, 255],
            SourceKind::Local => [170, 170, 170],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    #[default]
    Poster,
    FolderIcon,
    Icon,
    Logo,
    Backdrop,
}

impl Mode {
    pub const ALL: [Mode; 5] = [Mode::Poster, Mode::FolderIcon, Mode::Icon, Mode::Logo, Mode::Backdrop];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Poster => "Poster",
            Mode::FolderIcon => "Folder icon",
            Mode::Icon => "Icon",
            Mode::Logo => "Logo",
            Mode::Backdrop => "Backdrop",
        }
    }

    pub fn keywords(self) -> &'static str {
        match self {
            Mode::Poster => "poster",
            Mode::FolderIcon => "folder icon",
            Mode::Icon => "icon png",
            Mode::Logo => "logo png",
            Mode::Backdrop => "wallpaper",
        }
    }

    pub const ICON_SITES: [&'static str; 9] = [
        "deviantart.com",
        "freeiconspng.com",
        "iconarchive.com",
        "icon-icons.com",
        "pngwing.com",
        "iconfinder.com",
        "icons8.com",
        "pinterest.com",
        "pngegg.com",
    ];

    pub fn wants_original(self) -> bool {
        matches!(self, Mode::FolderIcon | Mode::Icon | Mode::Logo)
    }

    pub fn transparent(self) -> bool {
        matches!(self, Mode::FolderIcon | Mode::Icon | Mode::Logo)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Shape {
    #[default]
    Any,
    Portrait,
    Landscape,
    Square,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageHit {
    pub source: SourceKind,
    pub thumb_url: String,
    pub full_url: String,
    pub width: u32,
    pub height: u32,
    pub lang: Option<String>,
    pub title: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Match {
    pub tmdb_id: Option<u64>,
    pub imdb_id: Option<String>,
    pub is_tv: bool,
    pub title: String,
    pub year: Option<u16>,
    pub rating: Option<f32>,
    pub poster: Option<(String, u32, u32)>,
}

pub(crate) fn norm_title(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub(crate) fn title_rank(query: &str, title: &str) -> u8 {
    let (q, t) = (norm_title(query), norm_title(title));
    if q.is_empty() {
        2
    } else if q == t {
        0
    } else if t.starts_with(&q) {
        1
    } else {
        2
    }
}

pub(crate) fn rank_matches(v: &mut [Match], title: &str, year: Option<u16>, prefer_tv: bool) {
    v.sort_by_key(|m| {
        let t = title_rank(title, &m.title);
        let y = match (year, m.year) {
            (Some(a), Some(b)) if a == b => 0,
            (Some(a), Some(b)) if a.abs_diff(b) <= 1 => 1,
            (None, _) | (_, None) => 2,
            _ => 3,
        };
        let kind = if m.is_tv == prefer_tv { 0 } else { 1 };
        (t, y, kind)
    });
}

impl Match {
    pub fn display(&self) -> String {
        let mut s = self.title.clone();
        if let Some(y) = self.year {
            s.push_str(&format!(" ({y})"));
        }
        if self.is_tv {
            s.push_str(" · TV");
        }
        s
    }
}

#[derive(Clone, Debug, Default)]
pub struct WebQuery {
    pub text: String,
    pub mode: Mode,
    pub shape: Shape,
    pub large: bool,
}

pub(crate) fn dedupe(hits: &mut Vec<ImageHit>) {
    let mut seen = std::collections::HashSet::new();
    hits.retain(|h| seen.insert(h.full_url.clone()));
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else if b == b' ' {
            out.push_str("%20");
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn js_unescape(s: &str) -> String {
    s.replace("\\u003d", "=")
        .replace("\\u0026", "&")
        .replace("\\u0025", "%")
        .replace("\\u003c", "<")
        .replace("\\u003e", ">")
        .replace("\\/", "/")
        .replace("&amp;", "&")
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' | b'-' => Some(62),
            b'/' | b'_' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for &c in s.as_bytes() {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        buf = (buf << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64() {
        assert_eq!(super::base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(super::base64_decode("aGk=").unwrap(), b"hi");
    }

    #[test]
    fn ranks_exact_title_first() {
        use super::{rank_matches, Match};
        let mk = |t: &str, y: u16, tv: bool| Match {
            title: t.into(),
            year: Some(y),
            is_tv: tv,
            ..Default::default()
        };
        let mut v = vec![
            mk("The Dark Knight", 2008, false),
            mk("Dark Shadows", 2012, false),
            mk("Dark", 2017, true),
        ];
        rank_matches(&mut v, "Dark", None, false);
        assert_eq!(v[0].title, "Dark");
    }

    #[test]
    fn encode() {
        assert_eq!(super::url_encode("the matrix 1999"), "the%20matrix%201999");
    }
}
