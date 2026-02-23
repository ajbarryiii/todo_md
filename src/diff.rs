use std::collections::HashSet;

use similar::{Algorithm, TextDiff};
use uuid::Uuid;

use crate::recurrence_parser::is_rollover_due_date;
use crate::storage::ParsedTodoFile;
use crate::types::Todo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Updated,
    Deleted,
    Completed,
}

#[derive(Debug, Clone)]
pub struct TodoChange {
    pub id: Uuid,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone)]
pub struct ChangeSet {
    pub added: usize,
    pub updated: usize,
    pub deleted: usize,
    pub completed: usize,
    pub changes: Vec<TodoChange>,
}

impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.updated == 0 && self.deleted == 0 && self.completed == 0
    }
}

pub fn semantic_changes(previous: &ParsedTodoFile, current: &ParsedTodoFile) -> ChangeSet {
    let mut changes = Vec::new();
    let mut prev_ids = previous.todos_by_id.keys().copied().collect::<HashSet<_>>();
    let curr_ids = current.todos_by_id.keys().copied().collect::<HashSet<_>>();

    for id in curr_ids.difference(&prev_ids) {
        changes.push(TodoChange {
            id: *id,
            kind: ChangeKind::Added,
        });
    }

    for id in prev_ids.difference(&curr_ids) {
        changes.push(TodoChange {
            id: *id,
            kind: ChangeKind::Deleted,
        });
    }

    prev_ids.retain(|id| curr_ids.contains(id));
    for id in prev_ids {
        let Some(previous_todo) = previous.todos_by_id.get(&id) else {
            continue;
        };
        let Some(current_todo) = current.todos_by_id.get(&id) else {
            continue;
        };

        if !todos_differ(previous_todo, current_todo) {
            continue;
        }

        let kind = if is_completion_transition(previous_todo, current_todo) {
            ChangeKind::Completed
        } else {
            ChangeKind::Updated
        };

        changes.push(TodoChange { id, kind });
    }

    let added = changes
        .iter()
        .filter(|change| change.kind == ChangeKind::Added)
        .count();
    let updated = changes
        .iter()
        .filter(|change| change.kind == ChangeKind::Updated)
        .count();
    let deleted = changes
        .iter()
        .filter(|change| change.kind == ChangeKind::Deleted)
        .count();
    let completed = changes
        .iter()
        .filter(|change| change.kind == ChangeKind::Completed)
        .count();

    ChangeSet {
        added,
        updated,
        deleted,
        completed,
        changes,
    }
}

pub fn line_diff_summary(before: &str, after: &str) -> String {
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Myers)
        .diff_lines(before, after);

    let mut added = 0;
    let mut removed = 0;
    for change in diff.iter_all_changes() {
        match change.tag() {
            similar::ChangeTag::Insert => added += 1,
            similar::ChangeTag::Delete => removed += 1,
            similar::ChangeTag::Equal => {}
        }
    }

    format!("line diff (+{added}/-{removed})")
}

fn todos_differ(previous: &Todo, current: &Todo) -> bool {
    previous.done() != current.done()
        || previous.due_date() != current.due_date()
        || previous.recurence() != current.recurence()
        || previous.name() != current.name()
}

fn is_completion_transition(previous: &Todo, current: &Todo) -> bool {
    if !previous.done() && current.done() {
        return true;
    }

    let (Some(prev_recurrence), Some(prev_due), Some(curr_due)) = (
        previous.recurence(),
        previous.due_date(),
        current.due_date(),
    ) else {
        return false;
    };

    !current.done() && is_rollover_due_date(prev_due, curr_due, prev_recurrence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ParsedTodoFile;
    use std::collections::HashMap;

    #[test]
    fn classifies_added_updated_and_deleted() {
        let old = ParsedTodoFile {
            content: "".to_string(),
            todos_by_id: [
                (
                    Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").expect("id"),
                    Todo::from_str("- [_] A (id: 123e4567-e89b-12d3-a456-426614174000)"),
                ),
                (
                    Uuid::parse_str("123e4567-e89b-12d3-a456-426614174001").expect("id"),
                    Todo::from_str("- [_] B (id: 123e4567-e89b-12d3-a456-426614174001)"),
                ),
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
        };

        let new = ParsedTodoFile {
            content: "".to_string(),
            todos_by_id: [
                (
                    Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").expect("id"),
                    Todo::from_str("- [_] A changed (id: 123e4567-e89b-12d3-a456-426614174000)"),
                ),
                (
                    Uuid::parse_str("123e4567-e89b-12d3-a456-426614174002").expect("id"),
                    Todo::from_str("- [_] C (id: 123e4567-e89b-12d3-a456-426614174002)"),
                ),
            ]
            .into_iter()
            .collect::<HashMap<_, _>>(),
        };

        let changes = semantic_changes(&old, &new);
        assert_eq!(changes.added, 1);
        assert_eq!(changes.updated, 1);
        assert_eq!(changes.deleted, 1);
        assert_eq!(changes.completed, 0);
    }

    #[test]
    fn classifies_rollover_as_completion() {
        let old = ParsedTodoFile {
            content: "".to_string(),
            todos_by_id: [(
                Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").expect("id"),
                Todo::from_str(
                    "- [_] Water plants (due: 2026-02-23T14:00:00Z) (reccurence: weekly on monday, thursday) (id: 123e4567-e89b-12d3-a456-426614174000)",
                ),
            )]
            .into_iter()
            .collect::<HashMap<_, _>>(),
        };

        let new = ParsedTodoFile {
            content: "".to_string(),
            todos_by_id: [(
                Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").expect("id"),
                Todo::from_str(
                    "- [_] Water plants (due: 2026-02-26T14:00:00Z) (reccurence: weekly on monday, thursday) (id: 123e4567-e89b-12d3-a456-426614174000)",
                ),
            )]
            .into_iter()
            .collect::<HashMap<_, _>>(),
        };

        let changes = semantic_changes(&old, &new);
        assert_eq!(changes.completed, 1);
        assert_eq!(changes.updated, 0);
    }
}
