#!/usr/bin/env bash
# test-rename-crates.sh — L4.4's self-test for scripts/rename-crates.sh (the
# ticket's scripted-test AC: "a collision stops the run before any rename —
# covered by a script test with a stubbed `taken` answer").
#
# Builds a throwaway fixture workspace (three `hh-*` packages — a lib, a
# lib+bin collision name, a dep consumer) and drives rename-crates.sh at it
# with HH_RENAME_STUB_FILE answers:
#   a) one 200 (taken)        → exit 3, every file byte-identical
#   b) one non-200/404 (5xx)  → exit 3, every file byte-identical (inconclusive)
#   c) all 404                → exit 0; asserts the rename landed and every
#      protected surface (dirs, [[bin]] names, contract ids, env vars, `hh/`
#      namespace, `_hh/` methods, `hh://` scheme) stayed byte-faithful
#   d) re-run on the renamed tree → "already renamed" no-op exit 0
# Offline-only — the stub replaces the crates.io leg entirely.
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
SCRIPT="$ROOT/scripts/rename-crates.sh"
FAIL=0
say() { printf 'test-rename-crates: %s\n' "$*"; }
fail() { printf 'test-rename-crates: FAIL — %s\n' "$*" >&2; FAIL=1; }

FIX="$(mktemp -d "${TMPDIR:-/tmp}/rename-fix.XXXXXX")"
trap 'rm -rf "$FIX"' EXIT
mkdir -p "$FIX/crates/hh-alpha/src" "$FIX/crates/hh-beta/src" "$FIX/crates/hh-gamma/src" "$FIX/scripts" "$FIX/docs"

cat > "$FIX/Cargo.toml" <<'EOF'
[workspace]
resolver = "2"
members = ["crates/hh-alpha", "crates/hh-beta", "crates/hh-gamma"]
EOF

cat > "$FIX/crates/hh-alpha/Cargo.toml" <<'EOF'
[package]
name = "hh-alpha"
version = "0.0.1"
edition = "2021"
description = "fixture lib — mentions the hh-alpha/1 contract id on purpose"

[lib]
path = "src/lib.rs"
EOF
cat > "$FIX/crates/hh-alpha/src/lib.rs" <<'EOF'
//! `hh-alpha` — fixture. The `hh-alpha/1` contract id stays.
pub const SCHEMA: &str = "hh-alpha/1";
pub fn a() {}
EOF

cat > "$FIX/crates/hh-beta/Cargo.toml" <<'EOF'
[package]
name = "hh-beta"
version = "0.0.1"
edition = "2021"

[lib]
path = "src/lib.rs"

[[bin]]
name = "hh-beta"
path = "src/main.rs"

[features]
tier-c1 = ["hh-alpha/tier"]

[dependencies]
hh-alpha = { path = "../hh-alpha" }
EOF
cat > "$FIX/crates/hh-beta/src/lib.rs" <<'EOF'
//! The hh-beta crate over hh-alpha's codec (`dev.cognition/hir` interim key).
pub const HH_META_KEY: &str = "dev.cognition/hir";
pub fn b() {}
EOF
cat > "$FIX/crates/hh-beta/src/main.rs" <<'EOF'
use hh_alpha;
fn main() {
    let _ = hh_alpha::SCHEMA;
    let _ = hh_beta::HH_META_KEY;
    // binary `hh-beta` keeps its name; the `hh/` namespace + `_hh/` methods +
    // `hh://` scheme + `HH_ENV_VAR` env vars are contract spellings.
    let _ns = "hh";
    let _method = "_hh/account";
    let _uri = "hh://run/x/ledger";
    let _meta_get = m.get("hh");
}
EOF

cat > "$FIX/crates/hh-gamma/Cargo.toml" <<'EOF'
[package]
name = "hh-gamma"
version = "0.0.1"
edition = "2021"

[lib]
path = "src/lib.rs"

[dependencies]
hh-beta = { path = "../hh-beta" }
hh-alpha = { path = "../hh-alpha" }
EOF
cat > "$FIX/crates/hh-gamma/src/lib.rs" <<'EOF'
use hh_alpha;
use hh_beta;
pub fn g() { hh_alpha::a(); hh_beta::b(); }
EOF

