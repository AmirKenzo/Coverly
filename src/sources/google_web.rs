use super::{google, ImageHit};
use eframe::egui;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const SETTLE: Duration = Duration::from_millis(1500);
const TIMEOUT: Duration = Duration::from_secs(30);
const USER_TIMEOUT: Duration = Duration::from_secs(300);
const BLOCKED: &str = "Google has blocked searches from this connection for a while (“unusual traffic”). \
Switch the proxy on or off to search from another address, or try again later. Saved searches and the other sources still work.";

const GRAB_JS: &str = r#"(function () {
  var host = location.hostname, path = location.pathname;
  if (host.indexOf('consent.') === 0) {
    var b = document.querySelector('form[action*="consent"] button, button[aria-label*="Reject"], button[aria-label*="Accept"]');
    if (b) { b.click(); }
    return JSON.stringify({kind: 'consent'});
  }
  if (path.indexOf('/sorry/') === 0) {
    var form = document.querySelector('#captcha-form, .g-recaptcha, iframe[src*="recaptcha"]');
    return JSON.stringify({kind: form ? 'captcha' : 'blocked'});
  }
  if (host.indexOf('google.') < 0 || path.indexOf('/search') !== 0 || document.readyState === 'loading') {
    return JSON.stringify({kind: 'wait'});
  }
  window.scrollTo(0, document.body ? document.body.scrollHeight : 0);
  var imgs = [];
  document.querySelectorAll('img').forEach(function (i) {
    var s = i.currentSrc || i.src || i.getAttribute('data-src') || '';
    var thumb = s.indexOf('encrypted-tbn') >= 0 || (s.indexOf('data:image') === 0 && s.length > 1500);
    if (thumb) { imgs.push({src: s, w: i.naturalWidth || 0, h: i.naturalHeight || 0, alt: i.alt || ''}); }
  });
  return JSON.stringify({kind: 'page', html: document.documentElement.outerHTML, imgs: imgs});
})()"#;

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Grab {
    kind: String,
    html: String,
    imgs: Vec<GrabImg>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct GrabImg {
    src: String,
    w: u32,
    h: u32,
    alt: String,
}

fn hits_from(g: &Grab) -> Vec<ImageHit> {
    let mut hits = google::parse_page(&g.html);
    if hits.is_empty() {
        hits = g
            .imgs
            .iter()
            .map(|i| ImageHit {
                source: super::SourceKind::Google,
                thumb_url: i.src.clone(),
                full_url: i.src.clone(),
                width: i.w,
                height: i.h,
                lang: None,
                title: if i.alt.is_empty() {
                    "Google (small preview)".into()
                } else {
                    i.alt.clone()
                },
            })
            .collect();
        super::dedupe(&mut hits);
    }
    hits
}

fn save_debug(html: &str) -> std::path::PathBuf {
    let path = crate::config::cache_dir().join("google-last.html");
    let _ = std::fs::create_dir_all(crate::config::cache_dir());
    let _ = std::fs::write(&path, html);
    path
}

#[derive(Default)]
struct Shared {
    finished: Option<Instant>,
    html: Option<String>,
}

struct Job {
    gen: u64,
    url: String,
    routes: Vec<Option<String>>,
    route: usize,
    started: Instant,
    navigated: bool,
    asked: bool,
    consent_clicks: u32,
    needs_user: bool,
    grabs: u32,
    found: Vec<ImageHit>,
}

pub struct GoogleWeb {
    #[cfg(windows)]
    browser: Option<Browser>,
    job: Option<Job>,
    shared: Arc<Mutex<Shared>>,
    shown: Option<egui::Rect>,
    ready_gen: Option<u64>,
}

fn is_captcha(page: &str) -> bool {
    page == "__CAPTCHA__" || page.contains("/sorry/index") || page.contains("unusual traffic from your computer")
}

impl GoogleWeb {
    pub fn new() -> Self {
        Self {
            #[cfg(windows)]
            browser: None,
            job: None,
            shared: Arc::new(Mutex::new(Shared::default())),
            shown: None,
            ready_gen: None,
        }
    }

    pub fn start(&mut self, gen: u64, url: String, routes: Vec<Option<String>>) {
        self.hide();
        self.reset_shared();
        self.ready_gen = None;
        let routes = if routes.is_empty() { vec![None] } else { routes };
        self.job = Some(Job {
            gen,
            url,
            routes,
            route: 0,
            started: Instant::now(),
            navigated: false,
            asked: false,
            consent_clicks: 0,
            needs_user: false,
            grabs: 0,
            found: Vec::new(),
        });
    }

    #[cfg(windows)]
    pub fn more(&mut self, gen: u64) -> bool {
        if self.job.is_some() || self.ready_gen != Some(gen) {
            return false;
        }
        let Some(proxy) = self.browser.as_ref().map(|b| b.proxy.clone()) else {
            return false;
        };
        if let Ok(mut s) = self.shared.lock() {
            *s = Shared::default();
            s.finished = Some(Instant::now());
        }
        self.job = Some(Job {
            gen,
            url: String::new(),
            routes: vec![proxy],
            route: 0,
            started: Instant::now(),
            navigated: true,
            asked: false,
            consent_clicks: 0,
            needs_user: false,
            grabs: 1,
            found: Vec::new(),
        });
        true
    }

    #[cfg(not(windows))]
    pub fn more(&mut self, _gen: u64) -> bool {
        false
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    pub fn needs_user(&self) -> bool {
        self.job.as_ref().map(|j| j.needs_user).unwrap_or(false)
    }

    pub fn cancel(&mut self) -> Option<u64> {
        self.hide();
        self.job.take().map(|j| j.gen)
    }

    fn reset_shared(&self) {
        if let Ok(mut s) = self.shared.lock() {
            *s = Shared::default();
        }
    }

    #[cfg(windows)]
    pub fn place(&mut self, rect: Option<egui::Rect>) {
        let show = rect.filter(|_| self.needs_user());
        let Some(b) = &self.browser else { return };
        match show {
            Some(r) => {
                if self.shown != Some(r) {
                    let _ = b.view.set_bounds(wry::Rect {
                        position: wry::dpi::LogicalPosition::new(r.min.x as f64, r.min.y as f64).into(),
                        size: wry::dpi::LogicalSize::new(r.width().max(100.0) as f64, r.height().max(100.0) as f64)
                            .into(),
                    });
                    let _ = b.view.set_visible(true);
                    self.shown = Some(r);
                }
            }
            None => {
                if self.shown.is_some() {
                    let _ = b.view.set_visible(false);
                    self.shown = None;
                }
            }
        }
    }

    #[cfg(not(windows))]
    pub fn place(&mut self, _rect: Option<egui::Rect>) {}

    fn hide(&mut self) {
        #[cfg(windows)]
        if let Some(b) = &self.browser {
            let _ = b.view.set_visible(false);
        }
        self.shown = None;
    }

    #[cfg(windows)]
    pub fn poll(&mut self, frame: &eframe::Frame) -> Option<(u64, Result<Vec<ImageHit>, String>)> {
        let (gen, want) = {
            let j = self.job.as_ref()?;
            (j.gen, j.routes.get(j.route).cloned().unwrap_or(None))
        };

        let needs_new = match &self.browser {
            Some(b) => b.proxy != want,
            None => true,
        };
        if needs_new {
            self.browser = None;
            match Browser::new(frame, want, self.shared.clone()) {
                Ok(b) => self.browser = Some(b),
                Err(e) => {
                    self.job = None;
                    return Some((gen, Err(e)));
                }
            }
            if let Some(j) = self.job.as_mut() {
                j.navigated = false;
            }
        }
        let browser = self.browser.as_ref()?;
        let job = self.job.as_mut()?;

        let limit = if job.needs_user { USER_TIMEOUT } else { TIMEOUT };
        if job.started.elapsed() > limit {
            self.hide();
            self.job = None;
            return Some((
                gen,
                Err("Google took too long to answer. Check the connection or the proxy.".into()),
            ));
        }

        if !job.navigated {
            if let Ok(mut s) = self.shared.lock() {
                *s = Shared::default();
            }
            if let Err(e) = browser.view.load_url(&job.url) {
                self.job = None;
                return Some((gen, Err(format!("built-in browser: {e}"))));
            }
            job.navigated = true;
            job.asked = false;
            return None;
        }

        let (finished, html) = match self.shared.lock() {
            Ok(mut s) => (s.finished, s.html.take()),
            Err(_) => (None, None),
        };

        if let Some(raw) = html {
            let text: String = serde_json::from_str(&raw).unwrap_or(raw);
            let grab: Grab = serde_json::from_str(&text).unwrap_or_default();
            job.asked = false;

            match grab.kind.as_str() {
                "consent" => {
                    job.consent_clicks += 1;
                    if let Ok(mut s) = self.shared.lock() {
                        s.finished = None;
                    }
                    if job.consent_clicks > 3 {
                        self.job = None;
                        return Some((gen, Err("Google's cookie page couldn't be passed.".into())));
                    }
                    return None;
                }
                "captcha" => {
                    if let Ok(mut s) = self.shared.lock() {
                        s.finished = None;
                    }
                    if job.needs_user {
                        return None;
                    }
                    if job.route + 1 < job.routes.len() {
                        job.route += 1;
                        job.navigated = false;
                        job.started = Instant::now();
                        return None;
                    }
                    job.needs_user = true;
                    job.started = Instant::now();
                    return None;
                }
                "blocked" => {
                    if let Ok(mut s) = self.shared.lock() {
                        s.finished = None;
                    }
                    if job.route + 1 < job.routes.len() {
                        job.route += 1;
                        job.navigated = false;
                        job.started = Instant::now();
                        return None;
                    }
                    let was_shown = job.needs_user;
                    self.job = None;
                    if was_shown {
                        self.hide();
                    }
                    return Some((gen, Err(BLOCKED.into())));
                }
                "page" => {}
                _ => {
                    if let Ok(mut s) = self.shared.lock() {
                        s.finished = Some(Instant::now());
                    }
                    return None;
                }
            }

            if is_captcha(&grab.html) && !grab.html.contains("data-docid") {
                job.needs_user = true;
                job.started = Instant::now();
                return None;
            }

            job.grabs += 1;
            let mut hits = std::mem::take(&mut job.found);
            hits.extend(hits_from(&grab));
            super::dedupe(&mut hits);
            if job.grabs < 2 && hits.len() < 40 {
                job.found = hits;
                if let Ok(mut s) = self.shared.lock() {
                    s.finished = Some(Instant::now());
                }
                return None;
            }

            let was_shown = job.needs_user;
            self.job = None;
            if was_shown {
                self.hide();
            }
            let saved = save_debug(&grab.html);
            if hits.is_empty() {
                return Some((
                    gen,
                    Err(format!(
                        "Google's page had no pictures we could read (a copy is saved at {}).",
                        saved.display()
                    )),
                ));
            }
            self.ready_gen = Some(gen);
            return Some((gen, Ok(hits)));
        }

        if let Some(t) = finished {
            let settle = if job.needs_user || job.grabs > 0 {
                Duration::from_millis(900)
            } else {
                SETTLE
            };
            if !job.asked && t.elapsed() >= settle {
                job.asked = true;
                let shared = self.shared.clone();
                let r = browser.view.evaluate_script_with_callback(GRAB_JS, move |res| {
                    if let Ok(mut s) = shared.lock() {
                        s.html = Some(res);
                    }
                });
                if let Err(e) = r {
                    self.hide();
                    self.job = None;
                    return Some((gen, Err(format!("built-in browser: {e}"))));
                }
            }
        }
        None
    }

    #[cfg(not(windows))]
    pub fn poll(&mut self, _frame: &eframe::Frame) -> Option<(u64, Result<Vec<ImageHit>, String>)> {
        let job = self.job.take()?;
        Some((job.gen, Err("Google search needs Windows (Edge WebView2).".into())))
    }
}

#[cfg(windows)]
struct Browser {
    view: wry::WebView,
    _ctx: wry::WebContext,
    proxy: Option<String>,
}

#[cfg(windows)]
impl Browser {
    fn new(frame: &eframe::Frame, proxy: Option<String>, shared: Arc<Mutex<Shared>>) -> Result<Self, String> {
        let dir = crate::config::cache_dir()
            .join("webview")
            .join(if proxy.is_some() { "proxy" } else { "direct" });
        let _ = std::fs::create_dir_all(&dir);
        let mut ctx = wry::WebContext::new(Some(dir));

        const ARGS: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
        if proxy.is_none() {
            std::env::set_var(ARGS, "--no-proxy-server");
        } else {
            std::env::remove_var(ARGS);
        }

        let on_load = shared.clone();
        let mut builder = wry::WebViewBuilder::new_with_web_context(&mut ctx)
            .with_url("about:blank")
            .with_visible(false)
            .with_bounds(wry::Rect {
                position: wry::dpi::LogicalPosition::new(0.0, 0.0).into(),
                size: wry::dpi::LogicalSize::new(1280.0, 1600.0).into(),
            })
            .with_on_page_load_handler(move |ev, _url| {
                if matches!(ev, wry::PageLoadEvent::Finished) {
                    if let Ok(mut s) = on_load.lock() {
                        s.finished = Some(Instant::now());
                    }
                }
            });
        if let Some(cfg) = proxy.as_deref().and_then(proxy_config) {
            builder = builder.with_proxy_config(cfg);
        }
        let view = builder
            .build_as_child(frame)
            .map_err(|e| format!("couldn't start the built-in browser (Microsoft Edge WebView2 Runtime): {e}"))?;
        Ok(Self { view, _ctx: ctx, proxy })
    }
}

#[cfg(windows)]
fn proxy_config(url: &str) -> Option<wry::ProxyConfig> {
    let (scheme, rest) = url.split_once("://")?;
    let (host, port) = rest.trim_end_matches('/').rsplit_once(':')?;
    let ep = wry::ProxyEndpoint {
        host: host.to_string(),
        port: port.to_string(),
    };
    Some(if scheme.starts_with("socks") {
        wry::ProxyConfig::Socks5(ep)
    } else {
        wry::ProxyConfig::Http(ep)
    })
}
