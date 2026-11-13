//! Small text-formatting helpers shared by the window and the commands.

/// "3 h 12 min", "45 min" or "20 s". Used by the command-line output.
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

/// A compact playtime: "41h", "35m", or "0m".
pub fn short_playtime(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}m", secs / 60)
    }
}

/// The line under a game card: "41h played" or "Never played".
pub fn played_label(secs: u64) -> String {
    if secs == 0 {
        "Never played".to_string()
    } else if secs < 60 {
        "Under a minute played".to_string()
    } else {
        format!("{} played", short_playtime(secs))
    }
}

/// An exact playtime for the details page: "112h 05m".
pub fn long_playtime(secs: u64) -> String {
    format!("{}h {:02}m", secs / 3600, secs % 3600 / 60)
}

/// "Today", "Yesterday", "5 days ago", "Last week", "3 weeks ago",
/// "4 months ago" or "Never".
pub fn format_last_played(last_played: Option<i64>, now: i64) -> String {
    let Some(then) = last_played else {
        return "Never".to_string();
    };
    match (now - then).max(0) / 86_400 {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        days @ 2..=6 => format!("{days} days ago"),
        7..=13 => "Last week".to_string(),
        days @ 14..=59 => format!("{} weeks ago", days / 7),
        days => format!("{} months ago", days / 30),
    }
}

/// "5 min ago", "3h ago", "2 days ago", for "updated … ago".
pub fn format_ago(then: i64, now: i64) -> String {
    let secs = (now - then).max(0);
    if secs < 90 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 2 * 86_400 {
        "1 day ago".to_string()
    } else {
        format!("{} days ago", secs / 86_400)
    }
}

/// 1240 → "1,240".
pub fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
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
    fn playtimes() {
        assert_eq!(played_label(0), "Never played");
        assert_eq!(played_label(30), "Under a minute played");
        assert_eq!(played_label(35 * 60), "35m played");
        assert_eq!(played_label(41 * 3600 + 50), "41h played");
        assert_eq!(long_playtime(112 * 3600 + 5 * 60), "112h 05m");
        assert_eq!(long_playtime(59), "0h 00m");
    }

    #[test]
    fn last_played() {
        let day = 86_400;
        let at = |days: i64| format_last_played(Some(100 * day), (100 + days) * day + 60);
        assert_eq!(format_last_played(None, 100), "Never");
        assert_eq!(at(0), "Today");
        assert_eq!(at(1), "Yesterday");
        assert_eq!(at(5), "5 days ago");
        assert_eq!(at(9), "Last week");
        assert_eq!(at(21), "3 weeks ago");
        assert_eq!(at(120), "4 months ago");
    }

    #[test]
    fn ago() {
        assert_eq!(format_ago(1000, 1030), "just now");
        assert_eq!(format_ago(1000, 1000 + 300), "5 min ago");
        assert_eq!(format_ago(1000, 1000 + 3 * 3600), "3h ago");
        assert_eq!(format_ago(1000, 1000 + 2 * 86_400), "2 days ago");
    }

    #[test]
    fn thousands_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1240), "1,240");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(thousands(-1500), "-1,500");
    }
}
