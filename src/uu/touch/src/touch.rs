// This file is part of the uutils coreutils package.
//
// (c) Nick Platt <platt.nicholas@gmail.com>
// (c) Jian Zeng <anonymousknight96 AT gmail.com>
//
// For the full copyright and license information, please view the LICENSE file
// that was distributed with this source code.

// spell-checker:ignore (ToDO) filetime strptime utcoff strs datetime MMDDhhmm clapv PWSTR lpszfilepath hresult mktime YYYYMMDDHHMM YYMMDDHHMM DATETIME YYYYMMDDHHMMS subsecond

use clap::builder::ValueParser;
use clap::{crate_version, Arg, ArgAction, ArgGroup, Command};
use filetime::{set_symlink_file_times, FileTime};
use std::ffi::OsString;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use time::macros::{format_description, offset, time};
use time::Duration;
use uucore::display::Quotable;
use uucore::error::{FromIo, UError, UResult, USimpleError};
use uucore::parse_date;
use uucore::parse_date_common::local_dt_to_filetime;
use uucore::parse_relative_time;
use uucore::parse_timestamp;

use uucore::{format_usage, help_about, help_usage, show};

const ABOUT: &str = help_about!("touch.md");
const USAGE: &str = help_usage!("touch.md");

pub mod options {
    // Both SOURCES and sources are needed as we need to be able to refer to the ArgGroup.
    pub static SOURCES: &str = "sources";
    pub mod sources {
        pub static DATE: &str = "date";
        pub static REFERENCE: &str = "reference";
        pub static CURRENT: &str = "current";
    }
    pub static HELP: &str = "help";
    pub static ACCESS: &str = "access";
    pub static MODIFICATION: &str = "modification";
    pub static NO_CREATE: &str = "no-create";
    pub static NO_DEREF: &str = "no-dereference";
    pub static TIME: &str = "time";
}

