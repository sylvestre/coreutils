#!/bin/bash
# Test chmod with non-UTF-8 paths

set -e

TEST_DIR=$(mktemp -d)
echo "Testing in directory: $TEST_DIR"

cd "$TEST_DIR"

# Create a file with non-UTF-8 name using GNU touch
/usr/bin/touch "$(printf 'test_\xFF\xFE.txt')" 2>/dev/null || {
    echo "Failed to create test file with non-UTF-8 name"
    exit 1
}

echo "Created test file with non-UTF-8 name"
ls -la

# Test with the fixed chmod
echo "Testing uutils chmod with non-UTF-8 filename..."
if /home/sylvestre/dev/debian/coreutils.disable-loca/target/debug/coreutils chmod 644 "$(printf 'test_\xFF\xFE.txt')" 2>&1; then
    echo "SUCCESS: chmod handled non-UTF-8 filename correctly!"
else
    echo "FAILED: chmod could not handle non-UTF-8 filename"
    exit 1
fi

# Verify permissions were actually changed
echo "Verifying permissions..."
ls -la

# Clean up
cd /
rm -rf "$TEST_DIR"

echo "Test completed successfully!"
