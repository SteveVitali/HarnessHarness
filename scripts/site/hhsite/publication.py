"""publication — hh-site-publication/1 records (REQ-XF-128/129, REQ-XH-018).

A publication record is a committed, append-only JSON file at
``docs/results/publication/<phase>-<YYYY-MM-DD>.json`` (design/39 F10):

  schema          "hh-site-publication/1"
  phase           the phase whose pages the record pins ("phase-1" …)
  pages[]         the URL prefixes the record publishes
  inputs[]        [{path, sha256}] — the phase's exact input set
  inputs_digest   "sha256:" + sha256 over the canonical inputs list
  gate_row        the manifest gate row id whose readout carries the
                  operator's verbatim decision (referenced, never copied)
  readout         repo-relative path of that gate readout
  labels          {evidence_class, outside_verified: false}
  dataset         {visibility, revision, secret_scan_ref,
                   contamination_scan_ref}
  recorded_at     ISO date, matching the filename
  supersedes      optional repo-relative path of the earlier record for
                  the same phase (append-only supersede by reference)

The validator checks well-formedness only; it never verifies a packet's
current bytes (that is the public build's pin check — an input-set
mismatch is refused there, never laundered through the record).
"""
import hashlib
import json
import re
from pathlib import Path

SCHEMA = "hh-site-publication/1"
PUB_DIR_REL = "docs/results/publication"
CHECKLIST_REL = "docs/results/publication/CHECKLIST.md"
MANIFEST_REL = "docs/tickets/00_MANIFEST.md"

# Each publishing phase and the manifest gate row that authorizes it
# (AC-9.6-6: a phase without a manifest gate cannot publish).
PHASE_GATES = {
    "phase-1": "GATE-G22",
    "phase-2": "GATE-G31",
}
PHASES = tuple(sorted(PHASE_GATES) + ["phase-3", "phase-4"])

# Manifest rows an agent can never perform (REQ-XF-130): enabling Pages,
# the per-phase publication decisions, and the dataset visibility flip.
MANIFEST_HUMAN_ROWS = ("HUMAN-H12", "GATE-G22", "GATE-G31", "HUMAN-H17")

EVIDENCE_CLASSES = frozenset([
    "recorded execution", "fixture-verified", "live-executed",
    "foreign-run-verified", "environment-pending"])

_FILENAME_RE = re.compile(r"^(phase-[0-9]+)-(\d{4}-\d{2}-\d{2})\.json$")
_SHA_RE = re.compile(r"^[0-9a-f]{64}$")


def record_digest(inputs):
    """sha256 over the canonical inputs list — the same construction as
    packets.inputs_digest (sorted keys, compact separators)."""
    rows = [{"path": i["path"], "sha256": i["sha256"]} for i in inputs]
    return "sha256:" + hashlib.sha256(
        json.dumps(rows, sort_keys=True,
                   separators=(",", ":")).encode()).hexdigest()


def version_of(path):
    """The archive/changelog version id: the record's filename stem."""
    return Path(path).stem


def validate(doc, path, repo_root):
    """Well-formedness of one hh-site-publication/1 record. Returns a
    list of error strings; empty means the record is well-formed.
    ``path`` is the record's repo-relative path; ``repo_root`` resolves
    referenced files (readout, superseded record)."""
    errs = []
    repo_root = Path(repo_root)

    def need(cond, msg):
        if not cond:
            errs.append(msg)

    if not isinstance(doc, dict):
        return ["record is not a JSON object"]
    need(doc.get("schema") == SCHEMA,
         "schema %r != %s" % (doc.get("schema"), SCHEMA))

    m = _FILENAME_RE.match(Path(path).name)
    need(m is not None,
         "filename %r is not <phase>-<YYYY-MM-DD>.json" % Path(path).name)

    phase = doc.get("phase")
    need(phase in PHASES, "phase %r not a known phase" % phase)
    if m and phase is not None:
        need(m.group(1) == phase,
             "filename phase %r != record phase %r" % (m.group(1), phase))
        need(doc.get("recorded_at") == m.group(2),
             "recorded_at %r != filename date %r"
             % (doc.get("recorded_at"), m.group(2)))

    gate = PHASE_GATES.get(phase)
    need(gate is not None,
         "phase %r has no publication gate in the manifest" % phase)
    gate_row = doc.get("gate_row")
    need(isinstance(gate_row, str) and gate is not None
         and gate in gate_row,
         "gate_row %r does not name %s" % (gate_row, gate))

    readout = doc.get("readout")
    need(isinstance(readout, str) and readout.startswith("docs/build/"),
         "readout %r is not a docs/build/ path" % readout)
    if isinstance(readout, str):
        rf = repo_root / readout
        need(rf.is_file(), "gate readout %s absent" % readout)
        if rf.is_file() and isinstance(gate_row, str):
            text = rf.read_text(encoding="utf-8", errors="replace")
            need(gate in text,
                 "readout %s does not name %s" % (readout, gate))

    pages = doc.get("pages")
    need(isinstance(pages, list) and pages
         and all(isinstance(p, str) and p.startswith("/") for p in pages),
         "pages[] must be non-empty URL prefixes")
    if isinstance(pages, list):
        pref = "/" + phase + "/"
        need(any(p == "/" or p.startswith(pref) for p in pages),
             "pages[] does not cover the phase prefix %s" % pref)

    inputs = doc.get("inputs")
    need(isinstance(inputs, list) and inputs,
         "inputs[] must be a non-empty list")
    if isinstance(inputs, list):
        for n, i in enumerate(inputs):
            if not isinstance(i, dict):
                errs.append("inputs[%d] is not an object" % n)
                continue
            need(isinstance(i.get("path"), str) and i["path"]
                 and not i["path"].startswith("/")
                 and "*" not in i["path"] and "?" not in i["path"],
                 "inputs[%d].path %r is not a repo-relative path"
                 % (n, i.get("path")))
            need(isinstance(i.get("sha256"), str)
                 and _SHA_RE.match(i["sha256"] or "") is not None,
                 "inputs[%d].sha256 is not 64-hex" % n)
        good = [i for i in inputs
                if isinstance(i, dict) and isinstance(i.get("path"), str)
                and _SHA_RE.match(i.get("sha256") or "")]
        if good:
            want = record_digest(inputs)
            need(doc.get("inputs_digest") == want,
                 "inputs_digest %r != recomputed %r"
                 % (doc.get("inputs_digest"), want))

    labels = doc.get("labels")
    need(isinstance(labels, dict), "labels must be an object")
    if isinstance(labels, dict):
        need(labels.get("outside_verified") is False,
             "labels.outside_verified must be false until a graduation "
             "record exists")
        need(labels.get("evidence_class") in EVIDENCE_CLASSES,
             "labels.evidence_class %r not in the evidence vocabulary"
             % labels.get("evidence_class"))

    ds = doc.get("dataset")
    need(isinstance(ds, dict), "dataset must be an object")
    if isinstance(ds, dict):
        vis = ds.get("visibility")
        need(vis in ("private", "public"),
             "dataset.visibility %r not in {private, public}" % vis)
        for k in ("revision", "secret_scan_ref", "contamination_scan_ref"):
            need(k in ds, "dataset.%s absent" % k)
        if vis == "public":
            for k in ("revision", "secret_scan_ref",
                      "contamination_scan_ref"):
                need(isinstance(ds.get(k), str) and ds[k],
                     "dataset.%s required while visibility is public" % k)

    sup = doc.get("supersedes")
    if sup is not None:
        need(isinstance(sup, str) and sup.startswith(PUB_DIR_REL + "/"),
             "supersedes %r is not a docs/results/publication path" % sup)
        if isinstance(sup, str):
            sp = repo_root / sup
            need(sp.is_file(), "superseded record %s absent" % sup)
            if sp.is_file():
                try:
                    old = json.loads(sp.read_text())
                    need(old.get("phase") == phase,
                         "superseded record is for phase %r, not %r"
                         % (old.get("phase"), phase))
                    need(old.get("recorded_at", "") <= doc.get(
                        "recorded_at", ""), "supersedes a newer record")
                except (OSError, ValueError):
                    errs.append("superseded record %s is not JSON" % sup)
    return errs


