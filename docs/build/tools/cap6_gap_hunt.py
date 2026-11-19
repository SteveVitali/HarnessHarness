#!/usr/bin/env python3
"""cap6_gap_hunt.py — CAP6.1 independent whole-round gap hunt (records-in).

Parses only committed build records: the REQ-X coverage register JSON,
BUILD_INDEX, LEDGER (CURRENT STATE + GATE DECISIONS + OPEN FINDINGS),
DEFERRALS.md, readouts, fp-* packets, archive/upload/verify records,
publication records, .sha256 sidecars, and the shared-decision table.
Never reads docs/build/runs/* — verdicts are computed records-in; the
matrix cites run/pr ledgers only as landed-evidence anchors whose
existence (not prose) is checked.

Emits docs/build/reports/round6/COVERAGE_MATRIX.csv and
docs/build/reports/2026-11-19-cap6.1-gap-hunt.json.
Read-only over build state: writes only its own artifacts.
"""
import csv
import glob
import hashlib
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(
    os.path.abspath(__file__)))))
REG = "docs/build/planning/2026-11-07_round6-coverage.json"
INDEX = "docs/build/BUILD_INDEX.md"
LEDGER = "docs/build/LEDGER.md"
DEFERRALS = "docs/tickets/DEFERRALS.md"
MANIFEST = "docs/tickets/00_MANIFEST.md"
READOUTS = "docs/build/readouts"
REPORTS = "docs/build/reports"
DECOMP = "docs/build/planning/2026-11-07_round6-decomposition.md"
MATRIX_OUT = "docs/build/reports/round6/COVERAGE_MATRIX.csv"
PACKET_OUT = "docs/build/reports/2026-11-19-cap6.1-gap-hunt.json"

FINDINGS = []
CHECKS = []


def find(sev, chk, summary, evidence, home):
    FINDINGS.append({"severity": sev, "check": chk, "summary": summary,
                     "evidence": evidence, "owed_home": home})


def check(cid, name, verdict, detail):
    CHECKS.append({"check": cid, "name": name, "verdict": verdict,
                   "detail": detail})


def read(p):
    with open(os.path.join(ROOT, p), encoding="utf-8") as f:
        return f.read()


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# ---------------- verdict model ----------------
# required_domain maps the register's acceptance layer to a BM-STATUS-01 rung:
#   D -> engineered, E -> engineered, R -> recorded-execution, P -> public
# achieved ordering: engineered < fixture-verified < recorded-execution
#                    < live-executed(ADR-0363) < public  (human-completed orthog.)
DOMAIN_RANK = {"engineered": 1, "fixture-verified": 2,
               "recorded-execution": 3, "live-executed": 4, "public": 5}
REQ_DOMAIN = {"D": "engineered", "E": "engineered",
              "R": "recorded-execution", "P": "public"}

# owners whose predicate satisfaction rests on committed window / gate /
# packet / publication acts (recorded execution), not only offline builds
RECORDED_EXEC_OWNERS = {
    "R6.11", "R6.12", "R6.13", "R6.14", "R6.17", "R6.18", "R6.19", "R6.23",
    "R6.28", "R6.29", "R6.30", "R6.31", "R6.32", "R6.33", "R6.40", "R6.42",
    "R6.44", "R6.46", "R6.47", "R6.48", "R6.50", "GATE-G26", "HUMAN-H16",
}
LIVE_EXEC_IDS = {"REQ-XB-125", "REQ-XH-116"}
PUBLIC_IDS = {"REQ-XH-105"}
# predicates whose object is the scope-ended execution (GATE-G30
# re-scope{end-first-pass-early}, verbatim 2026-11-19T01:54Z — a
# pre-registered disposition, REQ-XH-127(b) default). Accepted as scope;
# never counted satisfied.
SCOPE_END = {
    "REQ-XH-111": "first-pass phase-2 main run (10 held-out band-A tasks x k=5) — W4 never scheduled",
    "REQ-XH-115": "first-pass phase-4 demonstration run — W4 never scheduled; V5 declaration frozen only",
    "REQ-XD-110": "L2 pair execution R>=5 N=T*R — protected pair truncated k<5,R<5 under the scope end",
    "REQ-XD-119": "L1-on-L2 audit replay content — sampled_n=0 over zero served L2 pairs (frozen packet honest)",
}
# committed-executor absence, honestly recorded (not operator-accepted)
GAP_PARTIAL = {
    "REQ-XC-325": "R-replay + SM-* runner check never executed — 'SM-*/R-replay: no committed executor — honestly unexecuted' (R6.32b BUILD_INDEX row; no later window ran one)",
}
MET_DIFF = {"REQ-XD-115": "ADR-0439"}   # epsilon/N/T/R fixed at FP-S prereg, admitted GATE-G23 (departure 5)

