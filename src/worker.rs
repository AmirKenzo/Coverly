use crate::{
    config::Config,
    folder::{apply_icon, remove_icon},
    icon::build_ico,
    name::{self, ParsedName},
    net::Http,
    nfo,
    sources::{bing, fanart, google, imdb, local, tmdb, ImageHit, Match, Mode, SourceKind, WebQuery},
    templates::{self, Adjust, LabelStyle, Overlay, Template},
};
use anyhow::{anyhow, bail, Result};
use eframe::egui;
use image::{imageops::FilterType, DynamicImage, RgbaImage};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex, OnceLock,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    Search,
    Local,
}

pub enum Job {
    Search {
        gen: u64,
        parsed: ParsedName,
        web: WebQuery,
        cfg: Config,
    },
    Match {
        gen: u64,
        m: Match,
        season: Option<u16>,
        mode: Mode,
        cfg: Config,
    },
    More {
        gen: u64,
        source: SourceKind,
        web: WebQuery,
        offset: usize,
        cfg: Config,
    },
    Thumb {
        list: List,
        gen: u64,
        idx: usize,
        url: String,
        routes: Vec<Option<String>>,
    },
    Full {
        gen: u64,
        url: String,
        routes: Vec<Option<String>>,
    },
    Preview {
        gen: u64,
        poster: Arc<RgbaImage>,
        template: Template,
        overlay: Overlay,
    },
    Gallery {
        gen: u64,
        poster: Arc<RgbaImage>,
        templates: Arc<Vec<Template>>,
        overlay: Overlay,
    },
    Apply {
        folder: PathBuf,
        poster: Arc<RgbaImage>,
        template: Template,
        overlay: Overlay,
        sizes: Vec<u32>,
    },
    Remove {
        folder: PathBuf,
    },
    Batch {
        items: Vec<(PathBuf, String)>,
        template: Template,
        cfg: Config,
        picture: Option<Arc<RgbaImage>>,
        rating: Option<f32>,
        style: LabelStyle,
        adjust: Adjust,
    },
}

pub enum Msg {
    Matches {
        gen: u64,
        matches: Vec<Match>,
    },
    Hits {
        gen: u64,
        source: SourceKind,
        hits: Vec<ImageHit>,
    },
    SourceFailed {
        gen: u64,
        source: SourceKind,
        error: String,
    },
    GoogleNeedsBrowser {
        gen: u64,
        url: String,
        routes: Vec<Option<String>>,
    },
    Thumb {
        list: List,
        gen: u64,
        idx: usize,
        image: Option<RgbaImage>,
    },
    Full {
        gen: u64,
        image: Result<RgbaImage, String>,
    },
    Preview {
        gen: u64,
        image: RgbaImage,
    },
    GalleryItem {
        gen: u64,
        idx: usize,
        image: RgbaImage,
    },
    Applied(PathBuf),
    ApplyFailed(PathBuf, String),
    Removed(PathBuf),
    Error(String),
    BatchProgress {
        done: usize,
        total: usize,
        current: String,
    },
    BatchDone {
        ok: usize,
        failed: Vec<String>,
    },
}

#[derive(Clone)]
struct Out {
    tx: Sender<Msg>,
    ctx: egui::Context,
}

impl Out {
    fn send(&self, m: Msg) {
        let _ = self.tx.send(m);
        self.ctx.request_repaint();
    }

    fn result(&self, gen: u64, source: SourceKind, r: Result<Vec<ImageHit>>) {
        match r {
            Ok(hits) => self.send(Msg::Hits { gen, source, hits }),
            Err(e) => self.send(Msg::SourceFailed {
                gen,
                source,
                error: explain(&e),
            }),
        }
    }
}

