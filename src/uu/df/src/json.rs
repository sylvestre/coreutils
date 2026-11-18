// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore Pcent Itotal Iused Iavail Ipcent

//! `--output-format=json`: the rows of the table, keyed like jc's `df` parser.

use uucore::json::{Map, Value, json};
use uucore::translate;

use crate::Options;
use crate::columns::Column;
use crate::filesystem::Filesystem;
use crate::table::Row;

/// The rows of the table as a JSON array, one record per row (the header
/// excluded, the `--total` row included).
pub(crate) fn to_json(options: &Options, filesystems: Vec<Filesystem>) -> Value {
    let mut records = Vec::new();
    let mut total = Row::new(&translate!("df-total"));
    for filesystem in filesystems {
        if options.show_all_fs || filesystem.usage.blocks > 0 {
            let row = Row::from_filesystem(filesystem, &options.block_size);
            records.push(json_record(&row, options, false));
            if options.show_total {
                total += row;
            }
        }
    }
    if options.show_total {
        records.push(json_record(&total, options, true));
    }
    Value::Array(records)
}

/// Returns the row as a JSON record, keyed like jc's `df` parser.
///
/// It has the same columns as the text row, but sizes are always
/// exact byte counts and percentages are integers (null for `-`).
fn json_record(row: &Row, options: &Options, is_total_row: bool) -> Value {
    let percentage = |fraction: Option<f64>| fraction.map(|x| (100.0 * x).ceil() as u64);
    let mut record = Map::new();
    for column in &options.columns {
        let (key, value) = match column {
            Column::Source => (
                "filesystem",
                if is_total_row {
                    json!(translate!("df-total"))
                } else {
                    json!(row.fs_device)
                },
            ),
            Column::Size => ("size", json!(row.bytes.bytes)),
            Column::Used => ("used", json!(row.bytes_used.bytes)),
            Column::Avail => ("available", json!(row.bytes_avail.bytes)),
            Column::Pcent => ("use_percent", json!(percentage(row.bytes_usage))),
            Column::Target => (
                "mounted_on",
                if is_total_row && !options.columns.contains(&Column::Source) {
                    json!(translate!("df-total"))
                } else {
                    json!(row.fs_mount.to_string_lossy())
                },
            ),
            Column::Itotal => ("inodes", json!(row.inodes)),
            Column::Iused => ("iused", json!(row.inodes_used)),
            Column::Iavail => ("ifree", json!(row.inodes_free)),
            Column::Ipcent => ("iused_percent", json!(percentage(row.inodes_usage))),
            Column::File => (
                "file",
                json!(row.file.as_ref().map(|f| f.to_string_lossy())),
            ),
            Column::Fstype => ("type", json!(row.fs_type)),
            #[cfg(target_vendor = "apple")]
            Column::Capacity => ("capacity_percent", json!(percentage(row.bytes_capacity))),
        };
        record.insert(key.into(), value);
    }
    Value::Object(record)
}

#[cfg(test)]
mod tests {
    use super::json_record;
    use crate::Options;
    use crate::columns::Column;
    use crate::table::Row;
    use uucore::json::json;

    #[test]
    fn test_empty_filesystem_has_null_percentage() {
        let row = Row::new("none");
        let options = Options {
            columns: vec![Column::Source, Column::Size, Column::Pcent],
            ..Default::default()
        };
        assert_eq!(
            json_record(&row, &options, false),
            json!({ "filesystem": "none", "size": 0, "use_percent": null })
        );
    }

    #[test]
    fn test_total_row_names_the_target_without_source() {
        let row = Row::new("ignored");
        let options = Options {
            columns: vec![Column::Target, Column::Itotal],
            ..Default::default()
        };
        assert_eq!(
            json_record(&row, &options, true)["mounted_on"],
            uucore::translate!("df-total")
        );
    }
}
