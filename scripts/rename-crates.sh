#!/usr/bin/env bash
# rename-crates.sh — L4.4's scripted crate rename: every publishable package
# `hh-<component>` → `hharness-<component>` per the 2026-10-20 naming ruling and
# the ADR-0210 allocation record. Deterministic and idempotent.
#
# Phase order is contractual (the ticket's MAJOR-1(a)):
#   1. BEFORE any file is touched — a crates.io availability check over the exact
#      publish set derived from `cargo metadata --no-deps` (every `hh-<x>` →
#      `hharness-<x>`, plus the umbrella `hharness`): one
#      `GET https://crates.io/api/v1/crates/<name>` per name, ≥ 1 s apart,
#      descriptive User-Agent; 404 = free, 200 = taken, anything else =
#      inconclusive. ANY taken or inconclusive name STOPS the run with a
#      non-zero exit — no partial rename, the operator is asked.
#   2. The mechanical rewrite — manifests, dependency keys, `use`/path
#      references, scripts, workflows, codegen inputs, and the OQ-068 `_meta`
#      code-constant conform (`dev.cognition/*` keys → `hharness/…`, the ACP
#      `_meta.hh` vendor member → `_meta.hharness`).
#   3. Regeneration — Cargo.lock, codegen artifacts, THIRD_PARTY_NOTICES, fmt.
#
# Rename mechanics (ADR-0379): directories keep `crates/hh-*` (provenance);
# binary names unchanged (`hh`, `hh-kernel`, `hh-helper`, …); env vars `HH_*`
# and wire/schema/product ids (`hh-embed/1`, `hh-kernel/<ver>`, `hh-json/1`, …)
# unchanged — contract names, not crate names. Library names default to the
# package name (`hh_wire` → `hharness_wire`), so `use` paths are rewritten.
#
# Preserved by construction (the boundary rules): contract ids `hh-*/<digit>`,
# `crates/hh-*`/`plugins/hh-*` paths, `hh-*` tokens glued to a longer name
# (`hh-embed-cap2`, `hh-mcp-serve`, `hh-embed-1.schema.json`, `hh-embed.ts`),
# the `hh/` registry namespace, `_hh/` methods, the `hh://` scheme, `hh` on the
# W3C tracestate member, `hh.*` OTLP/baggage attributes, and the protected
# trees docs/build, docs/tickets, docs/adr, research/, spec/, spikes/ (append-only
# build records + frozen spec + throwaway measurement trees — never rewritten).
#
# Usage:
#   rename-crates.sh [--root DIR] [--check-only] [--no-check] [--no-regen]
#     --check-only   run the availability check only (nothing rewritten)
#     --no-check     skip the availability check (documented re-runs/tests only —
#                    the ticket's default path always checks first)
#     --no-regen     stop after the rewrite (fixture roots without the codegen
#                    binary; the real run always regenerates)
# Env:
#   HH_RENAME_STUB_FILE — when set, the availability check reads
#     `name<TAB>http-status` lines from this file instead of the network
#     (the script test's stubbed-answers seam; still emits dated per-name lines).
# Exit: 0 ok · 2 usage/tool error · 3 availability check found a taken or
# inconclusive name — STOP, nothing was renamed.
set -euo pipefail

ROOT=""
CHECK_ONLY=0
NO_CHECK=0
NO_REGEN=0
while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="$2"; shift 2 ;;
    --check-only) CHECK_ONLY=1; shift ;;
    --no-check) NO_CHECK=1; shift ;;
    --no-regen) NO_REGEN=1; shift ;;
    -h|--help) sed -n '1,40p' "$0"; exit 0 ;;
    *) echo "rename-crates: unknown arg $1" >&2; exit 2 ;;
  esac
done
if [ -z "$ROOT" ]; then
  ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
fi
[ -n "$ROOT" ] || { echo "rename-crates: no --root and not inside a git work tree" >&2; exit 2; }
cd "$ROOT"
command -v cargo   >/dev/null || { echo "rename-crates: cargo absent" >&2; exit 2; }
command -v python3 >/dev/null || { echo "rename-crates: python3 absent" >&2; exit 2; }
command -v curl    >/dev/null || { echo "rename-crates: curl absent" >&2; exit 2; }

