#!/usr/bin/env bash
# test-check-site.sh — the §9.4 mutation battery for scripts/check-site.sh
# (R6.6 / REQ-XF-135): seeds a throwaway checkout of the site surface and
# fails each mutation (a)–(v) on its named leg; the clean seed and the
# real checkout are green; exit codes 0/1/2 hold. Publication-record
# legs cover AC-9.6-1/2/3/5/6 and AC-9.5-5 alongside.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
CHK="$ROOT/scripts/check-site.sh"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fails=0
say() { echo "test-check-site: $*"; }
bad() { echo "test-check-site: FAIL — $*" >&2; fails=$((fails + 1)); }

seed() {  # $1 = dir — minimal repo carrying the real site surface
  local d="$TMP/$1"
  mkdir -p "$d/scripts" "$d/docs/results/site-data" "$d/docs/results" \
          "$d/docs/build/reports" "$d/docs/build/readouts" \
          "$d/docs/tickets" "$d/docs/results/publication"
  cp -R "$ROOT/scripts/site" "$d/scripts/"
  rm -rf "$d/scripts/site/__pycache__" "$d/scripts/site/hhsite/__pycache__"
  cp "$ROOT/scripts/check-site.sh" "$d/scripts/"
  cp "$ROOT/scripts/claims-lexicon.json" "$d/scripts/"
  cp "$ROOT/docs/evidence.md" "$d/docs/"
  cp "$ROOT/docs/results/phase1-execution-surface-replay.md" "$d/docs/results/"
  cp "$ROOT/docs/results/site-data/"*.json "$d/docs/results/site-data/"
  cp "$ROOT/docs/tickets/00_MANIFEST.md" "$d/docs/tickets/"
  python3 - "$ROOT" "$d" <<'PY'
import json, shutil, sys
from pathlib import Path
root, d = Path(sys.argv[1]), Path(sys.argv[2])
cfg = json.loads((root / "scripts/site/config/inputs.json").read_text())
for i in cfg["inputs"]:
    dst = d / i["path"]
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(root / i["path"], dst)
PY
  git -C "$d" init -q >/dev/null
  git -C "$d" config user.email t@t; git -C "$d" config user.name t
  git -C "$d" add -A >/dev/null 2>&1
  git -C "$d" commit -qm seed >/dev/null 2>&1
  printf '%s' "$d"
}

expect() {  # $1 = ok|fail, $2 = label, $3+ = check-site.sh args
  local want="$1" label="$2"; shift 2
  local out rc=0
  out=$(bash "$CHK" "$@" 2>&1) || rc=$?
  if [ "$want" = ok ]; then
    [ "$rc" -eq 0 ] && say "ok  $label" || bad "$label: expected pass, got rc=$rc: $out"
  else
    [ "$rc" -ne 0 ] && say "ok  $label (rejected)" \
      || bad "$label: expected failure, got pass"
  fi
}

expect_leg() {  # $1 = named leg, $2 = label, $3+ = check-site.sh args
  local leg="$1" label="$2"; shift 2
  local out rc=0
  out=$(bash "$CHK" "$@" 2>&1) || rc=$?
  if [ "$rc" -eq 0 ]; then bad "$label: expected FAIL [$leg], got pass"; return; fi
  grep -q "FAIL \[$leg\]" <<<"$out" \
    && say "ok  $label → $leg" \
    || bad "$label: leg $leg absent from output: $(tail -5 <<<"$out")"
}

mutate() { sed -i '' "$2" "$1" 2>/dev/null || sed -i "$2" "$1"; }

D=$(seed good)
expect ok "seeded tree" "$D"
expect ok "seeded tree (public, zero records → nothing published)" --profile public "$D"

# (a) drop an interval — intervals
D=$(seed mut-a)
python3 - "$D/docs/results/site-data/phase1.json" <<'PY'
import json, sys
p = sys.argv[1]; d = json.loads(open(p).read())
b = d["body"]["estimator_outcomes"]["descriptive_bounds"][0]["bounds"][0]
del b["delta"]["hi"]
open(p, "w").write(json.dumps(d) + "\n")
PY
expect_leg "intervals" "(a) interval dropped" "$D"

# (b) a rank in the leader band — ranks (R6.20: seeded on the
# leaderboard fixture's band label)
D=$(seed mut-b)
mutate "$D/scripts/site/hhsite/sitepages.py" 's|("observed-leader group|("#1 observed-leader group|'
expect_leg "ranks" "(b) ordinal in leader band" "$D"

