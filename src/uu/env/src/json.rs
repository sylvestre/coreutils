// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: the environment `env` would print, keyed like
//! jc's `env` parser.

use std::env;

use uucore::json::Value;

/// The current environment as a JSON array of `{"name", "value"}` records.
pub(crate) fn from_current_env() -> Value {
    env::vars_os()
        .map(|(name, value)| {
            Value::object([
                ("name", Value::from(name.to_string_lossy())),
                ("value", Value::from(value.to_string_lossy())),
            ])
        })
        .collect()
}

/// The `NAME=VALUE` entries (as read with `--env0-from`) as a JSON array of
/// `{"name", "value"}` records. An entry without `=` has an empty value.
pub(crate) fn from_entries(entries: &[Vec<u8>]) -> Value {
    entries
        .iter()
        .map(|entry| {
            let (name, value) = entry
                .iter()
                .position(|&b| b == b'=')
                .map_or((&entry[..], &[][..]), |eq| (&entry[..eq], &entry[eq + 1..]));
            Value::object([
                ("name", Value::from(String::from_utf8_lossy(name))),
                ("value", Value::from(String::from_utf8_lossy(value))),
            ])
        })
        .collect()
}
