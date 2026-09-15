#!/usr/bin/env bash
# S0.3 S1/S2 measurement-spike runner (ADR-0050 R1–R6; §10.6; l1-spike-spec §3–§6).
#
# Offline/hermetic (operator gate: Stage-0 spike budget, no external spend). Runs the throwaway
# S1 kernel-slice and S2 boundary-slice spikes for the **E1** candidate / **E5a** split's crossing
# mechanism, repeat-scored N≥3 (l1-spike-spec R3: "three repetitions, report median and p95"), and
# emits the aggregated distribution (median · min · max over the repeats) plus the correctness
# gates G1–G4. The formatted measurement sheet + the ADR-0050 amendment-log append are authored
# from this output. Usage: `bash scripts/spike-s0.3-measurement-sheet.sh [REPS]` (default REPS=5).
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
REPS="${1:-5}"
RAW="$(mktemp)"
trap 'rm -f "$RAW"' EXIT

echo "s0.3-spike: building release kernel + codegen + spikes…" >&2
cargo build -q --release -p hh-kernel -p hh-codegen
cargo build -q --release --manifest-path spikes/s1-kernel-spike/Cargo.toml
cargo build -q --release --manifest-path spikes/s2-boundary-spike/Cargo.toml

KERNEL="$ROOT/target/release/hh-kernel"
CODEGEN="$ROOT/target/release/hh-codegen"
S1="$ROOT/spikes/s1-kernel-spike/target/release/s1-kernel-spike"
S1_HELPER="$ROOT/spikes/s1-kernel-spike/target/release/s1-helper"
S2="$ROOT/spikes/s2-boundary-spike/target/release/s2-boundary-spike"

# --- Gates G1–G4: the deterministic unit tests in the S1 spike lib ---------------------------
echo "s0.3-spike: running gates G1–G4 (cargo test)…" >&2
if cargo test -q --release --manifest-path spikes/s1-kernel-spike/Cargo.toml >/tmp/s0.3-gates.log 2>&1; then
  GATES="pass"
else
  GATES="FAIL"
fi
echo "gates_g1_g4 $GATES" >>"$RAW"

# --- N repeats of both spikes ----------------------------------------------------------------
echo "s0.3-spike: running $REPS repeats of S1 + S2 spikes…" >&2
for i in $(seq 1 "$REPS"); do
  echo "  repeat $i/$REPS" >&2
  "$S1" "$S1_HELPER" >>"$RAW"
  "$S2" "$KERNEL" "$CODEGEN" >>"$RAW"
done

HOST="$(uname -sm)"
CPU="$(sysctl -n hw.model 2>/dev/null || echo unknown) $(sysctl -n hw.ncpu 2>/dev/null || nproc) cores"
MEM="$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1073741824 )) GiB"
OSV="$(sw_vers -productVersion 2>/dev/null || uname -r)"
COMMIT="$(git rev-parse --short HEAD)"
DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# --- Aggregate: per key, median · min · max over the numeric repeats; pass-through others -----
echo "# S0.3 aggregated spike measurements ($REPS repeats)"
echo "# host: $HOST | $CPU | $MEM | macOS $OSV | commit $COMMIT | $DATE"
echo "gates_g1_g4 $GATES"
awk '
  # collect
  { key=$1; val=$2;
    if (val ~ /^-?[0-9]+(\.[0-9]+)?$/) { n[key]++; v[key","n[key]]=val+0 }
    else { pass[key]=val }
  }
  function median(k,   c,i,arr,tmp) {
    c=n[k]; for(i=1;i<=c;i++) arr[i]=v[k","i];
    # insertion sort
    for(i=2;i<=c;i++){ x=arr[i]; j=i-1; while(j>=1 && arr[j]>x){arr[j+1]=arr[j];j--} arr[j+1]=x }
    if(c%2==1) return arr[int(c/2)+1]; else return (arr[c/2]+arr[c/2+1])/2.0
  }
  function mn(k,  c,i,m){ c=n[k]; m=v[k",1"]; for(i=2;i<=c;i++) if(v[k","i]<m) m=v[k","i]; return m }
  function mx(k,  c,i,m){ c=n[k]; m=v[k",1"]; for(i=2;i<=c;i++) if(v[k","i]>m) m=v[k","i]; return m }
  END{
    for(k in n) printf "%s median=%.4g min=%.4g max=%.4g reps=%d\n", k, median(k), mn(k), mx(k), n[k]
    for(k in pass) printf "%s %s (all reps)\n", k, pass[k]
  }
' "$RAW" | sort