pub fn spawn(ctx: egui::Context) -> (Sender<Job>, Receiver<Msg>) {
    let (jtx, jrx) = channel::<Job>();
    let (mtx, mrx) = channel::<Msg>();
    let out = Out { tx: mtx, ctx };
    std::thread::spawn(move || {
        for job in jrx {
            let o = out.clone();
            if catch_unwind(AssertUnwindSafe(|| handle(job, o))).is_err() {
                out.send(Msg::Error("Something went wrong (details in crash.log).".into()));
            }
        }
    });
    (jtx, mrx)
}

fn net_pool() -> &'static rayon::ThreadPool {
    static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(16)
            .thread_name(|i| format!("net-{i}"))
            .panic_handler(|_| {})
            .build()
            .expect("network thread pool")
    })
}

fn guarded(out: &Out, net: bool, f: impl FnOnce(&Out) + Send + 'static) {
    let o = out.clone();
    let job = move || {
        if catch_unwind(AssertUnwindSafe(|| f(&o))).is_err() {
            o.send(Msg::Error("Something went wrong (details in crash.log).".into()));
        }
    };
    if net {
        net_pool().spawn(job);
    } else {
        rayon::spawn(job);
    }
}

fn http_for(proxy: Option<String>) -> Result<Arc<Http>> {
    static CLIENTS: OnceLock<Mutex<HashMap<Option<String>, Arc<Http>>>> = OnceLock::new();
    let map = CLIENTS.get_or_init(Default::default);
    let mut m = map.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(h) = m.get(&proxy) {
        return Ok(h.clone());
    }
    let h = Arc::new(Http::new(proxy.clone())?);
    m.insert(proxy, h.clone());
    Ok(h)
}

fn is_network(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<reqwest::Error>()
            .map(|r| r.is_connect() || r.is_timeout() || r.is_request())
            .unwrap_or(false)
    })
}

fn explain(e: &anyhow::Error) -> String {
    if is_network(e) {
        format!("couldn't connect ({e:#}). The site may be blocked: switch the proxy on.")
    } else {
        format!("{e:#}")
    }
}

fn with_routes<T>(routes: &[Option<String>], f: impl Fn(&Http) -> Result<T>) -> Result<T> {
    let mut last = anyhow!("no connection route");
    for (i, r) in routes.iter().enumerate() {
        match http_for(r.clone()).and_then(|h| f(&h)) {
            Ok(v) => return Ok(v),
            Err(e) => {
                let retry = i + 1 < routes.len() && is_network(&e);
                last = e;
                if !retry {
                    break;
                }
            }
        }
    }
    Err(last)
}

pub fn prep(d: DynamicImage, max_h: u32) -> RgbaImage {
    let d = if d.height() > max_h {
        d.resize(20_000, max_h, FilterType::Lanczos3)
    } else {
        d
    };
    d.to_rgba8()
}

fn decode(bytes: &[u8]) -> Result<DynamicImage> {
    Ok(image::load_from_memory(bytes)?)
}

