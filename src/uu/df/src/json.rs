// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore Pcent Itotal Iused Iavail Ipcent

//! `--output-format=json`: the rows of the table, keyed like jc's `df` parser.

use uucore::json::Value;
use uucore::translate;

use crate::Options;
use crate::columns::Column;
use crate::filesystem::Filesystem;
use crate::table::Row;

/// The rows of the table, the `--total` row included. Sizes are exact byte
/// counts and percentages are integers, null where the text shows `-`.
pub(crate) fn to_json(options: &Options, filesystems: Vec<Filesystem>) -> Value {
    let mut records = Vec::new();
    let mut total = Row::new(&translate!("df-total"));
    for filesystem in filesystems {
        if options.show_all_fs || filesystem.usage.blocks > 0 {
            let row = Row::from_filesystem(filesystem, &options.block_size);
            records.push(record(&row, options));
            total += row;
        }
    }
    // Like the table, nothing is listed when every path was rejected, and the
    // total is labelled under the target column when there is no source one.
    if options.show_total && !records.is_empty() {
        if !options.columns.contains(&Column::Source) {
            total.fs_mount = translate!("df-total").into();
        }
        records.push(record(&total, options));
    }
    Value::Array(records)
}

fn record(row: &Row, options: &Options) -> Value {
    let percentage = |fraction: Option<f64>| fraction.map(|x| (100.0 * x).ceil() as u64);
    let fields = options.columns.iter().map(|column| match column {
        Column::Source => ("filesystem", Value::from(&row.fs_device)),
        Column::Size => ("size", Value::from(row.bytes.bytes)),
        Column::Used => ("used", Value::from(row.bytes_used.bytes)),
        Column::Avail => ("available", Value::from(row.bytes_avail.bytes)),
        Column::Pcent => ("use_percent", Value::from(percentage(row.bytes_usage))),
        Column::Target => ("mounted_on", Value::from(row.fs_mount.to_string_lossy())),
        Column::Itotal => ("inodes", Value::from(row.inodes)),
        Column::Iused => ("iused", Value::from(row.inodes_used)),
        Column::Iavail => ("ifree", Value::from(row.inodes_free)),
        Column::Ipcent => ("iused_percent", Value::from(percentage(row.inodes_usage))),
        Column::File => (
            "file",
            Value::from(row.file.as_ref().map(|f| f.to_string_lossy())),
        ),
        Column::Fstype => ("type", Value::from(&row.fs_type)),
        #[cfg(target_vendor = "apple")]
        Column::Capacity => (
            "capacity_percent",
            Value::from(percentage(row.bytes_capacity)),
        ),
    });
    Value::Object(fields.collect())
}
