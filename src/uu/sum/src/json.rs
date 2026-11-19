// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per file, keyed like jc's `cksum`
//! parser (which also parses `sum`).

use std::ffi::OsStr;

use uucore::json::{Value, json};

/// The record of one file; `filename` is "-" for standard input.
pub(crate) fn record(checksum: u16, blocks: usize, filename: &OsStr) -> Value {
    json!({
        "filename": filename.to_string_lossy(),
        "checksum": checksum,
        "blocks": blocks,
    })
}
