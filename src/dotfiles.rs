use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::config::{DotfileEntry, DotfileType};

/// Get a display label for the target.
fn target_label(entry: &DotfileEntry) -> String {
    match entry.dotfile_type {
        DotfileType::Persist => format!("persist:{}", entry.target),
        DotfileType::Link => entry.target.clone(),
    }
}

/// Sync a single dotfile: create parent dirs and symlink source → target.
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

    // If target already exists
    if target.exists() || target.is_symlink() {
        // Check if it's already a symlink pointing to the correct source
        if target.is_symlink() {
            if let Ok(existing) = fs::read_link(&target) {
                if existing == source {
                    println!(
                        "  {} Already linked: {} → {}",
                        "✓".green(),
                        label,
                        entry.source
                    );
                    return Ok(());
                }
            }
        }

        // Target exists but is not the correct symlink — back it up
        let backup = target.with_extension(format!(
            "{}.bak",
            target
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default()
        ));
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
            fs::rename(&target, &backup)
                .with_context(|| format!("Failed to backup: {}", target.display()))?;
        }
    }

    if dry_run {
        println!(
            "  {} Would link: {} → {}",
            "→".cyan(),
            label,
            entry.source
        );
        return Ok(());
    }

    // Create the symlink
    #[cfg(windows)]
    {
        // On Windows, use directory symlink for dirs, file symlink for files
        if source.is_dir() {
            std::os::windows::fs::symlink_dir(&source, &target)
                .with_context(|| format!("Failed to create directory symlink: {}", target.display()))?;
        } else {
            std::os::windows::fs::symlink_file(&source, &target)
                .with_context(|| format!("Failed to create file symlink: {}", target.display()))?;
        }
    }

    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(&source, &target)
            .with_context(|| format!("Failed to create symlink: {}", target.display()))?;
    }

    println!(
        "  {} Linked: {} → {}",
        "→".cyan(),
        label,
        entry.source
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
    Ok(())
}