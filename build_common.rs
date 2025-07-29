// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:ignore ümläuts

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// Generate embedded locale strings from .ftl files
///
/// # Arguments
/// * `out_dir` - The output directory for generated files
/// * `project_root` - The root directory of the project
///
/// # Errors
///
/// Returns an error if file operations fail or if there are I/O issues
pub fn generate_embedded_locale_strings(
    out_dir: &str,
    project_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut all_strings = HashMap::new();

    // Collect all English strings from all utilities
    let uu_dir = project_root.join("src/uu");
    if uu_dir.exists() {
        for entry in fs::read_dir(&uu_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let locale_path = path.join("locales/en-US.ftl");
                if locale_path.exists() {
                    let content = fs::read_to_string(&locale_path)?;
                    parse_fluent_content(&content, &mut all_strings);
                }
            }
        }
    }

    // Also collect common locale strings from uucore if they exist
    let common_locale_path = project_root.join("src/uucore/locales/en-US.ftl");
    if common_locale_path.exists() {
        let content = fs::read_to_string(&common_locale_path)?;
        parse_fluent_content(&content, &mut all_strings);
    }

    // Generate Rust code with embedded strings
    generate_embedded_rust_code(out_dir, &all_strings)?;
    Ok(())
}

/// Parse Fluent file content and extract key-value pairs
fn parse_fluent_content(content: &str, all_strings: &mut HashMap<String, String>) {
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        // Check if this line starts a key-value pair
        if let Some(equals_pos) = line.find(" = ") {
            let key = line[..equals_pos].trim();
            let mut value = line[equals_pos + 3..].trim().to_string();

            // Look ahead for continuation lines (lines that start with whitespace or closing brace)
            i += 1;
            while i < lines.len() {
                let next_line = lines[i];
                let trimmed = next_line.trim();

                // Check if this line starts a new key-value pair
                let is_new_key = !trimmed.is_empty()
                    && !next_line.starts_with(' ')
                    && !next_line.starts_with('#')
                    && trimmed.contains(" = ");

                // Stop if we hit a new key
                if is_new_key {
                    break;
                }

                // For blank lines, we need to look ahead to see if there are more continuation lines
                if trimmed.is_empty() {
                    let mut lookahead = i + 1;
                    let mut has_more_continuation = false;

                    // Look ahead to see if there are more indented lines after this blank line
                    while lookahead < lines.len() {
                        let lookahead_line = lines[lookahead];
                        let lookahead_trimmed = lookahead_line.trim();

                        if lookahead_trimmed.is_empty() || lookahead_trimmed.starts_with('#') {
                            lookahead += 1;
                            continue;
                        }

                        if lookahead_line.starts_with(' ') && !lookahead_trimmed.is_empty() {
                            has_more_continuation = true;
                        }
                        break;
                    }

                    if has_more_continuation {
                        // This blank line is part of a multi-paragraph message
                        if !value.is_empty() {
                            value.push('\n');
                        }
                        value.push('\n'); // The blank line itself
                        i += 1;
                        continue;
                    }
                    // This blank line ends the message
                    break;
                }

                // Continuation line conditions:
                // 1. Starts with whitespace and is not empty
                // 2. Is just a closing brace (for select expressions)
                // 3. Is a comment line (should be skipped but continue parsing)
                let is_continuation = (next_line.starts_with(' ') && !trimmed.is_empty())
                    || trimmed == "}"
                    || trimmed.starts_with('#');

                if is_continuation {
                    // Skip comment lines completely
                    if trimmed.starts_with('#') {
                        i += 1;
                        continue;
                    }

                    if trimmed == "}" {
                        // Closing brace for select expressions
                        if !value.is_empty() {
                            value.push('\n');
                        }
                        value.push('}');
                    } else {
                        // Regular continuation line
                        if !value.is_empty() {
                            value.push('\n');
                        }

                        let line_content = next_line.trim();
                        if line_content.starts_with("- ") {
                            // Preserve 2-space indentation for list items
                            value.push_str("  ");
                            value.push_str(line_content);
                        } else {
                            // For other continuation lines, just use trimmed content
                            value.push_str(line_content);
                        }
                    }
                    i += 1;
                } else {
                    break;
                }
            }

            // Process simple Fluent literal templates (like {"]"} -> ])
            // and plural/select expressions
            let processed_value = process_fluent_expressions(&value);
            // Only insert non-empty values
            if !processed_value.is_empty() {
                all_strings.insert(key.to_string(), processed_value);
            }
        } else {
            i += 1;
        }
    }
}

