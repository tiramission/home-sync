use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::config::{DotfileBehavior, DotfileEntry, DotfileType};

/// Get a display label for the target.
fn target_label(entry: &DotfileEntry) -> String {
    match entry.dotfile_type {
        DotfileType::Persist => format!("persist:{}", entry.target),
        DotfileType::File => entry.target.clone(),
    }
}

/// Generate a backup path for the given target.
/// e.g. `file.ini` → `file.ini.bak`, `file` → `file.bak`
fn backup_path(target: &Path) -> std::path::PathBuf {
    let mut name = target
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    name.push_str(".bak");
    target.with_file_name(name)
}

/// Check if two files have the same content.
fn files_equal(a: &Path, b: &Path) -> Result<bool> {
    let a_meta = fs::metadata(a)
        .with_context(|| format!("Failed to read metadata: {}", a.display()))?;
    let b_meta = fs::metadata(b)
        .with_context(|| format!("Failed to read metadata: {}", b.display()))?;
    // Quick check: different sizes means different content
    if a_meta.len() != b_meta.len() {
        return Ok(false);
    }
    let a_content = fs::read(a).with_context(|| format!("Failed to read: {}", a.display()))?;
    let b_content = fs::read(b).with_context(|| format!("Failed to read: {}", b.display()))?;
    Ok(a_content == b_content)
}

/// Sync a single dotfile entry.
/// - `link` type: copy source → target (overwrite if content differs, backup existing)
/// - `persist` type: symlink source → target
fn sync_one(entry: &DotfileEntry, base_dir: &Path, dry_run: bool) -> Result<()> {
    let source = base_dir.join(&entry.source);
    let target = entry.resolve_target()?;
    let label = target_label(entry);

    // Ensure the source file exists
    if !source.exists() {
        println!(
            "  {} Source not found: {} (skipping)",
            "⚠".yellow(),
            &entry.source
        );
        return Ok(());
    }

    // Create parent directory for target if needed
    if !dry_run {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
    }

    match entry.behavior {
        DotfileBehavior::Copy => sync_copy(&source, &target, &label, &entry.source, dry_run),
        DotfileBehavior::Link => sync_symlink(&source, &target, &label, &entry.source, dry_run),
    }
}

/// Sync via file copy (for `type = "link"`).
fn sync_copy(
    source: &Path,
    target: &Path,
    label: &str,
    source_name: &str,
    dry_run: bool,
) -> Result<()> {
    if target.exists() {
        // Content matches → skip
        if files_equal(source, target)? {
            println!(
                "  {} Already up to date: {}",
                "✓".green(),
                label,
            );
            return Ok(());
        }

        // Content differs → backup and overwrite
        let backup = backup_path(target);
        if dry_run {
            println!(
                "  {} Would back up: {} → {}",
                "⚠".yellow(),
                label,
                backup.display()
            );
            println!(
                "  {} Would overwrite: {} (content differs)",
                "→".cyan(),
                label,
            );
        } else {
            println!(
                "  {} Backing up: {} → {}",
                "⚠".yellow(),
                label,
                backup.display()
            );
            fs::rename(target, &backup)
                .with_context(|| format!("Failed to backup: {}", target.display()))?;
            fs::copy(source, target)
                .with_context(|| format!("Failed to copy: {} → {}", source_name, label))?;
            println!(
                "  {} Overwritten: {} ← {}",
                "→".cyan(),
                label,
                source_name,
            );
        }
    } else {
        // Target doesn't exist → copy
        if dry_run {
            println!(
                "  {} Would copy: {} ← {}",
                "→".cyan(),
                label,
                source_name,
            );
        } else {
            fs::copy(source, target)
                .with_context(|| format!("Failed to copy: {} → {}", source_name, label))?;
            println!(
                "  {} Copied: {} ← {}",
                "→".cyan(),
                label,
                source_name,
            );
        }
    }
    Ok(())
}