# ── Phase 0: derive the rename map ─────────────────────────────────────────────
MAP_FILE="$(mktemp "${TMPDIR:-/tmp}/hh-rename-map.XXXXXX")"
META_FILE="$(mktemp "${TMPDIR:-/tmp}/hh-rename-meta.XXXXXX")"
trap 'rm -f "$MAP_FILE" "$META_FILE"' EXIT
cargo metadata --no-deps --format-version 1 --manifest-path "$ROOT/Cargo.toml" --offline >"$META_FILE" 2>/dev/null \
  || { echo "rename-crates: cargo metadata failed" >&2; exit 2; }
python3 - "$META_FILE" > "$MAP_FILE" <<'PY'
import json, sys
m = json.load(open(sys.argv[1]))
pkgs, libs, collisions, bad = {}, {}, {}, []
for p in m["packages"]:
    name = p["name"]
    if name.startswith("hharness-"):
        continue  # idempotent: already renamed
    if not name.startswith("hh-"):
        bad.append(name); continue
    if p.get("publish") in ([], False):
        # The ticket set = publishable packages; a publish=false member is not
        # renamed and is not expected at L4.4 (L4.5 sets them later).
        bad.append(name + " (publish=false)"); continue
    pkgs[name] = "hharness-" + name[3:]
    for t in p["targets"]:
        if "lib" in t["kind"]:
            libs[t["name"]] = "hharness_" + t["name"][3:]
        if "bin" in t["kind"] and t["name"] == name:
            collisions[name] = True
if bad:
    print("rename-crates: unexpected package names (not hh-*): " + ", ".join(bad), file=sys.stderr)
    sys.exit(2)
print(json.dumps({"pkgs": pkgs, "libs": libs, "collisions": sorted(collisions)}))
PY
[ -s "$MAP_FILE" ] || { echo "rename-crates: metadata derive produced no map" >&2; exit 2; }
N_PKGS="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["pkgs"]))' "$MAP_FILE")"
if [ "$N_PKGS" -eq 0 ]; then
  echo "rename-crates: no hh-* packages remain — already renamed, nothing to do."
  exit 0
fi

# ── Phase 1: the crates.io availability check — BEFORE any file is touched ────
if [ "$NO_CHECK" -eq 0 ]; then
  NAMES="$(python3 -c 'import json,sys
print("\n".join(sorted(["hharness"] + list(json.load(open(sys.argv[1]))["pkgs"].values()))))' "$MAP_FILE")"
  TOTAL="$(printf '%s\n' "$NAMES" | grep -c .)"
  echo "rename-crates: crates.io availability check — $TOTAL names (umbrella + $N_PKGS publishable packages)"
  FAIL=""
  i=0
  while IFS= read -r name; do
    [ -n "$name" ] || continue
    i=$((i + 1))
    ts="$(date -u +%FT%TZ)"
    if [ -n "${HH_RENAME_STUB_FILE:-}" ]; then
      code="$(awk -F'\t' -v n="$name" '$1==n{print $2}' "$HH_RENAME_STUB_FILE" | head -1)"
      [ -n "$code" ] || code="stub-missing"
    else
      code="$(curl -sS -o /dev/null -w '%{http_code}' \
        -A 'HarnessHarness-crate-rename (L4.4 scripted availability check; https://github.com/SteveVitali/HarnessHarness)' \
        "https://crates.io/api/v1/crates/$name" 2>/dev/null || echo curl-error)"
      # ≥ 1 s between requests (crates.io API courtesy).
      [ "$i" -lt "$TOTAL" ] && sleep 1.2
    fi
    case "$code" in
      404) verdict=free ;;
      200) verdict=TAKEN; FAIL="$FAIL $name(200)" ;;
      *)   verdict=inconclusive; FAIL="$FAIL $name($code)" ;;
    esac
    printf '%s  %-36s  %-12s  %s\n' "$ts" "$name" "$code" "$verdict"
  done <<< "$NAMES"
  if [ -n "$FAIL" ]; then
    {
      echo "rename-crates: STOP — crates.io names taken or inconclusive:$FAIL"
      echo "  A collision asks the operator before anything is renamed (L4.4 contract):"
      echo "  pick a different prefix or clear the holder; no partial rename was attempted."
    } >&2
    exit 3
  fi
  echo "rename-crates: all $TOTAL names free — proceeding to the mechanical rewrite."
fi
[ "$CHECK_ONLY" -eq 1 ] && exit 0

