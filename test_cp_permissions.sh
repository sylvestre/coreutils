#!/bin/bash

# Test script to reproduce cp permission issue
# This script demonstrates the difference between uutils cp and GNU cp
# when copying directories with the -a flag

set -e

echo "=== CP Permission Test Script ==="
echo "Creating test directory structure..."

# Clean up any existing test directories
rm -rf ~/test-images ~/test-images-coreutils ~/test-images-gnu

# Create the test directory structure
mkdir -p ~/test-images/{fail,gif-test-suite,randomly-modified,reftests}

# Set the original permissions to match the issue
chmod 755 ~/test-images/fail
chmod 755 ~/test-images/gif-test-suite
chmod 755 ~/test-images/randomly-modified
chmod 755 ~/test-images/reftests

# Add some test files
echo "test content for fail" > ~/test-images/fail/test1.txt
echo "test content for gif-test-suite" > ~/test-images/gif-test-suite/test2.txt
echo "test content for randomly-modified" > ~/test-images/randomly-modified/test3.txt
echo "test content for reftests" > ~/test-images/reftests/test4.txt

echo "Original directory permissions:"
ls -l ~/test-images/

echo
echo "=== Testing uutils cp ==="
# Test with uutils cp using cargo run
cargo run --quiet -- cp -a ~/test-images ~/test-images-coreutils
echo "uutils cp results:"
ls -l ~/test-images-coreutils/

echo
echo "=== Testing GNU cp ==="
# Test with GNU cp
/usr/bin/cp -a ~/test-images ~/test-images-gnu
echo "GNU cp results:"
ls -l ~/test-images-gnu/

echo
echo "=== Comparison ==="
echo "Directories that have different permissions:"
echo "Comparing ~/test-images-coreutils/ vs ~/test-images-gnu/"

# Compare permissions
for dir in fail gif-test-suite randomly-modified reftests; do
    uutils_perms=$(ls -ld ~/test-images-coreutils/$dir | cut -d' ' -f1)
    gnu_perms=$(ls -ld ~/test-images-gnu/$dir | cut -d' ' -f1)

    if [ "$uutils_perms" != "$gnu_perms" ]; then
        echo "  $dir: uutils=$uutils_perms, gnu=$gnu_perms [DIFFERENT]"
    else
        echo "  $dir: $uutils_perms [SAME]"
    fi
done

echo
echo "Test completed."
