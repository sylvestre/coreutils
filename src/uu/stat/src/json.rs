// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per file, with the fields of the
//! default output, keyed like jc's `stat` parser.

use std::ffi::OsStr;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use uucore::entries;
use uucore::fs::{display_permissions, major, minor};
use uucore::fsext::{MetadataTimeField, metadata_get_time, pretty_filetype};
use uucore::json::{Map, Value};
use uucore::libc::mode_t;

use crate::{pretty_time, system_time_to_timestamp};

/// The record of `file`, named `display_name` as the text output names it.
pub(crate) fn file_record(display_name: &str, file: &OsStr, meta: &Metadata) -> Value {
    let file_type = meta.file_type();
    let mut record = Map::new();
    record.insert("file", display_name);
    if file_type.is_symlink()
        && let Ok(target) = Path::new(file).read_link()
    {
        record.insert("link_to", target.to_string_lossy());
    }
    record.insert("size", meta.len());
    record.insert("blocks", meta.blocks());
    record.insert("io_blocks", meta.blksize());
    record.insert("type", pretty_filetype(meta.mode() as mode_t, meta.len()));
    record.insert(
        "device",
        format!("{},{}", major(meta.dev() as _), minor(meta.dev() as _)),
    );
    record.insert("inode", meta.ino());
    record.insert("links", meta.nlink());
    record.insert("access", format!("{:04o}", meta.mode() & 0o7777));
    record.insert("flags", display_permissions(meta, true));
    record.insert("uid", meta.uid());
    record.insert(
        "user",
        entries::uid2usr(meta.uid()).unwrap_or_else(|_| "UNKNOWN".to_owned()),
    );
    record.insert("gid", meta.gid());
    record.insert(
        "group",
        entries::gid2grp(meta.gid()).unwrap_or_else(|_| "UNKNOWN".to_owned()),
    );
    let times = [
        ("access_time", MetadataTimeField::Access),
        ("modify_time", MetadataTimeField::Modification),
        ("change_time", MetadataTimeField::Change),
        ("birth_time", MetadataTimeField::Birth),
    ];
    for (key, field) in times {
        // An unknown time, which the text prints as `-`, is null.
        let (text, epoch) = match metadata_get_time(meta, field) {
            Some(time) => (
                Value::from(pretty_time(meta, field)),
                Value::from(system_time_to_timestamp(time).0),
            ),
            None => (Value::Null, Value::Null),
        };
        record.insert(key, text);
        record.insert(format!("{key}_epoch"), epoch);
    }
    Value::Object(record)
}
