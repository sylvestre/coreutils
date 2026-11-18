// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: a record per listed entry, keyed like jc's `ls`
//! parser. A long listing adds the fields `ls -l` prints.

use std::ops::RangeInclusive;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

#[cfg(unix)]
use rustc_hash::FxHashMap;

use uucore::error::UResult;
use uucore::fs::display_permissions;
use uucore::fsext::metadata_get_time;
use uucore::json::{ArrayPrinter, Map, Value};
use uucore::time::system_time_to_sec;

use crate::display::display_date;
#[cfg(unix)]
use crate::display::{display_group, display_uname};
use crate::output::{LsOutput, StreamMode};
use crate::{Config, LsError, PathData, list_with_output};

pub fn list_json(locs: Vec<&Path>, config: &Config) -> UResult<()> {
    let now = SystemTime::now();
    let mut output = JsonOutput {
        printer: ArrayPrinter::new(),
        parent: None,
        recent_time_range: (now - Duration::new(31_556_952 / 2, 0))..=now,
        #[cfg(unix)]
        uid_cache: FxHashMap::default(),
        #[cfg(unix)]
        gid_cache: FxHashMap::default(),
    };
    list_with_output(locs, config, &mut output)?;
    Ok(output.printer.finish().map_err(LsError::WriteError)?)
}

struct JsonOutput {
    printer: ArrayPrinter,
    /// The directory being listed, once ls would print its header (`-R` or
    /// several operands), so its entries can be told apart.
    parent: Option<String>,
    recent_time_range: RangeInclusive<SystemTime>,
    #[cfg(unix)]
    uid_cache: FxHashMap<u32, String>,
    #[cfg(unix)]
    gid_cache: FxHashMap<u32, String>,
}

impl LsOutput for JsonOutput {
    fn stream_mode(&self) -> StreamMode {
        StreamMode::Batch
    }

    fn write_entries(&mut self, entries: &[PathData], config: &Config) -> UResult<()> {
        for entry in entries {
            let record = Value::Object(self.record(entry, config)?);
            self.printer.push(&record).map_err(LsError::WriteError)?;
        }
        Ok(())
    }

    fn write_dir_header(&mut self, path_data: &PathData, _: &Config, _: bool) -> UResult<()> {
        self.parent = Some(path_data.path().to_string_lossy().into_owned());
        Ok(())
    }

    /// Keeps the entries listed so far before an error message.
    fn flush(&mut self) -> UResult<()> {
        Ok(self.printer.flush().map_err(LsError::WriteError)?)
    }
}

impl JsonOutput {
    fn record(&mut self, entry: &PathData, config: &Config) -> UResult<Map> {
        let mut record = Map::new();
        record.insert("filename", entry.display_name().to_string_lossy());
        if let Some(parent) = &self.parent {
            record.insert("parent", parent.as_str());
        }
        // Only a long listing needs the metadata.
        let Some(md) = config.json_long.then(|| entry.metadata()).flatten() else {
            return Ok(record);
        };
        if md.file_type().is_symlink()
            && let Ok(target) = entry.path().read_link()
        {
            record.insert("link_to", target.to_string_lossy());
        }
        record.insert("flags", display_permissions(md, true));
        #[cfg(unix)]
        {
            record.insert("links", md.nlink());
            if config.long.owner {
                let owner = display_uname(md, config, &mut self.uid_cache);
                record.insert("owner", owner);
            }
            if config.long.group {
                let group = display_group(md, config, &mut self.gid_cache);
                record.insert("group", group);
            }
        }
        record.insert("size", md.len());
        let mut date = Vec::new();
        display_date(md, config, &self.recent_time_range, &mut date)?;
        record.insert("date", String::from_utf8_lossy(&date));
        if let Some(time) = metadata_get_time(md, config.time) {
            record.insert("epoch", system_time_to_sec(time).0);
        }
        Ok(record)
    }
}
