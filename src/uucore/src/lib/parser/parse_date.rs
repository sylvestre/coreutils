// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
use crate::error::{FromIo, UError, UResult, USimpleError};
use crate::parse_date_common::{
    dt_to_filename, local_dt_to_filetime, to_local, ISO_8601_FORMAT, POSIX_LOCALE_FORMAT,
    YYMMDDHHMM_DOT_SS_FORMAT, YYMMDDHHMM_FORMAT, YYYYMMDDHHMMSS_FORMAT, YYYYMMDDHHMMS_FORMAT,
    YYYYMMDDHHMM_DOT_SS_FORMAT, YYYYMMDDHHMM_FORMAT, YYYYMMDDHHMM_OFFSET_FORMAT,
    YYYY_MM_DD_HH_MM_FORMAT,
};
use crate::parse_relative_time;
use filetime::{set_symlink_file_times, FileTime};
use time::macros::{format_description, offset, time};
use time::{format_description, OffsetDateTime, PrimitiveDateTime};

pub fn from_str(s: &str) -> UResult<FileTime> {
    // This isn't actually compatible with GNU touch, but there doesn't seem to
    // be any simple specification for what format this parameter allows and I'm
    // not about to implement GNU parse_datetime.
    // http://git.savannah.gnu.org/gitweb/?p=gnulib.git;a=blob_plain;f=lib/parse-datetime.y

    // TODO: match on char count?

    // "The preferred date and time representation for the current locale."
    // "(In the POSIX locale this is equivalent to %a %b %e %H:%M:%S %Y.)"
    // time 0.1.43 parsed this as 'a b e T Y'
    // which is equivalent to the POSIX locale: %a %b %e %H:%M:%S %Y
    // Tue Dec  3 ...
    // ("%c", POSIX_LOCALE_FORMAT),
    //

    if let Ok(parsed) = time::PrimitiveDateTime::parse(s, &POSIX_LOCALE_FORMAT) {
        return Ok(local_dt_to_filetime(to_local(parsed)));
    }

    // Also support other formats found in the GNU tests like
    // in tests/misc/stat-nanoseconds.sh
    // or tests/touch/no-rights.sh
    for fmt in [
        YYYYMMDDHHMMS_FORMAT,
        YYYYMMDDHHMMSS_FORMAT,
        YYYY_MM_DD_HH_MM_FORMAT,
        YYYYMMDDHHMM_OFFSET_FORMAT,
    ] {
        if let Ok(parsed) = time::PrimitiveDateTime::parse(s, &fmt) {
            return Ok(dt_to_filename(parsed));
        }
    }

    // "Equivalent to %Y-%m-%d (the ISO 8601 date format). (C99)"
    // ("%F", ISO_8601_FORMAT),
    if let Ok(parsed) = time::Date::parse(s, &ISO_8601_FORMAT) {
        return Ok(local_dt_to_filetime(to_local(
            time::PrimitiveDateTime::new(parsed, time!(00:00)),
        )));
    }

    // "@%s" is "The number of seconds since the Epoch, 1970-01-01 00:00:00 +0000 (UTC). (TZ) (Calculated from mktime(tm).)"
    if s.bytes().next() == Some(b'@') {
        if let Ok(ts) = &s[1..].parse::<i64>() {
            // Don't convert to local time in this case - seconds since epoch are not time-zone dependent
            return Ok(local_dt_to_filetime(
                time::OffsetDateTime::from_unix_timestamp(*ts).unwrap(),
            ));
        }
    }

    if let Some(duration) = parse_relative_time::from_str(s) {
        let now_local = time::OffsetDateTime::now_local().unwrap();
        let diff = now_local.checked_add(duration).unwrap();
        return Ok(local_dt_to_filetime(diff));
    }

    Err(USimpleError::new(1, format!("Unable to parse date: {s}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn assert_file_time_eq(s: &str, expected: FileTime) {
        let parsed = from_str(s).unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_from_str_posix_locale_format() {
        let s = "2023-04-23T12:34:56";
        let expected = local_dt_to_filetime(OffsetDateTime::from_str(s).unwrap());
        assert_file_time_eq(s, expected);
    }

    #[test]
    fn test_from_str_various_formats() {
        let formats = [
            ("20230423123456", YYYYMMDDHHMMS_FORMAT),
            ("20230423123456.123", YYYYMMDDHHMMSS_FORMAT),
            ("2023-04-23_12-34", YYYY_MM_DD_HH_MM_FORMAT),
            ("202304231234-0500", YYYYMMDDHHMM_OFFSET_FORMAT),
        ];

        for (s, fmt) in formats.iter() {
            let expected = dt_to_filename(time::PrimitiveDateTime::parse(s, fmt).unwrap());
            assert_file_time_eq(s, expected);
        }
    }

    #[test]
    fn test_from_str_iso_8601_format() {
        let s = "2023-04-23";
        let expected = local_dt_to_filetime(to_local(time::PrimitiveDateTime::new(
            time::Date::parse(s, &ISO_8601_FORMAT).unwrap(),
            time!(00:00),
        )));
        assert_file_time_eq(s, expected);
    }

    #[test]
    fn test_from_str_seconds_since_epoch() {
        let s = "@1609459200";
        let expected =
            local_dt_to_filetime(time::OffsetDateTime::from_unix_timestamp(1609459200).unwrap());
        assert_file_time_eq(s, expected);
    }

    #[test]
    fn test_from_str_relative_time() {
        let s = "+1d";
        let duration = parse_relative_time::from_str(s).unwrap();
        let now_local = time::OffsetDateTime::now_local().unwrap();
        let expected = local_dt_to_filetime(now_local.checked_add(duration).unwrap());
        let parsed = from_str(s).unwrap();
        assert!(parsed.duration_since(expected).unwrap().abs() < time::Duration::seconds(1));
    }

    #[test]
    fn test_from_str_invalid_input() {
        let s = "invalid";
        assert!(from_str(s).is_err());
    }
}
