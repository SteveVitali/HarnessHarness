#!/usr/bin/env bash
# check-docsrs.sh — L4.5 / ADR-0380 (d): docs.rs readiness. For every
# publishable workspace member, render its docs under exactly the settings
# docs.rs will use — the crate's `[package.metadata.docs.rs]` table
# (`all-features`, `features`, `no-default-features`, `default-features`)
# or plain defaults when absent — via `cargo doc -p <crate> --no-deps`,
# warnings denied. Bin-only publishable crates (no lib target) are skipped
# honestly (docs.rs builds them but renders nothing — no lib docs exist).
#
# Exit 0 when every publishable lib documents clean; non-zero naming the
# failing crates otherwise.
set -euo pipefail
cd "$(dirname "$0")/.."

command -v cargo   >/dev/null || { echo "check-docsrs: cargo absent" >&2; exit 2; }
command -v python3 >/dev/null || { echo "check-docsrs: python3 absent" >&2; exit 2; }

PLAN="$(mktemp "${TMPDIR:-/tmp}/hh-docsrs-plan.XXXXXX")"
trap 'rm -f "$PLAN"' EXIT
python3 - > "$PLAN" <<'PY'
import json, subprocess, sys
try:
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        stderr=subprocess.PIPE))
except subprocess.CalledProcessError as e:
    sys.exit("check-docsrs: cargo metadata failed: " + e.stderr.decode()[:400])
ws = set(meta["workspace_members"])
for p in meta["packages"]:
    if p["id"] not in ws or p.get("publish") in ([], False):
        continue
    if not any("lib" in t["kind"] for t in p["targets"]):
        continue
    d = (p.get("metadata") or {}).get("docs", {}) or {}
    d = d.get("rs", {}) or {}
    args = []
    if d.get("all-features") is True:
        args.append("--all-features")
    else:
        feats = d.get("features")
        if feats:
            args.append("--features=" + ",".join(feats))
    if d.get("no-default-features") is True or d.get("default-features") is False:
        args.append("--no-default-features")
    print(p["name"] + "\t" + " ".join(args))
PY

TOTAL="$(grep -c . "$PLAN" || true)"
echo "check-docsrs: documenting $TOTAL publishable crates under their docs.rs settings"
FAIL=""
while IFS=$'\t' read -r name args; do
  [ -n "$name" ] || continue
  # shellcheck disable=SC2086
  if ! RUSTDOCFLAGS="-D warnings" cargo doc -p "$name" --no-deps $args \
      >"${TMPDIR:-/tmp}/hh-docsrs-$name.log" 2>&1; then
    echo "check-docsrs: FAIL $name (args: ${args:-defaults}) — see ${TMPDIR:-/tmp}/hh-docsrs-$name.log" >&2
    tail -20 "${TMPDIR:-/tmp}/hh-docsrs-$name.log" >&2
    FAIL="$FAIL $name"
  else
    echo "  ok $name (${args:-defaults})"
  fi
done < "$PLAN"
[ -z "$FAIL" ] || { echo "check-docsrs: FAIL —$FAIL" >&2; exit 1; }
echo "check-docsrs: OK — $TOTAL publishable crates document clean under their docs.rs settings"
