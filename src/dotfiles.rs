use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::config::{DotfileBehavior, DotfileEntry, DotfileType};

/// How to handle existing target files during sync.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConflictAction {
    /// Prompt the user interactively.
    Prompt,
    /// Delete the existing file.
    Delete,
    /// Backup the existing file (rename to `.bak`).
    Backup,
}

/// Normalize a path by stripping the `\\?\` UNC prefix that Windows `canonicalize` adds.
fn normalize_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

/// Resolve the conflict action, prompting the user if needed.
fn resolve_conflict(action: &ConflictAction, label: &str) -> Result<ConflictAction> {
    match action {
        ConflictAction::Prompt => {
            print!(
                "  {} Target exists: {} — [d]elete / [b]ackup / [s]kip? ",
                "?".yellow(),
                label,
            );
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            match input.trim().to_lowercase().as_str() {
                "d" | "delete" => Ok(ConflictAction::Delete),
                "b" | "backup" => Ok(ConflictAction::Backup),
                _ => Ok(ConflictAction::Prompt), // skip
            }
        }
        other => Ok(*other),
    }
}

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
fn sync_one(entry: &DotfileEntry, base_dir: &Path, dry_run: bool, conflict: &ConflictAction) -> Result<()> {
    let source = base_dir.join(&entry.source);
    let source = source.canonicalize()
        .with_context(|| format!("Failed to resolve source path: {}", source.display()))?;
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
        DotfileBehavior::Copy => sync_copy(&source, &target, &label, &entry.source, dry_run, conflict),
        DotfileBehavior::Link => sync_symlink(&source, &target, &label, &entry.source, dry_run, conflict),
    }
}

/// Sync via file copy (for `type = "link"`).
fn sync_copy(
    source: &Path,
    target: &Path,
    label: &str,
    source_name: &str,
    dry_run: bool,
    conflict: &ConflictAction,
) -> Result<()> {
    if target.exists() {
        // If target is a symlink, always replace it
        if target.is_symlink() {
            let backup = backup_path(target);
            if dry_run {
                println!(
                    "  {} Would replace symlink: {} → {}",
                    "⚠".yellow(),
                    label,
                    backup.display()
                );
                println!(
                    "  {} Would copy: {} ← {}",
                    "→".cyan(),
                    label,
                    source_name,
                );
            } else {
                let action = resolve_conflict(conflict, label)?;
                match action {
                    ConflictAction::Delete => {
                        fs::remove_file(target)
                            .with_context(|| format!("Failed to delete: {}", target.display()))?;
                        fs::copy(source, target)
                            .with_context(|| format!("Failed to copy: {} → {}", source_name, label))?;
                        println!("  {} Deleted & copied: {} ← {}", "→".cyan(), label, source_name);
                    }
                    ConflictAction::Backup => {
                        fs::rename(target, &backup)
                            .with_context(|| format!("Failed to backup: {}", target.display()))?;
                        fs::copy(source, target)
                            .with_context(|| format!("Failed to copy: {} → {}", source_name, label))?;
                        println!("  {} Backed up & copied: {} ← {}", "→".cyan(), label, source_name);
                    }
                    ConflictAction::Prompt => {
                        println!("  {} Skipped: {}", "○".dimmed(), label);
                    }
                }
            }
            return Ok(());
        }

        // Content matches → skip
        if files_equal(source, target)? {
            println!(
                "  {} Already up to date: {}",
                "✓".green(),
                label,
            );
            return Ok(());
        }

        // Content differs → resolve conflict
        if dry_run {
            let backup = backup_path(target);
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
            let action = resolve_conflict(conflict, label)?;
            match action {
                ConflictAction::Delete => {
                    fs::remove_file(target)
                        .with_context(|| format!("Failed to delete: {}", target.display()))?;
                    fs::copy(source, target)
                        .with_context(|| format!("Failed to copy: {} → {}", source_name, label))?;
                    println!("  {} Deleted & copied: {} ← {}", "→".cyan(), label, source_name);
                }
                ConflictAction::Backup => {
                    let backup = backup_path(target);
                    fs::rename(target, &backup)
                        .with_context(|| format!("Failed to backup: {}", target.display()))?;
                    fs::copy(source, target)
                        .with_context(|| format!("Failed to copy: {} → {}", source_name, label))?;
                    println!("  {} Backed up & copied: {} ← {}", "→".cyan(), label, source_name);
                }
                ConflictAction::Prompt => {
                    println!("  {} Skipped: {}", "○".dimmed(), label);
                }
            }
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
    conflict: &ConflictAction,
) -> Result<()> {
    // If target is already a symlink pointing to the correct source
    if target.is_symlink() {
        if let Ok(existing) = fs::read_link(target) {
            if normalize_path(&existing) == normalize_path(source) {
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

    // Target exists but is not the correct symlink — resolve conflict
    if target.exists() || target.is_symlink() {
        if dry_run {
            let backup = backup_path(target);
            println!(
                "  {} Would back up existing: {} → {}",
                "⚠".yellow(),
                label,
                backup.display()
            );
        } else {
            let action = resolve_conflict(conflict, label)?;
            match action {
                ConflictAction::Delete => {
                    if target.is_symlink() {
                        fs::remove_file(target)
                            .with_context(|| format!("Failed to delete: {}", target.display()))?;
                    } else {
                        fs::remove_file(target)
                            .with_context(|| format!("Failed to delete: {}", target.display()))?;
                    }
                }
                ConflictAction::Backup => {
                    let backup = backup_path(target);
                    fs::rename(target, &backup)
                        .with_context(|| format!("Failed to backup: {}", target.display()))?;
                }
                ConflictAction::Prompt => {
                    println!("  {} Skipped: {}", "○".dimmed(), label);
                    return Ok(());
                }
            }
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
pub fn sync(dotfiles: &[DotfileEntry], base_dir: &Path, dry_run: bool, conflict: &ConflictAction) -> Result<()> {
    if dotfiles.is_empty() {
        println!("{}", "No dotfiles declared in config.".dimmed());
        return Ok(());
    }

    println!("{}", "Syncing dotfiles...".bold());
    for entry in dotfiles {
        sync_one(entry, base_dir, dry_run, conflict)?;
    }
    println!("{}", "Dotfiles sync complete.".green().bold());
    Ok(())
}

/// Show the current status of all declared dotfiles.
pub fn status(dotfiles: &[DotfileEntry], base_dir: &Path) -> Result<()> {
    println!("{}", "Dotfile status:".bold());
    for entry in dotfiles {
        let source = base_dir.join(&entry.source);
        let source = source.canonicalize()
            .with_context(|| format!("Failed to resolve source path: {}", source.display()))?;
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
                        if normalize_path(&existing) == normalize_path(&source) {
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
