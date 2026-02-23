use std::path::Path;
use std::process::{Command, Output};
use std::{fs, io};

use anyhow::{bail, Context, Result};

use crate::config::{require_remote, AppConfig};
use crate::diff::{line_diff_summary, semantic_changes, ChangeSet};
use crate::storage::{
    ensure_layout, hydrate_todo_ids, parse_todo_content, read_todo_file, validate_todo_content,
    write_todo_file_atomic,
};

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub committed: bool,
    pub change_set: ChangeSet,
    pub line_summary: String,
}

pub fn setup(config: &AppConfig, remote_override: Option<&str>) -> Result<()> {
    ensure_layout(&config.config_dir, &config.todo_file, &config.env_file)?;

    if !config.config_dir.join(".git").exists() {
        run_git_checked(&config.config_dir, ["init"])?;
    }

    run_git_checked(
        &config.config_dir,
        ["checkout", "-B", config.git_branch.as_str()],
    )?;

    let remote = remote_override
        .map(|value| value.to_string())
        .or_else(|| config.git_remote.clone());

    if let Some(remote) = remote {
        ensure_github_repo_exists(config, &remote)?;
        ensure_remote(&config.config_dir, "origin", &remote)?;
        upsert_env_var(&config.env_file, "TODOS_GIT_REMOTE", &remote)?;
    }

    Ok(())
}

fn upsert_env_var(path: &Path, key: &str, value: &str) -> Result<()> {
    let existing = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read env file {}", path.display()))
        }
    };

    let mut found = false;
    let mut lines = Vec::new();

    for line in existing.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            lines.push(line.to_string());
            continue;
        }

        if let Some((lhs, _rhs)) = line.split_once('=') {
            if lhs.trim() == key {
                lines.push(format!("{key}={value}"));
                found = true;
                continue;
            }
        }

        lines.push(line.to_string());
    }

    if !found {
        lines.push(format!("{key}={value}"));
    }

    let mut next = lines.join("\n");
    if !next.is_empty() {
        next.push('\n');
    }

    if next != existing {
        write_todo_file_atomic(path, &next)?;
    }

    Ok(())
}

fn ensure_github_repo_exists(config: &AppConfig, remote_url: &str) -> Result<()> {
    let Some(slug) = github_repo_slug(remote_url) else {
        return Ok(());
    };

    let view = run_gh(config, ["repo", "view", slug.as_str()])?;
    if view.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&view.stderr).to_ascii_lowercase();
    let stdout = String::from_utf8_lossy(&view.stdout).to_ascii_lowercase();
    let missing = stderr.contains("could not resolve to a repository")
        || stderr.contains("not found")
        || stdout.contains("not found");

    if !missing {
        bail!(
            "failed to check github repo `{}` via gh\nstdout:\n{}\nstderr:\n{}",
            slug,
            String::from_utf8_lossy(&view.stdout).trim(),
            String::from_utf8_lossy(&view.stderr).trim()
        );
    }

    let create = run_gh(
        config,
        ["repo", "create", slug.as_str(), "--private", "--confirm"],
    )?;
    if create.status.success() {
        return Ok(());
    }

    bail!(
        "failed to create github repo `{}` via gh\nstdout:\n{}\nstderr:\n{}",
        slug,
        String::from_utf8_lossy(&create.stdout).trim(),
        String::from_utf8_lossy(&create.stderr).trim()
    )
}

pub fn sync(config: &AppConfig) -> Result<SyncResult> {
    ensure_layout(&config.config_dir, &config.todo_file, &config.env_file)?;
    let remote = require_remote(config)?;

    if !config.config_dir.join(".git").exists() {
        bail!(
            "{} is not a git repository; run `todo_md setup` first",
            config.config_dir.display()
        );
    }

    run_git_checked(&config.config_dir, ["fetch", "origin"])?;
    run_git_checked(
        &config.config_dir,
        ["checkout", "-B", config.git_branch.as_str()],
    )?;
    run_git_checked(
        &config.config_dir,
        ["pull", "--rebase", "origin", config.git_branch.as_str()],
    )?;

    let todo_rel = todo_path_relative_to_repo(config)?;
    let previous_content = git_show_or_empty(&config.config_dir, &format!("HEAD:{todo_rel}"))?;
    let mut current = read_todo_file(&config.todo_file)?;
    let (hydrated_content, hydrated_count, hydrate_issues) = hydrate_todo_ids(&current.content);
    if !hydrate_issues.is_empty() {
        let details = hydrate_issues
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "todo.md contains lines that could not be auto-assigned an id\n{}",
            details
        );
    }
    if hydrated_count > 0 {
        write_todo_file_atomic(&config.todo_file, &hydrated_content)?;
        current = read_todo_file(&config.todo_file)?;
    }

    let validation_issues = validate_todo_content(&current.content);
    if !validation_issues.is_empty() {
        let details = validation_issues
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "todo.md has invalid content; run `todo_md doctor` and fix issues before sync\n{}",
            details
        );
    }

    let previous = parse_todo_content(&previous_content);

    let change_set = semantic_changes(&previous, &current);
    let line_summary = line_diff_summary(&previous.content, &current.content);

    let todo_status = run_git_checked(
        &config.config_dir,
        ["status", "--porcelain", "--", todo_rel.as_str()],
    )?;

    if todo_status.trim().is_empty() {
        return Ok(SyncResult {
            committed: false,
            change_set,
            line_summary,
        });
    }

    run_git_checked(&config.config_dir, ["add", "--", todo_rel.as_str()])?;

    let message = commit_message(&change_set, &line_summary);
    run_git_commit(config, &message)?;
    run_git_checked(
        &config.config_dir,
        ["push", "-u", remote, config.git_branch.as_str()],
    )?;

    Ok(SyncResult {
        committed: true,
        change_set,
        line_summary,
    })
}

