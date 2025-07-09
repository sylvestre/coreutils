// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
use clap::{Arg, ArgAction};
use std::collections::HashMap;
use std::ffi::OsString;
use uucore::clap_localization::handle_clap_error;
use uucore::display::println_verbatim;
use uucore::error::{FromIo, UResult};
use uucore::locale::{get_message, get_message_with_args};
mod platform;

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    // Use try_get_matches_from to catch errors and apply custom handling
    match uu_app().try_get_matches_from(args) {
        Ok(_) => {
            let username = whoami()?;
            println_verbatim(username)
                .map_err_context(|| get_message("whoami-error-failed-to-print"))?;
            Ok(())
        }
        Err(err) => {
            // Use the centralized clap error handler
            handle_clap_error(err, "whoami");
        }
    }
}

/// Get the current username
pub fn whoami() -> UResult<OsString> {
    platform::get_username().map_err_context(|| get_message("whoami-error-failed-to-get"))
}

pub fn uu_app() -> clap::Command {
    use uucore::clap_localization::uu_app_common;

    uu_app_common(uucore::util_name().to_string())
        .version(uucore::crate_version!())
        .about(get_message("whoami-about"))
        .override_usage(get_message_with_args(
            "whoami-usage-format",
            HashMap::from([("util_name".to_string(), uucore::util_name().to_string())]),
        ))
        .help_template(format!(
            "{} {{usage}}\n\n{{about}}\n\n{}\n{{options}}",
            get_message("whoami-help-usage"),
            get_message("whoami-help-options")
        ))
        .next_help_heading(get_message("whoami-help-options"))
        // Add localized help flag
        .arg(
            Arg::new("help")
                .long("help")
                .short('h')
                .action(ArgAction::Help)
                .help(get_message("whoami-help-flag-help"))
                .help_heading(get_message("whoami-help-options")),
        )
        // Add localized version flag
        .arg(
            Arg::new("version")
                .long("version")
                .short('V')
                .action(ArgAction::Version)
                .help(get_message("whoami-version-flag-help"))
                .help_heading(get_message("whoami-help-options")),
        )
        .infer_long_args(true)
}
