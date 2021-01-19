//  * This file is part of the uutils coreutils package.
//  *
//  * (c) Alex Lyon <arcterus@mail.com>
//  *
//  * For the full copyright and license information, please view the LICENSE
//  * file that was distributed with this source code.

// spell-checker:ignore (ToDO) cmdline evec seps rvec fdata

#[macro_use]
extern crate uucore;

use clap::{App, Arg};
use rand::Rng;
use std::{fs::File, path::PathBuf};
use std::io::{stdin, stdout, BufReader, BufWriter, Read, Write};
use std::usize::MAX as MAX_USIZE;

enum Mode {
    Default,
    Echo,
    InputRange((usize, usize)),
}

static ABOUT: &str = "generate random permutations";
static VERSION: &str = env!("CARGO_PKG_VERSION");

static OPT_ECHO: &str = "echo";
static OPT_INPUT_RANGE: &str = "input-range";
static OPT_HEAD_COUNT: &str = "head-count";
static OPT_OUTPUT: &str = "output";
static OPT_RANDOM_SOURCE: &str = "random-source";
static OPT_REPEAT: &str = "repeat";
static OPT_ZERO_TERMINATED: &str = "zero-terminated";

static ARG_FILES: &str = "files";

fn get_usage() -> String {
    format!(
        "{0} [OPTION]... [FILE]
    {0} -e [OPTION]... [ARG]...
    {0} -i LO-HI [OPTION]...
  
  Write a random permutation of the input lines to standard output.
  With no FILE, or when FILE is -, read standard input.",
        executable!()
    )
}

pub fn uumain(args: impl uucore::Args) -> i32 {
    let usage = get_usage();

    let matches = App::new(executable!())
        .version(VERSION)
        .about(ABOUT)
        .usage(&usage[..])
        .arg(
            Arg::with_name(OPT_ECHO)
                .short("e")
                .long(OPT_ECHO)
                .help("treat each ARG as an input line"),
        )
        .arg(
            Arg::with_name(OPT_INPUT_RANGE)
                .short("i")
                .long(OPT_INPUT_RANGE)
                .help("treat each number LO through HI as an input line")
                .value_name("LO-HI"),
        )
        .arg(
            Arg::with_name(OPT_HEAD_COUNT)
                .short("n")
                .long(OPT_HEAD_COUNT)
                .help("output at most COUNT lines")
                .value_name("COUNT"),
        )
        .arg(
            Arg::with_name(OPT_OUTPUT)
                .short("o")
                .long(OPT_OUTPUT)
                .help("write result to FILE instead of standard output")
                .value_name("FILE"),
        )
        .arg(
            Arg::with_name(OPT_RANDOM_SOURCE)
                .long(OPT_RANDOM_SOURCE)
                .help("get random bytes from FILE")
                .value_name("FILE"),
        )
        .arg(
            Arg::with_name(OPT_REPEAT)
                .short("r")
                .long(OPT_REPEAT)
                .help("output lines can be repeated"),
        )
        .arg(
            Arg::with_name(OPT_ZERO_TERMINATED)
                .short("z")
                .long(OPT_ZERO_TERMINATED)
                .help("end lines with 0 byte, not newline"),
        )
        .arg(
            Arg::with_name(ARG_FILES)
                .multiple(true)
                .takes_value(true)
                .required(true)
                .min_values(0)
                .max_values(1)
        )
        .get_matches_from(args);

    let paths: Vec<String> = matches
        .values_of(ARG_FILES)
        .map(|v| v.map(ToString::to_string).collect())
        .unwrap_or_default();


    let echo = matches.is_present(OPT_ECHO);
    let mode = match matches.value_of(OPT_INPUT_RANGE) {
        Some(range) => {
            if echo {
                show_error!("cannot specify more than one mode");
                return 1;
            }
            match parse_range(range.to_string()) {
                Ok(m) => Mode::InputRange(m),
                Err(msg) => {
                    crash!(1, "{}", msg);
                }
            }
        }
        None => {
            if echo {
                Mode::Echo
            } else {
                if paths.is_empty() {
                    paths.push("-".to_owned());
                }
                Mode::Default
            }
        }
    };
    let repeat = matches.is_present(OPT_REPEAT);
    let sep = if matches.is_present(OPT_ZERO_TERMINATED) {
        0x00 as u8
    } else {
        0x0a as u8
    };
    let count = match matches.value_of(OPT_HEAD_COUNT) {
        Some(cnt) => match cnt.parse::<usize>() {
            Ok(val) => val,
            Err(e) => {
                show_error!("'{}' is not a valid count: {}", cnt, e);
                return 1;
            }
        },
        None => MAX_USIZE,
    };
    let output = matches.value_of(OPT_OUTPUT).map(String::from);
    let random = matches.value_of(OPT_RANDOM_SOURCE).map(String::from);

    match mode {
        Mode::Echo => {
            // XXX: this doesn't correctly handle non-UTF-8 cmdline args
            let mut evec = matches
                .values_of(ARG_FILES)
                .map(String::as_bytes)
                .collect::<Vec<&[u8]>>();
            find_seps(&mut evec, sep);
            shuf_bytes(&mut evec, repeat, count, sep, output, random);
        }
        Mode::InputRange((b, e)) => {
            let rvec = (b..e).map(|x| format!("{}", x)).collect::<Vec<String>>();
            let mut rvec = rvec.iter().map(String::as_bytes).collect::<Vec<&[u8]>>();
            shuf_bytes(&mut rvec, repeat, count, sep, output, random);
        }
        Mode::Default => {
            let fdata = read_input_file(&paths[0]);
            let mut fdata = vec![&fdata[..]];
            find_seps(&mut fdata, sep);
            shuf_bytes(&mut fdata, repeat, count, sep, output, random);
        }
    }

    0
}

