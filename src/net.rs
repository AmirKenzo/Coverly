use anyhow::{Context, Result};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    time::Duration,
};

pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

pub struct Http {
    pub client: reqwest::blocking::Client,
    cache: PathBuf,
}

impl Http {
    pub fn new(proxy: Option<String>) -> Result<Self> {
        let mut b = reqwest::blocking::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(25))
            .connect_timeout(Duration::from_secs(10));
        b = match &proxy {
            Some(p) => b.proxy(reqwest::Proxy::all(p).context("invalid proxy address")?),
            None => b.no_proxy(),
        };
        let cache = crate::config::cache_dir().join("http");
        let _ = std::fs::create_dir_all(&cache);
        Ok(Self {
            client: b.build()?,
            cache,
        })
    }

    pub fn bytes(&self, url: &str) -> Result<Vec<u8>> {
        if let Some(path) = url.strip_prefix("file:") {
            return Ok(std::fs::read(path)?);
        }
        if let Some(rest) = url.strip_prefix("data:") {
            let b64 = rest.split_once(";base64,").map(|(_, d)| d).unwrap_or("");
            return crate::sources::base64_decode(b64).context("bad embedded image");
        }
        let mut h = DefaultHasher::new();
        url.hash(&mut h);
        let file = self.cache.join(format!("{:016x}", h.finish()));
        if let Ok(b) = std::fs::read(&file) {
            if !b.is_empty() {
                return Ok(b);
            }
        }
        let b = self
            .client
            .get(url)
            .header("Referer", origin(url))
            .header("Accept", "image/avif,image/webp,image/png,image/jpeg,*/*;q=0.8")
            .send()?
            .error_for_status()?
            .bytes()?
            .to_vec();
        let _ = std::fs::write(&file, &b);
        Ok(b)
    }
}

fn origin(url: &str) -> String {
    let base: Vec<&str> = url.split('/').take(3).collect();
    format!("{}/", base.join("/"))
}
