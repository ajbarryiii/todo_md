use chrono::prelude::*;
use chrono::LocalResult;
use chrono::{Datelike, Duration, Months, NaiveDate, TimeZone};
use regex::Regex;
use strsim::normalized_levenshtein;

use crate::types::{DaysOfWeek, Reccurence};

pub fn parse_reccurence(raw: &str, now_local: DateTime<Local>) -> Option<Reccurence> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized == "daily" {
        return Some(Reccurence::Daily);
    }
    if normalized == "monthly" {
        return Some(Reccurence::Monthly(None));
    }
    if normalized == "yearly" {
        return Some(Reccurence::Yearly);
    }
    if normalized == "weekly" {
        return Some(Reccurence::Weekly(vec![from_chrono_weekday(
            now_local.weekday(),
        )]));
    }

    let weekly_prefix = "weekly on ";
    if let Some(days_part) = normalized.strip_prefix(weekly_prefix) {
        return parse_weekly_days(days_part).map(Reccurence::Weekly);
    }

    let monthly_prefix = "monthly on ";
    if let Some(day_part) = normalized.strip_prefix(monthly_prefix) {
        return parse_monthly_day(day_part).map(|day| Reccurence::Monthly(Some(day)));
    }

    None
}

pub fn next_due_date_utc(
    due_date: DateTime<Utc>,
    recurrence: &Reccurence,
) -> Option<DateTime<Utc>> {
    let due_local = due_date.with_timezone(&Local);
    let naive_due = due_local.naive_local();
    let next_naive = next_due_naive(naive_due, recurrence)?;
    let next_local = localize(next_naive)?;
    Some(next_local.with_timezone(&Utc))
}

pub fn is_rollover_due_date(
    previous_due: DateTime<Utc>,
    current_due: DateTime<Utc>,
    recurrence: &Reccurence,
) -> bool {
    next_due_date_utc(previous_due, recurrence)
        .map(|next| next == current_due)
        .unwrap_or(false)
}

fn next_due_naive(due: NaiveDateTime, recurrence: &Reccurence) -> Option<NaiveDateTime> {
    match recurrence {
        Reccurence::Daily => Some(due + Duration::days(1)),
        Reccurence::Weekly(days) => Some(next_weekly_due(due, days)),
        Reccurence::Monthly(Some(day)) => {
            let next_date = add_months_on_day(due.date(), 1, *day)?;
            Some(next_date.and_time(due.time()))
        }
        Reccurence::Monthly(None) => {
            let next_date = add_months_clamped(due.date(), 1)?;
            Some(next_date.and_time(due.time()))
        }
        Reccurence::Yearly => {
            let next_date = add_years_clamped(due.date(), 1)?;
            Some(next_date.and_time(due.time()))
        }
    }
}

fn parse_monthly_day(raw: &str) -> Option<u32> {
    let cleaned = raw.trim().trim_start_matches("the ");
    let day_re = Regex::new(r"^(?P<day>\d{1,2})(?:st|nd|rd|th)?$").expect("monthly day regex");
    let captures = day_re.captures(cleaned)?;
    let day: u32 = captures.name("day")?.as_str().parse().ok()?;
    if (1..=31).contains(&day) {
        Some(day)
    } else {
        None
    }
}

fn next_weekly_due(due: NaiveDateTime, days: &[DaysOfWeek]) -> NaiveDateTime {
    let mut day_indexes = days.iter().map(|d| weekday_number(*d)).collect::<Vec<_>>();
    if day_indexes.is_empty() {
        return due + Duration::days(7);
    }
    day_indexes.sort_unstable();
    day_indexes.dedup();

    let current_idx = due.weekday().number_from_monday();
    let mut next_delta = 7_i64;
    for idx in day_indexes {
        let mut delta = ((idx + 7 - current_idx) % 7) as i64;
        if delta == 0 {
            delta = 7;
        }
        if delta < next_delta {
            next_delta = delta;
        }
    }

    due + Duration::days(next_delta)
}

fn add_months_clamped(date: NaiveDate, months: u32) -> Option<NaiveDate> {
    let first_of_month = date.with_day(1)?;
    let target_month_first = first_of_month.checked_add_months(Months::new(months))?;
    let last_day = last_day_of_month(target_month_first.year(), target_month_first.month())?;
    let day = date.day().min(last_day);
    NaiveDate::from_ymd_opt(target_month_first.year(), target_month_first.month(), day)
}

