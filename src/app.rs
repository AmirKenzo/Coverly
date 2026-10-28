use crate::{
    config::{cache_dir, Config},
    name::{self, clean_folder_name, NameFmt, ParsedName},
    nfo,
    search_cache::{self, Saved, SearchCache},
    sources::{bing, google_web::GoogleWeb, local, title_rank, ImageHit, Match, Mode, Shape, SourceKind, WebQuery},
    templates::{self, Adjust, LabelStyle, Overlay, Template},
    ui::*,
    update::{self, Note, Release, State as UpdateState, Updater},
    videos,
    worker::{self, Job, List, Msg},
};
use eframe::egui::{
    self, load::SizedTexture, Align, Align2, Color32, ColorImage, FontFamily, FontId, Margin, RichText, Rounding,
    Sense, Stroke, TextureHandle, TextureOptions, Vec2,
};
use image::RgbaImage;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        mpsc::{Receiver, Sender},
        Arc,
    },
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const TITLE_H: f32 = 38.0;
const TILE: Vec2 = Vec2::new(152.0, 228.0);
const PAGE_SHOWN: usize = 60;
const THUMBS_PER_FRAME: usize = 12;
const GTILE: Vec2 = Vec2::new(74.0, 74.0);

const SITES: [(&str, &str); 8] = [
    ("Any site", ""),
    ("theposterdb.com", "theposterdb.com"),
    ("impawards.com", "impawards.com"),
    ("deviantart.com (folder icons)", "deviantart.com"),
    ("imdb.com", "imdb.com"),
    ("themoviedb.org", "themoviedb.org"),
    ("fanart.tv", "fanart.tv"),
    ("pinterest.com", "pinterest.com"),
];

struct Entry {
    hit: ImageHit,
    thumb: Option<TextureHandle>,
    requested: bool,
    failed: bool,
    removed: bool,
}

impl Entry {
    fn new(hit: ImageHit) -> Self {
        Self {
            hit,
            thumb: None,
            requested: false,
            failed: false,
            removed: false,
        }
    }
}

#[derive(Clone, PartialEq)]
enum QState {
    Pending,
    Done,
    Skipped,
    Failed(String),
}

struct QueueItem {
    path: PathBuf,
    state: QState,
}

struct RenameDialog {
    title: String,
    year: Option<u16>,
    fmt: NameFmt,
    name: String,
    from: String,
}

struct VideoDialog {
    rows: Vec<videos::Plan>,
    fmt: NameFmt,
    move_extras: bool,
    seasons: bool,
    auto: bool,
    banner: bool,
    tpl: usize,
}

