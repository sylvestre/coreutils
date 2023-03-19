#![no_main]
use libfuzzer_sys::fuzz_target;
use uu_test::uumain;

use rand::seq::SliceRandom;
use rand::Rng;
use std::ffi::OsString;

use libc::{dup, dup2, STDOUT_FILENO};
use std::fs::File;
use std::io::Read;
use std::io::{self, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::io::FromRawFd;
use std::process::Command;
use std::str;

fn run_gnu_test(args: &[OsString]) -> Result<(String, i32), std::io::Error> {
    let mut command = Command::new("test");
    for arg in args {
        command.arg(arg);
    }
    let output = command.output()?;
    let exit_status = output.status.code().unwrap_or(-1); // Capture the exit status code
    Ok((
        String::from_utf8_lossy(&output.stdout).to_string(),
        exit_status,
    ))
}

fn generate_random_string(max_length: usize) -> String {
    let mut rng = rand::thread_rng();
    let valid_utf8: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
        .chars()
        .collect();
    let invalid_utf8 = [0xC3, 0x28]; // Invalid UTF-8 sequence
    let mut result = String::new();

    for _ in 0..rng.gen_range(1..=max_length) {
        if rng.gen_bool(0.9) {
            let ch = valid_utf8.choose(&mut rng).unwrap();
            result.push(*ch);
        } else {
            let ch = invalid_utf8.choose(&mut rng).unwrap();
            if let Some(c) = char::from_u32(*ch as u32) {
                result.push(c);
            }
        }
    }

    result
}

fn generate_test_arg() -> String {
    let mut rng = rand::thread_rng();
    let mut arg = String::new();

    let choice = rng.gen_range(0..=2);

    match choice {
        0 => {
            arg.push_str(&rng.gen_range(1..=100).to_string());
        }
        1 => {
            arg.push_str(&format!(
                "-{}{}",
                *"-neqfLdltzsex"
                    .chars()
                    .collect::<Vec<char>>()
                    .choose(&mut rng)
                    .unwrap(),
                rng.gen_range(1..=100).to_string()
            ));
        }
        _ => {
            let random_str = generate_random_string(rng.gen_range(1..=10));
            arg.push_str(&random_str);
        }
    }

    arg
}

fuzz_target!(|data: &[u8]| {
    let mut rng = rand::thread_rng();
    let max_args = rng.gen_range(1..=6);
    let mut args = vec![OsString::from("test")];
    let mut uumain_exit_status = -1;

    for _ in 0..max_args {
        args.push(OsString::from(generate_test_arg()));
    }

    // Save the original stdout file descriptor
    let original_stdout_fd = unsafe { dup(STDOUT_FILENO) };
    println!("Running test {:?}", &args[1..]);
    // Create a pipe to capture stdout
    let mut pipe_fds = [-1; 2];
    unsafe { libc::pipe(pipe_fds.as_mut_ptr()) };

    {
        // Redirect stdout to the write end of the pipe
        unsafe { dup2(pipe_fds[1], STDOUT_FILENO) };

        // Run uumain with the provided arguments
        uumain_exit_status = uumain(args.clone().into_iter());

        // Restore original stdout
        unsafe { dup2(original_stdout_fd, STDOUT_FILENO) };
        unsafe { libc::close(original_stdout_fd) };
    }
    // Close the write end of the pipe
    unsafe { libc::close(pipe_fds[1]) };

    // Read captured output from the read end of the pipe
    let mut captured_output = Vec::new();
    let mut read_buffer = [0; 1024];
    loop {
        let bytes_read = unsafe {
            libc::read(
                pipe_fds[0],
                read_buffer.as_mut_ptr() as *mut libc::c_void,
                read_buffer.len(),
            )
        };
        if bytes_read <= 0 {
            break;
        }
        captured_output.extend_from_slice(&read_buffer[..bytes_read as usize]);
    }

    // Close the read end of the pipe
    unsafe { libc::close(pipe_fds[0]) };

    // Convert captured output to a string
    let my_output = String::from_utf8_lossy(&captured_output)
        .to_string()
        .trim()
        .to_owned();

    // Run GNU test with the provided arguments and compare the output
    match run_gnu_test(&args[1..]) {
        Ok((gnu_output, gnu_exit_status)) => {
            let gnu_output = gnu_output.trim().to_owned();
            println!("gnu_exit_status {}", gnu_exit_status);
            println!("uumain_exit_status {}", uumain_exit_status);
            if my_output != gnu_output || uumain_exit_status != gnu_exit_status {
                println!("Discrepancy detected!");
                println!("Test: {:?}", &args[1..]);
                println!("My output: {}", my_output);
                println!("GNU output: {}", gnu_output);
                println!("My exit status: {}", uumain_exit_status);
                println!("GNU exit status: {}", gnu_exit_status);
            } else {
                println!(
                    "Outputs and exit statuses matched for expression {:?}",
                    &args[1..]
                );
            }
        }
        Err(_) => {
            println!("GNU test execution failed for expression {:?}", &args[1..]);
        }
    }
});
