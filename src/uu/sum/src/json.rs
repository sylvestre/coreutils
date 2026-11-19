// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per file, keyed like jc's `cksum`
//! parser (which also parses `sum`).

use std::ffi::OsStr;

use uucore::json::Value;

/// The record of one file; `filename` is "-" for standard input.
pub(crate) fn record(checksum: u16, blocks: usize, filename: &OsStr) -> Value {
    Value::object([
        ("filename", Value::from(filename.to_string_lossy())),
        ("checksum", Value::from(checksum)),
        ("blocks", Value::from(blocks)),
    ])
}
