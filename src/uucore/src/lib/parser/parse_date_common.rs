// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
use filetime::FileTime;
use time::macros::{format_description, offset, time};

pub const POSIX_LOCALE_FORMAT: &[time::format_description::FormatItem] = format_description!(
    "[weekday repr:short] [month repr:short] [day padding:space] \
    [hour]:[minute]:[second] [year]"
);

pub const ISO_8601_FORMAT: &[time::format_description::FormatItem] =
    format_description!("[year]-[month]-[day]");

// "%Y%m%d%H%M.%S" 15 chars
pub const YYYYMMDDHHMM_DOT_SS_FORMAT: &[time::format_description::FormatItem] = format_description!(
    "[year repr:full][month repr:numerical padding:zero]\
    [day][hour][minute].[second]"
);

// "%Y-%m-%d %H:%M:%S.%SS" 12 chars
pub const YYYYMMDDHHMMSS_FORMAT: &[time::format_description::FormatItem] = format_description!(
    "[year repr:full]-[month repr:numerical padding:zero]-\
    [day] [hour]:[minute]:[second].[subsecond]"
);

// "%Y-%m-%d %H:%M:%S" 12 chars
pub const YYYYMMDDHHMMS_FORMAT: &[time::format_description::FormatItem] = format_description!(
    "[year repr:full]-[month repr:numerical padding:zero]-\
    [day] [hour]:[minute]:[second]"
);

// "%Y-%m-%d %H:%M" 12 chars
// Used for example in tests/touch/no-rights.sh
pub const YYYY_MM_DD_HH_MM_FORMAT: &[time::format_description::FormatItem] = format_description!(
    "[year repr:full]-[month repr:numerical padding:zero]-\
    [day] [hour]:[minute]"
);

// "%Y%m%d%H%M" 12 chars
pub const YYYYMMDDHHMM_FORMAT: &[time::format_description::FormatItem] = format_description!(
    "[year repr:full][month repr:numerical padding:zero]\
    [day][hour][minute]"
);

// "%y%m%d%H%M.%S" 13 chars
pub const YYMMDDHHMM_DOT_SS_FORMAT: &[time::format_description::FormatItem] = format_description!(
    "[year repr:last_two padding:none][month][day]\
    [hour][minute].[second]"
);

// "%y%m%d%H%M" 10 chars
pub const YYMMDDHHMM_FORMAT: &[time::format_description::FormatItem] = format_description!(
    "[year repr:last_two padding:none][month padding:zero][day padding:zero]\
    [hour repr:24 padding:zero][minute padding:zero]"
);

// "%Y-%m-%d %H:%M +offset"
// Used for example in tests/touch/relative.sh
pub const YYYYMMDDHHMM_OFFSET_FORMAT: &[time::format_description::FormatItem] = format_description!(
    "[year]-[month]-[day] [hour repr:24]:[minute] \
    [offset_hour sign:mandatory][offset_minute]"
);

// Convert a date/time with a TZ offset into a FileTime
pub fn local_dt_to_filetime(dt: time::OffsetDateTime) -> FileTime {
    FileTime::from_unix_time(dt.unix_timestamp(), dt.nanosecond())
}

// Convert a date/time to a date with a TZ offset
pub fn to_local(tm: time::PrimitiveDateTime) -> time::OffsetDateTime {
    let offset = match time::OffsetDateTime::now_local() {
        Ok(lo) => lo.offset(),
        Err(e) => {
            panic!("error: {e}");
        }
    };
    tm.assume_offset(offset)
}

// Convert a date/time, considering that the input is in UTC time
// Used for touch -d 1970-01-01 18:43:33.023456789 for example
pub fn dt_to_filename(tm: time::PrimitiveDateTime) -> FileTime {
    let dt = tm.assume_offset(offset!(UTC));
    local_dt_to_filetime(dt)
}
