// just formats the size by like B, KiB, MiB, and such
pub(super) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

// now this one does the same but for time rmeaining (i wonder how i can reuse this and the one above)
pub(super) fn format_duration(seconds: Option<u64>) -> String {
    let Some(seconds) = seconds else {
        return String::from("N/A");
    };

    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remaining_seconds = seconds % 60;

    if days > 0 {
        format!("{days}D {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {remaining_seconds}s")
    } else {
        format!("{remaining_seconds}s")
    }
}

// Tests for duration formatting
#[cfg(test)]
mod tests {
    use super::format_duration;

    #[test]
    fn formats_unknown_duration() {
        assert_eq!(format_duration(None), "N/A");
    }

    #[test]
    fn formats_seconds() {
        assert_eq!(format_duration(Some(59)), "59s");
    }

    #[test]
    fn formats_minutes_and_seconds() {
        assert_eq!(format_duration(Some(125)), "2m 5s");
    }

    #[test]
    fn formats_hours_and_minutes() {
        assert_eq!(format_duration(Some(3661)), "1h 1m");
    }

    #[test]
    fn formats_days_and_hours() {
        assert_eq!(format_duration(Some(90_061)), "1D 1h");
    }
}
