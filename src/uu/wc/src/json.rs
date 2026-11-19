// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per line the text output prints, keyed
//! like jc's `wc` parser.

use std::ffi::OsStr;

use uucore::json::{Map, Value, json};

use crate::Settings;
use crate::word_count::WordCount;

/// The counts the text line would show, in the same order, then the name
/// when the text line has one (none for an implicit stdin or `--total=only`).
pub(crate) fn record(settings: &Settings, counts: &WordCount, name: Option<&OsStr>) -> Value {
    let fields = [
        (settings.show_lines, "lines", counts.lines),
        (settings.show_words, "words", counts.words),
        (settings.show_chars, "characters", counts.chars),
        (settings.show_bytes, "bytes", counts.bytes),
        (
            settings.show_max_line_length,
            "max_line_length",
            counts.max_line_length,
        ),
    ];
    let mut record = Map::new();
    if let Some(name) = name {
        record.insert("filename".into(), json!(name.to_string_lossy()));
    }
    for (shown, key, count) in fields {
        if shown {
            record.insert(key.into(), json!(count));
        }
    }
    Value::Object(record)
}
