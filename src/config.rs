use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Top-level configuration loaded from `config.toml`.
#[derive(Debug, Deserialize)]
pub struct Config {
    pub scoop: Option<ScoopConfig>,
    #[serde(default)]
    pub dotfiles: Vec<DotfileEntry>,
}

/// A Scoop bucket entry — either a simple name or a name with a custom source URL.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum BucketEntry {
    /// Simple bucket name (e.g., "main", "extras")
    Name(String),
    /// Bucket with a custom source URL
    WithSource { name: String, source: String },
}

impl BucketEntry {
    /// Get the bucket name.
    pub fn name(&self) -> &str {
        match self {
            BucketEntry::Name(n) => n,
            BucketEntry::WithSource { name, .. } => name,
        }
    }

    /// Get the optional custom source URL.
    pub fn source(&self) -> Option<&str> {
        match self {
            BucketEntry::Name(_) => None,
            BucketEntry::WithSource { source, .. } => Some(source),
        }
    }
}

/// A Scoop package entry — either a simple name or a name with a specific bucket source.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PackageEntry {
    /// Simple package name (e.g., "git", "7zip")
    Name(String),
    /// Package with a specific bucket source (e.g., { name = "zig", bucket = "main" })
    WithBucket { name: String, bucket: String },
}

impl PackageEntry {
    /// Get the package name.
    pub fn name(&self) -> &str {
        match self {
            PackageEntry::Name(n) => n,
            PackageEntry::WithBucket { name, .. } => name,
        }
    }

    /// Get the optional bucket source.
    pub fn bucket(&self) -> Option<&str> {
        match self {
            PackageEntry::Name(_) => None,
            PackageEntry::WithBucket { bucket, .. } => Some(bucket),
        }
    }

    /// Get the install specifier (e.g., "bucket/package" or just "package").
    pub fn install_spec(&self) -> String {
        match self {
            PackageEntry::Name(n) => n.clone(),
            PackageEntry::WithBucket { name, bucket } => format!("{}/{}", bucket, name),
        }
    }
}

/// Scoop package manager configuration.
#[derive(Debug, Deserialize)]
pub struct ScoopConfig {
    /// List of Scoop buckets — each entry can be a string or {name, source}
    #[serde(default)]
    pub buckets: Vec<BucketEntry>,
    /// List of packages — each entry can be a string or {name, bucket}
    #[serde(default)]
    pub packages: Vec<PackageEntry>,
}

/// Dotfile target type — determines how the target path is resolved.
#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DotfileType {
    /// Regular file: target is an absolute path (supports `~` expansion).
    #[default]
    Link,
    /// Scoop persist: target is relative to `~/scoop/persist/`.
    Persist,
}

/// Dotfile sync behavior — determines how the file is synced.
#[derive(Debug, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DotfileBehavior {
    /// Copy the file to the target path.
    #[default]
    Copy,
    /// Create a symlink at the target path.
    Link,
}

/// A single dotfile mapping: source (in repo) → target (on disk).
#[derive(Debug, Deserialize)]
pub struct DotfileEntry {
    pub source: String,
    /// Target path. Interpretation depends on `type`:
    /// - `file` (default): absolute path, supports `~` expansion
    /// - `persist`: relative to `~/scoop/persist/`
    pub target: String,
    /// Dotfile target type: "link" (default) or "persist".
    #[serde(default, rename = "type")]
    pub dotfile_type: DotfileType,
    /// Sync behavior: "copy" (default) or "link".
    #[serde(default)]
    pub behavior: DotfileBehavior,
}

impl DotfileEntry {
    /// Resolve the effective target path for this dotfile entry.
    pub fn resolve_target(&self) -> Result<PathBuf> {
        match self.dotfile_type {
            DotfileType::Link => Config::resolve_target(&self.target),
            DotfileType::Persist => {
                let home = dirs::home_dir().context("Could not determine home directory")?;
                Ok(home.join("scoop").join("persist").join(&self.target))
            }
        }
    }
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
        Ok(config)
    }

    /// Resolve the target path, expanding `~` to the user's home directory.
    pub fn resolve_target(target: &str) -> Result<PathBuf> {
        if let Some(rest) = target.strip_prefix("~/") {
            let home = dirs::home_dir().context("Could not determine home directory")?;
            Ok(home.join(rest))
        } else if target == "~" {
            dirs::home_dir().context("Could not determine home directory")
        } else {
            Ok(PathBuf::from(target))
        }
    }
}