/// Process Fluent expressions for `disable_i18n` mode
/// Handles both simple literals and plural/select expressions
/// For example: {"]"} -> ], {"["} -> [, { $count -> [one] X *[other] Y } -> X, etc.
fn process_fluent_expressions(input: &str) -> String {
    // First handle plural/select expressions, then simple literals
    let after_selects = process_fluent_selects(input);
    process_fluent_literals(&after_selects)
}

/// Process simple Fluent literal templates for `disable_i18n` mode
/// For example: {"]"} -> ], {"["} -> [, etc.
fn process_fluent_literals(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Check if this looks like a simple literal template: {"X"} where X is a single char
            let mut template_content = String::new();
            let mut found_closing = false;

            // Expect a quote
            if chars.peek() == Some(&'"') {
                chars.next(); // consume the opening quote

                // Collect characters until closing quote
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == '"' {
                        chars.next(); // consume the closing quote
                        if chars.peek() == Some(&'}') {
                            chars.next(); // consume the closing brace
                            found_closing = true;
                        }
                        break;
                    }
                    template_content.push(chars.next().unwrap());
                }
            }

            if found_closing && !template_content.is_empty() {
                // This was a valid literal template, use the content
                // Process escape sequences within the template content
                let processed_content = process_escape_sequences(&template_content);
                result.push_str(&processed_content);
            } else {
                // Not a simple literal template, keep the original character
                result.push(ch);
                // Put back any characters we consumed but didn't use
                result.push_str(&template_content);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Process Fluent select expressions (plurals) for `disable_i18n` mode
/// For example: { $count -> [one] 1 record *[other] records } -> records (selects [other] variant)
///
/// LIMITATION: In `disable_i18n` mode, pluralization is handled at compile time by selecting
/// one variant. This means messages like "1 record" vs "2 records" won't have proper grammar
/// for all cases. We prefer the [other] variant when available for better grammar at count > 1,
/// but tests expecting runtime pluralization will not pass perfectly.
fn process_fluent_selects(input: &str) -> String {
    // Simple approach: look for select patterns and extract the [other] variant
    if let Some(start) = input.find("{ $") {
        if let Some(arrow_pos) = input[start..].find(" ->") {
            let arrow_abs = start + arrow_pos;

            // Find the matching closing brace by counting braces
            let mut depth = 1;
            let mut pos = arrow_abs + 3; // after " ->"
            let chars: Vec<char> = input.chars().collect();

            while pos < chars.len() && depth > 0 {
                if let Some(&ch) = chars.get(pos) {
                    match ch {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                }
                pos += 1;
            }

            if depth == 0 {
                let end_abs = pos - 1; // position of closing brace
                let select_content = &input[arrow_abs + 3..end_abs]; // after " ->"

                // Extract the preferred variant (prefer [other] for better grammar at count > 1)
                // This is a compile-time decision that affects all runtime uses
                if let Some(other_variant) = extract_variant(select_content, "other") {
                    let mut result = String::new();
                    result.push_str(&input[..start]);
                    result.push_str(&other_variant);
                    result.push_str(&input[pos..]);
                    return result;
                } else if let Some(one_variant) = extract_variant(select_content, "one") {
                    let mut result = String::new();
                    result.push_str(&input[..start]);
                    result.push_str(&one_variant);
                    result.push_str(&input[pos..]);
                    return result;
                }
            }
        }
    }

    input.to_string()
}

/// Extract a specific variant from Fluent select content
/// For example: `extract_variant`("[one] { $count } record *[other] { $count } records", "one")
/// returns Some("{ $count } record")
fn extract_variant(content: &str, target_key: &str) -> Option<String> {
    // Look for both regular [key] and default *[key] patterns
    let patterns = [
        format!("*[{target_key}]"), // default variant like *[other]
        format!("[{target_key}]"),  // regular variant like [one]
    ];

    for pattern in &patterns {
        if let Some(start) = content.find(pattern) {
            let after_key = start + pattern.len();
            let remaining = &content[after_key..];

            // Find the end of this variant (next variant marker like "*[" or "[" or end of string)
            let end = remaining
                .find("\n*[")
                .or_else(|| remaining.find(" *["))
                .or_else(|| remaining.find("\n["))
                .or_else(|| remaining.find(" ["))
                .unwrap_or(remaining.len());

            let variant_content = remaining[..end].trim();

            if !variant_content.is_empty() {
                return Some(variant_content.to_string());
            }
        }
    }

    None
}

/// Process escape sequences in Fluent template content
/// For example: \\{ -> \{, \\} -> \}, \\\\ -> \\, etc.
fn process_escape_sequences(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // Check what follows the backslash
            if let Some(&next_ch) = chars.peek() {
                match next_ch {
                    '\\' => {
                        // \\\ -> \
                        chars.next(); // consume the second backslash
                        result.push('\\');
                    }
                    '{' | '}' => {
                        // \{ -> {, \} -> }
                        chars.next(); // consume the brace
                        result.push('\\');
                        result.push(next_ch);
                    }
                    _ => {
                        // Other escapes, keep both characters
                        result.push(ch);
                    }
                }
            } else {
                // Backslash at end of string
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Generate Rust code with embedded strings as individual consts and lookup function
///
/// # Errors
///
/// Returns an error if file creation or writing operations fail
fn generate_embedded_rust_code(
    out_dir: &str,
    all_strings: &HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut embedded_file = File::create(Path::new(out_dir).join("embedded_locale.rs"))?;

    writeln!(embedded_file, "// Generated at compile time - do not edit")?;
    writeln!(embedded_file)?;

    // Generate individual const strings
    for (key, value) in all_strings {
        let const_name = create_rust_identifier(key);

        // The {:?} format already handles proper escaping for Rust string literals
        writeln!(embedded_file, "pub const {const_name}: &str = {value:?};")?;
    }

    writeln!(embedded_file)?;

    // Generate lookup function
    writeln!(
        embedded_file,
        "pub fn get_embedded_string(id: &str) -> Option<&'static str> {{"
    )?;
    writeln!(embedded_file, "match id {{")?;

    for key in all_strings.keys() {
        let const_name = create_rust_identifier(key);
        writeln!(embedded_file, "        {key:?} => Some({const_name}),")?;
    }

    writeln!(embedded_file, "        _ => None,")?;
    writeln!(embedded_file, "    }}")?;
    writeln!(embedded_file, "}}")?;

    embedded_file.flush()?;
    Ok(())
}

/// Create a valid Rust identifier from a Fluent key
fn create_rust_identifier(key: &str) -> String {
    let const_name = key
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .to_uppercase();

    // Ensure it doesn't start with a number
    if const_name.chars().next().unwrap_or('_').is_numeric() {
        format!("S_{const_name}")
    } else {
        const_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_rust_identifier() {
        assert_eq!(create_rust_identifier("simple-key"), "SIMPLE_KEY");
        assert_eq!(create_rust_identifier("key.with.dots"), "KEY_WITH_DOTS");
        assert_eq!(
            create_rust_identifier("key_with_underscores"),
            "KEY_WITH_UNDERSCORES"
        );
        assert_eq!(
            create_rust_identifier("123-starts-with-number"),
            "S_123_STARTS_WITH_NUMBER"
        );
        assert_eq!(
            create_rust_identifier("key-with-ümläuts"),
            "KEY_WITH_ÜMLÄUTS"
        );
        assert_eq!(create_rust_identifier(""), "");
    }

    #[test]
    fn test_process_escape_sequences() {
        assert_eq!(process_escape_sequences("simple text"), "simple text");
        assert_eq!(process_escape_sequences(r"\\"), "\\");
        assert_eq!(process_escape_sequences(r"\{"), "\\{");
        assert_eq!(process_escape_sequences(r"\}"), "\\}");
        assert_eq!(process_escape_sequences(r"\{hello\}"), "\\{hello\\}");
        assert_eq!(process_escape_sequences(r"\"), "\\");
        assert_eq!(process_escape_sequences(r"\n"), "\\n");
        assert_eq!(
            process_escape_sequences(r"multiple\\escapes\{here\}"),
            "multiple\\escapes\\{here\\}"
        );
    }

    #[test]
    fn test_process_fluent_literals() {
        assert_eq!(process_fluent_literals("simple text"), "simple text");
        assert_eq!(process_fluent_literals("{\"]\"}"), "]");
        assert_eq!(process_fluent_literals("{\"[\"}"), "[");
        assert_eq!(
            process_fluent_literals("prefix {\"]\"} suffix"),
            "prefix ] suffix"
        );
        assert_eq!(process_fluent_literals(r#"{"\\"}""#), "\\\"");
        assert_eq!(process_fluent_literals(r#"{"\{"}""#), "\\{\"");
        assert_eq!(process_fluent_literals(r#"{"\}"}""#), "\\}\"");
        assert_eq!(process_fluent_literals("{\"}"), "{}");
        assert_eq!(process_fluent_literals("{\""), "{");
        assert_eq!(process_fluent_literals("{}"), "{}");
        assert_eq!(process_fluent_literals("{no quotes}"), "{no quotes}");
        assert_eq!(
            process_fluent_literals("multiple {\"[\"} and {\"]\"} literals"),
            "multiple [ and ] literals"
        );
    }

    #[test]
    fn test_extract_variant() {
        let content = "[one] 1 record *[other] records";
        assert_eq!(
            extract_variant(content, "one"),
            Some("1 record".to_string())
        );
        assert_eq!(
            extract_variant(content, "other"),
            Some("records".to_string())
        );
        assert_eq!(extract_variant(content, "two"), None);

        let content_with_variables = "[one] { $count } record *[other] { $count } records";
        assert_eq!(
            extract_variant(content_with_variables, "one"),
            Some("{ $count } record".to_string())
        );
        assert_eq!(
            extract_variant(content_with_variables, "other"),
            Some("{ $count } records".to_string())
        );

        let multiline = "[one] single\n*[other] multiple\nitems";
        assert_eq!(
            extract_variant(multiline, "one"),
            Some("single".to_string())
        );
        assert_eq!(
            extract_variant(multiline, "other"),
            Some("multiple\nitems".to_string())
        );

        let only_default = "*[other] default value";
        assert_eq!(
            extract_variant(only_default, "other"),
            Some("default value".to_string())
        );
        assert_eq!(extract_variant(only_default, "one"), None);
    }

    #[test]
    fn test_process_fluent_selects() {
        let input = "{ $count -> [one] 1 record *[other] records }";
        assert_eq!(process_fluent_selects(input), "records");

        let input = "{ $count -> [one] { $count } record *[other] { $count } records }";
        assert_eq!(process_fluent_selects(input), "{ $count } records");

        let input = "{ $count -> [one] single item }";
        assert_eq!(process_fluent_selects(input), "single item");

        let input = "You have { $count -> [one] 1 item *[other] { $count } items } in your cart";
        assert_eq!(
            process_fluent_selects(input),
            "You have { $count } items in your cart"
        );

        let input = "{ $type -> [file] { open } *[other] { closed } }";
        assert_eq!(process_fluent_selects(input), "{ closed }");

        let input = "Just plain text with { $variable }";
        assert_eq!(
            process_fluent_selects(input),
            "Just plain text with { $variable }"
        );

        let input = "{ $count ->\n    [one] single\n   *[other] multiple\n}";
        assert_eq!(process_fluent_selects(input), "multiple");

        let input = "{ $count -> [zero] none [one] single [two] pair *[other] many }";
        assert_eq!(process_fluent_selects(input), "many");
    }

    #[test]
    fn test_process_fluent_expressions() {
        let input = "Array {\"[\"} { $count -> [one] item *[other] items } {\"]\"} found";
        assert_eq!(process_fluent_expressions(input), "Array [ items ] found");

        let input = "Special chars: {\"[\"} and {\"]\"} and {\"}\"}";
        assert_eq!(
            process_fluent_expressions(input),
            "Special chars: [ and ] and }"
        );

        let input = "{ $num -> [one] single *[other] multiple }";
        assert_eq!(process_fluent_expressions(input), "multiple");

        let input = "Plain text with { $var } variable";
        assert_eq!(
            process_fluent_expressions(input),
            "Plain text with { $var } variable"
        );
    }

    #[test]
    fn test_parse_fluent_content() {
        let mut strings = HashMap::new();

        let content = "hello = Hello, World!";
        parse_fluent_content(content, &mut strings);
        assert_eq!(strings.get("hello"), Some(&"Hello, World!".to_string()));

        strings.clear();
        let content = "multi-line = First line\n    Second line\n    Third line";
        parse_fluent_content(content, &mut strings);
        assert_eq!(
            strings.get("multi-line"),
            Some(&"First line\nSecond line\nThird line".to_string())
        );

        strings.clear();
        let content = "# This is a comment\nkey = value\n# Another comment";
        parse_fluent_content(content, &mut strings);
        assert_eq!(strings.get("key"), Some(&"value".to_string()));
        assert_eq!(strings.len(), 1);

        strings.clear();
        let content = "plural = { $count ->\n    [one] single\n   *[other] multiple\n}";
        parse_fluent_content(content, &mut strings);
        assert_eq!(strings.get("plural"), Some(&"multiple".to_string()));

        strings.clear();
        let content = "first = First value\nsecond = Second value\nthird = Third value";
        parse_fluent_content(content, &mut strings);
        assert_eq!(strings.len(), 3);
        assert_eq!(strings.get("first"), Some(&"First value".to_string()));
        assert_eq!(strings.get("second"), Some(&"Second value".to_string()));
        assert_eq!(strings.get("third"), Some(&"Third value".to_string()));

        strings.clear();
        let content = "list = List of items:\n  - First item\n  - Second item\n  - Third item";
        parse_fluent_content(content, &mut strings);
        assert_eq!(
            strings.get("list"),
            Some(&"List of items:\n  - First item\n  - Second item\n  - Third item".to_string())
        );

        strings.clear();
        let content =
            "paragraphs = First paragraph.\n\n    Second paragraph.\n\n    Third paragraph.";
        parse_fluent_content(content, &mut strings);
        assert_eq!(
            strings.get("paragraphs"),
            Some(&"First paragraph.\n\n\nSecond paragraph.\n\n\nThird paragraph.".to_string())
        );

        strings.clear();
        let content = r#"complex = Found {"]"} { $count -> [one] item *[other] items } {"["}"#;
        parse_fluent_content(content, &mut strings);
        assert_eq!(strings.get("complex"), Some(&"Found ] items [".to_string()));

        strings.clear();
        let content = "first = First\n\nsecond = Second";
        parse_fluent_content(content, &mut strings);
        assert_eq!(strings.len(), 2);
        assert_eq!(strings.get("first"), Some(&"First".to_string()));
        assert_eq!(strings.get("second"), Some(&"Second".to_string()));

        strings.clear();
        let content = "with-comment = Start\n    # This comment should be skipped\n    End";
        parse_fluent_content(content, &mut strings);
        assert_eq!(strings.get("with-comment"), Some(&"Start\nEnd".to_string()));
    }

    #[test]
    fn test_parse_fluent_content_edge_cases() {
        let mut strings = HashMap::new();

        parse_fluent_content("", &mut strings);
        assert_eq!(strings.len(), 0);

        parse_fluent_content("# Comment 1\n# Comment 2", &mut strings);
        assert_eq!(strings.len(), 0);

        parse_fluent_content("key-without-value", &mut strings);
        assert_eq!(strings.len(), 0);

        strings.clear();
        parse_fluent_content("empty = ", &mut strings);
        assert_eq!(strings.get("empty"), None);

        strings.clear();
        parse_fluent_content("equation = a = b + c", &mut strings);
        assert_eq!(strings.get("equation"), Some(&"a = b + c".to_string()));

        strings.clear();
        parse_fluent_content("trailing = value   \n", &mut strings);
        assert_eq!(strings.get("trailing"), Some(&"value".to_string()));
    }

    #[test]
    fn test_generate_embedded_rust_code() {
        use std::io::Read;

        let temp_path = std::env::temp_dir().join(format!("uutils_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_path).unwrap();

        let mut strings = HashMap::new();
        strings.insert("simple-key".to_string(), "Simple value".to_string());
        strings.insert(
            "complex-key".to_string(),
            r#"Value with "quotes" and \backslashes\"#.to_string(),
        );
        strings.insert("123-numeric".to_string(), "Numeric start".to_string());

        generate_embedded_rust_code(temp_path.to_str().unwrap(), &strings).unwrap();

        let mut content = String::new();
        let mut file = File::open(temp_path.join("embedded_locale.rs")).unwrap();
        file.read_to_string(&mut content).unwrap();

        assert!(content.contains("pub const SIMPLE_KEY: &str = \"Simple value\";"));
        assert!(content.contains(
            "pub const COMPLEX_KEY: &str = \"Value with \\\"quotes\\\" and \\\\backslashes\\\\\";"
        ));
        assert!(content.contains("pub const S_123_NUMERIC: &str = \"Numeric start\";"));
        assert!(content.contains("pub fn get_embedded_string(id: &str) -> Option<&'static str>"));
        assert!(content.contains("\"simple-key\" => Some(SIMPLE_KEY),"));
        assert!(content.contains("\"complex-key\" => Some(COMPLEX_KEY),"));
        assert!(content.contains("\"123-numeric\" => Some(S_123_NUMERIC),"));

        std::fs::remove_dir_all(&temp_path).ok();
    }

    #[test]
    fn test_generate_embedded_locale_strings_basic() {
        let temp_path =
            std::env::temp_dir().join(format!("uutils_locale_test_{}", std::process::id()));
        let project_root = &temp_path;

        let uu_dir = project_root.join("src/uu/test-util/locales");
        std::fs::create_dir_all(&uu_dir).unwrap();

        let fluent_content = "greeting = Hello World!\nfarewell = Goodbye!";
        std::fs::write(uu_dir.join("en-US.ftl"), fluent_content).unwrap();

        let out_dir = temp_path.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();

        let result = generate_embedded_locale_strings(out_dir.to_str().unwrap(), project_root);

        assert!(result.is_ok(), "Should succeed with basic setup");

        let output_file = out_dir.join("embedded_locale.rs");
        assert!(output_file.exists(), "Output file should be created");

        let content = std::fs::read_to_string(&output_file).unwrap();
        assert!(content.contains("GREETING"));
        assert!(content.contains("FAREWELL"));
        assert!(content.contains("Hello World!"));
        assert!(content.contains("Goodbye!"));

        std::fs::remove_dir_all(&temp_path).ok();
    }
}
