use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const PR_CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct PrCache {
    path: PathBuf,
    entries_by_remote: BTreeMap<String, PrCacheRemoteEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrCacheRemoteEntry {
    pub remote_url: String,
    pub refreshed_at: i64,
    pub default_branch: String,
    pub pull_requests_by_head: BTreeMap<String, CachedPullRequestRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedPullRequestRecord {
    pub state: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrCacheFile {
    version: u32,
    entries_by_remote: BTreeMap<String, PrCacheRemoteEntry>,
}

impl PrCache {
    pub fn new(repo: &Path) -> Result<Self> {
        Ok(Self {
            path: pr_cache_path(repo)?,
            entries_by_remote: BTreeMap::new(),
        })
    }

    pub fn load(repo: &Path) -> Result<Self> {
        let path = pr_cache_path(repo)?;
        if !path.exists() {
            return Ok(Self {
                path,
                entries_by_remote: BTreeMap::new(),
            });
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read PR cache {}", path.display()))?;
        let parsed = serde_json::from_str::<PrCacheFile>(&contents)
            .with_context(|| format!("failed to parse PR cache {}", path.display()))?;

        if parsed.version != PR_CACHE_VERSION {
            bail!(
                "unsupported PR cache version {} in {}",
                parsed.version,
                path.display()
            );
        }

        Ok(Self {
            path,
            entries_by_remote: parsed.entries_by_remote,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn remote_entry(&self, remote: &str, remote_url: &str) -> Option<&PrCacheRemoteEntry> {
        self.entries_by_remote
            .get(remote)
            .filter(|entry| entry.remote_url == remote_url)
    }

    pub fn replace_remote(&mut self, remote: &str, entry: PrCacheRemoteEntry) -> Result<()> {
        self.entries_by_remote.insert(remote.to_string(), entry);
        self.persist()
    }

    fn persist(&self) -> Result<()> {
        if self.entries_by_remote.is_empty() {
            match fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to remove PR cache {}", self.path.display())
                    });
                }
            }
            return Ok(());
        }

        let parent = self
            .path
            .parent()
            .context("PR cache path missing parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create PR cache dir {}", parent.display()))?;

        let file = PrCacheFile {
            version: PR_CACHE_VERSION,
            entries_by_remote: self.entries_by_remote.clone(),
        };
        let contents =
            serde_json::to_string_pretty(&file).context("failed to serialize PR cache")?;
        fs::write(&self.path, format!("{contents}\n"))
            .with_context(|| format!("failed to write PR cache {}", self.path.display()))
    }
}

fn pr_cache_path(repo: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to resolve git common dir")?;

    if !output.status.success() {
        bail!(
            "failed to resolve git common dir: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let common_dir =
        String::from_utf8(output.stdout).context("git returned non-utf8 common dir output")?;
    Ok(PathBuf::from(common_dir.trim()).join("git-broom/pr-cache.json"))
}