fn search_key(web: &WebQuery, parsed: &ParsedName, lang: &str) -> String {
    format!(
        "1|{:?}|{:?}|{}|{}|{}|{:?}|{:?}|{}",
        web.mode,
        web.shape,
        web.large,
        web.text.to_lowercase(),
        parsed.title.to_lowercase(),
        parsed.year,
        parsed.season,
        lang
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Subfolders,
    Siblings,
}

struct MultiRow {
    path: PathBuf,
    on: bool,
    label: String,
    is_self: bool,
}

struct Multi {
    scope: Scope,
    same_picture: bool,
    rows: Vec<MultiRow>,
}

fn auto_label(p: &Path) -> String {
    let n = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(s) = name::parse(&n).season {
        return format!("S{s}");
    }
    let lower = n.to_lowercase();
    let digits: String = lower.chars().filter(|c| c.is_ascii_digit()).collect();
    if (lower.starts_with("season") || lower.starts_with("fasl") || lower.starts_with("فصل")) && !digits.is_empty() {
        return format!("S{}", digits.trim_start_matches('0'));
    }
    if lower.contains("special") {
        return "Specials".into();
    }
    String::new()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextFilter {
    All,
    Mine,
    NoText,
}

pub struct App {
    cfg: Config,
    jobs: Sender<Job>,
    msgs: Receiver<Msg>,
    google: GoogleWeb,
    elevated: bool,

    folder: Option<PathBuf>,
    parsed: ParsedName,
    title_q: String,
    year_q: String,
    nfo_rating: Option<f32>,
    mode: Mode,

    gen: u64,
    pending: HashSet<SourceKind>,
    errors: Vec<(SourceKind, String)>,
    matches: Vec<Match>,
    match_idx: usize,
    results: Vec<Entry>,
    local: Vec<Entry>,
    local_gen: u64,
    tab: List,

    hidden: HashSet<SourceKind>,
    shape: Shape,
    min_h: u32,
    hide_unknown: bool,
    text_filter: TextFilter,

    picked: Option<(List, usize)>,
    full_gen: u64,
    loading_full: bool,
    poster: Option<Arc<RgbaImage>>,

    templates: Arc<Vec<Template>>,
    families: Vec<String>,
    family: String,
    tpl_idx: usize,
    poster_tpl_idx: usize,
    gallery: Vec<Option<TextureHandle>>,
    gallery_gen: u64,
    preview: Option<TextureHandle>,
    preview_gen: u64,

    status: String,
    last_error: Option<String>,
    show_settings: bool,
    draft: Config,
    batch: Option<(usize, usize, String)>,
    multi: Option<Multi>,
    label_text: String,
    google_rect: Option<egui::Rect>,

    seen: HashSet<String>,
    shown_limit: usize,
    bing_next: usize,
    exhausted: HashSet<SourceKind>,
    loading_more: HashSet<SourceKind>,

    queue: Vec<QueueItem>,
    queue_pos: Option<usize>,
    rename: Option<RenameDialog>,
    adjust: Adjust,
    paste_count: u32,

    cache: SearchCache,
    cur_key: Option<String>,
    saved_at: Option<u64>,
    videos: Option<VideoDialog>,
    logo: TextureHandle,
    updater: Updater,
    show_update: bool,
}

enum UpdateAction {
    Download(Release),
    Cancel,
    Install,
    Later,
    Skip(String),
    Check,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>) -> Self {
        setup_style(&cc.egui_ctx);
        let cfg = Config::load();
        let (jobs, msgs) = worker::spawn(cc.egui_ctx.clone());
        let templates = Arc::new(templates::catalog());
        let families = templates::families(&templates);
        let tpl_idx = templates.iter().position(|t| t.id == cfg.last_template).unwrap_or(0);
        let n = templates.len();
        let updater = Updater::new(cc.egui_ctx.clone());
        if cfg.update_check {
            updater.check(&cfg, false);
        }

        let mut app = Self {
            draft: cfg.clone(),
            cfg,
            jobs,
            msgs,
            google: GoogleWeb::new(),
            elevated: is_elevated(),
            folder: None,
            parsed: ParsedName::default(),
            title_q: String::new(),
            year_q: String::new(),
            nfo_rating: None,
            mode: Mode::Poster,
            gen: 0,
            pending: HashSet::new(),
            errors: Vec::new(),
            matches: Vec::new(),
            match_idx: 0,
            results: Vec::new(),
            local: Vec::new(),
            local_gen: 0,
            tab: List::Search,
            hidden: HashSet::new(),
            shape: Shape::Any,
            min_h: 0,
            hide_unknown: false,
            text_filter: TextFilter::All,
            picked: None,
            full_gen: 0,
            loading_full: false,
            poster: None,
            templates,
            families,
            family: "All".into(),
            tpl_idx,
            poster_tpl_idx: tpl_idx,
            gallery: (0..n).map(|_| None).collect(),
            gallery_gen: 0,
            preview: None,
            preview_gen: 0,
            status: "Drag a movie or series folder onto this window.".into(),
            last_error: None,
            show_settings: false,
            batch: None,
            multi: None,
            label_text: String::new(),
            google_rect: None,
            seen: HashSet::new(),
            shown_limit: PAGE_SHOWN,
            bing_next: bing::PAGE,
            exhausted: HashSet::new(),
            loading_more: HashSet::new(),
            queue: Vec::new(),
            queue_pos: None,
            rename: None,
            adjust: Adjust::default(),
            paste_count: 0,
            cache: SearchCache::new(),
            cur_key: None,
            saved_at: None,
            videos: None,
            logo: load_tex(&cc.egui_ctx, "logo", &templates::app_icon(64)),
            updater,
            show_update: false,
        };
        if let Some(p) = start {
            app.open_folder(p);
        }
        app
    }

    fn send(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    fn open_folder(&mut self, p: PathBuf) {
        let fname = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        self.parsed = name::parse(&fname);
        self.title_q = self.parsed.title.clone();
        self.year_q = self.parsed.year.map(|y| y.to_string()).unwrap_or_default();
        self.label_text = match self.parsed.season {
            Some(n) => format!("S{n}"),
            None => self.parsed.title.clone(),
        };
        self.nfo_rating = nfo::rating(&p);

        self.local_gen += 1;
        self.local = local::list(&p).into_iter().map(Entry::new).collect();

        self.folder = Some(p);
        self.picked = None;
        self.poster = None;
        self.preview = None;
        self.last_error = None;
        self.status = format!("Detected: {}", self.parsed.display());
        self.search();
    }

    fn web_query(&self) -> WebQuery {
        let icons = matches!(self.mode, Mode::FolderIcon | Mode::Icon);
        let mut parts = vec![self.title_q.trim().to_string()];
        if !icons && self.mode != Mode::Logo && !self.year_q.trim().is_empty() {
            parts.push(self.year_q.trim().to_string());
        }
        if self.mode == Mode::Poster {
            if let Some(n) = self.parsed.season {
                parts.push(format!("season {n}"));
            }
        }
        parts.push(self.mode.keywords().to_string());
        if !self.cfg.query_extra.trim().is_empty() {
            parts.push(self.cfg.query_extra.trim().to_string());
        }
        if !self.cfg.query_site.trim().is_empty() {
            parts.push(format!("site:{}", self.cfg.query_site.trim()));
        } else if icons {
            let sites: Vec<String> = Mode::ICON_SITES.iter().map(|s| format!("site:{s}")).collect();
            parts.push(sites.join(" OR "));
        }
        parts.retain(|s| !s.is_empty());
        WebQuery {
            text: parts.join(" "),
            mode: self.mode,
            shape: self.shape,
            large: self.min_h >= 1000,
        }
    }

    fn query_label(&self) -> String {
        let q = self.web_query().text;
        match q.find(" site:") {
            Some(i) if matches!(self.mode, Mode::FolderIcon | Mode::Icon) && self.cfg.query_site.is_empty() => {
                format!("{} (icon sites)", &q[..i])
            }
            _ => q,
        }
    }

    fn search(&mut self) {
        self.search_with(false);
    }

    fn search_with(&mut self, fresh: bool) {
        if self.title_q.trim().is_empty() {
            self.status = "Type a title, or drag a folder onto the window.".into();
            return;
        }
        self.remember();
        self.gen += 1;
        self.results.clear();
        self.seen.clear();
        self.shown_limit = PAGE_SHOWN;
        self.bing_next = bing::PAGE;
        self.exhausted.clear();
        self.loading_more.clear();
        self.matches.clear();
        self.match_idx = 0;
        self.errors.clear();
        self.saved_at = None;
        self.tab = List::Search;

        let mut parsed = self.parsed.clone();
        parsed.title = self.title_q.trim().to_string();
        parsed.year = self.year_q.trim().parse().ok();

        let web = self.web_query();
        let key = search_key(&web, &parsed, &self.cfg.language);
        self.cur_key = Some(key.clone());

        let active = self.cfg.active_sources();
        if active.is_empty() {
            self.pending.clear();
            self.status = "No image sources are switched on. Open Settings.".into();
            return;
        }
        let mut todo = active;
        if fresh {
            self.cache.forget(&key);
        } else if let Some(saved) = self.cache.get(&key) {
            for h in saved.hits {
                if self.seen.insert(h.full_url.clone()) {
                    self.results.push(Entry::new(h));
                }
            }
            self.matches = saved.matches;
            self.bing_next = saved.bing_next.max(bing::PAGE);
            if saved.done.contains(&SourceKind::Google) {
                self.exhausted.insert(SourceKind::Google);
            }
            todo.retain(|s| !saved.done.contains(s));
            self.saved_at = Some(saved.saved_at);
            if self.cfg.show_rating || self.cfg.show_name {
                self.overlay_changed();
            }
        }
        self.pending = todo.iter().copied().collect();
        let _ = self.cfg.save();
        if todo.is_empty() {
            self.refresh_status();
            return;
        }
        self.status = format!("Searching “{}”…", self.query_label());
        let mut cfg = self.cfg.clone();
        cfg.sources = todo;
        self.send(Job::Search {
            gen: self.gen,
            parsed,
            web,
            cfg,
        });
    }

    fn remember(&mut self) {
        let Some(key) = self.cur_key.clone() else { return };
        let hits: Vec<ImageHit> = self
            .results
            .iter()
            .filter(|e| !e.removed && !e.failed)
            .map(|e| e.hit.clone())
            .collect();
        if hits.is_empty() {
            return;
        }
        let failed: HashSet<SourceKind> = self.errors.iter().map(|(s, _)| *s).collect();
        let google_busy = self.google.busy();
        let done = self
            .cfg
            .active_sources()
            .into_iter()
            .filter(|s| !self.pending.contains(s) && !failed.contains(s))
            .filter(|s| !(google_busy && *s == SourceKind::Google))
            .collect();
        self.cache.put(Saved {
            key,
            saved_at: self.saved_at.unwrap_or_else(search_cache::now),
            hits,
            matches: self.matches.clone(),
            done,
            bing_next: self.bing_next,
        });
    }

    fn maybe_remember(&mut self) {
        if self.pending.is_empty() && self.loading_more.is_empty() {
            self.remember();
        }
    }

    fn set_mode(&mut self, m: Mode) {
        if m == self.mode {
            return;
        }
        let was = self.mode.wants_original();
        self.mode = m;
        if m.wants_original() && !was {
            self.poster_tpl_idx = self.tpl_idx;
            if let Some(i) = self.templates.iter().position(|t| t.id == "original-as-it-is") {
                self.tpl_idx = i;
            }
        } else if !m.wants_original() && was {
            self.tpl_idx = self.poster_tpl_idx.min(self.templates.len().saturating_sub(1));
        }
        self.shape = match m {
            Mode::Poster => Shape::Any,
            Mode::FolderIcon | Mode::Icon => Shape::Any,
            Mode::Logo => Shape::Any,
            Mode::Backdrop => Shape::Landscape,
        };
        self.search();
        self.request_preview();
    }

    fn select_match(&mut self, i: usize) {
        if i >= self.matches.len() {
            return;
        }
        self.match_idx = i;
        let active = self.cfg.active_sources();
        let mut any = false;
        for s in [SourceKind::Tmdb, SourceKind::Fanart] {
            if active.contains(&s) {
                self.pending.insert(s);
                any = true;
            }
        }
        if any {
            for e in self.results.iter_mut() {
                if matches!(e.hit.source, SourceKind::Tmdb | SourceKind::Fanart) {
                    e.removed = true;
                }
            }
            self.errors
                .retain(|(s, _)| !matches!(s, SourceKind::Tmdb | SourceKind::Fanart));
            let m = self.matches[i].clone();
            self.send(Job::Match {
                gen: self.gen,
                m,
                season: self.parsed.season,
                mode: self.mode,
                cfg: self.cfg.clone(),
            });
        }
        self.overlay_changed();
    }

    fn routes(&self, hit: &ImageHit) -> Vec<Option<String>> {
        if !hit.full_url.starts_with("http") {
            return vec![None];
        }
        let s = if hit.source == SourceKind::Local {
            SourceKind::Bing
        } else {
            hit.source
        };
        self.cfg.routes_for(s)
    }

    fn send_thumb(&mut self, list: List, idx: usize) {
        let gen = match list {
            List::Search => self.gen,
            List::Local => self.local_gen,
        };
        let entries = match list {
            List::Search => &self.results,
            List::Local => &self.local,
        };
        let Some(e) = entries.get(idx) else { return };
        if e.requested {
            return;
        }
        let job = Job::Thumb {
            list,
            gen,
            idx,
            url: e.hit.thumb_url.clone(),
            routes: self.routes(&e.hit),
        };
        self.send(job);
        let entries = match list {
            List::Search => &mut self.results,
            List::Local => &mut self.local,
        };
        if let Some(e) = entries.get_mut(idx) {
            e.requested = true;
        }
    }

    fn load_more(&mut self, listed: usize) {
        if listed > self.shown_limit {
            self.shown_limit += PAGE_SHOWN;
            return;
        }
        if !self.pending.is_empty() || !self.loading_more.is_empty() || self.google.busy() {
            return;
        }
        let active = self.cfg.active_sources();
        if active.contains(&SourceKind::Bing) && !self.exhausted.contains(&SourceKind::Bing) && self.bing_next < 600 {
            self.loading_more.insert(SourceKind::Bing);
            self.send(Job::More {
                gen: self.gen,
                source: SourceKind::Bing,
                web: self.web_query(),
                offset: self.bing_next,
                cfg: self.cfg.clone(),
            });
            self.bing_next += bing::PAGE;
        }
        if active.contains(&SourceKind::Google)
            && !self.exhausted.contains(&SourceKind::Google)
            && self.google.more(self.gen)
        {
            self.loading_more.insert(SourceKind::Google);
        }
        if !self.loading_more.is_empty() {
            self.shown_limit += PAGE_SHOWN;
            self.status = "Loading more pictures…".into();
        }
    }

    fn can_load_more(&self) -> bool {
        let active = self.cfg.active_sources();
        [SourceKind::Bing, SourceKind::Google]
            .iter()
            .any(|s| active.contains(s) && !self.exhausted.contains(s))
    }

    fn pick(&mut self, list: List, idx: usize) {
        let entries = match list {
            List::Search => &self.results,
            List::Local => &self.local,
        };
        let Some(e) = entries.get(idx) else { return };
        let url = e.hit.full_url.clone();
        let routes = self.routes(&e.hit);
        self.picked = Some((list, idx));
        self.full_gen += 1;
        self.loading_full = true;
        self.send(Job::Full {
            gen: self.full_gen,
            url,
            routes,
        });
    }

    fn add_local(&mut self, hit: ImageHit) {
        let idx = self.local.len();
        self.local.push(Entry::new(hit));
        self.send_thumb(List::Local, idx);
        self.tab = List::Local;
        self.pick(List::Local, idx);
    }

    fn use_file(&mut self, p: PathBuf) {
        self.add_local(local::hit_for(&p));
    }

    fn handle_paste(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let (text, ctrl_v) = ctx.input(|i| {
            let mut t = None;
            for e in &i.events {
                if let egui::Event::Paste(s) = e {
                    t = Some(s.clone());
                }
            }
            (t, i.modifiers.command && i.key_pressed(egui::Key::V))
        });
        if let Some(t) = text {
            self.paste_text(t);
        } else if ctrl_v {
            self.paste_image();
        }
    }

    fn paste_from_clipboard(&mut self) {
        let text = arboard::Clipboard::new()
            .and_then(|mut c| c.get_text())
            .unwrap_or_default();
        if !text.trim().is_empty() && self.paste_text(text) {
            return;
        }
        self.paste_image();
    }

    fn paste_text(&mut self, t: String) -> bool {
        let t = t.trim().trim_matches('"').trim().to_string();
        let p = PathBuf::from(&t);
        if p.is_dir() {
            self.open_folder(p);
        } else if p.is_file() && local::is_image(&p) {
            self.use_file(p);
        } else if t.starts_with("http://") || t.starts_with("https://") {
            self.add_local(ImageHit {
                source: SourceKind::Local,
                thumb_url: t.clone(),
                full_url: t,
                width: 0,
                height: 0,
                lang: None,
                title: "Pasted link".into(),
            });
        } else {
            self.status = "The clipboard has no picture, image link or folder path.".into();
            return false;
        }
        true
    }

    fn paste_image(&mut self) {
        let img = arboard::Clipboard::new().and_then(|mut c| c.get_image());
        let Ok(img) = img else {
            self.status = "Nothing to paste. Copy an image, an image link or a folder path first.".into();
            return;
        };
        let Some(rgba) = RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned()) else {
            return;
        };
        let dir = cache_dir().join("pasted");
        let _ = std::fs::create_dir_all(&dir);
        self.paste_count += 1;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = dir.join(format!("pasted-{stamp}-{}.png", self.paste_count));
        match rgba.save(&path) {
            Ok(()) => {
                let mut hit = local::hit_for(&path);
                hit.title = "Pasted image".into();
                self.add_local(hit);
            }
            Err(e) => self.last_error = Some(format!("Couldn't use the pasted image: {e}")),
        }
    }

    fn overlay(&self) -> Overlay {
        let rating = if self.cfg.show_rating {
            self.nfo_rating
                .or_else(|| self.matches.get(self.match_idx).and_then(|m| m.rating))
        } else {
            None
        };
        let name = if self.cfg.show_name {
            let t = self.label_text.trim().to_string();
            (!t.is_empty()).then_some(t)
        } else {
            None
        };
        Overlay {
            rating,
            name,
            style: self.cfg.label_style,
            adjust: self.adjust,
        }
    }

    fn multi_rows(&self, scope: Scope) -> Vec<MultiRow> {
        let Some(folder) = self.folder.clone() else {
            return Vec::new();
        };
        fn dirs_in(dir: &Path) -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
                .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
                .unwrap_or_default();
            v.sort();
            v
        }
        let own_label = if self.cfg.show_name {
            self.label_text.trim().to_string()
        } else {
            String::new()
        };
        let mut rows = Vec::new();
        match scope {
            Scope::Subfolders => {
                rows.push(MultiRow {
                    path: folder.clone(),
                    on: true,
                    label: own_label,
                    is_self: true,
                });
                for p in dirs_in(&folder) {
                    rows.push(MultiRow {
                        label: auto_label(&p),
                        path: p,
                        on: true,
                        is_self: false,
                    });
                }
            }
            Scope::Siblings => {
                if let Some(parent) = folder.parent() {
                    for p in dirs_in(parent) {
                        let is_self = p == folder;
                        rows.push(MultiRow {
                            label: if is_self { own_label.clone() } else { String::new() },
                            path: p,
                            on: true,
                            is_self,
                        });
                    }
                }
            }
        }
        rows
    }

    fn open_multi(&mut self) {
        let scope = Scope::Subfolders;
        let rows = self.multi_rows(scope);
        self.multi = Some(Multi {
            scope,
            same_picture: self.poster.is_some(),
            rows,
        });
    }

    fn overlay_changed(&mut self) {
        self.request_preview();
        self.request_gallery();
    }

    fn request_preview(&mut self) {
        let (Some(p), Some(t)) = (self.poster.clone(), self.templates.get(self.tpl_idx).cloned()) else {
            return;
        };
        self.preview_gen += 1;
        self.send(Job::Preview {
            gen: self.preview_gen,
            poster: p,
            template: t,
            overlay: self.overlay(),
        });
    }

    fn request_gallery(&mut self) {
        let Some(p) = self.poster.clone() else { return };
        self.gallery_gen += 1;
        self.send(Job::Gallery {
            gen: self.gallery_gen,
            poster: p,
            templates: self.templates.clone(),
            overlay: self.overlay(),
        });
    }

    fn apply(&mut self) {
        let (Some(folder), Some(poster), Some(template)) = (
            self.folder.clone(),
            self.poster.clone(),
            self.templates.get(self.tpl_idx).cloned(),
        ) else {
            return;
        };
        self.status = format!("Setting the icon on “{}”…", short(&folder));
        self.send(Job::Apply {
            folder,
            poster,
            template,
            overlay: self.overlay(),
            sizes: self.cfg.icon_sizes.clone(),
        });
        if self.cfg.auto_next && self.queue_pos.is_some() {
            self.queue_next();
        }
    }

    fn add_to_queue(&mut self, dirs: Vec<PathBuf>, open: bool) {
        let mut first_new = None;
        for d in dirs {
            if self.queue.iter().any(|q| q.path == d) {
                continue;
            }
            if first_new.is_none() {
                first_new = Some(self.queue.len());
            }
            self.queue.push(QueueItem {
                path: d,
                state: QState::Pending,
            });
        }
        if open && (self.queue_pos.is_none() || self.folder.is_none()) {
            if let Some(i) = first_new {
                self.open_queue_item(i);
            }
        }
        self.status = format!("{} folders in the queue", self.queue.len());
    }

    fn banner_for(&self, p: &Path) -> String {
        if !self.cfg.show_name {
            return String::new();
        }
        let season = auto_label(p);
        if !season.is_empty() {
            return season;
        }
        name::parse(&short(p)).title
    }

    fn poster_template(&self) -> usize {
        if self.mode.wants_original() {
            self.poster_tpl_idx
        } else {
            self.tpl_idx
        }
    }

    fn queue_auto(&mut self) {
        let items: Vec<(PathBuf, String)> = self
            .queue
            .iter()
            .filter(|q| q.state == QState::Pending)
            .map(|q| (q.path.clone(), self.banner_for(&q.path)))
            .collect();
        let tpl = self.poster_template();
        self.start_auto(items, tpl);
    }

    fn start_auto(&mut self, items: Vec<(PathBuf, String)>, tpl: usize) {
        if items.is_empty() {
            return;
        }
        if self.batch.is_some() {
            self.status = "Wait for the folders being set now to finish.".into();
            return;
        }
        let Some(template) = self.templates.get(tpl).cloned() else {
            return;
        };
        self.batch = Some((0, items.len(), String::new()));
        self.status = format!("Setting {} icons automatically…", items.len());
        self.send(Job::Batch {
            items,
            template,
            cfg: self.cfg.clone(),
            picture: None,
            rating: None,
            style: self.cfg.label_style,
            adjust: Adjust::default(),
        });
    }

    fn open_videos(&mut self, mut files: Vec<PathBuf>) {
        files.sort();
        files.dedup();
        let tpl = self.poster_template();
        let d = self.videos.get_or_insert_with(|| VideoDialog {
            rows: Vec::new(),
            fmt: NameFmt::Paren,
            move_extras: true,
            seasons: true,
            auto: true,
            banner: true,
            tpl,
        });
        for f in files {
            if !d.rows.iter().any(|r| r.video == f) {
                d.rows.push(videos::Plan::new(f, d.fmt));
            }
        }
    }

    fn create_video_folders(&mut self) {
        let Some(d) = self.videos.take() else { return };
        let mut made: Vec<PathBuf> = Vec::new();
        let mut errors = Vec::new();
        for r in d.rows.iter().filter(|r| r.on) {
            match videos::execute(r, d.move_extras, d.seasons) {
                Ok(root) => {
                    if !made.contains(&root) {
                        made.push(root);
                    }
                }
                Err(e) => errors.push(format!("{}: {e:#}", r.file_name())),
            }
        }
        self.last_error = (!errors.is_empty()).then(|| errors.join("\n"));
        if made.is_empty() {
            return;
        }
        let n = made.len();
        if d.auto {
            let items: Vec<(PathBuf, String)> = made
                .iter()
                .map(|p| {
                    let label = if d.banner {
                        name::parse(&short(p)).title
                    } else {
                        String::new()
                    };
                    (p.clone(), label)
                })
                .collect();
            self.add_to_queue(made, false);
            self.start_auto(items, d.tpl);
        } else {
            self.add_to_queue(made, true);
            self.status = format!("{n} folders made. Pick a picture for each one.");
        }
    }

    fn open_queue_item(&mut self, i: usize) {
        if let Some(q) = self.queue.get(i) {
            let p = q.path.clone();
            self.queue_pos = Some(i);
            self.open_folder(p);
        }
    }

    fn queue_next(&mut self) {
        let n = self.queue.len();
        let start = self.queue_pos.map(|p| p + 1).unwrap_or(0);
        let next = (0..n)
            .map(|k| (start + k) % n)
            .find(|&i| self.queue[i].state == QState::Pending && Some(i) != self.queue_pos);
        match next {
            Some(i) => self.open_queue_item(i),
            None => {
                self.status = "Queue finished. All folders are done.".into();
            }
        }
    }

    fn queue_skip(&mut self) {
        if let Some(i) = self.queue_pos {
            if let Some(q) = self.queue.get_mut(i) {
                if q.state == QState::Pending {
                    q.state = QState::Skipped;
                }
            }
            self.queue_next();
        }
    }

    fn queue_mark(&mut self, path: &Path, state: QState) {
        if let Some(q) = self.queue.iter_mut().find(|q| q.path == path) {
            q.state = state;
        }
    }

    fn open_rename(&mut self) {
        if self.folder.is_none() {
            return;
        }
        let (title, year, from) = match self.matches.get(self.match_idx) {
            Some(m) if title_rank(&self.title_q, &m.title) <= 1 && !m.title.is_empty() => {
                let src = if self.cfg.has_key(SourceKind::Tmdb) && m.tmdb_id.is_some() {
                    "TMDB"
                } else {
                    "IMDb"
                };
                (m.title.clone(), m.year, format!("{src}: {}", m.display()))
            }
            _ => (
                self.title_q.trim().to_string(),
                self.year_q.trim().parse().ok(),
                "the title and year boxes".to_string(),
            ),
        };
        let fmt = NameFmt::Paren;
        let mut name = clean_folder_name(&fmt.apply(&title, year));
        if self.parsed.is_tv {
            if let Some(n) = self.parsed.season {
                name = format!("{name} - Season {n}");
            }
        }
        self.rename = Some(RenameDialog {
            title,
            year,
            fmt,
            name,
            from,
        });
    }

    fn do_rename(&mut self, new_name: &str) {
        let Some(old) = self.folder.clone() else { return };
        let name = clean_folder_name(new_name);
        if name.is_empty() {
            self.last_error = Some("The new name is empty.".into());
            return;
        }
        let Some(parent) = old.parent() else { return };
        let new = parent.join(&name);
        if new == old {
            self.rename = None;
            return;
        }
        let only_case = old.to_string_lossy().to_lowercase() == new.to_string_lossy().to_lowercase();
        if new.exists() && !only_case {
            self.last_error = Some(format!("A folder named “{name}” already exists there."));
            return;
        }
        match std::fs::rename(&old, &new) {
            Ok(()) => {
                self.rename = None;
                if let Some(q) = self.queue.iter_mut().find(|q| q.path == old) {
                    q.path = new.clone();
                }
                self.folder = Some(new.clone());
                self.local_gen += 1;
                self.local = local::list(&new).into_iter().map(Entry::new).collect();
                if matches!(self.picked, Some((List::Local, _))) {
                    self.picked = None;
                }
                self.last_error = None;
                self.status = format!("Renamed to “{name}”");
            }
            Err(e) => {
                self.last_error = Some(format!(
                    "Couldn't rename: {e}. Close the folder in Explorer and any player using a file in it, then try again."
                ));
            }
        }
    }

    fn passes(&self, e: &Entry) -> bool {
        if e.removed || e.failed || self.hidden.contains(&e.hit.source) {
            return false;
        }
        let (w, h) = (e.hit.width, e.hit.height);
        if w > 0 && h > 0 {
            let ratio = h as f32 / w as f32;
            let ok = match self.shape {
                Shape::Any => true,
                Shape::Portrait => ratio > 1.15,
                Shape::Landscape => ratio < 0.87,
                Shape::Square => (0.8..=1.25).contains(&ratio),
            };
            if !ok || h < self.min_h {
                return false;
            }
        } else if self.hide_unknown {
            return false;
        }
        match self.text_filter {
            TextFilter::All => true,
            TextFilter::Mine => e.hit.lang.as_deref() == Some(self.cfg.language.trim()),
            TextFilter::NoText => matches!(e.hit.source, SourceKind::Tmdb | SourceKind::Fanart) && e.hit.lang.is_none(),
        }
    }

    fn refresh_status(&mut self) {
        if self.pending.is_empty() {
            let n = self.results.iter().filter(|e| !e.removed).count();
            self.status = match self.saved_at {
                Some(t) => format!(
                    "{n} pictures for “{}” (saved {}, no new search)",
                    self.query_label(),
                    search_cache::age(t)
                ),
                None => format!("{n} pictures found for “{}”", self.query_label()),
            };
        } else {
            let names: Vec<&str> = SourceKind::ONLINE
                .iter()
                .filter(|s| self.pending.contains(s))
                .map(|s| s.label())
                .collect();
            self.status = format!("Searching {}…", names.join(", "));
        }
    }

    fn on_hits(&mut self, source: SourceKind, hits: Vec<ImageHit>) {
        self.pending.remove(&source);
        let mut added = 0;
        for h in hits {
            if self.seen.insert(h.full_url.clone()) {
                self.results.push(Entry::new(h));
                added += 1;
            }
        }
        if self.loading_more.remove(&source) && added == 0 {
            self.exhausted.insert(source);
        }
        self.refresh_status();
        self.maybe_remember();
    }

    fn on_failed(&mut self, source: SourceKind, error: String) {
        self.pending.remove(&source);
        if self.loading_more.remove(&source) {
            self.exhausted.insert(source);
            self.refresh_status();
            self.maybe_remember();
            return;
        }
        self.errors.retain(|(s, _)| *s != source);
        self.errors.push((source, error));
        self.refresh_status();
        self.maybe_remember();
    }

    fn poll(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        while let Ok(m) = self.msgs.try_recv() {
            match m {
                Msg::Matches { gen, matches } if gen == self.gen => {
                    self.matches = matches;
                    self.match_idx = 0;
                    if self.cfg.show_rating || self.cfg.show_name {
                        self.overlay_changed();
                    }
                }
                Msg::Hits { gen, source, hits } if gen == self.gen => self.on_hits(source, hits),
                Msg::SourceFailed { gen, source, error } if gen == self.gen => self.on_failed(source, error),
                Msg::GoogleNeedsBrowser { gen, url, routes } if gen == self.gen => self.google.start(gen, url, routes),
                Msg::Thumb { list, gen, idx, image } => {
                    let (entries, current) = match list {
                        List::Search => (&mut self.results, self.gen),
                        List::Local => (&mut self.local, self.local_gen),
                    };
                    if gen == current {
                        if let Some(e) = entries.get_mut(idx) {
                            match image {
                                Some(img) => {
                                    e.thumb = Some(load_tex(ctx, &format!("t-{list:?}-{gen}-{idx}"), &img));
                                }
                                None => e.failed = true,
                            }
                        }
                    }
                }
                Msg::Full { gen, image } if gen == self.full_gen => {
                    self.loading_full = false;
                    match image {
                        Ok(img) => {
                            self.poster = Some(Arc::new(img));
                            self.adjust = Adjust::default();
                            self.last_error = None;
                            self.overlay_changed();
                        }
                        Err(e) => {
                            self.last_error = Some(format!("Couldn't load that picture: {e}"));
                            if let Some((list, idx)) = self.picked {
                                let entries = match list {
                                    List::Search => &mut self.results,
                                    List::Local => &mut self.local,
                                };
                                if let Some(en) = entries.get_mut(idx) {
                                    en.failed = true;
                                }
                            }
                        }
                    }
                }
                Msg::Preview { gen, image } if gen == self.preview_gen => {
                    self.preview = Some(load_tex(ctx, "preview", &image));
                }
                Msg::GalleryItem { gen, idx, image } if gen == self.gallery_gen => {
                    if let Some(slot) = self.gallery.get_mut(idx) {
                        *slot = Some(load_tex(ctx, &format!("g-{idx}"), &image));
                    }
                }
                Msg::ApplyFailed(p, e) => {
                    self.queue_mark(&p, QState::Failed(e.clone()));
                    self.last_error = Some(format!("“{}”: {e}", short(&p)));
                }
                Msg::Applied(p) => {
                    self.queue_mark(&p, QState::Done);
                    self.status = format!("✔ Icon set on “{}”", short(&p));
                    self.last_error = None;
                }
                Msg::Removed(p) => self.status = format!("Icon removed from “{}”", short(&p)),
                Msg::Error(e) => self.last_error = Some(e),
                Msg::BatchProgress { done, total, current } => self.batch = Some((done, total, current)),
                Msg::BatchDone { ok, failed } => {
                    self.batch = None;
                    self.status = format!("Batch finished: {ok} folders done, {} failed.", failed.len());
                    if !failed.is_empty() {
                        self.last_error = Some(failed.join("\n"));
                    }
                }
                _ => {}
            }
        }

        if let Some((gen, r)) = self.google.poll(frame) {
            if gen == self.gen {
                match r {
                    Ok(h) => self.on_hits(SourceKind::Google, h),
                    Err(e) => self.on_failed(SourceKind::Google, e),
                }
            }
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<egui::DroppedFile> = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = dropped.into_iter().filter_map(|f| f.path).collect();
        let mut dirs: Vec<PathBuf> = paths.iter().filter(|p| p.is_dir()).cloned().collect();
        dirs.sort();
        let films: Vec<PathBuf> = paths
            .iter()
            .filter(|p| p.is_file() && videos::is_video(p))
            .cloned()
            .collect();

        if !films.is_empty() {
            self.open_videos(films);
            if !dirs.is_empty() {
                let open = self.folder.is_none();
                self.add_to_queue(dirs, open);
            }
            return;
        }
        if dirs.len() > 1 || (!dirs.is_empty() && !self.queue.is_empty()) {
            self.add_to_queue(dirs, true);
            return;
        }
        if let Some(d) = dirs.into_iter().next() {
            self.queue_pos = None;
            self.open_folder(d);
            return;
        }
        if let Some(p) = paths.into_iter().find(|p| local::is_image(p)) {
            self.use_file(p);
            if self.folder.is_none() {
                self.status = "Picture loaded. Now drag the folder it is for onto the window.".into();
            }
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input()
            || self.show_settings
            || self.show_update
            || self.multi.is_some()
            || self.rename.is_some()
            || self.videos.is_some()
        {
            return;
        }
        let (enter, skip) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.modifiers.command && i.key_pressed(egui::Key::ArrowRight),
            )
        });
        if enter && self.folder.is_some() && self.poster.is_some() {
            self.apply();
        }
        if skip && self.queue_pos.is_some() {
            self.queue_skip();
        }
    }

    fn title_bar(&mut self, ctx: &egui::Context) {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        egui::TopBottomPanel::top("title_bar")
            .exact_height(TITLE_H)
            .show_separator_line(false)
            .frame(egui::Frame::none().fill(TITLEBAR).inner_margin(Margin {
                left: 12.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            }))
            .show(ctx, |ui| {
                let bar = ui.max_rect();
                let drag = ui.interact(bar, egui::Id::new("title_drag"), Sense::click_and_drag());
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                } else if drag.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                    egui::Image::from_texture(SizedTexture::from_handle(&self.logo)).paint_at(ui, r);
                    ui.label(RichText::new("Coverly").size(13.5).family(bold()).color(TEXT));
                    tag(ui, &format!("v{VERSION}"), MUTED, CHIP);
                    let pill = |text: String, kind: Kind| {
                        Btn::new(text, kind)
                            .icon(Icon::Download)
                            .height(22.0)
                            .font(12.0)
                            .rounding(11.0)
                    };
                    let badge = match self.updater.state() {
                        UpdateState::Available(r) => Some(
                            pill(format!("v{} available", r.version), Kind::Chip(false))
                                .icon_color(ACCENT)
                                .color(ACCENT),
                        ),
                        UpdateState::Downloading { release, done } => Some(pill(
                            format!("Updating {}%", percent(done, release.size)),
                            Kind::Chip(false),
                        )),
                        UpdateState::Ready { .. } => {
                            Some(pill("Restart to update".into(), Kind::Primary).icon(Icon::Refresh))
                        }
                        _ => None,
                    };
                    if let Some(b) = badge {
                        if ui.add(b).on_hover_text("Coverly update").clicked() {
                            self.show_update = true;
                        }
                    }
                    ui.add_space(12.0);

                    let room = (ui.available_width() - 330.0).clamp(120.0, 520.0);
                    let (text, col) = match &self.folder {
                        Some(f) => (short(f), TEXT),
                        None => ("Choose a folder…".to_string(), MUTED),
                    };
                    let tip = self
                        .folder
                        .as_ref()
                        .map(|f| format!("{}\nClick to choose another folder", f.display()))
                        .unwrap_or_else(|| "Click to choose a folder, or drag one onto the window".into());
                    let folder_btn = Btn::new(text, Kind::Quiet)
                        .icon(Icon::Folder)
                        .icon_color(ACCENT)
                        .color(col)
                        .font(12.5)
                        .max_width(room);
                    if ui.add(folder_btn).on_hover_text(tip).clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.queue_pos = None;
                            self.open_folder(p);
                        }
                    }
                    if self.folder.is_some()
                        && ui
                            .add(quiet_button("Rename").icon(Icon::Pencil).font(12.5))
                            .on_hover_text("Rename the folder to its clean title and year, e.g. “Interstellar (2014)”")
                            .clicked()
                    {
                        self.open_rename();
                    }

                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let win = |kind: Kind, icon: Icon| {
                            Btn::new("", kind).icon(icon).font(10.0).height(TITLE_H).width(46.0)
                        };
                        if ui.add(win(Kind::WindowClose, Icon::Close)).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let size_icon = if maximized { Icon::Restore } else { Icon::Maximize };
                        if ui.add(win(Kind::Window, size_icon)).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if ui.add(win(Kind::Window, Icon::Minimize)).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        ui.add_space(8.0);
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if ui
                            .add(icon_button(Icon::Gear, 28.0))
                            .on_hover_text("Settings")
                            .clicked()
                        {
                            self.draft = self.cfg.clone();
                            self.show_settings = true;
                        }

                        let on = self.cfg.proxy_enabled;
                        let pill = Btn::new("Proxy", if on { Kind::Chip(false) } else { Kind::Quiet })
                            .icon(Icon::Dot)
                            .icon_color(if on { OK } else { Color32::from_gray(90) })
                            .height(24.0)
                            .font(12.0)
                            .rounding(12.0);
                        let tip = match (on, self.cfg.proxy_address()) {
                            (true, Some(p)) => {
                                format!(
                                    "Proxy on: {p}\nUsed for: {}\nClick to switch off",
                                    proxy_list(&self.cfg)
                                )
                            }
                            (_, Some(p)) => format!("Proxy off ({p})\nClick to switch on"),
                            _ => "No proxy address yet: set one in Settings".into(),
                        };
                        if ui.add(pill).on_hover_text(tip).clicked() {
                            self.cfg.proxy_enabled = !on;
                            self.draft.proxy_enabled = !on;
                            let _ = self.cfg.save();
                        }
                    });
                });
            });
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        let mut go = false;
        let mut new_mode = None;
        let mut site = None;
        let mut paste = false;
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(14.0, 9.0))
                    .stroke(Stroke::new(1.0_f32, LINE)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    let avail = ui.available_width();
                    let title_w = (avail * 0.22).clamp(160.0, 300.0);
                    let extra_w = (avail * 0.1).clamp(90.0, 150.0);
                    let r1 = ui.add(text_field(&mut self.title_q, "Movie or series title", title_w));
                    let r2 = ui.add(text_field(&mut self.year_q, "Year", 52.0));
                    let r3 = ui.add(text_field(&mut self.cfg.query_extra, "Extra words", extra_w));
                    go |= (r1.lost_focus() || r2.lost_focus() || r3.lost_focus())
                        && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    let current = SITES
                        .iter()
                        .find(|(_, s)| *s == self.cfg.query_site)
                        .map(|(l, _)| l.to_string())
                        .unwrap_or_else(|| self.cfg.query_site.clone());
                    let btn = ghost_button(&current).icon(Icon::Globe).font(12.5).max_width(190.0);
                    let chosen = self.cfg.query_site.clone();
                    let (resp, _) = dropdown(ui, "site", btn, 240.0, |ui| {
                        for (label, s) in SITES {
                            if menu_item(ui, label, chosen == s) {
                                site = Some(s.to_string());
                            }
                        }
                    });
                    let _ = resp.on_hover_text("Limit Google and Bing to one website");

                    if ui.add(primary_button("Search", 28.0).icon(Icon::Search)).clicked() {
                        go = true;
                    }
                    paste = ui
                        .add(ghost_button("Paste").icon(Icon::Paste))
                        .on_hover_text(
                            "Use a picture copied anywhere (right-click → Copy image), an image link, \
                             or a file / folder path. Ctrl+V works too.",
                        )
                        .clicked();

                    ui.add_space(8.0);
                    let mut m = self.mode;
                    let items: Vec<(Mode, String)> = Mode::ALL.iter().map(|m| (*m, m.label().to_string())).collect();
                    if segmented(ui, "mode", &mut m, &items, 28.0) {
                        new_mode = Some(m);
                    }
                });
            });

        if let Some(s) = site {
            self.cfg.query_site = s;
            go = true;
        }
        if paste {
            self.paste_from_clipboard();
        }
        if let Some(m) = new_mode {
            self.set_mode(m);
        } else if go {
            self.search();
        }

        if self.elevated {
            egui::TopBottomPanel::top("admin")
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(70, 30, 30))
                        .inner_margin(Margin::symmetric(14.0, 7.0)),
                )
                .show(ctx, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Coverly is running as administrator, so Windows blocks drag & drop. \
                                 Close it and open it normally (not “Run as administrator”). Ctrl+V of a folder path still works.",
                            )
                            .size(12.5)
                            .color(TEXT),
                        )
                        .wrap(),
                    );
                });
        }
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(
                egui::Frame::none()
                    .fill(TITLEBAR)
                    .inner_margin(Margin::symmetric(14.0, 0.0))
                    .stroke(Stroke::new(1.0_f32, LINE)),
            )
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui.available_width() > 640.0 {
                        ui.label(
                            RichText::new("Drop folders, video files or a picture · Ctrl+V pastes")
                                .size(11.5)
                                .color(Color32::from_gray(110)),
                        );
                    }
                    ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                        if let Some((d, t, c)) = &self.batch {
                            ui.add(
                                egui::ProgressBar::new(*d as f32 / (*t).max(1) as f32)
                                    .desired_width(140.0)
                                    .desired_height(10.0),
                            );
                            ui.label(RichText::new(format!("{d}/{t}")).size(11.5).color(TEXT));
                            ui.add(egui::Label::new(RichText::new(c).size(11.5).color(MUTED)).truncate());
                        } else if let UpdateState::Downloading { release, done } = self.updater.state() {
                            ui.add(
                                egui::ProgressBar::new(percent(done, release.size) as f32 / 100.0)
                                    .desired_width(140.0)
                                    .desired_height(10.0),
                            );
                            ui.label(
                                RichText::new(format!("Downloading Coverly {}", release.version))
                                    .size(11.5)
                                    .color(TEXT),
                            );
                            ui.label(
                                RichText::new(format!("{} / {}", mb(done), mb(release.size)))
                                    .size(11.5)
                                    .color(MUTED),
                            );
                        } else if let Some(e) = &self.last_error {
                            let _ = ui
                                .add(
                                    egui::Label::new(
                                        RichText::new(e.lines().next().unwrap_or("")).size(11.5).color(DANGER),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(e);
                        } else {
                            ui.add(egui::Label::new(RichText::new(&self.status).size(11.5).color(MUTED)).truncate());
                        }
                    });
                });
            });
    }

    fn window_frame(&self, ctx: &egui::Context) {
        let (maximized, fullscreen) = ctx.input(|i| {
            let v = i.viewport();
            (v.maximized.unwrap_or(false), v.fullscreen.unwrap_or(false))
        });
        if maximized || fullscreen {
            return;
        }
        let screen = ctx.screen_rect();
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("window_border"),
        ))
        .rect_stroke(
            screen.shrink(0.5),
            0.0,
            Stroke::new(1.0_f32, Color32::from_rgb(58, 58, 66)),
        );

        let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        let m = 5.0;
        let (l, r) = (p.x - screen.left() < m, screen.right() - p.x < m);
        let (t, b) = (p.y - screen.top() < m, screen.bottom() - p.y < m);
        use egui::{viewport::ResizeDirection as D, CursorIcon as C};
        let (dir, cursor) = match (l, r, t, b) {
            (true, _, true, _) => (D::NorthWest, C::ResizeNwSe),
            (_, true, true, _) => (D::NorthEast, C::ResizeNeSw),
            (true, _, _, true) => (D::SouthWest, C::ResizeNeSw),
            (_, true, _, true) => (D::SouthEast, C::ResizeNwSe),
            (true, _, _, _) => (D::West, C::ResizeHorizontal),
            (_, true, _, _) => (D::East, C::ResizeHorizontal),
            (_, _, true, _) => (D::North, C::ResizeVertical),
            (_, _, _, true) => (D::South, C::ResizeVertical),
            _ => return,
        };
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }

    fn queue_panel(&mut self, ctx: &egui::Context) {
        if self.queue.is_empty() {
            return;
        }
        let total = self.queue.len();
        let done = self.queue.iter().filter(|q| q.state == QState::Done).count();
        let waiting = self.queue.iter().filter(|q| q.state == QState::Pending).count();
        let mut open_idx = None;
        let mut clear = false;
        let mut clear_done = false;
        let mut auto_all = false;
        egui::SidePanel::left("queue")
            .resizable(true)
            .default_width(240.0)
            .width_range(190.0..=380.0)
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(Margin::same(12.0))
                    .stroke(Stroke::new(1.0_f32, LINE)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    section_title(ui, "QUEUE");
                    ui.label(RichText::new(format!("{done} / {total}")).size(12.0).color(TEXT));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let busy = self.batch.is_some();
                        dropdown(ui, "queue_menu", icon_button(Icon::Dots, 24.0), 230.0, |ui| {
                            if waiting > 0
                                && !busy
                                && menu_action(ui, Icon::Layers, &format!("Set all {waiting} automatically"))
                            {
                                auto_all = true;
                            }
                            if menu_action(ui, Icon::Check, "Remove finished folders") {
                                clear_done = true;
                            }
                            if menu_action(ui, Icon::Trash, "Clear the queue") {
                                clear = true;
                            }
                        });
                    });
                });
                ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).desired_height(3.0));
                ui.add_space(4.0);
                let _ = toggle(ui, &mut self.cfg.auto_next, "Next folder after setting");
                ui.label(
                    RichText::new("Enter = set icon · Ctrl+→ = skip")
                        .size(11.0)
                        .color(Color32::from_gray(110)),
                );
                ui.add_space(4.0);

                let row_h = 28.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, row_h, total, |ui, range| {
                        for i in range {
                            let q = &self.queue[i];
                            let current = self.queue_pos == Some(i);
                            let (icon, col) = match &q.state {
                                QState::Pending => (Icon::Circle, MUTED),
                                QState::Done => (Icon::Check, OK),
                                QState::Skipped => (Icon::Skip, Color32::from_gray(110)),
                                QState::Failed(_) => (Icon::Alert, DANGER),
                            };
                            let (rect, resp) =
                                ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h - 2.0), Sense::click());
                            let fill = if current {
                                ACCENT_DIM
                            } else if resp.hovered() {
                                CHIP
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(rect, 6.0, fill);
                            let ir = egui::Rect::from_center_size(
                                rect.left_center() + egui::vec2(14.0, 0.0),
                                egui::Vec2::splat(12.0),
                            );
                            paint_icon(ui.painter(), ir, icon, col);
                            let g = one_line(
                                ui,
                                &short(&q.path),
                                FontId::proportional(12.5),
                                if current { ACCENT } else { TEXT },
                                rect.width() - 36.0,
                            );
                            let y = rect.center().y - g.size().y / 2.0;
                            ui.painter().galley(egui::pos2(rect.left() + 28.0, y), g, TEXT);
                            let tip = match &q.state {
                                QState::Failed(e) => format!("{}\n{e}", q.path.display()),
                                _ => q.path.display().to_string(),
                            };
                            if resp
                                .on_hover_text(tip)
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                open_idx = Some(i);
                            }
                        }
                    });
            });
        if let Some(i) = open_idx {
            self.open_queue_item(i);
        }
        if auto_all {
            self.queue_auto();
        }
        if clear_done {
            let current = self.queue_pos.and_then(|i| self.queue.get(i)).map(|q| q.path.clone());
            self.queue.retain(|q| q.state != QState::Done);
            self.queue_pos = current.and_then(|p| self.queue.iter().position(|q| q.path == p));
        }
        if clear {
            self.queue.clear();
            self.queue_pos = None;
        }
    }

    fn rename_window(&mut self, ctx: &egui::Context) {
        if self.rename.is_none() {
            return;
        }
        let current = self.folder.as_deref().map(short).unwrap_or_default();
        let mut go = None;
        let mut open = true;
        let mut cancel = false;
        egui::Window::new(RichText::new("Rename folder").family(bold()).size(18.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let Some(r) = self.rename.as_mut() else { return };
                ui.label(RichText::new("Now").size(12.0).color(MUTED));
                ui.add(egui::Label::new(RichText::new(&current).color(TEXT)).truncate());
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Name from {}", r.from)).size(12.0).color(MUTED));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    for f in NameFmt::ALL {
                        if ui.add(chip(f.label(), r.fmt == f)).clicked() {
                            r.fmt = f;
                            r.name = clean_folder_name(&f.apply(&r.title, r.year));
                        }
                    }
                });
                ui.add_space(6.0);
                let w = ui.available_width();
                let resp = ui.add(text_field(&mut r.name, "New folder name", w));
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = Some(r.name.clone());
                }
                let cleaned = clean_folder_name(&r.name);
                if cleaned != r.name.trim() {
                    ui.label(
                        RichText::new(format!("Will be saved as: {cleaned}"))
                            .size(12.0)
                            .color(MUTED),
                    );
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(primary_button("Rename", 34.0)).clicked() {
                        go = Some(r.name.clone());
                    }
                    if ui.add(ghost_button("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if let Some(name) = go {
            self.do_rename(&name);
        } else if cancel || !open {
            self.rename = None;
        }
    }

    fn side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("side")
            .resizable(true)
            .default_width(360.0)
            .width_range(300.0..=520.0)
            .frame(egui::Frame::none().fill(PANEL).stroke(Stroke::new(1.0_f32, LINE)))
            .show(ctx, |ui| {
                egui::TopBottomPanel::bottom("side_actions")
                    .frame(egui::Frame::none().fill(PANEL).inner_margin(Margin {
                        left: 14.0,
                        right: 14.0,
                        top: 10.0,
                        bottom: 12.0,
                    }))
                    .show_inside(ui, |ui| self.side_actions(ui));

                egui::CentralPanel::default()
                    .frame(egui::Frame::none().fill(PANEL))
                    .show_inside(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("side_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Frame::none()
                                    .inner_margin(Margin {
                                        left: 14.0,
                                        right: 14.0,
                                        top: 12.0,
                                        bottom: 12.0,
                                    })
                                    .show(ui, |ui| self.side_content(ui));
                            });
                    });
            });
    }

    fn side_content(&mut self, ui: &mut egui::Ui) {
        section_title(ui, "PREVIEW");
        let w = ui.available_width();
        let box_h = (w * 0.68).clamp(180.0, 260.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, box_h), Sense::hover());
        ui.painter().rect_filled(rect, 12.0, FIELD);
        checkerboard(ui, rect.shrink(10.0));
        let img = (box_h - 20.0).min(256.0);
        if let Some(t) = &self.preview {
            let r = egui::Rect::from_center_size(rect.center(), egui::vec2(img, img));
            egui::Image::from_texture(SizedTexture::from_handle(t)).paint_at(ui, r);
        } else if !self.loading_full {
            placeholder(ui, rect, "Pick a picture on the left");
        }
        if self.loading_full {
            egui::Spinner::new()
                .size(26.0)
                .paint_at(ui, egui::Rect::from_center_size(rect.center(), egui::vec2(26.0, 26.0)));
        }
        if let Some(t) = self.templates.get(self.tpl_idx) {
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(&t.name).size(13.0).family(bold()).color(TEXT)).truncate());
        }

        if self.poster.is_some() {
            ui.add_space(4.0);
            let header = if self.adjust.is_default() {
                "Adjust picture"
            } else {
                "Adjust picture  •"
            };
            egui::CollapsingHeader::new(RichText::new(header).size(12.5).color(TEXT))
                .id_salt("adjust")
                .default_open(false)
                .show(ui, |ui| {
                    let mut moved = false;
                    let mut released = false;
                    {
                        let a = &mut self.adjust;
                        let rows: [(&str, &mut f32, std::ops::RangeInclusive<f32>); 3] = [
                            ("Zoom", &mut a.zoom, 1.0..=3.0),
                            ("Left / right", &mut a.x, -1.0..=1.0),
                            ("Up / down", &mut a.y, -1.0..=1.0),
                        ];
                        for (label, v, range) in rows {
                            let r = slider_row(ui, label, v, range);
                            moved |= r.changed();
                            released |= r.drag_stopped() || (r.changed() && !r.dragged());
                        }
                    }
                    if !self.adjust.is_default()
                        && ui
                            .add(quiet_button("Reset").icon(Icon::Refresh).height(24.0).font(12.0))
                            .clicked()
                    {
                        self.adjust = Adjust::default();
                        moved = true;
                        released = true;
                    }
                    if moved {
                        self.request_preview();
                    }
                    if released {
                        self.request_gallery();
                    }
                });
        }

        ui.add_space(10.0);
        section_title(ui, "ON THE ICON");
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            changed |= toggle(ui, &mut self.cfg.show_rating, "Rating")
                .on_hover_text("From the folder's .nfo file, or TMDB when a key is set")
                .changed();
            changed |= toggle(ui, &mut self.cfg.show_name, "Label")
                .on_hover_text("Text on the icon: the title, S2, anything")
                .changed();
        });
        if self.cfg.show_name {
            ui.add_space(2.0);
            let w = ui.available_width();
            changed |= ui.add(text_field(&mut self.label_text, "Label text", w)).changed();
            changed |= label_style_picker(ui, "side_style", &mut self.cfg.label_style);
        }
        if changed {
            let _ = self.cfg.save();
            self.overlay_changed();
        }

        ui.add_space(12.0);
        let mut family = None;
        ui.horizontal(|ui| {
            section_title(ui, "TEMPLATES");
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let label = if self.family == "All" {
                    "All styles".to_string()
                } else {
                    self.family.clone()
                };
                let w = (ui.available_width() - 8.0).clamp(110.0, 190.0);
                let current = self.family.clone();
                let families = &self.families;
                dropdown(
                    ui,
                    "family",
                    ghost_button(&label).height(24.0).font(12.0).width(w).left(),
                    w,
                    |ui| {
                        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                            if menu_item(ui, "All styles", current == "All") {
                                family = Some("All".to_string());
                            }
                            for f in families {
                                if menu_item(ui, f, current == *f) {
                                    family = Some(f.clone());
                                }
                            }
                        });
                    },
                );
            });
        });
        if let Some(f) = family {
            self.family = f;
        }
        ui.add_space(4.0);
        let mut chosen = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(7.0, 7.0);
            for (i, t) in self.templates.iter().enumerate() {
                if self.family != "All" && t.family != self.family {
                    continue;
                }
                let tex = self.gallery.get(i).and_then(|g| g.as_ref());
                if gallery_tile(ui, tex, &t.name, i == self.tpl_idx).clicked() {
                    chosen = Some(i);
                }
            }
        });
        if let Some(i) = chosen {
            self.tpl_idx = i;
            if !self.mode.wants_original() {
                self.poster_tpl_idx = i;
            }
            self.cfg.last_template = self.templates[i].id.clone();
            let _ = self.cfg.save();
            self.request_preview();
        }
    }

    fn side_actions(&mut self, ui: &mut egui::Ui) {
        let can = self.folder.is_some() && self.poster.is_some();
        let w = ui.available_width();
        let in_queue = self.queue_pos.is_some();
        let tip = if self.folder.is_none() {
            "Drag a folder onto the window first"
        } else if self.poster.is_none() {
            "Pick a picture first"
        } else {
            "Make this the folder's icon (Enter)"
        };
        let main = if in_queue && self.cfg.auto_next {
            primary_button("Set icon & next", 38.0).trail(Icon::ChevronRight)
        } else {
            primary_button("Set as folder icon", 38.0)
        };
        if ui
            .add_enabled(can, main.width(w))
            .on_hover_text(tip)
            .on_disabled_hover_text(tip)
            .clicked()
        {
            self.apply();
        }
        if in_queue {
            ui.add_space(2.0);
            if ui
                .add(ghost_button("Skip this folder").icon(Icon::Skip).width(w))
                .on_hover_text("Ctrl+→")
                .clicked()
            {
                self.queue_skip();
            }
        }
        ui.add_space(2.0);
        let has = self.folder.is_some();
        let half = ((w - ui.spacing().item_spacing.x) / 2.0).floor();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(has, ghost_button("Remove icon").icon(Icon::Trash).width(half))
                .clicked()
            {
                if let Some(f) = self.folder.clone() {
                    self.send(Job::Remove { folder: f });
                }
            }
            if ui
                .add_enabled(
                    has && self.batch.is_none(),
                    ghost_button("Several folders…").icon(Icon::Layers).width(half),
                )
                .on_hover_text("Season folders inside this one, or the folders next to it — each with its own label")
                .clicked()
            {
                self.open_multi();
            }
        });
    }

    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(Margin::same(14.0)))
            .show(ctx, |ui| {
                let mut new_match = None;
                let mut refresh = false;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let n_res = self.results.iter().filter(|e| !e.removed).count();
                    let mut tab = self.tab;
                    let items = [
                        (List::Search, format!("Results  {n_res}")),
                        (List::Local, format!("In this folder  {}", self.local.len())),
                    ];
                    if segmented(ui, "tabs", &mut tab, &items, 28.0) {
                        self.tab = tab;
                    }
                    if !self.pending.is_empty() || self.google.busy() {
                        ui.add_space(2.0);
                        ui.spinner();
                    }
                    if self.tab == List::Search && self.cur_key.is_some() {
                        if let Some(t) = self.saved_at {
                            let _ = tag(ui, &format!("Saved · {}", search_cache::age(t)), MUTED, CHIP).on_hover_text(
                                "Found by an earlier search, so the sites weren't asked again. \
                                 The arrow searches again.",
                            );
                        }
                        if ui
                            .add(icon_button(Icon::Refresh, 26.0))
                            .on_hover_text("Search the sites again")
                            .clicked()
                        {
                            refresh = true;
                        }
                    }
                    if self.tab == List::Search && self.matches.len() > 1 && ui.available_width() > 230.0 {
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let w = (ui.available_width() - 50.0).clamp(150.0, 320.0);
                            let current = self.match_idx;
                            let matches = &self.matches;
                            let label = matches[current].display();
                            dropdown(ui, "match", ghost_button(&label).font(12.5).width(w).left(), w, |ui| {
                                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                    for (i, m) in matches.iter().enumerate().take(15) {
                                        if menu_item(ui, &m.display(), i == current) {
                                            new_match = Some(i);
                                        }
                                    }
                                });
                            });
                            ui.label(RichText::new("Title").size(12.5).color(MUTED));
                        });
                    }
                });
                if let Some(i) = new_match {
                    self.select_match(i);
                }
                if refresh {
                    self.search_with(true);
                }

                if self.tab == List::Search {
                    ui.add_space(6.0);
                    self.filter_bar(ui);
                    if !self.errors.is_empty() {
                        ui.add_space(2.0);
                        for (s, e) in &self.errors {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("{}: {e}", s.label())).size(11.5).color(DANGER),
                                )
                                .truncate(),
                            );
                        }
                    }
                }
                ui.add_space(8.0);

                if self.tab == List::Search && self.google.needs_user() {
                    let mut cancel = false;
                    egui::Frame::none()
                        .fill(Color32::from_rgb(52, 42, 18))
                        .stroke(Stroke::new(1.0_f32, ACCENT))
                        .rounding(10.0)
                        .inner_margin(Margin::symmetric(14.0, 10.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    cancel = ui.add(ghost_button("Skip Google")).clicked();
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(
                                                "Google wants to check that you're human. Solve it below once: \
                                                 the results then load by themselves.",
                                            )
                                            .size(12.5)
                                            .color(TEXT),
                                        )
                                        .wrap(),
                                    );
                                });
                            });
                        });
                    ui.add_space(8.0);
                    self.google_rect = Some(ui.available_rect_before_wrap().intersect(ui.clip_rect()));
                    if cancel {
                        if let Some(g) = self.google.cancel() {
                            if g == self.gen {
                                self.on_failed(SourceKind::Google, "skipped (Google asked for a check)".into());
                            }
                        }
                    }
                    return;
                }

                let visible: Vec<usize> = match self.tab {
                    List::Search => (0..self.results.len())
                        .filter(|&i| self.passes(&self.results[i]))
                        .collect(),
                    List::Local => (0..self.local.len()).filter(|&i| !self.local[i].failed).collect(),
                };

                if visible.is_empty() {
                    let (big, small) = if self.folder.is_none() && self.results.is_empty() {
                        (
                            "Drag movie or series folders here",
                            "Or video files: a folder is made for each. The name is read and the search starts by itself.",
                        )
                    } else if !self.pending.is_empty() || self.google.busy() {
                        ("Searching…", "")
                    } else if self.tab == List::Local {
                        ("No pictures in this folder", "Ctrl+V pastes a copied image.")
                    } else {
                        (
                            "Nothing to show",
                            "Try other words, another site, or loosen the filters.",
                        )
                    };
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() * 0.3);
                        let _ = icon_label(ui, Icon::Folder, 46.0, Color32::from_gray(90));
                        ui.add_space(10.0);
                        ui.label(RichText::new(big).size(20.0).family(bold()).color(TEXT));
                        ui.label(RichText::new(small).size(13.5).color(MUTED));
                    });
                    return;
                }

                let listed = visible.len();
                let limit = if self.tab == List::Search {
                    self.shown_limit
                } else {
                    usize::MAX
                };
                let can_more = self.tab == List::Search && (listed > limit || self.can_load_more());
                let loading_more = !self.loading_more.is_empty() || self.google.busy();
                let mut clicked = None;
                let mut need: Vec<usize> = Vec::new();
                let mut more_clicked = false;
                let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(14.0, 14.0);
                        let entries = match self.tab {
                            List::Search => &self.results,
                            List::Local => &self.local,
                        };
                        for &i in visible.iter().take(limit) {
                            let e = &entries[i];
                            let selected = self.picked == Some((self.tab, i));
                            let (resp, on_screen) = tile(ui, e, selected);
                            if on_screen && e.thumb.is_none() && !e.requested && !e.failed {
                                need.push(i);
                            }
                            if resp.clicked() {
                                clicked = Some(i);
                            }
                        }
                    });
                    ui.add_space(14.0);
                    if can_more {
                        ui.vertical_centered(|ui| {
                            if loading_more {
                                ui.spinner();
                            } else if ui.add(ghost_button("Show more").icon(Icon::ChevronDown)).clicked() {
                                more_clicked = true;
                            }
                        });
                        ui.add_space(14.0);
                    }
                });
                let near_end = out.state.offset.y + out.inner_rect.height() >= out.content_size.y - 320.0;
                let tab = self.tab;
                for i in need.into_iter().take(THUMBS_PER_FRAME) {
                    self.send_thumb(tab, i);
                }
                if tab == List::Search && (more_clicked || (near_end && can_more && !loading_more && listed > 0)) {
                    self.load_more(listed);
                }
                if let Some(i) = clicked {
                    self.pick(self.tab, i);
                }
            });
    }

    fn filter_bar(&mut self, ui: &mut egui::Ui) {
        let mut research = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
            let active = self.cfg.active_sources();
            for s in SourceKind::ONLINE {
                let count = self.results.iter().filter(|e| e.hit.source == s && !e.removed).count();
                let failed = self.errors.iter().find(|(k, _)| *k == s).map(|(_, e)| e.clone());
                if !active.contains(&s) && count == 0 && failed.is_none() {
                    continue;
                }
                let shown = !self.hidden.contains(&s);
                let busy = self.pending.contains(&s)
                    || self.loading_more.contains(&s)
                    || (s == SourceKind::Google && self.google.busy());
                let [r, g, b] = s.color();
                let dot = if failed.is_some() {
                    DANGER
                } else {
                    Color32::from_rgb(r, g, b)
                };
                let tail = if busy {
                    "…".to_string()
                } else if failed.is_some() {
                    "!".to_string()
                } else {
                    count.to_string()
                };
                let tip = match &failed {
                    Some(e) => format!("{} failed: {e}\nClick to show or hide", s.label()),
                    None => format!("{} · click to show or hide", s.label()),
                };
                if source_toggle(ui, s.label(), dot, &tail, shown)
                    .on_hover_text(tip)
                    .clicked()
                {
                    if shown {
                        self.hidden.insert(s);
                    } else {
                        self.hidden.remove(&s);
                    }
                }
            }

            ui.add_space(4.0);
            let (div, _) = ui.allocate_exact_size(egui::vec2(1.0, 16.0), Sense::hover());
            ui.painter().rect_filled(div, 0.0, LINE);
            ui.add_space(4.0);

            let shape_name = match self.shape {
                Shape::Any => "Shape",
                Shape::Portrait => "Tall",
                Shape::Landscape => "Wide",
                Shape::Square => "Square",
            };
            let current = self.shape;
            let mut shape = None;
            filter_dropdown(ui, "f_shape", shape_name, current != Shape::Any, |ui| {
                for (v, n) in [
                    (Shape::Any, "Any shape"),
                    (Shape::Portrait, "Tall (posters)"),
                    (Shape::Landscape, "Wide (backdrops)"),
                    (Shape::Square, "Square (icons)"),
                ] {
                    if menu_item(ui, n, current == v) {
                        shape = Some(v);
                    }
                }
            });
            if let Some(v) = shape {
                if v != self.shape {
                    self.shape = v;
                    research = true;
                }
            }

            let size_name = if self.min_h == 0 {
                "Size".to_string()
            } else {
                format!("{}px +", self.min_h)
            };
            let min_h = self.min_h;
            let mut pick_h = None;
            let hide_unknown = &mut self.hide_unknown;
            filter_dropdown(ui, "f_size", &size_name, min_h > 0 || *hide_unknown, |ui| {
                for h in [0u32, 500, 1000, 2000] {
                    let label = if h == 0 {
                        "Any size".to_string()
                    } else {
                        format!("At least {h}px tall")
                    };
                    if menu_item(ui, &label, min_h == h) {
                        pick_h = Some(h);
                    }
                }
                ui.separator();
                let _ = toggle(ui, hide_unknown, "Hide unknown sizes");
            });
            if let Some(h) = pick_h {
                self.min_h = h;
            }

            if self.cfg.has_key(SourceKind::Tmdb) || self.cfg.has_key(SourceKind::Fanart) {
                let lang = self.cfg.language.to_uppercase();
                let text_name = match self.text_filter {
                    TextFilter::All => "Text".to_string(),
                    TextFilter::Mine => format!("{lang} only"),
                    TextFilter::NoText => "No text".to_string(),
                };
                let current = self.text_filter;
                let mut pick = None;
                filter_dropdown(ui, "f_text", &text_name, current != TextFilter::All, |ui| {
                    for (v, n) in [
                        (TextFilter::All, "Any text".to_string()),
                        (TextFilter::Mine, format!("Only {lang} posters")),
                        (TextFilter::NoText, "No text (TMDB / fanart.tv)".to_string()),
                    ] {
                        if menu_item(ui, &n, current == v) {
                            pick = Some(v);
                        }
                    }
                });
                if let Some(v) = pick {
                    self.text_filter = v;
                }
            }

            let filtered = self.shape != Shape::Any
                || self.min_h > 0
                || self.hide_unknown
                || self.text_filter != TextFilter::All
                || !self.hidden.is_empty();
            if filtered && ui.add(quiet_button("Clear").height(22.0).font(12.0)).clicked() {
                if self.shape != Shape::Any {
                    research = true;
                }
                self.shape = Shape::Any;
                self.min_h = 0;
                self.hide_unknown = false;
                self.text_filter = TextFilter::All;
                self.hidden.clear();
            }
        });
        if research {
            self.search();
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let mut save = false;
        egui::Window::new(RichText::new("Settings").family(bold()).size(18.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(580.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let d = &mut self.draft;

                section_title(ui, "IMAGE SOURCES");
                ui.label(
                    RichText::new("Google, Bing and IMDb need no key.")
                        .size(12.0)
                        .color(MUTED),
                );
                ui.horizontal_wrapped(|ui| {
                    for s in SourceKind::ONLINE {
                        let mut on = d.sources.contains(&s);
                        let label = if s.needs_key() {
                            format!("{} (key)", s.label())
                        } else {
                            s.label().to_string()
                        };
                        if ui.checkbox(&mut on, label).changed() {
                            if on {
                                d.sources.push(s);
                            } else {
                                d.sources.retain(|x| *x != s);
                            }
                        }
                    }
                });
                ui.add_space(8.0);
                egui::Grid::new("keys")
                    .num_columns(2)
                    .spacing([14.0, 10.0])
                    .show(ui, |ui| {
                        ui.label("fanart.tv API key");
                        ui.add(
                            egui::TextEdit::singleline(&mut d.fanart_key)
                                .password(true)
                                .desired_width(330.0)
                                .margin(Margin::symmetric(8.0, 6.0)),
                        );
                        ui.end_row();
                        ui.label("TMDB key (optional)");
                        ui.add(
                            egui::TextEdit::singleline(&mut d.tmdb_key)
                                .password(true)
                                .desired_width(330.0)
                                .margin(Margin::symmetric(8.0, 6.0)),
                        );
                        ui.end_row();
                        ui.label("Poster language");
                        let _ = ui
                            .add(
                                egui::TextEdit::singleline(&mut d.language)
                                    .desired_width(60.0)
                                    .margin(Margin::symmetric(8.0, 6.0)),
                            )
                            .on_hover_text("Two-letter code: en, fa, de, fr …");
                        ui.end_row();
                    });
                ui.horizontal_wrapped(|ui| {
                    ui.hyperlink_to("Get a fanart.tv key", "https://fanart.tv/get-an-api-key/");
                    ui.hyperlink_to("Get a TMDB key", "https://www.themoviedb.org/settings/api");
                });

                ui.add_space(12.0);
                section_title(ui, "PROXY");
                ui.checkbox(&mut d.proxy_enabled, "Proxy on");
                ui.horizontal(|ui| {
                    ui.label("Address");
                    ui.add(
                        egui::TextEdit::singleline(&mut d.proxy_url)
                            .desired_width(300.0)
                            .margin(Margin::symmetric(8.0, 6.0)),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.add(ghost_button("v2rayN SOCKS · 10808")).clicked() {
                        d.proxy_url = "socks5h://127.0.0.1:10808".into();
                    }
                    if ui.add(ghost_button("v2rayN HTTP · 10809")).clicked() {
                        d.proxy_url = "http://127.0.0.1:10809".into();
                    }
                });
                ui.label(RichText::new("Use the proxy for:").color(MUTED));
                ui.horizontal_wrapped(|ui| {
                    for s in SourceKind::ONLINE {
                        let mut on = d.proxy_sources.contains(&s);
                        if ui.checkbox(&mut on, s.label()).changed() {
                            if on {
                                d.proxy_sources.push(s);
                            } else {
                                d.proxy_sources.retain(|x| *x != s);
                            }
                        }
                    }
                });
                ui.label(
                    RichText::new(
                        "Sources not ticked connect directly, and use the proxy only if the direct connection fails.",
                    )
                    .size(12.0)
                    .color(MUTED),
                );

                ui.add_space(12.0);
                section_title(ui, "UPDATES");
                ui.checkbox(&mut d.update_check, "Check for updates when Coverly starts");
                ui.checkbox(&mut d.update_beta, "Include beta versions");
                ui.checkbox(&mut d.update_via_proxy, "Use the proxy for GitHub");
                ui.horizontal(|ui| {
                    if ui.add(ghost_button("Check now").icon(Icon::Refresh)).clicked() {
                        self.updater.check(d, true);
                    }
                    let (text, color) = update_status(&self.updater.state());
                    let resp = ui.add(
                        egui::Label::new(RichText::new(text).size(12.0).color(color))
                            .truncate()
                            .sense(Sense::click()),
                    );
                    if matches!(self.updater.state(), UpdateState::Available(_)) && resp.clicked() {
                        self.show_update = true;
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.add(primary_button("Save", 34.0)).clicked() {
                        save = true;
                    }
                    if ui.add(ghost_button("Open templates folder")).clicked() {
                        let dir = Config::dir().join("templates");
                        let _ = std::fs::create_dir_all(&dir);
                        open_in_explorer(&dir);
                    }
                    if ui.add(ghost_button("Clear image cache")).clicked() {
                        let _ = std::fs::remove_dir_all(cache_dir().join("http"));
                    }
                });
            });

        if save {
            self.cfg = self.draft.clone();
            let _ = self.cfg.save();
            self.show_settings = false;
            self.status = "Settings saved.".into();
        } else if !open {
            self.show_settings = false;
            self.draft = self.cfg.clone();
        }
    }

    fn update_window(&mut self, ctx: &egui::Context) {
        if !self.show_update {
            return;
        }
        let state = self.updater.state();
        let title = match &state {
            UpdateState::Ready { .. } => "Update ready",
            UpdateState::Available(_) | UpdateState::Downloading { .. } => "Update available",
            _ => "Updates",
        };
        let mut open = true;
        let mut action = None;
        egui::Window::new(RichText::new(title).family(bold()).size(18.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let release = match &state {
                    UpdateState::Available(r)
                    | UpdateState::Downloading { release: r, .. }
                    | UpdateState::Ready { release: r, .. } => r,
                    other => {
                        let (text, color) = update_status(other);
                        ui.label(RichText::new(text).color(color));
                        ui.add_space(10.0);
                        if !matches!(other, UpdateState::Checking) && ui.add(ghost_button("Check now")).clicked() {
                            action = Some(UpdateAction::Check);
                        }
                        return;
                    }
                };

                let mut meta = format!("v{VERSION}  →  v{}", release.version);
                if !release.published.is_empty() {
                    meta.push_str(&format!("  ·  released {}", release.published));
                }
                meta.push_str(&format!("  ·  {}", mb(release.size)));
                ui.label(RichText::new(meta).size(12.0).color(MUTED));
                ui.add_space(10.0);

                section_title(ui, "WHAT'S NEW");
                egui::Frame::none()
                    .fill(FIELD)
                    .rounding(6.0)
                    .inner_margin(Margin::symmetric(12.0, 10.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            let notes = update::notes(&release.notes);
                            if notes.is_empty() {
                                ui.label(RichText::new("No notes for this version.").size(12.5).color(MUTED));
                            }
                            for n in notes {
                                match n {
                                    Note::Heading(h) => {
                                        ui.add_space(4.0);
                                        ui.label(RichText::new(h).family(bold()).size(12.5).color(TEXT));
                                    }
                                    Note::Item(t) => {
                                        ui.label(RichText::new(format!("•  {t}")).size(12.5).color(TEXT));
                                    }
                                }
                            }
                        });
                    });
                ui.add_space(4.0);
                ui.hyperlink_to(RichText::new("View on GitHub").size(12.0), &release.page);
                ui.add_space(12.0);

                match &state {
                    UpdateState::Downloading { done, .. } => {
                        ui.add(egui::ProgressBar::new(percent(*done, release.size) as f32 / 100.0).desired_height(8.0));
                        ui.label(
                            RichText::new(format!("{} of {}", mb(*done), mb(release.size)))
                                .size(12.0)
                                .color(MUTED),
                        );
                        ui.add_space(8.0);
                        if ui.add(ghost_button("Cancel")).clicked() {
                            action = Some(UpdateAction::Cancel);
                        }
                    }
                    UpdateState::Ready { .. } => {
                        ui.label(
                            RichText::new("Coverly closes, installs the update and opens again.")
                                .size(12.0)
                                .color(MUTED),
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add(primary_button("Restart now", 34.0).icon(Icon::Refresh))
                                .clicked()
                            {
                                action = Some(UpdateAction::Install);
                            }
                            if ui.add(ghost_button("Later")).clicked() {
                                action = Some(UpdateAction::Later);
                            }
                        });
                    }
                    _ => {
                        ui.horizontal(|ui| {
                            if ui
                                .add(primary_button("Update now", 34.0).icon(Icon::Download))
                                .clicked()
                            {
                                action = Some(UpdateAction::Download(release.clone()));
                            }
                            if ui.add(ghost_button("Later")).clicked() {
                                action = Some(UpdateAction::Later);
                            }
                            if ui.add(quiet_button("Skip this version")).clicked() {
                                action = Some(UpdateAction::Skip(release.version.clone()));
                            }
                        });
                    }
                }
            });

        match action {
            Some(UpdateAction::Download(r)) => self.updater.download(r, &self.cfg),
            Some(UpdateAction::Cancel) => self.updater.cancel(),
            Some(UpdateAction::Install) => {
                if self.updater.install() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Some(UpdateAction::Later) => self.show_update = false,
            Some(UpdateAction::Skip(v)) => {
                self.cfg.update_skip = v.clone();
                self.draft.update_skip = v;
                let _ = self.cfg.save();
                self.updater.dismiss();
                self.show_update = false;
            }
            Some(UpdateAction::Check) => self.updater.check(&self.cfg, true),
            None => {}
        }
        if !open {
            self.show_update = false;
        }
    }

    fn multi_window(&mut self, ctx: &egui::Context) {
        if self.multi.is_none() {
            return;
        }
        let has_picture = self.poster.is_some();
        let tname = self
            .templates
            .get(self.tpl_idx)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let mut start = false;
        let mut close = false;
        let mut rescope = None;
        let mut style_changed = false;
        let mut open = true;
        egui::Window::new(RichText::new("Set icons on several folders").family(bold()).size(18.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(600.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let Some(m) = self.multi.as_mut() else { return };
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Folders").color(MUTED));
                    if ui
                        .add(chip("Inside this folder (seasons…)", m.scope == Scope::Subfolders))
                        .clicked()
                    {
                        rescope = Some(Scope::Subfolders);
                    }
                    if ui
                        .add(chip("Next to this folder", m.scope == Scope::Siblings))
                        .clicked()
                    {
                        rescope = Some(Scope::Siblings);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Picture").color(MUTED));
                    if ui
                        .add_enabled(has_picture, chip("The picture I picked", m.same_picture))
                        .clicked()
                    {
                        m.same_picture = true;
                    }
                    if ui.add(chip("Each folder's own poster", !m.same_picture)).clicked() {
                        m.same_picture = false;
                    }
                });
                ui.label(RichText::new(format!("Template: {tname}")).size(12.0).color(MUTED));
                ui.add_space(4.0);
                if label_style_picker(ui, "multi_style", &mut self.cfg.label_style) {
                    style_changed = true;
                }
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.add(ghost_button("Select all")).clicked() {
                        for r in m.rows.iter_mut() {
                            r.on = true;
                        }
                    }
                    if ui.add(ghost_button("Select none")).clicked() {
                        for r in m.rows.iter_mut() {
                            r.on = false;
                        }
                    }
                    if ui
                        .add(ghost_button("Season labels"))
                        .on_hover_text("S1, S2 … for folders named like seasons")
                        .clicked()
                    {
                        for r in m.rows.iter_mut().filter(|r| !r.is_self) {
                            r.label = auto_label(&r.path);
                        }
                    }
                    if ui.add(ghost_button("Clear labels")).clicked() {
                        for r in m.rows.iter_mut() {
                            r.label.clear();
                        }
                    }
                });
                ui.add_space(6.0);
                if m.rows.is_empty() {
                    ui.label(RichText::new("No folders here.").color(MUTED));
                }
                egui::ScrollArea::vertical()
                    .max_height(340.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("multi_rows")
                            .num_columns(3)
                            .spacing([12.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("");
                                ui.label(RichText::new("Folder").size(12.0).color(MUTED));
                                ui.label(RichText::new("Label on the icon").size(12.0).color(MUTED));
                                ui.end_row();
                                for r in m.rows.iter_mut() {
                                    ui.checkbox(&mut r.on, "");
                                    let mut name = short(&r.path);
                                    if r.is_self {
                                        name.push_str("   (this folder)");
                                    }
                                    ui.label(RichText::new(name).color(if r.on { TEXT } else { MUTED }));
                                    ui.add_enabled(
                                        r.on,
                                        egui::TextEdit::singleline(&mut r.label)
                                            .hint_text("no label")
                                            .desired_width(170.0)
                                            .margin(Margin::symmetric(8.0, 4.0)),
                                    );
                                    ui.end_row();
                                }
                            });
                    });
                let n = m.rows.iter().filter(|r| r.on).count();
                ui.add_space(8.0);
                if !m.same_picture {
                    ui.label(
                        RichText::new(
                            "Each folder uses its own poster.jpg / folder.jpg / cover.jpg, \
                             or the first IMDb, TMDB or Bing poster found for its name.",
                        )
                        .size(12.0)
                        .color(MUTED),
                    );
                    ui.add_space(4.0);
                }
                ui.horizontal_wrapped(|ui| {
                    start = ui
                        .add_enabled(n > 0, primary_button(&format!("Set {n} icons"), 36.0))
                        .clicked();
                    close = ui.add(ghost_button("Cancel")).clicked();
                });
            });

        if style_changed {
            let _ = self.cfg.save();
            self.overlay_changed();
        }
        if let Some(sc) = rescope {
            let rows = self.multi_rows(sc);
            if let Some(m) = self.multi.as_mut() {
                m.scope = sc;
                m.rows = rows;
            }
        }
        if start {
            if let (Some(m), Some(t)) = (self.multi.take(), self.templates.get(self.tpl_idx).cloned()) {
                let items: Vec<(PathBuf, String)> = m
                    .rows
                    .into_iter()
                    .filter(|r| r.on)
                    .map(|r| (r.path, r.label.trim().to_string()))
                    .collect();
                let picture = if m.same_picture { self.poster.clone() } else { None };
                let picture_is_shared = picture.is_some();
                self.batch = Some((0, items.len(), String::new()));
                self.send(Job::Batch {
                    items,
                    template: t,
                    cfg: self.cfg.clone(),
                    picture,
                    rating: self.overlay().rating,
                    style: self.cfg.label_style,
                    adjust: if picture_is_shared {
                        self.adjust
                    } else {
                        Adjust::default()
                    },
                });
            }
        } else if close || !open {
            self.multi = None;
        }
    }

    fn videos_window(&mut self, ctx: &egui::Context) {
        if self.videos.is_none() {
            return;
        }
        let names: Vec<String> = self.templates.iter().map(|t| t.name.clone()).collect();
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        egui::Window::new(RichText::new("Make folders for video files").family(bold()).size(16.0))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(660.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let Some(d) = self.videos.as_mut() else { return };
                ui.horizontal(|ui| {
                    let _ = icon_label(ui, Icon::Film, 15.0, ACCENT);
                    ui.label(
                        RichText::new("Each video gets its own folder, named from the file. Edit any name first.")
                            .size(12.5)
                            .color(MUTED),
                    );
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Folder name").size(12.5).color(MUTED));
                    let items: Vec<(NameFmt, String)> =
                        [NameFmt::Paren, NameFmt::Bracket, NameFmt::Dash, NameFmt::TitleOnly]
                            .iter()
                            .map(|f| (*f, f.label().to_string()))
                            .collect();
                    let mut fmt = d.fmt;
                    if segmented(ui, "video_fmt", &mut fmt, &items, 26.0) {
                        d.fmt = fmt;
                        for r in d.rows.iter_mut() {
                            r.rename(fmt);
                        }
                    }
                });
                ui.add_space(6.0);

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Grid::new("video_rows")
                            .num_columns(3)
                            .spacing([10.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for r in d.rows.iter_mut() {
                                    ui.checkbox(&mut r.on, "");
                                    ui.vertical(|ui| {
                                        ui.set_max_width(270.0);
                                        ui.add(
                                            egui::Label::new(RichText::new(r.file_name()).size(12.0).color(if r.on {
                                                TEXT
                                            } else {
                                                MUTED
                                            }))
                                            .truncate(),
                                        );
                                        let mut extra = Vec::new();
                                        if let Some(s) = r.season_dir() {
                                            extra.push(format!("into {s}"));
                                        }
                                        if !r.extras.is_empty() {
                                            extra.push(format!("+{} files with the same name", r.extras.len()));
                                        }
                                        if !extra.is_empty() {
                                            ui.label(RichText::new(extra.join(" · ")).size(11.0).color(MUTED));
                                        }
                                    });
                                    ui.add_enabled(r.on, text_field(&mut r.folder, "Folder name", 250.0));
                                    ui.end_row();
                                }
                            });
                    });

                ui.add_space(8.0);
                let _ = toggle(
                    ui,
                    &mut d.move_extras,
                    "Move subtitles and other files with the same name",
                );
                if d.rows.iter().any(|r| r.season_dir().is_some()) {
                    let _ = toggle(ui, &mut d.seasons, "Put episodes in “Season 01” folders");
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Then").size(12.5).color(MUTED));
                    let items = [
                        (true, "Set the icons automatically".to_string()),
                        (false, "Let me pick each picture".to_string()),
                    ];
                    segmented(ui, "video_then", &mut d.auto, &items, 26.0);
                });
                if d.auto {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        let _ = toggle(ui, &mut d.banner, "Name banner on the icon");
                        ui.add_space(10.0);
                        let current = d.tpl;
                        let label = names.get(current).cloned().unwrap_or_default();
                        let mut pick = None;
                        dropdown(
                            ui,
                            "video_tpl",
                            ghost_button(&format!("Template: {label}"))
                                .height(26.0)
                                .font(12.5)
                                .max_width(330.0),
                            300.0,
                            |ui| {
                                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                    for (i, n) in names.iter().enumerate() {
                                        if menu_item(ui, n, i == current) {
                                            pick = Some(i);
                                        }
                                    }
                                });
                            },
                        );
                        if let Some(i) = pick {
                            d.tpl = i;
                        }
                    });
                    ui.label(
                        RichText::new(
                            "Each folder uses a poster.jpg / folder.jpg it already has, \
                             otherwise the best IMDb, TMDB or Bing poster for its name.",
                        )
                        .size(11.5)
                        .color(MUTED),
                    );
                }

                ui.add_space(10.0);
                let n = d.rows.iter().filter(|r| r.on).count();
                ui.horizontal(|ui| {
                    go = ui
                        .add_enabled(
                            n > 0,
                            primary_button(&format!("Make {n} folders"), 32.0).icon(Icon::Folder),
                        )
                        .clicked();
                    cancel = ui.add(ghost_button("Cancel").height(32.0)).clicked();
                });
            });
        if go {
            self.create_video_folders();
        } else if cancel || !open {
            self.videos = None;
        }
    }

    fn drop_overlay(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
        let r = ctx.screen_rect();
        painter.rect_filled(r, 0.0, Color32::from_black_alpha(210));
        painter.rect_stroke(r.shrink(24.0), 18.0, Stroke::new(2.0_f32, ACCENT));
        painter.text(
            r.center() - egui::vec2(0.0, 14.0),
            Align2::CENTER_CENTER,
            "Drop here",
            FontId::new(28.0, bold()),
            TEXT,
        );
        painter.text(
            r.center() + egui::vec2(0.0, 22.0),
            Align2::CENTER_CENTER,
            "Folders · video files (a folder is made for each) · a picture",
            FontId::proportional(14.0),
            MUTED,
        );
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.poll(ctx, frame);
        self.handle_drops(ctx);
        self.handle_paste(ctx);
        self.handle_keys(ctx);
        self.title_bar(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
        self.queue_panel(ctx);
        self.side_panel(ctx);
        self.central(ctx);
        self.settings_window(ctx);
        self.update_window(ctx);
        self.multi_window(ctx);
        self.rename_window(ctx);
        self.videos_window(ctx);
        self.drop_overlay(ctx);
        self.window_frame(ctx);
        let area = self.google_rect.take().filter(|_| {
            !self.show_settings
                && !self.show_update
                && self.multi.is_none()
                && self.rename.is_none()
                && self.videos.is_none()
        });
        self.google.place(area);
        if !self.pending.is_empty() || self.loading_full || self.batch.is_some() || self.google.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }
}