# ── Phase 2: the mechanical rewrite ────────────────────────────────────────────
HH_ROOT="$ROOT" HH_MAP_FILE="$MAP_FILE" python3 - <<'PY'
import json, os, re, subprocess, sys

m = json.load(open(os.environ["HH_MAP_FILE"]))
pkgs = m["pkgs"]                 # hh-x -> hharness-x
libs = m["libs"]                 # hh_x -> hharness_x
collisions = set(m["collisions"])  # package name == a kept binary name
ROOT = os.environ["HH_ROOT"]

# Trees that are never rewritten: append-only build records, the frozen spec,
# throwaway spike trees, and this script + its test (self-protection).
SKIP_DIRS = ("docs/build/", "docs/tickets/", "docs/adr/", "research/", "spec/",
             "spikes/", "target/", ".git/")
SKIP_FILES = {
    "scripts/rename-crates.sh", "scripts/ci/test-rename-crates.sh",
    # regenerated, not swept:
    "Cargo.lock", "THIRD_PARTY_NOTICES.md",
    "crates/hh-embed-client-generated/src/generated.rs",
    "clients/ts/hh-embed.ts",
}

def files():
    try:
        out = subprocess.run(["git", "-C", ROOT, "ls-files"],
                             capture_output=True, text=True, check=True).stdout
        for f in out.splitlines():
            yield f
    except Exception:
        for dp, dns, fns in os.walk(ROOT):
            for d in list(dns):
                if d in (".git", "target"):
                    dns.remove(d)
            for fn in fns:
                yield os.path.relpath(os.path.join(dp, fn), ROOT)

LB = r"(?<![A-Za-z0-9_/.:-])"       # left boundary: not identifier/path/kebab/dot/colon glued
RB = r"(?![A-Za-z0-9_/\-.:])"       # right boundary: same + `/`, `.`, `:` blocked
# `:` is a wire-spelling boundary: `hh-bundle:<ver>` tags, `hh-embed:fleet`
# holder ids, `otel.service:hh-bundle`, `<bin>: <log>` prefixes are contract or
# binary spellings — never crate names. `.` likewise: `hh.*` attributes and
# filenames like `b.hh-bundle`.
ESC = {o: re.escape(o) for o in pkgs}
noncol = sorted((o for o in pkgs if o not in collisions), key=len, reverse=True)
col = sorted(collisions, key=len, reverse=True)
NONCOL_RE = re.compile(LB + "(" + "|".join(ESC[o] for o in noncol) + ")" + RB) if noncol else None
COL_P_RE = re.compile(r"((?:-p|--package)[=\s]|\b-p=)" + "(" + "|".join(ESC[o] for o in col) + ")" + RB) if col else None
COL_Q_RE = re.compile(r"(['\"])(" + "|".join(ESC[o] for o in col) + r")\1") if col else None
CHAIN_RE = re.compile(LB + r"(hh-[a-z0-9-]+(?:/hh-[a-z0-9-]+)+)")
# `..hh_x::…` is struct-update spread (a crate path); `a.hh_x`/`x/hh_y.rs` are
# attribute/file spellings — block a single dot, allow the `..` spread.
LLB = r"(?<![A-Za-z0-9_/])(?:(?<!\.)|(?<=\.\.))"
LRB = r"(?![A-Za-z0-9_.])"           # lib-name right boundary (file stems `hh_x.rs` preserved)
LIB_RE = re.compile(LLB + "(" + "|".join(sorted((re.escape(l) for l in libs), key=len, reverse=True)) + ")" + LRB) if libs else None
QUOTED_LIST_RE = re.compile(r"(['\"])([a-z0-9_\- ]+)\1")
# Path-context protections: directories keep `crates/hh-*`/`plugins/hh-*` and
# bin/src files keep `hh_*` basenames. `/`, `.` and `:` are already token
# boundaries; what remains is the *exact-quoted* directory basename inside a
# path-API call (`.join("hh-x")`, `Path::new("hh-x")`, …) and `realm=` wire
# values — never crate-name references.
DIRSET = set()
for _base in ("crates", "plugins"):
    _bp = os.path.join(ROOT, _base)
    if os.path.isdir(_bp):
        for _d in os.scandir(_bp):
            if _d.is_dir() and _d.name.startswith("hh-"):
                DIRSET.add(_d.name)
