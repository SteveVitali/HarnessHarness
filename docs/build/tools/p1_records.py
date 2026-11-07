#!/usr/bin/env python3
"""p1_records.py — `hh-p1-records-check/1` phase-1 records-in verifier (R6.2,
REQ-XA-106). Python stdlib only, read-only: it writes nothing but the report
on stdout. Committed inputs only and no absolute paths (ruling S-2): every
input is a repository-relative path under the check root.

    python3 -I docs/build/tools/p1_records.py --check
    python3 -I docs/build/tools/p1_records.py --check --root DIR
    python3 -I docs/build/tools/p1_records.py --check --dataset DIR --emit-dir DIR

Checks (REQ-XA-106's list order):
  RC-1  archive + raw hashes — files/ vs SHA256SUMS (71/71), the manifest's
        own sha256 equals the README value, index.json validates as
        hh-scratch-driver-archive/1, raw/ vs raw/SHA256SUMS.
  RC-2  dataset content digests — recomputes the 30-task and 80-task
        sha256tree/1 digests ONLY when a dataset tree is supplied via
        --dataset; otherwise skipped(dataset_absent), never a pass.
  RC-3  raw ↔ packet slots — every member of each S4 packet's slots[] row
        equals the raw record carrying its slot_ref (schedule members
        checked against the embedded manifest schedule; prior_disposition
        against the prior packet), plus the tranche-3 pairs[] o_*/h_*
        members against the raw O/H records.
  RC-4  fold histograms — slot multiplicity, round-0 later-attempts,
        backend and network splits recomputed from the raw set equal
        raw/index.json's fold_expectations; plus the REQ-XA-006
        intended-vs-served arm fold over the emitted xcheck packet.
  RC-5  fixture ↔ docs byte equality — the byte-copy fixtures under
        crates/hh-env/tests/fixtures/{r5_49,p1}/ equal their
        docs/build/reports/ namesakes.
  RC-6  emission equality — each committed emitted packet is byte-exact
        canonical JSON and carries a self-consistent packet_id digest;
        with --emit-dir DIR a fresh emission is byte-compared.

Exit 0 all green, 1 at least one check failed, 2 a required input is
absent or malformed — a missing prerequisite is never a pass.
"""
import argparse
import hashlib
import json
import os
import sys

SCHEMA = "hh-p1-records-check/1"
PROV = "docs/build/reports/provenance/2026-11-06-scratch-drivers"
REPORTS = "docs/build/reports"
FIXTURES = "crates/hh-env/tests/fixtures"
MANIFEST_SHA256 = "f80859977e5a276603f0cedfb2c04c47193bddf577c8d27e5c9f355c03a863e5"
ARCHIVE_FILES = 71

# packet -> (raw epoch dir, prior packet whose slots carry prior_disposition)
S4_PACKETS = [
    ("2026-11-05-s4-confirmation.json", "s4-confirm", None),
    ("2026-11-05-s4-egress-cohort.json", "s4-egress",
     "2026-11-05-s4-confirmation.json"),
    ("2026-11-06-s4-completion.json", "s4-completion",
     "2026-11-05-s4-egress-cohort.json"),
    ("2026-11-06-s4-30slot-repairs.json", "s4-repairs",
     "2026-11-06-s4-completion.json"),
]
T3_PACKET = "2026-11-05-parity-tranche-3.json"
EPOCH_ORDER = ["s4-confirm", "s4-egress", "s4-completion", "s4-repairs"]
# members of a packet slots[] row that are not raw-record members:
# schedule/verifier literals and the prior packet's disposition.
NON_RECORD = {"slot_ref", "task_id", "arm_id", "round", "first", "lane_id",
              "prior_disposition", "supplement_epoch", "disposition"}
VERDICTS = {"resolved", "unresolved"}
# (file stem, declared schema)
EMITTED_PACKETS = [("p1-surface-xcheck", "hh-surface-xcheck/1"),
                   ("p1-estimators", "hh-p1-estimators/1")]

