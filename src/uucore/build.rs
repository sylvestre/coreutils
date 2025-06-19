// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var("OUT_DIR").unwrap();

    // Generate embedded locale strings if no_i18n feature is enabled
    if env::var("CARGO_FEATURE_NO_I18N").is_ok() {
        generate_embedded_locale_strings(&out_dir).unwrap();
    }
}

fn generate_embedded_locale_strings(out_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    use fluent_syntax::ast::{Entry, Pattern, PatternElement};
    use fluent_syntax::parser::parse;
    use std::collections::HashMap;
    use std::fs;

    let mut all_strings = HashMap::new();

    // Find all locale directories in the parent uu directory
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let uu_dir = Path::new(manifest_dir).join("../uu");

    if uu_dir.exists() {
        // Iterate through all utility directories
        for entry in fs::read_dir(&uu_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let locale_path = entry.path().join("locales/en-US.ftl");
                if locale_path.exists() {
                    let content = fs::read_to_string(&locale_path)?;

                    // Parse the Fluent file using the fluent-syntax crate
                    let resource = parse(content)
                        .map_err(|e| format!("Failed to parse {:?}: {:?}", locale_path, e))?;

                    for entry in resource.body {
                        if let Entry::Message(message) = entry {
                            if let Some(Pattern { ref elements }) = message.value {
                                // Extract text from the pattern elements
                                let mut text = String::new();
                                for element in elements {
                                    if let PatternElement::TextElement { value } = element {
                                        text.push_str(value);
                                    }
                                }
                                all_strings.insert(message.id.name.to_string(), text);
                            }
                        }
                    }
                }
            }
        }
    }

    // Generate Rust code with embedded strings as individual consts
    let mut embedded_file = File::create(Path::new(out_dir).join("embedded_locale.rs"))?;

    writeln!(embedded_file, "// Generated at compile time - do not edit")?;
    writeln!(embedded_file)?;

    // No individual const strings needed - they're embedded in the function

    writeln!(embedded_file)?;

    // Generate lookup function
    writeln!(
        embedded_file,
        "pub fn get_embedded_string(id: &str) -> Option<&'static str> {{"
    )?;
    writeln!(embedded_file, "    match id {{")?;

    for (key, value) in &all_strings {
        // Escape the value for Rust string literal
        let escaped_value = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        writeln!(
            embedded_file,
            "        {:?} => Some({:?}),",
            key, escaped_value
        )?;
    }

    writeln!(embedded_file, "        _ => None,")?;
    writeln!(embedded_file, "    }}")?;
    writeln!(embedded_file, "}}")?;

    embedded_file.flush()?;
    Ok(())
}