fn proxy_list(cfg: &Config) -> String {
    let v: Vec<&str> = SourceKind::ONLINE
        .iter()
        .filter(|s| cfg.proxy_sources.contains(s))
        .map(|s| s.label())
        .collect();
    if v.is_empty() {
        "nothing (direct, proxy only as a fallback)".into()
    } else {
        v.join(", ")
    }
}

fn label_style_picker(ui: &mut egui::Ui, id_salt: &str, style: &mut LabelStyle) -> bool {
    let items: Vec<(LabelStyle, String)> = LabelStyle::ALL.iter().map(|s| (*s, s.label().to_string())).collect();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Style").size(12.5).color(MUTED));
        segmented(ui, id_salt, style, &items, 26.0)
    })
    .inner
}

fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fn read(names: &[&str]) -> Option<Vec<u8>> {
        names
            .iter()
            .find_map(|n| std::fs::read(format!(r"C:\Windows\Fonts\{n}")).ok())
    }
    let default_prop = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(b) = read(&["segoeui.ttf"]) {
        fonts.font_data.insert("segoe".into(), egui::FontData::from_owned(b));
        if let Some(f) = fonts.families.get_mut(&FontFamily::Proportional) {
            f.insert(0, "segoe".into());
        }
    }
    let mut bold_family = Vec::new();
    if let Some(b) = read(&["seguisb.ttf", "segoeuib.ttf"]) {
        fonts
            .font_data
            .insert("segoe-semibold".into(), egui::FontData::from_owned(b));
        bold_family.push("segoe-semibold".to_string());
    }
    bold_family.extend(default_prop);
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold_family);
    ctx.set_fonts(fonts);

    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);

    ctx.style_mut(|s| {
        use egui::TextStyle;
        s.text_styles.insert(TextStyle::Body, FontId::proportional(13.5));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(13.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(TextStyle::Heading, FontId::new(19.0, bold()));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));

        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.interact_size.y = 24.0;
        s.spacing.menu_margin = Margin::same(6.0);
        s.spacing.scroll.bar_width = 8.0;
        s.spacing.scroll.floating = true;

        s.scroll_animation = egui::style::ScrollAnimation::new(2400.0, egui::Rangef::new(0.08, 0.22));

        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = PANEL;
        v.window_stroke = Stroke::new(1.0_f32, LINE);
        v.window_rounding = Rounding::same(12.0);
        v.menu_rounding = Rounding::same(8.0);
        v.extreme_bg_color = FIELD;
        v.faint_bg_color = CHIP;
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = Color32::from_rgb(110, 84, 30);
        v.selection.stroke = Stroke::new(1.5_f32, ACCENT);
        v.override_text_color = None;

        let r = Rounding::same(6.0);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        v.widgets.noninteractive.rounding = r;
        v.widgets.inactive.weak_bg_fill = CHIP;
        v.widgets.inactive.bg_fill = CHIP;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        v.widgets.inactive.rounding = r;
        v.widgets.hovered.weak_bg_fill = CHIP_HOVER;
        v.widgets.hovered.bg_fill = CHIP_HOVER;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(80, 80, 90));
        v.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, TEXT);
        v.widgets.hovered.rounding = r;
        v.widgets.hovered.expansion = 0.0;
        v.widgets.active.weak_bg_fill = Color32::from_rgb(58, 58, 66);
        v.widgets.active.bg_fill = Color32::from_rgb(58, 58, 66);
        v.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        v.widgets.active.fg_stroke = Stroke::new(1.5_f32, TEXT);
        v.widgets.active.rounding = r;
        v.widgets.active.expansion = 0.0;
        v.widgets.open.weak_bg_fill = CHIP_HOVER;
        v.widgets.open.rounding = r;
    });

    ctx.options_mut(|o| o.line_scroll_speed = 90.0);
}

