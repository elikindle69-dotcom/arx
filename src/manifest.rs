use anyhow::{anyhow, Context, Result};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum PackageSource {
    Core,
    Extra,
    Aur,
    Git,
    Repo(String),
}

impl Serialize for PackageSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PackageSource::Core => serializer.serialize_str("core"),
            PackageSource::Extra => serializer.serialize_str("extra"),
            PackageSource::Aur => serializer.serialize_str("aur"),
            PackageSource::Git => serializer.serialize_str("git"),
            PackageSource::Repo(name) => serializer.serialize_str(name),
        }
    }
}

impl<'de> Deserialize<'de> for PackageSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "core" => Ok(PackageSource::Core),
            "extra" => Ok(PackageSource::Extra),
            "aur" => Ok(PackageSource::Aur),
            "git" => Ok(PackageSource::Git),
            other => Ok(PackageSource::Repo(other.to_string())),
        }
    }
}

impl fmt::Display for PackageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageSource::Core => write!(f, "core"),
            PackageSource::Extra => write!(f, "extra"),
            PackageSource::Aur => write!(f, "aur"),
            PackageSource::Git => write!(f, "git"),
            PackageSource::Repo(name) => write!(f, "{}", name),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
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
            .with_context(|| format!("failed to canonicalize manifest path {}", source_path.display()))?;

        let mut visited = HashSet::new();
        let resolved = load_recursive(&canonical_path, &mut visited)?;
        Ok(resolved)
    }

    pub fn validate(&self) -> Result<()> {
        let mut seen = HashSet::new();
        for package in &self.packages {
            if package.name.trim().is_empty() {
                return Err(anyhow!("package name must not be empty"));
            }

            if !seen.insert(&package.name) {
                return Err(anyhow!("duplicate package name: {:?}", package.name));
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
                .with_context(|| format!("failed to resolve included manifest {}", included_path.display()))?;
            let included_manifest = load_recursive(&included_path, visited)?;
            merged_packages.extend(included_manifest.packages);
            merged_options = merge_options(merged_options, included_manifest.options);
        }
    }

    merged_packages.extend(manifest.packages);
    let merged_packages = dedupe_packages(merged_packages);
    merged_options = merge_options(merged_options, manifest.options.unwrap_or_default());

    Ok(ResolvedManifest {
        packages: merged_packages,
        options: merged_options,
    })
}

fn load_manifest_file(path: &Path) -> Result<Manifest> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest file {}", path.display()))?;
    let manifest: Manifest = json5::from_str(&text)
        .with_context(|| format!("failed to parse manifest JSONC {}", path.display()))?;
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
    let mut seen = HashSet::new();
    packages
        .into_iter()
        .filter(|package| seen.insert(package.name.clone()))
        .collect()
}

impl Package {
    pub fn resolve_build_options(&self, defaults: Option<&DefaultBuildOptions>) -> BuildOptions {
        let explicit = self.build_options.clone().unwrap_or_default();
        if explicit.ignore_default {
            return explicit;
        }

        let mut merged = BuildOptions {
            language: explicit.language.clone(),
            toolchain: explicit.toolchain.clone(),
            ..BuildOptions::default()
        };

        if let (Some(defaults), Some(language)) = (defaults, merged.language.as_deref()) {
            let lang_lower = language.to_ascii_lowercase();
            if lang_lower == "rust" {
                if merged.toolchain.is_none() {
                    merged.toolchain = defaults.toolchain_rust.clone();
                }
                if merged.build_flags.is_none() {
                    merged.build_flags = defaults.build_flags_rust.clone();
                }
            } else if lang_lower == "c" || lang_lower == "c++" {
                if merged.toolchain.is_none() {
                    merged.toolchain = defaults.toolchain_c.clone();
                }
                if merged.build_flags.is_none() {
                    merged.build_flags = defaults.build_flags_c.clone();
                }
            }
        }

        merged
    }
}
