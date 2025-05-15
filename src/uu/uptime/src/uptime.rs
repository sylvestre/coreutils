// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:ignore getloadavg behaviour loadavg uptime upsecs updays upmins uphours boottime nusers utmpxname gettime clockid
use chrono::{Local, TimeZone, Utc};
use clap::ArgMatches;
use clap::{Arg, ArgAction, Command, ValueHint, builder::ValueParser};
use std::io;
use thiserror::Error;
use uucore::error::{UError, UResult};
use uucore::libc::time_t;
use uucore::uptime::*;
use uucore::{format_usage, help_about, help_usage};
// Import the new locale module
mod locale;
use crate::locale::{LocalizationError, get_message, get_message_with_args};
#[cfg(unix)]
#[cfg(not(target_os = "openbsd"))]
use uucore::utmpx::*;

#[cfg(target_env = "musl")]
const ABOUT: &str = concat!(
    help_about!("uptime.md"),
    "\n\nWarning: When built with musl libc, the `uptime` utility may show '0 users' \n",
    "due to musl's stub implementation of utmpx functions. Boot time and load averages \n",
    "are still calculated using alternative mechanisms."
);
#[cfg(not(target_env = "musl"))]
const ABOUT: &str = help_about!("uptime.md");
const USAGE: &str = help_usage!("uptime.md");

pub mod options {
    pub static SINCE: &str = "since";
    pub static PATH: &str = "path";
}

#[derive(Debug, Error)]
pub enum UptimeError {
    // io::Error wrapper
    #[error("couldn't get boot time: {0}")]
    IoErr(#[from] io::Error),
    #[error("couldn't get boot time: Is a directory")]
    TargetIsDir,
    #[error("couldn't get boot time: Illegal seek")]
    TargetIsFifo,
    #[error("extra operand '{0}'")]
    ExtraOperandError(String),
    #[error("couldn't load localization resources: {0}")]
    LocalizationError(#[from] LocalizationError),
}

impl UError for UptimeError {
    fn code(&self) -> i32 {
        1
    }
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    locale::setup_localization().map_err(UptimeError::from)?;
    let matches = uu_app().try_get_matches_from(args)?;

    #[cfg(windows)]
    return default_uptime(&matches);

    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use uucore::error::set_exit_code;
        use uucore::show_error;

        let argument = matches.get_many::<OsString>(options::PATH);
        // Switches to default uptime behaviour if there is no argument
        if argument.is_none() {
            return default_uptime(&matches);
        }

        let mut arg_iter = argument.unwrap();
        let file_path = arg_iter.next().unwrap();
        if let Some(path) = arg_iter.next() {
            // Uptime doesn't attempt to calculate boot time if there is extra arguments.
            // It's a fatal error
            let mut args = fluent::FluentArgs::new();
            args.set("path", path.to_string_lossy().to_string());

            let extra_operand_msg = get_message_with_args(
                "extra-operand-error",
                args,
                &format!("extra operand '{}'", path.to_string_lossy()),
            );
            show_error!("{}", extra_operand_msg);
            set_exit_code(1);
            return Ok(());
        }
        uptime_with_file(file_path)
    }
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(uucore::crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        .arg(
            Arg::new(options::SINCE)
                .short('s')
                .long(options::SINCE)
                .help("system up since")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::PATH)
                .help("file to search boot time from")
                .action(ArgAction::Append)
                .value_parser(ValueParser::os_string())
                .value_hint(ValueHint::AnyPath),
        )
}

#[cfg(unix)]
fn uptime_with_file(file_path: &std::ffi::OsString) -> UResult<()> {
    use std::fs;
    use std::os::unix::fs::FileTypeExt;
    use uucore::error::set_exit_code;
    use uucore::show_error;

    // Uptime will print loadavg and time to stderr unless we encounter an extra operand.
    let mut non_fatal_error = false;

    // process_utmpx_from_file() doesn't detect or report failures, we check if the path is valid
    // before proceeding with more operations.
    let md_res = fs::metadata(file_path);
    if let Ok(md) = md_res {
        if md.is_dir() {
            let target_is_dir_msg =
                get_message("target-is-dir", "couldn't get boot time: Is a directory");
            show_error!("{}", target_is_dir_msg);
            non_fatal_error = true;
            set_exit_code(1);
        }
        if md.file_type().is_fifo() {
            let target_is_fifo_msg =
                get_message("target-is-fifo", "couldn't get boot time: Illegal seek");
            show_error!("{}", target_is_fifo_msg);
            non_fatal_error = true;
            set_exit_code(1);
        }
    } else if let Err(e) = md_res {
        non_fatal_error = true;
        set_exit_code(1);

        let mut args = fluent::FluentArgs::new();
        args.set("error", e.to_string());

        let io_err_msg =
            get_message_with_args("io-error", args, &format!("couldn't get boot time: {}", e));
        show_error!("{}", io_err_msg);
    }

    // utmpxname() returns an -1 , when filename doesn't end with 'x' or its too long.
    // Reference: `<https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/utmpxname.3.html>`
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let bytes = file_path.as_os_str().as_bytes();
        if bytes[bytes.len() - 1] != b'x' {
            let boot_time_error_msg = get_message("boot-time-error", "couldn't get boot time");
            show_error!("{}", boot_time_error_msg);
            print_time();
            let unknown_uptime_msg = get_message("unknown-uptime", "up ???? days ??:??,");
            print!("{}", unknown_uptime_msg);
            print_nusers(Some(0));
            print_loadavg();
            set_exit_code(1);
            return Ok(());
        }
    }

