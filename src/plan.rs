use crate::manifest::{BuildOptions, PackageSource, ResolvedManifest};

pub struct PlanPackage {
    pub name: String,
    pub source: PackageSource,
    pub url: Option<String>,
    pub build_options: BuildOptions,
}

pub struct InstallPlan {
    pub packages: Vec<PlanPackage>,
    pub prune_undeclared: bool,
}

impl InstallPlan {
    pub fn from_manifest(manifest: &ResolvedManifest) -> Self {
        let defaults = manifest.options.default_build_options.as_ref();
        let packages = manifest
            .packages
            .iter()
            .map(|package| PlanPackage {
                name: package.name.clone(),
                source: package.source.clone(),
                url: package.url.clone(),
                build_options: package.resolve_build_options(defaults),
            })
            .collect();

        InstallPlan {
            packages,
            prune_undeclared: manifest.options.remove_undeclared.unwrap_or(false),
        }
    }

    pub fn print_summary(&self) {
        println!("Arx plan summary:");
        println!("  packages to install or manage: {}", self.packages.len());
        for package in &self.packages {
            let source = match &package.source {
                PackageSource::Core => "core",
                PackageSource::Extra => "extra",
                PackageSource::Aur => "aur",
                PackageSource::Git => "git",
            };
            let flags = package
                .build_options
                .build_flags
                .as_ref()
                .map(|flags| flags.join(" "))
                .unwrap_or_default();
            println!("    - {} ({})", package.name, source);
            if let Some(url) = &package.url {
                println!("        url: {}", url);
            }
            if let Some(toolchain) = &package.build_options.toolchain {
                println!("        toolchain: {}", toolchain);
            }
            if !flags.is_empty() {
                println!("        build flags: {}", flags);
            }
        }

        if self.prune_undeclared {
            println!("  prune undeclared packages: enabled");
        } else {
            println!("  prune undeclared packages: disabled");
        }
    }
}
