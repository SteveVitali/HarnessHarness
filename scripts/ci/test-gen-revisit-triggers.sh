#!/usr/bin/env bash
# test-gen-revisit-triggers.sh — self-test for scripts/gen-revisit-triggers.sh
# (L4.2). Builds throwaway fixture roots (mini ADR set + mini BACKLOG.csv),
# drives the generator under GEN_TRIGGERS_ROOT/GEN_TRIGGERS_OUT, and asserts
# the classification, the unplaced-ADR failure, --check sync/drift, and
# re-run determinism. Also runs --check on the real checkout.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
GEN="$ROOT/scripts/gen-revisit-triggers.sh"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fails=0
say() { echo "test-gen-revisit-triggers: $*"; }
bad() { echo "test-gen-revisit-triggers: FAIL — $*" >&2; fails=$((fails + 1)); }

# fixture root: docs/adr/ADR-000{1,2,3,9}.md + docs/build/BACKLOG.csv
#   ADR-0001 → BL-47 (quiet) · ADR-0002 → BL-48 (fired-answered) ·
#   ADR-0003 → BL-07 (fired-unanswered) · ADR-0009 → unplaced (failure case)
FIX="$TMP/fix"; mkdir -p "$FIX/docs/adr" "$FIX/docs/build"
cat > "$FIX/docs/adr/ADR-0001-first.md" <<'EOF'
# ADR-0001: first

## Revisit trigger

Reopen when the sky falls.
EOF
cat > "$FIX/docs/adr/ADR-0002-second.md" <<'EOF'
# ADR-0002: second

## Revisit trigger

Reopen when a second codec lands.
EOF
cat > "$FIX/docs/adr/ADR-0003-third.md" <<'EOF'
# ADR-0003: third

## Revisit trigger

Reopen when the deferred leg lands.
EOF
cat > "$FIX/docs/adr/ADR-0009-ninth.md" <<'EOF'
# ADR-0009: ninth

## Revisit trigger

Reopen never (unplaced fixture).
EOF
cat > "$FIX/docs/build/BACKLOG.csv" <<'EOF'
bl_id,title,type,sources,req_ids,package,blocks,landing,gate,size,status
BL-07,owner,process,ADR-0003,,process,x,landing,none,S,open
BL-47,quiet,process,ADR-0001,,process,x,landing,none,S,accepted
BL-48,fired,process,ADR-0002,,process,x,landing,none,S,accepted
EOF

OUT="$TMP/out.md"

# 1. unplaced ADR fails generation (never a silent drop)
if GEN_TRIGGERS_ROOT="$FIX" GEN_TRIGGERS_OUT="$OUT" bash "$GEN" >/dev/null 2>&1; then
  bad "unplaced ADR-0009 did not fail generation"
else
  say "ok  unplaced ADR fails"
fi

# 2. place it, regenerate, assert the three classes + verbatim trigger text
sed -i '' 's/^BL-47,quiet,process,ADR-0001/BL-47,quiet,process,ADR-0001; ADR-0009/' "$FIX/docs/build/BACKLOG.csv"
GEN_TRIGGERS_ROOT="$FIX" GEN_TRIGGERS_OUT="$OUT" bash "$GEN" >/dev/null
grep -qF '| ADR-0001 | Reopen when the sky falls. |' "$OUT" \
  && grep -qF '| ADR-0009 | Reopen never (unplaced fixture). |' "$OUT" \
  || bad "quiet-conditional rows missing/incorrect"
grep -qF '| ADR-0002 | Reopen when a second codec lands. |' "$OUT" \
  || bad "fired-answered row missing"
grep -qF '| ADR-0003 | BL-07 | Reopen when the deferred leg lands. |' "$OUT" \
  || bad "fired-unanswered row missing its home column"
say "ok  three classes classified"

# 3. --check in sync / on planted drift
GEN_TRIGGERS_ROOT="$FIX" GEN_TRIGGERS_OUT="$OUT" bash "$GEN" --check >/dev/null \
  && say "ok  --check in sync" || bad "--check failed on an in-sync file"
echo "| planted | drift |" >> "$OUT"
GEN_TRIGGERS_ROOT="$FIX" GEN_TRIGGERS_OUT="$OUT" bash "$GEN" --check >/dev/null 2>&1 \
  && bad "--check passed on planted drift" || say "ok  --check catches drift"

# 4. determinism — regenerate twice, byte-identical
sed -i '' '$d' "$OUT" 2>/dev/null || true
GEN_TRIGGERS_ROOT="$FIX" GEN_TRIGGERS_OUT="$OUT" bash "$GEN" >/dev/null
cp "$OUT" "$TMP/first.md"
GEN_TRIGGERS_ROOT="$FIX" GEN_TRIGGERS_OUT="$OUT" bash "$GEN" >/dev/null
cmp -s "$TMP/first.md" "$OUT" && say "ok  deterministic re-run" || bad "re-run differs"

# 5. real checkout: committed catalogue in sync
bash "$GEN" --check >/dev/null && say "ok  real checkout in sync" \
  || bad "real checkout drifted — run scripts/gen-revisit-triggers.sh"

[ "$fails" -eq 0 ] || { echo "test-gen-revisit-triggers: $fails failure(s)" >&2; exit 1; }
echo "test-gen-revisit-triggers: OK"
