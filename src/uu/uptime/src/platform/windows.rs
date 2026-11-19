// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Windows implementation of `uptime`'s platform facade. There are no utmp
//! files on Windows, so the uptime and users come from the system, and `--since`
//! reads the uptime directly from [`uucore::uptime::get_uptime`]
//! (`GetTickCount64`).

use clap::{ArgMatches, Command};
use std::time::Duration;
use uucore::error::UResult;
use uucore::uptime::{default_nusers, get_uptime};

/// No platform-only CLI arguments on Windows (no utmp files).
pub(crate) fn add_platform_args(cmd: Command) -> Command {
    cmd
}

/// The time since boot and the number of users; there is no utmp file
/// operand on Windows.
pub(crate) fn uptime_and_users(_matches: &ArgMatches) -> UResult<(Option<Duration>, usize)> {
    Ok((Some(get_uptime(None)?), default_nusers()))
}

/// The elapsed duration since boot, for `--since`.
pub(crate) fn system_uptime() -> UResult<Duration> {
    get_uptime(None)
}
