// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per line the text output prints, keyed
//! like jc's `du` parser.

use std::io;
use std::path::Path;

use uucore::json::{Value, json, print_json};

#[derive(Default)]
pub(crate) struct JsonEntries {
    entries: Vec<Value>,
}

impl JsonEntries {
    /// `size` is in bytes (or inodes with `--inodes`), whatever unit the text
    /// output would use.
    pub(crate) fn push(&mut self, size: u64, path: &Path) {
        self.entries
            .push(json!({ "size": size, "name": path.to_string_lossy() }));
    }

    pub(crate) fn print(self) -> io::Result<()> {
        print_json(&Value::Array(self.entries))
    }
}

#[cfg(test)]
mod tests {
    use super::JsonEntries;
    use std::path::Path;
    use uucore::json::json;

    #[test]
    fn test_entries_keep_order() {
        let mut entries = JsonEntries::default();
        entries.push(4096, Path::new("tree/leaf"));
        entries.push(12_288, Path::new("tree"));
        assert_eq!(
            json!(entries.entries),
            json!([
                { "size": 4096, "name": "tree/leaf" },
                { "size": 12_288, "name": "tree" },
            ])
        );
    }
}
