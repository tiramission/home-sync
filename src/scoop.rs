use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::io::{self, Write};
use std::process::Command;

use crate::config::{BucketEntry, PackageEntry, ScoopConfig};

/// Run a scoop command via `cmd /c scoop ...` for Windows compatibility.
fn scoop_cmd() -> Command {
    let mut cmd = Command::new("cmd");
    cmd.args(["/c", "scoop"]);
    cmd
}

/// Ensure Scoop is installed on the system.
pub fn ensure_scoop_installed() -> Result<()> {
    if which::which("scoop").is_ok() {
        return Ok(());
    }

    println!(
        "{}",
        "Scoop is not installed. Installing Scoop...".yellow()
    );

    // Run the official Scoop installer via PowerShell
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "iwr -useb get.scoop.sh | iex",
        ])
        .status()
        .context("Failed to run Scoop installer")?;

    if !status.success() {
        bail!("Scoop installation failed");
    }

    println!("{}", "Scoop installed successfully.".green());
    Ok(())
}

/// Strip ANSI escape codes from a string.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
            continue;
        }
        result.push(ch);
    }
    result
}

/// Get the list of currently installed Scoop bucket names.
fn get_installed_buckets() -> Result<Vec<String>> {
    let output = scoop_cmd()
        .args(["bucket", "list"])
        .output()
        .context("Failed to run `scoop bucket list`")?;

    let list = String::from_utf8_lossy(&output.stdout);
    let names: Vec<String> = list
        .lines()
        .map(|line| strip_ansi(line))
        .filter(|line| {
            // Skip empty lines, header line, and separator line
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("Name")
                && !trimmed.starts_with("----")
        })
        .filter_map(|line| {
            line.split_whitespace()
                .next()
                .map(|s| s.to_string())
        })
        .collect();
    Ok(names)
}

/// Prompt the user for confirmation (y/n). Returns true if confirmed.
fn confirm(prompt: &str) -> Result<bool> {
    print!("{}", prompt);
    io::stdout().flush().context("Failed to flush stdout")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;
    let answer = input.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Sync Scoop buckets declaratively: add missing, remove extra (with confirmation).
pub fn sync_buckets(buckets: &[BucketEntry], dry_run: bool) -> Result<()> {
    let declared_names: Vec<&str> = buckets.iter().map(|b| b.name()).collect();
    let installed = get_installed_buckets()?;

    // --- Add missing buckets ---
    for bucket in buckets {
        let name = bucket.name();
        let source = bucket.source();

        if installed.iter().any(|n| n == name) {
            if !dry_run {
                println!("  {} Bucket '{}' already added", "✓".green(), name);
            }
            continue;
        }

        if dry_run {
            match source {
                Some(src) => println!(
                    "  {} Would add bucket '{}' from '{}'",
                    "→".cyan(), name, src
                ),
                None => println!(
                    "  {} Would add bucket '{}'",
                    "→".cyan(), name
                ),
            }
            continue;
        }

        // Build the `scoop bucket add` command
        let mut args = vec!["bucket", "add", name];
        if let Some(src) = source {
            args.push(src);
        }

        println!("  {} Adding bucket '{}'...", "→".cyan(), name);
        let status = scoop_cmd()
            .args(&args)
            .status()
            .with_context(|| format!("Failed to add bucket '{}'", name))?;

        if !status.success() {
            println!(
                "  {} Failed to add bucket '{}' (may already exist)",
                "⚠".yellow(),
                name
            );
        }
    }

    // --- Remove extra buckets ---
    let extra: Vec<&String> = installed
        .iter()
        .filter(|name| !declared_names.iter().any(|d| *d == name.as_str()))
        .collect();

    if !extra.is_empty() {
        if dry_run {
            for name in &extra {
                println!(
                    "  {} Would remove undeclared bucket '{}'",
                    "→".red(), name
                );
            }
        } else {
            // Ask for confirmation before removing
            let names: Vec<&str> = extra.iter().map(|s| s.as_str()).collect();
            let prompt = format!(
                "\n  {} The following buckets will be removed: {}\n  Do you want to proceed? (y/n): ",
                "⚠".yellow(),
                names.join(", ")
            );
            if confirm(&prompt)? {
                for name in &extra {
                    println!("  {} Removing bucket '{}'...", "→".red(), name);
                    let status = scoop_cmd()
                        .args(["bucket", "rm", name])
                        .status()
                        .with_context(|| format!("Failed to remove bucket '{}'", name))?;

                    if !status.success() {
                        println!(
                            "  {} Failed to remove bucket '{}'",
                            "✗".red(),
                            name
                        );
                    }
                }
            } else {
                println!("  {} Skipped removing extra buckets.", "⚠".yellow());
            }
        }
    }

    Ok(())
}

/// Get the list of currently installed Scoop package names.
fn get_installed_packages() -> Result<Vec<String>> {
    let output = scoop_cmd()
        .args(["list"])
        .output()
        .context("Failed to run `scoop list`")?;

    let list = String::from_utf8_lossy(&output.stdout);
    let names: Vec<String> = list
        .lines()
        .map(|line| strip_ansi(line))
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !trimmed.starts_with("Installed")
                && !trimmed.starts_with("Name")
                && !trimmed.starts_with("----")
        })
        .filter_map(|line| {
            line.split_whitespace()
                .next()
                .map(|s| s.to_string())
        })
        .collect();
    Ok(names)
}

