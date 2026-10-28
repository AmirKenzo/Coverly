use super::{rank_matches, title_rank, ImageHit, Match, SourceKind};
use crate::net::Http;
use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct Resp {
    #[serde(default)]
    d: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    id: String,
    #[serde(default)]
    l: Option<String>,
    #[serde(default)]
    y: Option<u16>,
    #[serde(default)]
    qid: Option<String>,
    #[serde(default)]
    i: Option<Img>,
}

#[derive(Deserialize)]
struct Img {
    #[serde(rename = "imageUrl")]
    image_url: String,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn search(http: &Http, title: &str, year: Option<u16>, prefer_tv: bool) -> Result<Vec<Match>> {
    let q = title.trim().to_lowercase();
    let first = q.chars().find(|c| c.is_ascii_alphanumeric()).unwrap_or('x');
    let url = format!("https://v2.sg.media-imdb.com/suggestion/{first}/{}.json", encode(&q));
    let resp: Resp = http.client.get(url).send()?.error_for_status()?.json()?;

    let mut out: Vec<Match> = resp
        .d
        .into_iter()
        .filter(|it| it.id.starts_with("tt"))
        .filter(|it| {
            !matches!(
                it.qid.as_deref(),
                Some("videoGame") | Some("podcastSeries") | Some("musicVideo")
            )
        })
        .map(|it| {
            let is_tv = matches!(it.qid.as_deref(), Some("tvSeries") | Some("tvMiniSeries"));
            Match {
                tmdb_id: None,
                imdb_id: Some(it.id),
                is_tv,
                title: it.l.unwrap_or_default(),
                year: it.y,
                rating: None,
                poster: it.i.map(|i| (i.image_url, i.width, i.height)),
            }
        })
        .collect();
    rank_matches(&mut out, title, year, prefer_tv);
    Ok(out)
}

pub fn posters(matches: &[Match], query: &str) -> Vec<ImageHit> {
    let exact: Vec<&Match> = matches.iter().filter(|m| title_rank(query, &m.title) == 0).collect();
    let chosen: Vec<&Match> = if exact.is_empty() {
        matches.iter().take(1).collect()
    } else {
        exact
    };
    chosen
        .into_iter()
        .filter_map(|m| {
            let (url, w, h) = m.poster.clone()?;
            let thumb = url.replacen("._V1_", "._V1_UY450_", 1);
            let full = url.replacen("._V1_", "._V1_UY1200_", 1);
            Some(ImageHit {
                source: SourceKind::Imdb,
                thumb_url: thumb,
                full_url: full,
                width: w,
                height: h,
                lang: None,
                title: m.display(),
            })
        })
        .collect()
}