CHECKS = ["RC-1", "RC-2", "RC-3", "RC-4", "RC-5", "RC-6"]


class Missing(Exception):
    pass


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def rel(root, *parts):
    return os.path.join(root, *parts)


def read_json(root, path):
    full = rel(root, path)
    if not os.path.isfile(full):
        raise Missing(path)
    try:
        with open(full, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, json.JSONDecodeError) as e:
        raise Missing(f"{path} ({e})")


def read_bytes(root, path):
    full = rel(root, path)
    if not os.path.isfile(full):
        raise Missing(path)
    with open(full, "rb") as f:
        return f.read()


def read_sums(root, path):
    """Parse a sha256sum manifest: 'hash  relpath' lines → {relpath: hash}."""
    text = read_bytes(root, path).decode("utf-8")
    out = {}
    for line in text.splitlines():
        if not line.strip():
            continue
        h, _, p = line.partition("  ")
        if not p:
            h, _, p = line.partition(" ")
        p = p.strip().lstrip("./")
        out[p] = h.strip()
    return out


def sha256tree(root, tasks=None):
    """sha256tree/1 — sha256 over sorted '<file sha256>  <relpath>' lines."""
    lines = []
    if tasks is None:
        tasks = sorted(
            d for d in os.listdir(root)
            if os.path.isdir(os.path.join(root, d)))
    for task in sorted(tasks):
        tdir = os.path.join(root, task)
        if not os.path.isdir(tdir):
            raise Missing(f"dataset task dir {task}")
        for r, dirs, files in os.walk(tdir):
            dirs.sort()
            for fn in sorted(files):
                p = os.path.join(r, fn)
                rp = os.path.relpath(p, root)
                lines.append(sha256_file(p) + "  " + rp)
    lines.sort()
    return hashlib.sha256(("\n".join(lines) + "\n").encode()).hexdigest()


