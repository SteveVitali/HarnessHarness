#!/usr/bin/env bash
# check-publication.sh — R6.6 (F2-g): validate committed
# hh-site-publication/1 records and the publication-gate checklist
# (REQ-XF-128/129/130, REQ-XH-018, AC-9.6-6).
#
#   check-publication.sh [ROOT]
#
# A record is well-formed iff it names its phase's manifest gate row and
# readout file, pins a sha256 input set, carries the labels/dataset
# fields, and (when present) references a committed superseded record.
# The manifest must carry the HUMAN-H12 / GATE-G22 / GATE-G31 / HUMAN-H17
# rows and the publication-gate checklist. Exit 0 clean (including zero
# records — nothing published yet), 1 a violation, 2 usage.
set -euo pipefail

ROOT="${1:-$(git rev-parse --show-toplevel)}"
python3 - "$ROOT" <<'PY'
import sys
from pathlib import Path

root = Path(sys.argv[1])
sys.path.insert(0, str(root / "scripts" / "site"))
from hhsite import publication  # noqa: E402

fails = []

records, errors = publication.load_dir(root)
fails += errors

# append-only: a later record supersedes an earlier one only by the
# `supersedes` field, and never renames/deletes (well-formedness covers
# the reference; git history carries immutability)
by_phase = {}
for rel, doc in records:
    by_phase.setdefault(doc["phase"], []).append((rel, doc))
for phase, recs in by_phase.items():
    recs.sort(key=lambda r: r[1]["recorded_at"])
    for _r, doc in recs[1:]:
        if not doc.get("supersedes"):
            fails.append("%s: second %s record does not supersede the "
                         "earlier one" % (_r, phase))

for e in publication.manifest_row_errors(root):
    fails.append(e)

if fails:
    for f in fails:
        print("check-publication: FAIL — %s" % f, file=sys.stderr)
    print("check-publication: %d failure(s)" % len(fails), file=sys.stderr)
    sys.exit(1)
print("check-publication: OK — %d record(s); manifest rows and checklist "
      "present (nothing published until GATE-G22)" % len(records))
PY
