// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! `--output-format=json`: one record per file, keyed like jc's `hashsum`
//! parser for digests and like its `cksum` parser for the legacy sums.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use uucore::checksum::compute::DigestFormat;
use uucore::checksum::{ReadingMode, SizedAlgoKind, digest_reader};
use uucore::display::Quotable;
use uucore::error::{FromIo, UResult};
use uucore::json::{Value, json, print_json};
use uucore::show;
use uucore::sum::DigestOutput;

/// Prints the checksum of each file as a JSON array. Unreadable files are
/// reported on stderr and left out, as the text output does.
pub(crate) fn print_checksums<'a>(
    algo: SizedAlgoKind,
    digest_format: DigestFormat,
    files: impl Iterator<Item = &'a OsStr>,
) -> UResult<()> {
    let mut records = Vec::new();
    let mut digest = algo.create_digest();
    for filename in files {
        let mut reader: Box<dyn Read> = if filename == "-" {
            Box::new(io::stdin())
        } else {
            match File::open(Path::new(filename)) {
                Ok(file) => Box::new(BufReader::new(file)),
                Err(err) => {
                    show!(err.map_err_context(|| filename.maybe_quote().to_string()));
                    continue;
                }
            }
        };
        match digest_reader(&mut digest, &mut reader, ReadingMode::Binary) {
            Ok((sum, size)) => records.push(record(algo, digest_format, filename, &sum, size)?),
            Err(err) => show!(err.map_err_context(|| filename.maybe_quote().to_string())),
        }
    }
    print_json(&Value::Array(records))?;
    Ok(())
}

/// `{"filename", "checksum", "blocks"}` for the legacy sums, where `blocks`
/// is the second number the text prints (a byte count for CRC), and
/// `{"filename", "hash"}` for the digests.
fn record(
    algo: SizedAlgoKind,
    digest_format: DigestFormat,
    filename: &OsStr,
    sum: &DigestOutput,
    size: usize,
) -> UResult<Value> {
    let filename = filename.to_string_lossy();
    let legacy_sum = match sum {
        DigestOutput::Crc(sum) => Some(u64::from(*sum)),
        DigestOutput::U16(sum) => Some(u64::from(*sum)),
        DigestOutput::Vec(_) => None,
    };
    if let Some(checksum) = legacy_sum {
        let blocks = match algo {
            SizedAlgoKind::Sysv | SizedAlgoKind::Bsd => size.div_ceil(algo.bitlen()),
            _ => size,
        };
        return Ok(json!({ "filename": filename, "checksum": checksum, "blocks": blocks }));
    }
    let hash = match digest_format {
        DigestFormat::Hexadecimal => sum.to_hex()?,
        DigestFormat::Base64 => sum.to_base64()?,
    };
    Ok(json!({ "filename": filename, "hash": hash }))
}
