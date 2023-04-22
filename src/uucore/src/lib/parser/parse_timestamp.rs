// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
use crate::error::{FromIo, UError, UResult, USimpleError};
use crate::parse_date_common::{
    local_dt_to_filetime, to_local, ISO_8601_FORMAT, POSIX_LOCALE_FORMAT, YYMMDDHHMM_DOT_SS_FORMAT,
    YYMMDDHHMM_FORMAT, YYYYMMDDHHMMSS_FORMAT, YYYYMMDDHHMMS_FORMAT, YYYYMMDDHHMM_DOT_SS_FORMAT,
    YYYYMMDDHHMM_FORMAT, YYYYMMDDHHMM_OFFSET_FORMAT, YYYY_MM_DD_HH_MM_FORMAT,
};
use filetime::FileTime;
use os_display::Quotable;
#[cfg(feature = "time")]
use time::Duration;

pub fn from_str(s: &str) -> UResult<FileTime> {
    // TODO: handle error
    let now = time::OffsetDateTime::now_utc();

    let (mut format, mut ts) = match s.chars().count() {
        15 => (YYYYMMDDHHMM_DOT_SS_FORMAT, s.to_owned()),
        12 => (YYYYMMDDHHMM_FORMAT, s.to_owned()),
        13 => (YYMMDDHHMM_DOT_SS_FORMAT, s.to_owned()),
        10 => (YYMMDDHHMM_FORMAT, s.to_owned()),
        11 => (YYYYMMDDHHMM_DOT_SS_FORMAT, format!("{}{}", now.year(), s)),
        8 => (YYYYMMDDHHMM_FORMAT, format!("{}{}", now.year(), s)),
        _ => {
            return Err(USimpleError::new(
                1,
                format!("invalid date format {}", s.quote()),
            ))
        }
    };
    // workaround time returning Err(TryFromParsed(InsufficientInformation)) for year w/
    // repr:last_two
    // https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=1ccfac7c07c5d1c7887a11decf0e1996
    if s.chars().count() == 10 {
        format = YYYYMMDDHHMM_FORMAT;
        ts = "20".to_owned() + &ts;
    } else if s.chars().count() == 13 {
        format = YYYYMMDDHHMM_DOT_SS_FORMAT;
        ts = "20".to_owned() + &ts;
    }

    let leap_sec = if (format == YYYYMMDDHHMM_DOT_SS_FORMAT || format == YYMMDDHHMM_DOT_SS_FORMAT)
        && ts.ends_with(".60")
    {
        // Work around to disable leap seconds
        // Used in gnu/tests/touch/60-seconds
        ts = ts.replace(".60", ".59");
        true
    } else {
        false
    };

    let tm = time::PrimitiveDateTime::parse(&ts, &format)
        .map_err(|_| USimpleError::new(1, format!("invalid date ts format {}", ts.quote())))?;
    let mut local = to_local(tm);
    if leap_sec {
        // We are dealing with a leap second, add it
        local = local.saturating_add(Duration::SECOND);
    }
    let ft = local_dt_to_filetime(local);

    // // We have to check that ft is valid time. Due to daylight saving
    // // time switch, local time can jump from 1:59 AM to 3:00 AM,
    // // in which case any time between 2:00 AM and 2:59 AM is not valid.
    // // Convert back to local time and see if we got the same value back.
    // let ts = time::Timespec {
    //     sec: ft.unix_seconds(),
    //     nsec: 0,
    // };
    // let tm2 = time::at(ts);
    // if tm.tm_hour != tm2.tm_hour {
    //     return Err(USimpleError::new(
    //         1,
    //         format!("invalid date format {}", s.quote()),
    //     ));
    // }

    Ok(ft)
}