fn handle(job: Job, out: Out) {
    match job {
        Job::Search { gen, parsed, web, cfg } => search(gen, parsed, web, cfg, &out),

        Job::Match {
            gen,
            m,
            season,
            mode,
            cfg,
        } => guarded(&out, true, move |out| {
            let active = cfg.active_sources();
            load_match(out, gen, &m, season, mode, &cfg, &active);
        }),

        Job::More {
            gen,
            source,
            web,
            offset,
            cfg,
        } => guarded(&out, true, move |out| {
            let r = match source {
                SourceKind::Bing => with_routes(&cfg.routes_for(SourceKind::Bing), |h| {
                    bing::search_page(h, &web, offset)
                }),
                _ => Ok(Vec::new()),
            };
            out.result(gen, source, r);
        }),

        Job::Thumb {
            list,
            gen,
            idx,
            url,
            routes,
        } => guarded(&out, true, move |out| {
            let image = with_routes(&routes, |h| h.bytes(&url))
                .ok()
                .and_then(|b| decode(&b).ok())
                .map(|d| d.thumbnail(300, 450).to_rgba8());
            out.send(Msg::Thumb { list, gen, idx, image });
        }),

        Job::Full { gen, url, routes } => guarded(&out, true, move |out| {
            let image = with_routes(&routes, |h| h.bytes(&url))
                .and_then(|b| decode(&b))
                .map(|d| prep(d, 900))
                .map_err(|e| explain(&e));
            out.send(Msg::Full { gen, image });
        }),

        Job::Preview {
            gen,
            poster,
            template,
            overlay,
        } => guarded(&out, false, move |out| {
            let image = templates::render(&template, &poster, &overlay, 256);
            out.send(Msg::Preview { gen, image });
        }),

        Job::Gallery {
            gen,
            poster,
            templates,
            overlay,
        } => guarded(&out, false, move |out| {
            let small = if poster.height() > 360 {
                Arc::new(image::imageops::resize(
                    &*poster,
                    (poster.width() * 360 / poster.height()).max(1),
                    360,
                    FilterType::Triangle,
                ))
            } else {
                poster
            };
            templates.par_iter().enumerate().for_each(|(idx, t)| {
                if let Ok(image) = catch_unwind(AssertUnwindSafe(|| templates::render(t, &small, &overlay, 128))) {
                    out.send(Msg::GalleryItem { gen, idx, image });
                }
            });
        }),

        Job::Apply {
            folder,
            poster,
            template,
            overlay,
            sizes,
        } => guarded(&out, false, move |out| {
            let r = (|| -> Result<()> {
                let img = templates::render(&template, &poster, &overlay, 512);
                let ico = build_ico(&img, &sizes)?;
                apply_icon(&folder, &ico)?;
                Ok(())
            })();
            match r {
                Ok(()) => out.send(Msg::Applied(folder)),
                Err(e) => out.send(Msg::ApplyFailed(folder, format!("couldn't set the icon: {e:#}"))),
            }
        }),

        Job::Remove { folder } => match remove_icon(&folder) {
            Ok(()) => out.send(Msg::Removed(folder)),
            Err(e) => out.send(Msg::Error(format!("Couldn't remove the icon: {e:#}"))),
        },

        Job::Batch {
            items,
            template,
            cfg,
            picture,
            rating,
            style,
            adjust,
        } => {
            let o = out.clone();
            std::thread::spawn(move || {
                let spec = BatchSpec {
                    template,
                    cfg,
                    picture,
                    rating,
                    style,
                    adjust,
                };
                if catch_unwind(AssertUnwindSafe(|| batch(items, &spec, &o))).is_err() {
                    o.send(Msg::BatchDone {
                        ok: 0,
                        failed: vec!["The batch stopped because of an error (see crash.log).".into()],
                    });
                }
            });
        }
    }
}

fn search(gen: u64, parsed: ParsedName, web: WebQuery, cfg: Config, out: &Out) {
    let active = cfg.active_sources();

    if active.contains(&SourceKind::Google) {
        let (web, cfg) = (web.clone(), cfg.clone());
        guarded(out, true, move |out| {
            static PLAIN_WORKS: AtomicBool = AtomicBool::new(true);
            let routes = cfg.routes_for(SourceKind::Google);
            if PLAIN_WORKS.load(Ordering::Relaxed) {
                if let Ok(hits) = with_routes(&routes, |h| google::search_http(h, &web)) {
                    out.send(Msg::Hits {
                        gen,
                        source: SourceKind::Google,
                        hits,
                    });
                    return;
                }
                PLAIN_WORKS.store(false, Ordering::Relaxed);
            }
            out.send(Msg::GoogleNeedsBrowser {
                gen,
                url: google::search_url(&web),
                routes,
            });
        });
    }

    if active.contains(&SourceKind::Bing) {
        let (web, cfg) = (web.clone(), cfg.clone());
        guarded(out, true, move |out| {
            let r = with_routes(&cfg.routes_for(SourceKind::Bing), |h| bing::search(h, &web));
            out.result(gen, SourceKind::Bing, r);
        });
    }

    if active.iter().any(|s| s.is_title_based()) {
        let mode = web.mode;
        guarded(out, true, move |out| {
            title_sources(out, gen, &parsed, mode, &cfg, &active)
        });
    }
}

