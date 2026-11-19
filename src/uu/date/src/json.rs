// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore yday

//! `--output-format=json`: the date being displayed, keyed like jc's `date`
//! parser. The time zone is known, so every field is exact: `epoch` and
//! `epoch_utc` are both the Unix time and `timezone_aware` is always true.

use jiff::Zoned;
use jiff::fmt::strtime;
use uucore::json::{Value, json};

use crate::timestamp_seconds_floor;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Returns `date` as a JSON record. Names are English whatever the locale.
pub(crate) fn to_json(date: &Zoned) -> Value {
    // Monday is 0, as the weeks of `week_of_year` start on Monday.
    let weekday_from_monday = date.weekday().to_monday_zero_offset();
    let day_of_year = date.day_of_year();
    let hour = date.hour();
    let hour_12 = match hour % 12 {
        0 => 12,
        h => h,
    };
    let epoch = timestamp_seconds_floor(date.timestamp());
    // Infallible for these specifiers: `%Z` falls back to the offset.
    let format = |spec: &str| strtime::format(spec, date).unwrap_or_default();
    json!({
        "year": date.year(),
        "month": MONTHS[usize::from(date.month().unsigned_abs()) - 1],
        "month_num": date.month(),
        "day": date.day(),
        "weekday": WEEKDAYS[usize::from(weekday_from_monday.unsigned_abs())],
        "weekday_num": weekday_from_monday + 1,
        "hour": hour_12,
        "hour_24": hour,
        "minute": date.minute(),
        "second": date.second(),
        "period": if hour < 12 { "AM" } else { "PM" },
        "timezone": format("%Z"),
        "utc_offset": format("%z"),
        "day_of_year": day_of_year,
        // Week number with Monday as the first day of the week: the days
        // before the first Monday of the year are in week 0.
        "week_of_year": (day_of_year - 1 + 7 - i16::from(weekday_from_monday)) / 7,
        "iso": format("%Y-%m-%dT%H:%M:%S%:z"),
        "epoch": epoch,
        "epoch_utc": epoch,
        "timezone_aware": true,
    })
}

#[cfg(test)]
mod tests {
    use super::to_json;
    use jiff::Zoned;
    use uucore::json::json;

    #[test]
    fn test_evening_in_a_fixed_offset() {
        // A Sunday, so still in the week of the previous Monday.
        let date: Zoned = "2023-01-08T19:04:05-03:30[-03:30]".parse().unwrap();
        assert_eq!(
            to_json(&date),
            json!({
                "year": 2023,
                "month": "Jan",
                "month_num": 1,
                "day": 8,
                "weekday": "Sun",
                "weekday_num": 7,
                "hour": 7,
                "hour_24": 19,
                "minute": 4,
                "second": 5,
                "period": "PM",
                "timezone": "-03:30",
                "utc_offset": "-0330",
                "day_of_year": 8,
                "week_of_year": 1,
                "iso": "2023-01-08T19:04:05-03:30",
                "epoch": 1_673_217_245,
                "epoch_utc": 1_673_217_245,
                "timezone_aware": true,
            })
        );
    }

    #[test]
    fn test_midnight_before_the_first_monday() {
        // 2027 starts on a Friday: week 0 until Monday the 4th.
        let date: Zoned = "2027-01-03T00:00:00+00:00[UTC]".parse().unwrap();
        let value = to_json(&date);
        assert_eq!(value["hour"], 12);
        assert_eq!(value["period"], "AM");
        assert_eq!(value["week_of_year"], 0);
        assert_eq!(value["timezone"], "UTC");
    }
}
