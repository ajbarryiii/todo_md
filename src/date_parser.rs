use chrono::prelude::*;
use chrono::{Duration, FixedOffset, NaiveDate, NaiveTime, TimeZone};
use regex::Regex;
use strsim::normalized_levenshtein;

pub fn parse_human_datetime(input: &str, now_utc: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(input.trim()) {
        return Some(parsed.with_timezone(&Utc));
    }

    let home_tz = Local::now().offset().fix();
    parse_human_datetime_with_tz(input, now_utc, home_tz)
}

fn parse_human_datetime_with_tz(
    input: &str,
    now_utc: DateTime<Utc>,
    home_tz: FixedOffset,
) -> Option<DateTime<Utc>> {
    let normalized = normalize_input(input);
    if normalized.is_empty() {
        return None;
    }

    let (value_without_tz, tz) = split_timezone_suffix(&normalized, home_tz);
    let now_local = now_utc.with_timezone(&tz);

    let (hour, minute, has_time) = parse_time(&value_without_tz).unwrap_or((23, 59, false));
    let target_date = resolve_date(
        &value_without_tz,
        now_local.date_naive(),
        now_local.time(),
        has_time,
        hour,
        minute,
    )?;

    let local_naive = target_date.and_time(NaiveTime::from_hms_opt(hour, minute, 0)?);
    let local_dt = tz.from_local_datetime(&local_naive).single()?;
    Some(local_dt.with_timezone(&Utc))
}

fn normalize_input(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace('.', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_timezone_suffix(value: &str, home_tz: FixedOffset) -> (String, FixedOffset) {
    let tz_re = Regex::new(r"^(?P<rest>.*?)(?:\s+(?P<tz>utc|gmt|z|[+-]\d{2}:?\d{2}|[a-z]{2,8}))$")
        .expect("timezone parser regex must be valid");

    let Some(captures) = tz_re.captures(value) else {
        return (value.to_string(), home_tz);
    };

    let tz_raw = captures.name("tz").map(|m| m.as_str()).unwrap_or_default();
    let Some(tz) = parse_timezone_token(tz_raw) else {
        return (value.to_string(), home_tz);
    };
    let rest = captures
        .name("rest")
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| value.to_string());

    (rest, tz)
}

fn parse_timezone_token(token: &str) -> Option<FixedOffset> {
    let canonical_utc = fuzzy_match(token, &["utc", "gmt", "z"]);
    if canonical_utc.is_some() {
        return FixedOffset::east_opt(0);
    }

    let offset_re =
        Regex::new(r"^(?P<sign>[+-])(?P<h>\d{2}):?(?P<m>\d{2})$").expect("offset regex");
    let captures = offset_re.captures(token)?;

    let sign = if &captures["sign"] == "+" { 1 } else { -1 };
    let hours: i32 = captures["h"].parse().ok()?;
    let minutes: i32 = captures["m"].parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }

    let seconds = sign * (hours * 3600 + minutes * 60);
    FixedOffset::east_opt(seconds)
}

fn parse_time(value: &str) -> Option<(u32, u32, bool)> {
    let with_meridiem = Regex::new(r"\b(?P<h>\d{1,2})(?::(?P<m>\d{2}))?\s*(?P<ampm>am|pm)\b")
        .expect("time regex with meridiem");
    if let Some(captures) = with_meridiem.captures(value) {
        let mut hour: u32 = captures.name("h")?.as_str().parse().ok()?;
        let minute: u32 = captures
            .name("m")
            .map_or(Some(0_u32), |m| m.as_str().parse::<u32>().ok())?;
        if hour == 0 || hour > 12 || minute > 59 {
            return None;
        }

        let meridiem = captures.name("ampm")?.as_str();
        if meridiem == "am" {
            if hour == 12 {
                hour = 0;
            }
        } else if hour != 12 {
            hour += 12;
        }

        return Some((hour, minute, true));
    }

    let twenty_four = Regex::new(r"\b(?P<h>\d{1,2})(?::(?P<m>\d{2}))\b").expect("24 hour regex");
    let captures = twenty_four.captures(value)?;
    let hour: u32 = captures.name("h")?.as_str().parse().ok()?;
    let minute: u32 = captures.name("m")?.as_str().parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }

    Some((hour, minute, true))
}

