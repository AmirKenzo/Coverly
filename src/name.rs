use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedName {
    pub title: String,
    pub year: Option<u16>,
    pub is_tv: bool,
    pub season: Option<u16>,
}

impl ParsedName {
    pub fn display(&self) -> String {
        let mut s = self.title.clone();
        if let Some(y) = self.year {
            s.push_str(&format!(" ({y})"));
        }
        if let Some(n) = self.season {
            s.push_str(&format!(" · Season {n}"));
        } else if self.is_tv {
            s.push_str(" · TV");
        }
        s
    }
}

fn re_year() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\b(?:19|20)\d{2}\b").unwrap())
}

fn re_season() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\bS(\d{1,2})(?:E\d{1,3})?\b|\bseason\s*(\d{1,2})\b").unwrap())
}

fn re_tags() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(concat!(
            r"(?i)\b(?:2160p|1080p|720p|576p|480p|4k|uhd|bluray|blu-ray|bdrip|brrip|",
            r"web-?dl|webrip|hdrip|dvdrip|hdtv|hdr10|hdr|x264|x265|h ?26[45]|hevc|avc|",
            r"10bit|aac|ac3|dts|ddp?5 ?1|atmos|remux|proper|repack|extended|unrated|",
            r"remastered|dubbed|dual ?audio|farsi|persian|subbed|softsub|hardsub|",
            r"complete|imax|yify|rarbg)\b"
        ))
        .unwrap()
    })
}

pub fn parse(raw: &str) -> ParsedName {
    let mut s = raw.trim().to_string();
    let lower = s.to_ascii_lowercase();
    for ext in crate::videos::EXTS.iter().map(|e| format!(".{e}")) {
        if lower.ends_with(&ext) {
            s.truncate(s.len() - ext.len());
            break;
        }
    }

    let norm: String = s
        .chars()
        .map(|c| match c {
            '.' | '_' | '[' | ']' | '(' | ')' | '{' | '}' => ' ',
            _ => c,
        })
        .collect();

    let mut cut = norm.len();

    let mut year = None;
    if let Some(m) = re_year().find_iter(&norm).filter(|m| m.start() > 0).last() {
        year = m.as_str().parse().ok();
        cut = cut.min(m.start());
    }

    let mut is_tv = false;
    let mut season = None;
    if let Some(c) = re_season().captures(&norm) {
        let whole = c.get(0).unwrap();
        if whole.start() > 0 {
            is_tv = true;
            season = c.get(1).or_else(|| c.get(2)).and_then(|m| m.as_str().parse().ok());
            cut = cut.min(whole.start());
        }
    }

    if let Some(m) = re_tags().find_iter(&norm).find(|m| m.start() > 0) {
        cut = cut.min(m.start());
    }

    let clean = |t: &str| {
        t.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim_matches(|c: char| c == '-' || c == ' ' || c == ',')
            .to_string()
    };
    let mut title = clean(&norm[..cut]);
    if title.is_empty() {
        title = clean(&norm);
    }

    ParsedName {
        title,
        year,
        is_tv,
        season,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NameFmt {
    Paren,
    Bracket,
    Dash,
    Dot,
    TitleOnly,
}

impl NameFmt {
    pub const ALL: [NameFmt; 5] = [
        NameFmt::Paren,
        NameFmt::Bracket,
        NameFmt::Dash,
        NameFmt::Dot,
        NameFmt::TitleOnly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NameFmt::Paren => "Title (Year)",
            NameFmt::Bracket => "Title [Year]",
            NameFmt::Dash => "Title - Year",
            NameFmt::Dot => "Title.Year",
            NameFmt::TitleOnly => "Title",
        }
    }

    pub fn apply(self, title: &str, year: Option<u16>) -> String {
        let t = title.trim();
        match (self, year) {
            (NameFmt::Paren, Some(y)) => format!("{t} ({y})"),
            (NameFmt::Bracket, Some(y)) => format!("{t} [{y}]"),
            (NameFmt::Dash, Some(y)) => format!("{t} - {y}"),
            (NameFmt::Dot, Some(y)) => format!("{}.{y}", t.replace(' ', ".")),
            (NameFmt::Dot, None) => t.replace(' ', "."),
            _ => t.to_string(),
        }
    }
}

pub fn clean_folder_name(s: &str) -> String {
    let s = s.replace(": ", " - ").replace(':', "-");
    let s: String = s
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control())
        .collect();
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', ' '])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> (String, Option<u16>, bool, Option<u16>) {
        let r = parse(s);
        (r.title, r.year, r.is_tv, r.season)
    }

    #[test]
    fn movies() {
        assert_eq!(
            p("Interstellar.2014.1080p.BluRay.x264"),
            ("Interstellar".into(), Some(2014), false, None)
        );
        assert_eq!(p("The Matrix (1999)"), ("The Matrix".into(), Some(1999), false, None));
        assert_eq!(
            p("Blade Runner 2049 (2017) [2160p]"),
            ("Blade Runner 2049".into(), Some(2017), false, None)
        );
        assert_eq!(p("1917.2019.1080p"), ("1917".into(), Some(2019), false, None));
        assert_eq!(
            p("Dune Part Two 2024 WEB-DL"),
            ("Dune Part Two".into(), Some(2024), false, None)
        );
        assert_eq!(p("Oppenheimer"), ("Oppenheimer".into(), None, false, None));
    }

    #[test]
    fn series() {
        assert_eq!(
            p("Breaking.Bad.S03.Complete.720p"),
            ("Breaking Bad".into(), None, true, Some(3))
        );
        assert_eq!(
            p("Game of Thrones Season 4"),
            ("Game of Thrones".into(), None, true, Some(4))
        );
        assert_eq!(p("Dark.S01E01.1080p"), ("Dark".into(), None, true, Some(1)));
    }
}
