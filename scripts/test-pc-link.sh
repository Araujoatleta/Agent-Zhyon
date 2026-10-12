#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/coucou-pc-link.XXXXXX")"
trap 'rm -rf "$TEST_DIR"' EXIT
swiftc NotchBuddy/Sources/Phone/PCLinkWire.swift \
    tests/PCLinkWireTests.swift -o "$TEST_DIR/pc-link-tests"
"$TEST_DIR/pc-link-tests"
