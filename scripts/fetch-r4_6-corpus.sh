#!/usr/bin/env bash
# fetch-r4_6-corpus.sh — the R4.6 pinned upstream corpus fetch
# (H3 pre-authorization 2026-10-20 "Toolchains + online corpus": read-only
# fetches of PUBLIC upstream corpora; no accounts, no mutations).
#
#   scripts/fetch-r4_6-corpus.sh            fetch + verify + extract the
#                                          committed fixture subsets
#   scripts/fetch-r4_6-corpus.sh --check   re-fetch at the committed
#                                          fixtures' own recorded pins,
#                                          sha256-verify, re-extract and
#                                          byte-compare vs the committed
#                                          fixtures
#
# Pins (URL + commit + per-file sha256 — recorded here and inside each
# fixture's `upstream` member):
#
#   AgentDojo (ethz-spylab/agentdojo, MIT) — tag v0.1.35 @ commit
#   a75aba7631d3ca5fb7ab938965c97ead2f9ff84b. Sources: the default
#   suites' injection_tasks.py — the injection tasks' GOAL attack
#   strings. The extracted strings are TEST FIXTURES ONLY — quoted
#   data, never instructions.
#
#   Harbor Index (harbor-framework/harbor, Apache-2.0) — commit
#   dd8815755a61e849ad0f146ae85eb4aaff4fe61f. Source: registry.json —
#   the real foreign-benchmark manifest (dataset entries x task rows
#   carrying pinned git_commit_id digests).
set -euo pipefail
set +x

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
AGENTDOJO_COMMIT="a75aba7631d3ca5fb7ab938965c97ead2f9ff84b"
AGENTDOJO_TAG="v0.1.35"
AGENTDOJO_BASE="https://raw.githubusercontent.com/ethz-spylab/agentdojo/$AGENTDOJO_COMMIT"
# Repo-relative sources — the v1_2 base suites + the v1_2_1 workspace update.
AD_PATHS="default_suites/v1_2/workspace/injection_tasks.py
default_suites/v1_2/banking/injection_tasks.py
default_suites/v1_2/travel/injection_tasks.py
default_suites/v1_2_1/workspace/injection_tasks.py"
HARBOR_COMMIT="dd8815755a61e849ad0f146ae85eb4aaff4fe61f"
HARBOR_BASE="https://raw.githubusercontent.com/harbor-framework/harbor/$HARBOR_COMMIT"
HARBOR_PATH="registry.json"

EMBED_FIX="$REPO/crates/hh-embed/tests/fixtures/r4_6"
BENCH_FIX="$REPO/crates/hh-bench/fixtures/live/r4_6"
BATTERY_N=8    # injection goals carried in the committed subset
SUBSET_TASKS=8 # harbor task rows carried in the committed subset
TMPD_F=""

say()  { printf '%s\n' "fetch-r4_6-corpus: $*"; }
fail() { printf '%s\n' "fetch-r4_6-corpus: FAIL — $*" >&2; exit 1; }
require() { command -v "$1" >/dev/null 2>&1 || fail "required tool absent: $1"; }
cleanup() { [ -n "$TMPD_F" ] && rm -rf "$TMPD_F"; }
trap cleanup EXIT

fetch() { # $1 url $2 out — read-only public GET; prints the file's sha256
  require curl
  curl -sS --max-time 120 -fL "$1" -o "$2" \
    || { printf 'fetch failed: %s\n' "$1" >&2; return 1; }
  shasum -a 256 "$2" | cut -d' ' -f1
}

# Deterministic local name for a repo-relative source path.
ad_local() { printf '%s\n' "$1" | sed 's|default_suites/||; s|/|__|g'; }

