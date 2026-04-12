use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::app::CleanupMode;

const KEEP_STORE_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct KeepStore {
    path: PathBuf,
    labels_by_mode: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeepStoreFile {
    version: u32,
    labels_by_mode: BTreeMap<String, BTreeSet<String>>,
}

impl KeepStore {
    pub fn load(repo: &Path) -> Result<Self> {
        let path = keep_store_path(repo)?;
        if !path.exists() {
            return Ok(Self {
                path,
                labels_by_mode: BTreeMap::new(),
            });
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read keep-label store {}", path.display()))?;
        let parsed = serde_json::from_str::<KeepStoreFile>(&contents)
            .with_context(|| format!("failed to parse keep-label store {}", path.display()))?;

        if parsed.version != KEEP_STORE_VERSION {
            bail!(
                "unsupported keep-label store version {} in {}",
                parsed.version,
                path.display()
            );
        }

        Ok(Self {
            path,
            labels_by_mode: parsed.labels_by_mode,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_saved(&self, mode: CleanupMode, branch: &str) -> bool {
        self.labels_by_mode
            .get(mode.name())
            .is_some_and(|branches| branches.contains(branch))
    }

    pub fn replace_mode<I, S>(&mut self, mode: CleanupMode, branches: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let saved = branches
            .into_iter()
            .map(Into::into)
            .collect::<BTreeSet<_>>();
        if saved.is_empty() {
            self.labels_by_mode.remove(mode.name());
        } else {
            self.labels_by_mode.insert(mode.name().to_string(), saved);
        }

        self.persist()
    }

    fn persist(&self) -> Result<()> {
        if self.labels_by_mode.is_empty() {
            match fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to remove keep-label store {}", self.path.display())
                    });
                }
            }
            return Ok(());
        }

        let parent = self
            .path
            .parent()
            .context("keep-label store path missing parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create keep-label dir {}", parent.display()))?;

        let file = KeepStoreFile {
            version: KEEP_STORE_VERSION,
            labels_by_mode: self.labels_by_mode.clone(),
        };
        let contents =
            serde_json::to_string_pretty(&file).context("failed to serialize keep-label store")?;
        fs::write(&self.path, format!("{contents}\n"))
            .with_context(|| format!("failed to write keep-label store {}", self.path.display()))
    }
}

fn keep_store_path(repo: &Path) -> Result<PathBuf> {
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
    Ok(PathBuf::from(common_dir.trim()).join("git-broom/keep-labels.json"))
}
