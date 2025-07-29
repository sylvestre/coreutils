// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = env::var("OUT_DIR").unwrap();

    // Generate embedded locale strings if disable_i18n feature is enabled
    if env::var("CARGO_FEATURE_DISABLE_I18N").is_ok() {
        generate_embedded_locale_strings(&out_dir).unwrap();
    }
}

// Include the shared build logic
#[path = "../../build_common.rs"]
mod build_common;

/// Generate embedded locale strings from .ftl files using shared logic
///
/// # Errors
///
/// Returns an error if file operations fail or if there are I/O issues
fn generate_embedded_locale_strings(out_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Since we're in uucore, we need to go up to the project root
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // src/
        .and_then(|p| p.parent()) // project root
        .ok_or("Failed to find project root")?;

    build_common::generate_embedded_locale_strings(out_dir, project_root)
}