    if non_fatal_error {
        print_time();
        let unknown_uptime_msg = get_message("unknown-uptime", "up ???? days ??:??,");
        print!("{}", unknown_uptime_msg);
        print_nusers(Some(0));
        print_loadavg();
        return Ok(());
    }

    print_time();
    let user_count;

    #[cfg(not(target_os = "openbsd"))]
    {
        let (boot_time, count) = process_utmpx(Some(file_path));
        if let Some(time) = boot_time {
            print_uptime(Some(time))?;
        } else {
            let boot_time_error_msg = get_message("boot-time-error", "couldn't get boot time");
            show_error!("{}", boot_time_error_msg);
            set_exit_code(1);
            let unknown_uptime_msg = get_message("unknown-uptime", "up ???? days ??:??,");
            print!("{}", unknown_uptime_msg);
        }
        user_count = count;
    }

    #[cfg(target_os = "openbsd")]
    {
        let upsecs = get_uptime(None);
        if upsecs >= 0 {
            print_uptime(Some(upsecs))?;
        } else {
            let boot_time_error_msg = get_message("boot-time-error", "couldn't get boot time");
            show_error!("{}", boot_time_error_msg);
            set_exit_code(1);
            let unknown_uptime_msg = get_message("unknown-uptime", "up ???? days ??:??,");
            print!("{}", unknown_uptime_msg);
        }
        user_count = get_nusers(file_path.to_str().expect("invalid utmp path file"));
    }

    print_nusers(Some(user_count));
    print_loadavg();
    Ok(())
}

/// Default uptime behaviour i.e. when no file argument is given.
fn default_uptime(matches: &ArgMatches) -> UResult<()> {
    if matches.get_flag(options::SINCE) {
        #[cfg(unix)]
        #[cfg(not(target_os = "openbsd"))]
        let (boot_time, _) = process_utmpx(None);

        #[cfg(target_os = "openbsd")]
        let uptime = get_uptime(None)?;

        #[cfg(unix)]
        #[cfg(not(target_os = "openbsd"))]
        let uptime = get_uptime(boot_time)?;

        #[cfg(target_os = "windows")]
        let uptime = get_uptime(None)?;

        let initial_date = Local
            .timestamp_opt(Utc::now().timestamp() - uptime, 0)
            .unwrap();
        println!("{}", initial_date.format("%Y-%m-%d %H:%M:%S"));
        return Ok(());
    }

    print_time();
    print_uptime(None)?;
    print_nusers(None);
    print_loadavg();
    Ok(())
}

#[inline]
fn print_loadavg() {
    let load_average_prefix = get_message("load_average_prefix", "load average");
    match get_formatted_loadavg(&load_average_prefix) {
        Err(_) => {}
        Ok(s) => println!("{s}"),
    }
}

#[cfg(unix)]
#[cfg(not(target_os = "openbsd"))]
fn process_utmpx(file: Option<&std::ffi::OsString>) -> (Option<time_t>, usize) {
    let mut nusers = 0;
    let mut boot_time = None;
    let records = match file {
        Some(f) => Utmpx::iter_all_records_from(f),
        None => Utmpx::iter_all_records(),
    };
    for line in records {
        match line.record_type() {
            USER_PROCESS => nusers += 1,
            BOOT_TIME => {
                let dt = line.login_time();
                if dt.unix_timestamp() > 0 {
                    boot_time = Some(dt.unix_timestamp() as time_t);
                }
            }
            _ => continue,
        }
    }
    (boot_time, nusers)
}

// Simplified print_nusers function
fn print_nusers(nusers: Option<usize>) {
    // Get localized strings for user/users
    let user_singular = get_message("user_singular", "user");
    let user_plural = get_message("user_plural", "users");

    match nusers {
        None => {
            // For None case, use the modified get_formatted_nusers with localized strings
            let formatted = get_formatted_nusers(&user_singular, &user_plural);
            print!("{},  ", formatted);
        }
        Some(nusers) => {
            // Use message with args for better localization
            let mut args = fluent::FluentArgs::new();
            args.set("count", nusers);

            let msg_id = if nusers == 1 {
                "user-count-singular"
            } else {
                "user-count-plural"
            };

            // Get the message with arguments
            let users_text = get_message_with_args(
                msg_id,
                args,
                &format!("{} {}", nusers, if nusers == 1 { "user" } else { "users" }),
            );
            print!("{},  ", users_text);
        }
    }
}

fn print_time() {
    print!(" {}  ", get_formatted_time());
}

fn print_uptime(boot_time: Option<time_t>) -> UResult<()> {
    // Get localized singular and plural forms for "day"
    let day_singular = get_message("day_singular", "day");
    let day_plural = get_message("day_plural", "days");

    // Call the function with all three required parameters
    let uptime_text = match get_formatted_uptime(boot_time, &day_singular, &day_plural) {
        Ok(text) => text,
        Err(e) => return Err(e.into()),
    };

    // Add localized "up" prefix
    let up_prefix = get_message("up-prefix", "up");
    print!("{}  {},  ", up_prefix, uptime_text);
    Ok(())
}
