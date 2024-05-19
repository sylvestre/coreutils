// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use os_display::Quotable;
use regex::Regex;
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, BufReader, Read},
};

use crate::{
    error::{set_exit_code, FromIo, UResult},
    show, show_error, show_warning_caps,
    sum::{
        Blake2b, Blake3, Digest, DigestWriter, Md5, Sha1, Sha224, Sha256, Sha384, Sha512, Sm3, BSD,
        CRC, SYSV,
    },
    util_name,
};
use std::io::stdin;
use std::io::BufRead;

pub const ALGORITHM_OPTIONS_SYSV: &str = "sysv";
pub const ALGORITHM_OPTIONS_BSD: &str = "bsd";
pub const ALGORITHM_OPTIONS_CRC: &str = "crc";
pub const ALGORITHM_OPTIONS_MD5: &str = "md5";
pub const ALGORITHM_OPTIONS_SHA1: &str = "sha1";
pub const ALGORITHM_OPTIONS_SHA224: &str = "sha224";
pub const ALGORITHM_OPTIONS_SHA256: &str = "sha256";
pub const ALGORITHM_OPTIONS_SHA384: &str = "sha384";
pub const ALGORITHM_OPTIONS_SHA512: &str = "sha512";
pub const ALGORITHM_OPTIONS_BLAKE2B: &str = "blake2b";
pub const ALGORITHM_OPTIONS_BLAKE3: &str = "blake3";
pub const ALGORITHM_OPTIONS_SM3: &str = "sm3";

pub const SUPPORTED_ALGO: [&str; 12] = [
    ALGORITHM_OPTIONS_SYSV,
    ALGORITHM_OPTIONS_BSD,
    ALGORITHM_OPTIONS_CRC,
    ALGORITHM_OPTIONS_MD5,
    ALGORITHM_OPTIONS_SHA1,
    ALGORITHM_OPTIONS_SHA224,
    ALGORITHM_OPTIONS_SHA256,
    ALGORITHM_OPTIONS_SHA384,
    ALGORITHM_OPTIONS_SHA512,
    ALGORITHM_OPTIONS_BLAKE2B,
    ALGORITHM_OPTIONS_BLAKE3,
    ALGORITHM_OPTIONS_SM3,
];

#[allow(clippy::comparison_chain)]
pub fn cksum_output(bad_format: i32, failed_cksum: i32, failed_open_file: i32) {
    if bad_format == 1 {
        show_warning_caps!("{} line is improperly formatted", bad_format);
    } else if bad_format > 1 {
        show_warning_caps!("{} lines are improperly formatted", bad_format);
    }

    if failed_cksum == 1 {
        show_warning_caps!("{} computed checksum did NOT match", failed_cksum);
    } else if failed_cksum > 1 {
        show_warning_caps!("{} computed checksums did NOT match", failed_cksum);
    }

    if failed_open_file == 1 {
        show_warning_caps!("{} listed file could not be read", failed_open_file);
    } else if failed_open_file > 1 {
        show_warning_caps!("{} listed files could not be read", failed_open_file);
    }
}

pub fn detect_algo(
    algo: &str,
    length: Option<usize>,
) -> (&'static str, Box<dyn Digest + 'static>, usize) {
    match algo {
        ALGORITHM_OPTIONS_SYSV => (
            ALGORITHM_OPTIONS_SYSV,
            Box::new(SYSV::new()) as Box<dyn Digest>,
            512,
        ),
        ALGORITHM_OPTIONS_BSD => (
            ALGORITHM_OPTIONS_BSD,
            Box::new(BSD::new()) as Box<dyn Digest>,
            1024,
        ),
        ALGORITHM_OPTIONS_CRC => (
            ALGORITHM_OPTIONS_CRC,
            Box::new(CRC::new()) as Box<dyn Digest>,
            256,
        ),
        ALGORITHM_OPTIONS_MD5 => (
            ALGORITHM_OPTIONS_MD5,
            Box::new(Md5::new()) as Box<dyn Digest>,
            128,
        ),
        ALGORITHM_OPTIONS_SHA1 => (
            ALGORITHM_OPTIONS_SHA1,
            Box::new(Sha1::new()) as Box<dyn Digest>,
            160,
        ),
        ALGORITHM_OPTIONS_SHA224 => (
            ALGORITHM_OPTIONS_SHA224,
            Box::new(Sha224::new()) as Box<dyn Digest>,
            224,
        ),
        ALGORITHM_OPTIONS_SHA256 => (
            ALGORITHM_OPTIONS_SHA256,
            Box::new(Sha256::new()) as Box<dyn Digest>,
            256,
        ),
        ALGORITHM_OPTIONS_SHA384 => (
            ALGORITHM_OPTIONS_SHA384,
            Box::new(Sha384::new()) as Box<dyn Digest>,
            384,
        ),
        ALGORITHM_OPTIONS_SHA512 => (
            ALGORITHM_OPTIONS_SHA512,
            Box::new(Sha512::new()) as Box<dyn Digest>,
            512,
        ),
        ALGORITHM_OPTIONS_BLAKE2B => (
            ALGORITHM_OPTIONS_BLAKE2B,
            Box::new(if let Some(length) = length {
                Blake2b::with_output_bytes(length)
            } else {
                Blake2b::new()
            }) as Box<dyn Digest>,
            512,
        ),
        ALGORITHM_OPTIONS_BLAKE3 => (
            ALGORITHM_OPTIONS_BLAKE3,
            Box::new(Blake3::new()) as Box<dyn Digest>,
            256,
        ),
        ALGORITHM_OPTIONS_SM3 => (
            ALGORITHM_OPTIONS_SM3,
            Box::new(Sm3::new()) as Box<dyn Digest>,
            512,
        ),
        _ => unreachable!("unknown algorithm: clap should have prevented this case"),
    }
}

