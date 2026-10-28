use super::{dedupe, ImageHit, Shape, SourceKind, WebQuery};
use crate::net::Http;
use anyhow::{bail, Result};
use regex::Regex;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Meta {
    murl: Option<String>,
    turl: Option<String>,
    t: Option<String>,
}

pub const PAGE: usize = 50;

pub fn search(http: &Http, q: &WebQuery) -> Result<Vec<ImageHit>> {
    search_page(http, q, 0)
}

pub fn search_page(http: &Http, q: &WebQuery, first: usize) -> Result<Vec<ImageHit>> {
    let mut filters = String::new();
    match q.shape {
        Shape::Portrait => filters.push_str("+filterui:aspect-tall"),
        Shape::Landscape => filters.push_str("+filterui:aspect-wide"),
        Shape::Square => filters.push_str("+filterui:aspect-square"),
        Shape::Any => {}
    }
    if q.large {
        filters.push_str("+filterui:imagesize-large");
    }
    if q.mode.transparent() {
        filters.push_str("+filterui:photo-transparent");
    }

    let text = match q.text.find(" site:") {
        Some(i) if q.mode.transparent() && q.text[i..].contains(" OR site:") => q.text[..i].to_string(),
        _ => q.text.clone(),
    };
    if q.mode.transparent() && q.shape == Shape::Any {
        filters.push_str("+filterui:aspect-square");
    }

    let mut params = vec![
        ("q", text),
        ("first", first.to_string()),
        ("count", PAGE.to_string()),
        ("mmasync", "1".to_string()),
    ];
    if !filters.is_empty() {
        params.push(("qft", filters));
    }
    let html = http
        .client
        .get("https://www.bing.com/images/async")
        .query(&params)
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()?
        .error_for_status()?
        .text()?;

    let mut hits = Vec::new();
    parse(&html, &mut hits);
    dedupe(&mut hits);
    if hits.is_empty() && first == 0 {
        bail!("Bing found nothing for this search");
    }
    Ok(hits)
}

fn parse(html: &str, hits: &mut Vec<ImageHit>) {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"\sm="(\{[^"]+\})""#).unwrap());
    for c in re.captures_iter(html) {
        let raw = c[1]
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        if let Ok(m) = serde_json::from_str::<Meta>(&raw) {
            if let Some(full) = m.murl {
                hits.push(ImageHit {
                    source: SourceKind::Bing,
                    thumb_url: m.turl.unwrap_or_else(|| full.clone()),
                    full_url: full,
                    width: 0,
                    height: 0,
                    lang: None,
                    title: m.t.unwrap_or_default(),
                });
            }
        }
    }
}
