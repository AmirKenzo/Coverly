use super::{rank_matches, ImageHit, Match, Mode, SourceKind};
use crate::net::Http;
use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Deserialize};

const API: &str = "https://api.themoviedb.org/3";
const IMG: &str = "https://image.tmdb.org/t/p";

fn get<T: DeserializeOwned>(http: &Http, key: &str, path: &str, params: &[(&str, String)]) -> Result<T> {
    let key = key.trim();
    if key.is_empty() {
        bail!("TMDB key is not set (Settings)");
    }
    let mut req = http.client.get(format!("{API}{path}")).query(params);
    req = if key.starts_with("eyJ") {
        req.bearer_auth(key)
    } else {
        req.query(&[("api_key", key)])
    };
    Ok(req.send()?.error_for_status()?.json::<T>()?)
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    results: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    id: u64,
    #[serde(default)]
    media_type: Option<String>,
    title: Option<String>,
    name: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    vote_average: Option<f32>,
}

fn year_of(d: &Option<String>) -> Option<u16> {
    d.as_deref().and_then(|s| s.get(0..4)).and_then(|s| s.parse().ok())
}

pub fn search(
    http: &Http,
    key: &str,
    lang: &str,
    title: &str,
    year: Option<u16>,
    prefer_tv: bool,
) -> Result<Vec<Match>> {
    let page: Page = get(
        http,
        key,
        "/search/multi",
        &[
            ("query", title.to_string()),
            ("language", lang.to_string()),
            ("include_adult", "false".to_string()),
        ],
    )?;

    let mut out: Vec<Match> = page
        .results
        .into_iter()
        .filter_map(|it| {
            let is_tv = match it.media_type.as_deref() {
                Some("movie") => false,
                Some("tv") => true,
                _ => return None,
            };
            let (t, d) = if is_tv {
                (it.name, it.first_air_date)
            } else {
                (it.title, it.release_date)
            };
            Some(Match {
                tmdb_id: Some(it.id),
                imdb_id: None,
                is_tv,
                title: t.unwrap_or_default(),
                year: year_of(&d),
                rating: it.vote_average.filter(|v| *v > 0.0),
                poster: None,
            })
        })
        .collect();
    rank_matches(&mut out, title, year, prefer_tv);
    Ok(out)
}

#[derive(Deserialize)]
struct Images {
    #[serde(default)]
    posters: Vec<Img>,
    #[serde(default)]
    backdrops: Vec<Img>,
    #[serde(default)]
    logos: Vec<Img>,
}

impl Images {
    fn pick(self, mode: Mode) -> Vec<Img> {
        match mode {
            Mode::Backdrop => self.backdrops,
            Mode::Logo => self.logos,
            _ => self.posters,
        }
    }
}

#[derive(Deserialize)]
struct Img {
    file_path: String,
    iso_639_1: Option<String>,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
}

fn hit(p: Img, title: String, mode: Mode) -> ImageHit {
    let (thumb, full) = match mode {
        Mode::Backdrop => ("w300", "w1280"),
        Mode::Logo => ("w300", "w500"),
        _ => ("w342", "w780"),
    };
    ImageHit {
        source: SourceKind::Tmdb,
        thumb_url: format!("{IMG}/{thumb}{}", p.file_path),
        full_url: format!("{IMG}/{full}{}", p.file_path),
        width: p.width,
        height: p.height,
        lang: p.iso_639_1,
        title,
    }
}

pub fn find_by_imdb(http: &Http, key: &str, imdb_id: &str, is_tv: bool) -> Result<Option<u64>> {
    #[derive(Deserialize)]
    struct Found {
        #[serde(default)]
        movie_results: Vec<Id>,
        #[serde(default)]
        tv_results: Vec<Id>,
    }
    #[derive(Deserialize)]
    struct Id {
        id: u64,
    }
    let f: Found = get(
        http,
        key,
        &format!("/find/{imdb_id}"),
        &[("external_source", "imdb_id".to_string())],
    )?;
    let list = if is_tv { f.tv_results } else { f.movie_results };
    Ok(list.first().map(|i| i.id))
}

pub fn images(http: &Http, key: &str, lang: &str, m: &Match, season: Option<u16>, mode: Mode) -> Result<Vec<ImageHit>> {
    if matches!(mode, Mode::FolderIcon | Mode::Icon) {
        return Ok(Vec::new());
    }
    let id = match (m.tmdb_id, &m.imdb_id) {
        (Some(id), _) => id,
        (None, Some(imdb)) => match find_by_imdb(http, key, imdb, m.is_tv)? {
            Some(id) => id,
            None => bail!("TMDB doesn't know this title"),
        },
        (None, None) => bail!("no id to look this title up"),
    };
    let langs = format!("{lang},en,null");
    let mut hits = Vec::new();
    if let (true, Some(s), Mode::Poster) = (m.is_tv, season, mode) {
        let path = format!("/tv/{id}/season/{s}/images");
        if let Ok(im) = get::<Images>(http, key, &path, &[("include_image_language", langs.clone())]) {
            hits.extend(
                im.pick(mode)
                    .into_iter()
                    .map(|p| hit(p, format!("{} · Season {s}", m.title), mode)),
            );
        }
    }
    let kind = if m.is_tv { "tv" } else { "movie" };
    let im: Images = get(
        http,
        key,
        &format!("/{kind}/{id}/images"),
        &[("include_image_language", langs)],
    )?;
    hits.extend(im.pick(mode).into_iter().map(|p| hit(p, m.title.clone(), mode)));
    Ok(hits)
}

pub fn tvdb_id(http: &Http, key: &str, tv_id: u64) -> Result<Option<u64>> {
    #[derive(Deserialize)]
    struct Ext {
        tvdb_id: Option<u64>,
    }
    let e: Ext = get(http, key, &format!("/tv/{tv_id}/external_ids"), &[])?;
    Ok(e.tvdb_id)
}