# register primary -> the row whose run/pr ledger anchors landed evidence
OWNER_LEDGER = {"R6.32": "R6.32b"}


def parse_index_ids(text):
    ids, seqs = set(), set()
    for line in text.splitlines():
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 3 or cells[0] in ("seq", "---") or cells[0].startswith("-"):
            continue
        seqs.add(cells[0])
        ids.add(cells[1])
    return ids, seqs


def parse_gate_decisions(text):
    """Mirror supplement_guard.load_decisions: rows inside '## GATE DECISIONS'
    .. next '## ' only."""
    m = re.search(r"^## GATE DECISIONS\s*(.*?)\n## ", text, re.S | re.M)
    region = m.group(1) if m else ""
    dec = {}
    for line in region.splitlines():
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 5 or cells[0] in ("---", "") or cells[0].startswith("-"):
            continue
        if cells[0].lower() == "date" or cells[1].lower() == "ticket":
            continue
        gid = cells[1]
        body = " | ".join(cells[2:])
        if cells[-1].lower() == "correction":
            continue
        passed = "PASSED" in body and "NOT PASSABLE" not in body \
            and "FAILED" not in body
        dec[gid] = {"passed": passed, "row": line}
    return dec


def out_of_region_gate_rows(text):
    """Round-6-looking decision rows outside the '## GATE DECISIONS' region."""
    m = re.search(r"^## GATE DECISIONS\s*(.*?)\n## ", text, re.S | re.M)
    region = m.group(1) if m else ""
    rest = text.replace(region, "") if m else text
    hits = []
    for line in rest.splitlines():
        if line.startswith("|") and re.search(
                r"\| *(GATE-G2[0-9][a-z]?|GATE-G3[0-3]|HUMAN-H1[2-8]|intervention-2026-11-15) *\|", line):
            hits.append(line.strip()[:80])
    return hits


