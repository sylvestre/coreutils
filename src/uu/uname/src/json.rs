// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: the printed fields, keyed like jc's `uname` parser.

use uucore::json::{Map, Value, json};

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
    let mut record = Map::new();
    for (key, value) in fields {
        if let Some(value) = value {
            record.insert(key.into(), json!(value.to_string_lossy()));
        }
    }
    Value::Object(record)
}

#[cfg(test)]
mod tests {
    use super::to_json;
    use crate::UNameOutput;
    use uucore::json::json;

    #[test]
    fn test_only_printed_fields_with_jc_keys() {
        let output = UNameOutput {
            kernel_name: Some("Plan9".into()),
            nodename: Some("cpu1".into()),
            kernel_release: None,
            kernel_version: None,
            machine: Some("mips".into()),
            os: Some("Bell Labs".into()),
            processor: None,
            hardware_platform: None,
        };
        assert_eq!(
            to_json(&output),
            json!({
                "kernel_name": "Plan9",
                "node_name": "cpu1",
                "machine": "mips",
                "operating_system": "Bell Labs",
            })
        );
    }
}