fn title_sources(out: &Out, gen: u64, parsed: &ParsedName, mode: Mode, cfg: &Config, active: &[SourceKind]) {
    let has_tmdb = active.contains(&SourceKind::Tmdb);
    let want_imdb = active.contains(&SourceKind::Imdb);

    let imdb_matches = if want_imdb || !has_tmdb {
        Some(with_routes(&cfg.routes_for(SourceKind::Imdb), |h| {
            imdb::search(h, &parsed.title, parsed.year, parsed.is_tv)
        }))
    } else {
        None
    };

    if want_imdb {
        let r = match (&imdb_matches, mode) {
            (Some(Ok(ms)), Mode::Poster) => Ok(imdb::posters(ms, &parsed.title)),
            (Some(Ok(_)), _) => Ok(Vec::new()),
            (Some(Err(e)), _) => Err(anyhow!("{}", explain(e))),
            (None, _) => Ok(Vec::new()),
        };
        out.result(gen, SourceKind::Imdb, r);
    }

    let matches: Result<Vec<Match>> = if has_tmdb {
        with_routes(&cfg.routes_for(SourceKind::Tmdb), |h| {
            tmdb::search(
                h,
                &cfg.tmdb_key,
                &cfg.language,
                &parsed.title,
                parsed.year,
                parsed.is_tv,
            )
        })
    } else {
        match imdb_matches {
            Some(Ok(ms)) => Ok(ms),
            Some(Err(e)) => Err(e),
            None => Ok(Vec::new()),
        }
    };

    let rest: Vec<SourceKind> = [SourceKind::Tmdb, SourceKind::Fanart]
        .into_iter()
        .filter(|s| active.contains(s))
        .collect();
    match matches {
        Err(e) => {
            for s in rest {
                out.result(gen, s, Err(anyhow!("title lookup failed: {}", explain(&e))));
            }
        }
        Ok(ms) => {
            out.send(Msg::Matches {
                gen,
                matches: ms.clone(),
            });
            match ms.first() {
                Some(m) => load_match(out, gen, m, parsed.season, mode, cfg, active),
                None => {
                    for s in rest {
                        out.result(gen, s, Ok(Vec::new()));
                    }
                }
            }
        }
    }
}

fn load_match(out: &Out, gen: u64, m: &Match, season: Option<u16>, mode: Mode, cfg: &Config, active: &[SourceKind]) {
    if active.contains(&SourceKind::Tmdb) {
        let r = with_routes(&cfg.routes_for(SourceKind::Tmdb), |h| {
            tmdb::images(h, &cfg.tmdb_key, &cfg.language, m, season, mode)
        });
        out.result(gen, SourceKind::Tmdb, r);
    }
    if active.contains(&SourceKind::Fanart) {
        let r = with_routes(&cfg.routes_for(SourceKind::Fanart), |h| {
            fanart::images(h, &cfg.fanart_key, &cfg.tmdb_key, m, season, mode)
        });
        out.result(gen, SourceKind::Fanart, r);
    }
}

struct BatchSpec {
    template: Template,
    cfg: Config,
    picture: Option<Arc<RgbaImage>>,
    rating: Option<f32>,
    style: LabelStyle,
    adjust: Adjust,
}