cat > "$FIX/scripts/run.sh" <<'EOF'
#!/usr/bin/env bash
cargo build -p hh-alpha --all-targets
cargo test -p hh-beta --test integration
EXT="hh-alpha hh-beta hh-gamma"
XCHECK="${HH_XCHECK_BIN:-$ROOT/target/debug/hh-beta}"
cargo build -p hh-beta --no-default-features
EOF

cat > "$FIX/docs/note.md" <<'EOF'
# fixture doc
The `hh-alpha` crate, `hh-beta` binary, `hh-gamma`; `hh-alpha/1` id; `hh-kernel/0.1.0`.
cargo test -p hh-<crate>
EOF

# Fixture-content checksum — only the payload paths (stub/logs/sums live at the
# fixture root and are intentionally outside this set).
snapshot() { (cd "$FIX" && find Cargo.toml crates scripts docs -type f -print0 | sort -z | xargs -0 shasum -a 256) > "$1"; }

assert_contains() { # file, needle
  grep -qF "$2" "$1" || { fail "$1 lacks expected '$2'"; sed -n '1,12p' "$1" >&2; }
}
assert_lacks() { # file, needle
  if grep -qF "$2" "$1"; then fail "$1 still carries '$2'"; sed -n '1,12p' "$1" >&2; fi
}

# ── leg a: a `taken` answer STOPS before any rename ────────────────────────────
snapshot "$FIX/before.sum"
printf 'hharness\t404\nhharness-alpha\t404\nhharness-beta\t200\nhharness-gamma\t404\n' > "$FIX/stub.tsv"
if HH_RENAME_STUB_FILE="$FIX/stub.tsv" bash "$SCRIPT" --root "$FIX" --no-regen >"$FIX/out-a.log" 2>&1; then
  fail "leg a: script exited 0 on a taken name"; cat "$FIX/out-a.log" >&2
else
  code=$?
  [ "$code" -eq 3 ] || fail "leg a: expected exit 3, got $code"
  grep -q 'STOP' "$FIX/out-a.log" || fail "leg a: no STOP line"; cat "$FIX/out-a.log" >&2
fi
snapshot "$FIX/after-a.sum"
if ! cmp -s "$FIX/before.sum" "$FIX/after-a.sum"; then fail "leg a: files were touched despite the stop"; fi
say "leg a ok — taken name stops the run, zero files touched"

# ── leg b: an inconclusive answer also STOPS ───────────────────────────────────
printf 'hharness\t404\nhharness-alpha\t404\nhharness-beta\t404\nhharness-gamma\t503\n' > "$FIX/stub.tsv"
if HH_RENAME_STUB_FILE="$FIX/stub.tsv" bash "$SCRIPT" --root "$FIX" --no-regen >"$FIX/out-b.log" 2>&1; then
  fail "leg b: script exited 0 on an inconclusive name"
else
  code=$?
  [ "$code" -eq 3 ] || fail "leg b: expected exit 3, got $code"
fi
snapshot "$FIX/after-b.sum"
if ! cmp -s "$FIX/before.sum" "$FIX/after-b.sum"; then fail "leg b: files were touched despite the stop"; fi
say "leg b ok — inconclusive name stops the run"

# ── leg c: all free → the mechanical rename applies ────────────────────────────
printf 'hharness\t404\nhharness-alpha\t404\nhharness-beta\t404\nhharness-gamma\t404\n' > "$FIX/stub.tsv"
HH_RENAME_STUB_FILE="$FIX/stub.tsv" bash "$SCRIPT" --root "$FIX" --no-regen >"$FIX/out-c.log" 2>&1 \
  || { fail "leg c: script failed"; cat "$FIX/out-c.log" >&2; }
grep -q 'proceeding to the mechanical rewrite' "$FIX/out-c.log" || fail "leg c: check-pass line absent"

names="$(cd "$FIX" && cargo metadata --no-deps --format-version 1 --offline 2>/dev/null \
  | python3 -c 'import json,sys; print(",".join(sorted(p["name"] for p in json.load(sys.stdin)["packages"])))')"
[ "$names" = "hharness-alpha,hharness-beta,hharness-gamma" ] \
  || fail "leg c: cargo metadata names = $names"

