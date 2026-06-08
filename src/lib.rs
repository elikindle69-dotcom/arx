pub mod cli;
pub mod manifest;
pub mod plan;

#[cfg(test)]
mod tests {
    use crate::manifest::{Package, PackageSource, ResolvedManifest};

    #[test]
    fn test_manifest_validation_empty_name() {
        let resolved = ResolvedManifest {
            packages: vec![Package {
                name: "".to_string(),
                source: PackageSource::Extra,
                url: None,
                depends: None,
                build_inputs: None,
                build_options: None,
            }],
            options: Default::default(),
        };
        
        assert!(resolved.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_git_without_url() {
        let resolved = ResolvedManifest {
            packages: vec![Package {
                name: "mypackage".to_string(),
                source: PackageSource::Git,
                url: None,
                depends: None,
                build_inputs: None,
                build_options: None,
            }],
            options: Default::default(),
        };
        
        assert!(resolved.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_git_with_url() {
        let resolved = ResolvedManifest {
            packages: vec![Package {
                name: "mypackage".to_string(),
                source: PackageSource::Git,
                url: Some("https://github.com/user/repo.git".to_string()),
                depends: None,
                build_inputs: None,
                build_options: None,
            }],
            options: Default::default(),
        };
        
        assert!(resolved.validate().is_ok());
    }

    #[test]
    fn test_build_option_merging() {
        use crate::manifest::{BuildOptions, DefaultBuildOptions};

        let package = Package {
            name: "test".to_string(),
            source: PackageSource::Extra,
            url: None,
            depends: None,
            build_inputs: None,
            build_options: Some(BuildOptions {
                ignore_default: false,
                language: Some("rust".to_string()),
                toolchain: None,
                build_flags: Some(vec!["--release".to_string()]),
            }),
        };

        let defaults = DefaultBuildOptions {
            toolchain_c: Some("gcc".to_string()),
            toolchain_rust: Some("cargo".to_string()),
            build_flags_c: None,
            build_flags_rust: Some(vec!["--locked".to_string()]),
        };

        let merged = package.resolve_build_options(Some(&defaults));
        assert_eq!(merged.language, Some("rust".to_string()));
        assert_eq!(merged.toolchain, Some("cargo".to_string()));
    }
}
