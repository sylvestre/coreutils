// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! I/O processing infrastructure for tr operations

use crate::operation::ChunkProcessor;
use std::io::{BufRead, Write};
use uucore::error::{FromIo, UResult};
use uucore::translate;

/// Unified I/O processing for all operations
pub fn process_input<R, W, P>(input: &mut R, output: &mut W, processor: &P) -> UResult<()>
where
    R: BufRead,
    W: Write,
    P: ChunkProcessor + ?Sized,
{
    const BUFFER_SIZE: usize = 32768;
    let mut buf = [0; BUFFER_SIZE];
    let mut output_buf = Vec::with_capacity(BUFFER_SIZE);

    loop {
        let length = match input.read(&mut buf[..]) {
            Ok(0) => break,
            Ok(len) => len,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.map_err_context(|| translate!("tr-error-read-error"))),
        };

        output_buf.clear();
        processor.process_chunk(&buf[..length], &mut output_buf);

        if !output_buf.is_empty() {
            write_output(output, &output_buf)?;
        }
    }

    Ok(())
}

/// Helper function to handle platform-specific write operations
#[inline]
pub(crate) fn write_output<W: Write>(output: &mut W, buf: &[u8]) -> UResult<()> {
    #[cfg(not(target_os = "windows"))]
    return output
        .write_all(buf)
        .map_err_context(|| translate!("tr-error-write-error"));

    #[cfg(target_os = "windows")]
    match output.write_all(buf) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
            std::process::exit(13);
        }
        Err(err) => Err(err.map_err_context(|| translate!("tr-error-write-error"))),
    }
}

// Re-export for compatibility
pub use process_input as process_input_fast;
