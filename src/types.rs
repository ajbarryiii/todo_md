use crate::date_parser::parse_human_datetime;
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

pub enum Reccurence {
    Daily,
    Weekly([DaysOfWeek; 7]),
    Monthly,
    Yearly,
}

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
            r"^- \[(?P<done>[x_])\] (?P<name>.+?)(?: \(due: (?P<due_date>[^)]+)\))?(?: \(reccurence: (?P<reccurence>[^)]+)\))?\.?$",
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
            let parsed_reccurence = match reccurence_match
                .as_str()
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "daily" => Some(Reccurence::Daily),
                "weekly" => Some(Reccurence::Weekly([
                    DaysOfWeek::Monday,
                    DaysOfWeek::Tuesday,
                    DaysOfWeek::Wednesday,
                    DaysOfWeek::Thursday,
                    DaysOfWeek::Friday,
                    DaysOfWeek::Saturday,
                    DaysOfWeek::Sunday,
                ])),
                "monthly" => Some(Reccurence::Monthly),
                "yearly" => Some(Reccurence::Yearly),
                _ => None,
            };

            todo.recurence = parsed_reccurence;
        }

        todo.updated_at = Utc::now();
        todo
    }

    pub fn id(&self) -> Uuid {
        self.id
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
