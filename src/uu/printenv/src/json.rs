// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: the printed variables, keyed like jc's `env`
//! parser.

use std::env;
use std::ffi::OsStr;

use uucore::error::UResult;
use uucore::json::{Value, json, print_json};

fn record(name: &OsStr, value: &OsStr) -> Value {
    json!({ "name": name.to_string_lossy(), "value": value.to_string_lossy() })
}

/// Prints the whole environment, or the named variables that are set, as a
/// JSON array of `{"name", "value"}` records. As with the text output, a
/// variable that is not set (or a name with `=`) is left out and makes the
/// exit status 1.
pub(crate) fn print_variables(variables: &[String]) -> UResult<()> {
    if variables.is_empty() {
        let records: Value = env::vars_os()
            .map(|(name, value)| record(&name, &value))
            .collect();
        print_json(&records)?;
        return Ok(());
    }
    let mut records = Vec::new();
    let mut error_found = false;
    for name in variables {
        match env::var_os(name).filter(|_| !name.contains('=')) {
            Some(value) => records.push(record(name.as_ref(), &value)),
            None => error_found = true,
        }
    }
    print_json(&Value::Array(records))?;
    if error_found { Err(1.into()) } else { Ok(()) }
}
