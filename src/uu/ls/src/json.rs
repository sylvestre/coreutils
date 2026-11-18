// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one JSON array with a record per listed entry,
//! keyed like jc's `ls` parser. A plain listing gives the names; a long one
//! adds the fields `ls -l` prints, with the exact size and an epoch time.

use std::ops::RangeInclusive;
#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::time::{Duration, SystemTime};

use uucore::error::UResult;
use uucore::fs::display_permissions;
use uucore::fsext::metadata_get_time;
use uucore::json::{Map, Value, json, print_json};
use uucore::time::system_time_to_sec;

use crate::display::display_date;
use crate::output::{LsOutput, StreamMode};
use crate::{Config, LsError, PathData, list_with_output};

/// Lists files as a JSON array to stdout.
pub fn list_json(locs: Vec<&Path>, config: &Config) -> UResult<()> {
    let now = SystemTime::now();
    let mut output = JsonOutput {
        entries: Vec::new(),
        parent: None,
        // The same "recent" window as the text listing, for the `date` field.
        recent_time_range: (now - Duration::new(31_556_952 / 2, 0))..=now,
    };
    list_with_output(locs, config, &mut output)?;
    print_json(&Value::Array(output.entries)).map_err(|e| LsError::WriteError(e).into())
}

/// Collects one record per entry; the array is written once listing is done.
struct JsonOutput {
    entries: Vec<Value>,
    /// The directory whose heading the text output would print last.
    parent: Option<String>,
    recent_time_range: RangeInclusive<SystemTime>,
}

impl LsOutput for JsonOutput {
    fn stream_mode(&self) -> StreamMode {
        // Entries go through `write_entries`, which has the config.
        StreamMode::Batch
    }

    fn write_dir_header(
        &mut self,
        path_data: &PathData,
        _config: &Config,
        _is_first: bool,
    ) -> UResult<()> {
        self.parent = Some(path_data.path().to_string_lossy().into_owned());
        Ok(())
    }

    fn write_entries(&mut self, entries: &[PathData], config: &Config) -> UResult<()> {
        for entry in entries {
            let record = self.record(entry, config)?;
            self.entries.push(Value::Object(record));
        }
        Ok(())
    }
}

impl JsonOutput {
    fn record(&self, entry: &PathData, config: &Config) -> UResult<Map<String, Value>> {
        let mut record = Map::new();
        record.insert(
            "filename".into(),
            json!(entry.display_name().to_string_lossy()),
        );
        let md = entry.metadata().filter(|_| config.json_long);
        if let Some(md) = md {
            if md.file_type().is_symlink()
                && let Ok(target) = entry.path().read_link()
            {
                record.insert("link_to".into(), json!(target.to_string_lossy()));
            }
            record.insert("flags".into(), json!(display_permissions(md, true)));
            #[cfg(unix)]
            record.insert("links".into(), json!(md.nlink()));
        }
        if let Some(parent) = &self.parent {
            record.insert("parent".into(), json!(parent));
        }
        let Some(md) = md else {
            return Ok(record);
        };
        #[cfg(unix)]
        {
            if config.long.owner {
                record.insert("owner".into(), json!(user_name(md.uid(), config)));
            }
            if config.long.group {
                record.insert("group".into(), json!(group_name(md.gid(), config)));
            }
            let file_type = md.file_type();
            if file_type.is_block_device() || file_type.is_char_device() {
                // A type cast is needed here as the `dev_t` type varies across OSes.
                let device = md.rdev() as uucore::libc::dev_t;
                record.insert("major_number".into(), json!(uucore::libc::major(device)));
                record.insert("minor_number".into(), json!(uucore::libc::minor(device)));
            } else {
                record.insert("size".into(), json!(md.len()));
            }
        }
        #[cfg(not(unix))]
        record.insert("size".into(), json!(md.len()));

        let mut date = Vec::new();
        display_date(md, config, &self.recent_time_range, &mut date)?;
        record.insert("date".into(), json!(String::from_utf8_lossy(&date)));
        if let Some(time) = metadata_get_time(md, config.time) {
            record.insert("epoch".into(), json!(system_time_to_sec(time).0));
        }
        Ok(record)
    }
}

#[cfg(unix)]
fn user_name(uid: u32, config: &Config) -> String {
    if config.long.numeric_uid_gid {
        return uid.to_string();
    }
    uucore::entries::uid2usr(uid).unwrap_or_else(|_| uid.to_string())
}

#[cfg(unix)]
fn group_name(gid: u32, config: &Config) -> String {
    if config.long.numeric_uid_gid {
        return gid.to_string();
    }
    uucore::entries::gid2grp(gid).unwrap_or_else(|_| gid.to_string())
}
