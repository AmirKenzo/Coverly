use super::{dedupe, js_unescape, percent_decode, url_encode, ImageHit, Shape, SourceKind, WebQuery};
use crate::net::Http;
use anyhow::{bail, Result};
use regex::Regex;
use std::sync::OnceLock;

pub fn search_url(q: &WebQuery) -> String {
    let mut tbs = Vec::new();
    match q.shape {
        Shape::Portrait => tbs.push("iar:t"),
        Shape::Landscape => tbs.push("iar:w"),
        Shape::Square => tbs.push("iar:s"),
        Shape::Any => {}
    }
    if q.large {
        tbs.push("isz:l");
    }
    if q.mode.transparent() {
        tbs.push("ic:trans");
    }
    let mut url = format!("https://www.google.com/search?q={}&tbm=isch&hl=en", url_encode(&q.text));
    if !tbs.is_empty() {
        url.push_str("&tbs=");
        url.push_str(&tbs.join(","));
    }
    url
}

pub fn search_http(http: &Http, q: &WebQuery) -> Result<Vec<ImageHit>> {
    let html = http
        .client
        .get(search_url(q))
        .header("User-Agent", "AdsBot-Google (+http://www.google.com/adsbot.html)")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header(
            "Cookie",
            "CONSENT=YES+cb; SOCS=CAESEwgDEgk0ODE3Nzk3MjQaAmVuIAEaBgiA_LyaBg",
        )
        .send()?
        .error_for_status()?
        .text()?;
    if html.contains("enablejs") || html.contains("/sorry/") {
        bail!("needs a browser");
    }
    let hits = parse_page(&html);
    if hits.is_empty() {
        bail!("no images in the plain page");
    }
    Ok(hits)
}

pub fn parse_page(html: &str) -> Vec<ImageHit> {
    let mut hits = parse_embedded(html);
    if hits.is_empty() {
        hits = parse_basic(html);
    }
    if hits.is_empty() {
        hits = parse_thumbnails(html);
    }
    dedupe(&mut hits);
    hits
}

fn parse_embedded(html: &str) -> Vec<ImageHit> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"\["(https?://[^"]+)",(\d+),(\d+)\]"#).unwrap());

    let mut hits = Vec::new();
    let mut prev_thumb: Option<String> = None;
    for c in re.captures_iter(html) {
        let url = js_unescape(&c[1]);
        let is_thumb = url.contains("gstatic.com/images") || url.contains("encrypted-tbn");
        if is_thumb {
            prev_thumb = Some(url);
            continue;
        }
        if let Some(thumb) = prev_thumb.take() {
            hits.push(ImageHit {
                source: SourceKind::Google,
                thumb_url: thumb,
                full_url: url,
                width: c[3].parse().unwrap_or(0),
                height: c[2].parse().unwrap_or(0),
                lang: None,
                title: String::new(),
            });
        }
    }
    hits
}

fn parse_basic(html: &str) -> Vec<ImageHit> {
    static FULL: OnceLock<Regex> = OnceLock::new();
    static THUMB: OnceLock<Regex> = OnceLock::new();
    let full = FULL.get_or_init(|| Regex::new(r"imgurl=([^&\x22]+)").unwrap());
    let thumb = THUMB.get_or_init(|| Regex::new(r#"src="(https://encrypted-tbn[^"]+)""#).unwrap());

    let fulls: Vec<String> = full
        .captures_iter(html)
        .map(|c| percent_decode(&js_unescape(&c[1])))
        .collect();
    let thumbs: Vec<String> = thumb.captures_iter(html).map(|c| js_unescape(&c[1])).collect();

    fulls
        .into_iter()
        .enumerate()
        .map(|(i, f)| ImageHit {
            source: SourceKind::Google,
            thumb_url: thumbs.get(i).cloned().unwrap_or_else(|| f.clone()),
            full_url: f,
            width: 0,
            height: 0,
            lang: None,
            title: String::new(),
        })
        .collect()
}

fn parse_thumbnails(html: &str) -> Vec<ImageHit> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"(?:src|data-src)="((?:https://encrypted-tbn[^"]+)|(?:data:image/(?:jpeg|png|webp);base64,[A-Za-z0-9+/=]{2000,}))""#)
            .unwrap()
    });
    re.captures_iter(html)
        .map(|c| {
            let u = js_unescape(&c[1]);
            ImageHit {
                source: SourceKind::Google,
                thumb_url: u.clone(),
                full_url: u,
                width: 0,
                height: 0,
                lang: None,
                title: "Google thumbnail (small)".into(),
            }
        })
        .collect()
}
