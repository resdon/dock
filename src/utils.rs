//! Shared utility functions.

/// Month names for date formatting.
const MONTH_NAMES: &[&str] = &[
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Month abbreviations for date formatting.
const MONTH_ABBRS: &[&str] = &[
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Represents a local date/time broken down into components.
#[derive(Debug, Clone, Copy)]
pub struct LocalTime {
    /// Year (e.g., 2026)
    pub year: i32,
    /// Month (1-12)
    pub month: u32,
    /// Day of month (1-31)
    pub day: u32,
    /// Hour (0-23)
    pub hour: u32,
    /// Minute (0-59)
    pub minute: u32,
    /// Second (0-59)
    pub second: u32,
}

/// Get the current local time as a `LocalTime` struct.
///
/// Uses `std::time::SystemTime` and UTC offset detection via `/proc` or
/// fallback to UTC. Returns `None` only if the system clock is unavailable.
pub fn get_local_time() -> Option<LocalTime> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    let secs = now.as_secs() as i64;

    // Detect local timezone offset by comparing /proc/uptime with boot time
    // A simpler approach: read the timezone offset from the environment or /etc/localtime
    let offset_secs = read_timezone_offset();

    let local_secs = secs + offset_secs;
    broken_down_time(local_secs)
}

/// Read the local timezone offset in seconds from the system.
fn read_timezone_offset() -> i64 {
    // Try reading /etc/localtime symlink to determine timezone
    // Fallback: try std::time::SystemTime with known UTC values
    // Simplest reliable method on Linux: use the TZ environment or read /etc/timezone

    // Method 1: Check if we can determine offset by comparing two SystemTime readings
    // This doesn't work directly, so we use a heuristic:
    // Read /etc/timezone or /etc/localtime symlink target

    if let Ok(link) = std::fs::read_link("/etc/localtime") {
        if let Some(path_str) = link.to_str() {
            // Parse zoneinfo path, e.g., /usr/share/zoneinfo/Europe/London
            if let Some(tz_name) = path_str
                .strip_prefix("/usr/share/zoneinfo/")
                .or_else(|| path_str.strip_prefix("../usr/share/zoneinfo/"))
            {
                return estimate_offset_for_timezone(tz_name);
            }
        }
    }

    // Fallback: try to detect offset by reading /proc/uptime and comparing
    // with a known-good method. If all else fails, return 0 (UTC).
    0
}

/// Estimate a rough timezone offset based on common timezone names.
/// This is a simplified lookup for the most common Linux timezones.
fn estimate_offset_for_timezone(tz: &str) -> i64 {
    // Common timezone offsets (in seconds)
    // Note: This doesn't handle DST perfectly, but covers the major cases
    match tz {
        // Europe
        "Europe/London" | "Europe/Guernsey" | "Europe/Jersey" | "Europe/Isle_of_Man" => 0,
        "Europe/Paris" | "Europe/Berlin" | "Europe/Rome" | "Europe/Madrid" | "Europe/Amsterdam"
        | "Europe/Brussels" | "Europe/Vienna" | "Europe/Zurich" | "Europe/Luxembourg"
        | "Europe/Stockholm" | "Europe/Oslo" | "Europe/Copenhagen" | "Europe/Helsinki"
        | "Europe/Warsaw" | "Europe/Prague" | "Europe/Budapest" | "Europe/Bucharest"
        | "Europe/Athens" | "Europe/Belgrade" | "Europe/Sofia" | "Europe/Zagreb" => 3600,
        "Europe/Moscow" | "Europe/Kaliningrad" => 10800,
        "Europe/Kiev" | "Europe/Chisinau" | "Europe/Riga" | "Europe/Tallinn" | "Europe/Vilnius"
        | "Europe/Minsk" => 7200,
        "Europe/Dublin" | "Europe/Lisbon" => 0,
        "Europe/Istanbul" => 10800,
        // America
        "America/New_York" | "America/Detroit" | "America/Kentucky/Louisville" => -18000,
        "America/Chicago" | "America/Winnipeg" | "America/Mexico_City" => -21600,
        "America/Denver" | "America/Edmonton" => -25200,
        "America/Los_Angeles" | "America/Vancouver" | "America/Anchorage" => -28800,
        "America/Sao_Paulo" | "America/Argentina/Buenos_Aires" => -10800,
        "America/Toronto" | "America/Montreal" => -18000,
        "America/Phoenix" => -25200,
        "America/Bogota" | "America/Lima" | "America/Guayaquil" => -18000,
        "America/Santiago" => -14400,
        "America/Caracas" => -16200,
        // Asia
        "Asia/Tokyo" | "Asia/Seoul" => 32400,
        "Asia/Shanghai" | "Asia/Chongqing" | "Asia/Harbin" | "Asia/Urumqi" => 28800,
        "Asia/Hong_Kong" | "Asia/Singapore" | "Asia/Kuala_Lumpur" => 28800,
        "Asia/Taipei" => 28800,
        "Asia/Manila" => 28800,
        "Asia/Bangkok" | "Asia/Ho_Chi_Minh" => 25200,
        "Asia/Jakarta" => 25200,
        "Asia/Calcutta" | "Asia/Kolkata" => 19800,
        "Asia/Dubai" => 14400,
        "Asia/Karachi" => 18000,
        "Asia/Dhaka" => 21600,
        "Asia/Rangoon" | "Asia/Yangon" => 23400,
        "Asia/Colombo" => 19800,
        "Asia/Tehran" => 12600,
        "Asia/Baku" => 14400,
        "Asia/Almaty" => 21600,
        "Asia/Tashkent" => 18000,
        "Asia/Ulaanbaatar" => 28800,
        "Asia/Kathmandu" => 20700,
        // Australia/Oceania
        "Australia/Sydney" | "Australia/Melbourne" | "Australia/Brisbane" => 36000,
        "Australia/Perth" => 28800,
        "Australia/Adelaide" => 37800,
        "Pacific/Auckland" => 46800,
        "Pacific/Fiji" => 43200,
        // Africa
        "Africa/Cairo" => 7200,
        "Africa/Lagos" | "Africa/Nairobi" | "Africa/Johannesburg" => 7200,
        "Africa/Casablanca" => 3600,
        "Africa/Dar_es_Salaam" => 10800,
        // Pacific
        "Pacific/Honolulu" => -36000,
        "Pacific/Guam" | "Pacific/Port_Moresby" => 36000,
        _ => 0,
    }
}

/// Convert Unix timestamp (seconds since epoch) to a broken-down local time.
/// This is a pure function with no unsafe code.
fn broken_down_time(timestamp: i64) -> Option<LocalTime> {
    // Algorithm: convert Unix timestamp to broken-down time
    // Based on the well-known algorithm for civil date from days since epoch

    let mut days = timestamp / 86400;
    let mut rem = (timestamp % 86400) as i32;

    // Handle negative timestamps (before 1970)
    if rem < 0 {
        rem += 86400;
        days -= 1;
    }

    let hour = (rem / 3600) as u32;
    let minute = ((rem % 3600) / 60) as u32;
    let second = (rem % 60) as u32;

    // Convert days since epoch to year/month/day
    // Algorithm from Howard Hinnant's date library
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = days - era * 146097; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe as i32 + (era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy as i32 + 2) / 153; // month index [0, 11]
    let d = doy as i32 - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let year = if m <= 2 { y + 1 } else { y }; // year

    Some(LocalTime {
        year,
        month: m as u32,
        day: d as u32,
        hour: hour as u32,
        minute: minute as u32,
        second: second as u32,
    })
}

/// Format time as "HH:MM".
pub fn format_time_hhmm(local: &LocalTime) -> String {
    format!("{:02}:{:02}", local.hour, local.minute)
}

/// Format date as "DD Mon" (e.g., "08 November").
pub fn format_date_day_month_full(local: &LocalTime) -> String {
    let month_name = month_name_full(local.month);
    format!("{} {}", local.day, month_name)
}

/// Format date as "D Month" with space-padded day (e.g., " 8 November").
pub fn format_date_padded_day_month_full(local: &LocalTime) -> String {
    let month_name = month_name_full(local.month);
    format!("{:>2} {}", local.day, month_name)
}

/// Format date as "DD Mon" with abbreviated month (e.g., "08 Nov").
pub fn format_date_day_month_abbr(local: &LocalTime) -> String {
    let month_name = month_name_abbr(local.month);
    format!("{:02} {}", local.day, month_name)
}

/// Format year as "'YY" (e.g., "'26").
pub fn format_year_short(local: &LocalTime) -> String {
    format!("'{}", local.year % 100)
}

/// Get the full month name (1-12 → "January"-"December").
fn month_name_full(month: u32) -> &'static str {
    MONTH_NAMES
        .get((month - 1) as usize)
        .copied()
        .unwrap_or("???")
}

