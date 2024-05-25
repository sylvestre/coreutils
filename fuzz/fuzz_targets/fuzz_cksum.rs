// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

#![no_main]
use libfuzzer_sys::fuzz_target;
use rand::prelude::SliceRandom;
use rand::Rng;
use std::ffi::OsString;
use uu_cksum::uumain;

mod fuzz_common;
use crate::fuzz_common::{
    compare_result, generate_and_run_uumain, generate_random_string, run_gnu_cmd, CommandResult,
};

static CMD_PATH: &str = "cksum";

fn generate_cksum_args() -> Vec<String> {
    let mut rng = rand::thread_rng();
    let arg_count = rng.gen_range(1..=4); // Adjust based on how many arguments you want to generate
    let mut args = Vec::new();

    let opts = [
        "-a",
        "--algorithm",
        "--base64",
        "-c",
        "--check",
        "-l",
        "--length",
        "--raw",
        "--tag",
        "--untagged",
        "-z",
        "--zero",
        "--ignore-missing",
        "--quiet",
        "--status",
        "--strict",
        "-w",
        "--warn",
        "--debug",
    ];

    for _ in 0..arg_count {
        match rng.gen_range(0..=17) {
            0 => args.push(String::from("-a")),
            1 => {
                args.push(String::from("--algorithm"));
                args.push(generate_random_algorithm(&mut rng));
            }
            2 => args.push(String::from("--base64")),
            3 => args.push(String::from("-c")),
            4 => args.push(String::from("--check")),
            5 => args.push(String::from("-l")),
            6 => {
                args.push(String::from("--length"));
                args.push(rng.gen_range(8..=512).to_string()); // Example length, adjust as necessary
            }
            7 => args.push(String::from("--raw")),
            8 => args.push(String::from("--tag")),
            9 => args.push(String::from("--untagged")),
            10 => args.push(String::from("-z")),
            11 => args.push(String::from("--zero")),
            12 => args.push(String::from("--ignore-missing")),
            13 => args.push(String::from("--quiet")),
            14 => args.push(String::from("--status")),
            15 => args.push(String::from("--strict")),
            16 => args.push(String::from("-w")),
            17 => args.push(String::from("--warn")),
            18 => args.push(String::from("--debug")),
            _ => (),
        }
    }

    // Adding a few random FILE arguments
    if rng.gen_bool(0.5) {
        args.push(generate_random_string(rng.gen_range(1..=10))); // Simulate file names
    }

    args
}

fn generate_random_algorithm(rng: &mut impl Rng) -> String {
    let algorithms = [
        "sysv", "bsd", "crc", "md5", "sha1", "sha224", "sha256", "sha384", "sha512", "blake2b",
        "sm3",
    ];
    algorithms.choose(rng).unwrap().to_string()
}

fn generate_random_input() -> String {
    generate_random_string(100) // Example input length, adjust as necessary
}

fuzz_target!(|_data: &[u8]| {
    let cksum_args = generate_cksum_args();
    let mut args = vec![OsString::from("cksum")];
    args.extend(cksum_args.iter().map(OsString::from));

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
        "cksum",
        &format!("{:?}", &args[1..]),
        Some(&input_data),
        &rust_result,
        &gnu_result,
        false,
    );
});
