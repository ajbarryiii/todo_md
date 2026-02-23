use crate::date_parser::parse_human_datetime;
use crate::recurrence_parser::{next_due_date_utc, parse_reccurence};
use chrono::prelude::*;
use regex::Regex;
use uuid::*;

pub struct Todo {
    id: Uuid,
    done: bool,
    due_date: Option<DateTime<Utc>>,
    recurence: Option<Reccurence>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reccurence {
    Daily,
    Weekly(Vec<DaysOfWeek>),
    Monthly(Option<u32>),
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaysOfWeek {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Todo {
    pub fn new(name: String) -> Todo {
        Todo {
            id: Uuid::new_v4(),
            done: false,
            due_date: None,
            recurence: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            name: name,
        }
    }

    pub fn from_str(line: &str) -> Todo {
        let todo_regex = Regex::new(
            r"^- \[(?P<done>[x_])\] (?P<name>.+?)(?: \(due: (?P<due_date>[^)]+)\))?(?: \(reccurence: (?P<reccurence>[^)]+)\))?(?: \(id: (?P<id>[0-9a-fA-F-]{36})\))?\.?$",
        )
        .expect("todo parser regex must be valid");

        let captures = todo_regex
            .captures(line)
            .expect("todo line does not match expected format");

        let mut todo = Todo::new(captures["name"].trim().to_string());
        todo.done = &captures["done"] == "x";

        if let Some(due_date_match) = captures.name("due_date") {
            if let Some(parsed_due_date) = parse_human_datetime(due_date_match.as_str(), Utc::now())
            {
                todo.due_date = Some(parsed_due_date);
            }
        }

        if let Some(reccurence_match) = captures.name("reccurence") {
            todo.recurence = parse_reccurence(reccurence_match.as_str(), Local::now());
        }

        if let Some(id_match) = captures.name("id") {
            if let Ok(parsed_id) = Uuid::parse_str(id_match.as_str()) {
                todo.id = parsed_id;
            }
        }

        if todo.done {
            todo.complete();
        }

        todo.updated_at = Utc::now();
        todo
    }

    pub fn to_line(&self) -> String {
        let mut line = format!("- [{}] {}", if self.done { "x" } else { "_" }, self.name);

        if let Some(due_date) = self.due_date {
            line.push_str(&format!(" (due: {})", due_date.to_rfc3339()));
        }

        if let Some(reccurence) = self.recurence() {
            line.push_str(&format!(" (reccurence: {})", reccurence.as_str()));
        }

        line.push_str(&format!(" (id: {})", self.id));
        line
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn complete(&mut self) {
        if let (Some(reccurence), Some(due_date)) = (&self.recurence, self.due_date) {
            if let Some(next_due) = next_due_date_utc(due_date, reccurence) {
                self.due_date = Some(next_due);
                self.done = false;
                self.updated_at = Utc::now();
                return;
            }
        }

        self.done = true;
        self.updated_at = Utc::now();
    }

    pub fn done(&self) -> bool {
        self.done
    }

    pub fn due_date(&self) -> Option<DateTime<Utc>> {
        self.due_date
    }

    pub fn recurence(&self) -> Option<&Reccurence> {
        self.recurence.as_ref()
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }
}

impl Reccurence {
    fn as_str(&self) -> String {
        match self {
            Reccurence::Daily => "daily".to_string(),
            Reccurence::Weekly(days) => {
                if days.len() == 7 {
                    "weekly".to_string()
                } else {
                    let day_list = days
                        .iter()
                        .map(DaysOfWeek::as_str)
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("weekly on {day_list}")
                }
            }
            Reccurence::Monthly(Some(day)) => format!("monthly on {}", ordinal_day(*day)),
            Reccurence::Monthly(None) => "monthly".to_string(),
            Reccurence::Yearly => "yearly".to_string(),
        }
    }
}

fn ordinal_day(day: u32) -> String {
    let suffix = match day % 100 {
        11..=13 => "th",
        _ => match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{day}{suffix}")
}

impl DaysOfWeek {
    fn as_str(&self) -> &'static str {
        match self {
            DaysOfWeek::Monday => "monday",
            DaysOfWeek::Tuesday => "tuesday",
            DaysOfWeek::Wednesday => "wednesday",
            DaysOfWeek::Thursday => "thursday",
            DaysOfWeek::Friday => "friday",
            DaysOfWeek::Saturday => "saturday",
            DaysOfWeek::Sunday => "sunday",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_weekly_reccurence_with_days() {
        let todo = Todo::from_str(
            "- [_] Team sync (reccurence: weekly on tuesday, thursday, friday) (id: 123e4567-e89b-12d3-a456-426614174000)",
        );

        assert_eq!(
            todo.recurence(),
            Some(&Reccurence::Weekly(vec![
                DaysOfWeek::Tuesday,
                DaysOfWeek::Thursday,
                DaysOfWeek::Friday
            ]))
        );
    }

    #[test]
    fn serializes_weekly_reccurence_with_days() {
        let todo = Todo::from_str(
            "- [_] Gym (reccurence: weekly on tue, thurs) (id: 123e4567-e89b-12d3-a456-426614174000)",
        );

        let line = todo.to_line();
        assert!(line.contains("(reccurence: weekly on tuesday, thursday)"));
    }

    #[test]
    fn parses_weekly_range_reccurence() {
        let todo = Todo::from_str(
            "- [_] Build feature (reccurence: weekly on mon-fri) (id: 123e4567-e89b-12d3-a456-426614174000)",
        );

        assert_eq!(
            todo.recurence(),
            Some(&Reccurence::Weekly(vec![
                DaysOfWeek::Monday,
                DaysOfWeek::Tuesday,
                DaysOfWeek::Wednesday,
                DaysOfWeek::Thursday,
                DaysOfWeek::Friday,
            ]))
        );
    }

    #[test]
    fn completed_recurring_item_rolls_due_date_forward() {
        let todo = Todo::from_str(
            "- [x] Water plants (due: 2026-02-23T14:00:00Z) (reccurence: weekly on monday, thursday) (id: 123e4567-e89b-12d3-a456-426614174000)",
        );

        assert!(!todo.done());
        assert_eq!(
            todo.due_date().expect("due date").to_rfc3339(),
            "2026-02-26T14:00:00+00:00"
        );
    }

    #[test]
    fn complete_marks_non_recurring_item_done() {
        let mut todo = Todo::new("Write docs".to_string());
        todo.complete();
        assert!(todo.done());
    }

    #[test]
    fn complete_rolls_recurring_item_forward() {
        let mut todo = Todo::from_str(
            "- [_] Water plants (due: 2026-02-23T14:00:00Z) (reccurence: weekly on monday, thursday) (id: 123e4567-e89b-12d3-a456-426614174000)",
        );

        todo.complete();

        assert!(!todo.done());
        assert_eq!(
            todo.due_date().expect("due date").to_rfc3339(),
            "2026-02-26T14:00:00+00:00"
        );
    }

    #[test]
    fn parses_monthly_on_specific_day() {
        let todo = Todo::from_str(
            "- [_] Pay rent (reccurence: monthly on the 1st) (id: 123e4567-e89b-12d3-a456-426614174000)",
        );

        assert_eq!(todo.recurence(), Some(&Reccurence::Monthly(Some(1))));
        assert!(todo.to_line().contains("(reccurence: monthly on 1st)"));
    }
}