fn add_months_on_day(date: NaiveDate, months: u32, day_of_month: u32) -> Option<NaiveDate> {
    let first_of_month = date.with_day(1)?;
    let target_month_first = first_of_month.checked_add_months(Months::new(months))?;
    let last_day = last_day_of_month(target_month_first.year(), target_month_first.month())?;
    let day = day_of_month.min(last_day);
    NaiveDate::from_ymd_opt(target_month_first.year(), target_month_first.month(), day)
}

fn add_years_clamped(date: NaiveDate, years: i32) -> Option<NaiveDate> {
    let target_year = date.year() + years;
    let last_day = last_day_of_month(target_year, date.month())?;
    let day = date.day().min(last_day);
    NaiveDate::from_ymd_opt(target_year, date.month(), day)
}

fn last_day_of_month(year: i32, month: u32) -> Option<u32> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let next = first.with_day(1)?.checked_add_months(Months::new(1))?;
    Some((next - Duration::days(1)).day())
}

fn localize(naive: NaiveDateTime) -> Option<DateTime<Local>> {
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(a, b) => Some(a.min(b)),
        LocalResult::None => None,
    }
}

fn parse_weekly_days(raw: &str) -> Option<Vec<DaysOfWeek>> {
    let mut days = Vec::new();
    let normalized = raw.replace(" and ", ",");

    for token in normalized
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        for day in parse_day_group(token)? {
            if !days.contains(&day) {
                days.push(day);
            }
        }
    }

    if days.is_empty() {
        None
    } else {
        Some(days)
    }
}

fn parse_day_group(token: &str) -> Option<Vec<DaysOfWeek>> {
    if let Some((start, end)) = token.split_once('-') {
        let start_day = parse_day_of_week(start.trim())?;
        let end_day = parse_day_of_week(end.trim())?;
        return Some(expand_day_range(start_day, end_day));
    }

    Some(vec![parse_day_of_week(token)?])
}

fn expand_day_range(start: DaysOfWeek, end: DaysOfWeek) -> Vec<DaysOfWeek> {
    let start_idx = day_index(start);
    let end_idx = day_index(end);
    let mut days = Vec::new();

    let mut idx = start_idx;
    loop {
        days.push(day_from_index(idx));
        if idx == end_idx {
            break;
        }
        idx = (idx + 1) % 7;
    }

    days
}

fn day_index(day: DaysOfWeek) -> usize {
    match day {
        DaysOfWeek::Monday => 0,
        DaysOfWeek::Tuesday => 1,
        DaysOfWeek::Wednesday => 2,
        DaysOfWeek::Thursday => 3,
        DaysOfWeek::Friday => 4,
        DaysOfWeek::Saturday => 5,
        DaysOfWeek::Sunday => 6,
    }
}

fn weekday_number(day: DaysOfWeek) -> u32 {
    match day {
        DaysOfWeek::Monday => 1,
        DaysOfWeek::Tuesday => 2,
        DaysOfWeek::Wednesday => 3,
        DaysOfWeek::Thursday => 4,
        DaysOfWeek::Friday => 5,
        DaysOfWeek::Saturday => 6,
        DaysOfWeek::Sunday => 7,
    }
}

fn day_from_index(index: usize) -> DaysOfWeek {
    match index {
        0 => DaysOfWeek::Monday,
        1 => DaysOfWeek::Tuesday,
        2 => DaysOfWeek::Wednesday,
        3 => DaysOfWeek::Thursday,
        4 => DaysOfWeek::Friday,
        5 => DaysOfWeek::Saturday,
        _ => DaysOfWeek::Sunday,
    }
}

fn parse_day_of_week(raw: &str) -> Option<DaysOfWeek> {
    let token = raw.trim().to_ascii_lowercase();
    let aliases = [
        ("monday", DaysOfWeek::Monday),
        ("mon", DaysOfWeek::Monday),
        ("tuesday", DaysOfWeek::Tuesday),
        ("tue", DaysOfWeek::Tuesday),
        ("tues", DaysOfWeek::Tuesday),
        ("wednesday", DaysOfWeek::Wednesday),
        ("wed", DaysOfWeek::Wednesday),
        ("thursday", DaysOfWeek::Thursday),
        ("thu", DaysOfWeek::Thursday),
        ("thur", DaysOfWeek::Thursday),
        ("thurs", DaysOfWeek::Thursday),
        ("friday", DaysOfWeek::Friday),
        ("fri", DaysOfWeek::Friday),
        ("saturday", DaysOfWeek::Saturday),
        ("sat", DaysOfWeek::Saturday),
        ("sunday", DaysOfWeek::Sunday),
        ("sun", DaysOfWeek::Sunday),
    ];

    if let Some((_, day)) = aliases.iter().find(|(name, _)| *name == token) {
        return Some(*day);
    }

    let mut best: Option<DaysOfWeek> = None;
    let mut best_score = 0.0;
    for (name, day) in aliases {
        let score = normalized_levenshtein(&token, name);
        if score > best_score {
            best_score = score;
            best = Some(day);
        }
    }

    if best_score >= 0.72 {
        best
    } else {
        None
    }
}

