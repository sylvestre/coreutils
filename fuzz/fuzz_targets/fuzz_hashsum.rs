// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

#![no_main]
use libfuzzer_sys::fuzz_target;
use uu_hashsum::uumain;

use rand::Rng;
use std::ffi::OsString;

mod fuzz_common;
use crate::fuzz_common::{
    compare_result, generate_and_run_uumain, generate_random_string, run_gnu_cmd, CommandResult,
};

static CMD_PATH: &str = "sha256sum"; // Example, replace with appropriate command

fn generate_hashsum_args() -> Vec<String> {
    let mut rng = rand::thread_rng();
    let arg_count = rng.gen_range(1..=4);
    let mut args = Vec::new();

    for _ in 0..arg_count {
        match rng.gen_range(0..=3) {
            0 => args.push(String::from("-b")),
            1 => args.push(String::from("-c")),
            2 => args.push(String::from("--tag")),
            3 => args.push(generate_random_string(rng.gen_range(1..=20))), // Random invalid argument
            _ => (),
        }
    }

    args
}

fn generate_random_input() -> String {
    generate_random_string(100) // Example input length, adjust as necessary
}

fuzz_target!(|_data: &[u8]| {
    let hashsum_args = generate_hashsum_args();
    let mut args = vec![OsString::from("hashsum")];
    args.extend(hashsum_args.iter().map(OsString::from));

    let input_data = generate_random_input();

    let rust_result = generate_and_run_uumain(&args, uumain, Some(&input_data));
    let gnu_result = match run_gnu_cmd(CMD_PATH, &args[1..], false, Some(&input_data)) {
        Ok(result) => result,
        Err(error_result) => {
            eprintln!("Failed to run GNU command:");
            eprintln!("Stderr: {}", error_result.stderr);
            eprintln!("Exit Code: {}", error_result.exit_code);
            CommandResult {
                stdout: String::new(),
                stderr: error_result.stderr,
                exit_code: error_result.exit_code,
            }
        }
    };

    compare_result(
        "hashsum",
        &format!("{:?}", &args[1..]),
        Some(&input_data),
        &rust_result,
        &gnu_result,
        false,
    );
});
