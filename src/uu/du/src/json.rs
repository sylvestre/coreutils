// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per line the text output prints, keyed
//! like jc's `du` parser.

use std::path::Path;

use uucore::json::Value;

/// `size` is in bytes (or inodes with `--inodes`), whatever unit the text
/// output would use.
pub(crate) fn record(size: u64, path: &Path) -> Value {
    Value::object([
        ("size", Value::from(size)),
        ("name", Value::from(path.to_string_lossy())),
    ])
}
