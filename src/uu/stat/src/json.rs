// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per file, with the fields of the
//! default output, keyed like jc's `stat` parser.

use std::ffi::OsStr;
use std::fs::Metadata;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;

use uucore::entries;
use uucore::fs::{display_permissions, major, minor};
use uucore::fsext::{MetadataTimeField, metadata_get_time, pretty_filetype};
use uucore::json::{Map, Value, json};
use uucore::libc::mode_t;

use crate::{pretty_time, system_time_to_timestamp};

/// The record of `file`, named `display_name` as the text output names it.
pub(crate) fn file_record(display_name: &str, file: &OsStr, meta: &Metadata) -> Value {
    let file_type = meta.file_type();
    let mut record = Map::new();
    record.insert("file".into(), json!(display_name));
    if file_type.is_symlink()
        && let Ok(target) = Path::new(file).read_link()
    {
        record.insert("link_to".into(), json!(target.to_string_lossy()));
    }
    record.insert("size".into(), json!(meta.len()));
    record.insert("blocks".into(), json!(meta.blocks()));
    record.insert("io_blocks".into(), json!(meta.blksize()));
    record.insert(
        "type".into(),
        json!(pretty_filetype(meta.mode() as mode_t, meta.len())),
    );
    record.insert(
        "device".into(),
        json!(format!(
            "{},{}",
            major(meta.dev() as _),
            minor(meta.dev() as _)
        )),
    );
    record.insert("inode".into(), json!(meta.ino()));
    record.insert("links".into(), json!(meta.nlink()));
    if file_type.is_char_device() || file_type.is_block_device() {
        record.insert("rdev".into(), json!(meta.rdev()));
    }
    record.insert(
        "access".into(),
        json!(format!("{:04o}", meta.mode() & 0o7777)),
    );
    record.insert("flags".into(), json!(display_permissions(meta, true)));
    record.insert("uid".into(), json!(meta.uid()));
    record.insert(
        "user".into(),
        json!(entries::uid2usr(meta.uid()).unwrap_or_else(|_| "UNKNOWN".to_owned())),
    );
    record.insert("gid".into(), json!(meta.gid()));
    record.insert(
        "group".into(),
        json!(entries::gid2grp(meta.gid()).unwrap_or_else(|_| "UNKNOWN".to_owned())),
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
                json!(pretty_time(meta, field)),
                json!(system_time_to_timestamp(time).0),
            ),
            None => (Value::Null, Value::Null),
        };
        record.insert(key.into(), text);
        record.insert(format!("{key}_epoch"), epoch);
    }
    Value::Object(record)
}

#[cfg(test)]
mod tests {
    use super::file_record;
    use std::ffi::OsStr;

    #[test]
    fn test_directory_record() {
        // Unit tests run from the crate directory.
        let meta = std::fs::metadata(".").unwrap();
        let record = file_record("some dir", OsStr::new("."), &meta);
        assert_eq!(record["file"], "some dir");
        assert_eq!(record["type"], "directory");
        assert!(record.get("link_to").is_none());
        assert!(record.get("rdev").is_none());
        assert!(record["modify_time_epoch"].is_i64());
        assert_eq!(record["access"].as_str().unwrap().len(), 4);
    }
}
