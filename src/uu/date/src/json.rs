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
use uucore::json::{Map, Value};

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
    let mut record = Map::new();
    record.insert("year", date.year());
    record.insert(
        "month",
        MONTHS[usize::from(date.month().unsigned_abs()) - 1],
    );
    record.insert("month_num", date.month());
    record.insert("day", date.day());
    record.insert(
        "weekday",
        WEEKDAYS[usize::from(weekday_from_monday.unsigned_abs())],
    );
    record.insert("weekday_num", weekday_from_monday + 1);
    record.insert("hour", hour_12);
    record.insert("hour_24", hour);
    record.insert("minute", date.minute());
    record.insert("second", date.second());
    record.insert("period", if hour < 12 { "AM" } else { "PM" });
    record.insert("timezone", format("%Z"));
    record.insert("utc_offset", format("%z"));
    record.insert("day_of_year", day_of_year);
    // Week number with Monday as the first day of the week: the days
    // before the first Monday of the year are in week 0.
    record.insert(
        "week_of_year",
        (day_of_year - 1 + 7 - i16::from(weekday_from_monday)) / 7,
    );
    record.insert("iso", format("%Y-%m-%dT%H:%M:%S%:z"));
    record.insert("epoch", epoch);
    record.insert("epoch_utc", epoch);
    record.insert("timezone_aware", true);
    Value::Object(record)
}