static ARG_FILES: &str = "files";

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().try_get_matches_from(args)?;

    let files = matches.get_many::<OsString>(ARG_FILES).ok_or_else(|| {
        USimpleError::new(
            1,
            format!(
                "missing file operand\nTry '{} --help' for more information.",
                uucore::execution_phrase()
            ),
        )
    })?;
    let (mut atime, mut mtime) = match (
        matches.get_one::<OsString>(options::sources::REFERENCE),
        matches.get_one::<String>(options::sources::DATE),
    ) {
        (Some(reference), Some(date)) => {
            let (atime, mtime) = stat(Path::new(reference), !matches.get_flag(options::NO_DEREF))?;
            if let Some(offset) = parse_relative_time::from_str(date) {
                let mut seconds = offset.whole_seconds();
                let mut nanos = offset.subsec_nanoseconds();
                if nanos < 0 {
                    nanos += 1_000_000_000;
                    seconds -= 1;
                }

                let ref_atime_secs = atime.unix_seconds();
                let ref_atime_nanos = atime.nanoseconds();
                let atime = FileTime::from_unix_time(
                    ref_atime_secs + seconds,
                    ref_atime_nanos + nanos as u32,
                );

                let ref_mtime_secs = mtime.unix_seconds();
                let ref_mtime_nanos = mtime.nanoseconds();
                let mtime = FileTime::from_unix_time(
                    ref_mtime_secs + seconds,
                    ref_mtime_nanos + nanos as u32,
                );

                (atime, mtime)
            } else {
                let timestamp = parse_date::from_str(date)?;
                (timestamp, timestamp)
            }
        }
        (Some(reference), None) => {
            stat(Path::new(reference), !matches.get_flag(options::NO_DEREF))?
        }
        (None, Some(date)) => {
            let timestamp = parse_date::from_str(date)?;
            (timestamp, timestamp)
        }
        (None, None) => {
            let timestamp =
                if let Some(current) = matches.get_one::<String>(options::sources::CURRENT) {
                    parse_timestamp::from_str(current)?
                } else {
                    local_dt_to_filetime(time::OffsetDateTime::now_local().unwrap())
                };
            (timestamp, timestamp)
        }
    };

    for filename in files {
        // FIXME: find a way to avoid having to clone the path
        let pathbuf = if filename == "-" {
            pathbuf_from_stdout()?
        } else {
            PathBuf::from(filename)
        };

        let path = pathbuf.as_path();

        if let Err(e) = path.metadata() {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.map_err_context(|| format!("setting times of {}", filename.quote())));
            }

            if matches.get_flag(options::NO_CREATE) {
                continue;
            }

            if matches.get_flag(options::NO_DEREF) {
                show!(USimpleError::new(
                    1,
                    format!(
                        "setting times of {}: No such file or directory",
                        filename.quote()
                    )
                ));
                continue;
            }

            if let Err(e) = File::create(path) {
                show!(e.map_err_context(|| format!("cannot touch {}", path.quote())));
                continue;
            };

            // Minor optimization: if no reference time was specified, we're done.
            if !matches.contains_id(options::SOURCES) {
                continue;
            }
        }

        // If changing "only" atime or mtime, grab the existing value of the other.
        // Note that "-a" and "-m" may be passed together; this is not an xor.
        if matches.get_flag(options::ACCESS)
            || matches.get_flag(options::MODIFICATION)
            || matches.contains_id(options::TIME)
        {
            let st = stat(path, !matches.get_flag(options::NO_DEREF))?;
            let time = matches
                .get_one::<String>(options::TIME)
                .map(|s| s.as_str())
                .unwrap_or("");

            if !(matches.get_flag(options::ACCESS)
                || time.contains(&"access".to_owned())
                || time.contains(&"atime".to_owned())
                || time.contains(&"use".to_owned()))
            {
                atime = st.0;
            }

            if !(matches.get_flag(options::MODIFICATION)
                || time.contains(&"modify".to_owned())
                || time.contains(&"mtime".to_owned()))
            {
                mtime = st.1;
            }
        }

        if matches.get_flag(options::NO_DEREF) {
            set_symlink_file_times(path, atime, mtime)
        } else {
            filetime::set_file_times(path, atime, mtime)
        }
        .map_err_context(|| format!("setting times of {}", path.quote()))?;
    }

    Ok(())
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        .disable_help_flag(true)
        .arg(
            Arg::new(options::HELP)
                .long(options::HELP)
                .help("Print help information.")
                .action(ArgAction::Help),
        )
        .arg(
            Arg::new(options::ACCESS)
                .short('a')
                .help("change only the access time")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::sources::CURRENT)
                .short('t')
                .help("use [[CC]YY]MMDDhhmm[.ss] instead of the current time")
                .value_name("STAMP"),
        )
        .arg(
            Arg::new(options::sources::DATE)
                .short('d')
                .long(options::sources::DATE)
                .allow_hyphen_values(true)
                .help("parse argument and use it instead of current time")
                .value_name("STRING")
                .conflicts_with(options::sources::CURRENT),
        )
        .arg(
            Arg::new(options::MODIFICATION)
                .short('m')
                .help("change only the modification time")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::NO_CREATE)
                .short('c')
                .long(options::NO_CREATE)
                .help("do not create any files")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::NO_DEREF)
                .short('h')
                .long(options::NO_DEREF)
                .help(
                    "affect each symbolic link instead of any referenced file \
                     (only for systems that can change the timestamps of a symlink)",
                )
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::sources::REFERENCE)
                .short('r')
                .long(options::sources::REFERENCE)
                .help("use this file's times instead of the current time")
                .value_name("FILE")
                .value_parser(ValueParser::os_string())
                .value_hint(clap::ValueHint::AnyPath)
                .conflicts_with(options::sources::CURRENT),
        )
        .arg(
            Arg::new(options::TIME)
                .long(options::TIME)
                .help(
                    "change only the specified time: \"access\", \"atime\", or \
                     \"use\" are equivalent to -a; \"modify\" or \"mtime\" are \
                     equivalent to -m",
                )
                .value_name("WORD")
                .value_parser(["access", "atime", "use"]),
        )
        .arg(
            Arg::new(ARG_FILES)
                .action(ArgAction::Append)
                .num_args(1..)
                .value_parser(ValueParser::os_string())
                .value_hint(clap::ValueHint::AnyPath),
        )
        .group(
            ArgGroup::new(options::SOURCES)
                .args([
                    options::sources::CURRENT,
                    options::sources::DATE,
                    options::sources::REFERENCE,
                ])
                .multiple(true),
        )
}

