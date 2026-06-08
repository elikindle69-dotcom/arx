use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::ResolvedManifest;
use crate::plan::{InstallPlan, execute_plan, init_alpm, installed_package_names};
use crate::pacman_conf::PacmanConfig;

#[derive(Parser)]
#[command(author, version, about = "Arx declarative package manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Override the root filesystem path (default: /)
    #[arg(long, global = true, default_value = "/")]
    root: PathBuf,

    /// Override the pacman database path (default: /var/lib/pacman)
    #[arg(long = "dbpath", global = true, default_value = "/var/lib/pacman")]
    db_path: PathBuf,
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

        /// Output the plan as JSON instead of a human-readable summary
        #[arg(long)]
        json: bool,
    },
    /// Apply a manifest: install missing packages and optionally remove undeclared ones
    Apply {
        /// Path to the manifest file
        manifest: PathBuf,

        /// Only show what would be done, don't make changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Save the current system state as a JSONC manifest
    SaveCurrent {
        /// Output file path
        output: PathBuf,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Validate { manifest } => {
            let resolved = ResolvedManifest::load(&manifest)
                .with_context(|| format!("failed to load manifest {}", manifest.display()))?;
            resolved.validate().with_context(|| format!("manifest {} is invalid", manifest.display()))?;
            println!("Manifest {} is valid.", manifest.display());
        }
        Command::Plan { manifest, json } => {
            let resolved = ResolvedManifest::load(&manifest)
                .with_context(|| format!("failed to load manifest {}", manifest.display()))?;
            resolved.validate().with_context(|| format!("manifest {} is invalid", manifest.display()))?;
            let conf = PacmanConfig::load_default()
                .unwrap_or_else(|_| PacmanConfig {
                    root_dir: cli.root.clone(),
                    db_path: cli.db_path.clone(),
                    repos: vec![],
                });
            let alpm = init_alpm(&conf)?;
            let plan = InstallPlan::from_manifest(&resolved, &alpm)
                .with_context(|| "failed to generate package plan")?;
            if json {
                let summary = plan.to_summary();
                println!("{}", serde_json::to_string_pretty(&summary)
                    .with_context(|| "failed to serialize plan to JSON")?);
            } else {
                plan.print_summary();
            }
        }
        Command::Apply { manifest, dry_run } => {
            let resolved = ResolvedManifest::load(&manifest)
                .with_context(|| format!("failed to load manifest {}", manifest.display()))?;
            resolved.validate().with_context(|| format!("manifest {} is invalid", manifest.display()))?;
            let conf = PacmanConfig::load_default()
                .unwrap_or_else(|_| PacmanConfig {
                    root_dir: cli.root.clone(),
                    db_path: cli.db_path.clone(),
                    repos: vec![],
                });
            let mut alpm = init_alpm(&conf)?;
            let plan = InstallPlan::from_manifest(&resolved, &alpm)
                .with_context(|| "failed to generate package plan")?;
            execute_plan(&plan, &mut alpm, dry_run)
                .with_context(|| "failed to execute apply plan")?;
        }
        Command::SaveCurrent { output } => {
            save_current(&cli.root, &cli.db_path, &output)?;
        }
    }

    Ok(())
}

fn save_current(root: &Path, db_path: &Path, output: &Path) -> Result<()> {
    let conf = PacmanConfig::load_default()
        .unwrap_or_else(|_| PacmanConfig {
            root_dir: root.to_path_buf(),
            db_path: db_path.to_path_buf(),
            repos: vec![],
        });

    let alpm = init_alpm(&conf)?;
    let installed = installed_package_names(&alpm)?;

    let mut core_pkgs: Vec<String> = Vec::new();
    let mut extra_pkgs: Vec<String> = Vec::new();
    let mut other_pkgs: Vec<(String, String)> = Vec::new(); // (name, repo)

    for name in &installed {
        let mut found_repo = None;
        for db in alpm.syncdbs().iter() {
            if db.pkg(name.as_str()).is_ok() {
                found_repo = Some(db.name().to_string());
                break;
            }
        }
        match found_repo.as_deref() {
            Some("core") => core_pkgs.push(name.clone()),
            Some("extra") => extra_pkgs.push(name.clone()),
            Some(repo) => other_pkgs.push((name.clone(), repo.to_string())),
            None => other_pkgs.push((name.clone(), "extra".to_string())),
        }
    }

    core_pkgs.sort();
    extra_pkgs.sort();
    other_pkgs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut lines: Vec<String> = Vec::new();
    lines.push("{".to_string());
    lines.push("    // Arx system manifest - generated by arx save-current".to_string());
    lines.push(format!("    // Core packages ({})", core_pkgs.len()));
    lines.push(format!("    // Extra packages ({})", extra_pkgs.len()));
    if !other_pkgs.is_empty() {
        lines.push(format!("    // Other packages ({})", other_pkgs.len()));
    }
    lines.push("    \"packages\": [".to_string());

    for (i, pkg) in core_pkgs.iter().enumerate() {
        let comma = if i < core_pkgs.len() - 1 || !extra_pkgs.is_empty() || !other_pkgs.is_empty() { "," } else { "" };
        lines.push(format!("        {{ \"name\": \"{}\", \"source\": \"core\" }}{}", pkg, comma));
    }

    for (i, pkg) in extra_pkgs.iter().enumerate() {
        let comma = if i < extra_pkgs.len() - 1 || !other_pkgs.is_empty() { "," } else { "" };
        lines.push(format!("        {{ \"name\": \"{}\", \"source\": \"extra\" }}{}", pkg, comma));
    }

    for (i, (pkg, repo)) in other_pkgs.iter().enumerate() {
        let comma = if i < other_pkgs.len() - 1 { "," } else { "" };
        lines.push(format!("        {{ \"name\": \"{}\", \"source\": \"{}\" }}{}", pkg, repo, comma));
    }

    lines.push("    ],".to_string());
    lines.push("    \"options\": {".to_string());
    lines.push("        \"remove_undeclared\": true,".to_string());
    lines.push("        \"default_build_options\": {".to_string());
    lines.push("            \"toolchain_c\": \"gcc\",".to_string());
    lines.push("            \"toolchain_rust\": \"cargo\",".to_string());
    lines.push("            \"build_flags_rust\": [\"--release\"],".to_string());
    lines.push("            \"build_flags_c\": [\"-DCMAKE_BUILD_TYPE=Release\", \"-j4\"]".to_string());
    lines.push("        }".to_string());
    lines.push("    }".to_string());
    lines.push("}".to_string());

    let content = lines.join("\n") + "\n";
    fs::write(output, content)
        .with_context(|| format!("failed to write {}", output.display()))?;

    println!("Saved {} packages to {}", core_pkgs.len() + extra_pkgs.len() + other_pkgs.len(), output.display());
    Ok(())
}
