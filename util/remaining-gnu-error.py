#!/usr/bin/env python3
# This script lists the GNU failing tests by size
# Just like with util/run-gnu-test.sh, we expect the gnu sources
# to be in ../
import urllib.request

import urllib
import os
import glob
import json
import sys

base = "../gnu/tests/"

# Try to download the file, use local copy if download fails
result_json = "result.json"
try:
    urllib.request.urlretrieve(
        "https://raw.githubusercontent.com/uutils/coreutils-tracking/main/aggregated-result.json",
        result_json,
    )
except Exception as e:
    print(f"Failed to download the file: {e}")
    if not os.path.exists(result_json):
        print(f"Local file '{result_json}' not found. Exiting.")
        sys.exit(1)
    else:
        print(f"Using local file '{result_json}'.")

types = ("/*/*.sh", "/*/*.pl", "/*/*.xpl")

tests = []
error_tests = []
skip_tests = []
missing_from_json = []

for files in types:
    tests.extend(glob.glob(base + files))
# sort by size
list_of_files = sorted(tests, key=lambda x: os.stat(x).st_size)
# Keep track of all tests found on disk
all_disk_tests = set(list_of_files)


def show_list(list_test):
    # Remove the factor tests and reverse the list (bigger first)
    tests = list(filter(lambda k: "factor" not in k, list_test))

    for f in reversed(tests):
        if contains_require_root(f):
            print("%s: %s / require_root" % (f, os.stat(f).st_size))
        else:
            print("%s: %s" % (f, os.stat(f).st_size))
    print("")
    print("%s tests remaining" % len(tests))


def contains_require_root(file_path):
    try:
        with open(file_path, "r") as file:
            return "require_root_" in file.read()
    except IOError:
        return False


with open("result.json", "r") as json_file:
    data = json.load(json_file)

# Keep track of tests we've seen in JSON
tests_in_json = set()

for d in data:
    for e in data[d]:
        # Not all the tests are .sh files, rename them if not.
        script = e.replace(".log", ".sh")
        a = f"{base}{d}/{script}"
        if not os.path.exists(a):
            a = a.replace(".sh", ".pl")
            if not os.path.exists(a):
                a = a.replace(".pl", ".xpl")

        # Track that we've seen this test in JSON
        tests_in_json.add(a)

        # the tests pass, we don't care anymore
        if data[d][e] == "PASS":
            try:
                list_of_files.remove(a)
            except ValueError:
                print("Could not find test '%s'. Maybe update the GNU repo?" % a)
                sys.exit(1)

        # if it is SKIP, show it
        if data[d][e] == "SKIP":
            list_of_files.remove(a)
            skip_tests.append(a)

        # if it is ERROR, show it
        if data[d][e] == "ERROR":
            list_of_files.remove(a)
            error_tests.append(a)

# Find tests that exist on disk but aren't in JSON
missing_from_json = sorted(list(all_disk_tests - tests_in_json), key=lambda x: os.stat(x).st_size)

# Remove missing tests from the fail list
for test in missing_from_json:
    if test in list_of_files:
        list_of_files.remove(test)

print("===============")
print("ERROR: Tests found on disk but missing from result.json:")
show_list(missing_from_json)
print("===============")
print("SKIP tests:")
show_list(skip_tests)
print("")
print("===============")
print("ERROR tests:")
show_list(error_tests)
print("")
print("===============")
print("FAIL tests:")
show_list(list_of_files)