/***
 * Do the checksum validation (can be strict or not)
*/
pub fn perform_checksum_validation<'a, I>(
    files: I,
    strict: bool,
    status: bool,
    warn: bool,
    binary: bool,
    algo_name_input: Option<&str>,
    length_input: Option<usize>,
) -> UResult<()>
where
    I: Iterator<Item = &'a OsStr>,
{
    // Regexp to handle the two input formats:
    // 1. <algo>[-<bits>] (<filename>) = <checksum>
    //    algo must be uppercase or b (for blake2b)
    // 2. <checksum> [* ]<filename>
    let regex_pattern = r"^\s*\\?(?P<algo>(?:[A-Z0-9]+|BLAKE2b))(?:-(?P<bits>\d+))?\s?\((?P<filename1>.*)\) = (?P<checksum1>[a-fA-F0-9]+)$|^(?P<checksum2>[a-fA-F0-9]+)\s[* ](?P<filename2>.*)";
    let re = Regex::new(regex_pattern).unwrap();

    // if cksum has several input files, it will print the result for each file
    for filename_input in files {
        let mut bad_format = 0;
        let mut failed_cksum = 0;
        let mut failed_open_file = 0;
        let mut properly_formatted = false;
        let input_is_stdin = filename_input == OsStr::new("-");

        let file: Box<dyn Read> = if input_is_stdin {
            Box::new(stdin()) // Use stdin if "-" is specified
        } else {
            match File::open(filename_input) {
                Ok(f) => Box::new(f),
                Err(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "{}: No such file or directory",
                            filename_input.to_string_lossy()
                        ),
                    )
                    .into());
                }
            }
        };
        let reader = BufReader::new(file);

        // for each line in the input, check if it is a valid checksum line
        for (i, line) in reader.lines().enumerate() {
            let line = line.unwrap_or_else(|_| String::new());
            if let Some(caps) = re.captures(&line) {
                properly_formatted = true;

                // Determine what kind of file input we had
                // we need it for case "--check -a sm3 <file>" when <file> is
                // <algo>[-<bits>] (<filename>) = <checksum>
                let algo_based_format =
                    caps.name("filename1").is_some() && caps.name("checksum1").is_some();

                let filename_to_check = caps
                    .name("filename1")
                    .or(caps.name("filename2"))
                    .unwrap()
                    .as_str();
                let expected_checksum = caps
                    .name("checksum1")
                    .or(caps.name("checksum2"))
                    .unwrap()
                    .as_str();

                // If the algo_name is provided, we use it, otherwise we try to detect it
                let (algo_name, length) = if algo_based_format {
                    // When the algo-based format is matched, extract details from regex captures
                    let algorithm = caps.name("algo").map_or("", |m| m.as_str()).to_lowercase();
                    if !SUPPORTED_ALGO.contains(&algorithm.as_str()) {
                        // Not supported algo, leave early
                        properly_formatted = false;
                        continue;
                    }

                    let bits = caps.name("bits").map_or(Some(None), |m| {
                        let bits_value = m.as_str().parse::<usize>().unwrap();
                        if bits_value % 8 == 0 {
                            Some(Some(bits_value / 8))
                        } else {
                            properly_formatted = false;
                            None // Return None to signal a parsing or divisibility issue
                        }
                    });

                    if bits.is_none() {
                        // If bits is None, we have a parsing or divisibility issue
                        // Exit the loop outside of the closure
                        continue;
                    }

                    (algorithm, bits.unwrap())
                } else if let Some(a) = algo_name_input {
                    // When a specific algorithm name is input, use it and default bits to None
                    (a.to_lowercase(), length_input.map(|length| length / 8))
                } else {
                    // Default case if no algorithm is specified and non-algo based format is matched
                    (String::new(), None)
                };

                if algo_based_format && algo_name_input.map_or(false, |input| algo_name != input) {
                    bad_format += 1;
                    continue;
                }

                if algo_name.is_empty() {
                    // we haven't been able to detect the algo name. No point to continue
                    properly_formatted = false;
                    continue;
                }
                let (_, mut algo, bits) = detect_algo(&algo_name, length);

                // manage the input file
                let file_to_check: Box<dyn Read> = if filename_to_check == "-" {
                    Box::new(stdin()) // Use stdin if "-" is specified in the checksum file
                } else {
                    match File::open(filename_to_check) {
                        Ok(f) => Box::new(f),
                        Err(err) => {
                            // yes, we have both stderr and stdout here
                            show!(err.map_err_context(|| filename_to_check.to_string()));
                            println!("{}: FAILED open or read", filename_to_check);
                            failed_open_file += 1;
                            // we could not open the file but we want to continue
                            continue;
                        }
                    }
                };
                let mut file_reader = BufReader::new(file_to_check);
                // Read the file and calculate the checksum
                let (calculated_checksum, _) =
                    digest_reader(&mut algo, &mut file_reader, binary, bits).unwrap();

                // Do the checksum validation
                if expected_checksum == calculated_checksum {
                    println!("{}: OK", filename_to_check);
                } else {
                    if !status {
                        println!("{}: FAILED", filename_to_check);
                    }
                    failed_cksum += 1;
                }
            } else {
                if warn {
                    eprintln!(
                        "{}: {}: {}: improperly formatted {:?} checksum line",
                        util_name(),
                        &filename_input.maybe_quote(),
                        i + 1,
                        algo_name_input.unwrap_or("Unknown algorithm")
                    );
                }
                if line.is_empty() {
                    continue;
                }
                bad_format += 1;
            }
        }

        // not a single line correctly formatted found
        // return an error
        if !properly_formatted {
            let filename = filename_input.to_string_lossy();
            show_error!(
                "{}: no properly formatted checksum lines found",
                if input_is_stdin {
                    "standard input"
                } else {
                    &filename
                }
                .maybe_quote()
            );
            set_exit_code(1);
        }
        // strict means that we should have an exit code.
        if strict && bad_format > 0 {
            set_exit_code(1);
        }

        // if we have any failed checksum verification, we set an exit code
        if failed_cksum > 0 || failed_open_file > 0 {
            set_exit_code(1);
        }

        // if any incorrectly formatted line, show it
        cksum_output(bad_format, failed_cksum, failed_open_file);
    }
    Ok(())
}