def canon(v):
    """Canonical JSON identical to hh-wire Json::write_canonical."""
    if v is None:
        return "null"
    if v is True:
        return "true"
    if v is False:
        return "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return '"' + esc(v) + '"'
    if isinstance(v, list):
        return "[" + ",".join(canon(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{" + ",".join(
            '"' + esc(k) + '":' + canon(val)
            for k, val in sorted(v.items())) + "}"
    raise ValueError(f"non-canonical value {v!r}")


def esc(s):
    out = []
    for c in s:
        if c == '"':
            out.append('\\"')
        elif c == "\\":
            out.append("\\\\")
        elif c == "\n":
            out.append("\\n")
        elif c == "\r":
            out.append("\\r")
        elif c == "\t":
            out.append("\\t")
        elif ord(c) < 0x20:
            out.append("\\u%04x" % ord(c))
        else:
            out.append(c)
    return "".join(out)


# ── RC-1: archive + raw hashes ─────────────────────────────────────────────
def rc1(root):
    detail = {}
    sums = read_sums(root, f"{PROV}/SHA256SUMS")
    if len(sums) != ARCHIVE_FILES:
        return "fail", f"manifest lines {len(sums)} != {ARCHIVE_FILES}"
    fdir = rel(root, PROV, "files")
    if not os.path.isdir(fdir):
        raise Missing(f"{PROV}/files/")
    on_disk = []
    for r, dirs, files in os.walk(fdir):
        dirs.sort()
        for fn in files:
            on_disk.append(
                os.path.relpath(os.path.join(r, fn), fdir))
    on_disk.sort()
    if sorted(sums) != on_disk:
        return "fail", "files/ content does not match the manifest set"
    bad = [p for p, h in sums.items()
           if sha256_file(os.path.join(fdir, p)) != h]
    if bad:
        return "fail", f"{len(bad)} file hash(es) mismatch (first {bad[0]})"
    actual = sha256_file(rel(root, PROV, "SHA256SUMS"))
    if actual != MANIFEST_SHA256:
        return "fail", f"manifest sha256 {actual} != {MANIFEST_SHA256}"
    readme = read_bytes(root, f"{PROV}/README.md").decode("utf-8")
    if MANIFEST_SHA256 not in readme:
        return "fail", "README does not carry the manifest sha256"
    idx = read_json(root, f"{PROV}/index.json")
    if idx.get("schema") != "hh-scratch-driver-archive/1":
        return "fail", "index.json schema is not hh-scratch-driver-archive/1"
    if idx.get("file_count") != ARCHIVE_FILES or \
            len(idx.get("files", [])) != ARCHIVE_FILES:
        return "fail", "index.json does not list the 71 files"
    for e in idx["files"]:
        p = e.get("path")
        if sums.get(p) != e.get("sha256"):
            return "fail", f"index.json sha256 mismatch at {p}"
        if e.get("bytes") != os.path.getsize(os.path.join(fdir, p)):
            return "fail", f"index.json size mismatch at {p}"
    if idx.get("source_archive", {}).get("manifest_sha256") != MANIFEST_SHA256:
        return "fail", "index.json manifest_sha256 mismatch"
    detail["archive"] = "71/71 verified; manifest sha256 matches README+index"
    # raw set
    rsums = read_sums(root, f"{PROV}/raw/SHA256SUMS")
    rdir = rel(root, PROV, "raw")
    bad = []
    for p, h in rsums.items():
        fp = os.path.join(rdir, p)
        if not os.path.isfile(fp) or sha256_file(fp) != h:
            bad.append(p)
    extra = []
    for r, dirs, files in os.walk(rdir):
        dirs.sort()
        for fn in files:
            rp = os.path.relpath(os.path.join(r, fn), rdir)
            if rp not in ("SHA256SUMS", "index.json") and rp not in rsums:
                extra.append(rp)
    if bad:
        return "fail", f"{len(bad)} raw hash(es) mismatch (first {bad[0]})"
    if extra:
        return "fail", f"{len(extra)} unmanifested raw file(s) (first {extra[0]})"
    ridx = read_json(root, f"{PROV}/raw/index.json")
    if ridx.get("schema") != "hh-p1-raw-records/1":
        return "fail", "raw/index.json schema is not hh-p1-raw-records/1"
    if ridx.get("total_files") != len(rsums):
        return "fail", "raw/index.json total_files != manifest"
    for e in ridx.get("not_committed", []):
        if e.get("committed") is not False:
            return "fail", "a not_committed entry claims committed=true"
    detail["raw"] = f"{len(rsums)}/{len(rsums)} verified; bulk labelled not committed"
    return "pass", detail


# ── RC-2: dataset content digests ─────────────────────────────────────────
def rc2(root, dataset):
    dig = read_json(root, f"{PROV}/dataset-digests.json")
    if dig.get("schema") != "hh-dataset-digests/1":
        return "fail", "dataset-digests.json schema is not hh-dataset-digests/1"
    if not dataset:
        return "skipped", ("dataset_absent", dig["sets"])
    t3 = read_json(root, f"{REPORTS}/{T3_PACKET}")
    tasks30 = sorted(p["task_id"] for p in t3["pairs"])
    want30 = dig["sets"]["tranche30"]["sha256tree"]
    want80 = dig["sets"]["catalogue80"]["sha256tree"]
    got30 = sha256tree(dataset, tasks30)
    got80 = sha256tree(dataset, None)
    if got30 != want30 or got80 != want80:
        return "fail", (f"dataset digests differ: 30={got30} 80={got80} "
                        f"(want {want30} / {want80})")
    return "pass", "both sha256tree/1 digests recomputed equal"


# ── RC-3: raw ↔ packet slots ──────────────────────────────────────────────
def rc3(root):
    conf = read_json(root, f"{REPORTS}/{S4_PACKETS[0][0]}")
    sched = {}
    try:
        for s in conf["manifest"]["schedule"]:
            sched[s["slot_ref"]] = s
    except (KeyError, TypeError):
        return "fail", "confirmation packet manifest.schedule missing"
    prior = {}
    checked = 0
    for name, epoch, prior_name in S4_PACKETS:
        pkt = read_json(root, f"{REPORTS}/{name}")
        raw = {}
        rdir = rel(root, PROV, "raw", epoch, "slots")
        if not os.path.isdir(rdir):
            raise Missing(f"{PROV}/raw/{epoch}/slots/")
        for fn in os.listdir(rdir):
            if fn.endswith(".json"):
                r = json.load(open(os.path.join(rdir, fn), encoding="utf-8"))
                raw[r["slot_ref"]] = r
        for row in pkt.get("slots", []):
            ref = row.get("slot_ref")
            s = sched.get(ref)
            if s is None:
                return "fail", f"{name}: slot_ref {ref} absent from schedule"
            for k in ("task_id", "arm_id", "round", "first", "lane_id"):
                if row.get(k) != s.get(k):
                    return "fail", f"{name}: {ref} {k} != schedule"
            if prior_name is not None:
                if row.get("prior_disposition") != prior.get(ref):
                    return "fail", f"{name}: {ref} prior_disposition wrong"
            r = raw.get(ref)
            if r is None:
                if row.get("disposition") != "unattempted":
                    return "fail", f"{name}: {ref} has no raw record but is not unattempted"
                continue
            if row.get("disposition") != r.get("disposition", "unattempted"):
                return "fail", f"{name}: {ref} disposition != raw record"
            for k, v in row.items():
                if k in NON_RECORD:
                    continue
                if k not in r:
                    return "fail", f"{name}: {ref} member {k} absent from raw record"
                if r[k] != v:
                    return "fail", f"{name}: {ref} member {k} != raw record"
            checked += 1
        prior = {row["slot_ref"]: row["disposition"] for row in pkt["slots"]}
    # tranche-3 pairs ↔ raw O/H records
    t3 = read_json(root, f"{REPORTS}/{T3_PACKET}")
    pairs_checked = 0
    for p in t3.get("pairs", []):
        task = p["task_id"]
        odir = f"{PROV}/raw/parity-tranche-3/o-side/{p['o_run_dir']}"
        res = read_json(root, f"{odir}/results.json")
        ent = next((e for e in res.get("results", [])
                    if e.get("task_id") == task), None)
        if ent is None:
            return "fail", f"no O results entry for {task} in {p['o_run_dir']}"
        if p["o_is_resolved"] != ent["is_resolved"]:
            return "fail", f"{task}: o_is_resolved != raw"
        if p["o_verdict"] != ("resolved" if ent["is_resolved"] else "unresolved"):
            return "fail", f"{task}: o_verdict != raw"
        if p["o_failure_mode"] != ent["failure_mode"]:
            return "fail", f"{task}: o_failure_mode != raw"
        if p["o_parser_results"] != ent.get("parser_results"):
            return "fail", f"{task}: o_parser_results != raw"
        h = read_json(root, f"{PROV}/raw/parity-tranche-3/h-side/{task}.h-side.json")
        for pk, hk in (("h_verdict", "verdict"), ("h_reason", "verdict_reason"),
                       ("h_parsed", "parsed"), ("h_image", "image"),
                       ("h_image_id_digest", "image_id_digest"),
                       ("h_image_preexisting", "image_preexisting"),
                       ("h_ansi_stripped", "ansi_stripped")):
            if p.get(pk) != h.get(hk):
                return "fail", f"{task}: {pk} != raw h-side record"
        pairs_checked += 1
    return "pass", f"{checked} packet slots + {pairs_checked} tranche pairs equal the raw records"


# ── RC-4: fold histograms (+ REQ-XA-006 arm fold) ─────────────────────────
def rc4(root):
    ridx = read_json(root, f"{PROV}/raw/index.json")
    exp = ridx.get("fold_expectations")
    if not isinstance(exp, dict):
        raise Missing("raw/index.json fold_expectations")
    recs = {}
    for i, d in enumerate(EPOCH_ORDER):
        rdir = rel(root, PROV, "raw", d, "slots")
        if not os.path.isdir(rdir):
            raise Missing(f"{PROV}/raw/{d}/slots/")
        for fn in sorted(os.listdir(rdir)):
            if not fn.endswith(".json"):
                continue
            r = json.load(open(os.path.join(rdir, fn), encoding="utf-8"))
            recs.setdefault(r["slot_ref"], []).append((i, r))
    mult = {}
    for v in recs.values():
        mult[str(len(v))] = mult.get(str(len(v)), 0) + 1
    conf = read_json(root, f"{REPORTS}/{S4_PACKETS[0][0]}")
    sched = {s["slot_ref"]: s for s in conf["manifest"]["schedule"]}
    first = [ref for ref, s in sched.items() if s.get("first")]
    later = sum(1 for ref in first
                if recs.get(ref) and max(recs[ref], key=lambda t: t[0])[0] != 0)
    backend = {"arm64_applehv": 0, "amd64_gce": 0}
    net = {"none": 0, "bridge": 0}
    for lst in recs.values():
        i, r = max(lst, key=lambda t: t[0])
        if i >= 2:
            backend["amd64_gce"] += 1
        else:
            backend["arm64_applehv"] += 1
        net["bridge" if r.get("cell_network") == "bridge" else "none"] += 1
    got = {
        "slot_multiplicity": mult,
        "round0_later_attempt": {"of": len(first), "count": later},
        "backend_final_fold": backend,
        "network_final_fold": net,
    }
    for k, want in exp.items():
        if k == "note":
            continue
        if got.get(k) != want:
            return "fail", f"{k}: recomputed {got.get(k)} != expected {want}"
    detail = dict(got)
    # REQ-XA-006: the emitted xcheck packet's policy_counts must equal a
    # recompute from its own slots[] (when that stage-2 input exists).
    emitted = _emitted_path(root, "p1-surface-xcheck")
    if emitted is None:
        raise Missing("emitted p1-surface-xcheck packet (stage-2 input)")
    x = json.loads(read_bytes(root, emitted).decode("utf-8"))
    pc = x.get("policy_counts")
    slots = x.get("slots")
    if not isinstance(pc, dict) or not isinstance(slots, list):
        return "fail", "xcheck packet lacks policy_counts/slots"
    arms = x.get("declared_arms") or {}
    want = {}
    for s in slots:
        arm = s["arm_id"]
        a = want.setdefault(arm, {"intended": {}, "served": {}})
        it = arms.get(arm)
        if it is not None:
            a["intended"][it] = a["intended"].get(it, 0) + 1
        sv = s.get("served_variant")
        if sv is not None:
            a["served"][sv] = a["served"].get(sv, 0) + 1
    if pc.get("arms") != want:
        return "fail", "policy_counts.arms != recompute from slots[]"
    if pc.get("analysis_basis") not in ("assignment", "served"):
        return "fail", "policy_counts.analysis_basis not named"
    detail["arm_counts"] = "policy_counts == fold over slots[]"
    return "pass", detail


def _emitted_path(root, stem):
    """docs/build/reports/<date>-<stem>.json — the single committed packet."""
    rdir = rel(root, REPORTS)
    hits = [f"{REPORTS}/{fn}" for fn in sorted(os.listdir(rdir))
            if fn.endswith(f"-{stem}.json")] if os.path.isdir(rdir) else []
    return hits[0] if hits else None


# ── RC-5: fixture ↔ docs byte equality ────────────────────────────────────
def rc5(root):
    pairs = []
    for fxdir in ("r5_49", "p1"):
        d = rel(root, FIXTURES, fxdir)
        if not os.path.isdir(d):
            continue
        for fn in sorted(os.listdir(d)):
            if fn.endswith(".json"):
                pairs.append((f"{FIXTURES}/{fxdir}/{fn}",
                              f"{REPORTS}/{fn}"))
    if not pairs:
        raise Missing("no fixture↔docs pairs found")
    n = 0
    for fx, doc in pairs:
        fb = read_bytes(root, fx)
        db = read_bytes(root, doc)
        if fb != db:
            return "fail", f"{fx} is not byte-equal to {doc}"
        n += 1
    return "pass", f"{n} fixture(s) byte-equal their committed docs namesakes"


# ── RC-6: emission equality ────────────────────────────────────────────────
def rc6(root, emit_dir):
    detail = {}
    for stem, schema in EMITTED_PACKETS:
        p = _emitted_path(root, stem)
        if p is None:
            raise Missing(f"emitted {stem} packet")
        raw = read_bytes(root, p)
        try:
            obj = json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError as e:
            return "fail", f"{stem}: not parseable JSON ({e})"
        if obj.get("schema") != schema:
            return "fail", f"{stem}: schema member mismatch"
        if raw != (canon(obj) + "\n").encode("utf-8"):
            return "fail", f"{stem}: committed bytes are not canonical emission bytes"
        pid = obj.get("packet_id")
        body = {k: v for k, v in obj.items() if k != "packet_id"}
        want = "sha256:" + hashlib.sha256(canon(body).encode()).hexdigest()
        if pid != want:
            return "fail", f"{stem}: packet_id {pid} != recomputed {want}"
        detail[stem] = "canonical + self-consistent packet_id"
    if emit_dir:
        for stem, _schema in EMITTED_PACKETS:
            committed = read_bytes(root, _emitted_path(root, stem))
            fresh = None
            for fn in os.listdir(emit_dir):
                if fn.endswith(f"-{stem}.json") or fn == f"{stem}.json":
                    fresh = read_bytes(emit_dir, fn)
            if fresh is None:
                raise Missing(f"re-emitted {stem} under --emit-dir")
            if fresh != committed:
                return "fail", f"{stem}: re-emission is not byte-equal to the committed packet"
            detail[f"{stem}:reemit"] = "byte-equal to fresh emission"
    return "pass", detail


def main():
    ap = argparse.ArgumentParser(description="hh-p1-records-check/1 verifier")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--root", default=None,
                    help="check root (default: the repository root)")
    ap.add_argument("--dataset", default=None,
                    help="dataset tree for RC-2 recompute")
    ap.add_argument("--emit-dir", default=None,
                    help="fresh emission directory for RC-6 byte equality")
    ap.add_argument("--only", default=None, choices=CHECKS)
    args = ap.parse_args()
    if not args.check:
        ap.print_help()
        return 2
    root = args.root or os.path.dirname(os.path.dirname(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    if args.root and not os.path.isabs(args.root):
        root = os.path.join(os.getcwd(), args.root)
    checks = [args.only] if args.only else CHECKS
    results = []
    for cid in checks:
        try:
            if cid == "RC-1":
                verdict, detail = rc1(root)
            elif cid == "RC-2":
                verdict, detail = rc2(root, args.dataset)
            elif cid == "RC-3":
                verdict, detail = rc3(root)
            elif cid == "RC-4":
                verdict, detail = rc4(root)
            elif cid == "RC-5":
                verdict, detail = rc5(root)
            else:
                verdict, detail = rc6(root, args.emit_dir)
        except Missing as e:
            verdict, detail = "missing", str(e)
        except Exception as e:  # malformed input is a missing prerequisite
            verdict, detail = "missing", f"{cid} error: {e}"
        results.append({"check": cid, "verdict": verdict, "detail": detail})
    failed = any(r["verdict"] == "fail" for r in results)
    missing = any(r["verdict"] == "missing" for r in results)
    report = {
        "schema": SCHEMA,
        "tool": "docs/build/tools/p1_records.py",
        "root": os.path.relpath(root) if os.path.isabs(root) else root,
        "checks": results,
        "verdict": ("missing-input" if missing else
                    ("fail" if failed else "pass")),
    }
    print(json.dumps(report, indent=1, sort_keys=True))
    if missing:
        return 2
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
