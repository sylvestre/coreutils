// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: the printed variables, keyed like jc's `env`
//! parser.

use std::env;
use std::ffi::OsString;

use uucore::error::UResult;
use uucore::json::{Value, print_json};

/// Prints the whole environment, or the named variables that are set, as a
/// JSON array of `{"name", "value"}` records. As with the text output, a
/// variable that is not set (or a name with `=`) is left out and makes the
/// exit status 1.
pub(crate) fn print_variables(variables: &[String]) -> UResult<()> {
    let found: Vec<(OsString, OsString)> = if variables.is_empty() {
        env::vars_os().collect()
    } else {
        let names = variables.iter().filter(|name| !name.contains('='));
        names
            .filter_map(|name| Some((name.into(), env::var_os(name)?)))
            .collect()
    };
    let records = found.iter().map(|(name, value)| {
        Value::object([
            ("name", Value::from(name.to_string_lossy())),
            ("value", Value::from(value.to_string_lossy())),
        ])
    });
    print_json(&records.collect())?;
    if found.len() < variables.len() {
        Err(1.into())
    } else {
        Ok(())
    }
}
