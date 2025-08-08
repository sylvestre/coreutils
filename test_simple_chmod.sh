#!/bin/bash
# Simple test to debug chmod --reference issue

set -e

TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"

# Create regular files first
echo "test1" > ref.txt
echo "test2" > target.txt

# Set reference file permissions
chmod 751 ref.txt

# Test normal reference operation
echo "Testing normal --reference:"
if /home/sylvestre/dev/debian/coreutils.disable-loca/target/debug/coreutils chmod --reference ref.txt target.txt 2>&1; then
    echo "SUCCESS: normal reference works"
    ls -la
else
    echo "FAIL: normal reference failed"
fi

# Now test with non-UTF-8 files created using GNU tools
/usr/bin/touch "$(printf 'ref_\xFF\xFE.txt')"
/usr/bin/touch "$(printf 'target_\xC0\x80.txt')"
/usr/bin/chmod 751 "$(printf 'ref_\xFF\xFE.txt')"

echo "Testing --reference with non-UTF-8 files:"
if /home/sylvestre/dev/debian/coreutils.disable-loca/target/debug/coreutils chmod --reference "$(printf 'ref_\xFF\xFE.txt')" "$(printf 'target_\xC0\x80.txt')" 2>&1; then
    echo "SUCCESS: non-UTF-8 reference works"
    ls -la
else
    echo "FAIL: non-UTF-8 reference failed"
fi

# Clean up
cd /
rm -rf "$TEST_DIR"
