// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: the environment `env` would print, keyed like
//! jc's `env` parser.

use std::env;

use uucore::json::{Value, json};

/// The current environment as a JSON array of `{"name", "value"}` records.
pub(crate) fn from_current_env() -> Value {
    env::vars_os()
        .map(|(name, value)| json!({ "name": name.to_string_lossy(), "value": value.to_string_lossy() }))
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
            json!({
                "name": String::from_utf8_lossy(name),
                "value": String::from_utf8_lossy(value),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::from_entries;
    use uucore::json::json;

    #[test]
    fn test_entries_split_at_first_equal_sign() {
        let entries = vec![b"EQ=a=b".to_vec(), b"BARE".to_vec(), b"EMPTY=".to_vec()];
        assert_eq!(
            from_entries(&entries),
            json!([
                { "name": "EQ", "value": "a=b" },
                { "name": "BARE", "value": "" },
                { "name": "EMPTY", "value": "" },
            ])
        );
    }
}
