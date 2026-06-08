use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct RepoConfig {
    pub name: String,
    pub servers: Vec<String>,
}

#[derive(Debug)]
pub struct PacmanConfig {
    pub root_dir: PathBuf,
    pub db_path: PathBuf,
    pub repos: Vec<RepoConfig>,
}

impl PacmanConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let text = fs::read_to_string(path.as_ref())
            .with_context(|| format!("failed to read {}", path.as_ref().display()))?;
        Self::parse(&text)
    }

    pub fn load_default() -> Result<Self> {
        Self::load("/etc/pacman.conf")
    }

    fn split_key_value(line: &str) -> Option<(&str, &str)> {
        let (key, rest) = line.split_once('=')?;
        Some((key.trim(), rest.trim()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut root_dir = PathBuf::from("/");
        let mut db_path = PathBuf::from("/var/lib/pacman");
        let mut repos: Vec<RepoConfig> = Vec::new();
        let mut current_repo: Option<RepoConfig> = None;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    let name = name.trim().to_string();
                    if name == "options" {
                        continue;
                    }
                    if let Some(repo) = current_repo.take() {
                        repos.push(repo);
                    }
                    current_repo = Some(RepoConfig {
                        name,
                        servers: Vec::new(),
                    });
                }
                continue;
            }

            if let Some((key, val)) = Self::split_key_value(line) {
                match key {
                    "RootDir" => root_dir = PathBuf::from(val),
                    "DBPath" => db_path = PathBuf::from(val),
                    "Include" => {
                        let include_path = PathBuf::from(val);
                        if let Some(repo) = &mut current_repo {
                            match Self::parse_mirrorlist(&include_path) {
                                Ok(servers) => {
                                    for server in servers {
                                        repo.servers.push(server);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("warning: failed to read {}: {}", include_path.display(), e);
                                }
                            }
                        }
                    }
                    "Server" => {
                        if let Some(repo) = &mut current_repo {
                            repo.servers.push(val.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(repo) = current_repo.take() {
            repos.push(repo);
        }

        Ok(PacmanConfig {
            root_dir,
            db_path,
            repos,
        })
    }

    fn parse_mirrorlist(path: &Path) -> Result<Vec<String>> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read mirrorlist {}", path.display()))?;

        let mut servers = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, val)) = Self::split_key_value(line)
                && key == "Server"
            {
                servers.push(val.to_string());
            }
        }
        Ok(servers)
    }
}
