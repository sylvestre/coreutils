// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:ignore parens backref multibyte

#![no_main]
use libfuzzer_sys::fuzz_target;
use uu_expr::uumain;

use rand::RngExt;
use rand::prelude::IndexedRandom;
use std::{env, ffi::OsString};

use uufuzz::CommandResult;
use uufuzz::{compare_result, generate_and_run_uumain, generate_random_string, run_gnu_cmd};
static CMD_PATH: &str = "expr";

fn generate_expr(max_depth: u32) -> String {
    let mut rng = rand::rng();
    let ops = [
        "+", "-", "*", "/", "%", "<", ">", "=", "&", "|", "!=", "<=", ">=", ":", "index", "length",
        "substr",
    ];

    let mut expr = String::new();
    let mut depth = 0;
    let mut last_was_operator = false;

    while depth <= max_depth {
        if last_was_operator || depth == 0 {
            expr.push_str(&rng.random_range(1..=100).to_string());
            last_was_operator = false;
        } else {
            if rng.random_bool(0.9) {
                let op = *ops.choose(&mut rng).unwrap();
                expr.push_str(&format!(" {op} "));
                last_was_operator = true;
            } else {
                let random_str = generate_random_string(rng.random_range(1..=10));
                expr.push_str(&random_str);
                last_was_operator = false;
            }
        }
        depth += 1;
    }

    if last_was_operator {
        expr.push_str(&rng.random_range(1..=100).to_string());
    }

    expr
}

/// Generate a BRE pattern targeting the three fancy-regex edge cases:
///  1. Invalid-UTF-8 byte sequences → potential panic on byte-offset mixing
///  2. Multiline strings (embedded \n) → (?s) / dot-matches-newline behaviour
///  3. Bracket expressions with trailing backslash → TrailingBackslash error path
fn generate_regex_match_expr(data: &[u8]) -> Option<Vec<OsString>> {
    if data.len() < 3 {
        return None;
    }

    let mut rng = rand::rng();
    let mode = data[0] % 4;

    match mode {
        // ── 1. Invalid UTF-8 in the subject string ────────────────────────
        // The fancy-regex match path converts `String` byte offsets back to
        // the original bytes.  Feeding non-UTF-8 subjects exercises that path
        // and may expose off-by-one panics.
        0 => {
            // Build a subject that mixes valid UTF-8 with arbitrary raw bytes.
            let mut subject = String::from("abc");
            // Append some valid multibyte chars
            subject.push('é'); // 2-byte UTF-8
            subject.push('中'); // 3-byte UTF-8
            // Now append bytes that may produce invalid sequences when the
            // OsString is round-tripped through the shell.  We use the raw
            // byte slice directly so the fuzzer corpus can inject anything.
            let raw: Vec<u8> = data[1..].iter().take(8).cloned().collect();
            let lossy = String::from_utf8_lossy(&raw).into_owned();
            subject.push_str(&lossy);

            let patterns = [
                ".",        // matches any single char — exercises multibyte walk
                ".*",       // greedy — exercises full-string byte-range return
                ".\\{2\\}", // BRE repeat — exercises counted match offsets
                "[a-z]*",   // bracket — exercises char-class on mixed bytes
            ];
            let pattern = *patterns.choose(&mut rng).unwrap();

            Some(vec![
                OsString::from("expr"),
                OsString::from(subject),
                OsString::from(":"),
                OsString::from(pattern),
            ])
        }

        // ── 2. Multiline subjects ─────────────────────────────────────────
        // GNU expr does NOT treat \n as a line boundary for `.` — it matches
        // any byte including newline.  fancy-regex defaults to (?s) off, so
        // `.*` would stop at \n unless the transpiler adds `(?s)`.
        1 => {
            let lines = [
                "foo\nbar",
                "hello\nworld\n",
                "\n",
                "a\nb\nc",
                "line1\nline2\nline3",
            ];
            let subject = *lines.choose(&mut rng).unwrap();

            let patterns = [
                ".*",       // must match across newline like GNU
                "foo.*bar", // cross-newline greedy
                "\\(.*\\)", // capture group across newline
                "[^\n]*",   // explicit exclusion — should stop at \n
                ".*\n.*",   // explicit newline in pattern
            ];
            let pattern = *patterns.choose(&mut rng).unwrap();

            Some(vec![
                OsString::from("expr"),
                OsString::from(subject),
                OsString::from(":"),
                OsString::from(pattern),
            ])
        }

        // ── 3. Bracket expressions with trailing backslash ────────────────
        // The BRE→ERE transpiler's bracket-mode parser has a path where a
        // trailing `\` inside `[…]` should raise `TrailingBackslash` but
        // may fall through silently with fancy-regex.
        2 => {
            let patterns = [
                "[\\",        // unclosed bracket + trailing backslash
                "[a\\",       // char + trailing backslash in bracket
                "[a-z\\",     // range + trailing backslash
                "[^\\",       // negated bracket + trailing backslash
                "\\(\\[\\\\", // escaped bracket with backslash inside
                "[abc\\]",    // backslash before closing bracket
                "\\",         // lone trailing backslash (non-bracket)
                "a\\",        // char + trailing backslash
            ];
            let pattern = *patterns.choose(&mut rng).unwrap();
            let subject = "hello";

            Some(vec![
                OsString::from("expr"),
                OsString::from(subject),
                OsString::from(":"),
                OsString::from(pattern),
            ])
        }

        // ── 4. Backreferences ─────────────────────────────────────────────
        // fancy-regex supports `\1` backrefs; onig did too but with different
        // semantics on empty captures.  Exercise the backref path.
        _ => {
            let patterns = [
                "\\(.*\\)\\1",         // simple backref
                "\\(a*\\)\\1",         // empty-capture backref
                "\\(ab\\)\\1",         // non-empty backref
                "\\([a-z]*\\)\\1",     // backref with bracket class
                "\\(.*\\)\\(.*\\)\\1", // two groups, ref to first
            ];
            let subjects = ["abab", "aa", "abcabc", "", "aabbaa", "hello"];
            let pattern = *patterns.choose(&mut rng).unwrap();
            let subject = *subjects.choose(&mut rng).unwrap();

            Some(vec![
                OsString::from("expr"),
                OsString::from(subject),
                OsString::from(":"),
                OsString::from(pattern),
            ])
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let mut rng = rand::rng();

    // Use C locale to avoid false positives from locale-specific collation
    unsafe {
        env::set_var("LC_ALL", "C");
        env::set_var("LC_COLLATE", "C");
    }

    // 40% chance: regex-focused cases targeting fancy-regex issues
    // 60% chance: original arithmetic/general expression fuzzing
    let args: Vec<OsString> = if !data.is_empty() && data[0] % 10 < 4 {
        match generate_regex_match_expr(data) {
            Some(a) => a,
            None => {
                let expr = generate_expr(rng.random_range(0..=20));
                let mut a = vec![OsString::from("expr")];
                a.extend(expr.split_whitespace().map(OsString::from));
                a
            }
        }
    } else {
        let expr = generate_expr(rng.random_range(0..=20));
        let mut a = vec![OsString::from("expr")];
        a.extend(expr.split_whitespace().map(OsString::from));
        a
    };

    let rust_result = generate_and_run_uumain(&args, uumain, None);

    let gnu_result = match run_gnu_cmd(CMD_PATH, &args[1..], false, None) {
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
        "expr",
        &format!("{:?}", &args[1..]),
        None,
        &rust_result,
        &gnu_result,
        false,
    );
});
