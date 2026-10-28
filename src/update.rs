use crate::config::{cache_dir, Config};
use anyhow::{anyhow, bail, Context, Result};
use eframe::egui;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{self, AtomicBool},
        Arc, Mutex,
    },
    time::Duration,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const EXE: &str = "coverly.exe";
const SUMS: &str = "SHA256SUMS.txt";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    pub published: String,
    pub size: u64,
    asset: String,
    url: String,
    sums: String,
}

#[derive(Clone, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    Latest,
    Available(Release),
    Downloading {
        release: Release,
        done: u64,
    },
    Ready {
        release: Release,
        file: PathBuf,
    },
    Failed(String),
}

pub struct Updater {
    state: Arc<Mutex<State>>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
}

impl Updater {
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            state: Arc::default(),
            cancel: Arc::default(),
            ctx,
        }
    }

    pub fn state(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn busy(&self) -> bool {
        matches!(self.state(), State::Checking | State::Downloading { .. })
    }

    pub fn dismiss(&self) {
        put(&self.state, &self.ctx, State::Idle);
    }

    /// Only an automatic check (`manual == false`) hides the version the user chose to skip.
    pub fn check(&self, cfg: &Config, manual: bool) {
        if self.busy() {
            return;
        }
        put(&self.state, &self.ctx, State::Checking);
        let routes = cfg.update_routes();
        let beta = cfg.update_beta;
        let skip = (!manual).then(|| cfg.update_skip.clone());
        let (state, ctx) = (self.state.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let next = match latest(&routes, beta) {
                Ok(Some(r)) if skip.as_deref() != Some(r.version.as_str()) => State::Available(r),
                Ok(_) => State::Latest,
                Err(e) => State::Failed(format!("Couldn't check for updates: {e:#}")),
            };
            put(&state, &ctx, next);
        });
    }

    pub fn download(&self, release: Release, cfg: &Config) {
        if self.busy() {
            return;
        }
        self.cancel.store(false, atomic::Ordering::Relaxed);
        put(
            &self.state,
            &self.ctx,
            State::Downloading {
                release: release.clone(),
                done: 0,
            },
        );
        let routes = cfg.update_routes();
        let (state, ctx, cancel) = (self.state.clone(), self.ctx.clone(), self.cancel.clone());
        std::thread::spawn(move || {
            let progress = |done: u64| {
                put(
                    &state,
                    &ctx,
                    State::Downloading {
                        release: release.clone(),
                        done,
                    },
                )
            };
            let next = match fetch(&release, &routes, &progress, &cancel) {
                Ok(file) => State::Ready {
                    release: release.clone(),
                    file,
                },
                Err(_) if cancel.load(atomic::Ordering::Relaxed) => State::Available(release.clone()),
                Err(e) => State::Failed(format!("Couldn't download the update: {e:#}")),
            };
            put(&state, &ctx, next);
        });
    }

    pub fn cancel(&self) {
        self.cancel.store(true, atomic::Ordering::Relaxed);
    }

    /// Starts the installer (or swaps the portable files) and returns true when Coverly should close.
    pub fn install(&self) -> bool {
        let State::Ready { file, .. } = self.state() else {
            return false;
        };
        let done = if file.is_dir() {
            replace_portable(&file)
        } else {
            run_installer(&file)
        };
        match done {
            Ok(()) => true,
            Err(e) => {
                put(
                    &self.state,
                    &self.ctx,
                    State::Failed(format!("Couldn't install the update: {e:#}")),
                );
                false
            }
        }
    }
}

fn put(state: &Mutex<State>, ctx: &egui::Context, next: State) {
    if let Ok(mut s) = state.lock() {
        *s = next;
    }
    ctx.request_repaint();
}

pub fn cleanup() {
    let _ = std::fs::remove_dir_all(download_dir());
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_exe(&exe));
    }
}

fn download_dir() -> PathBuf {
    cache_dir().join("update")
}

fn old_exe(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

fn installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("uninstall.exe").is_file()))
        .unwrap_or(false)
}

fn repo() -> &'static str {
    env!("CARGO_PKG_REPOSITORY")
        .trim_start_matches("https://github.com/")
        .trim_end_matches('/')
}

fn client(proxy: &Option<String>, timeout: Option<Duration>) -> Result<reqwest::blocking::Client> {
    let b = reqwest::blocking::Client::builder()
        .user_agent(format!("Coverly/{VERSION}"))
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout);
    let b = match proxy {
        Some(p) => b.proxy(reqwest::Proxy::all(p).context("invalid proxy address")?),
        None => b.no_proxy(),
    };
    Ok(b.build()?)
}