/// Install all declared Scoop packages (batch comparison approach).
pub fn sync_packages(packages: &[PackageEntry], dry_run: bool) -> Result<()> {
    let installed = get_installed_packages()?;

    let mut to_install: Vec<&PackageEntry> = Vec::new();
    let mut already_installed = 0;

    for package in packages {
        let name = package.name();
        if installed.iter().any(|n| n == name) {
            already_installed += 1;
        } else {
            to_install.push(package);
        }
    }

    // Report already installed
    if already_installed > 0 {
        println!(
            "  {} {} package(s) already installed",
            "✓".green(),
            already_installed
        );
    }

    // --- Install missing packages ---
    if !to_install.is_empty() {
        if dry_run {
            for package in &to_install {
                match package.bucket() {
                    Some(bucket) => println!(
                        "  {} Would install '{}' from bucket '{}'",
                        "→".cyan(), package.name(), bucket
                    ),
                    None => println!(
                        "  {} Would install '{}'",
                        "→".cyan(), package.name()
                    ),
                }
            }
        } else {
            println!(
                "  {} {} package(s) to install:",
                "→".cyan(),
                to_install.len()
            );

            for package in &to_install {
                let spec = package.install_spec();
                println!("    {} Installing '{}'...", "→".cyan(), spec);
                let status = scoop_cmd()
                    .args(["install", &spec])
                    .status()
                    .with_context(|| format!("Failed to install '{}'", spec))?;

                if !status.success() {
                    println!("    {} Failed to install '{}'", "✗".red(), spec);
                }
            }
        }
    }

    // --- Remove undeclared packages ---
    let declared_names: Vec<&str> = packages.iter().map(|p| p.name()).collect();
    let extra: Vec<&String> = installed
        .iter()
        .filter(|name| !declared_names.iter().any(|d| *d == name.as_str()))
        .collect();

    if !extra.is_empty() {
        if dry_run {
            for name in &extra {
                println!(
                    "  {} Would purge undeclared package '{}'",
                    "→".red(), name
                );
            }
        } else {
            let names: Vec<&str> = extra.iter().map(|s| s.as_str()).collect();
            let prompt = format!(
                "\n  {} The following packages will be purged: {}\n  Do you want to proceed? (y/n): ",
                "⚠".yellow(),
                names.join(", ")
            );
            if confirm(&prompt)? {
                for name in &extra {
                    println!("  {} Purging '{}'...", "→".red(), name);
                    let status = scoop_cmd()
                        .args(["uninstall", "--purge", name])
                        .status()
                        .with_context(|| format!("Failed to purge '{}'", name))?;

                    if !status.success() {
                        println!("    {} Failed to purge '{}'", "✗".red(), name);
                    }
                }
            } else {
                println!("  {} Skipped purging extra packages.", "⚠".yellow());
            }
        }
    }

    Ok(())
}

/// Run the full Scoop sync: ensure installed → buckets → packages.
pub fn sync(config: &ScoopConfig, dry_run: bool) -> Result<()> {
    if !dry_run {
        ensure_scoop_installed()?;
    } else {
        let scoop_available = which::which("scoop").is_ok();
        if scoop_available {
            println!("  {} Scoop is installed", "✓".green());
        } else {
            println!("  {} Scoop is not installed (would install)", "⚠".yellow());
        }
    }

    println!("{}", "Syncing Scoop buckets...".bold());
    sync_buckets(&config.buckets, dry_run)?;

    println!("{}", "Syncing Scoop packages...".bold());
    sync_packages(&config.packages, dry_run)?;

    println!("{}", "Scoop sync complete.".green().bold());
    Ok(())
}
