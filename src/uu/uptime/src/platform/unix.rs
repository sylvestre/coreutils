// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore loadavg nusers upsecs utmpxname couldnt

//! Unix implementation of `uptime`'s platform facade: the utmp-file operand
//! and the utmpx-derived boot time for `--since` (on OpenBSD, which has no
//! utmpx binding, boot time comes from [`get_uptime`] directly).

use clap::{Arg, ArgAction, ArgMatches, Command, ValueHint, builder::ValueParser};
use std::ffi::OsString;
use std::time::Duration;
use uucore::error::UResult;
use uucore::translate;
use uucore::uptime::{default_nusers, get_uptime};
#[cfg(not(any(target_os = "openbsd", target_os = "android")))]
use uucore::utmpx::{BOOT_TIME, USER_PROCESS, Utmpx};

use crate::{UptimeError, options};

/// Register platform-only CLI arguments: the utmp file operand, which only
/// makes sense where utmp files exist.
pub(crate) fn add_platform_args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new(options::PATH)
            .help(translate!("uptime-help-path"))
            .action(ArgAction::Set)
            .num_args(0..=1)
            .value_parser(ValueParser::os_string())
            .value_hint(ValueHint::AnyPath),
    )
}

/// The time since boot and the number of users: from the utmp file operand
/// if there is one, from the system otherwise. The uptime is `None`, after
/// reporting why, when the file has no usable boot time.
pub(crate) fn uptime_and_users(matches: &ArgMatches) -> UResult<(Option<Duration>, usize)> {
    let Some(file_path) = matches.get_one::<OsString>(options::PATH) else {
        return Ok((Some(get_uptime(None)?), default_nusers()));
    };
    if !utmp_file_is_usable(file_path) {
        return Ok((None, 0));
    }

    #[cfg(not(any(target_os = "openbsd", target_os = "android")))]
    {
        use uucore::error::set_exit_code;
        use uucore::show_error;

        let (boot_time, users) = process_utmpx(Some(file_path));
        let Some(boot_time) = boot_time else {
            show_error!("{}", translate!("uptime-error-couldnt-get-boot-time"));
            set_exit_code(1);
            return Ok((None, users));
        };
        Ok((Some(get_uptime(Some(boot_time))?), users))
    }
    #[cfg(target_os = "openbsd")]
    {
        let users = file_path.to_str().map_or(0, uucore::uptime::get_nusers);
        Ok((Some(get_uptime(None)?), users))
    }
    #[cfg(target_os = "android")]
    Ok((Some(get_uptime(None)?), 0))
}

/// The elapsed duration since boot, for `--since`: derived from the utmpx
/// `BOOT_TIME` record where utmpx is available (on OpenBSD, from
/// [`get_uptime`] directly).
pub(crate) fn system_uptime() -> UResult<Duration> {
    #[cfg(not(any(target_os = "openbsd", target_os = "android")))]
    {
        let (boot_time, _) = process_utmpx(None);
        get_uptime(boot_time)
    }
    #[cfg(any(target_os = "openbsd", target_os = "android"))]
    get_uptime(None)
}

/// Reports why `file_path` cannot be read as a utmp file, if it cannot, and
/// returns whether it can.
fn utmp_file_is_usable(file_path: &OsString) -> bool {
    use std::fs;
    use std::os::unix::fs::FileTypeExt;
    use uucore::error::set_exit_code;
    use uucore::show_error;

    let mut non_fatal_error = false;

    // process_utmpx_from_file() doesn't detect or report failures, we check if the path is valid
    // before proceeding with more operations.
    let md_res = fs::metadata(file_path);
    if let Ok(md) = md_res {
        if md.is_dir() {
            show_error!("{}", UptimeError::TargetIsDir);
            non_fatal_error = true;
            set_exit_code(1);
        }
        if md.file_type().is_fifo() {
            show_error!("{}", UptimeError::TargetIsFifo);
            non_fatal_error = true;
            set_exit_code(1);
        }
    } else if let Err(e) = md_res {
        non_fatal_error = true;
        set_exit_code(1);
        show_error!("{}", UptimeError::IoErr(e));
    }
    // utmpxname() returns an -1 , when filename doesn't end with 'x' or its too long.
    // Reference: `<https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/utmpxname.3.html>`

    #[cfg(target_vendor = "apple")]
    {
        use std::os::unix::ffi::OsStrExt;
        let bytes = file_path.as_os_str().as_bytes();

        if bytes.last() != Some(&b'x') {
            show_error!("{}", translate!("uptime-error-couldnt-get-boot-time"));
            set_exit_code(1);
            return false;
        }
    }

    !non_fatal_error
}

#[cfg(not(any(target_os = "openbsd", target_os = "android")))]
fn process_utmpx(file: Option<&OsString>) -> (Option<i64>, usize) {
    let mut nusers = 0;
    let mut boot_time = None;

    let records = match file {
        Some(f) => Utmpx::iter_all_records_from(f),
        None => Utmpx::iter_all_records(),
    };

    for line in records {
        match line.record_type() {
            x if x == USER_PROCESS => nusers += 1,
            x if x == BOOT_TIME => {
                let dt = line.login_time();
                if dt.unix_timestamp() > 0 {
                    boot_time = Some(dt.unix_timestamp());
                }
            }
            _ => (),
        }
    }
    (boot_time, nusers)
}
