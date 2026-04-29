use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::config::{DotfileBehavior, DotfileEntry, DotfileType, MergeFormat};

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

/// Prompt the user to choose a conflict resolution.
fn prompt_conflict(label: &str) -> Result<Option<ConflictAction>> {
    print!(
        "  {} Target exists: {} — [d]elete / [b]ackup / [s]kip? ",
        "?".yellow(),
        label,
    );
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    match input.trim().to_lowercase().as_str() {
        "d" | "delete" => Ok(Some(ConflictAction::Delete)),
        "b" | "backup" => Ok(Some(ConflictAction::Backup)),
        _ => Ok(None),
    }
}

/// Resolve the conflict action. Returns `None` if the user chooses to skip.
fn resolve_conflict(action: &ConflictAction, label: &str) -> Result<Option<ConflictAction>> {
    match action {
        ConflictAction::Prompt => prompt_conflict(label),
        other => Ok(Some(*other)),
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
fn backup_path(target: &Path) -> PathBuf {
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
    if a_meta.len() != b_meta.len() {
        return Ok(false);
    }
    let a_content = fs::read(a).with_context(|| format!("Failed to read: {}", a.display()))?;
    let b_content = fs::read(b).with_context(|| format!("Failed to read: {}", b.display()))?;
    Ok(a_content == b_content)
}

/// Resolve conflict and apply delete or backup to the target. Returns `Ok(true)` if
/// the conflict was resolved (target removed/backed up), `Ok(false)` if skipped.
fn resolve_and_apply(
    conflict: &ConflictAction,
    target: &Path,
    label: &str,
) -> Result<bool> {
    match resolve_conflict(conflict, label)? {
        Some(ConflictAction::Delete) => {
            fs::remove_file(target)
                .with_context(|| format!("Failed to delete: {}", target.display()))?;
            Ok(true)
        }
        Some(ConflictAction::Backup) => {
            let backup = backup_path(target);
            fs::rename(target, &backup)
                .with_context(|| format!("Failed to backup: {}", target.display()))?;
            Ok(true)
        }
        Some(ConflictAction::Prompt) => unreachable!(),
        None => {
            println!("  {} Skipped: {}", "○".dimmed(), label);
            Ok(false)
        }
    }
}

/// Deep-merge two JSON values. Source values overwrite target values for conflicting keys;
/// nested objects are merged recursively; arrays are replaced by source.
fn deep_merge_json(target: serde_json::Value, source: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match (target, source) {
        (Value::Object(mut t_map), Value::Object(s_map)) => {
            for (key, s_val) in s_map {
                match t_map.remove(&key) {
                    Some(t_val) => { t_map.insert(key, deep_merge_json(t_val, s_val)); }
                    None => { t_map.insert(key, s_val); }
                }
            }
            Value::Object(t_map)
        }
        (_target, source) => source,
    }
}

/// Deep-merge two YAML values. Same semantics as JSON merge.
fn deep_merge_yaml(target: serde_yaml::Value, source: serde_yaml::Value) -> serde_yaml::Value {
    use serde_yaml::Value;
    match (target, source) {
        (Value::Mapping(mut t_map), Value::Mapping(s_map)) => {
            for (key, s_val) in s_map {
                let t_val = t_map.remove(&key);
                let merged = match t_val {
                    Some(t) => deep_merge_yaml(t, s_val),
                    None => s_val,
                };
                t_map.insert(key, merged);
            }
            Value::Mapping(t_map)
        }
        (_target, source) => source,
    }
}

/// Deep-merge two git config files. For sections that exist in both, source keys
/// overwrite target keys; keys only in target are preserved. Sections only in
/// source are appended to target.
fn deep_merge_gitconfig(
    target_str: &str,
    source_str: &str,
) -> Result<String> {
    use gix_config::File;
    use std::str::FromStr;

    let mut target = File::from_str(target_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse target gitconfig: {}", e))?;
    let source = File::from_str(source_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse source gitconfig: {}", e))?;

    // Collect all source sections with owned data
    let source_sections: Vec<(String, Option<Vec<u8>>, Vec<(String, Vec<u8>)>)> = source
        .sections_and_ids()
        .map(|(section, _id)| {
            let header = section.header();
            let name = header.name().to_string();
            let subsection = header.subsection_name().map(|s| s.to_vec());
            let kvs: Vec<(String, Vec<u8>)> = section
                .value_names()
                .map(|vn| {
                    let key = vn.as_ref().to_string();
                    let value = section.value(vn.as_ref())
                        .map(|v| v.to_vec())
                        .unwrap_or_default();
                    (key, value)
                })
                .collect();
            (name, subsection, kvs)
        })
        .collect();

    for (section_name, subsection, kvs) in &source_sections {
        // Check if target has a matching section
        let target_has_section = target
            .sections_by_name_and_filter(section_name.as_str(), |_| true)
            .map(|mut iter| {
                iter.any(|section| {
                    let sec_sub = section.header().subsection_name();
                    match (subsection.as_ref(), sec_sub) {
                        (Some(want), Some(have)) => {
                            want.as_slice() == <bstr::BStr as AsRef<[u8]>>::as_ref(have)
                        }
                        (None, None) => true,
                        _ => false,
                    }
                })
            })
            .unwrap_or(false);

        if !target_has_section {
            let sub_cow = subsection.clone().map(|s| bstr::BString::from(s).into());
            let _new_section = target
                .new_section(section_name.clone(), sub_cow)
                .map_err(|e| anyhow::anyhow!("Failed to create section: {}", e))?;
        }

        // Set all key-value pairs using owned data
        let sub_bstr = subsection.as_ref().map(|s| bstr::BStr::new(s.as_slice()));
        for (key, value) in kvs {
            let value_bstr = bstr::BStr::new(value.as_slice());
            let _ = target.set_raw_value_by(
                section_name.clone(),
                sub_bstr,
                key.clone(),
                value_bstr,
            );
        }
    }

    Ok(target.to_string())
}

/// Sync a single dotfile entry.
fn sync_one(entry: &DotfileEntry, base_dir: &Path, dry_run: bool, conflict: &ConflictAction) -> Result<()> {
    entry.validate()?;

    let source = base_dir.join(&entry.source);
    let source = source.canonicalize()
        .with_context(|| format!("Failed to resolve source path: {}", source.display()))?;
    let target = entry.resolve_target()?;
    let label = target_label(entry);

    if !source.exists() {
        println!("  {} Source not found: {} (skipping)", "⚠".yellow(), &entry.source);
        return Ok(());
    }

    if !dry_run {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
    }

    match entry.behavior {
        DotfileBehavior::Copy => sync_copy(&source, &target, &label, &entry.source, dry_run, conflict),
        DotfileBehavior::Merge => sync_merge(&source, &target, &label, &entry.source, dry_run, entry.format.unwrap()),
    }
}

/// Sync via file copy.
fn sync_copy(
    source: &Path,
    target: &Path,
    label: &str,
    source_name: &str,
    dry_run: bool,
    conflict: &ConflictAction,
) -> Result<()> {
    // Target doesn't exist → simple copy
    if !target.exists() {
        if dry_run {
            println!("  {} Would copy: {} ← {}", "→".cyan(), label, source_name);
        } else {
            fs::copy(source, target)
                .with_context(|| format!("Failed to copy: {} → {}", source_name, label))?;
            println!("  {} Copied: {} ← {}", "→".cyan(), label, source_name);
        }
        return Ok(());
    }

    // Content matches → skip
    if files_equal(source, target)? {
        println!("  {} Already up to date: {}", "✓".green(), label);
        return Ok(());
    }

    // Content differs → resolve conflict
    if dry_run {
        println!("  {} Would overwrite: {} (content differs)", "→".cyan(), label);
    } else if resolve_and_apply(conflict, target, label)? {
        fs::copy(source, target)
            .with_context(|| format!("Failed to copy: {} → {}", source_name, label))?;
        println!("  {} Copied: {} ← {}", "→".cyan(), label, source_name);
    }

    Ok(())
}

/// Sync via deep merge (JSON or YAML).
fn sync_merge(
    source: &Path,
    target: &Path,
    label: &str,
    source_name: &str,
    dry_run: bool,
    format: MergeFormat,
) -> Result<()> {
    // Target doesn't exist → simple copy
    if !target.exists() {
        if dry_run {
            println!("  {} Would copy (new): {} ← {}", "→".cyan(), label, source_name);
        } else {
            fs::copy(source, target)
                .with_context(|| format!("Failed to copy: {} → {}", source_name, label))?;
            println!("  {} Copied (new): {} ← {}", "→".cyan(), label, source_name);
        }
        return Ok(());
    }

    // Read source content
    let source_content = fs::read_to_string(source)
        .with_context(|| format!("Failed to read source: {}", source.display()))?;

    // Read target content
    let target_content = fs::read_to_string(target)
        .with_context(|| format!("Failed to read target: {}", target.display()))?;

    // Compute merged result
    let merged = match format {
        MergeFormat::Json => {
            let src: serde_json::Value = serde_json::from_str(&source_content)
                .with_context(|| format!("Failed to parse source JSON: {}", source.display()))?;
            let tgt: serde_json::Value = serde_json::from_str(&target_content)
                .with_context(|| format!("Failed to parse target JSON: {}", target.display()))?;
            let result = deep_merge_json(tgt, src);
            serde_json::to_string_pretty(&result)?
        }
        MergeFormat::Yaml => {
            let src: serde_yaml::Value = serde_yaml::from_str(&source_content)
                .with_context(|| format!("Failed to parse source YAML: {}", source.display()))?;
            let tgt: serde_yaml::Value = serde_yaml::from_str(&target_content)
                .with_context(|| format!("Failed to parse target YAML: {}", target.display()))?;
            let result = deep_merge_yaml(tgt, src);
            serde_yaml::to_string(&result)?
        }
        MergeFormat::GitConfig => {
            deep_merge_gitconfig(&target_content, &source_content)
                .with_context(|| format!("Failed to merge gitconfig: {} ← {}", target.display(), source.display()))?
        }
    };

    // Compare with current target
    if target_content == merged {
        println!("  {} Already up to date: {}", "✓".green(), label);
        return Ok(());
    }

    if dry_run {
        println!("  {} Would merge: {} ← {}", "→".cyan(), label, source_name);
    } else {
        fs::write(target, &merged)
            .with_context(|| format!("Failed to write merged result: {}", target.display()))?;
        println!("  {} Merged: {} ← {}", "→".cyan(), label, source_name);
    }

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
            DotfileBehavior::Merge => {
                if target.exists() {
                    println!("  {} {} (merge target exists)", "✓".green(), label);
                } else {
                    println!("  {} {} (not synced)", "○".dimmed(), label);
                }
            }
        }
    }
    Ok(())
}
