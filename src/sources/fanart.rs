use super::{tmdb, ImageHit, Match, Mode, SourceKind};
use crate::net::Http;
use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Deserialize};

#[derive(Deserialize)]
struct Art {
    url: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    season: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct MovieResp {
    movieposter: Vec<Art>,
    hdmovielogo: Vec<Art>,
    movielogo: Vec<Art>,
    hdmovieclearart: Vec<Art>,
    moviebackground: Vec<Art>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TvResp {
    tvposter: Vec<Art>,
    seasonposter: Vec<Art>,
    hdtvlogo: Vec<Art>,
    clearlogo: Vec<Art>,
    hdclearart: Vec<Art>,
    showbackground: Vec<Art>,
}

fn fetch<T: DeserializeOwned + Default>(http: &Http, url: &str, key: &str) -> Result<T> {
    let resp = http.client.get(url).query(&[("api_key", key)]).send()?;
    if resp.status() == 404 {
        return Ok(T::default());
    }
    Ok(resp.error_for_status()?.json()?)
}

fn hit(a: Art, title: String, (width, height): (u32, u32)) -> ImageHit {
    let thumb = a.url.replacen("/fanart/", "/preview/", 1);
    let lang = match a.lang.as_str() {
        "" | "00" => None,
        l => Some(l.to_string()),
    };
    ImageHit {
        source: SourceKind::Fanart,
        thumb_url: thumb,
        full_url: a.url,
        width,
        height,
        lang,
        title,
    }
}

fn tvdb_for(http: &Http, tmdb_key: &str, m: &Match) -> Result<Option<u64>> {
    if let (Some(id), false) = (m.tmdb_id, tmdb_key.trim().is_empty()) {
        if let Some(t) = tmdb::tvdb_id(http, tmdb_key, id)? {
            return Ok(Some(t));
        }
    }
    let Some(imdb) = &m.imdb_id else { return Ok(None) };
    #[derive(Deserialize)]
    struct Show {
        #[serde(default)]
        externals: Externals,
    }
    #[derive(Deserialize, Default)]
    struct Externals {
        thetvdb: Option<u64>,
    }
    let resp = http
        .client
        .get("https://api.tvmaze.com/lookup/shows")
        .query(&[("imdb", imdb.as_str())])
        .send()?;
    if resp.status() == 404 {
        return Ok(None);
    }
    let show: Show = resp.error_for_status()?.json()?;
    Ok(show.externals.thetvdb)
}

const POSTER: (u32, u32) = (1000, 1426);
const LOGO: (u32, u32) = (800, 310);
const BACKGROUND: (u32, u32) = (1920, 1080);

pub fn images(
    http: &Http,
    fanart_key: &str,
    tmdb_key: &str,
    m: &Match,
    season: Option<u16>,
    mode: Mode,
) -> Result<Vec<ImageHit>> {
    if matches!(mode, Mode::FolderIcon | Mode::Icon) {
        return Ok(Vec::new());
    }
    let key = fanart_key.trim();
    if key.is_empty() {
        bail!("fanart.tv key is not set (Settings)");
    }
    if m.is_tv {
        let Some(tvdb) = tvdb_for(http, tmdb_key, m)? else {
            bail!("couldn't find this show's TVDB id, so fanart.tv can't look it up");
        };
        let r: TvResp = fetch(http, &format!("https://webservice.fanart.tv/v3/tv/{tvdb}"), key)?;
        let t = m.title.clone();
        let mut v = Vec::new();
        match mode {
            Mode::Logo => {
                v.extend(r.hdtvlogo.into_iter().map(|a| hit(a, t.clone(), LOGO)));
                v.extend(r.clearlogo.into_iter().map(|a| hit(a, t.clone(), LOGO)));
                v.extend(
                    r.hdclearart
                        .into_iter()
                        .map(|a| hit(a, format!("{t} · clear art"), LOGO)),
                );
            }
            Mode::Backdrop => v.extend(r.showbackground.into_iter().map(|a| hit(a, t.clone(), BACKGROUND))),
            _ => {
                if let Some(s) = season {
                    let want = s.to_string();
                    v.extend(
                        r.seasonposter
                            .into_iter()
                            .filter(|a| a.season.as_deref() == Some(want.as_str()))
                            .map(|a| hit(a, format!("{t} · Season {s}"), POSTER)),
                    );
                }
                v.extend(r.tvposter.into_iter().map(|a| hit(a, t.clone(), POSTER)));
            }
        }
        Ok(v)
    } else {
        let id = match (m.tmdb_id, &m.imdb_id) {
            (Some(t), _) => t.to_string(),
            (None, Some(i)) => i.clone(),
            (None, None) => bail!("no id to look this movie up"),
        };
        let r: MovieResp = fetch(http, &format!("https://webservice.fanart.tv/v3/movies/{id}"), key)?;
        let t = m.title.clone();
        let mut v = Vec::new();
        match mode {
            Mode::Logo => {
                v.extend(r.hdmovielogo.into_iter().map(|a| hit(a, t.clone(), LOGO)));
                v.extend(r.movielogo.into_iter().map(|a| hit(a, t.clone(), LOGO)));
                v.extend(
                    r.hdmovieclearart
                        .into_iter()
                        .map(|a| hit(a, format!("{t} · clear art"), LOGO)),
                );
            }
            Mode::Backdrop => v.extend(r.moviebackground.into_iter().map(|a| hit(a, t.clone(), BACKGROUND))),
            _ => v.extend(r.movieposter.into_iter().map(|a| hit(a, t.clone(), POSTER))),
        }
        Ok(v)
    }
}