fn from_chrono_weekday(day: Weekday) -> DaysOfWeek {
    match day {
        Weekday::Mon => DaysOfWeek::Monday,
        Weekday::Tue => DaysOfWeek::Tuesday,
        Weekday::Wed => DaysOfWeek::Wednesday,
        Weekday::Thu => DaysOfWeek::Thursday,
        Weekday::Fri => DaysOfWeek::Friday,
        Weekday::Sat => DaysOfWeek::Saturday,
        Weekday::Sun => DaysOfWeek::Sunday,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    fn fixed_local() -> DateTime<Local> {
        FixedOffset::east_opt(0)
            .expect("valid offset")
            .with_ymd_and_hms(2026, 2, 23, 12, 0, 0)
            .single()
            .expect("valid dt")
            .with_timezone(&Local)
    }

    #[test]
    fn parses_weekly_range() {
        let parsed = parse_reccurence("weekly on mon-fri", fixed_local()).expect("valid parse");
        assert_eq!(
            parsed,
            Reccurence::Weekly(vec![
                DaysOfWeek::Monday,
                DaysOfWeek::Tuesday,
                DaysOfWeek::Wednesday,
                DaysOfWeek::Thursday,
                DaysOfWeek::Friday
            ])
        );
    }

    #[test]
    fn parses_wrapping_weekly_range() {
        let parsed = parse_reccurence("weekly on fri-mon", fixed_local()).expect("valid parse");
        assert_eq!(
            parsed,
            Reccurence::Weekly(vec![
                DaysOfWeek::Friday,
                DaysOfWeek::Saturday,
                DaysOfWeek::Sunday,
                DaysOfWeek::Monday
            ])
        );
    }

    #[test]
    fn defaults_plain_weekly_to_local_today() {
        let monday_noon = FixedOffset::east_opt(0)
            .expect("valid offset")
            .with_ymd_and_hms(2026, 2, 23, 12, 0, 0)
            .single()
            .expect("valid dt")
            .with_timezone(&Local);

        let parsed = parse_reccurence("weekly", monday_noon).expect("valid parse");
        assert_eq!(parsed, Reccurence::Weekly(vec![DaysOfWeek::Monday]));
    }

    #[test]
    fn parses_monthly_with_ordinal_day() {
        let parsed = parse_reccurence("monthly on the 18th", fixed_local()).expect("valid parse");
        assert_eq!(parsed, Reccurence::Monthly(Some(18)));
    }

    #[test]
    fn parses_monthly_with_short_ordinal_day() {
        let parsed = parse_reccurence("monthly on 1st", fixed_local()).expect("valid parse");
        assert_eq!(parsed, Reccurence::Monthly(Some(1)));
    }

    #[test]
    fn advances_weekly_due_to_next_selected_day() {
        let due = DateTime::parse_from_rfc3339("2026-02-23T14:00:00Z")
            .expect("valid due")
            .with_timezone(&Utc);
        let recurrence = Reccurence::Weekly(vec![DaysOfWeek::Monday, DaysOfWeek::Thursday]);

        let next = next_due_date_utc(due, &recurrence).expect("next due");
        assert_eq!(next.to_rfc3339(), "2026-02-26T14:00:00+00:00");
    }

    #[test]
    fn advances_monthly_with_month_end_clamp() {
        let due = DateTime::parse_from_rfc3339("2026-01-31T10:30:00Z")
            .expect("valid due")
            .with_timezone(&Utc);

        let next = next_due_date_utc(due, &Reccurence::Monthly(None)).expect("next due");
        assert_eq!(next.to_rfc3339(), "2026-02-28T10:30:00+00:00");
    }

    #[test]
    fn advances_monthly_specific_day_with_clamp() {
        let due = DateTime::parse_from_rfc3339("2026-01-18T10:30:00Z")
            .expect("valid due")
            .with_timezone(&Utc);

        let next = next_due_date_utc(due, &Reccurence::Monthly(Some(31))).expect("next due");
        assert_eq!(next.to_rfc3339(), "2026-02-28T10:30:00+00:00");
    }
}
