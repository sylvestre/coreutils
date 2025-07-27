// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (vars) krate

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn main() {
    const ENV_FEATURE_PREFIX: &str = "CARGO_FEATURE_";
    const FEATURE_PREFIX: &str = "feat_";
    const OVERRIDE_PREFIX: &str = "uu_";

    // Do not rebuild build script unless the script itself or the enabled features are modified
    // See <https://doc.rust-lang.org/cargo/reference/build-scripts.html#change-detection>
    println!("cargo:rerun-if-changed=build.rs");

    if let Ok(profile) = env::var("PROFILE") {
        println!("cargo:rustc-cfg=build={profile:?}");
    }

    let out_dir = env::var("OUT_DIR").unwrap();

    let mut crates = Vec::new();
    for (key, val) in env::vars() {
        if val == "1" && key.starts_with(ENV_FEATURE_PREFIX) {
            let krate = key[ENV_FEATURE_PREFIX.len()..].to_lowercase();
            // Allow this as we have a bunch of info in the comments
            #[allow(clippy::match_same_arms)]
            match krate.as_ref() {
                "default" | "macos" | "unix" | "windows" | "selinux" | "zip" => continue, // common/standard feature names
                "nightly" | "test_unimplemented" | "expensive_tests" | "test_risky_names" => {
                    continue;
                } // crate-local custom features
                "uudoc" => continue,         // is not a utility
                "test" => continue, // over-ridden with 'uu_test' to avoid collision with rust core crate 'test'
                "embed_strings" => continue, // is not a utility, just a feature flag
                s if s.starts_with(FEATURE_PREFIX) => continue, // crate feature sets
                _ => {}             // util feature name
            }
            crates.push(krate);
        }
    }
    crates.sort();

    let mut mf = File::create(Path::new(&out_dir).join("uutils_map.rs")).unwrap();

    mf.write_all(
        "type UtilityMap<T> = phf::OrderedMap<&'static str, (fn(T) -> i32, fn() -> Command)>;\n\
         \n\
         #[allow(clippy::too_many_lines)]
         #[allow(clippy::unreadable_literal)]
         fn util_map<T: uucore::Args>() -> UtilityMap<T> {\n"
            .as_bytes(),
    )
    .unwrap();

    let mut phf_map = phf_codegen::OrderedMap::<&str>::new();
    for krate in &crates {
        let map_value = format!("({krate}::uumain, {krate}::uu_app)");
        match krate.as_ref() {
            // 'test' is named uu_test to avoid collision with rust core crate 'test'.
            // It can also be invoked by name '[' for the '[ expr ] syntax'.
            "uu_test" => {
                phf_map.entry("test", map_value.clone());
                phf_map.entry("[", map_value.clone());
            }
            k if k.starts_with(OVERRIDE_PREFIX) => {
                phf_map.entry(&k[OVERRIDE_PREFIX.len()..], map_value.clone());
            }
            "false" | "true" => {
                phf_map.entry(krate, format!("(r#{krate}::uumain, r#{krate}::uu_app)"));
            }
            "hashsum" => {
                phf_map.entry(krate, format!("({krate}::uumain, {krate}::uu_app_custom)"));

                let map_value = format!("({krate}::uumain, {krate}::uu_app_common)");
                let map_value_bits = format!("({krate}::uumain, {krate}::uu_app_bits)");
                let map_value_b3sum = format!("({krate}::uumain, {krate}::uu_app_b3sum)");
                phf_map.entry("md5sum", map_value.clone());
                phf_map.entry("sha1sum", map_value.clone());
                phf_map.entry("sha224sum", map_value.clone());
                phf_map.entry("sha256sum", map_value.clone());
                phf_map.entry("sha384sum", map_value.clone());
                phf_map.entry("sha512sum", map_value.clone());
                phf_map.entry("sha3sum", map_value_bits.clone());
                phf_map.entry("sha3-224sum", map_value.clone());
                phf_map.entry("sha3-256sum", map_value.clone());
                phf_map.entry("sha3-384sum", map_value.clone());
                phf_map.entry("sha3-512sum", map_value.clone());
                phf_map.entry("shake128sum", map_value_bits.clone());
                phf_map.entry("shake256sum", map_value_bits.clone());
                phf_map.entry("b2sum", map_value.clone());
                phf_map.entry("b3sum", map_value_b3sum);
            }
            _ => {
                phf_map.entry(krate, map_value.clone());
            }
        }
    }
    write!(mf, "{}", phf_map.build()).unwrap();
    mf.write_all(b"\n}\n").unwrap();

    mf.flush().unwrap();

    // Generate embedded locale strings if embed_strings feature is enabled
    if env::var("CARGO_FEATURE_EMBED_STRINGS").is_ok() {
        generate_embedded_locale_strings(&out_dir, &crates).unwrap();
    }
}

/// Generate embedded locale strings from .ftl files
///
/// # Errors
///
/// Returns an error if file operations fail or if there are I/O issues
fn generate_embedded_locale_strings(
    out_dir: &str,
    crates: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashMap;
    use std::fs;

    let mut all_strings = HashMap::new();

    // Collect all English strings from all utilities
    for krate in crates {
        let locale_path = format!("src/uu/{krate}/locales/en-US.ftl");
        if Path::new(&locale_path).exists() {
            let content = fs::read_to_string(&locale_path)?;

            // Parse the Fluent file and extract key-value pairs
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                if let Some(equals_pos) = line.find(" = ") {
                    let key = line[..equals_pos].trim();
                    let value = line[equals_pos + 3..].trim();
                    all_strings.insert(key.to_string(), value.to_string());
                }
            }
        }
    }

    // Generate Rust code with embedded strings as individual consts
    let mut embedded_file = File::create(Path::new(out_dir).join("embedded_locale.rs"))?;

    writeln!(embedded_file, "// Generated at compile time - do not edit")?;
    writeln!(embedded_file)?;

    // Generate individual const strings
    for (key, value) in &all_strings {
        // Create a valid Rust identifier from the key
        let const_name = key
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .to_uppercase();

        // Ensure it doesn't start with a number
        let const_name = if const_name.chars().next().unwrap_or('_').is_numeric() {
            format!("S_{const_name}")
        } else {
            const_name
        };

        // Escape the value for Rust string literal
        let escaped_value = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        writeln!(
            embedded_file,
            "pub const {const_name}: &str = {escaped_value:?};"
        )?;
    }

    writeln!(embedded_file)?;

    // Generate lookup function
    writeln!(
        embedded_file,
        "pub fn get_embedded_string(id: &str) -> Option<&'static str> {{"
    )?;
    writeln!(embedded_file, "    match id {{")?;

    for key in all_strings.keys() {
        let const_name = key
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .to_uppercase();

        // Ensure it doesn't start with a number
        let const_name = if const_name.chars().next().unwrap_or('_').is_numeric() {
            format!("S_{const_name}")
        } else {
            const_name
        };

        writeln!(embedded_file, "        {key:?} => Some({const_name}),")?;
    }

    writeln!(embedded_file, "        _ => None,")?;
    writeln!(embedded_file, "    }}")?;
    writeln!(embedded_file, "}}")?;

    embedded_file.flush()?;
    Ok(())
}