fn batch(items: Vec<(PathBuf, String)>, spec: &BatchSpec, out: &Out) {
    let total = items.len();
    let done = AtomicUsize::new(0);
    let failed = Mutex::new(Vec::new());
    items.par_iter().for_each(|(f, label)| {
        let name = f
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let r = catch_unwind(AssertUnwindSafe(|| match &spec.picture {
            Some(pic) => {
                let ov = Overlay {
                    rating: spec.rating,
                    name: Some(label.clone()).filter(|l| !l.trim().is_empty()),
                    style: spec.style,
                    adjust: spec.adjust,
                };
                let img = templates::render(&spec.template, pic, &ov, 512);
                build_ico(&img, &spec.cfg.icon_sizes).and_then(|ico| apply_icon(f, &ico).map(|_| ()))
            }
            None => batch_one(f, label, spec),
        }))
        .unwrap_or_else(|_| Err(anyhow!("internal error")));
        match r {
            Ok(()) => out.send(Msg::Applied(f.clone())),
            Err(e) => {
                let why = explain(&e);
                out.send(Msg::ApplyFailed(f.clone(), why.clone()));
                failed
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(format!("{name}: {why}"));
            }
        }
        let d = done.fetch_add(1, Ordering::SeqCst) + 1;
        out.send(Msg::BatchProgress {
            done: d,
            total,
            current: name,
        });
    });
    let failed = failed.into_inner().unwrap_or_else(|e| e.into_inner());
    out.send(Msg::BatchDone {
        ok: total - failed.len(),
        failed,
    });
}

fn batch_one(folder: &Path, label: &str, spec: &BatchSpec) -> Result<()> {
    let (t, cfg) = (&spec.template, &spec.cfg);
    let fname = folder
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let parsed = name::parse(&fname);
    let mut rating = if cfg.show_rating { nfo::rating(folder) } else { None };

    let bytes = if let Some(p) = local::best_poster(folder) {
        std::fs::read(p)?
    } else {
        let mut found: Option<(String, SourceKind)> = None;

        if cfg.has_key(SourceKind::Tmdb) {
            let routes = cfg.routes_for(SourceKind::Tmdb);
            if let Ok(ms) = with_routes(&routes, |h| {
                tmdb::search(
                    h,
                    &cfg.tmdb_key,
                    &cfg.language,
                    &parsed.title,
                    parsed.year,
                    parsed.is_tv,
                )
            }) {
                if let Some(m) = ms.first() {
                    if rating.is_none() && cfg.show_rating {
                        rating = m.rating;
                    }
                    if let Ok(h) = with_routes(&routes, |h| {
                        tmdb::images(h, &cfg.tmdb_key, &cfg.language, m, parsed.season, Mode::Poster)
                    }) {
                        found = h.into_iter().next().map(|h| (h.full_url, SourceKind::Tmdb));
                    }
                }
            }
        }
        if found.is_none() {
            if let Ok(ms) = with_routes(&cfg.routes_for(SourceKind::Imdb), |h| {
                imdb::search(h, &parsed.title, parsed.year, parsed.is_tv)
            }) {
                found = imdb::posters(&ms, &parsed.title)
                    .into_iter()
                    .next()
                    .map(|h| (h.full_url, SourceKind::Imdb));
            }
        }
        if found.is_none() {
            let year = parsed.year.map(|y| y.to_string()).unwrap_or_default();
            let q = WebQuery {
                text: format!("{} {} poster", parsed.title, year)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
                mode: Mode::Poster,
                shape: crate::sources::Shape::Portrait,
                large: false,
            };
            if let Ok(h) = with_routes(&cfg.routes_for(SourceKind::Bing), |h| bing::search(h, &q)) {
                found = h.into_iter().next().map(|h| (h.full_url, SourceKind::Bing));
            }
        }
        let Some((url, source)) = found else {
            bail!("no poster found online")
        };
        with_routes(&cfg.routes_for(source), |h| h.bytes(&url))?
    };

    let poster = prep(decode(&bytes)?, 900);
    let ov = Overlay {
        rating,
        name: Some(label.to_string()).filter(|l| !l.trim().is_empty()),
        style: spec.style,
        adjust: Adjust::default(),
    };
    let img = templates::render(t, &poster, &ov, 512);
    let ico = build_ico(&img, &cfg.icon_sizes)?;
    apply_icon(folder, &ico)?;
    Ok(())
}