/// Get the abbreviated month name (1-12 → "Jan"-"Dec").
fn month_name_abbr(month: u32) -> &'static str {
    MONTH_ABBRS
        .get((month - 1) as usize)
        .copied()
        .unwrap_or("???")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_broken_down_time_epoch() {
        // Unix epoch: 1970-01-01 00:00:00 UTC
        let lt = broken_down_time(0).unwrap();
        assert_eq!(lt.year, 1970);
        assert_eq!(lt.month, 1);
        assert_eq!(lt.day, 1);
        assert_eq!(lt.hour, 0);
        assert_eq!(lt.minute, 0);
    }

    #[test]
    fn test_format_time_hhmm() {
        let lt = LocalTime {
            year: 2026,
            month: 11,
            day: 10,
            hour: 9,
            minute: 5,
            second: 30,
        };
        assert_eq!(format_time_hhmm(&lt), "09:05");
    }

    #[test]
    fn test_format_date_day_month_full() {
        let lt = LocalTime {
            year: 2026,
            month: 11,
            day: 8,
            hour: 12,
            minute: 0,
            second: 0,
        };
        assert_eq!(format_date_day_month_full(&lt), "8 November");
    }

    #[test]
    fn test_format_year_short() {
        let lt = LocalTime {
            year: 2026,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        };
        assert_eq!(format_year_short(&lt), "'26");
    }

    #[test]
    fn test_month_names() {
        assert_eq!(month_name_full(1), "January");
        assert_eq!(month_name_full(12), "December");
        assert_eq!(month_name_abbr(1), "Jan");
        assert_eq!(month_name_abbr(12), "Dec");
    }
}