fn with_routes<T>(
    routes: &[Option<String>],
    timeout: Option<Duration>,
    f: impl Fn(&reqwest::blocking::Client) -> Result<T>,
) -> Result<T> {
    let mut last = anyhow!("no connection");
    for proxy in routes {
        match client(proxy, timeout).and_then(|c| f(&c)) {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn latest(routes: &[Option<String>], beta: bool) -> Result<Option<Release>> {
    let url = format!("https://api.github.com/repos/{}/releases?per_page=20", repo());
    let list: Vec<ApiRelease> = with_routes(routes, Some(Duration::from_secs(20)), |c| {
        Ok(c.get(&url)
            .header("Accept", "application/vnd.github+json")
            .send()?
            .error_for_status()?
            .json()?)
    })?;
    Ok(pick(list, beta, VERSION, installed()))
}

fn pick(list: Vec<ApiRelease>, beta: bool, current: &str, installed: bool) -> Option<Release> {
    list.into_iter()
        .filter(|r| !r.draft && (beta || !r.prerelease))
        .filter_map(|r| {
            let version = r.tag_name.trim_start_matches('v').to_string();
            if compare(&version, current) != Some(Ordering::Greater) {
                return None;
            }
            let asset = r.assets.iter().find(|a| wanted(&a.name, installed))?;
            let sums = r.assets.iter().find(|a| a.name == SUMS)?;
            Some(Release {
                notes: r.body.clone().unwrap_or_default(),
                page: r.html_url.clone(),
                published: r.published_at.as_deref().unwrap_or("").chars().take(10).collect(),
                size: asset.size,
                asset: asset.name.clone(),
                url: asset.browser_download_url.clone(),
                sums: sums.browser_download_url.clone(),
                version,
            })
        })
        .max_by(|a, b| compare(&a.version, &b.version).unwrap_or(Ordering::Equal))
}

fn wanted(name: &str, installed: bool) -> bool {
    if installed {
        name.starts_with("Coverly-Setup-") && name.ends_with(".exe")
    } else {
        name.starts_with("Coverly-") && name.ends_with("-portable.zip")
    }
}

fn parse(v: &str) -> Option<([u64; 3], Option<&str>)> {
    let v = v.trim().trim_start_matches('v');
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let mut parts = core.split('.');
    let mut n = [0u64; 3];
    for x in &mut n {
        *x = parts.next()?.parse().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some((n, pre))
}

/// Semantic version order: 1.2.0-beta.2 < 1.2.0-beta.10 < 1.2.0 < 1.2.1.
fn compare(a: &str, b: &str) -> Option<Ordering> {
    let (an, ap) = parse(a)?;
    let (bn, bp) = parse(b)?;
    Some(an.cmp(&bn).then_with(|| match (ap, bp) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => compare_pre(x, y),
    }))
}

fn compare_pre(a: &str, b: &str) -> Ordering {
    let mut ai = a.split('.');
    let mut bi = b.split('.');
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let o = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut it = line.split_whitespace();
        let hash = it.next()?;
        let file = it.next()?.trim_start_matches('*');
        (file == name).then(|| hash.to_ascii_lowercase())
    })
}

fn fetch(r: &Release, routes: &[Option<String>], progress: &dyn Fn(u64), cancel: &AtomicBool) -> Result<PathBuf> {
    let dir = download_dir();
    std::fs::create_dir_all(&dir)?;
    let sums = with_routes(routes, Some(Duration::from_secs(20)), |c| {
        Ok(c.get(&r.sums).send()?.error_for_status()?.text()?)
    })?;
    let want = checksum_for(&sums, &r.asset).ok_or_else(|| anyhow!("{} isn't listed in {SUMS}", r.asset))?;

    let file = dir.join(&r.asset);
    let got = with_routes(routes, None, |c| save(c, &r.url, &file, progress, cancel))?;
    if got != want {
        let _ = std::fs::remove_file(&file);
        bail!("the downloaded file is damaged (checksum mismatch)");
    }
    if !r.asset.ends_with(".zip") {
        return Ok(file);
    }

    let out = dir.join(format!("Coverly-{}", r.version));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;
    let status = hidden(Command::new(tar()).arg("-xf").arg(&file).arg("-C").arg(&out))
        .status()
        .context("couldn't run tar.exe")?;
    if !status.success() || !out.join(EXE).is_file() {
        bail!("couldn't unpack {}", r.asset);
    }
    Ok(out)
}

fn save(
    c: &reqwest::blocking::Client,
    url: &str,
    file: &Path,
    progress: &dyn Fn(u64),
    cancel: &AtomicBool,
) -> Result<String> {
    let mut resp = c.get(url).send()?.error_for_status()?;
    let mut out = File::create(file)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        if cancel.load(atomic::Ordering::Relaxed) {
            drop(out);
            let _ = std::fs::remove_file(file);
            bail!("cancelled");
        }
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        done += n as u64;
        progress(done);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn tar() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join("System32").join("tar.exe"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("tar"))
}

#[cfg(windows)]
fn hidden(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
fn hidden(cmd: &mut Command) -> &mut Command {
    cmd
}

/// The running exe can be renamed but not overwritten, so it moves aside and the new files take its place.
fn replace_portable(from: &Path) -> Result<()> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().context("no program folder")?.to_path_buf();
    let old = old_exe(&exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).context("the program folder isn't writable")?;
    if let Err(e) = copy_tree(from, &dir) {
        if !exe.exists() {
            let _ = std::fs::rename(&old, &exe);
        }
        return Err(e);
    }
    Command::new(dir.join(EXE)).current_dir(&dir).spawn()?;
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).with_context(|| format!("couldn't write {}", target.display()))?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn run_installer(setup: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    const SW_SHOWNORMAL: i32 = 1;
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let verb = wide("runas".as_ref());
    let file = wide(setup.as_os_str());
    let args = wide("/S /RELAUNCH".as_ref());
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            args.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if code <= 32 {
        bail!("the installer didn't start (administrator permission is needed)");
    }
    Ok(())
}

#[cfg(not(windows))]
fn run_installer(_setup: &Path) -> Result<()> {
    bail!("the installer only runs on Windows")
}

pub enum Note {
    Heading(String),
    Item(String),
}

pub fn notes(body: &str) -> Vec<Note> {
    let mut out = Vec::new();
    for line in body.lines().map(str::trim) {
        if line.starts_with("### Downloads") {
            break;
        }
        if line.is_empty() || line.starts_with("## ") || line.starts_with("**Full Changelog**") {
            continue;
        }
        if let Some(h) = line.strip_prefix("### ") {
            let h = h.trim_start_matches(|c: char| !c.is_alphanumeric()).trim();
            out.push(Note::Heading(h.to_string()));
        } else {
            let item = line.trim_start_matches(['-', '*']).trim().replace("**", "");
            out.push(Note::Item(item));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> ApiAsset {
        ApiAsset {
            name: name.into(),
            browser_download_url: format!("https://example.com/{name}"),
            size: 10,
        }
    }

    fn release(tag: &str, prerelease: bool) -> ApiRelease {
        let v = tag.trim_start_matches('v');
        ApiRelease {
            tag_name: tag.into(),
            body: Some("notes".into()),
            html_url: format!("https://example.com/{tag}"),
            published_at: Some("2026-10-28T08:56:32Z".into()),
            draft: false,
            prerelease,
            assets: vec![
                asset(&format!("Coverly-Setup-{v}.exe")),
                asset(&format!("Coverly-{v}-portable.zip")),
                asset(SUMS),
            ],
        }
    }

    #[test]
    fn version_order() {
        assert_eq!(compare("1.0.3", "1.0.2"), Some(Ordering::Greater));
        assert_eq!(compare("v1.0.10", "1.0.9"), Some(Ordering::Greater));
        assert_eq!(compare("1.1.0", "1.1.0"), Some(Ordering::Equal));
        assert_eq!(compare("1.1.0-beta.1", "1.1.0"), Some(Ordering::Less));
        assert_eq!(compare("1.1.0-beta.10", "1.1.0-beta.2"), Some(Ordering::Greater));
        assert_eq!(compare("1.1.0-beta", "1.1.0-beta.1"), Some(Ordering::Less));
        assert_eq!(compare("1.1", "1.0.0"), None);
    }

    #[test]
    fn picks_newest_matching_release() {
        let list = vec![
            release("v1.2.0-beta.1", true),
            release("v1.1.0", false),
            release("v1.0.2", false),
        ];
        let r = pick(list, false, "1.0.2", true).unwrap();
        assert_eq!(r.version, "1.1.0");
        assert_eq!(r.asset, "Coverly-Setup-1.1.0.exe");
        assert_eq!(r.published, "2026-10-28");

        let list = vec![release("v1.2.0-beta.1", true), release("v1.1.0", false)];
        let r = pick(list, true, "1.0.2", false).unwrap();
        assert_eq!(r.version, "1.2.0-beta.1");
        assert_eq!(r.asset, "Coverly-1.2.0-beta.1-portable.zip");

        assert!(pick(vec![release("v1.0.2", false)], false, "1.0.2", true).is_none());
    }

    #[test]
    fn skips_release_without_checksums() {
        let mut r = release("v1.1.0", false);
        r.assets.retain(|a| a.name != SUMS);
        assert!(pick(vec![r], false, "1.0.2", true).is_none());
    }

    #[test]
    fn finds_checksum() {
        let sums = "AbC123  Coverly-Setup-1.1.0.exe\ndef456 *Coverly-1.1.0-portable.zip\n";
        assert_eq!(checksum_for(sums, "Coverly-Setup-1.1.0.exe").as_deref(), Some("abc123"));
        assert_eq!(
            checksum_for(sums, "Coverly-1.1.0-portable.zip").as_deref(),
            Some("def456")
        );
        assert_eq!(checksum_for(sums, "other.zip"), None);
    }

    #[test]
    fn release_notes() {
        let body = "## What's Changed in v1.1.0\n\n### ✨ Features\n\n- **update**: Check GitHub for new versions\n\n\
                    **Full Changelog**: https://example.com\n\n### Downloads\n- **Coverly-Setup-1.1.0.exe**: installer\n";
        let n = notes(body);
        assert_eq!(n.len(), 2);
        assert!(matches!(&n[0], Note::Heading(h) if h == "Features"));
        assert!(matches!(&n[1], Note::Item(t) if t == "update: Check GitHub for new versions"));
    }
}
