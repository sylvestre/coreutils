#!/bin/bash
# Script to systematically identify programs that fail with non-UTF-8 paths

set -e

echo "Testing each program individually with non-UTF-8 filenames..."

# Create test directory
TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"

# Create files with non-UTF-8 names using GNU tools
/usr/bin/touch "$(printf 'test_\xFF\xFE.txt')" 2>/dev/null || true
/usr/bin/touch "$(printf 'file_\xC0\x80.dat')" 2>/dev/null || true
/usr/bin/mkdir "$(printf 'dir_\xED\xA0\x80')" 2>/dev/null || true

PROGRAMS=(
    "basename" "cat" "cp" "dirname" "du" "head" "ln"
    "ls" "mkdir" "mv" "readlink" "realpath" "rm" "rmdir"
    "tail" "touch" "unlink"
)

BINARY="/home/sylvestre/dev/debian/coreutils.disable-loca/target/debug/coreutils"

FAILING_PROGRAMS=()
WORKING_PROGRAMS=()

for prog in "${PROGRAMS[@]}"; do
    echo -n "Testing $prog... "

    case "$prog" in
        "basename"|"dirname"|"realpath")
            # These need a path argument
            if $BINARY $prog "$(printf 'test_\xFF\xFE.txt')" >/dev/null 2>&1; then
                echo "PASS"
                WORKING_PROGRAMS+=("$prog")
            else
                echo "FAIL"
                FAILING_PROGRAMS+=("$prog")
            fi
            ;;
        "cat"|"head"|"tail")
            # These work with files
            if $BINARY $prog "$(printf 'test_\xFF\xFE.txt')" >/dev/null 2>&1; then
                echo "PASS"
                WORKING_PROGRAMS+=("$prog")
            else
                echo "FAIL"
                FAILING_PROGRAMS+=("$prog")
            fi
            ;;
        "ls")
            # ls can list files or directories
            if $BINARY $prog "$(printf 'test_\xFF\xFE.txt')" >/dev/null 2>&1; then
                echo "PASS"
                WORKING_PROGRAMS+=("$prog")
            else
                echo "FAIL"
                FAILING_PROGRAMS+=("$prog")
            fi
            ;;
        "du")
            # du works with files or directories
            if $BINARY $prog "$(printf 'test_\xFF\xFE.txt')" >/dev/null 2>&1; then
                echo "PASS"
                WORKING_PROGRAMS+=("$prog")
            else
                echo "FAIL"
                FAILING_PROGRAMS+=("$prog")
            fi
            ;;
        "mkdir")
            # mkdir creates directories
            if $BINARY $prog "$(printf 'newdir_\xFF\xFE')" >/dev/null 2>&1; then
                echo "PASS"
                WORKING_PROGRAMS+=("$prog")
            else
                echo "FAIL"
                FAILING_PROGRAMS+=("$prog")
            fi
            ;;
        "touch")
            # touch creates files
            if $BINARY $prog "$(printf 'newfile_\xFF\xFE.txt')" >/dev/null 2>&1; then
                echo "PASS"
                WORKING_PROGRAMS+=("$prog")
            else
                echo "FAIL"
                FAILING_PROGRAMS+=("$prog")
            fi
            ;;
        "rm"|"unlink")
            # rm/unlink remove files
            if $BINARY $prog "$(printf 'test_\xFF\xFE.txt')" >/dev/null 2>&1; then
                echo "PASS"
                WORKING_PROGRAMS+=("$prog")
            else
                echo "FAIL"
                FAILING_PROGRAMS+=("$prog")
            fi
            ;;
        "rmdir")
            # rmdir removes directories
            if $BINARY $prog "$(printf 'dir_\xED\xA0\x80')" >/dev/null 2>&1; then
                echo "PASS"
                WORKING_PROGRAMS+=("$prog")
            else
                echo "FAIL"
                FAILING_PROGRAMS+=("$prog")
            fi
            ;;
        "mv"|"cp"|"ln")
            # These need source and destination
            if $BINARY $prog "$(printf 'test_\xFF\xFE.txt')" "$(printf 'dest_\xC0\x80.txt')" >/dev/null 2>&1; then
                echo "PASS"
                WORKING_PROGRAMS+=("$prog")
            else
                echo "FAIL"
                FAILING_PROGRAMS+=("$prog")
            fi
            ;;
        "readlink")
            # readlink needs a symlink - skip for now
            echo "SKIP (needs symlink)"
            continue
            ;;
        *)
            echo "SKIP (unknown test method)"
            continue
            ;;
    esac
done

echo
echo "=== SUMMARY ==="
echo "WORKING PROGRAMS: ${WORKING_PROGRAMS[*]}"
echo "FAILING PROGRAMS: ${FAILING_PROGRAMS[*]}"
echo
echo "Programs to fix: ${#FAILING_PROGRAMS[@]}"
echo "Programs already working: ${#WORKING_PROGRAMS[@]}"

# Clean up
cd /
rm -rf "$TEST_DIR"

# Save failing programs to a file
printf '%s\n' "${FAILING_PROGRAMS[@]}" > /home/sylvestre/dev/debian/coreutils.disable-loca/failing_programs.txt
echo "Failing programs saved to failing_programs.txt"
