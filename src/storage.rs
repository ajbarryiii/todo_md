use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
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

pub fn validate_todo_content(content: &str) -> Vec<String> {
    let mut issues = Vec::new();
    let mut seen_ids: HashMap<Uuid, usize> = HashMap::new();
    let id_re = Regex::new(r"\(id:\s*([0-9a-fA-F-]{36})\)").expect("valid id regex");

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim_start();

        if trimmed.starts_with("<<<<<<<")
            || trimmed.starts_with("=======")
            || trimmed.starts_with(">>>>>>>")
        {
            issues.push(format!("line {line_no}: unresolved git conflict marker"));
            continue;
        }

        if !trimmed.starts_with("- [") {
            continue;
        }

        if !line.contains("(id:") {
            issues.push(format!("line {line_no}: todo line is missing required id"));
            continue;
        }

        let parsed = std::panic::catch_unwind(|| Todo::from_str(line));
        let todo = match parsed {
            Ok(todo) => todo,
            Err(_) => {
                issues.push(format!("line {line_no}: todo line could not be parsed"));
                continue;
            }
        };

        if let Some(captures) = id_re.captures(line) {
            if let Some(raw_id) = captures.get(1).map(|m| m.as_str()) {
                if let Ok(id) = Uuid::parse_str(raw_id) {
                    if let Some(previous_line) = seen_ids.insert(id, line_no) {
                        issues.push(format!(
                            "line {line_no}: duplicate id {id} (first seen on line {previous_line})"
                        ));
                    }
                    if id != todo.id() {
                        issues.push(format!(
                            "line {line_no}: parsed id mismatch, this line may be malformed"
                        ));
                    }
                }
            }
        }
    }

    issues
}

pub fn format_todo_content(content: &str) -> (String, Vec<String>) {
    let mut issues = Vec::new();
    let mut out = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim_start();
        if !trimmed.starts_with("- [") {
            out.push(line.trim_end().to_string());
            continue;
        }

        if !line.contains("(id:") {
            issues.push(format!("line {line_no}: cannot format todo without id"));
            out.push(line.trim_end().to_string());
            continue;
        }

        let parsed = std::panic::catch_unwind(|| Todo::from_str(line));
        match parsed {
            Ok(todo) => out.push(todo.to_line()),
            Err(_) => {
                issues.push(format!("line {line_no}: todo line could not be parsed"));
                out.push(line.trim_end().to_string());
            }
        }
    }

    let mut formatted = out.join("\n");
    if content.ends_with('\n') {
        formatted.push('\n');
    }

    (formatted, issues)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_missing_id_and_conflicts() {
        let input = "<<<<<<< HEAD\n- [_] Task without id\n";
        let issues = validate_todo_content(input);
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().any(|m| m.contains("conflict marker")));
        assert!(issues.iter().any(|m| m.contains("missing required id")));
    }

    #[test]
    fn formats_parsable_todo_lines() {
        let input =
            "- [_] Pay rent (reccurence: monthly on the 1st) (id: 123e4567-e89b-12d3-a456-426614174000)\n";
        let (formatted, issues) = format_todo_content(input);
        assert!(issues.is_empty());
        assert_eq!(
            formatted,
            "- [_] Pay rent (reccurence: monthly on 1st) (id: 123e4567-e89b-12d3-a456-426614174000)\n"
        );
    }
}
