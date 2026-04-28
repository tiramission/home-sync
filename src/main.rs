mod config;
mod dotfiles;
mod scoop;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// home-sync — A declarative user environment manager for Windows.
#[derive(Parser)]
#[command(name = "home-sync", version, about)]
struct Cli {
    /// Path to the configuration file (default: config.toml in current directory)
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run full sync: install Scoop packages and link dotfiles
    Sync {
        /// Only sync Scoop packages
        #[arg(long)]
        scoop_only: bool,
        /// Only sync dotfiles
        #[arg(long)]
        dotfiles_only: bool,
        /// Show what would be done without making any changes
        #[arg(long)]
        dry_run: bool,
        /// Delete existing files when conflicts occur
        #[arg(long)]
        delete: bool,
        /// Backup existing files when conflicts occur
        #[arg(long)]
        backup: bool,
    },
    /// Show the current status of the environment
    Status,
    /// Initialize a new config.toml from the example template
    Init,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => cmd_init(&cli.config),
        Commands::Status => cmd_status(&cli.config),
        Commands::Sync {
            scoop_only,
            dotfiles_only,
            dry_run,
            delete,
            backup,
        } => cmd_sync(&cli.config, SyncArgs { scoop_only, dotfiles_only, dry_run, delete, backup }),
    }
}

struct SyncArgs {
    scoop_only: bool,
    dotfiles_only: bool,
    dry_run: bool,
    delete: bool,
    backup: bool,
}

fn resolve_base_dir(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

fn cmd_init(config_path: &PathBuf) -> Result<()> {
    if config_path.exists() {
        println!("{} Config file already exists: {}", "⚠".yellow(), config_path.display());
        return Ok(());
    }

    let example = include_str!("../config.example.toml");
    std::fs::write(config_path, example)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;

    println!("{} Created config file: {}", "✓".green(), config_path.display());
    println!(
        "{}",
        "Edit config.toml to declare your Scoop packages and dotfiles, then run `home-sync sync`.".dimmed()
    );
    Ok(())
}

fn cmd_status(config_path: &PathBuf) -> Result<()> {
    let config = config::Config::load(config_path)?;
    let base_dir = resolve_base_dir(config_path);

    if let Some(ref scoop_config) = config.scoop {
        println!("{}", "Scoop status:".bold());
        if which::which("scoop").is_ok() {
            println!("  {} Scoop is installed", "✓".green());
            println!("  {} Buckets: {}", "→".cyan(), scoop_config.buckets.len());
            println!("  {} Packages: {}", "→".cyan(), scoop_config.packages.len());
        } else {
            println!("  {} Scoop is not installed", "✗".red());
        }
        println!();
    }

    dotfiles::status(&config.dotfiles, &base_dir)?;
    Ok(())
}

fn cmd_sync(config_path: &PathBuf, args: SyncArgs) -> Result<()> {
    if args.delete && args.backup {
        anyhow::bail!("Cannot specify both --delete and --backup");
    }

    let conflict = if args.delete {
        dotfiles::ConflictAction::Delete
    } else if args.backup {
        dotfiles::ConflictAction::Backup
    } else {
        dotfiles::ConflictAction::Prompt
    };

    let config = config::Config::load(config_path)?;
    let base_dir = resolve_base_dir(config_path);

    println!("{}", "╔══════════════════════════════════════╗".cyan());
    println!("{}", "║        home-sync — Environment       ║".cyan());
    println!("{}", "╚══════════════════════════════════════╝".cyan());
    if args.dry_run {
        println!("{}", "  [DRY RUN] No changes will be made.".yellow().bold());
    }
    println!();

    if !args.dotfiles_only {
        if let Some(ref scoop_config) = config.scoop {
            scoop::sync(scoop_config, args.dry_run)?;
        } else {
            println!("{}", "No Scoop configuration found, skipping.".dimmed());
        }
        println!();
    }

    if !args.scoop_only {
        dotfiles::sync(&config.dotfiles, &base_dir, args.dry_run, &conflict)?;
        println!();
    }

    println!("{}", "All done! Your environment is in sync.".green().bold());
    Ok(())
}
