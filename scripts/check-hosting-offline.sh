#!/usr/bin/env bash
# check-hosting-offline.sh — REQ-XC-329 / R6.25: every hosting-machinery
# acceptance test runs offline on committed fixtures. A live leg —
# one that would run the smoke battery, a podman cell or a phase-VM
# command — must be #[ignore]d and must cite ADR-0363. FAILS when a
# live test lacks the citation.
#
# Mechanism: scan the hosting test batteries for live markers
# (`fn live_*` test names, Command::new("podman"|"docker"|"harbor"),
# non-loopback endpoint literals). A hit requires `#[ignore` and
# `ADR-0363` within a three-line window above it.
# --self-test: run the scan over a seeded violation and require failure.
set -euo pipefail
cd "$(dirname "$0")/.."

FILES="crates/hh-runner/tests/r6_25.rs"

scan() {
    local f="$1" fail=0
    while IFS= read -r line; do
        n=$(echo "$line" | cut -d: -f1)
        ctx=$(sed -n "$((n>3 ? n-3 : 1)),${n}p" "$f")
        case "$ctx" in
            *"#[ignore"*ADR-0363*|*"#[ignore"*"ADR-0363"*) ;;
            *)
                echo "check-hosting-offline: $f:$n — live marker without #[ignore + ADR-0363]"
                echo "  $line"
                fail=1
                ;;
        esac
    done < <(grep -nE 'fn live_|Command::new\("(podman|docker|harbor|ssh|gcloud)"|"https?://[a-zA-Z0-9]' "$f" \
             | grep -vE '127\.0\.0\.1|localhost|replay:|\.local|http://x|fixture\.local')
    return $fail
}

if [ "${1:-}" = "--self-test" ]; then
    seed=$(mktemp)
    trap 'rm -f "$seed"' EXIT
    cat > "$seed" <<'SEED'
#[test]
fn live_smoke_runs_on_the_vm() {}
SEED
    if scan "$seed"; then
        echo "check-hosting-offline --self-test: FAIL — seeded live test was not caught" >&2
        exit 1
    fi
    echo "check-hosting-offline --self-test: PASS — seeded live test caught"
    exit 0
fi

fail=0
for f in $FILES; do
    [ -f "$f" ] || { echo "check-hosting-offline: $f missing" >&2; fail=1; continue; }
    scan "$f" || fail=1
done
if [ "$fail" -ne 0 ]; then
    echo "check-hosting-offline: FAIL — a hosting live test must be #[ignore]d with an ADR-0363 citation" >&2
    exit 1
fi
echo "check-hosting-offline: PASS — all hosting acceptance tests are offline/cited"
