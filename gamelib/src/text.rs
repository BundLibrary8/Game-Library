//! Small text-formatting helpers shared by the window and the commands.

/// "3 h 12 min", "45 min" or "20 s".
pub fn format_duration(secs: u64) -> String {
    let (hours, minutes) = (secs / 3600, secs % 3600 / 60);
    if hours > 0 {
        format!("{hours} h {minutes} min")
    } else if minutes > 0 {
        format!("{minutes} min")
    } else {
        format!("{secs} s")
    }
}

/// "today", "yesterday", "5 days ago", or "never".
pub fn format_last_played(last_played: Option<i64>, now: i64) -> String {
    let Some(then) = last_played else {
        return "never".to_string();
    };
    match (now - then).max(0) / 86_400 {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        days => format!("{days} days ago"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(20), "20 s");
        assert_eq!(format_duration(45 * 60 + 5), "45 min");
        assert_eq!(format_duration(3 * 3600 + 12 * 60), "3 h 12 min");
    }

    #[test]
    fn last_played() {
        let day = 86_400;
        assert_eq!(format_last_played(None, 100), "never");
        assert_eq!(format_last_played(Some(10 * day), 10 * day + 60), "today");
        assert_eq!(
            format_last_played(Some(10 * day), 11 * day + 60),
            "yesterday"
        );
        assert_eq!(
            format_last_played(Some(10 * day), 15 * day + 60),
            "5 days ago"
        );
    }
}
