#!/bin/bash
# Test if chmod passes the fuzzer

set -e

# Create test directory
TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"

# Create files with non-UTF-8 names
/usr/bin/touch "$(printf 'test_\xFF\xFE.txt')" 2>/dev/null || true
/usr/bin/touch "$(printf 'test_\xC0\x80.txt')" 2>/dev/null || true

echo "Testing programs with non-UTF-8 paths..."

# Test GNU chmod (should work)
echo -n "GNU chmod: "
if /usr/bin/chmod 644 "$(printf 'test_\xFF\xFE.txt')" 2>&1; then
    echo "PASS"
else
    echo "FAIL"
fi

# Test uutils chmod (should now work with our fix)
echo -n "uutils chmod: "
if /home/sylvestre/dev/debian/coreutils.disable-loca/target/debug/coreutils chmod 644 "$(printf 'test_\xC0\x80.txt')" 2>&1; then
    echo "PASS"
else
    echo "FAIL"
fi

# Test other programs that should still fail
for prog in basename dirname ls; do
    echo -n "uutils $prog: "
    if /home/sylvestre/dev/debian/coreutils.disable-loca/target/debug/coreutils $prog "$(printf 'test_\xFF\xFE.txt')" 2>&1 >/dev/null; then
        echo "PASS (unexpected!)"
    else
        echo "FAIL (expected - not fixed yet)"
    fi
done

# Clean up
cd /
rm -rf "$TEST_DIR"

echo "Test completed."
