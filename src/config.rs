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

/// A single dotfile mapping: source (in repo) → target (on disk).
#[derive(Debug, Deserialize)]
pub struct DotfileEntry {
    pub source: String,
    /// Target path on disk. Supports `~` for home directory.
    /// Mutually exclusive with `persist`.
    #[serde(default)]
    pub target: String,
    /// Scoop persist path relative to `~/scoop/persist/`.
    /// Mutually exclusive with `target`.
    /// Example: "mihomo/config.yaml" → `~/scoop/persist/mihomo/config.yaml`
    #[serde(default)]
    pub persist: String,
}

impl DotfileEntry {
    /// Resolve the effective target path for this dotfile entry.
    /// If `persist` is set, resolves to `~/scoop/persist/<persist>`.
    /// Otherwise, resolves `target` with `~` expansion.
    pub fn resolve_target(&self) -> Result<PathBuf> {
        if !self.persist.is_empty() {
            let home = dirs::home_dir().context("Could not determine home directory")?;
            Ok(home.join("scoop").join("persist").join(&self.persist))
        } else if !self.target.is_empty() {
            Config::resolve_target(&self.target)
        } else {
            anyhow::bail!(
                "Dotfile entry must have either 'target' or 'persist' set for source: {}",
                self.source
            );
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