#!/bin/bash
#
# Run all tests that "cargo test" knows and list ones which fail to
# remove the temporary files they create. This is done by running one
# test at a time, and creating an empty directory to use as TMPDIR. If
# the directory contains any files after the test has finished, the
# test didn't remove them properly.

set -euo pipefail

# List all test "cargo test" knows.
tests() {
	cargo test -q --lib -- --list | sed '/: test$/s///'
}

# Delete contents of a directory, but not the directory itself.
cleanup() {
	find "$1" -mindepth 1 -delete
}

# Is a directory empty?
is_empty() {
	if find "$1" -mindepth 1 | grep . >/dev/null; then
		return 1
	else
		return 0
	fi
}

# Create an empty directory and clean it up after the script ends.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
echo "TMPDIR: $tmp"

# Run each test with an empty TMPDIR, check its empty after the test
# has finished.
tests | while read x; do
	cleanup "$tmp"
	env TMPDIR="$tmp" chronic cargo test -q -- "$x"
	if ! is_empty "$tmp"; then
		echo "failed to clean up its temporary files: $x"
		tree -a "$tmp"
		echo
	fi
done