fn resolve_date(
    value: &str,
    base_date: NaiveDate,
    now_time: NaiveTime,
    has_time: bool,
    hour: u32,
    minute: u32,
) -> Option<NaiveDate> {
    let tokens = Regex::new(r"[a-z]+")
        .expect("token regex")
        .find_iter(value)
        .map(|m| m.as_str().to_string())
        .collect::<Vec<_>>();

    let mut date_keyword: Option<String> = None;
    for token in tokens {
        if let Some(keyword) = fuzzy_match(
            &token,
            &[
                "today",
                "tomorrow",
                "monday",
                "tuesday",
                "wednesday",
                "thursday",
                "friday",
                "saturday",
                "sunday",
            ],
        ) {
            date_keyword = Some(keyword.to_string());
            break;
        }
    }

    let requested_time = NaiveTime::from_hms_opt(hour, minute, 0)?;
    let date = match date_keyword.as_deref() {
        Some("today") => base_date,
        Some("tomorrow") => base_date + Duration::days(1),
        Some(day_name) => {
            let target_weekday = day_name_to_num(day_name)?;
            let current_weekday = base_date.weekday().number_from_monday() as i64;
            let mut delta_days = (target_weekday - current_weekday + 7) % 7;
            if delta_days == 0 && (!has_time || requested_time <= now_time) {
                delta_days = 7;
            }
            base_date + Duration::days(delta_days)
        }
        None => {
            if has_time {
                if requested_time <= now_time {
                    base_date + Duration::days(1)
                } else {
                    base_date
                }
            } else {
                base_date
            }
        }
    };

    Some(date)
}

fn day_name_to_num(day: &str) -> Option<i64> {
    match day {
        "monday" => Some(1),
        "tuesday" => Some(2),
        "wednesday" => Some(3),
        "thursday" => Some(4),
        "friday" => Some(5),
        "saturday" => Some(6),
        "sunday" => Some(7),
        _ => None,
    }
}

fn fuzzy_match<'a>(input: &str, choices: &'a [&'a str]) -> Option<&'a str> {
    let normalized = input.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if let Some(exact) = choices.iter().copied().find(|choice| *choice == normalized) {
        return Some(exact);
    }

    let mut best_choice = None;
    let mut best_score = 0.0;
    for choice in choices {
        let score = normalized_levenshtein(&normalized, choice);
        if score > best_score {
            best_score = score;
            best_choice = Some(*choice);
        }
    }

    if best_score >= 0.72 {
        best_choice
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_utc() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-02-23T18:00:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }

    fn et() -> FixedOffset {
        FixedOffset::west_opt(5 * 3600).expect("valid offset")
    }

    #[test]
    fn parses_today_keyword() {
        let dt = parse_human_datetime_with_tz("today", now_utc(), et()).expect("parse today");
        assert_eq!(dt.to_rfc3339(), "2026-02-24T04:59:00+00:00");
    }

    #[test]
    fn parses_tomorrow_typo() {
        let dt = parse_human_datetime_with_tz("tomorow", now_utc(), et()).expect("parse tomorrow");
        assert_eq!(dt.to_rfc3339(), "2026-02-25T04:59:00+00:00");
    }

    #[test]
    fn parses_weekday_typo() {
        let dt = parse_human_datetime_with_tz("tuesdy", now_utc(), et()).expect("parse weekday");
        assert_eq!(dt.to_rfc3339(), "2026-02-25T04:59:00+00:00");
    }

    #[test]
    fn parses_time_with_spacing_variants() {
        let a = parse_human_datetime_with_tz("9:00PM", now_utc(), et()).expect("parse A");
        let b = parse_human_datetime_with_tz("9:00 pm", now_utc(), et()).expect("parse B");
        let c = parse_human_datetime_with_tz("9:00pm", now_utc(), et()).expect("parse C");
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(a.to_rfc3339(), "2026-02-24T02:00:00+00:00");
    }

    #[test]
    fn parses_time_with_utc_suffix() {
        let dt = parse_human_datetime_with_tz("9:00PM UTC", now_utc(), et()).expect("parse UTC");
        assert_eq!(dt.to_rfc3339(), "2026-02-23T21:00:00+00:00");
    }
}