fn ensure_remote(repo_dir: &Path, name: &str, url: &str) -> Result<()> {
    let list = run_git_checked(repo_dir, ["remote"])?;
    if list.lines().any(|line| line.trim() == name) {
        run_git_checked(repo_dir, ["remote", "set-url", name, url])?;
    } else {
        run_git_checked(repo_dir, ["remote", "add", name, url])?;
    }
    Ok(())
}

fn run_git_commit(config: &AppConfig, message: &str) -> Result<()> {
    let mut command = Command::new("git");
    command
        .args(["commit", "-m", message])
        .current_dir(&config.config_dir);

    if let Some(name) = &config.git_author_name {
        command.env("GIT_AUTHOR_NAME", name);
        command.env("GIT_COMMITTER_NAME", name);
    }
    if let Some(email) = &config.git_author_email {
        command.env("GIT_AUTHOR_EMAIL", email);
        command.env("GIT_COMMITTER_EMAIL", email);
    }

    let output = command.output().context("failed to execute git commit")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    bail!(
        "git commit failed\nstdout:\n{}\nstderr:\n{}",
        stdout.trim(),
        stderr.trim()
    );
}

fn run_git_checked<const N: usize>(repo_dir: &Path, args: [&str; N]) -> Result<String> {
    let output = run_git(repo_dir, args)?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    bail!(
        "git {} failed\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        stdout.trim(),
        stderr.trim()
    );
}

fn run_git<const N: usize>(repo_dir: &Path, args: [&str; N]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .with_context(|| format!("failed to execute git in {}", repo_dir.display()))
}

fn run_gh<const N: usize>(config: &AppConfig, args: [&str; N]) -> Result<Output> {
    let mut command = Command::new("gh");
    command.args(args).current_dir(&config.config_dir);

    if let Some(token) = &config.github_token {
        command.env("GITHUB_TOKEN", token);
    }

    command.output().with_context(|| {
        format!(
            "failed to execute gh in {}; install gh or create the repo manually",
            config.config_dir.display()
        )
    })
}

fn github_repo_slug(remote_url: &str) -> Option<String> {
    let trimmed = remote_url.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        return clean_slug(rest);
    }

    if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        return clean_slug(rest);
    }

    if let Some(rest) = trimmed.strip_prefix("http://github.com/") {
        return clean_slug(rest);
    }

    if let Some(rest) = trimmed.strip_prefix("ssh://git@github.com/") {
        return clean_slug(rest);
    }

    None
}

fn clean_slug(raw: &str) -> Option<String> {
    let without_git = raw.trim_end_matches(".git").trim_matches('/');
    let mut parts = without_git.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

fn git_show_or_empty(repo_dir: &Path, object: &str) -> Result<String> {
    let output = run_git(repo_dir, ["show", object])?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    Ok(String::new())
}

fn todo_path_relative_to_repo(config: &AppConfig) -> Result<String> {
    let relative = config
        .todo_file
        .strip_prefix(&config.config_dir)
        .with_context(|| {
            format!(
                "todo file {} must be inside config dir {}",
                config.todo_file.display(),
                config.config_dir.display()
            )
        })?;

    Ok(relative.to_string_lossy().to_string())
}

fn commit_message(change_set: &ChangeSet, line_summary: &str) -> String {
    format!(
        "sync todos: +{} ~{} -{} done {} ({})",
        change_set.added,
        change_set.updated,
        change_set.deleted,
        change_set.completed,
        line_summary
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_slugs_from_common_urls() {
        assert_eq!(
            github_repo_slug("git@github.com:acme/todos.git").as_deref(),
            Some("acme/todos")
        );
        assert_eq!(
            github_repo_slug("https://github.com/acme/todos.git").as_deref(),
            Some("acme/todos")
        );
        assert_eq!(
            github_repo_slug("ssh://git@github.com/acme/todos").as_deref(),
            Some("acme/todos")
        );
    }

    #[test]
    fn ignores_non_github_or_invalid_urls() {
        assert_eq!(github_repo_slug("git@gitlab.com:acme/todos.git"), None);
        assert_eq!(github_repo_slug("https://github.com/acme"), None);
        assert_eq!(github_repo_slug(""), None);
    }

    #[test]
    fn upserts_env_variable_idempotently() {
        let temp_dir = std::env::temp_dir().join(format!("todo_md_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let env_path = temp_dir.join(".env");

        write_todo_file_atomic(&env_path, "FOO=bar\nTODOS_GIT_REMOTE=old\n").expect("write");
        upsert_env_var(&env_path, "TODOS_GIT_REMOTE", "git@github.com:acme/new.git")
            .expect("upsert");

        let content = fs::read_to_string(&env_path).expect("read");
        assert!(content.contains("FOO=bar"));
        assert!(content.contains("TODOS_GIT_REMOTE=git@github.com:acme/new.git"));
        assert!(!content.contains("TODOS_GIT_REMOTE=old"));
    }
}