pub fn digest_reader<T: Read>(
    digest: &mut Box<dyn Digest>,
    reader: &mut BufReader<T>,
    binary: bool,
    output_bits: usize,
) -> io::Result<(String, usize)> {
    digest.reset();

    // Read bytes from `reader` and write those bytes to `digest`.
    //
    // If `binary` is `false` and the operating system is Windows, then
    // `DigestWriter` replaces "\r\n" with "\n" before it writes the
    // bytes into `digest`. Otherwise, it just inserts the bytes as-is.
    //
    // In order to support replacing "\r\n", we must call `finalize()`
    // in order to support the possibility that the last character read
    // from the reader was "\r". (This character gets buffered by
    // `DigestWriter` and only written if the following character is
    // "\n". But when "\r" is the last character read, we need to force
    // it to be written.)
    let mut digest_writer = DigestWriter::new(digest, true);
    let output_size = std::io::copy(reader, &mut digest_writer)? as usize;
    digest_writer.finalize();

    if digest.output_bits() > 0 {
        Ok((digest.result_str(), output_size))
    } else {
        // Assume it's SHAKE.  result_str() doesn't work with shake (as of 8/30/2016)
        let mut bytes = vec![0; (output_bits + 7) / 8];
        digest.hash_finalize(&mut bytes);
        Ok((hex::encode(bytes), output_size))
    }
}