if DIRSET:
    PATHARG_RE = re.compile(
        r"((?:join|Path::new|Path::from|PathBuf::from|component|file_name|push|"
        r"exists|canonicalize|read_dir|read_to_string|read|metadata|"
        r"create_dir_all|create_dir|strip_prefix|set_current_dir)\s*\(\s*[\"'])("
        + "|".join(sorted((re.escape(d) for d in DIRSET), key=len, reverse=True))
        + r")([\"'])")
else:
    PATHARG_RE = None
REALM_RE = re.compile(r"(realm=\\?[\"'])(" + "|".join(ESC[o] for o in sorted(pkgs, key=len, reverse=True)) + r")(\\?[\"'])")

def _hold(value, kept):
    """Stash a protected fragment; returns its token index."""
    kept.append(value)
    return len(kept) - 1

DEP_SECTIONS = ("dependencies", "dev-dependencies", "build-dependencies",
                "workspace.dependencies")
KEEP_SECTIONS = ("bin", "lib", "test", "bench", "example")

def rename_toml(text):
    """Section-aware pass for *.toml: [package] name, dep keys, package=,
    [features] dep:/pkg-feat refs; [[bin]]/[lib]/[[test]] names are protected."""
    lines = text.split("\n")
    out, section, kept = [], None, []
    for i, ln in enumerate(lines):
        sm = re.match(r"^\s*\[\[?([A-Za-z0-9_.-]+)\]?\]?", ln)
        if sm:
            section = sm.group(1)
        if section == "package":
            ln = re.sub(r'(\bname\s*=\s*")(' + "|".join(ESC[o] for o in pkgs) + r')(")',
                        lambda g: g.group(1) + pkgs[g.group(2)] + g.group(3), ln)
        elif section in KEEP_SECTIONS:
            # Binary/lib/test names are contract names — protect the value.
            def keep(g):
                kept.append(g.group(2))
                return g.group(1) + "\x00KEEPTOK%d\x00" % (len(kept) - 1) + g.group(3)
            ln = re.sub(r'(\bname\s*=\s*")([^"]+)(")', keep, ln)
        elif section in DEP_SECTIONS or (section or "").endswith(".dependencies"):
            ln = re.sub(r"^(\s*)(" + "|".join(ESC[o] for o in pkgs) + r")(\s*=)",
                        lambda g: g.group(1) + pkgs[g.group(2)] + g.group(3), ln)
            ln = re.sub(r'(package\s*=\s*")(' + "|".join(ESC[o] for o in pkgs) + r')(")',
                        lambda g: g.group(1) + pkgs[g.group(2)] + g.group(3), ln)
        elif section == "features":
            for o in pkgs:
                ln = ln.replace("dep:" + o, "dep:" + pkgs[o])
                ln = ln.replace('"' + o + "/", '"' + pkgs[o] + "/")
                ln = ln.replace('"' + o + '"', '"' + pkgs[o] + '"')
            ln = re.sub(r"^(\s*)(" + "|".join(ESC[o] for o in pkgs) + r")(\s*=)",
                        lambda g: g.group(1) + pkgs[g.group(2)] + g.group(3), ln)
        out.append(ln)
    return "\n".join(out), kept

def conform_meta(text):
    """The OQ-068 code-constant conform (ADR-0210 allocation record):
    interim `dev.cognition/*` `_meta` keys → `hharness/…`; the ACP `_meta`
    vendor member `hh` → `hharness` (inner members unchanged)."""
    text = text.replace("dev.cognition/hir", "hharness/hir")
    text = text.replace("dev.cognition/propagation", "hharness/propagation")
    text = text.replace("dev.cognition/input_required", "hharness/input_required")
    text = re.sub(r"_meta\.hh(?![A-Za-z_])", "_meta.hharness", text)
    text = text.replace('.get("hh")', '.get("hharness")')
    text = text.replace('.get_mut("hh")', '.get_mut("hharness")')
    text = text.replace('.insert("hh".to_string()', '.insert("hharness".to_string()')
    # The `_meta` member key tuples: `("hh", Json::…)` / multiline `"hh",\n Json::obj(`.
    text = re.sub(r'\("hh",\s*Json::', '("hharness", Json::', text)
    text = re.sub(r'(\n\s*)"hh",(\s*\n\s*Json::obj)', r'\1"hharness",\2', text)
    # Producer/accessor names that describe the wire member.
    text = text.replace("acp_hh_members", "acp_hharness_members")
    return text

