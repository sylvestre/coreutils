// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:ignore (path) osrelease

//! Helper clap functions to localize error handling and options
//!

use crate::error::{ClapErrorWrapper, UError};
use crate::locale::{get_message, get_message_with_args};
use clap::Command;
use clap::error::{ContextKind, ErrorKind};
use std::collections::HashMap;
use std::process;

/// Apply color to text using ANSI escape codes
fn colorize(text: &str, color_code: &str) -> String {
    format!("\x1b[{}m{}\x1b[0m", color_code, text)
}

/// Color constants for consistent styling
mod colors {
    pub const RED: &str = "31";
    pub const YELLOW: &str = "33";
    pub const GREEN: &str = "32";
}

/// Helper to create a HashMap with colored arguments for Fluent messages
fn create_message_args<const N: usize>(pairs: [(&str, String); N]) -> HashMap<String, String> {
    HashMap::from(pairs.map(|(k, v)| (k.to_string(), v)))
}

/// The goal of this function is to be able to set
/// translation infrastructure for clap commands.
pub fn uu_app_common(name: impl Into<clap::builder::Str>) -> Command {
    Command::new(name)
        .disable_help_flag(true)
        .disable_version_flag(true)
    // TODO: Add localized help template
    // .help_template(get_message("help-template"))
    // .next_help_heading(get_message("help-heading-options"))
    // TODO add RichFormatter for better error messages
}

/// Handle clap errors with localized and colored output
///
/// This function processes clap errors and provides localized error messages
/// with appropriate coloring for better user experience.
///
/// # Arguments
/// * `err` - The clap error wrapper to handle
/// * `util_name` - Name of the utility for error context
///
/// # Returns
/// * Never returns (calls process::exit)
pub fn handle_clap_error(err: ClapErrorWrapper, util_name: &str) -> ! {
    let clap_err = &err.error;
    match clap_err.kind() {
        ErrorKind::UnknownArgument => {
            if let Some(invalid_arg) = clap_err.get(ContextKind::InvalidArg) {
                let arg_str = invalid_arg.to_string();

                // Get the uncolored words from common strings
                let error_word = get_message("common-error");
                let tip_word = get_message("common-tip");

                // Prepare colored components
                let colored_arg = colorize(&arg_str, colors::YELLOW);
                let colored_error_word = colorize(&error_word, colors::RED);
                let colored_tip_word = colorize(&tip_word, colors::GREEN);

                // Print main error message
                let error_msg = get_message_with_args(
                    "clap-error-unexpected-argument",
                    create_message_args([
                        ("arg", colored_arg.clone()),
                        ("error_word", colored_error_word),
                    ]),
                );
                eprintln!("{}", error_msg);
                eprintln!();

                // Show suggestion or generic tip
                let suggestion = clap_err.get(ContextKind::SuggestedArg);
                if let Some(suggested_arg) = suggestion {
                    let suggestion_msg = get_message_with_args(
                        "clap-error-similar-argument",
                        create_message_args([
                            ("tip_word", colored_tip_word),
                            (
                                "suggestion",
                                colorize(&suggested_arg.to_string(), colors::GREEN),
                            ),
                        ]),
                    );
                    eprintln!("  {}", suggestion_msg);
                } else {
                    let tip_msg = get_message_with_args(
                        "clap-error-pass-as-value",
                        create_message_args([
                            ("arg", colored_arg),
                            ("tip_word", colored_tip_word),
                            (
                                "tip_command",
                                colorize(&format!("-- {}", arg_str), colors::GREEN),
                            ),
                        ]),
                    );
                    eprintln!("  {}", tip_msg);
                }

                // Show usage and help
                eprintln!();
                let usage_label = get_message("common-usage");
                let usage_pattern = get_message(&format!("{}-usage", util_name));
                eprintln!("{}: {}", usage_label, usage_pattern);
                eprintln!();

                let help_msg = get_message_with_args(
                    "clap-error-help-suggestion",
                    create_message_args([("command", util_name.to_string())]),
                );
                eprintln!("{}", help_msg);
            } else {
                // Generic fallback case using common strings
                let error_word = get_message("common-error");
                let colored_error_word = colorize(&error_word, colors::RED);
                eprintln!("{}: unexpected argument", colored_error_word);
            }
        }
        _ => {
            // For other errors, use the default clap handling
            eprint!("{}", clap_err);
        }
    }
    process::exit(err.code());
}

/// Convenience macro to wrap clap error handling
///
/// Usage: `init_clap_with_l10n!(app.try_get_matches_from(args))`
#[macro_export]
macro_rules! init_clap_with_l10n {
    ($result:expr) => {
        match $result {
            Ok(matches) => matches,
            Err(err) => {
                $crate::clap_localization::handle_clap_error(err.into(), $crate::util_name())
            }
        }
    };
    ($result:expr, $exit_code:expr) => {
        match $result {
            Ok(matches) => matches,
            Err(err) => {
                use $crate::error::UClapError;
                $crate::clap_localization::handle_clap_error(
                    err.with_exit_code($exit_code),
                    $crate::util_name(),
                )
            }
        }
    };
}
