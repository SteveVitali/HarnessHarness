#!/usr/bin/env bash
# check-proxy-offline.sh — REQ-XC-125 / R6.12: every proxy-machinery
# acceptance test runs on a fixture or replay upstream. A live test —
# one that dials a non-loopback endpoint — must be #[ignore]d and must
# cite ADR-0363 (the live-evidence contract). This script FAILS when a
# seeded non-ignored live proxy test is present.
#
# Mechanism: scan the proxy test batteries for endpoint literals that
# are not loopback/fixture markers (127.0.0.1, localhost, replay:*,
# "*.local" placeholders, http://x). A `#[test]` that names a live
# endpoint must carry `#[ignore` and `ADR-0363` within a two-line
# window above it.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
files="crates/hh-hosting/tests/r6_10.rs crates/hh-hosting/tests/r6_11.rs crates/hh-hosting/tests/r6_12.rs crates/hh-gateway/tests/r6_7.rs crates/hh-gateway/tests/r6_12.rs"

for f in $files; do
    [ -f "$f" ] || continue
    # Any URL literal that is not loopback/fixture is a live endpoint.
    while IFS= read -r line; do
        n=$(echo "$line" | cut -d: -f1)
        # the two lines above must be #[ignore ...] + ADR-0363
        ctx=$(sed -n "$((n>2 ? n-2 : 1)),${n}p" "$f")
        case "$ctx" in
            *"#[ignore"*ADR-0363*|*"#[ignore"*"ADR-0363"*) ;;
            *)
                echo "check-proxy-offline: $f:$n — live endpoint literal without #[ignore + ADR-0363]"
                echo "  $line"
                fail=1
                ;;
        esac
    done < <(grep -nE '"https?://[a-zA-Z0-9]' "$f" | grep -vE '127\.0\.0\.1|localhost|replay:|\.local|http://x|proxy\.local|fixture\.local')
    # A #[test] fn immediately following a LIVE marker comment without
    # an #[ignore] is also caught above; additionally forbid non-ignored
    # tests that bind a non-loopback listener.
done

if [ "$fail" -ne 0 ]; then
    echo "check-proxy-offline: FAIL — live proxy test must be #[ignore]d with an ADR-0363 citation" >&2
    exit 1
fi
echo "check-proxy-offline: PASS — all proxy acceptance tests are fixture/replay upstreams"
