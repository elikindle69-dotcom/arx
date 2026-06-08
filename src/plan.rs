use alpm::Alpm;
use anyhow::{Context, Result, anyhow};
use crate::manifest::{BuildOptions, PackageSource, ResolvedManifest};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct PlanPackage {
    pub name: String,
    pub source: PackageSource,
    pub url: Option<String>,
    pub build_options: BuildOptions,
    pub installed: bool,
}

pub struct InstallPlan {
    pub packages: Vec<PlanPackage>,
    pub missing_packages: Vec<PlanPackage>,
    pub undeclared_installed: Vec<String>,
    pub prune_undeclared: bool,
}

impl InstallPlan {
    pub fn from_manifest(manifest: &ResolvedManifest) -> Result<Self> {
        let alpm = Alpm::new("/", "/var/lib/pacman")
            .with_context(|| "failed to initialize alpm against the host system")?;

        let installed_names = installed_package_names(&alpm)?;
        let manifest_names: HashSet<String> = manifest
            .packages
            .iter()
            .map(|package| package.name.clone())
            .collect();

        let packages: Vec<PlanPackage> = manifest
            .packages
            .iter()
            .map(|package| PlanPackage {
                name: package.name.clone(),
                source: package.source.clone(),
                url: package.url.clone(),
                build_options: package.resolve_build_options(manifest.options.default_build_options.as_ref()),
                installed: installed_names.contains(&package.name),
            })
            .collect();

        let missing_packages = packages
            .iter()
            .filter(|pkg| !pkg.installed)
            .cloned()
            .collect();

        let undeclared_installed = if manifest.options.remove_undeclared.unwrap_or(false) {
            installed_names
                .into_iter()
                .filter(|name| !manifest_names.contains(name))
                .collect()
        } else {
            Vec::new()
        };

        Ok(InstallPlan {
            packages,
            missing_packages,
            undeclared_installed,
            prune_undeclared: manifest.options.remove_undeclared.unwrap_or(false),
        })
    }

    pub fn print_summary(&self) {
        println!("Arx dry-run package plan summary:");
        println!("  manifest packages: {}", self.packages.len());
        println!("  already installed: {}", self.packages.iter().filter(|p| p.installed).count());
        println!("  packages that would be installed: {}", self.missing_packages.len());

        if !self.missing_packages.is_empty() {
            println!("  install candidates:");
            for package in &self.missing_packages {
                println!("    - {} ({})", package.name, source_label(&package.source));
            }
        }

        if self.prune_undeclared {
            println!("  prune undeclared packages: enabled");
            println!("  undeclared installed packages: {}", self.undeclared_installed.len());
            for name in self.undeclared_installed.iter().take(20) {
                println!("    - {}", name);
            }
            if self.undeclared_installed.len() > 20 {
                println!("    ...and {} more", self.undeclared_installed.len() - 20);
            }
        } else {
            println!("  prune undeclared packages: disabled");
        }

        println!("  dry run: no changes will be applied");
    }

    pub fn is_empty(&self) -> bool {
        self.missing_packages.is_empty() && (self.undeclared_installed.is_empty() || !self.prune_undeclared)
    }

