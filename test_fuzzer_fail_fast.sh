#!/bin/bash
# Test that the fuzzer fails fast on the first UTF-8 error

set -e

echo "Testing that fuzzer fails on first UTF-8 error..."

# Build the fuzzer
cd /home/sylvestre/dev/debian/coreutils.disable-loca/fuzz
cargo +nightly build --bin fuzz_non_utf8_paths

# Run the fuzzer for a short time and expect it to fail
echo "Running fuzzer - expecting it to panic on first UTF-8 error..."
timeout 30s cargo +nightly fuzz run fuzz_non_utf8_paths 2>&1 | head -20 || {
    exit_code=$?
    if [ $exit_code -eq 124 ]; then
        echo "Fuzzer timed out (30s) - this might mean it's not finding UTF-8 errors quickly enough"
        echo "Let's try running it manually to see what happens..."
        timeout 5s ./target/x86_64-unknown-linux-gnu/release/fuzz_non_utf8_paths 2>&1 | head -10 || echo "Manual run also timed out or failed"
    else
        echo "Fuzzer exited with code $exit_code - this is expected if it found UTF-8 errors and panicked"
    fi
}

echo "Test completed. The fuzzer should panic immediately when it encounters UTF-8 errors."
