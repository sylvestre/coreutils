// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per line the text output prints (the
//! heading excluded), keyed like jc's `who` parser.

use uucore::json::{Map, Value, json};

/// The columns of one line; empty or `None` columns are left out.
#[derive(Default)]
pub(crate) struct Record<'a> {
    pub(crate) user: &'a str,
    /// What a line that is not a user session stands for, e.g. `"reboot"`.
    pub(crate) event: &'a str,
    /// `+`, `-` or `?`, with `-T`.
    pub(crate) writeable_tty: Option<char>,
    pub(crate) tty: &'a str,
    /// The time as printed.
    pub(crate) time: &'a str,
    /// The same time, in seconds since the Unix epoch.
    pub(crate) epoch: Option<i64>,
    pub(crate) idle: Option<&'a str>,
    pub(crate) pid: Option<&'a str>,
    /// The remote host of a session, without the parentheses of the text.
    pub(crate) from: &'a str,
    pub(crate) comment: &'a str,
}

impl Record<'_> {
    pub(crate) fn to_json(&self) -> Value {
        let mut record = Map::new();
        insert_text(&mut record, "user", self.user);
        insert_text(&mut record, "event", self.event);
        if let Some(state) = self.writeable_tty.filter(|state| *state != ' ') {
            record.insert("writeable_tty".into(), json!(state.to_string()));
        }
        insert_text(&mut record, "tty", self.tty);
        insert_text(&mut record, "time", self.time);
        if let Some(idle) = self.idle {
            insert_text(&mut record, "idle", idle);
        }
        if let Some(pid) = self.pid.and_then(|pid| pid.trim().parse::<i64>().ok()) {
            record.insert("pid".into(), json!(pid));
        }
        insert_text(&mut record, "from", self.from);
        insert_text(&mut record, "comment", self.comment);
        if let Some(epoch) = self.epoch {
            record.insert("epoch".into(), json!(epoch));
        }
        Value::Object(record)
    }
}

/// Inserts `value` without its padding, unless the column is empty.
fn insert_text(record: &mut Map<String, Value>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        record.insert(key.into(), json!(value));
    }
}

#[cfg(test)]
mod tests {
    use super::Record;
    use uucore::json::json;

    #[test]
    fn test_session_trims_columns_and_parses_pid() {
        let record = Record {
            user: "ada",
            writeable_tty: Some('+'),
            tty: "pts/7",
            time: "Mar  3 07:15",
            epoch: Some(1_741_000_000),
            idle: Some("  .  "),
            pid: Some("4242"),
            from: "lab.example",
            ..Record::default()
        };
        assert_eq!(
            record.to_json(),
            json!({
                "user": "ada",
                "writeable_tty": "+",
                "tty": "pts/7",
                "time": "Mar  3 07:15",
                "idle": ".",
                "pid": 4242,
                "from": "lab.example",
                "epoch": 1_741_000_000,
            })
        );
    }

    #[test]
    fn test_event_leaves_out_empty_columns() {
        let record = Record {
            event: "reboot",
            writeable_tty: Some(' '),
            time: "2031-12-24 23:59",
            epoch: Some(1_956_000_000),
            pid: Some(""),
            ..Record::default()
        };
        assert_eq!(
            record.to_json(),
            json!({ "event": "reboot", "time": "2031-12-24 23:59", "epoch": 1_956_000_000 })
        );
    }
}