# renamed: package positions, dep keys, use paths, cargo -p args, quoted lists,
# the dev.cognition key, the _meta member
assert_contains "$FIX/crates/hh-alpha/Cargo.toml" 'name = "hharness-alpha"'
assert_contains "$FIX/crates/hh-beta/Cargo.toml"  'name = "hharness-beta"'
assert_contains "$FIX/crates/hh-beta/Cargo.toml"  'tier-c1 = ["hharness-alpha/tier"]'  # dep side renamed, feature name kept
assert_contains "$FIX/crates/hh-beta/Cargo.toml"  'hharness-alpha = { path = "../hh-alpha" }'
assert_contains "$FIX/crates/hh-gamma/src/lib.rs" 'use hharness_alpha;'
assert_contains "$FIX/crates/hh-beta/src/main.rs" 'hharness_alpha::SCHEMA'
assert_contains "$FIX/scripts/run.sh"           'cargo build -p hharness-alpha --all-targets'
assert_contains "$FIX/scripts/run.sh"           'cargo test -p hharness-beta --test integration'
assert_contains "$FIX/scripts/run.sh"           'EXT="hharness-alpha hharness-beta hharness-gamma"'
assert_contains "$FIX/scripts/run.sh"           'cargo build -p hharness-beta --no-default-features'
assert_contains "$FIX/crates/hh-beta/src/lib.rs" '"hharness/hir"'
assert_contains "$FIX/crates/hh-beta/src/main.rs" '.get("hharness")'
assert_contains "$FIX/docs/note.md"             '`hharness-alpha` crate'
assert_contains "$FIX/docs/note.md"             '`hharness-gamma`'

# protected: directories, [[bin]] names, contract ids, hh/ namespace, _hh/
# methods, hh:// scheme, env vars, product ids — all byte-faithful
[ -d "$FIX/crates/hh-alpha" ] || fail "leg c: crates/hh-alpha directory was renamed"
[ -d "$FIX/crates/hh-beta" ]  || fail "leg c: crates/hh-beta directory was renamed"
assert_contains "$FIX/crates/hh-beta/Cargo.toml"  'path = "../hh-alpha"'       # dir path kept
assert_contains "$FIX/crates/hh-beta/Cargo.toml"  'name = "hh-beta"'           # [[bin]] name kept
assert_contains "$FIX/crates/hh-alpha/src/lib.rs" '"hh-alpha/1"'               # contract id kept
assert_contains "$FIX/crates/hh-beta/src/main.rs" '"hh"'                       # hh/ namespace kept
assert_contains "$FIX/crates/hh-beta/src/main.rs" '"_hh/account"'              # _hh/ methods kept
assert_contains "$FIX/crates/hh-beta/src/main.rs" '"hh://run/x/ledger"'        # hh:// scheme kept
assert_contains "$FIX/scripts/run.sh"           'HH_XCHECK_BIN'                # env var kept
assert_contains "$FIX/scripts/run.sh"           'target/debug/hh-beta'         # bin path kept
assert_contains "$FIX/docs/note.md"             'hh-kernel/0.1.0'              # product id kept
assert_contains "$FIX/docs/note.md"             '`hh-beta` binary'             # binary ref kept (collision name)
assert_contains "$FIX/docs/note.md"             'cargo test -p hh-<crate>'     # placeholder kept
assert_lacks    "$FIX/crates/hh-beta/src/lib.rs"  'dev.cognition'
say "leg c ok — rename applied; every protected surface byte-faithful"

# ── leg d: idempotent — a second run is a clean no-op ──────────────────────────
snapshot "$FIX/after-c.sum"
HH_RENAME_STUB_FILE="$FIX/stub.tsv" bash "$SCRIPT" --root "$FIX" --no-regen >"$FIX/out-d.log" 2>&1 \
  || { fail "leg d: re-run failed"; cat "$FIX/out-d.log" >&2; }
grep -q 'already renamed' "$FIX/out-d.log" || fail "leg d: expected the already-renamed no-op"
snapshot "$FIX/after-d.sum"
if ! cmp -s "$FIX/after-c.sum" "$FIX/after-d.sum"; then fail "leg d: re-run was not a no-op"; fi
say "leg d ok — idempotent no-op"

if [ "$FAIL" -eq 0 ]; then
  say "PASS — 4 legs green"
  exit 0
fi
exit 1
