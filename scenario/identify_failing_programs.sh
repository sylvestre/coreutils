#\!/bin/bash

# Script to identify which uutils coreutils programs fail with non-UTF-8 filenames
# This helps in tracking the progress of fixing Ubuntu bug #2117527

CARGO_TARGET_DIR="target"
BINARY_PATH="$CARGO_TARGET_DIR/debug/coreutils"

# Build the project first to ensure we have the latest binary
echo "Building uutils coreutils..."
cargo build --features unix > /dev/null 2>&1

# Create test directory
TEST_DIR="test_scenario_$$"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

# Create a test file with non-UTF-8 bytes in filename
TEST_FILE=$(python3 -c "import sys; sys.stdout.buffer.write(b'test_file_\xff\xfe.txt')" 2>/dev/null || echo -e 'test_file_\xff\xfe.txt')
echo "test content" > "$TEST_FILE" 2>/dev/null || true

# Programs to test (focusing on file operations)
PROGRAMS=(
    "basename" 
    "cat" 
    "cp"      # needs source and target
    "dirname" 
    "du"      
    "head"    
    "ln"      # needs source and target
    "ls"      
    "mkdir"   # doesn't use existing file
    "mv"      # needs source and target  
    "readlink"
    "realpath"
    "rm"      
    "rmdir"   # different from rm
    "tail"    
    "touch"   # doesn't need existing file
    "unlink"  
)

# Track results
WORKING=()
FAILING=()

echo "Testing each program individually with non-UTF-8 filenames..."

for prog in "${PROGRAMS[@]}"; do
    echo -n "Testing $prog... "
    
    case "$prog" in
        "cp")
            # cp needs source and target
            DEST_FILE=$(python3 -c "import sys; sys.stdout.buffer.write(b'dest_\xff\xfe.txt')" 2>/dev/null || echo -e 'dest_\xff\xfe.txt')
            timeout 5 "../$BINARY_PATH" "$prog" "$TEST_FILE" "$DEST_FILE" >/dev/null 2>&1
            ;;
        "ln")
            # ln needs source and target
            LINK_FILE=$(python3 -c "import sys; sys.stdout.buffer.write(b'link_\xff\xfe.txt')" 2>/dev/null || echo -e 'link_\xff\xfe.txt')
            timeout 5 "../$BINARY_PATH" "$prog" "$TEST_FILE" "$LINK_FILE" >/dev/null 2>&1
            ;;
        "mv")
            # mv needs source and target - create a copy first
            cp "$TEST_FILE" "${TEST_FILE}.bak" 2>/dev/null
            MV_FILE=$(python3 -c "import sys; sys.stdout.buffer.write(b'moved_\xff\xfe.txt')" 2>/dev/null || echo -e 'moved_\xff\xfe.txt')
            timeout 5 "../$BINARY_PATH" "$prog" "${TEST_FILE}.bak" "$MV_FILE" >/dev/null 2>&1
            ;;
        "mkdir")
            # mkdir creates new directory
            DIR_NAME=$(python3 -c "import sys; sys.stdout.buffer.write(b'dir_\xff\xfe')" 2>/dev/null || echo -e 'dir_\xff\xfe')
            timeout 5 "../$BINARY_PATH" "$prog" "$DIR_NAME" >/dev/null 2>&1
            ;;
        "rmdir")
            # rmdir needs an empty directory
            DIR_TO_REMOVE=$(python3 -c "import sys; sys.stdout.buffer.write(b'rmdir_\xff\xfe')" 2>/dev/null || echo -e 'rmdir_\xff\xfe')
            mkdir "$DIR_TO_REMOVE" 2>/dev/null
            timeout 5 "../$BINARY_PATH" "$prog" "$DIR_TO_REMOVE" >/dev/null 2>&1
            ;;
        "touch")
            # touch can create new files
            TOUCH_FILE=$(python3 -c "import sys; sys.stdout.buffer.write(b'touch_\xff\xfe.txt')" 2>/dev/null || echo -e 'touch_\xff\xfe.txt')
            timeout 5 "../$BINARY_PATH" "$prog" "$TOUCH_FILE" >/dev/null 2>&1
            ;;
        "readlink")
            # readlink needs a symlink - create one first
            SYMLINK_FILE=$(python3 -c "import sys; sys.stdout.buffer.write(b'symlink_\xff\xfe.txt')" 2>/dev/null || echo -e 'symlink_\xff\xfe.txt')
            ln -sf "$TEST_FILE" "$SYMLINK_FILE" 2>/dev/null
            if [ -L "$SYMLINK_FILE" ]; then
                timeout 5 "../$BINARY_PATH" "$prog" "$SYMLINK_FILE" >/dev/null 2>&1
            else
                echo "SKIP (needs symlink)"
                continue
            fi
            ;;
        *)
            # Most programs: try with the test file
            timeout 5 "../$BINARY_PATH" "$prog" "$TEST_FILE" >/dev/null 2>&1
            ;;
    esac
    
    if [ $? -eq 0 ]; then
        echo "PASS"
        WORKING+=("$prog")
    else
        echo "FAIL"
        FAILING+=("$prog")
    fi
done

# Clean up
cd ..
rm -rf "$TEST_DIR"

# Print summary
echo
echo "=== SUMMARY ==="
echo "WORKING PROGRAMS: ${WORKING[*]}"
echo "FAILING PROGRAMS: ${FAILING[*]}"
echo
echo "Programs to fix: ${#FAILING[@]}"
echo "Programs already working: ${#WORKING[@]}"

# Save failing programs to file for easy access
printf '%s\n' "${FAILING[@]}" > failing_programs.txt
echo "Failing programs saved to failing_programs.txt"
EOF < /dev/null