use alpm::Alpm;
use anyhow::{Context, Result};
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