# (c) phase 2 Available with no packet — availability
D=$(seed mut-c)
mutate "$D/scripts/site/hhsite/sitepages.py" 's|"awaits the FP-S gate", "phase-2/"|"available now", "phase-2/"|; s|stamp("absent", "Not yet available"),\n         "available now"|stamp("verified", "Available"),\n         "available now"|'
python3 - "$D/scripts/site/hhsite/sitepages.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
t = t.replace('("P2 · comparative", "phase-2",\n         stamp("absent", "Not yet available"),',
              '("P2 · comparative", "phase-2",\n         stamp("verified", "Available"),', 1)
open(p, "w").write(t)
PY
expect_leg "availability" "(c) phase 2 stamped Available, no packet" "$D"

# (d) a banned entry inserted — lexicon
D=$(seed mut-d)
python3 - "$D/scripts/site/hhsite/sitepages.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = "keeps its receipts."
assert old in t
open(p, "w").write(t.replace(old, "keeps its receipts and outperform.", 1))
PY
expect_leg "lexicon" "(d) banned phrase inserted" "$D"

# (e) "statistically tied" — lexicon (site_banned)
D=$(seed mut-e)
mutate "$D/scripts/site/hhsite/sitepages.py" 's|records-in cross-check confound|statistically tied records-in cross-check confound|'
expect_leg "lexicon" "(e) 'statistically tied'" "$D"

# (f) "significant" outside the Holm form — conditional
D=$(seed mut-f)
mutate "$D/scripts/site/hhsite/sitepages.py" 's|disposition `reported`|a significant disposition `reported`|'
expect_leg "conditional" "(f) 'significant' outside Holm form" "$D"

# (g) a displayed value off by one unit — provenance
D=$(seed mut-g)
python3 - "$D/docs/results/site-data/phase1.json" <<'PY'
import json, sys
p = sys.argv[1]; d = json.loads(open(p).read())
d["body"]["tranche3"]["counts"]["agree"]["v"] += 1
open(p, "w").write(json.dumps(d) + "\n")
PY
expect_leg "provenance" "(g) displayed value off by one" "$D"

# (h) one packet byte altered — provenance
D=$(seed mut-h)
python3 - "$D/docs/build/reports/2026-11-05-parity-tranche-3.json" <<'PY'
import json, sys
p = sys.argv[1]; d = json.loads(open(p).read())
d["counts"]["n_pairs"] += 1
open(p, "w").write(json.dumps(d) + "\n")
PY
expect_leg "provenance" "(h) packet byte altered" "$D"

# (i) the not-outside-verified label removed — labels
D=$(seed mut-i)
python3 - "$D/scripts/site/hhsite/project.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = "not outside-verified"
assert old in t
open(p, "w").write(t.replace(old, "verified"))
PY
expect_leg "labels" "(i) not-outside-verified label removed" "$D"

# (j) an external script — no-third-party
D=$(seed mut-j)
python3 - "$D/scripts/site/hhsite/pages.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = 'defer></script>"'
assert old in t
open(p, "w").write(t.replace(
    old, 'defer></script><script src=\\"https://cdn.example/x.js\\">'
         '</script>"', 1))
PY
expect_leg "no-third-party" "(j) external script" "$D"

# (k) a chart's data table deleted — a11y-static
D=$(seed mut-k)
python3 - "$D/scripts/site/hhsite/components.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = "chart, rows, table,"
assert old in t
open(p, "w").write(t.replace(old, 'chart, rows, "",', 1))
PY
expect_leg "a11y-static" "(k) data table deleted" "$D"