# The extraction is constants-only Python (ast.literal_eval + f-string
# substitution over string constants) — it never executes upstream code.
extract() { # $1 workdir (ad-manifest at $1/manifest: localfile<TAB>relpath)
  python3 - "$1" "$BATTERY_N" "$SUBSET_TASKS" <<'PY'
import ast, hashlib, json, os, sys

work, battery_n, subset_n = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
AD_COMMIT = "a75aba7631d3ca5fb7ab938965c97ead2f9ff84b"
HB_COMMIT = "dd8815755a61e849ad0f146ae85eb4aaff4fe61f"
HB_PATH = "registry.json"

def sha256(p):
    return hashlib.sha256(open(p, "rb").read()).hexdigest()

def consts(node):
    out = {}
    for st in node.body:
        if isinstance(st, ast.Assign) and len(st.targets) == 1 \
                and isinstance(st.targets[0], ast.Name):
            try:
                v = ast.literal_eval(st.value)
            except Exception:
                continue
            if isinstance(v, str):
                out[st.targets[0].id] = v
    return out

def render(node, names):
    """Render a GOAL expression — str literal | f-string | string concat,
    class/module string constants for names. Attack strings stay data."""
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    if isinstance(node, ast.JoinedStr):
        parts = []
        for v in node.values:
            if isinstance(v, ast.Constant):
                parts.append(str(v.value))
            elif isinstance(v, ast.FormattedValue):
                parts.append(render(v.value, names))
            else:
                raise ValueError("unsupported JoinedStr member")
        return "".join(parts)
    if isinstance(node, ast.Name):
        return names[node.id]
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Add):
        return render(node.left, names) + render(node.right, names)
    raise ValueError("unsupported GOAL expression: " + ast.dump(node)[:80])

def goals_from(path, suite):
    tree = ast.parse(open(path, encoding="utf-8").read())
    module_names = consts(tree)
    out = []
    for node in tree.body:
        if not isinstance(node, ast.ClassDef) \
                or not node.name.startswith("InjectionTask"):
            continue
        names = dict(module_names)
        names.update(consts(node))
        goal = difficulty = None
        for st in node.body:
            if isinstance(st, ast.Assign):
                for t in st.targets:
                    if isinstance(t, ast.Name) and t.id == "GOAL":
                        goal = render(st.value, names)
                    elif isinstance(t, ast.Name) and t.id == "DIFFICULTY":
                        difficulty = getattr(st.value, "attr", None)
        if goal:
            out.append({
                "id": suite + "/" + node.name,
                "suite": suite,
                "difficulty": (difficulty or "unknown").lower(),
                "goal": goal,
            })
    return out

# ── AgentDojo battery — round-robin the suites so the subset samples
# every fetched suite, not just the first ────────────────────────────────
source_files, per_suite = [], []
for line in open(os.path.join(work, "manifest")).read().splitlines():
    local, rel = line.split("\t")
    p = os.path.join(work, "ad", local)
    suite = rel.split("/")[2]  # default_suites/<ver>/<suite>/...
    source_files.append({
        "path": "src/agentdojo/" + rel,
        "sha256": sha256(p),
    })
    per_suite.append(goals_from(p, suite))
tasks = []
i = 0
while len(tasks) < battery_n and any(i < len(q) for q in per_suite):
    for q in per_suite:
        if i < len(q):
            tasks.append(q[i])
    i += 1
tasks = tasks[:battery_n]
if not tasks:
    sys.exit("extract: no GOAL strings found in the AgentDojo sources")
battery = {
    "schema": "lt03-corpus/1",
    "note": "AgentDojo-shaped injection battery — the goals are ATTACK "
            "strings carried as quoted test-fixture data only; they are "
            "never instructions to follow.",
    "upstream": {
        "repo": "ethz-spylab/agentdojo",
        "url": "https://github.com/ethz-spylab/agentdojo",
        "tag": "v0.1.35",
        "commit": AD_COMMIT,
        "license": "MIT",
        "source_files": source_files,
    },
    "tasks": tasks,
}
with open(os.path.join(work, "lt03-battery.json"), "w") as f:
    json.dump(battery, f, indent=1, sort_keys=True)
    f.write("\n")

# ── Harbor registry subset ──────────────────────────────────────────────
reg_path = os.path.join(work, "registry.json")
reg = json.load(open(reg_path))
entry = dict(reg[0])
entry["tasks"] = entry["tasks"][:subset_n]
total_tasks = sum(len(e.get("tasks", [])) for e in reg)
subset = {
    "schema": "foreign-manifest-subset/1",
    "note": "Committed slice of the upstream manifest for the offline "
            "import re-run; the live leg fetches + sha256-verifies the "
            "full registry.json at this pin.",
    "upstream": {
        "repo": "harbor-framework/harbor",
        "url": "https://raw.githubusercontent.com/harbor-framework/harbor/"
               + HB_COMMIT + "/" + HB_PATH,
        "commit": HB_COMMIT,
        "license": "Apache-2.0",
        "source_file": HB_PATH,
        "source_sha256": sha256(reg_path),
        "manifest_entries": len(reg),
        "manifest_tasks": total_tasks,
        "subset": "entries[0].tasks[0:%d]" % subset_n,
    },
    "entry": entry,
}
with open(os.path.join(work, "harbor-registry-subset.json"), "w") as f:
    json.dump(subset, f, indent=1, sort_keys=True)
    f.write("\n")
