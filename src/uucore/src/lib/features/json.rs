// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=FORMAT`, where FORMAT is `text` (the default) or `json`.

use std::io::{self, IsTerminal, Write, stdout};

use clap::{Arg, ArgMatches};
pub use serde_json::{Map, Value, json};

pub const OUTPUT_FORMAT: &str = "output-format";

/// The `--output-format` argument; the last one given wins.
pub fn output_format_arg(help: String) -> Arg {
    Arg::new(OUTPUT_FORMAT)
        .long(OUTPUT_FORMAT)
        .value_name("FORMAT")
        .value_parser(["text", "json"])
        .overrides_with(OUTPUT_FORMAT)
        .help(help)
}

/// Whether `--output-format=json` was given.
pub fn is_json(matches: &ArgMatches) -> bool {
    matches
        .get_one::<String>(OUTPUT_FORMAT)
        .is_some_and(|format| format == "json")
}

/// Prints `value` to stdout and a newline: indented on a terminal, so it is
/// easy to read, and on a single line otherwise, so it is easy to process.
pub fn print_json(value: &Value) -> io::Result<()> {
    let mut stdout = stdout().lock();
    if stdout.is_terminal() {
        serde_json::to_writer_pretty(&mut stdout, value)?;
    } else {
        serde_json::to_writer(&mut stdout, value)?;
    }
    writeln!(stdout)?;
    stdout.flush()
}
