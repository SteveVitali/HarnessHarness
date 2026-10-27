#!/usr/bin/env bash
# check-quickstart.sh — L4.7: the README's hermetic quickstart block,
# executed verbatim as a newcomer would run it, from a clean clone of
# the current commit. AC: `check-quickstart.sh` green locally and in CI.
#
# The script clones HEAD into a temp dir, extracts the marked
# `<!-- quickstart:begin -->`…`<!-- quickstart:end -->` ```sh block from
# the CLONE's README.md (what a newcomer at this commit actually reads),
# and runs it under `bash -eu` — the only substitution is the clone URL
# itself: the public remote becomes the pinned local clone, so the
# block's own `git clone` + `cd HarnessHarness` lines execute verbatim
# against exactly this commit's tree (uncommitted edits are never
# tested — that is the point). Asserts the documented outputs:
# `"connectivity":"ok"` from `hh doctor` and the first hermetic run's
# `lifecycle.run.finished{status:"succeeded", stop_reason:{kind:
# "completed"}}` on the scripted kernel lane.
#
# The live (`quickstart-live`) block is not exercised here — it dials a
# local model endpoint; `scripts/live-verify.sh quickstart-live` owns it
# (ADR-0363). Exit 0 = green; 1 = a leg failed; 2 = malformed block.
set -euo pipefail

REPO="$(git rev-parse --show-toplevel)"
SHA="$(git -C "$REPO" rev-parse HEAD)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/hh-quickstart.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

say()  { printf '%s\n' "check-quickstart: $*"; }
fail() { printf '%s\n' "check-quickstart: FAIL — $*" >&2; exit 1; }

# ── 1. clean clone of the current commit ───────────────────────────────
git clone --quiet --no-hardlinks "$REPO" "$TMP/HarnessHarness" \
  || fail "git clone of $REPO failed"
git -C "$TMP/HarnessHarness" checkout --quiet "$SHA" \
  || fail "checkout of $SHA failed in the clone"

# ── 2. extract the marked hermetic block from the clone's README ───────
awk '
  /<!-- quickstart:begin -->/ { inmark = 1 }
  /<!-- quickstart:end -->/   { inmark = 0 }
  inmark && /^```sh[[:space:]]*$/ { insh = 1; next }
  insh   && /^```[[:space:]]*$/   { insh = 0 }
  insh                          { print }
' "$TMP/HarnessHarness/README.md" > "$TMP/block.sh"
[ -s "$TMP/block.sh" ] || { say 'no quickstart:begin/end fenced-sh block in the clone README.md'; exit 2; }

# The public remote URL → the pinned local clone (the one substitution;
# every other byte of the block executes verbatim). A URL the block does
# not name fails loudly rather than silently skipping the rewrite.
REMOTE_URL="https://github.com/SteveVitali/HarnessHarness.git"
grep -qF "$REMOTE_URL" "$TMP/block.sh" \
  || { say "quickstart block does not clone $REMOTE_URL — recipe drifted"; exit 2; }
sed "s|$REMOTE_URL|$TMP/HarnessHarness|" "$TMP/block.sh" > "$TMP/run.sh"
say "extracted $(wc -l < "$TMP/run.sh" | tr -d ' ') verbatim lines (clone → build → doctor → scripted-lane run)"

# ── 3. execute the block verbatim in a scratch cwd ─────────────────────
mkdir -p "$TMP/home"
set +e
(cd "$TMP/home" && bash -eu "$TMP/run.sh") > "$TMP/out.log" 2>&1
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
  sed 's/^/  /' "$TMP/out.log" >&2
  fail "the quickstart block exited $rc in the clean clone"
fi

# ── 4. assert the documented outputs ───────────────────────────────────
grep -qF '"connectivity":"ok"' "$TMP/out.log" \
  || { sed 's/^/  /' "$TMP/out.log" >&2; fail "hh doctor did not report connectivity: ok"; }
grep -qF '"class":"lifecycle.run.finished"' "$TMP/out.log" \
  || { sed 's/^/  /' "$TMP/out.log" >&2; fail "no lifecycle.run.finished in the run's read"; }
grep -qF '"status":"succeeded"' "$TMP/out.log" \
  || { sed 's/^/  /' "$TMP/out.log" >&2; fail "run did not finish succeeded"; }
grep -qF '"stop_reason":{"kind":"completed"}' "$TMP/out.log" \
  || { sed 's/^/  /' "$TMP/out.log" >&2; fail "run did not stop with kind:completed"; }

say "verbatim quickstart: clone $SHA → cargo build → doctor connectivity:ok → run finished succeeded/completed"