fn stat(path: &Path, follow: bool) -> UResult<(FileTime, FileTime)> {
    let metadata = if follow {
        fs::symlink_metadata(path)
    } else {
        fs::metadata(path)
    }
    .map_err_context(|| format!("failed to get attributes of {}", path.quote()))?;

    Ok((
        FileTime::from_last_access_time(&metadata),
        FileTime::from_last_modification_time(&metadata),
    ))
}

// TODO: this may be a good candidate to put in fsext.rs
/// Returns a PathBuf to stdout.
///
/// On Windows, uses GetFinalPathNameByHandleW to attempt to get the path
/// from the stdout handle.
fn pathbuf_from_stdout() -> UResult<PathBuf> {
    #[cfg(all(unix, not(target_os = "android")))]
    {
        Ok(PathBuf::from("/dev/stdout"))
    }
    #[cfg(target_os = "android")]
    {
        Ok(PathBuf::from("/proc/self/fd/1"))
    }
    #[cfg(windows)]
    {
        use std::os::windows::prelude::AsRawHandle;
        use windows_sys::Win32::Foundation::{
            GetLastError, ERROR_INVALID_PARAMETER, ERROR_NOT_ENOUGH_MEMORY, ERROR_PATH_NOT_FOUND,
            HANDLE, MAX_PATH,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            GetFinalPathNameByHandleW, FILE_NAME_OPENED,
        };

        let handle = std::io::stdout().lock().as_raw_handle() as HANDLE;
        let mut file_path_buffer: [u16; MAX_PATH as usize] = [0; MAX_PATH as usize];

        // https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlea#examples
        // SAFETY: We transmute the handle to be able to cast *mut c_void into a
        // HANDLE (i32) so rustc will let us call GetFinalPathNameByHandleW. The
        // reference example code for GetFinalPathNameByHandleW implies that
        // it is safe for us to leave lpszfilepath uninitialized, so long as
        // the buffer size is correct. We know the buffer size (MAX_PATH) at
        // compile time. MAX_PATH is a small number (260) so we can cast it
        // to a u32.
        let ret = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                file_path_buffer.as_mut_ptr(),
                file_path_buffer.len() as u32,
                FILE_NAME_OPENED,
            )
        };

        let buffer_size = match ret {
            ERROR_PATH_NOT_FOUND | ERROR_NOT_ENOUGH_MEMORY | ERROR_INVALID_PARAMETER => {
                return Err(USimpleError::new(
                    1,
                    format!("GetFinalPathNameByHandleW failed with code {ret}"),
                ))
            }
            e if e == 0 => {
                return Err(USimpleError::new(
                    1,
                    format!(
                        "GetFinalPathNameByHandleW failed with code {}",
                        // SAFETY: GetLastError is thread-safe and has no documented memory unsafety.
                        unsafe { GetLastError() }
                    ),
                ));
            }
            e => e as usize,
        };

        // Don't include the null terminator
        Ok(String::from_utf16(&file_path_buffer[0..buffer_size])
            .map_err(|e| USimpleError::new(1, e.to_string()))?
            .into())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn test_get_pathbuf_from_stdout_fails_if_stdout_is_not_a_file() {
        // We can trigger an error by not setting stdout to anything (will
        // fail with code 1)
        assert!(super::pathbuf_from_stdout()
            .expect_err("pathbuf_from_stdout should have failed")
            .to_string()
            .contains("GetFinalPathNameByHandleW failed with code 1"));
    }
}
