use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // Try to find the coreutils binary using cargo metadata
    let output = Command::new("cargo")
        .args(["metadata", "--format-version=1"])
        .output()
        .expect("Failed to execute cargo metadata");

    let output_str = String::from_utf8(output.stdout)
        .expect("Failed to convert output to UTF-8");

    // Parse the JSON output
    let metadata: serde_json::Value = serde_json::from_str(&output_str)
        .expect("Failed to parse cargo metadata JSON");

    // Get the target directory
    let target_directory = metadata["target_directory"].as_str()
        .expect("Failed to parse target directory");

    // Find the coreutils package
    let packages = metadata["packages"].as_array()
        .expect("Failed to parse packages array");
    
    let coreutils_package = packages.iter()
        .find(|package| package["name"].as_str().unwrap_or("") == "coreutils")
        .expect("Could not find coreutils package");

    // Find the binary target
    let targets = coreutils_package["targets"].as_array()
        .expect("Failed to parse targets array");
    
    let binary_target = targets.iter()
        .find(|target| {
            let kinds = target["kind"].as_array().unwrap();
            kinds.iter().any(|kind| kind.as_str().unwrap() == "bin")
        })
        .expect("Could not find bin target in coreutils package");

    let target_name = binary_target["name"].as_str().unwrap();

    // Determine debug or release mode
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    
    // Binary name with extension on Windows
    let binary_name = if cfg!(windows) {
        format!("{}.exe", target_name)
    } else {
        target_name.to_string()
    };
    
    // Try possible locations for the binary
    let mut binary_path = None;
    let possible_paths = [
        // Standard location
        PathBuf::from(target_directory).join(profile).join(&binary_name),
        // Location in workspace
        PathBuf::from(target_directory).join(profile).join(target_name).join(&binary_name),
    ];
    
    for path in &possible_paths {
        if path.exists() {
            binary_path = Some(path.clone());
            break;
        }
    }
    
    // If not found, try to build it
    if binary_path.is_none() {
        println!("cargo:warning=Building coreutils binary...");
        
        Command::new("cargo")
            .args(["build", "--bin", target_name])
            .status()
            .expect("Failed to build coreutils binary");
        
        // Check again after building
        for path in &possible_paths {
            if path.exists() {
                binary_path = Some(path.clone());
                break;
            }
        }
    }
    
    let binary_path = binary_path
        .expect("Could not find or build coreutils binary");
    
    println!("cargo:rustc-env=CARGO_BIN_EXE_coreutils={}", binary_path.display());
}