// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per line the text output prints, keyed
//! like jc's `who` parser.

use uucore::json::Value;

/// The non-empty columns, without their padding; `pid` is a number.
pub(crate) fn record(columns: &[(&'static str, &str)]) -> Value {
    let columns = columns
        .iter()
        .map(|(key, value)| (*key, value.trim()))
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| match (key, value.parse::<u64>()) {
            ("pid", Ok(pid)) => (key, Value::from(pid)),
            _ => (key, Value::from(value)),
        });
    Value::Object(columns.collect())
}
