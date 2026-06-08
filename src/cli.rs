use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::io;

use crate::manifest::ResolvedManifest;
use crate::plan::InstallPlan;

#[derive(Parser)]
#[command(author, version, about = "Arx package manager CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a manifest file
    Validate {
        /// Path to the manifest file
        manifest: PathBuf,
    },
    /// Check manifest and show detailed removal analysis
    Check {
        /// Path to the manifest file
        manifest: PathBuf,
    },
    /// Generate an install/prune plan from a manifest
    Plan {
        /// Path to the manifest file
        manifest: PathBuf,
    },
    /// Apply the manifest: install missing packages and optionally prune undeclared ones
    Apply {
        /// Path to the manifest file
        manifest: PathBuf,
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Validate { manifest } => {
            let resolved = ResolvedManifest::load(&manifest)
                .with_context(|| format!("failed to load manifest {manifest:?}"))?;
            resolved.validate().with_context(|| format!("manifest {manifest:?} is invalid"))?;
            println!("Manifest {manifest:?} is valid.");
        }
        Command::Check { manifest } => {
            let resolved = ResolvedManifest::load(&manifest)
                .with_context(|| format!("failed to load manifest {manifest:?}"))?;
            resolved.validate().with_context(|| format!("manifest {manifest:?} is invalid"))?;
            let plan = InstallPlan::from_manifest(&resolved)
                .with_context(|| "failed to generate package plan using ALPM")?;
            plan.print_summary();
            plan.print_removal_details()
                .with_context(|| "failed to analyze removal dependencies")?;
        }
        Command::Plan { manifest } => {
            let resolved = ResolvedManifest::load(&manifest)
                .with_context(|| format!("failed to load manifest {manifest:?}"))?;
            resolved.validate().with_context(|| format!("manifest {manifest:?} is invalid"))?;
            let plan = InstallPlan::from_manifest(&resolved)
                .with_context(|| "failed to generate package plan using ALPM")?;
            plan.print_summary();
        }
        Command::Apply { manifest, yes } => {
            let resolved = ResolvedManifest::load(&manifest)
                .with_context(|| format!("failed to load manifest {manifest:?}"))?;
            resolved.validate().with_context(|| format!("manifest {manifest:?} is invalid"))?;
            let plan = InstallPlan::from_manifest(&resolved)
                .with_context(|| "failed to generate package plan using ALPM")?;

            if plan.is_empty() {
                println!("No changes needed. System is up to date.");
                return Ok(());
            }

            plan.print_summary();
            plan.print_removal_details()
                .with_context(|| "failed to analyze removal dependencies")?;

            if !yes {
                eprintln!("\nProceed with these changes? [y/N]");
                let mut input = String::new();
                io::stdin().read_line(&mut input).context("failed to read user input")?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    eprintln!("Aborted.");
                    return Ok(());
                }
            }

            plan.apply().with_context(|| "failed to apply package changes")
                .or_else(|e| {
                    // Check if this is a permission error and provide a hint
                    let msg = format!("{:?}", e);
                    if msg.contains("root") || msg.contains("permission") {
                        Err(anyhow!("{}\n\nTip: Package operations require root. Try: sudo arx apply {}", e, manifest.display()))
                    } else {
                        Err(e)
                    }
                })?;
            println!("\nPackage changes applied successfully.");
        }
    }

    Ok(())
}
