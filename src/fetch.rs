//! Cache-first HTTP fetcher. Every URL is stored under data/raw/<slug>.<ext> with a
//! manifest recording URL and content hash so the build is reproducible offline.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub entries: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ManifestEntry {
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}

pub struct Fetcher {
    raw_dir: PathBuf,
    client: Option<reqwest::blocking::Client>,
    manifest: Manifest,
    offline: bool,
    delay_ms: u64,
}

impl Fetcher {
    pub fn new(raw_dir: &Path, offline: bool) -> Result<Self> {
        fs::create_dir_all(raw_dir)?;
        let manifest_path = raw_dir.join("manifest.json");
        let manifest = if manifest_path.exists() {
            serde_json::from_str(&fs::read_to_string(&manifest_path)?)?
        } else {
            Manifest::default()
        };
        let client = if offline {
            None
        } else {
            Some(
                reqwest::blocking::Client::builder()
                    .user_agent(UA)
                    .timeout(Duration::from_secs(60))
                    .build()?,
            )
        };
        Ok(Self { raw_dir: raw_dir.to_path_buf(), client, manifest, offline, delay_ms: 400 })
    }

    fn path_for(&self, slug: &str, ext: &str) -> PathBuf {
        self.raw_dir.join(format!("{slug}.{ext}"))
    }

    /// Return cached body if present, otherwise fetch, cache, and return.
    pub fn get(&mut self, slug: &str, ext: &str, url: &str) -> Result<String> {
        let path = self.path_for(slug, ext);
        if path.exists() {
            return fs::read_to_string(&path).with_context(|| format!("read cache {}", path.display()));
        }
        if self.offline {
            return Err(anyhow!("offline and no cache for {slug} ({url})"));
        }
        let client = self.client.as_ref().expect("client");
        let mut last_err = None;
        for attempt in 0..3 {
            if attempt > 0 {
                sleep(Duration::from_millis(1500 * attempt));
            }
            match client
                .get(url)
                .header("Accept", "text/html,application/xhtml+xml,text/csv,*/*;q=0.8")
                .header("Accept-Language", "en-US,en;q=0.9")
                .send()
            {
                Ok(resp) => {
                    let status = resp.status();
                    match resp.text() {
                        Ok(body) if status.is_success() && !body.trim().is_empty() => {
                            fs::write(&path, &body)?;
                            let mut hasher = Sha256::new();
                            hasher.update(body.as_bytes());
                            self.manifest.entries.insert(
                                format!("{slug}.{ext}"),
                                ManifestEntry {
                                    url: url.to_string(),
                                    sha256: hex::encode(hasher.finalize()),
                                    bytes: body.len() as u64,
                                },
                            );
                            self.save_manifest()?;
                            sleep(Duration::from_millis(self.delay_ms));
                            return Ok(body);
                        }
                        Ok(_) => last_err = Some(anyhow!("HTTP {status} for {url}")),
                        Err(e) => last_err = Some(anyhow!("body error for {url}: {e}")),
                    }
                }
                Err(e) => last_err = Some(anyhow!("request error for {url}: {e}")),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("unknown fetch failure for {url}")))
    }

    fn save_manifest(&self) -> Result<()> {
        let path = self.raw_dir.join("manifest.json");
        fs::write(&path, serde_json::to_string_pretty(&self.manifest)?)?;
        Ok(())
    }
}
