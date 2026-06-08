use alpm::{Alpm, SigLevel, TransFlag};
use anyhow::{anyhow, Context, Result};
use crate::manifest::{BuildOptions, PackageSource, ResolvedManifest};
use crate::pacman_conf::PacmanConfig;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
pub struct PlanPackage {
    pub name: String,
    pub source: PackageSource,
    pub url: Option<String>,
    pub build_options: BuildOptions,
    pub installed: bool,
    pub explicit: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanSummary {
    pub manifest_packages: usize,
    pub resolved_packages: usize,
    pub already_installed: usize,
    pub would_install: Vec<String>,
    pub would_remove: Vec<String>,
    pub prune_undeclared: bool,
}

pub struct InstallPlan {
    pub packages: Vec<PlanPackage>,
    pub missing_packages: Vec<PlanPackage>,
    pub packages_to_remove: Vec<String>,
    pub prune_undeclared: bool,
}

impl InstallPlan {
    pub fn from_manifest(manifest: &ResolvedManifest, alpm: &Alpm) -> Result<Self> {
        let installed_names = installed_package_names(alpm)?;
        let manifest_names: HashSet<&str> = manifest
            .packages
            .iter()
            .map(|package| package.name.as_str())
            .collect();

        let resolved_deps = resolve_all_dependencies(alpm, manifest)?;

        let all_needed: HashSet<String> = resolved_deps;

        let to_install: Vec<String> = all_needed
            .iter()
            .filter(|name| {
                if alpm.localdb().pkg(name.as_str()).is_ok() {
                    return false;
                }
                if alpm.localdb().pkgs().find_satisfier(name.as_str()).is_some() {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        let to_remove: Vec<String> = if manifest.options.remove_undeclared.unwrap_or(false) {
            find_removable_packages(alpm, &installed_names, &all_needed)
        } else {
            Vec::new()
        };

        let packages: Vec<PlanPackage> = manifest
            .packages
            .iter()
            .map(|package| PlanPackage {
                name: package.name.clone(),
                source: package.source.clone(),
                url: package.url.clone(),
                build_options: package.resolve_build_options(manifest.options.default_build_options.as_ref()),
                installed: installed_names.contains(package.name.as_str()),
                explicit: true,
            })
            .collect();

        let missing_packages: Vec<PlanPackage> = to_install
            .iter()
            .map(|name| PlanPackage {
                name: name.clone(),
                source: PackageSource::Core,
                url: None,
                build_options: BuildOptions::default(),
                installed: false,
                explicit: manifest_names.contains(name.as_str()),
            })
            .collect();

        Ok(InstallPlan {
            packages,
            missing_packages,
            packages_to_remove: to_remove,
            prune_undeclared: manifest.options.remove_undeclared.unwrap_or(false),
        })
    }

    pub fn to_summary(&self) -> PlanSummary {
        PlanSummary {
            manifest_packages: self.packages.len(),
            resolved_packages: self.packages.len() + self.missing_packages.len(),
            already_installed: self.packages.iter().filter(|p| p.installed).count()
                + self.missing_packages.iter().filter(|p| p.installed).count(),
            would_install: self.missing_packages.iter().map(|p| p.name.clone()).collect(),
            would_remove: self.packages_to_remove.clone(),
            prune_undeclared: self.prune_undeclared,
        }
    }

    pub fn print_summary(&self) {
        println!("Arx package plan summary:");
        println!("  manifest packages: {}", self.packages.len());
        println!("  resolved packages: {}", self.packages.len() + self.missing_packages.len());
        println!("  already installed: {}",
            self.packages.iter().filter(|p| p.installed).count()
            + self.missing_packages.iter().filter(|p| p.installed).count());

        if !self.missing_packages.is_empty() {
            println!("  packages to install: {}", self.missing_packages.len());
            for package in &self.missing_packages {
                let tag = if package.explicit { " (explicit)" } else { " (dependency)" };
                println!("    - {}{}", package.name, tag);
            }
        } else {
            println!("  packages to install: 0");
        }

        if self.prune_undeclared {
            println!("  prune undeclared packages: enabled");
            println!("  packages to remove: {}", self.packages_to_remove.len());
            for name in self.packages_to_remove.iter().take(20) {
                println!("    - {}", name);
            }
            if self.packages_to_remove.len() > 20 {
                println!("    ...and {} more", self.packages_to_remove.len() - 20);
            }
        } else {
            println!("  prune undeclared packages: disabled");
        }
    }
}

pub fn init_alpm(conf: &PacmanConfig) -> Result<Alpm> {
    let root = conf.root_dir.to_str()
        .with_context(|| "root path is not valid UTF-8")?;
    let db_path = conf.db_path.to_str()
        .with_context(|| "db path is not valid UTF-8")?;

    let mut alpm = Alpm::new(root, db_path)
        .with_context(|| "failed to initialize alpm")?;

    for repo in &conf.repos {
        if repo.servers.is_empty() {
            continue;
        }
        let db = alpm.register_syncdb_mut(repo.name.as_str(), SigLevel::USE_DEFAULT)
            .with_context(|| format!("failed to register sync db: {}", repo.name))?;
        for server in &repo.servers {
            db.add_server(server.as_str())
                .with_context(|| format!("failed to add server {} to {}", server, repo.name))?;
        }
    }

    Ok(alpm)
}

fn resolve_all_dependencies(alpm: &Alpm, manifest: &ResolvedManifest) -> Result<HashSet<String>> {
    let mut resolved: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = manifest
        .packages
        .iter()
        .map(|p| p.name.clone())
        .collect();

    while let Some(name) = stack.pop() {
        if resolved.contains(&name) {
            continue;
        }

        // Check if already installed locally (handles virtual packages)
        if alpm.localdb().pkg(name.as_str()).is_ok() {
            resolved.insert(name);
            continue;
        }

        // Check if an installed package provides this dependency
        if alpm.localdb().pkgs().find_satisfier(name.as_str()).is_some() {
            resolved.insert(name);
            continue;
        }

        // Find in sync databases
        let pkg = match alpm.syncdbs().find_satisfier(name.as_str()) {
            Some(pkg) => pkg,
            None => continue,
        };

        resolved.insert(name.clone());

        for dep in pkg.depends().iter() {
            let dep_name = dep.name().to_string();
            if !resolved.contains(&dep_name) {
                stack.push(dep_name);
            }
        }
    }

    Ok(resolved)
}

fn find_removable_packages(
    alpm: &Alpm,
    installed_names: &HashSet<String>,
    all_needed: &HashSet<String>,
) -> Vec<String> {
    installed_names
        .iter()
        .filter(|name| {
            if all_needed.contains(name.as_str()) {
                return false;
            }
            if let Ok(pkg) = alpm.localdb().pkg(name.as_str()) {
                for needed in all_needed {
                    if pkg.provides().iter().any(|p| p.name() == needed.as_str()) {
                        return false;
                    }
                }
            }
            true
        })
        .cloned()
        .collect()
}

pub fn execute_plan(plan: &InstallPlan, alpm: &mut Alpm, dry_run: bool) -> Result<()> {
    let to_install: Vec<&str> = plan.missing_packages.iter().map(|p| p.name.as_str()).collect();
    let to_remove: &Vec<String> = &plan.packages_to_remove;

    if to_install.is_empty() && to_remove.is_empty() {
        println!("System is already in sync with the manifest.");
        return Ok(());
    }

    println!("Plan:");
    if !to_install.is_empty() {
        println!("  {} packages to install", to_install.len());
    }
    if !to_remove.is_empty() {
        println!("  {} packages to remove", to_remove.len());
    }

    if dry_run {
        if !to_install.is_empty() {
            println!("\nPackages to install:");
            for name in &to_install {
                println!("  - {}", name);
            }
        }
        if !to_remove.is_empty() {
            println!("\nPackages to remove:");
            for name in to_remove {
                println!("  - {}", name);
            }
        }
        println!("\nDry run complete. No changes were made.");
        return Ok(());
    }

    if !to_install.is_empty() {
        println!("\nInstalling {} packages...", to_install.len());
        alpm.trans_init(TransFlag::NONE)
            .with_context(|| "failed to initialize transaction")?;

        for name in &to_install {
            if let Some(pkg) = alpm.syncdbs().find_satisfier(*name)
                && let Err(e) = alpm.trans_add_pkg(pkg)
            {
                eprintln!("warning: failed to add {}: {}", name, e);
            }
        }

        let prepare_err = {
            match alpm.trans_prepare() {
                Ok(()) => None,
                Err(e) => Some(format!("{}", e.error())),
            }
        };
        if let Some(msg) = prepare_err {
            let _ = alpm.trans_release();
            return Err(anyhow!("failed to prepare transaction: {}", msg));
        }

        let install_list = alpm.trans_add();
        let install_count = install_list.into_iter().count();
        println!("  {} packages will be installed", install_count);

        let commit_err = {
            match alpm.trans_commit() {
                Ok(()) => None,
                Err(e) => Some(format!("{}", e.error())),
            }
        };
        if let Some(msg) = commit_err {
            let _ = alpm.trans_release();
            return Err(anyhow!("failed to commit transaction: {}", msg));
        }

        alpm.trans_release()
            .with_context(|| "failed to release transaction")?;

        println!("  Installation complete.");
    }

    if !to_remove.is_empty() {
        println!("\nRemoving {} packages...", to_remove.len());
        alpm.trans_init(TransFlag::RECURSE | TransFlag::CASCADE)
            .with_context(|| "failed to initialize removal transaction")?;

        for name in to_remove {
            if let Ok(pkg) = alpm.localdb().pkg(name.as_str()) {
                alpm.trans_remove_pkg(pkg)
                    .with_context(|| format!("failed to add {} to removal transaction", name))?;
            }
        }

        let prepare_err = {
            match alpm.trans_prepare() {
                Ok(()) => None,
                Err(e) => Some(format!("{}", e.error())),
            }
        };
        if let Some(msg) = prepare_err {
            let _ = alpm.trans_release();
            return Err(anyhow!("failed to prepare removal transaction: {}", msg));
        }

        let remove_list = alpm.trans_remove();
        let remove_count = remove_list.into_iter().count();
        println!("  {} packages will be removed", remove_count);

        let commit_err = {
            match alpm.trans_commit() {
                Ok(()) => None,
                Err(e) => Some(format!("{}", e.error())),
            }
        };
        if let Some(msg) = commit_err {
            let _ = alpm.trans_release();
            return Err(anyhow!("failed to commit removal transaction: {}", msg));
        }

        alpm.trans_release()
            .with_context(|| "failed to release removal transaction")?;

        println!("  Removal complete.");
    }

    println!("\nSystem is now in sync with the manifest.");
    Ok(())
}

pub fn installed_package_names(alpm: &Alpm) -> Result<HashSet<String>> {
    let localdb = alpm.localdb();
    let names = localdb
        .pkgs()
        .iter()
        .map(|pkg| pkg.name().to_string())
        .collect();
    Ok(names)
}
