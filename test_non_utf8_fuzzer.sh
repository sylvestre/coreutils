#!/bin/bash
# Simple test to verify non-UTF-8 path handling across coreutils programs

set -e

# Create a temporary directory for our tests
TEST_DIR=$(mktemp -d)
echo "Testing in directory: $TEST_DIR"

cd "$TEST_DIR"

# Create files with non-UTF-8 names using GNU tools (to bypass uutils issue)
# These will contain invalid UTF-8 sequences
echo "test content" > normal_file.txt
echo "Creating files with non-UTF-8 names using GNU coreutils..."

# Use GNU touch if available to create files with non-UTF-8 names
if command -v /usr/bin/touch >/dev/null 2>&1; then
    /usr/bin/touch "$(printf 'test_\xFF\xFE.txt')" 2>/dev/null || true
    /usr/bin/mkdir "$(printf 'dir_\xC0\x80')" 2>/dev/null || true
else
    echo "GNU touch not available, creating files manually"
    # Create using shell redirection to avoid path issues
    echo "content" > normal_file.txt
    # Create a file with bytes that will be invalid UTF-8
    echo "content" > "$(echo -e 'test_\xFF\xFE.txt')" 2>/dev/null || true
fi

echo "Created test files with non-UTF-8 names"

# List of programs to test that typically work with file paths
PROGRAMS=(
    "basename" "cat" "cp" "dirname" "du" "head" "ln"
    "ls" "mkdir" "mv" "readlink" "realpath" "rm" "rmdir"
    "tail" "touch" "unlink"
)

echo "Testing coreutils programs with non-UTF-8 filenames..."

# Test each program with the non-UTF-8 files
for prog in "${PROGRAMS[@]}"; do
    echo "Testing $prog..."

    # Test with the non-UTF-8 filename - this should reveal UTF-8 conversion issues
    for file in $(ls); do
        if command -v "$prog" >/dev/null 2>&1; then
            case "$prog" in
                "basename"|"dirname"|"realpath")
                    timeout 2s "$prog" "$file" 2>&1 || echo "  $prog failed with file: $file"
                    ;;
                "ls")
                    timeout 2s "$prog" -la "$file" 2>&1 || echo "  $prog failed with file: $file"
                    ;;
                "cat"|"head"|"tail")
                    if [[ -f "$file" ]]; then
                        timeout 2s "$prog" "$file" 2>&1 || echo "  $prog failed with file: $file"
                    fi
                    ;;
                "du")
                    timeout 2s "$prog" "$file" 2>&1 || echo "  $prog failed with file: $file"
                    ;;
                *)
                    echo "  Skipping $prog (needs special handling)"
                    ;;
            esac
        fi
    done
done

# Special test for chmod (the program mentioned in the bug report)
if command -v chmod >/dev/null 2>&1; then
    echo "Special test for chmod with non-UTF-8 filenames..."
    for file in $(ls); do
        if [[ -f "$file" ]]; then
            echo "Testing chmod 644 on: $file"
            timeout 2s chmod 644 "$file" 2>&1 || echo "  chmod failed with file: $file"
        fi
    done
fi

# Test with the uutils implementation if available
if [[ -x "./target/debug/coreutils" ]]; then
    echo "Testing uutils coreutils implementation..."
    for prog in "${PROGRAMS[@]}"; do
        for file in $(ls); do
            if [[ -f "$file" ]]; then
                echo "Testing uutils $prog on: $file"
                timeout 2s ./target/debug/coreutils "$prog" "$file" 2>&1 || echo "  uutils $prog failed with file: $file"
            fi
        done
    done
fi

echo "Test completed. Check output above for any UTF-8 related errors."

# Clean up
cd /
rm -rf "$TEST_DIR"
