use crate::common::util::*;
extern crate regex;
use self::regex::Regex;

#[test]
fn test_ls_ls() {
    new_ucmd!().succeeds();
}

#[test]
// Not a thing on windows
#[cfg(not(windows))]
fn test_ls_ls_i() {
    let (at, mut _ucmd) = at_and_ucmd!();
    let dir = "test_ls_directory";
    let file_a = "test_ls_directory/test_ls_recursive_file_a";
    let file_b = "test_ls_directory/test_ls_recursive_file_b";

    at.mkdir(dir);
    at.touch(file_a);
    at.touch(file_b);

    let result = new_ucmd!().arg("-i").arg(at.plus(dir)).run();
    assert!(result.success);
    println!("{}", result.stdout);
    assert!(result.stdout.contains("test_ls_recursive_file_a"));
    assert!(result.stdout.contains("test_ls_recursive_file_b"));

    let re = Regex::new(r"[0-9] .*test_ls.*").unwrap();
    assert!(re.is_match(&result.stdout.trim()));

    let result = new_ucmd!().arg("-il").arg(at.plus(dir)).run();
    assert!(result.success);
    println!("{}", result.stdout);
    assert!(result.stdout.contains("test_ls_recursive_file_a"));
    assert!(result.stdout.contains("test_ls_recursive_file_b"));

    let re = Regex::new(r"[0-9] .* .* .*test_ls.*").unwrap();
    assert!(re.is_match(&result.stdout.trim()));
}


#[test]
fn test_ls_ls_all() {
    let (at, mut _ucmd) = at_and_ucmd!();
    let dir = "test_ls_directory";
    let file_a = "test_ls_directory/UPPERCASE_file_a";
    let file_b = "test_ls_directory/lowercase_file_b";

    at.mkdir(dir);
    at.touch(file_a);
    at.touch(file_b);

    let result = new_ucmd!().arg(at.plus(dir)).arg("-a").run();
    assert!(result.success);
    println!("{}", result.stdout);
    let re = Regex::new(r"^.").unwrap();
    assert!(re.is_match(&result.stdout.trim()));
    assert!(result.stdout.contains(".."));
    assert!(result.stdout.contains("UPPERCASE_file_a"));
    assert!(result.stdout.contains("lowercase_file_b"));
}


#[test]
fn test_ls_ls_recursive() {
    let (at, mut _ucmd) = at_and_ucmd!();
    let dir = "test_ls_directory";
    let file_a = "test_ls_directory/UPPERCASE_file_a";
    let file_b = "test_ls_directory/lowercase_file_b";

    at.mkdir(dir);
    at.touch(file_a);
    at.touch(file_b);

    let result = new_ucmd!().arg(at.as_string()).arg("-R").run();
    assert!(result.success);
    println!("{}", result.stdout);
    assert!(result.stdout.contains("test_ls_directory:"));
    assert!(result.stdout.contains("UPPERCASE_file_a"));
    assert!(result.stdout.contains("lowercase_file_b"));
}

#[test]
// Creation by date
fn test_ls_ls_tc() {
    let (at, mut _ucmd) = at_and_ucmd!();
    let dir = "test_ls_directory";
    let file_a = "test_ls_directory/UPPERCASE_file_a";
    let file_b = "test_ls_directory/lowercase_file_b";

    at.mkdir(dir);
    at.touch(file_a);
    at.touch(file_b);

    let result = new_ucmd!().arg(at.plus(dir)).run();
    assert!(result.success);
    println!("{}", result.stdout);

    let result = new_ucmd!().arg("-tc").arg(at.plus(dir)).run();
    assert!(result.success);
    println!("{}", result.stdout);
    let re = Regex::new(r"^UPPERCASE_file_a.*").unwrap();
    assert!(re.is_match(&result.stdout.trim()));
    let re = Regex::new(r"lowercase_file_b$").unwrap();
    assert!(re.is_match(&result.stdout.trim()));
}

#[test]
fn test_ls_ls_size() {
    let (at, mut _ucmd) = at_and_ucmd!();
    let dir = "test_ls_directory";
    let file_a = "test_ls_directory/file_a";
    let file_b = "test_ls_directory/file_b";

    at.mkdir(dir);
    at.touch(file_a);
    at.write(file_a, "short");
    at.touch(file_b);
    at.write(file_b, "a longer file to make sure it is bigger");

    let result = new_ucmd!().arg(at.plus(dir)).run();
    assert!(result.success);
    println!("{}", result.stdout);

    let result = new_ucmd!().arg("-S").arg(at.plus(dir)).run();
    assert!(result.success);
    println!("{}", result.stdout);
    let re = Regex::new(r"^file_b.*").unwrap();
    assert!(re.is_match(&result.stdout.trim()));
    let re = Regex::new(r"file_a$").unwrap();
    assert!(re.is_match(&result.stdout.trim()));

    let result = new_ucmd!().arg("-Sr").arg(at.plus(dir)).run();
    // smaller file first
    assert!(result.success);
    println!("{}", result.stdout);
    let re = Regex::new(r"^file_a.*").unwrap();
    assert!(re.is_match(&result.stdout.trim()));
    let re = Regex::new(r"file_b$").unwrap();
    assert!(re.is_match(&result.stdout.trim()));
}


#[test]
fn test_ls_ls_color() {
    new_ucmd!().arg("--color").succeeds();
    new_ucmd!().arg("--color=always").succeeds();
    new_ucmd!().arg("--color=never").succeeds();
}