fn read_input_file(filename: &str) -> Vec<u8> {
    let mut file = BufReader::new(if filename == "-" {
        Box::new(stdin()) as Box<dyn Read>
    } else {
        match File::open(filename) {
            Ok(f) => Box::new(f) as Box<dyn Read>,
            Err(e) => crash!(1, "failed to open '{}': {}", filename, e),
        }
    });

    let mut data = Vec::new();
    if let Err(e) = file.read_to_end(&mut data) {
        crash!(1, "failed reading '{}': {}", filename, e)
    };

    data
}

fn find_seps(data: &mut Vec<&[u8]>, sep: u8) {
    // need to use for loop so we don't borrow the vector as we modify it in place
    // basic idea:
    // * We don't care about the order of the result. This lets us slice the slices
    //   without making a new vector.
    // * Starting from the end of the vector, we examine each element.
    // * If that element contains the separator, we remove it from the vector,
    //   and then sub-slice it into slices that do not contain the separator.
    // * We maintain the invariant throughout that each element in the vector past
    //   the ith element does not have any separators remaining.
    for i in (0..data.len()).rev() {
        if data[i].contains(&sep) {
            let this = data.swap_remove(i);
            let mut p = 0;
            let mut i = 1;
            loop {
                if i == this.len() {
                    break;
                }

                if this[i] == sep {
                    data.push(&this[p..i]);
                    p = i + 1;
                }
                i += 1;
            }
            if p < this.len() {
                data.push(&this[p..i]);
            }
        }
    }
}

fn shuf_bytes(
    input: &mut Vec<&[u8]>,
    repeat: bool,
    count: usize,
    sep: u8,
    output: Option<String>,
    random: Option<String>,
) {
    let mut output = BufWriter::new(match output {
        None => Box::new(stdout()) as Box<dyn Write>,
        Some(s) => match File::create(&s[..]) {
            Ok(f) => Box::new(f) as Box<dyn Write>,
            Err(e) => crash!(1, "failed to open '{}' for writing: {}", &s[..], e),
        },
    });

    let mut rng = match random {
        Some(r) => WrappedRng::RngFile(rand::read::ReadRng::new(match File::open(&r[..]) {
            Ok(f) => f,
            Err(e) => crash!(1, "failed to open random source '{}': {}", &r[..], e),
        })),
        None => WrappedRng::RngDefault(rand::thread_rng()),
    };

    // we're generating a random usize. To keep things fair, we take this number mod ceil(log2(length+1))
    let mut len_mod = 1;
    let mut len = input.len();
    while len > 0 {
        len >>= 1;
        len_mod <<= 1;
    }

    let mut count = count;
    while count > 0 && !input.is_empty() {
        let mut r = input.len();
        while r >= input.len() {
            r = rng.next_usize() % len_mod;
        }

        // write the randomly chosen value and the separator
        output
            .write_all(input[r])
            .unwrap_or_else(|e| crash!(1, "write failed: {}", e));
        output
            .write_all(&[sep])
            .unwrap_or_else(|e| crash!(1, "write failed: {}", e));

        // if we do not allow repeats, remove the chosen value from the input vector
        if !repeat {
            // shrink the mask if we will drop below a power of 2
            if input.len() % 2 == 0 && len_mod > 2 {
                len_mod >>= 1;
            }
            input.swap_remove(r);
        }

        count -= 1;
    }
}

fn parse_range(input_range: String) -> Result<(usize, usize), String> {
    let split: Vec<&str> = input_range.split('-').collect();
    if split.len() != 2 {
        Err("invalid range format".to_owned())
    } else {
        let begin = match split[0].parse::<usize>() {
            Ok(m) => m,
            Err(e) => return Err(format!("{} is not a valid number: {}", split[0], e)),
        };
        let end = match split[1].parse::<usize>() {
            Ok(m) => m,
            Err(e) => return Err(format!("{} is not a valid number: {}", split[1], e)),
        };
        Ok((begin, end + 1))
    }
}

enum WrappedRng {
    RngFile(rand::read::ReadRng<File>),
    RngDefault(rand::ThreadRng),
}

impl WrappedRng {
    fn next_usize(&mut self) -> usize {
        match *self {
            WrappedRng::RngFile(ref mut r) => r.gen(),
            WrappedRng::RngDefault(ref mut r) => r.gen(),
        }
    }
}
