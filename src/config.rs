use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub const DEFAULT_CONFIG_DIR_SUFFIX: &str = ".config/todos";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub config_dir: PathBuf,
    pub todo_file: PathBuf,
    pub env_file: PathBuf,
    pub git_remote: Option<String>,
    pub git_branch: String,
    pub git_author_name: Option<String>,
    pub git_author_email: Option<String>,
    pub github_token: Option<String>,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let config_dir = resolve_config_dir()?;
        let env_file = config_dir.join(".env");
        let env_map = load_optional_env_file(&env_file)?;

        let todo_file =
            resolve_path_override("TODOS_FILE", &env_map, Some(config_dir.join("todo.md")))?;

        let git_remote = first_non_empty(
            env::var("TODOS_GIT_REMOTE").ok(),
            env_map.get("TODOS_GIT_REMOTE").cloned(),
        );

        let git_branch = first_non_empty(
            env::var("TODOS_GIT_BRANCH").ok(),
            env_map.get("TODOS_GIT_BRANCH").cloned(),
        )
        .unwrap_or_else(|| "main".to_string());

        let git_author_name = first_non_empty(
            env::var("TODOS_GIT_AUTHOR_NAME").ok(),
            env_map.get("TODOS_GIT_AUTHOR_NAME").cloned(),
        );

        let git_author_email = first_non_empty(
            env::var("TODOS_GIT_AUTHOR_EMAIL").ok(),
            env_map.get("TODOS_GIT_AUTHOR_EMAIL").cloned(),
        );

        let github_token = first_non_empty(
            env::var("GITHUB_TOKEN").ok(),
            env_map.get("GITHUB_TOKEN").cloned(),
        );

        Ok(Self {
            config_dir,
            todo_file,
            env_file,
            git_remote,
            git_branch,
            git_author_name,
            git_author_email,
            github_token,
        })
    }
}

fn resolve_config_dir() -> Result<PathBuf> {
    let default_dir = dirs::home_dir()
        .map(|home| home.join(DEFAULT_CONFIG_DIR_SUFFIX))
        .context("could not resolve home directory")?;

    resolve_path_override("TODOS_CONFIG_DIR", &HashMap::new(), Some(default_dir))
}

fn resolve_path_override(
    key: &str,
    env_map: &HashMap<String, String>,
    fallback: Option<PathBuf>,
) -> Result<PathBuf> {
    let candidate = first_non_empty(env::var(key).ok(), env_map.get(key).cloned());
    match candidate {
        Some(value) => expand_tilde(PathBuf::from(value)),
        None => fallback.context("missing path fallback"),
    }
}

fn load_optional_env_file(path: &Path) -> Result<HashMap<String, String>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let iter = dotenvy::from_path_iter(path)
        .with_context(|| format!("failed to read env file at {}", path.display()))?;

    let mut map = HashMap::new();
    for item in iter {
        let (key, value) =
            item.with_context(|| format!("invalid env entry in {}", path.display()))?;
        map.insert(key, value);
    }

    Ok(map)
}

fn expand_tilde(path: PathBuf) -> Result<PathBuf> {
    let as_str = path.to_string_lossy();
    if as_str == "~" || as_str.starts_with("~/") {
        let home = dirs::home_dir().context("could not resolve home directory for ~ expansion")?;
        if as_str == "~" {
            return Ok(home);
        }
        return Ok(home.join(&as_str[2..]));
    }

    if path.is_absolute() {
        return Ok(path);
    }

    let cwd = env::current_dir().context("could not resolve current directory")?;
    Ok(cwd.join(path))
}

fn first_non_empty(first: Option<String>, second: Option<String>) -> Option<String> {
    [first, second]
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
}

pub fn require_remote(config: &AppConfig) -> Result<&str> {
    let Some(remote) = config.git_remote.as_deref() else {
        bail!(
            "missing git remote; set TODOS_GIT_REMOTE in {} or environment",
            config.env_file.display()
        );
    };
    Ok(remote)
}
