#!/usr/bin/env bash
# test-check-trace.sh — the fails-without self-test for check-trace.sh:
# mutate a scratch copy and require the checker to refuse each drift.
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/hh-trace-self.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT
pass=0; failn=0
say() { printf '%s\n' "test-check-trace: $*"; }

mut() { # mut <name> — copy the needed files to scratch and mutate
  local d="$SCRATCH/$1"
  mkdir -p "$d/docs" "$d/crates/hh-ledger/src" "$d/scripts/examples"
  for f in docs/inspect-a-run.md docs/experiments.md docs/README.md            crates/hh-ledger/src/classes.rs            docs/build/reports/2026-11-04-r5.43-s1-useful.json            docs/build/reports/2026-11-04-r5.44-ops-live-second.json            scripts/examples/ex-01.json scripts/examples/ex-02.json            scripts/examples/ex-03.json scripts/examples/ex-04.json; do
    mkdir -p "$d/$(dirname "$f")"
    cp "$ROOT/$f" "$d/$f" 2>/dev/null || { say "FAIL copy $f"; failn=$((failn+1)); return; }
  done
  python3 - "$d" "$1" <<'PY'
import sys
d, case = sys.argv[1], sys.argv[2]
def rw(rel, fn):
    p = d + "/" + rel
    s = open(p).read()
    open(p, "w").write(fn(s))
if case == "drop-class":
    rw("docs/inspect-a-run.md", lambda s: s.replace("action.effect.observed", "action.effect.touched"))
elif case == "fake-class":
    rw("docs/inspect-a-run.md", lambda s: s + "\n\naction.effect.decided invented\n")
elif case == "drop-packet-id":
    rw("docs/inspect-a-run.md", lambda s: s.replace("26c9d5afe97c135a36bb544301ccd1d58343db3366a62c98696ba2680e5d395b", "0"*64))
elif case == "drop-family":
    rw("docs/experiments.md", lambda s: s.replace("EX-04", "EX-40"))
elif case == "drop-label":
    rw("docs/experiments.md", lambda s: s.replace("scratch is not sandbox", "sandbox is sandbox"))
elif case == "broken-link":
    rw("docs/inspect-a-run.md", lambda s: s + "\n[nope](no/such/file.md)\n")
elif case == "nav-drift":
    rw("docs/README.md", lambda s: s.replace("inspect-a-run.md", "inspect.md").replace("experiments.md", "experiments-x.md"))
elif case == "card-run-drift":
    rw("docs/experiments.md", lambda s: s.replace("cargo test -p hharness --test r5_45 ex04", "cargo test -p hharness --test r5_45 ex99"))
PY
}

expect_fail() {
  mut "$1"
  if bash scripts/check-trace.sh "$SCRATCH/$1" >/dev/null 2>&1; then
    say "FAIL — mutation $1 passed the checker"; failn=$((failn+1))
  else
    say "ok   — mutation $1 refused"; pass=$((pass+1))
  fi
}
for c in drop-class fake-class drop-packet-id drop-family drop-label broken-link nav-drift card-run-drift; do
  expect_fail "$c"
done

# --head must read committed HEAD, not the working tree
if bash scripts/check-trace.sh --head "$ROOT" >/dev/null 2>&1; then
  say "ok   — --head checks committed HEAD"; pass=$((pass+1))
else
  say "FAIL — --head leg"; failn=$((failn+1))
fi

say "$pass passed / $failn failed"
[ "$failn" -eq 0 ]
