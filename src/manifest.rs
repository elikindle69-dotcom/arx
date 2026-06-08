use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum PackageSource {
    Core,
    Extra,
    Aur,
    Git,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "snake_case")]
pub struct BuildOptions {
    #[serde(default)]
    pub ignore_default: bool,
    pub language: Option<String>,
    pub toolchain: Option<String>,
    pub build_flags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DefaultBuildOptions {
    pub toolchain_c: Option<String>,
    pub toolchain_rust: Option<String>,
    pub build_flags_c: Option<Vec<String>>,
    pub build_flags_rust: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ManifestOptions {
    pub remove_undeclared: Option<bool>,
    pub default_build_options: Option<DefaultBuildOptions>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Package {
    pub name: String,
    pub source: PackageSource,
    pub url: Option<String>,
    pub depends: Option<Vec<String>>,
    pub build_inputs: Option<Vec<String>>,
    pub build_options: Option<BuildOptions>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Manifest {
    pub include: Option<Vec<PathBuf>>,
    pub packages: Vec<Package>,
    pub options: Option<ManifestOptions>,
}

#[derive(Debug, Clone)]
pub struct ResolvedManifest {
    pub packages: Vec<Package>,
    pub options: ManifestOptions,
}

impl ResolvedManifest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let source_path = path.as_ref();
        let canonical_path = source_path
            .canonicalize()
            .with_context(|| format!("cannot canonicalize manifest path {source_path:?}"))?;

        let mut visited = HashSet::new();
        let resolved = load_recursive(&canonical_path, &mut visited)?;
        Ok(resolved)
    }

    pub fn validate(&self) -> Result<()> {
        for package in &self.packages {
            if package.name.trim().is_empty() {
                return Err(anyhow!("package name must not be empty"));
            }

            if matches!(package.source, PackageSource::Git) && package.url.is_none() {
                return Err(anyhow!("git packages must include a url field"));
            }
        }

        Ok(())
    }
}

fn load_recursive(path: &Path, visited: &mut HashSet<PathBuf>) -> Result<ResolvedManifest> {
    if !visited.insert(path.to_path_buf()) {
        return Err(anyhow!("circular include detected for manifest {:?}", path));
    }

    let manifest = load_manifest_file(path)?;
    let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let mut merged_packages = Vec::new();
    let mut merged_options = ManifestOptions::default();

    if let Some(includes) = &manifest.include {
        for include_path in includes {
            let included_path = if include_path.is_absolute() {
                include_path.clone()
            } else {
                parent_dir.join(include_path)
            };
            let included_path = included_path
                .canonicalize()
                .with_context(|| format!("cannot resolve included manifest {included_path:?}"))?;
            let included_manifest = load_recursive(&included_path, visited)?;
            merged_packages.extend(included_manifest.packages);
            merged_options = merge_options(merged_options, included_manifest.options);
        }
    }

    merged_packages.extend(manifest.packages.clone());
    let merged_packages = dedupe_packages(merged_packages);
    merged_options = merge_options(merged_options, manifest.options.unwrap_or_default());

    visited.remove(path);
    Ok(ResolvedManifest {
        packages: merged_packages,
        options: merged_options,
    })
}

fn load_manifest_file(path: &Path) -> Result<Manifest> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest file {path:?}"))?;
    let manifest: Manifest = json5::from_str(&text)
        .with_context(|| format!("failed to parse manifest JSONC {path:?}"))?;
    Ok(manifest)
}

fn merge_options(base: ManifestOptions, override_with: ManifestOptions) -> ManifestOptions {
    ManifestOptions {
        remove_undeclared: override_with.remove_undeclared.or(base.remove_undeclared),
        default_build_options: override_with
            .default_build_options
            .or(base.default_build_options),
    }
}

fn dedupe_packages(packages: Vec<Package>) -> Vec<Package> {
    let mut seen = HashMap::new();
    for package in packages.into_iter().rev() {
        seen.entry(package.name.clone()).or_insert(package);
    }

    let mut deduped: Vec<Package> = seen.into_values().collect();
    deduped.reverse();
    deduped
}

impl Package {
    pub fn resolve_build_options(&self, defaults: Option<&DefaultBuildOptions>) -> BuildOptions {
        let explicit = self.build_options.clone().unwrap_or_default();
        if explicit.ignore_default {
            return explicit;
        }

        let mut merged = BuildOptions::default();
        merged.language = explicit.language.clone();
        merged.toolchain = explicit.toolchain.clone();

        if let Some(defaults) = defaults {
            let language = merged
                .language
                .as_deref()
                .or_else(|| explicit.language.as_deref());

            if let Some(language) = language {
                if language.eq_ignore_ascii_case("rust") {
                    if merged.toolchain.is_none() {
                        merged.toolchain = defaults.toolchain_rust.clone();
                    }
                    merged.build_flags = defaults.build_flags_rust.clone();
                } else if language.eq_ignore_ascii_case("c") || language.eq_ignore_ascii_case("c++") {
                    if merged.toolchain.is_none() {
                        merged.toolchain = defaults.toolchain_c.clone();
                    }
                    merged.build_flags = defaults.build_flags_c.clone();
                }
            }
        }

        if let Some(toolchain) = explicit.toolchain {
            merged.toolchain = Some(toolchain);
        }

        if let Some(build_flags) = explicit.build_flags {
            let mut flags = merged.build_flags.unwrap_or_default();
            flags.extend(build_flags);
            merged.build_flags = Some(flags);
        }

        merged
    }
}
