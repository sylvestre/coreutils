use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Try to find the binary location directly
    if let Ok(binary_path) = find_binary_manually() {
        println!(
            "cargo:rustc-env=CARGO_BIN_EXE_coreutils={}",
            binary_path.display()
        );
        return;
    }

    // Fallback to using cargo metadata
    let output = Command::new("cargo")
        .args(["metadata", "--format-version=1"])
        .output()
        .expect("Failed to execute cargo metadata");

    let output_str = String::from_utf8(output.stdout).expect("Failed to convert output to UTF-8");

    // Parse the JSON output
    let metadata: serde_json::Value =
        serde_json::from_str(&output_str).expect("Failed to parse cargo metadata JSON");

    // Get the target directory
    let target_directory = metadata["target_directory"]
        .as_str()
        .expect("Failed to parse target directory");

    // Find any binary target in the workspace
    let packages = metadata["packages"]
        .as_array()
        .expect("Failed to parse packages array");

    // Look for a binary in any package in the workspace
    let mut binary_path = None;

    for package in packages {
        let targets = package["targets"]
            .as_array()
            .expect("Failed to parse targets array");

        for target in targets {
            let kinds = target["kind"].as_array().unwrap();
            if kinds.iter().any(|kind| kind.as_str().unwrap() == "bin") {
                let target_name = target["name"].as_str().unwrap();

                // Determine debug or release mode
                let profile = if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                };

                // Binary name with extension on Windows
                let binary_name = if cfg!(windows) {
                    format!("{}.exe", target_name)
                } else {
                    target_name.to_string()
                };

                // Try possible locations
                let possible_paths = [
                    PathBuf::from(target_directory)
                        .join(profile)
                        .join(&binary_name),
                    PathBuf::from(target_directory)
                        .join(profile)
                        .join(target_name)
                        .join(&binary_name),
                ];

                for path in &possible_paths {
                    if path.exists() {
                        binary_path = Some(path.clone());
                        println!("cargo:warning=Found binary at: {}", path.display());
                        break;
                    }
                }

                if binary_path.is_some() {
                    break;
                }

                // Try to build this binary if not found
                println!("cargo:warning=Building binary {}...", target_name);

                let build_result = Command::new("cargo")
                    .args(["build", "--bin", target_name])
                    .status();

                if let Ok(status) = build_result {
                    if status.success() {
                        // Check again after building
                        for path in &possible_paths {
                            if path.exists() {
                                binary_path = Some(path.clone());
                                println!("cargo:warning=Built binary at: {}", path.display());
                                break;
                            }
                        }
                    }
                }

                if binary_path.is_some() {
                    break;
                }
            }
        }

        if binary_path.is_some() {
            break;
        }
    }

    // If binary wasn't found, provide a fallback path
    if binary_path.is_none() {
        // Use a relative path that could potentially be correct
        // This will be a best-effort fallback
        let fallback = PathBuf::from("../target/debug/coreutils");
        println!(
            "cargo:warning=Using fallback binary path: {}",
            fallback.display()
        );
        binary_path = Some(fallback);
    }

    println!(
        "cargo:rustc-env=CARGO_BIN_EXE_coreutils={}",
        binary_path.unwrap().display()
    );
}

fn find_binary_manually() -> Result<PathBuf, &'static str> {
    // Check common locations relative to CARGO_MANIFEST_DIR
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").map_err(|_| "No CARGO_MANIFEST_DIR")?;
    let manifest_path = Path::new(&manifest_dir);

    // Try to find the root of the project
    let mut project_root = manifest_path.to_path_buf();
    while !project_root.join("Cargo.toml").exists() && project_root.parent().is_some() {
        project_root = project_root.parent().unwrap().to_path_buf();
    }

    // Try to locate the binary in common target directories
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    let possible_paths = [
        project_root.join("target").join(profile).join("coreutils"),
        project_root
            .join("target")
            .join(profile)
            .join(if cfg!(windows) {
                "coreutils.exe"
            } else {
                "coreutils"
            }),
    ];

    for path in &possible_paths {
        if path.exists() {
            println!("cargo:warning=Found binary manually at: {}", path.display());
            return Ok(path.clone());
        }
    }

    Err("Binary not found in common locations")
}
