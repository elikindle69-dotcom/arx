use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
    /// Generate an install/prune plan from a manifest
    Plan {
        /// Path to the manifest file
        manifest: PathBuf,
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
        Command::Plan { manifest } => {
            let resolved = ResolvedManifest::load(&manifest)
                .with_context(|| format!("failed to load manifest {manifest:?}"))?;
            resolved.validate().with_context(|| format!("manifest {manifest:?} is invalid"))?;
            let plan = InstallPlan::from_manifest(&resolved)
                .with_context(|| "failed to generate package plan using ALPM")?;
            plan.print_summary();
        }
    }

    Ok(())
}
