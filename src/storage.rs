use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::types::Todo;

#[derive(Debug, Clone)]
pub struct ParsedTodoFile {
    pub content: String,
    pub todos_by_id: HashMap<Uuid, Todo>,
}

pub fn ensure_layout(config_dir: &Path, todo_file: &Path, env_file: &Path) -> Result<()> {
    fs::create_dir_all(config_dir)
        .with_context(|| format!("failed to create {}", config_dir.display()))?;

    if !todo_file.exists() {
        fs::File::create(todo_file)
            .with_context(|| format!("failed to create {}", todo_file.display()))?;
    }

    if !env_file.exists() {
        fs::File::create(env_file)
            .with_context(|| format!("failed to create {}", env_file.display()))?;
    }

    let gitignore = config_dir.join(".gitignore");
    ensure_gitignore_has_env(&gitignore)?;
    Ok(())
}

pub fn read_todo_file(path: &Path) -> Result<ParsedTodoFile> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let todos_by_id = parse_todos_from_content(&content);

    Ok(ParsedTodoFile {
        content,
        todos_by_id,
    })
}

pub fn parse_todo_content(content: &str) -> ParsedTodoFile {
    ParsedTodoFile {
        content: content.to_string(),
        todos_by_id: parse_todos_from_content(content),
    }
}

pub fn write_todo_file_atomic(path: &Path, content: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    let temp_path = parent.join("todo.md.tmp");

    {
        let mut file = fs::File::create(&temp_path)
            .with_context(|| format!("failed to create {}", temp_path.display()))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to fsync {}", temp_path.display()))?;
    }

    fs::rename(&temp_path, path)
        .with_context(|| format!("failed to replace {}", path.display()))?;

    Ok(())
}

fn parse_todos_from_content(content: &str) -> HashMap<Uuid, Todo> {
    let mut todos = HashMap::new();
    for line in content.lines() {
        if !line.trim_start().starts_with("- [") {
            continue;
        }

        if let Ok(todo) = std::panic::catch_unwind(|| Todo::from_str(line)) {
            todos.insert(todo.id(), todo);
        }
    }

    todos
}

fn ensure_gitignore_has_env(gitignore_path: &Path) -> Result<()> {
    let mut content = if gitignore_path.exists() {
        fs::read_to_string(gitignore_path)
            .with_context(|| format!("failed to read {}", gitignore_path.display()))?
    } else {
        String::new()
    };

    if !content.lines().any(|line| line.trim() == ".env") {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(".env\n");
        write_todo_file_atomic(gitignore_path, &content)?;
    }

    Ok(())
}
