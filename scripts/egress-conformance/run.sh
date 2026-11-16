#!/usr/bin/env bash
# run.sh — the egress-conformance host driver (R6.45k).
#
#   run.sh [--subject <ref|dir>] [--shapes s1,s2,s3,s4,s5] [--freeze <f>]
#          [--out <dir>] [--platform <p>] [--keep]
#
# Builds the SUBJECT tree's hh-egress-mediator + hh-cell-relay inside
# ubuntu:noble for the image arch, renders the phase policies from the
# freeze through `hh-w1 render-policy` (the same code path h-run uses),
# runs the extracted shapes against relay 127.0.0.1:3128, and lands
# verdict.json + the mediator store + logs in --out.
#
# --subject names the code under test: a git ref (archived via
# `git archive`, fetched shallowly if absent) or a source directory.
# Default: HEAD. The policy bytes are rendered by the CHECKOUT's hh_w1
# — the render path (admit_allowlist_mediated + kernel_default(0) +
# phase_schedule) is freeze data, unchanged by the D8–D14 fixes.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
SUBJECT="HEAD"; SHAPES="s1 s2 s3 s4 s5"; FREEZE=""; OUT=""
PLATFORM=""; KEEP=""; IMAGE="hh-egress-conformance:local"
while [ $# -gt 0 ]; do
  case "$1" in
    --subject)  SUBJECT="${2:?}"; shift 2;;
    --shapes)   SHAPES="${2//,/ }"; shift 2;;
    --freeze)   FREEZE="$2"; shift 2;;
    --out)      OUT="$2"; shift 2;;
    --platform) PLATFORM="$2"; shift 2;;
    --image)    IMAGE="$2"; shift 2;;
    --keep)     KEEP=1; shift;;
    *) echo "run.sh: unknown arg $1" >&2; exit 1;;
  esac
done
[ -z "$FREEZE" ] && FREEZE="$(ls -1 "$REPO"/docs/build/reports/*-fp-egress-allowlist-freeze-epoch-*.json | sort | tail -1)"
OUT="${OUT:-$(mktemp -d /tmp/egress-conformance-out.XXXXXX)}"
mkdir -p "$OUT"
CTX="$(mktemp -d /tmp/egress-conformance-ctx.XXXXXX)"
[ -n "$KEEP" ] || trap 'rm -rf "$CTX"' EXIT

say() { printf 'run.sh: %s\n' "$*"; }

# ── subject source → ctx/src ──────────────────────────────────────────
mkdir -p "$CTX/src"
SRC_PATHS="Cargo.toml Cargo.lock rust-toolchain.toml crates"
SUBJECT_LABEL="$SUBJECT"
if [ -d "$SUBJECT" ]; then
  (cd "$SUBJECT" && tar -cf - $SRC_PATHS) | tar -xf - -C "$CTX/src"
  SUBJECT_LABEL="dir:$(cd "$SUBJECT" && git rev-parse HEAD 2>/dev/null || echo local)"
else
  if ! git -C "$REPO" cat-file -e "$SUBJECT^{commit}" 2>/dev/null; then
    say "fetching $SUBJECT"
    git -C "$REPO" fetch -q --depth=1 origin "$SUBJECT" \
      || git -C "$REPO" fetch -q --depth=1 origin "+refs/heads/*:refs/remotes/origin/*"
  fi
  git -C "$REPO" archive "$SUBJECT" $SRC_PATHS | tar -xf - -C "$CTX/src"
  SUBJECT_LABEL="$(git -C "$REPO" rev-parse "$SUBJECT^{commit}")"
fi
say "subject: $SUBJECT_LABEL"

# ── the phase policies (the checkout's hh-w1 — same code path) ────────
W1="${HH_W1:-$REPO/target/debug/hh_w1}"
[ -x "$W1" ] || (cd "$REPO" && cargo build -q -p hharness-runner --bin hh_w1)
mkdir -p "$CTX/policies"
for ph in verify agent; do
  "$W1" render-policy --freeze "$FREEZE" --phase "$ph" --out "$CTX/policies/$ph.json" >/dev/null
done
EPOCH="$(python3 -c "import json;print(json.load(open('$FREEZE'))['epoch'])")"
DIGEST="$(python3 -c "import json;print(json.load(open('$FREEZE'))['digest'])")"
say "freeze epoch=$EPOCH digest=${DIGEST:0:24}… → policies rendered"

cp -r "$REPO/scripts/egress-conformance/derived" "$CTX/derived"
cp "$REPO/scripts/egress-conformance/container-run.sh" \
   "$REPO/scripts/egress-conformance/walcheck.py" \
   "$REPO/scripts/egress-conformance/Dockerfile" "$CTX/"

# ── image + run ───────────────────────────────────────────────────────
PF=""; [ -n "$PLATFORM" ] && PF="--platform $PLATFORM"
# shellcheck disable=SC2086
docker build -q $PF -t "$IMAGE" "$CTX" >&2
rc=0
# shellcheck disable=SC2086
docker run --rm $PF \
  -e "SHAPES=$SHAPES" -e "SUBJECT_REF=$SUBJECT_LABEL" \
  -e "FREEZE_EPOCH=$EPOCH" -e "FREEZE_DIGEST=$DIGEST" -e "OUT=/out" \
  -v "$OUT:/out" "$IMAGE" || rc=$?
say "out: $OUT (rc=$rc)"
[ -f "$OUT/verdict.json" ] && python3 -c "
import json; v=json.load(open('$OUT/verdict.json'))
print(json.dumps({'result':v['result'],'subject':v['subject_ref'][:12],
 'wal':v['wal'],'peak_conns':v['peak_conns'],'failures':v['failures']},indent=1))"
exit "$rc"