def load_dir(repo_root):
    """Load and validate every committed record under
    docs/results/publication/. Returns (records, errors): records are
    (repo_rel_path, doc) pairs sorted by (recorded_at, name); errors are
    "path: error" strings across all files — a malformed record fails
    closed, never silently skipped."""
    d = Path(repo_root) / PUB_DIR_REL
    records, errors = [], []
    if d.is_dir():
        for f in sorted(d.glob("*.json")):
            rel = "%s/%s" % (PUB_DIR_REL, f.name)
            try:
                doc = json.loads(f.read_text())
            except (OSError, ValueError) as e:
                errors.append("%s: not JSON: %s" % (rel, e))
                continue
            for e in validate(doc, rel, repo_root):
                errors.append("%s: %s" % (rel, e))
            records.append((rel, doc))
    records.sort(key=lambda r: (r[1].get("recorded_at", ""), r[0]))
    return records, errors


def pin_errors(record, current_inputs):
    """Does the record pin the phase's *current* input set? Returns a
    list of mismatch strings. current_inputs are the named generator
    inputs ({id, path, schema, phase, sha256}); the record's inputs must
    equal the phase's exact current set — a mismatch is a refusal
    upstream (REQ-XF-129), never a partial publish."""
    errors = []
    phase = record["phase"]
    cur = {i.path: i.sha256 for i in current_inputs if i.phase == phase}
    rec = {i["path"]: i["sha256"] for i in record["inputs"]}
    for p, sha in rec.items():
        if p not in cur:
            errors.append("pinned input %s is not a current %s input"
                          % (p, phase))
        elif cur[p] != sha:
            errors.append("pinned input %s sha256 %s != current %s"
                          % (p, sha[:12], cur[p][:12]))
    for p in cur:
        if p not in rec:
            errors.append("current %s input %s is not pinned" % (phase, p))
    return errors


def published_phases(records, current_inputs):
    """The phases a public build may render: those pinned by a valid
    record whose input set matches exactly. A record for a phase whose
    pins mismatch is a caller-side refusal (never a silent skip)."""
    out = set()
    for _rel, doc in records:
        if not pin_errors(doc, current_inputs):
            out.add(doc["phase"])
    return out


def manifest_row_errors(repo_root):
    """REQ-XF-130/REQ-XH-018: the manifest carries (a) Pages enablement
    (HUMAN-H12), (b) a publication decision per phase (GATE-G22,
    GATE-G31) and (c) the dataset-visibility flip (HUMAN-H17) — none
    performed by an agent — and the publication-gate checklist exists."""
    errors = []
    mf = Path(repo_root) / MANIFEST_REL
    if not mf.is_file():
        return ["manifest %s absent" % MANIFEST_REL]
    text = mf.read_text()
    for row in MANIFEST_HUMAN_ROWS:
        if row not in text:
            errors.append("manifest lacks the %s row" % row)
    for gate in PHASE_GATES.values():
        if not re.search(r"\d+_%s__" % gate, text):
            errors.append("manifest carries no ticket row for %s" % gate)
    if not (Path(repo_root) / CHECKLIST_REL).is_file():
        errors.append("publication-gate checklist %s absent"
                      % CHECKLIST_REL)
    return errors
