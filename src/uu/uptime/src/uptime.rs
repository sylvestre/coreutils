// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore behaviour loadavg nusers

#[cfg(feature = "json")]
mod json;
mod platform;

use clap::{Arg, ArgAction, Command};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use std::io::{self, Write, stdout};
use std::time::Duration;
use uucore::error::{UError, UResult};
use uucore::format_usage;
use uucore::translate;
use uucore::uptime::{
    FormattedUptime, OutputFormat, format_nusers, get_formatted_loadavg, get_formatted_time,
    get_formatted_uptime,
};

pub mod options {
    pub static SINCE: &str = "since";
    pub static PATH: &str = "path";
    pub static PRETTY: &str = "pretty";
}

#[derive(Debug, thiserror::Error)]
pub enum UptimeError {
    // io::Error wrapper
    #[error("{}", translate!("uptime-error-io", "error" => format!("{}", .0)))]
    IoErr(#[from] io::Error),
    #[error("{}", translate!("uptime-error-target-is-dir"))]
    TargetIsDir,
    #[error("{}", translate!("uptime-error-target-is-fifo"))]
    TargetIsFifo,
}

impl UError for UptimeError {
    fn code(&self) -> i32 {
        1
    }
}

#[uucore::main(no_signals)]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uucore::clap_localization::handle_clap_result(uu_app(), args)?;

    // The JSON has the fields of the default output, whatever -s or -p say.
    #[cfg(feature = "json")]
    if uucore::json::is_json(&matches) {
        return json::print_uptime(&matches);
    }
    if matches.get_flag(options::SINCE) {
        return uptime_since();
    }
    if matches.get_flag(options::PRETTY) {
        return pretty_print_uptime();
    }

    let (uptime, users) = platform::uptime_and_users(&matches)?;
    print_time()?;
    match uptime {
        Some(uptime) => print_uptime(uptime)?,
        None => write!(stdout(), "{}", translate!("uptime-output-unknown-uptime"))?,
    }
    write!(stdout(), "{}", format_nusers(users))?;
    print_loadavg()
}

pub fn uu_app() -> Command {
    #[cfg(not(target_env = "musl"))]
    let about = translate!("uptime-about");
    #[cfg(target_env = "musl")]
    let about = translate!("uptime-about") + &translate!("uptime-about-musl-warning");

    let cmd = Command::new("uptime")
        .version(uucore::crate_version!())
        .help_template(uucore::localized_help_template("uptime"))
        .about(about)
        .override_usage(format_usage(&translate!("uptime-usage")))
        .infer_long_args(true)
        .arg(
            Arg::new(options::SINCE)
                .short('s')
                .long(options::SINCE)
                .help(translate!("uptime-help-since"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::PRETTY)
                .short('p')
                .long(options::PRETTY)
                .help(translate!("uptime-help-pretty"))
                .action(ArgAction::SetTrue),
        );
    #[cfg(feature = "json")]
    let cmd = cmd.arg(uucore::json::output_format_arg(translate!(
        "uptime-help-output-format"
    )));
    platform::add_platform_args(cmd)
}

fn uptime_since() -> UResult<()> {
    let uptime = platform::system_uptime()?;

    let since_date = (Timestamp::now() - uptime).to_zoned(TimeZone::system());
    writeln!(stdout(), "{}", since_date.strftime("%Y-%m-%d %H:%M:%S"))?;

    Ok(())
}

/// Prints the load average with its leading separator, or just the line ending
/// where load averages are unavailable (e.g. Windows), as GNU does.
#[inline]
fn print_loadavg() -> UResult<()> {
    if let Ok(s) = get_formatted_loadavg() {
        write!(stdout(), ",  {s}")?;
    }
    writeln!(stdout())?;
    Ok(())
}

fn print_time() -> UResult<()> {
    write!(stdout(), " {} ", get_formatted_time())?;
    Ok(())
}

fn print_uptime(uptime: Duration) -> UResult<()> {
    let localized_text = translate!("uptime-output-up-text");
    let uptime_message = FormattedUptime::new(uptime.as_secs()).get_human_readable_uptime();

    write!(stdout(), "{localized_text} {uptime_message},  ")?;
    Ok(())
}

fn pretty_print_uptime() -> UResult<()> {
    let localized_text = translate!("uptime-output-up-text");
    let uptime_message = get_formatted_uptime(None, OutputFormat::PrettyPrint)?;

    writeln!(stdout(), "{localized_text} {uptime_message}")?;
    Ok(())
}
