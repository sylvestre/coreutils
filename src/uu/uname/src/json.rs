// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: the printed fields, keyed like jc's `uname` parser.

use uucore::json::Value;

use crate::UNameOutput;

/// One key per field the text output would print, in the same order.
pub(crate) fn to_json(output: &UNameOutput) -> Value {
    let fields = [
        ("kernel_name", &output.kernel_name),
        ("node_name", &output.nodename),
        ("kernel_release", &output.kernel_release),
        ("kernel_version", &output.kernel_version),
        ("machine", &output.machine),
        ("processor", &output.processor),
        ("hardware_platform", &output.hardware_platform),
        ("operating_system", &output.os),
    ];
    let printed = fields
        .into_iter()
        .filter_map(|(key, value)| Some((key, value.as_ref()?.to_string_lossy())));
    Value::Object(printed.collect())
}
