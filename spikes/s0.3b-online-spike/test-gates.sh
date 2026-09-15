#!/usr/bin/env bash
# S0.3b — executable gates (fail if the measured behaviour regresses). Exits 0 only if every gate
# passes. Run: bash spikes/s0.3b-online-spike/test-gates.sh
#   G1  cross-candidate hash-chain byte-identity (E1 reference vs E2, E3), + a NEGATIVE test proving
#       the gate fires when the corpus is mutated (so the assert is not vacuous).
#   M-S1-7  MCP echo + ACP session verified through the official SDKs (E2, E3).
#   M-S2-7  cross-ecosystem far-side hash-equality = 100% (E5b, E5c).
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
CORPUS="$HERE/corpus/shared-corpus.json"
REF="$HERE/corpus/reference.json"
PY="$HERE/e2-python/.venv/bin/python"
fail=0
pass() { echo "PASS  $1"; }
die()  { echo "FAIL  $1"; fail=1; }

# 0. Ensure the E1 reference + shared corpus exist (emit if missing).
if [ ! -f "$REF" ] || [ ! -f "$CORPUS" ]; then
  ( cd "$HERE/e1-ref" && cargo build --release >/dev/null 2>&1 && ./target/release/s03b-e1-ref "$CORPUS" "$REF" >/dev/null 2>&1 )
fi

# 1. G1 byte-identity (E2, E3).
for cand in \
  "E2:$PY $HERE/e2-python/e2_kernel.py" \
  "E3:node $HERE/e3-node/e3_kernel.mjs"; do
  name="${cand%%:*}"; cmd="${cand#*:}"
  if $cmd g1 "$CORPUS" "$REF" | grep -q "g1_byte_identical_across_candidates true"; then
    pass "G1 byte-identity $name reproduces the E1 reference head"
  else
    die "G1 byte-identity $name"
  fi
done

# 1b. NEGATIVE G1: a mutated corpus MUST break byte-identity (the gate is not vacuous).
# The g1 command intentionally exits 1 on mismatch, so capture output first (avoid pipefail).
MUT="$(mktemp)"; sed 's/work.step/work.STEP/' "$CORPUS" > "$MUT"
neg_out="$($PY "$HERE/e2-python/e2_kernel.py" g1 "$MUT" "$REF" 2>&1)"; neg_rc=$?
if echo "$neg_out" | grep -q "g1_head_matches_reference false" && [ "$neg_rc" -ne 0 ]; then
  pass "G1 negative: mutated corpus is correctly rejected (gate is live, exit=$neg_rc)"
else
  die "G1 negative: mutated corpus was NOT rejected — gate is vacuous"
fi
rm -f "$MUT"

# 2. M-S1-7 official-SDK round-trips verified.
if $PY "$HERE/e2-python/mcp_roundtrip.py" | grep -q "m_s1_7_mcp_echo_verified true"; then pass "M-S1-7 MCP echo E2"; else die "M-S1-7 MCP echo E2"; fi
if node "$HERE/e3-node/e3_mcp_roundtrip.mjs" | grep -q "m_s1_7_mcp_echo_verified true"; then pass "M-S1-7 MCP echo E3"; else die "M-S1-7 MCP echo E3"; fi
if $PY "$HERE/e2-python/acp_roundtrip.py" 2>/dev/null | grep -q "m_s1_7_acp_session_verified true"; then pass "M-S1-7 ACP session E2"; else die "M-S1-7 ACP session E2"; fi
if node "$HERE/e3-node/e3_acp_roundtrip.mjs" 2>/dev/null | grep -q "m_s1_7_acp_session_verified true"; then pass "M-S1-7 ACP session E3"; else die "M-S1-7 ACP session E3"; fi

# 3. M-S2-7 cross-ecosystem hash-equality = 100%.
if $PY "$HERE/e5b-e5c/e5b_lab_python.py" "$CORPUS" | grep -q "m_s2_7_hash_equal_pct 100.0"; then pass "M-S2-7 E5b hash-equality 100%"; else die "M-S2-7 E5b"; fi
if node "$HERE/e5b-e5c/e5c_surface_node.mjs" "$CORPUS" | grep -q "m_s2_7_hash_equal_pct 100.0"; then pass "M-S2-7 E5c hash-equality 100%"; else die "M-S2-7 E5c"; fi

if [ "$fail" -eq 0 ]; then echo "ALL GATES PASS"; exit 0; else echo "GATES FAILED"; exit 1; fi
