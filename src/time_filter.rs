use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Utc};

/// Parse a time filter string and return a DateTime<Utc>
///
/// Supported formats:
/// - "7d" - 7 days ago
/// - "24h" - 24 hours ago
/// - "30m" - 30 minutes ago
/// - "yesterday" - start of yesterday
/// - "today" - start of today
/// - "this-week" - start of current week (Monday)
/// - "this-month" - start of current month
pub fn parse_time_filter(input: &str) -> Result<DateTime<Utc>> {
    let input = input.trim().to_lowercase();

    // Handle special keywords
    match input.as_str() {
        "yesterday" => {
            let yesterday = Local::now().date_naive() - Duration::days(1);
            let start_of_day = yesterday.and_time(NaiveTime::MIN);
            return Ok(Local
                .from_local_datetime(&start_of_day)
                .single()
                .ok_or_else(|| anyhow!("Invalid date/time"))?
                .with_timezone(&Utc));
        }
        "today" => {
            let today = Local::now().date_naive();
            let start_of_day = today.and_time(NaiveTime::MIN);
            return Ok(Local
                .from_local_datetime(&start_of_day)
                .single()
                .ok_or_else(|| anyhow!("Invalid date/time"))?
                .with_timezone(&Utc));
        }
        "this-week" => {
            let now = Local::now();
            let days_since_monday = now.weekday().num_days_from_monday() as i64;
            let monday = now.date_naive() - Duration::days(days_since_monday);
            let start_of_day = monday.and_time(NaiveTime::MIN);
            return Ok(Local
                .from_local_datetime(&start_of_day)
                .single()
                .ok_or_else(|| anyhow!("Invalid date/time"))?
                .with_timezone(&Utc));
        }
        "this-month" => {
            let now = Local::now();
            let first_of_month = now
                .date_naive()
                .with_day(1)
                .ok_or_else(|| anyhow!("Invalid date"))?;
            let start_of_day = first_of_month.and_time(NaiveTime::MIN);
            return Ok(Local
                .from_local_datetime(&start_of_day)
                .single()
                .ok_or_else(|| anyhow!("Invalid date/time"))?
                .with_timezone(&Utc));
        }
        _ => {}
    }

    // Parse duration format (e.g., "7d", "24h", "30m")
    if let Some(duration) = parse_duration(&input) {
        let now = Utc::now();
        return Ok(now - duration);
    }

    Err(anyhow!(
        "Invalid time filter: '{}'. Use formats like: 7d, 24h, 30m, yesterday, today, this-week, this-month",
        input
    ))
}

fn parse_duration(input: &str) -> Option<Duration> {
    let input = input.trim();

    if input.is_empty() {
        return None;
    }

    // Find where the number ends and unit begins
    let (num_str, unit) = input
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map(|(i, _)| (&input[..i], &input[i..]))
        .unwrap_or((input, ""));

    let num: i64 = num_str.parse().ok()?;

    match unit {
        "d" | "day" | "days" => Some(Duration::days(num)),
        "h" | "hour" | "hours" => Some(Duration::hours(num)),
        "m" | "min" | "mins" | "minute" | "minutes" => Some(Duration::minutes(num)),
        "w" | "week" | "weeks" => Some(Duration::weeks(num)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_days() {
        let result = parse_time_filter("7d").unwrap();
        let expected = Utc::now() - Duration::days(7);
        // Allow 1 second tolerance
        assert!((result - expected).num_seconds().abs() < 1);
    }

    #[test]
    fn test_parse_hours() {
        let result = parse_time_filter("24h").unwrap();
        let expected = Utc::now() - Duration::hours(24);
        assert!((result - expected).num_seconds().abs() < 1);
    }

    #[test]
    fn test_parse_minutes() {
        let result = parse_time_filter("30m").unwrap();
        let expected = Utc::now() - Duration::minutes(30);
        assert!((result - expected).num_seconds().abs() < 1);
    }

    #[test]
    fn test_invalid_filter() {
        assert!(parse_time_filter("invalid").is_err());
    }
}