/// Sync via symlink (for `type = "persist"`).
fn sync_symlink(
    source: &Path,
    target: &Path,
    label: &str,
    source_name: &str,
    dry_run: bool,
) -> Result<()> {
    // If target is already a symlink pointing to the correct source
    if target.is_symlink() {
        if let Ok(existing) = fs::read_link(target) {
            if existing == source {
                println!(
                    "  {} Already linked: {} → {}",
                    "✓".green(),
                    label,
                    source_name,
                );
                return Ok(());
            }
        }
    }

    // Target exists but is not the correct symlink — back it up
    if target.exists() || target.is_symlink() {
        let backup = backup_path(target);
        if dry_run {
            println!(
                "  {} Would back up existing: {} → {}",
                "⚠".yellow(),
                label,
                backup.display()
            );
        } else {
            println!(
                "  {} Backing up existing: {} → {}",
                "⚠".yellow(),
                label,
                backup.display()
            );
            fs::rename(target, &backup)
                .with_context(|| format!("Failed to backup: {}", target.display()))?;
        }
    }

    if dry_run {
        println!(
            "  {} Would link: {} → {}",
            "→".cyan(),
            label,
            source_name,
        );
        return Ok(());
    }

    // Create the symlink
    #[cfg(windows)]
    {
        if source.is_dir() {
            std::os::windows::fs::symlink_dir(source, target)
                .with_context(|| format!("Failed to create directory symlink: {}", target.display()))?;
        } else {
            std::os::windows::fs::symlink_file(source, target)
                .with_context(|| format!("Failed to create file symlink: {}", target.display()))?;
        }
    }

    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(source, target)
            .with_context(|| format!("Failed to create symlink: {}", target.display()))?;
    }

    println!(
        "  {} Linked: {} → {}",
        "→".cyan(),
        label,
        source_name,
    );
    Ok(())
}

/// Sync all dotfiles declared in the configuration.
pub fn sync(dotfiles: &[DotfileEntry], base_dir: &Path, dry_run: bool) -> Result<()> {
    if dotfiles.is_empty() {
        println!("{}", "No dotfiles declared in config.".dimmed());
        return Ok(());
    }

    println!("{}", "Syncing dotfiles...".bold());
    for entry in dotfiles {
        sync_one(entry, base_dir, dry_run)?;
    }
    println!("{}", "Dotfiles sync complete.".green().bold());
    Ok(())
}

/// Show the current status of all declared dotfiles.
pub fn status(dotfiles: &[DotfileEntry], base_dir: &Path) -> Result<()> {
    println!("{}", "Dotfile status:".bold());
    for entry in dotfiles {
        let source = base_dir.join(&entry.source);
        let target = entry.resolve_target()?;
        let label = target_label(entry);

        if !source.exists() {
            println!("  {} {} (source missing)", "⚠".yellow(), &entry.source);
            continue;
        }

        match entry.behavior {
            DotfileBehavior::Copy => {
                if target.exists() {
                    match files_equal(&source, &target) {
                        Ok(true) => println!("  {} {} (up to date)", "✓".green(), label),
                        Ok(false) => println!("  {} {} (content differs)", "⚠".yellow(), label),
                        Err(_) => println!("  {} {} (unreadable)", "✗".red(), label),
                    }
                } else {
                    println!("  {} {} (not synced)", "○".dimmed(), label);
                }
            }
            DotfileBehavior::Link => {
                if target.is_symlink() {
                    if let Ok(existing) = fs::read_link(&target) {
                        if existing == source {
                            println!(
                                "  {} {} → {}",
                                "✓".green(),
                                label,
                                entry.source
                            );
                        } else {
                            println!(
                                "  {} {} (linked to different source: {})",
                                "✗".red(),
                                label,
                                existing.display()
                            );
                        }
                    } else {
                        println!("  {} {} (unreadable symlink)", "✗".red(), label);
                    }
                } else if target.exists() {
                    println!(
                        "  {} {} (exists but not a symlink)",
                        "⚠".yellow(),
                        label
                    );
                } else {
                    println!("  {} {} (not linked)", "○".dimmed(), label);
                }
            }
        }
    }
    Ok(())
}
