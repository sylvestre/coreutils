// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore loadavg

//! `--output-format=json`: the fields of the default output, keyed like jc's
//! `uptime` parser.

use std::time::Duration;

use jiff::Timestamp;
use jiff::civil::Time;
use jiff::tz::TimeZone;
use uucore::error::UResult;
use uucore::json::{Map, Value, json, print_json};
use uucore::uptime::{FormattedUptime, get_loadavg};

use crate::platform;

/// Prints the current time, the uptime (left out when unknown), the number
/// of users and the load averages (left out where there are none).
pub(crate) fn print_uptime(matches: &clap::ArgMatches) -> UResult<()> {
    let (uptime, users) = platform::uptime_and_users(matches)?;
    let now = Timestamp::now().to_zoned(TimeZone::system()).time();
    let record = to_json(now, uptime, users, get_loadavg().ok());
    print_json(&record)?;
    Ok(())
}

fn to_json(
    now: Time,
    uptime: Option<Duration>,
    users: usize,
    loadavg: Option<(f64, f64, f64)>,
) -> Value {
    // Rounded as the text prints them.
    let round = |load: f64| (load * 100.0).round() / 100.0;
    let mut record = Map::new();
    record.insert("time".into(), json!(now.strftime("%H:%M:%S").to_string()));
    let formatted = uptime.map(|uptime| FormattedUptime::new(uptime.as_secs()));
    if let Some(formatted) = &formatted {
        record.insert(
            "uptime".into(),
            json!(formatted.get_human_readable_uptime()),
        );
    }
    record.insert("users".into(), json!(users));
    if let Some((load_1m, load_5m, load_15m)) = loadavg {
        record.insert("load_1m".into(), json!(round(load_1m)));
        record.insert("load_5m".into(), json!(round(load_5m)));
        record.insert("load_15m".into(), json!(round(load_15m)));
    }
    record.insert("time_hour".into(), json!(now.hour()));
    record.insert("time_minute".into(), json!(now.minute()));
    record.insert("time_second".into(), json!(now.second()));
    if let (Some(uptime), Some(formatted)) = (uptime, formatted) {
        record.insert("uptime_days".into(), json!(formatted.days));
        record.insert("uptime_hours".into(), json!(formatted.hours));
        record.insert("uptime_minutes".into(), json!(formatted.mins));
        record.insert("uptime_total_seconds".into(), json!(uptime.as_secs()));
    }
    Value::Object(record)
}

#[cfg(test)]
mod tests {
    use super::to_json;
    use jiff::civil::time;
    use std::time::Duration;
    use uucore::json::json;

    #[test]
    fn test_uptime_is_split_and_loads_rounded() {
        let uptime = Duration::from_secs(2 * 86400 + 7 * 3600 + 5 * 60 + 41);
        let record = to_json(
            time(6, 9, 30, 0),
            Some(uptime),
            4,
            Some((0.004, 1.5, 12.346)),
        );
        let keys: Vec<&str> = record
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "time",
                "uptime",
                "users",
                "load_1m",
                "load_5m",
                "load_15m",
                "time_hour",
                "time_minute",
                "time_second",
                "uptime_days",
                "uptime_hours",
                "uptime_minutes",
                "uptime_total_seconds"
            ]
        );
        assert_eq!(record["time"], "06:09:30");
        assert_eq!(record["users"], 4);
        assert_eq!(record["load_1m"], json!(0.0));
        assert_eq!(record["load_5m"], json!(1.5));
        assert_eq!(record["load_15m"], json!(12.35));
        assert_eq!(record["time_hour"], 6);
        assert_eq!(record["time_second"], 30);
        assert_eq!(record["uptime_days"], 2);
        assert_eq!(record["uptime_hours"], 7);
        assert_eq!(record["uptime_minutes"], 5);
        assert_eq!(record["uptime_total_seconds"], 198_341);
    }

    #[test]
    fn test_unknown_uptime_and_loads_are_left_out() {
        let record = to_json(time(23, 0, 0, 0), None, 0, None);
        assert_eq!(
            record,
            json!({
                "time": "23:00:00",
                "users": 0,
                "time_hour": 23,
                "time_minute": 0,
                "time_second": 0,
            })
        );
    }
}