PY
}

cmd_fetch() {
  TMPD_F=$(mktemp -d)
  mkdir -p "$TMPD_F/ad"; : > "$TMPD_F/manifest"
  say "fetching AgentDojo $AGENTDOJO_TAG @ ${AGENTDOJO_COMMIT:0:12}"
  printf '%s\n' "$AD_PATHS" | while IFS= read -r rel; do
    dest=$(ad_local "$rel")
    sha=$(fetch "$AGENTDOJO_BASE/src/agentdojo/$rel" "$TMPD_F/ad/$dest") \
      || fail "AgentDojo source unreachable: $rel"
    printf '%s\t%s\n' "$dest" "$rel" >> "$TMPD_F/manifest"
    say "  $rel sha256:$sha"
  done
  fetch "$AGENTDOJO_BASE/LICENSE" "$TMPD_F/LICENSE.agentdojo" >/dev/null \
    || fail "AgentDojo LICENSE unreachable"
  say "fetching Harbor @ ${HARBOR_COMMIT:0:12}"
  local hb_sha
  hb_sha=$(fetch "$HARBOR_BASE/$HARBOR_PATH" "$TMPD_F/registry.json") \
    || fail "Harbor registry.json unreachable"
  say "  $HARBOR_PATH sha256:$hb_sha"
  fetch "$HARBOR_BASE/LICENSE" "$TMPD_F/LICENSE.harbor" >/dev/null \
    || fail "Harbor LICENSE unreachable"
  extract "$TMPD_F"
  mkdir -p "$EMBED_FIX" "$BENCH_FIX"
  cp "$TMPD_F/lt03-battery.json" "$EMBED_FIX/lt03-battery.json"
  cp "$TMPD_F/LICENSE.agentdojo" "$EMBED_FIX/UPSTREAM-LICENSE.agentdojo"
  cp "$TMPD_F/harbor-registry-subset.json" "$BENCH_FIX/harbor-registry-subset.json"
  cp "$TMPD_F/LICENSE.harbor" "$BENCH_FIX/UPSTREAM-LICENSE.harbor"
  say "wrote $EMBED_FIX/lt03-battery.json (+ UPSTREAM-LICENSE.agentdojo)"
  say "wrote $BENCH_FIX/harbor-registry-subset.json (+ UPSTREAM-LICENSE.harbor)"
}

cmd_check() {
  TMPD_F=$(mktemp -d)
  mkdir -p "$TMPD_F/ad"
  # The committed fixtures' own pins are the source of truth for the
  # re-fetch — a rewritten script can't smuggle a different pin.
  local hb_url hb_sha
  hb_url=$(python3 -c "import json; print(json.load(open('$BENCH_FIX/harbor-registry-subset.json'))['upstream']['url'])")
  hb_sha=$(python3 -c "import json; print(json.load(open('$BENCH_FIX/harbor-registry-subset.json'))['upstream']['source_sha256'])")
  python3 - "$EMBED_FIX/lt03-battery.json" > "$TMPD_F/ad_files" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
for f in d["upstream"]["source_files"]:
    print(f["path"], f["sha256"])
PY
  while read -r path sha; do
    rel=${path#src/agentdojo/}
    dest=$(ad_local "$rel")
    printf '%s\t%s\n' "$dest" "$rel" >> "$TMPD_F/manifest"
    got=$(fetch "$AGENTDOJO_BASE/$path" "$TMPD_F/ad/$dest") \
      || fail "AgentDojo source unreachable: $rel"
    [ "$got" = "$sha" ] || fail "AgentDojo pin drifted on $rel: $got != $sha"
  done < "$TMPD_F/ad_files"
  local got
  got=$(fetch "$hb_url" "$TMPD_F/registry.json") || fail "Harbor registry unreachable"
  [ "$got" = "$hb_sha" ] || fail "Harbor pin drifted: $got != $hb_sha"
  extract "$TMPD_F"
  cmp -s "$TMPD_F/lt03-battery.json" "$EMBED_FIX/lt03-battery.json" \
    || fail "lt03-battery.json not reproducible from the pin"
  cmp -s "$TMPD_F/harbor-registry-subset.json" "$BENCH_FIX/harbor-registry-subset.json" \
    || fail "harbor-registry-subset.json not reproducible from the pin"
  say "check: both fixtures reproduce byte-identically from the recorded pins"
}

case "${1:-}" in
  --check) cmd_check;;
  "")      cmd_fetch;;
  *)       fail "unknown argument: $1";;
esac
