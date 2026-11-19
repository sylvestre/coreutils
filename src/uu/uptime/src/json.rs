// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore loadavg

//! `--output-format=json`: the fields of the default output, keyed like jc's
//! `uptime` parser.

use uucore::json::{Map, Value};
use uucore::uptime::FormattedUptime;

use crate::Report;

/// The fields of the default output, left out when unknown, and the time and
/// uptime split into numbers.
impl From<&Report> for Value {
    fn from(report: &Report) -> Self {
        // Rounded as the text prints them.
        let round = |load: f64| (load * 100.0).round() / 100.0;
        let formatted = report
            .uptime
            .map(|uptime| FormattedUptime::new(uptime.as_secs()));
        let mut record = Map::new();
        record.insert("time", report.now.strftime("%H:%M:%S").to_string());
        if let Some(formatted) = &formatted {
            record.insert("uptime", formatted.get_human_readable_uptime());
        }
        record.insert("users", report.users);
        if let Some((load_1m, load_5m, load_15m)) = report.loadavg {
            record.insert("load_1m", round(load_1m));
            record.insert("load_5m", round(load_5m));
            record.insert("load_15m", round(load_15m));
        }
        record.insert("time_hour", report.now.hour());
        record.insert("time_minute", report.now.minute());
        record.insert("time_second", report.now.second());
        if let (Some(uptime), Some(formatted)) = (report.uptime, formatted) {
            record.insert("uptime_days", formatted.days);
            record.insert("uptime_hours", formatted.hours);
            record.insert("uptime_minutes", formatted.mins);
            record.insert("uptime_total_seconds", uptime.as_secs());
        }
        Self::Object(record)
    }
}