#[cfg(windows)]
fn is_elevated() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

#[cfg(not(windows))]
fn is_elevated() -> bool {
    false
}

fn load_tex(ctx: &egui::Context, name: &str, img: &RgbaImage) -> TextureHandle {
    let ci = ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    ctx.load_texture(name, ci, TextureOptions::LINEAR)
}

fn percent(done: u64, total: u64) -> u64 {
    (done * 100 / total.max(1)).min(100)
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

fn update_status(state: &UpdateState) -> (String, Color32) {
    match state {
        UpdateState::Idle => ("Not checked yet".into(), MUTED),
        UpdateState::Checking => ("Checking GitHub…".into(), MUTED),
        UpdateState::Latest => (format!("You're on the latest version (v{VERSION})"), OK),
        UpdateState::Available(r) => (format!("v{} is available · see what's new", r.version), ACCENT),
        UpdateState::Downloading { release, done } => (
            format!("Downloading v{} · {}%", release.version, percent(*done, release.size)),
            MUTED,
        ),
        UpdateState::Ready { release, .. } => (format!("v{} is ready to install", release.version), OK),
        UpdateState::Failed(e) => (e.clone(), DANGER),
    }
}

fn short(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

fn open_in_explorer(dir: &Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}

fn tile(ui: &mut egui::Ui, e: &Entry, selected: bool) -> (egui::Response, bool) {
    let (rect, resp) = ui.allocate_exact_size(TILE + egui::vec2(0.0, 22.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return (resp, false);
    }
    let img_rect = egui::Rect::from_min_size(rect.min, TILE);
    let painter = ui.painter_at(rect.expand(6.0));
    let hovered = resp.hovered();
    painter.rect_filled(img_rect, 10.0, if hovered { CHIP_HOVER } else { CHIP });

    match &e.thumb {
        Some(t) => {
            let ts = t.size_vec2();
            let scale = (TILE.x / ts.x).min(TILE.y / ts.y);
            let r = egui::Rect::from_center_size(img_rect.center(), ts * scale);
            egui::Image::from_texture(SizedTexture::from_handle(t))
                .rounding(8.0)
                .paint_at(ui, r);
        }
        None if e.failed => {
            let r = egui::Rect::from_center_size(img_rect.center(), Vec2::splat(18.0));
            paint_icon(&painter, r, Icon::Close, MUTED);
        }
        None => {
            egui::Spinner::new().size(20.0).paint_at(
                ui,
                egui::Rect::from_center_size(img_rect.center(), egui::vec2(20.0, 20.0)),
            );
        }
    }

    let galley = painter.layout_no_wrap(
        e.hit.source.label().to_string(),
        FontId::proportional(11.0),
        Color32::WHITE,
    );
    let badge = egui::Rect::from_min_size(
        img_rect.min + egui::vec2(7.0, 7.0),
        galley.size() + egui::vec2(20.0, 8.0),
    );
    painter.rect_filled(badge, 6.0, Color32::from_black_alpha(205));
    let [r, g, b] = e.hit.source.color();
    painter.circle_filled(
        badge.left_center() + egui::vec2(9.0, 0.0),
        3.5,
        Color32::from_rgb(r, g, b),
    );
    painter.galley(badge.min + egui::vec2(15.0, 4.0), galley, Color32::WHITE);

    let mut cap = Vec::new();
    if e.hit.width > 0 && e.hit.height > 0 {
        cap.push(format!("{}×{}", e.hit.width, e.hit.height));
    }
    match (&e.hit.lang, e.hit.source) {
        (Some(l), _) => cap.push(l.to_uppercase()),
        (None, SourceKind::Tmdb | SourceKind::Fanart) => cap.push("No text".into()),
        _ => {}
    }
    painter.text(
        egui::pos2(rect.min.x + 3.0, rect.max.y - 3.0),
        Align2::LEFT_BOTTOM,
        cap.join(" · "),
        FontId::proportional(11.5),
        MUTED,
    );

    if selected {
        painter.rect_stroke(img_rect.expand(3.0), 12.0, Stroke::new(3.0_f32, ACCENT));
    } else if hovered {
        painter.rect_stroke(
            img_rect.expand(1.0),
            11.0,
            Stroke::new(1.0_f32, Color32::from_gray(120)),
        );
    }

    let mut tip = String::new();
    if !e.hit.title.is_empty() {
        tip.push_str(&e.hit.title);
        tip.push('\n');
    }
    tip.push_str(&format!("{} · {}", e.hit.source.label(), e.hit.full_url));
    (
        resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand),
        true,
    )
}

fn gallery_tile(ui: &mut egui::Ui, tex: Option<&TextureHandle>, name: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(GTILE, Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let painter = ui.painter_at(rect.expand(4.0));
    painter.rect_filled(rect, 10.0, if resp.hovered() { CHIP_HOVER } else { FIELD });
    match tex {
        Some(t) => egui::Image::from_texture(SizedTexture::from_handle(t)).paint_at(ui, rect.shrink(5.0)),
        None => {
            let short_name = name.split('–').last().unwrap_or(name).trim();
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                short_name,
                FontId::proportional(10.5),
                MUTED,
            );
        }
    }
    if selected {
        painter.rect_stroke(rect.expand(1.5), 11.0, Stroke::new(2.5_f32, ACCENT));
    }
    resp.on_hover_text(name).on_hover_cursor(egui::CursorIcon::PointingHand)
}