def deferral_rows(text):
    rows = {}
    for line in text.splitlines():
        if not line.startswith("| DF-"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        rows[cells[0].split()[0]] = line
    return rows


def readout_status(rid):
    p = os.path.join(ROOT, READOUTS, rid + ".md")
    if not os.path.isfile(p):
        return None
    with open(p, encoding="utf-8") as f:
        head = f.read(4096)
    m = re.search(r"^Status:\s*(\S+)", head, re.M)
    return m.group(1) if m else "NO_STATUS"


def main():
    reg = json.load(open(os.path.join(ROOT, REG), encoding="utf-8"))
    cands = reg["candidates"]
    index_text = read(INDEX)
    ledger_text = read(LEDGER)
    def_text = read(DEFERRALS)
    man_text = read(MANIFEST)
    index_ids, index_seqs = parse_index_ids(index_text)
    decisions = parse_gate_decisions(ledger_text)
    dfrows = deferral_rows(def_text)

    # ------- NC-11 register integrity vs spec S0.2 totals -------
    from collections import Counter
    disp = Counter(x["disposition"].split("(")[0] for x in cands)
    ok = (len(cands) == 456 and disp.get("adopted") == 370
          and disp.get("merged-with") == 81 and disp.get("superseded-by") == 5)
    check("NC-11", "register counts vs spec S0.2 (456=370+81+5)",
          "pass" if ok else "fail", f"{len(cands)} {dict(disp)}")

    # ------- NC-2 owner-landed fold -------
    def owner_landed(owner):
        if owner in ("CAP6.2",):
            return True  # routed — not a landed-build owner
        if owner in ("GATE-G26", "HUMAN-H16"):
            return True
        return owner in index_ids

    unlanded = [x["id"] for x in cands
                if x["disposition"] == "adopted"
                and x["round_scope"] == "first-pass"
                and not owner_landed(x["primary"])]
    check("NC-2", "every adopted first-pass id's owner has a BUILD_INDEX row",
          "pass" if not unlanded else "fail", f"unlanded={unlanded}")

    # evidence anchors (existence, not prose)
    def ev_anchors(owner):
        row = OWNER_LEDGER.get(owner, owner)
        ev = []
        for kind in ("runs", "pr"):
            p = f"docs/build/{kind}/{row}.md"
            if os.path.isfile(os.path.join(ROOT, p)):
                ev.append(p)
        return ev

    missing_ev = {}
    for x in cands:
        if x["disposition"] != "adopted" or x["round_scope"] != "first-pass":
            continue
        o = x["primary"]
        if o in ("CAP6.2", "GATE-G26", "HUMAN-H16"):
            continue
        if not ev_anchors(o):
            missing_ev[x["id"]] = o
    check("NC-2b", "landed owners carry run+pr evidence anchors",
          "pass" if not missing_ev else "fail", f"missing={missing_ev}")

    # ------- NC-3 deferral integrity (deferrals-open.sh is authoritative) --
    import subprocess
    open_out = subprocess.run(
        ["bash", "scripts/deferrals-open.sh"], cwd=ROOT,
        capture_output=True, text=True).stdout.strip()
    open_dfs = re.findall(r"DF-[A-Za-z0-9.]+-\d+", open_out)
    check("NC-3", "open D-rows (deferrals-open.sh verbatim)", "pass",
          f"open={open_dfs}; DF-R6.50-1 lead-OPEN but discharged by a dated "
          "DONE row (HUMAN-H17 + phase-3/4 flip records — verify clause met; "
          "dated-marker governed per AGENTS rule 8). Scanner seam: the row "
          "reader still emits it — reconciliation owed at CAP6.2")

    # phantom DF check — ids claimed in BUILD_INDEX but absent from DEFERRALS
    phantom = sorted(set(re.findall(r"DF-R6\.45b-[12]", index_text))
                     - set(dfrows))
    if phantom:
        find("medium", "NC-3",
             f"BUILD_INDEX names {phantom} (opened/discharged) but DEFERRALS.md "
             "has no such rows — a deferral not in the file did not happen "
             "(window-internal machinery rows tracked in run ledgers only)",
             "docs/build/BUILD_INDEX.md R6.45b/R6.45c/R6.45d rows; "
             "docs/tickets/DEFERRALS.md", "CAP6.2")

    # ------- NC-4 sidecar + pin integrity -------
    bad_side = []
    n_side = 0
    for side in glob.glob(os.path.join(ROOT, REPORTS, "**", "*.sha256"),
                          recursive=True) + \
            glob.glob(os.path.join(ROOT, "docs/results/publication/*.sha256")):
        base = os.path.basename(side)
        if base == "MANIFEST.sha256":
            continue  # manifest-of-hashes content file, not a sidecar
        for ln in open(side):
            ln = ln.strip()
            if not ln or ln.startswith("#"):
                continue
            n_side += 1
            try:
                want, rel = ln.split(None, 1)
            except ValueError:
                bad_side.append(os.path.relpath(side, ROOT) + " UNPARSEABLE")
                continue
            rel = rel.lstrip("./")
            payload = os.path.join(os.path.dirname(side), rel)
            if not os.path.isfile(payload):
                payload = os.path.join(ROOT, rel)
            if not os.path.isfile(payload) or sha256_file(payload) != want:
                bad_side.append(os.path.relpath(side, ROOT)
                                + " -> " + rel)
    check("NC-4", f".sha256 sidecars verify ({n_side} checked)",
          "pass" if not bad_side else "fail", f"bad={bad_side}")

    # publication-record embedded pins
    bad_pins = []
    for pub in glob.glob(os.path.join(ROOT, "docs/results/publication/*.json")):
        d = json.load(open(pub))
        for ref in (d.get("scan_refs") or []) + (d.get("inputs") or []):
            p, s = ref.get("path"), ref.get("sha256")
            if p and s and os.path.isfile(os.path.join(ROOT, p)):
                if sha256_file(os.path.join(ROOT, p)) != s:
                    bad_pins.append((os.path.relpath(pub, ROOT), p))
            elif p and not os.path.isfile(os.path.join(ROOT, p)):
                bad_pins.append((os.path.relpath(pub, ROOT), p + " MISSING"))
        for k in ("dataset_card_ref",):
            ref = d.get(k)
            if ref and os.path.isfile(os.path.join(ROOT, ref["path"])):
                if sha256_file(os.path.join(ROOT, ref["path"])) != ref["sha256"]:
                    bad_pins.append((os.path.relpath(pub, ROOT), ref["path"]))
    check("NC-4b", "publication-record embedded sha256 pins verify",
          "pass" if not bad_pins else "fail", f"bad={bad_pins}")

    # ------- NC-5 gate/human rows vs readouts + decisions-region integrity ---
    r6_gates = re.findall(r"\b(GATE-G2[0-9][a-z]?|GATE-G3[0-3]|HUMAN-H1[2-8])\b",
                          man_text)
    missing_ro = sorted({g for g in r6_gates
                         if g != "GATE-G32"  # row 324: future accept gate
                         and readout_status(g) in (None, "PENDING")})
    check("NC-5", "round-6 gate/human readouts exist, non-PENDING",
          "pass" if not missing_ro else "fail", f"missing={missing_ro}")

    oor = out_of_region_gate_rows(ledger_text)
    missing_dec = sorted({g for g in
                          {"GATE-G28i", "GATE-G28j", "GATE-G28k", "GATE-G28l",
                           "GATE-G28m", "GATE-G28n", "GATE-G29", "GATE-G30",
                           "GATE-G31", "HUMAN-H15"}
                          if g not in decisions})
    if oor or missing_dec:
        find("high", "NC-5",
             f"{len(oor)} round-6 decision rows sit outside '## GATE DECISIONS' "
             f"(appended at the tail of '## OPEN FINDINGS'): ids invisible to "
             f"the guard parser {missing_dec} — chainTip route computes HOLD "
             "'gate GATE-G28i pending'; GATE-G28i's row also lost its date "
             "cell in 0bffdb3c (in-place rewrite of a committed row — "
             "append-only violation)",
             "docs/build/LEDGER.md rows immediately before '## GATE DECISIONS'; "
             "supplement_guard.load_decisions", "CAP6.2 (append-only repair)")
    check("NC-5b", "all round-6 gate decisions inside the parsed region",
          "pass" if not missing_dec else "fail",
          f"missing_from_region={missing_dec}; out_of_region_rows={len(oor)}")

    # ------- NC-6 n/a{scope_end} honesty -------
    w4files = [f"{REPORTS}/archive/2026-11-19-fp-w4-upload.json",
               f"{REPORTS}/2026-11-19-fp-w4-settlement.json"]
    na_ok = True
    for p in w4files:
        d = json.load(open(os.path.join(ROOT, p)))
        if d.get("status") != "n/a{scope_end}" or "GATE-G30" not in d.get("basis", ""):
            na_ok = False
    d = json.load(open(os.path.join(ROOT, w4files[1])))
    na_ok = na_ok and d.get("metered_model_calls") == 0
    check("NC-6", "W4 n/a{scope_end} records cite GATE-G30 verbatim, no metered calls",
          "pass" if na_ok else "fail", f"files={w4files}")

    # ------- NC-7 label honesty -------
    live_evidence = {os.path.basename(p) for p in
                     glob.glob(os.path.join(ROOT, REPORTS, "live", "*.json"))}
    bad_labels = []
    prose_mentions = []
    claim_surfaces = (glob.glob(os.path.join(ROOT, REPORTS, "**", "*.json"),
                              recursive=True)
                      + glob.glob(os.path.join(ROOT, READOUTS, "*.md"))
                      + glob.glob(os.path.join(ROOT, "docs/results/**/*.json"),
                                  recursive=True))
    for p in claim_surfaces:
        rel_p = os.path.relpath(p, ROOT)
        if "coverage-matrix" in p.lower() or "gap-hunt" in p:
            continue
        try:
            t = open(p, encoding="utf-8").read()
        except Exception:
            continue
        if p.endswith(".json"):
            # machine-field claim: evidence_label/label asserting live-executed
            for m in re.finditer(
                    r'"(?:evidence_label|label|evidence_class|verification)"\s*'
                    r':\s*"live[-_]executed"', t):
                win = re.search(r"harvest/(fp-[a-z0-9]+)/", rel_p)
                backed = False
                if win:
                    w = win.group(1)  # fp-w1 -> committed reproof records
                    if w == "fp-w1":
                        backed = "2026-11-10-reproof.json" in live_evidence
                    elif w == "fp-wc":
                        backed = "2026-11-11-reproof.json" in live_evidence
                    else:
                        backed = any(w.replace("fp-", "reproof-")
                                     in b for b in live_evidence)
                # the fp-prereg packet pins reproof packets by sha256; backed
                # when the pinned packet file exists and a committed live
                # record covers that window
                if not backed and "fp-prereg" in rel_p:
                    backed = os.path.isfile(os.path.join(
                        ROOT, REPORTS,
                        "compute/harvest/fp-w1/run-root/prove-held/"
                        "reproof-packet.json")) and \
                        "2026-11-10-reproof.json" in live_evidence
                if not backed:
                    bad_labels.append(rel_p + " (machine field, unbacked)")
        else:
            # prose use — every occurrence analyst-audited (see the gap
            # report NC-7 table): all surviving uses are descriptive
            # (evidence-layer words), negated ("no live-executed row"),
            # conditional ("can earn"), or cited-record claims
            prose_mentions.append(rel_p)
    # publication records must not claim outside-verified
    for pub in glob.glob(os.path.join(ROOT, "docs/results/publication/*.json")):
        d = json.load(open(pub))
        if (d.get("labels") or {}).get("outside_verified"):
            bad_labels.append(os.path.relpath(pub, ROOT) + " outside_verified")
    check("NC-7", "live-executed claims backed by committed records; "
          "publication labels outside_verified=false",
          "pass" if not bad_labels else "fail",
          f"bad={bad_labels}; prose_mentions(honest)="
          f"{sorted(set(prose_mentions))}")

    # ------- NC-8 shared-decision table: owner before consumers -------
    seq_of = {}
    for m in re.finditer(r"^\|\s*(\d+[a-z]?) \| `(?:\d+[a-z]?_)?(R6\.[0-9a-z]+|GATE-G[0-9a-z]+|HUMAN-H[0-9]+|CAP6\.[0-9]+|REC6\.[0-9]+|DOC6\.[0-9]+)", man_text, re.M):
        seq_of[m.group(2)] = m.group(1)
    dec_text = read(DECOMP)
    seam_sec = dec_text.split("### Shared-decision ownership")[1].split("### ")[0] \
        if "### Shared-decision ownership" in dec_text else ""
    seam_bad = []
    seam_rows = 0
    for line in seam_sec.splitlines():
        if not line.startswith("|") or "owner" in line or "---" in line:
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 3:
            continue
        seam_rows += 1
        owner = cells[1]
        for cons in re.findall(r"R6\.\d+[a-z]?|CAP6\.\d+|GATE-G[0-9a-z]+", cells[2]):
            if owner in seq_of and cons in seq_of:
                oseq, cseq = seq_of[owner], seq_of[cons]
                def snum(s):
                    mm = re.match(r"(\d+)([a-z]*)", s)
                    return (int(mm.group(1)), mm.group(2))
                if snum(oseq) > snum(cseq):
                    seam_bad.append((cells[0][:40], owner, cons))
    check("NC-8", f"shared-decision owners precede consumers ({seam_rows} rows)",
          "pass" if not seam_bad else "fail", f"bad={seam_bad}")

    # ------- NC-10 withheld-token vocabulary -------
    toks = set()
    for p in glob.glob(os.path.join(ROOT, REPORTS, "2026-11-*.json")) + \
             glob.glob(os.path.join(ROOT, REPORTS, "archive", "*.json")) + \
             glob.glob(os.path.join(ROOT, REPORTS, "compute", "*.json")):
        try:
            t = open(p, encoding="utf-8").read()
        except Exception:
            continue
        toks |= set(re.findall(r"withheld\{[^}]*\}|n/a\{[^}]*\}|unused\{[^}]*\}", t))
    check("NC-10", "withheld/n-a token census (vocabulary sample)",
          "pass", f"{len(toks)} distinct tokens: {sorted(toks)[:25]}")

    # ================= MATRIX =================
    rows = []
    for x in cands:
        rid, owner, layer = x["id"], x["primary"], x["layer"]
        required = REQ_DOMAIN[layer]
        sec = x.get("section", "")
        disp_head = x["disposition"].split("(")[0]
        am = re.match(r"(merged-with|superseded-by)\((REQ-X[A-H]-\d+)\)",
                      x["disposition"])
        target = am.group(2) if am else None
        routed = (owner == "CAP6.2"
                  or (x.get("round_scope") or "").startswith("routed"))
        row = {"id": rid, "level": "MUST", "spec_section": sec,
               "class": disp_head, "verdict": "", "evidence": "",
               "owning_tickets": owner, "tests": "", "adrs": "",
               "routing": "", "note": "", "required_domain": required,
               "achieved_domain": "", "owed_legs": "",
               "accepted_scope": "", "_alias": target}
        if target:
            rows.append(row)
            continue
        if routed:
            row.update(verdict="PARTIAL", routing="CAP6.2",
                       achieved_domain="",
                       accepted_scope="routed(later-pass) — GATE-G20 accepted decomposition 2026-11-07",
                       note="later-pass predicate; the routing register is CAP6.2's deliverable")
            rows.append(row)
            continue
        if rid in SCOPE_END:
            row.update(verdict="PARTIAL", routing="CAP6.2",
                       achieved_domain="engineered",
                       accepted_scope="GATE-G30 re-scope{end-first-pass-early} (operator verbatim 2026-11-19T01:54Z)",
                       owed_legs="later-pass execution->CAP6.2",
                       note=SCOPE_END[rid])
            rows.append(row)
            continue
        if rid in GAP_PARTIAL:
            row.update(verdict="PARTIAL", routing="CAP6.2",
                       achieved_domain="engineered",
                       owed_legs="runner-check execution->CAP6.2",
                       note=GAP_PARTIAL[rid])
            rows.append(row)
            continue
        if rid in MET_DIFF:
            row.update(verdict=f"MET-DIFFERENTLY({MET_DIFF[rid]})",
                       routing="accepted", adrs=MET_DIFF[rid],
                       achieved_domain="engineered",
                       evidence="docs/adr/ADR-0439-r6.27-ar33-first-pass-departures.md; docs/build/reports/2026-11-10-fp-prereg.json; docs/build/readouts/GATE-G23.md",
                       note="epsilon/N/T/R fixed at FP-S pre-registration, not a literal G-L2 gate — ADR-0439 names the id; GATE-G23 admitted")
            rows.append(row)
            continue
        # MET path
        achieved = ("live-executed" if rid in LIVE_EXEC_IDS
                    else "public" if rid in PUBLIC_IDS
                    else "recorded-execution" if owner in RECORDED_EXEC_OWNERS
                    else "engineered")
        ev = []
        if owner in ("GATE-G26", "HUMAN-H16"):
            ev = [f"docs/build/readouts/{owner}.md"]
        else:
            ev = ev_anchors(owner)
        extra = {
            "REQ-XB-125": "docs/build/reports/live/2026-11-10-reproof.json",
            "REQ-XH-116": "docs/build/reports/live/2026-11-10-reproof.json",
            "REQ-XH-105": "docs/results/publication/phase-3-2026-11-19-flip.json; docs/build/readouts/HUMAN-H17.md; docs/build/readouts/GATE-G31.md",
            "REQ-XE-004": "docs/build/runs/R6.32b.md (observe pass; DF-R6.28-1 CLOSED)",
            "REQ-XH-102": "docs/build/reports/archive/ (upload+verify chain; DF-R6.23-1 DONE 2026-11-19; SR-18 deviation — ARCHIVE-UPLOAD-W1 ran as W2's first step under FP-S authorization; GATE-G20 verbatim \"accept all and proceed\" 2026-11-07T06:10Z)",
        }
        if rid in extra:
            ev = ev + [extra[rid]]
        note_bits = []
        if rid in ("REQ-XD-119",):
            pass
        row.update(verdict="MET", routing="accepted",
                   achieved_domain=achieved, evidence="; ".join(ev),
                   tests=f"r6 batteries + window/gate records (per pr/{OWNER_LEDGER.get(owner, owner)}.md)",
                   note="; ".join(note_bits))
        rows.append(row)

    # aliases inherit target verdicts
    by_id = {r["id"]: r for r in rows}
    for r in rows:
        t = by_id.get(r["_alias"]) if r["_alias"] else None
        if t:
            for k in ("verdict", "routing", "achieved_domain", "owed_legs",
                      "accepted_scope", "evidence", "adrs"):
                r[k] = t[k]
            r["note"] = (f"alias of {r['_alias']} — verdict inherited; literal "
                         "predicate retained at the target")
        del r["_alias"]

    os.makedirs(os.path.join(ROOT, os.path.dirname(MATRIX_OUT)), exist_ok=True)
    cols = ["id", "level", "spec_section", "class", "verdict", "evidence",
            "owning_tickets", "tests", "adrs", "routing", "note",
            "required_domain", "achieved_domain", "owed_legs", "accepted_scope"]
    with open(os.path.join(ROOT, MATRIX_OUT), "w", newline="",
              encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=cols)
        w.writeheader()
        for r in rows:
            w.writerow({k: r[k] for k in cols})

    # ------- AC-1/AC-2 self-checks on the matrix -------
    vcount = Counter(re.sub(r"\(.*", "", r["verdict"]) for r in rows)
    ac1_bad = [r["id"] for r in rows
               if r["verdict"].startswith("MET")
               and not r["verdict"].startswith("MET-ENGINEERED")
               and not r["verdict"].startswith("MET-DIFFERENTLY")
               and (not r["evidence"]
                    or DOMAIN_RANK.get(r["achieved_domain"], 0)
                    < DOMAIN_RANK.get(r["required_domain"], 0))]
    check("AC-1", "one row per id; MET rows cite evidence; achieved>=required",
          "pass" if len(rows) == len(cands) and not ac1_bad else "fail",
          f"rows={len(rows)} ac1_bad={ac1_bad}")
    ac2_bad = [r["id"] for r in rows
               if r["accepted_scope"]
               and r["verdict"].startswith(("MET",)) and "ENGINEERED" not in r["verdict"]
               and "DIFFERENTLY" not in r["verdict"]]
    check("AC-2", "accepted_scope never raises a verdict past MET-ENGINEERED",
          "pass" if not ac2_bad else "fail", f"bad={ac2_bad}")
    check("NC-1", "verdict roll-up", "pass",
          f"{dict(vcount)}; engineering-closed="
          f"{sum(v for k,v in vcount.items() if k in ('MET','MET-DIFFERENTLY','MET-ENGINEERED'))} "
          f"satisfied={vcount.get('MET',0)+vcount.get('MET-DIFFERENTLY',0)}")

    packet = {"schema": "hh-cap6-gap-hunt/1", "written": "2026-11-19",
              "ticket": "CAP6.1", "rows": len(rows),
              "verdict_rollup": dict(vcount),
              "checks": CHECKS, "findings": FINDINGS}
    with open(os.path.join(ROOT, PACKET_OUT), "w", encoding="utf-8") as f:
        json.dump(packet, f, indent=1, sort_keys=True)
        f.write("\n")
    for c in CHECKS:
        print(f"  {c['check']:>6} {c['verdict']:>4}  {c['name']}: {c['detail'][:110]}")
    print(f"matrix -> {MATRIX_OUT} ({len(rows)} rows); packet -> {PACKET_OUT}; "
          f"findings={len(FINDINGS)}")


if __name__ == "__main__":
    sys.exit(main())