def sweep(path, text):
    is_toml = path.endswith(".toml")
    is_scriptish = path.endswith((".sh", ".py", ".yml", ".yaml", ".allow"))
    kept = []
    if is_toml:
        text, kept = rename_toml(text)
    # `_meta` conform + slash-chains of package names first.
    text = conform_meta(text)
    def chains(g):
        parts = g.group(1).split("/")
        if all(p in pkgs for p in parts):
            return "/".join(pkgs[p] for p in parts)
        return g.group(1)
    text = CHAIN_RE.sub(chains, text)
    # Protect path/wire spellings (directory basenames inside path-API calls,
    # `realm=` values) before the token rules.
    if PATHARG_RE:
        text = PATHARG_RE.sub(lambda g: g.group(1) + "\x00KEEPTOK%d\x00" % _hold(g.group(2), kept) + g.group(3), text)
    text = REALM_RE.sub(lambda g: g.group(1) + "\x00KEEPTOK%d\x00" % _hold(g.group(2), kept) + g.group(3), text)
    # Non-collision package names — plain token rewrite.
    if NONCOL_RE:
        text = NONCOL_RE.sub(lambda g: pkgs[g.group(1)], text)
    # Collision names (package name == kept binary name): only unambiguous
    # package-reference positions — cargo -p/--package args, and exact-quoted
    # strings in scripts/YAML (name compares, package lists).
    if col:
        if COL_P_RE:
            text = COL_P_RE.sub(lambda g: g.group(1) + pkgs[g.group(2)], text)
        if is_scriptish and COL_Q_RE:
            text = COL_Q_RE.sub(lambda g: g.group(1) + pkgs[g.group(2)] + g.group(1), text)
        def qlist(g):
            toks = g.group(2).split()
            if toks and all(t in pkgs or t.startswith("hharness-") for t in toks):
                return g.group(1) + " ".join(pkgs.get(t, t) for t in toks) + g.group(1)
            return g.group(0)
        if is_scriptish:
            text = QUOTED_LIST_RE.sub(qlist, text)
    # Library `use` paths last (independent of the hyphen rules).
    if LIB_RE:
        text = LIB_RE.sub(lambda g: libs[g.group(1)], text)
    # Restore protected [[bin]]/[lib] names.
    for i, v in enumerate(kept):
        text = text.replace("\x00KEEPTOK%d\x00" % i, v)
    return text

changed = []
for rel in sorted(files()):
    if rel in SKIP_FILES or any(rel.startswith(d) for d in SKIP_DIRS):
        continue
    p = os.path.join(ROOT, rel)
    try:
        raw = open(p, "rb").read()
    except OSError:
        continue
    if b"\x00" in raw:
        continue  # binary fixture — never swept
    text = raw.decode("utf-8", "surrogateescape")
    new = sweep(rel, text)
    if new != text:
        open(p, "wb").write(new.encode("utf-8", "surrogateescape"))
        changed.append(rel)
print("rename-crates: rewrote %d files" % len(changed))
for c in changed:
    print("  " + c)
PY

# ── Phase 3: regeneration ──────────────────────────────────────────────────────
if [ "$NO_REGEN" -eq 1 ]; then
  echo "rename-crates: --no-regen — regeneration skipped (fixture mode)."
  exit 0
fi
echo "rename-crates: refreshing Cargo.lock via metadata resolution…"
cargo metadata --format-version 1 --manifest-path "$ROOT/Cargo.toml" --offline -q >/dev/null
echo "rename-crates: regenerating schema + client artifacts (check-drift codegen leg)…"
cargo run -q -p hharness-codegen --manifest-path "$ROOT/Cargo.toml" -- --root "$ROOT"
echo "rename-crates: regenerating THIRD_PARTY_NOTICES.md…"
bash "$ROOT/scripts/gen-third-party-notices.sh" >/dev/null
echo "rename-crates: cargo fmt --all…"
cargo fmt --all --manifest-path "$ROOT/Cargo.toml"
echo "rename-crates: done — re-run is a no-op; verify with cargo fmt --all --check && cargo build --workspace --all-targets && cargo test --workspace && bash scripts/check-drift.sh"