# (l) an SVG <title> removed — a11y-static
D=$(seed mut-l)
python3 - "$D/scripts/site/hhsite/components.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = "parts.append('</g><title>%s</title></svg>' % ESC(label or \"interval\"))"
assert old in t
open(p, "w").write(t.replace(old, "parts.append('</g></svg>')", 1))
PY
expect_leg "a11y-static" "(l) svg <title> removed" "$D"

# (m) a token below 4.5:1 — a11y-static
D=$(seed mut-m)
mutate "$D/scripts/site/assets/tokens.css" 's|  --ink-3: #5E6A73;|  --ink-3: #C4CDD4;|'
expect_leg "a11y-static" "(m) token below 4.5:1" "$D"

# (n) $0.00 for $/solve at 0 solves — labels (R6.21: seeded on
# the undefined-solve renderer — a dollar figure in $/solve context is
# the defect the labels leg names)
D=$(seed mut-n)
python3 - "$D/scripts/site/hhsite/sitepages.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = '>undefined (0 solves)<'
assert old in t
open(p, "w").write(t.replace(old, '>$/solve $0.00<', 1))

PY
expect_leg "labels" '(n) dollar zero for $/solve at 0 solves' "$D"

# (o) a trajectory link while the index is private — no-third-party
# (R6.21: seeded inside the viewer block — the private index shows the
# sha256 and never a link)
D=$(seed mut-o)
python3 - "$D/scripts/site/hhsite/sitepages.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = "'withheld{dataset_private} — no link while private</span>'"
assert old in t
open(p, "w").write(t.replace(
    old, "'withheld{dataset_private} <a href=\"https://huggingface.co/x\">trajectory</a></span>'", 1))
PY
expect_leg "no-third-party" "(o) trajectory link while private" "$D"

# (p) a colour-only stamp — a11y-static
D=$(seed mut-p)
python3 - "$D/scripts/site/hhsite/components.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = '        raise GuardError("stamp without its verbatim text is colour-only")'
assert old in t
t = t.replace(old, '        return  # fixture: colour-only stamps allowed', 1)
old2 = "'<svg class=\"glyph\" role=\"img\" aria-hidden=\"true\">'"
assert old2 in t
t = t.replace(old2, "''", 1)
open(p, "w").write(t)
PY
expect_leg "a11y-static" "(p) colour-only stamp" "$D"

# (q) hand-edited site-data — drift
D=$(seed mut-q)
python3 - "$D/docs/results/site-data/spend.json" <<'PY'
import json, sys
p = sys.argv[1]; d = json.loads(open(p).read())
d["body"]["inference_usd"]["v"] += 1
open(p, "w").write(json.dumps(d, indent=2) + "\n")
PY
expect_leg "drift" "(q) hand-edited site-data" "$D"

# (r) a digit in a skeleton — availability
D=$(seed mut-r)
python3 - "$D/scripts/site/hhsite/components.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = '        if _DIGIT_RE.search(body):'
assert old in t
t = t.replace(old, '        if False and _DIGIT_RE.search(body):', 1)
old2 = "body = \"\".join('<div class=\"bar %s\"></div>' % b for b in bars)"
assert old2 in t
t = t.replace(old2, old2 + " + '<div class=\"bar w-5\"></div>'", 1)
open(p, "w").write(t)
PY
expect_leg "availability" "(r) digit in a skeleton" "$D"

# (s) a fixture value in public — hygiene
D=$(seed mut-s)
python3 - "$D/scripts/site/hhsite/sitepages.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = "keeps its receipts."
assert old in t
open(p, "w").write(t.replace(old, "fixture · keeps its receipts.", 1))
PY
expect_leg "hygiene" "(s) fixture value in public" --profile public "$D"

# (t) a status token and a series token in one chart — a11y-static
D=$(seed mut-t)
mutate "$D/scripts/site/hhsite/components.py" 's|cls = "int-rule" if separated else "int-rule open"|cls = "int-rule st-verified series-3"|'
expect_leg "a11y-static" "(t) status + series token in one chart" "$D"

# (u) a local path in site-data — hygiene
D=$(seed mut-u)
python3 - "$D/docs/results/site-data/method.json" <<'PY'
import json, sys
p = sys.argv[1]; d = json.loads(open(p).read())
d["body"]["note"] = "/Users/example/scratch"
open(p, "w").write(json.dumps(d) + "\n")
PY
expect_leg "hygiene" "(u) local path in site-data" "$D"

# (v) "equivalent" outside the L2 verdict form — conditional
D=$(seed mut-v)
mutate "$D/scripts/site/hhsite/sitepages.py" 's|parity admission + directional-KL bound|equivalent parity admission|'
expect_leg "conditional" "(v) 'equivalent' outside L2 verdict form" "$D"

# --head ignores working-tree dirt; committed drift fails
D=$(seed head-dirty)
python3 - "$D/scripts/site/hhsite/sitepages.py" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
old = "keeps its receipts."
assert old in t
open(p, "w").write(t.replace(old, "keeps its receipts and outperform.", 1))
PY
expect ok "--head ignores working-tree dirt" --head "$D"
expect_leg "lexicon" "working-tree mode sees the dirt" "$D"

# (w) a third-party logo mark without a recorded permission — logos
D=$(seed mut-w)
mutate "$D/scripts/site/hhsite/sitepages.py" 's|keeps its receipts.</h1>|keeps its receipts.</h1><img src="claude-logo.png" alt="vendor logo">|'
expect_leg "logos" "(w) third-party logo mark" "$D"

# usage / malformed input exits 2
out=$(bash "$CHK" --bogus "$D" 2>&1) && bad "usage: --bogus accepted" \
  || { [ $? -eq 2 ] && say "ok  usage exit 2" || bad "usage exit != 2: $out"; }

# publication fixtures (AC-9.6-1/2/6, AC-9.5-5): a record pinning phase 1
# publishes only phase 1; an input-set mismatch refuses; two records yield
# two changelog entries and a superseded /archive/<v>/.
pub_seed() {  # $1 = seed name, $2 = "one"|"two"|"bad"
  local d=$(seed "pub-$1")
  local sd="$d/docs/results/site-data"
  python3 - "$d" "$2" <<'PY'
import hashlib, json, sys
from pathlib import Path
d = Path(sys.argv[1]); kind = sys.argv[2]
cfg = json.loads((d / "scripts/site/config/inputs.json").read_text())
inputs = [{"path": i["path"],
           "sha256": hashlib.sha256((d / i["path"]).read_bytes()).hexdigest()}
          for i in cfg["inputs"] if i["phase"] == "phase-1"]
dig = "sha256:" + hashlib.sha256(json.dumps(
    inputs, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
rd = d / "docs/build/readouts"; rd.mkdir(parents=True, exist_ok=True)
(rd / "GATE-G22.md").write_text(
    "# GATE-G22 — phase-1 publication\nStatus: PASSED\n")
pub = d / "docs/results/publication"; pub.mkdir(parents=True, exist_ok=True)
rec = {
    "schema": "hh-site-publication/1",
    "phase": "phase-1",
    "pages": ["/", "/phase-1/"],
    "inputs": inputs, "inputs_digest": dig,
    "gate_row": "286_GATE-G22__pub-p1",
    "readout": "docs/build/readouts/GATE-G22.md",
    "labels": {"evidence_class": "recorded execution",
               "outside_verified": False},
    "dataset": {"visibility": "private", "revision": None,
                "secret_scan_ref": None,
                "contamination_scan_ref": None},
    "recorded_at": "2026-11-08",
}
if kind == "bad":
    rec["inputs"][0]["sha256"] = "0" * 64
    rec["inputs_digest"] = "sha256:" + hashlib.sha256(json.dumps(
        rec["inputs"], sort_keys=True,
        separators=(",", ":")).encode()).hexdigest()
(pub / "phase-1-2026-11-08.json").write_text(json.dumps(rec, indent=2))
if kind == "two":
    rec2 = dict(rec, recorded_at="2026-11-09",
                supersedes="docs/results/publication/phase-1-2026-11-08.json")
    (pub / "phase-1-2026-11-09.json").write_text(json.dumps(rec2, indent=2))
PY
  printf '%s' "$d"
}

D=$(pub_seed one one)
expect ok "public profile: phase-1 record publishes phase 1" --profile public "$D"

D=$(pub_seed bad bad)
expect fail "input-set mismatch refused" --profile public "$D"

D=$(pub_seed two two)
out=$(bash "$CHK" --profile public "$D" 2>&1) || bad "two records: check failed: $out"
site_dir=$(python3 - "$D" <<'PY'
import subprocess, sys, tempfile, os
d = sys.argv[1]
out = os.path.join(tempfile.mkdtemp(prefix="hh-pub-"), "s")
r = subprocess.run(["python3", os.path.join(d, "scripts/site/build.py"),
                    "--out", out, "--profile", "public"],
                   cwd=d, capture_output=True, text=True)
print(out if r.returncode == 0 else "FAIL:" + r.stderr)
PY
)
if [ -f "$site_dir/archive/phase-1-2026-11-08/index.html" ] \
   && grep -q "superseded by" "$site_dir/archive/phase-1-2026-11-08/index.html" \
   && grep -q "phase-1-2026-11-08" "$site_dir/changelog/index.html" \
   && grep -q "phase-1-2026-11-09" "$site_dir/changelog/index.html"; then
  say "ok  two records → two changelog entries + superseded /archive/<v>/"
else
  bad "changelog/archive fixture wrong (site: $site_dir)"
fi

# validator: a record without a gate readout reference is rejected (AC-9.6-6)
D=$(pub_seed noreadout one)
python3 - "$D" <<'PY'
import json, sys
from pathlib import Path
d = Path(sys.argv[1])
f = d / "docs/results/publication/phase-1-2026-11-08.json"
rec = json.loads(f.read_text())
rec["readout"] = "docs/build/readouts/GATE-G99.md"
f.write_text(json.dumps(rec, indent=2))
PY
expect fail "record without gate readout rejected" --profile public "$D"

D=$(pub_seed badgate one)
python3 - "$D" <<'PY'
import json, sys
from pathlib import Path
d = Path(sys.argv[1])
f = d / "docs/results/publication/phase-1-2026-11-08.json"
rec = json.loads(f.read_text())
rec["gate_row"] = "GATE-G99"
f.write_text(json.dumps(rec, indent=2))
PY
expect fail "record naming wrong gate rejected" --profile public "$D"

# the real checkout is green
expect ok "real checkout (working tree)" "$ROOT"

[ "$fails" -eq 0 ] || { echo "test-check-site: $fails failure(s)" >&2; exit 1; }
echo "test-check-site: OK"