    pub fn print_removal_details(&self) -> Result<()> {
        if !self.prune_undeclared || self.undeclared_installed.is_empty() {
            return Ok(());
        }

        let alpm = Alpm::new("/", "/var/lib/pacman")
            .context("failed to initialize alpm for dependency analysis")?;

        let localdb = alpm.localdb();

        println!("\n⚠️  Removal Analysis (packages that will be removed):");
        
        for removed_pkg_name in &self.undeclared_installed {
            print!("  - {} ", removed_pkg_name);

            // Try to get package info for group membership
            if let Ok(pkg) = localdb.pkg(removed_pkg_name.as_str()) {
                let mut details = Vec::new();

                // Get the package size info
                let size = pkg.download_size();
                if size > 0 {
                    details.push(format!("{}KB", size / 1024));
                }

                // Get groups this package belongs to
                let groups: Vec<String> = pkg.groups()
                    .iter()
                    .map(|g| g.to_string())
                    .collect();
                if !groups.is_empty() {
                    details.push(format!("groups: {}", groups.join(", ")));
                }

                if !details.is_empty() {
                    println!("({})", details.join(", "));
                } else {
                    println!();
                }
            } else {
                println!();
            }

            // Check if any declared packages depend on this removed package
            let mut dependents = Vec::new();
            for pkg in self.packages.iter() {
                if pkg.installed {
                    if let Ok(installed_pkg) = localdb.pkg(pkg.name.as_str()) {
                        let depends: Vec<&str> = installed_pkg
                            .depends()
                            .iter()
                            .filter_map(|dep| {
                                let dep_str = dep.to_string();
                                // Extract package name from dependency string (before version specs)
                                if let Some(dep_name) = dep_str.split(|c| c == '>' || c == '<' || c == '=' || c == '!').next() {
                                    if dep_name == removed_pkg_name {
                                        Some(pkg.name.as_str())
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })
                            .collect();
                        
                        if !depends.is_empty() {
                            dependents.extend(depends);
                        }
                    }
                }
            }

            if !dependents.is_empty() {
                println!("    ⚠️  WARNING: Required by declared packages: {}", dependents.join(", "));
            }
        }

        Ok(())
    }

    pub fn apply(&self) -> Result<()> {
        // Check for root permissions
        if unsafe { libc::geteuid() } != 0 {
            return Err(anyhow!(
                "package management requires root privileges. Try: sudo arx apply <manifest>"
            ));
        }

        let mut alpm = Alpm::new("/", "/var/lib/pacman")
            .context("failed to initialize alpm for package operations")?;

        alpm.trans_init(alpm::TransFlag::NONE)
            .context("failed to initialize ALPM transaction")?;

        // Pre-flight check: verify all packages exist before starting transaction
        for package in &self.missing_packages {
            match &package.source {
                PackageSource::Core | PackageSource::Extra => {
                    verify_repo_package(&alpm, &package.name)?;
                }
                PackageSource::Aur => {
                    eprintln!("warning: AUR package '{}' cannot be installed yet (not implemented)", package.name);
                }
                PackageSource::Git => {
                    eprintln!("warning: git package '{}' cannot be installed yet (not implemented)", package.name);
                }
            }
        }

        // Add packages that need to be installed
        for package in &self.missing_packages {
            match &package.source {
                PackageSource::Core | PackageSource::Extra => {
                    add_repo_package(&mut alpm, &package.name)?;
                }
                PackageSource::Aur | PackageSource::Git => {
                    // Already warned in pre-flight check
                }
            }
        }

        // Remove packages that are undeclared
        if self.prune_undeclared {
            let localdb = alpm.localdb();
            for pkg_name in &self.undeclared_installed {
                if let Ok(pkg) = localdb.pkg(pkg_name.as_str()) {
                    alpm.trans_remove_pkg(pkg)
                        .with_context(|| format!("failed to queue removal of package '{}'", pkg_name))?;
                }
            }
        }

        // Prepare the transaction
        let prep_err = match alpm.trans_prepare() {
            Err(e) => {
                let msg = e.to_string();
                drop(e);
                Some(msg)
            }
            Ok(_) => None,
        };

        if let Some(msg) = prep_err {
            let _ = alpm.trans_release();
            return Err(anyhow!("transaction preparation failed: {}", msg));
        }

        // Commit the transaction
        let commit_err = match alpm.trans_commit() {
            Err(e) => {
                let msg = e.to_string();
                drop(e);
                Some(msg)
            }
            Ok(_) => None,
        };

        if let Some(msg) = commit_err {
            let _ = alpm.trans_release();
            return Err(anyhow!("transaction commit failed: {}", msg));
        }

        alpm.trans_release()
            .context("failed to release ALPM transaction")?;

        Ok(())
    }
}

fn source_label(source: &PackageSource) -> &'static str {
    match source {
        PackageSource::Core => "core",
        PackageSource::Extra => "extra",
        PackageSource::Aur => "aur",
        PackageSource::Git => "git",
    }
}

fn installed_package_names(alpm: &Alpm) -> Result<HashSet<String>> {
    let localdb = alpm.localdb();
    let names = localdb
        .pkgs()
        .iter()
        .map(|pkg| pkg.name().to_string())
        .collect();
    Ok(names)
}

fn add_repo_package(alpm: &mut Alpm, pkg_name: &str) -> Result<()> {
    let syncdbs = alpm.syncdbs();
    for db in syncdbs.iter() {
        if let Ok(pkg) = db.pkg(pkg_name) {
            alpm.trans_add_pkg(pkg)
                .map_err(|e| anyhow!("failed to add package '{}' to transaction: {}", pkg_name, e.error))?;
            return Ok(());
        }
    }
    Err(anyhow!("package '{}' not found in any sync database", pkg_name))
}

fn verify_repo_package(alpm: &Alpm, pkg_name: &str) -> Result<()> {
    let syncdbs = alpm.syncdbs();
    for db in syncdbs.iter() {
        if db.pkg(pkg_name).is_ok() {
            return Ok(());
        }
    }
    Err(anyhow!("package '{}' not found in any sync database", pkg_name))
}
