#!/usr/bin/env python3
"""cap5_gap_hunt.py — CAP5.1 independent whole-scope gap hunt (records-in).

Parses only committed build records (round-5 packets, LEDGER gate decisions +
phase log, readouts, DEFERRALS, the coverage register, BUILD_INDEX, PR bodies,
ticket contracts). Cross-checks declared dispositions vs leg folds, gate
decisions vs landed records, cited DF-ids vs DEFERRALS.md, coverage anchors vs
contract anchors, and owed-work homes. Emits
`docs/build/reports/2026-11-04-cap5.1-gap-hunt.json` plus, with --matrix,
`docs/build/reports/round5/COVERAGE_MATRIX.csv` (BM matrix columns).
Read-only over build state: writes only its own artifacts.
"""
import argparse
import csv
import glob
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
REPORTS = "docs/build/reports"
COVERAGE_MD = "docs/build/planning/2026-10-30_evidence-coverage.md"
DEFERRALS = "docs/tickets/DEFERRALS.md"
LEDGER = "docs/build/LEDGER.md"
BUILD_INDEX = "docs/build/BUILD_INDEX.md"
MANIFEST = "docs/tickets/00_MANIFEST.md"
TICKETS = "docs/tickets"
READOUTS = "docs/build/readouts"
PR_DIR = "docs/build/pr"
LIVE_DIR = "docs/build/reports/live"
MATRIX_OUT = "docs/build/reports/round5/COVERAGE_MATRIX.csv"
PACKET_OUT = "docs/build/reports/2026-11-04-cap5.1-gap-hunt.json"

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


EXPECTED_PACKETS = [
    "2026-11-03-r5.40-sim-battery.json",
    "2026-11-03-r5.42a-backend-preflight.json",
    "2026-11-04-r5.42a-backend-preflight-reprobe.json",
    "2026-11-04-r5.43-s1-useful.json",
    "2026-11-04-r5.44-ops-live-first.json",
    "2026-11-04-r5.44-ops-live-second.json",
    "2026-11-04-r5.46-activation-podman.json",
    "2026-11-04-r5.46-pilot-podman.json",
    "2026-11-04-r5.47-parity-n0.json",
    "2026-11-04-r5.48-confirm-withheld.json",
    "2026-11-04-r5.49-study-report.json",
    "2026-11-04-r5.50-plugin-unselected.json",
    "2026-11-04-r5.51-interchange-unselected.json",
    "2026-11-04-r5.52-outside-withheld.json",
    "2026-11-04-r5.53-formative-withheld.json",
    "2026-11-04-r5.54-publication-withheld.json",
    "2026-11-04-r5.55-value-withheld.json",
]

# ticket id -> packet basename(s) its evidence record carries
TICKET_PACKETS = {
    "R5.42a": ["2026-11-03-r5.42a-backend-preflight.json",
               "2026-11-04-r5.42a-backend-preflight-reprobe.json"],
    "R5.43": ["2026-11-04-r5.43-s1-useful.json"],
    "R5.44": ["2026-11-04-r5.44-ops-live-first.json",
              "2026-11-04-r5.44-ops-live-second.json"],
    "R5.46": ["2026-11-04-r5.46-activation-podman.json",
              "2026-11-04-r5.46-pilot-podman.json"],
    "R5.47": ["2026-11-04-r5.47-parity-n0.json"],
    "R5.48": ["2026-11-04-r5.48-confirm-withheld.json"],
    "R5.49": ["2026-11-04-r5.49-study-report.json"],
    "R5.50": ["2026-11-04-r5.50-plugin-unselected.json"],
    "R5.51": ["2026-11-04-r5.51-interchange-unselected.json"],
    "R5.52": ["2026-11-04-r5.52-outside-withheld.json"],
    "R5.53": ["2026-11-04-r5.53-formative-withheld.json"],
    "R5.54": ["2026-11-04-r5.54-publication-withheld.json"],
    "R5.55": ["2026-11-04-r5.55-value-withheld.json"],
}

PACKET_STATE = {
    "R5.42a": "inconclusive", "R5.43": "failed", "R5.44": "executed",
    "R5.46": "executed", "R5.47": "withheld", "R5.48": "withheld",
    "R5.49": "reported", "R5.50": "unselected", "R5.51": "unselected",
    "R5.52": "withheld", "R5.53": "withheld", "R5.54": "withheld",
    "R5.55": "withheld",
}
HUMAN_STATE = {"HUMAN-H8": "complete", "HUMAN-H9": "withheld",
               "HUMAN-H10": "withheld", "HUMAN-H11": "withheld"}
NON_POSITIVE = {"failed", "inconclusive"}

# owed-leg -> home classification (homes are DEFERRALS rows or live chain rows)
PARTICIPANT_SET = {"R5.50", "R5.51", "R5.52", "R5.53", "HUMAN-H9"}
PUBVAL_SET = {"R5.54", "R5.55", "HUMAN-H10", "HUMAN-H11", "GATE-G16"}
PARITY_SET = {"R5.47", "R5.48", "GATE-G13", "GATE-G14"}
RENDERED_DECISIONS = {"GATE-G8", "GATE-G9", "GATE-G10", "GATE-G11",
                      "GATE-G12", "GATE-G13", "GATE-G14", "GATE-G15",
                      "GATE-G16", "GATE-G18", "HUMAN-H8"}
D_ROW_HOMES = {"DF-R5.1-1", "DF-H9-1", "DF-CAP5.1-1"}
R51_IDS = {"REQ-C-104", "REQ-C-107", "REQ-C-108", "REQ-H-001", "REQ-H-002",
           "REQ-H-003", "REQ-H-005", "REQ-H-006", "REQ-H-007", "REQ-H-011"}


def home_of(tid):
    """The live home an owed contributor leg is routed to (records-in)."""
    if tid in PARTICIPANT_SET:
        return "DF-H9-1"
    if tid in PUBVAL_SET or tid in PARITY_SET:
        return "GATE-G17"
    if tid == "R5.43":
        return "DF-CAP5.1-1"
    if tid == "R5.42a":
        return None  # inconclusive preflight consumed by G11; probe discharged at R5.44
    if tid in RENDERED_DECISIONS:
        return None  # the contribution is the decision record itself — landed
    if tid == "GATE-G17":
        return "GATE-G17"  # pending acceptance row — owed
    if tid and re.match(r"(CAP|REC|DOC)\d*\.", tid):
        return tid  # future tail owner — owed to that row
    return "GATE-G17"


def tid_of(fname):
    """'217_R5.43__useful' -> 'R5.43'; '227_HUMAN-H9__participants' -> 'HUMAN-H9'."""
    if not fname:
        return None
    return re.sub(r"^\d+[a-z]*_", "", fname).split("__")[0]


def kind_of(fname):
    base = re.sub(r"^\d+[a-z]*_", "", fname or "")
    if base.startswith("GATE-"):
        return "gate"
    if base.startswith("HUMAN-"):
        return "human"
    if re.match(r"(CAP|REC|DOC)\d*\.\d", base):
        return "tail"
    return "ticket"


def load_packets():
    pkts = {}
    for p in sorted(glob.glob(os.path.join(ROOT, REPORTS, "2026-11-0[34]-*.json"))):
        if "cap5.1" in os.path.basename(p):
            continue  # this checker's own packet
        name = os.path.basename(p)
        try:
            pkts[name] = json.load(open(p, encoding="utf-8"))
        except Exception as e:
            find("high", "NC-1", f"packet {name} does not parse", str(e), "CAP5.3")
    return pkts


def nc1_inventory(pkts):
    missing = [n for n in EXPECTED_PACKETS if n not in pkts]
    extra = [n for n in pkts if n not in EXPECTED_PACKETS]
    for n in missing:
        find("high", "NC-1", f"expected packet {n} absent", REPORTS, "CAP5.3")
    for n in extra:
        find("info", "NC-1", f"unexpected packet {n} present", REPORTS, "CAP5.3")
    bad = [n for n, d in pkts.items()
           if n != "2026-11-03-r5.40-sim-battery.json" and "disposition" not in d]
    for n in bad:
        find("high", "NC-1", f"packet {n} has no disposition member", REPORTS, "CAP5.3")
    check("NC-1", "packet inventory", "pass" if not (missing or bad) else "fail",
          f"{len(pkts)} files parsed; expected {len(EXPECTED_PACKETS)}; "
          f"missing={missing} extra={extra}")


def nc2_fold(pkts):
    bad = []
    for n, d in pkts.items():
        if n == "2026-11-03-r5.40-sim-battery.json":
            continue
        vs = [(leg.get("verdict") or {}).get("verdict")
              for leg in d.get("legs") or []]
        declared = d.get("disposition")
        if any(v == "refused" for v in vs):
            want = "failed"
        elif any(v == "inconclusive" for v in vs):
            want = "inconclusive"
        else:
            want = declared  # all-verified legs: declared positive word stands
        if declared != want:
            bad.append((n, declared, want))
            find("high", "NC-2", f"{n}: declared '{declared}' but legs fold "
                 f"to '{want}'", f"legs={vs}", "CAP5.3")
        if declared in ("executed", "withheld", "unselected", "reported") and \
                any(v in NON_POSITIVE or v == "refused" for v in vs):
            find("high", "NC-2", f"{n}: positive disposition over "
                 f"non-verified legs", f"legs={vs}", "CAP5.3")
    check("NC-2", "disposition fold consistency",
          "pass" if not bad else "fail",
          f"re-folded {len(pkts)-1} packets; mismatches={bad}")


def nc3_gates(pkts, ledger):
    rows = re.findall(
        r"\| (\d{4}-\d{2}-\d{2}T[0-9:]+Z) \| (GATE-G\d+|HUMAN-H\d+) \| "
        r"([^|]+) \| ([^|]+) \| ([^|]+) \| ([^|]+) \| [a-z-]+ \|", ledger)
    by_gate = {}
    for ts, gate, title, item, answer, conseq in rows:
        by_gate.setdefault(gate, []).append({"ts": ts, "answer": answer,
                                             "consequence": conseq})
    probs = []
    for n, d in pkts.items():
        for leg in d.get("legs") or []:
            if leg.get("name") != "gate_readout":
                continue
            f = leg.get("facts") or {}
            gr = by_gate.get(d.get("gate") or "", [])
            if not gr:
                probs.append(f"{n}: no GATE DECISIONS row for {d.get('gate')}")
                continue
            joined = " ".join(r["answer"] + " " + r["consequence"] for r in gr)
            for key, sev in (("operator_words", "high"),
                             ("block_sha256_prefix", "high"),
                             ("decided_at", "medium")):
                val = (f.get(key) or "").strip()
                if val and val not in joined:
                    find(sev, "NC-3", f"{n}: {key} '{val}' not in "
                         f"{d.get('gate')} GATE DECISIONS verbatim", val, "CAP5.3")
    # G12: a reading-2 PASSED row must exist once Reading 2 passed
    g12 = " ".join(r["answer"] + " " + r["consequence"]
                   for r in by_gate.get("GATE-G12", []))
    if "Reading 2 PASSED" not in g12 and "reading 2 of 2" not in g12:
        rp = os.path.join(ROOT, READOUTS, "GATE-G12.md")
        readout = open(rp, encoding="utf-8").read() if os.path.exists(rp) else ""
        if "Reading 2 — PASSED" in readout:
            find("medium", "NC-3",
                 "GATE DECISIONS has no GATE-G12 reading-2 PASSED row "
                 "(PASSED per readouts/GATE-G12.md, 2026-11-04T05:44:42Z; "
                 "pilot packet executed on it)",
                 "readouts/GATE-G12.md vs LEDGER GATE DECISIONS",
                 "CAP5.1 repair (append reading-2 row)")
        else:
            probs.append("G12 reading-2 absent in readout too")
    check("NC-3", "gate↔record verbatim", "pass" if not probs else "fail",
          f"gate_readout legs cross-checked; structural problems={probs}")


def nc4_deferrals(pkts, ledger, manifest):
    dtext = read(DEFERRALS)
    deferral_ids = set(re.findall(r"DF-[A-Z0-9]+(?:\.[0-9a-z]+)?-\d+", dtext))
    cited = set()
    for n, d in pkts.items():
        blob = json.dumps(d)
        for m in re.findall(r"[Dd][Ff]_([a-z]+[0-9]+)_(\d+)", blob):
            cited.add(f"DF-{m[0].upper()}-{m[1]}")
        cited.update(re.findall(r"DF-[A-Z0-9]+(?:\.[0-9a-z]+)?-\d+", blob))
    for txt in (ledger, manifest):
        cited.update(re.findall(r"DF-[A-Z0-9]+(?:\.[0-9a-z]+)?-\d+", txt))
    for rp in glob.glob(os.path.join(ROOT, READOUTS, "*.md")) + \
            glob.glob(os.path.join(ROOT, PR_DIR, "*.md")):
        cited.update(re.findall(r"DF-[A-Z0-9]+(?:\.[0-9a-z]+)?-\d+",
                                open(rp, encoding="utf-8").read()))
    missing = sorted(c for c in cited if c not in deferral_ids)
    for m in missing:
        where = [n for n, d in pkts.items()
                 if re.search(re.escape(m), json.dumps(d), re.I)]
        find("high", "NC-4", f"DF-id '{m}' cited but no DEFERRALS.md row "
             f"exists", f"cited in: {', '.join(where) or 'ledger/manifest/readouts/pr'}",
             "mint verbatim or CAP5.3")
    # OPEN rows (status = last bold cell; old **OPEN** inside DONE rows is
    # history, not state — AGENTS gotcha 8) must carry owner + verify cells
    for line in dtext.splitlines():
        if not line.startswith("| DF-"):
            continue
        cells = [c.strip() for c in line.split("|")]
        sc = [c for c in cells if re.fullmatch(r"\*\*[A-Z-]+\*\*.*", c)]
        if sc and sc[-1].startswith("**OPEN**") and \
                (len(cells) < 8 or not cells[4] or not cells[5]):
            find("medium", "NC-4", f"OPEN row {cells[1]} missing owner/verify "
                 "cell", line[:120], "row owner")
    check("NC-4", "deferral integrity",
          "pass" if not missing else "fail",
          f"{len(deferral_ids)} DEFERRALS ids; {len(cited)} cited; "
          f"missing={missing}")


def parse_coverage():
    rows = []
    for line in read(COVERAGE_MD).splitlines():
        m = re.match(
            r"\| \[(REQ-[A-H]-\d+)\]\([^)]*#req-[a-z]-\d+\) \| ([^|]+) \| "
            r"([^|]+) \| ([^|]+) \| ([^|]+) \| ([^|]+) \| ([^|]+) \|", line)
        if not m:
            continue
        rid, disp, owner, layer, hooks, contrib, status = \
            [x.strip() for x in m.groups()]
        om = re.search(r"tickets/([0-9a-z]+_[^)#]+)\.md(#([^)]*))?\)", owner)
        rows.append({"id": rid, "disposition": disp, "owner_owner": owner,
                     "owner_file": om.group(1) if om else None,
                     "owner_anchor": om.group(3) if om else None,
                     "layer": layer, "hooks": hooks,
                     "contributors": re.findall(
                         r"tickets/([0-9a-z]+_[^)#]+)\.md", contrib),
                     "status": status})
    return rows


def nc5_coverage(rows):
    dis = {}
    probs = []
    for r in rows:
        key = r["disposition"].split("(")[0].strip()
        dis[key] = dis.get(key, 0) + 1
        if r["disposition"].startswith("adopted"):
            if not r["owner_file"]:
                probs.append(f"{r['id']}: unparseable owner cell")
                continue
            fp = os.path.join(ROOT, TICKETS, r["owner_file"] + ".md")
            if not os.path.exists(fp):
                probs.append(f"{r['id']}: owner {r['owner_file']}.md absent")
                continue
            if r["owner_anchor"]:
                body = open(fp, encoding="utf-8").read()
                if f'id="{r["owner_anchor"].lstrip("#")}"' not in body:
                    probs.append(f"{r['id']}: anchor {r['owner_anchor']} "
                                 f"absent in {r['owner_file']}.md")
        for c in r["contributors"]:
            if not os.path.exists(os.path.join(ROOT, TICKETS, c + ".md")):
                probs.append(f"{r['id']}: contributor {c}.md absent")
    exp = {"adopted": 167, "merged-with": 12, "superseded-by": 1}
    if dis != exp:
        find("high", "NC-5", f"disposition counts {dis} != {exp}", str(dis), "CAP5.3")
    for p in probs:
        find("high", "NC-5", p, COVERAGE_MD, "CAP5.3")
    check("NC-5", "coverage register integrity",
          "pass" if not probs else "fail",
          f"{len(rows)} rows; dispositions={dis}; problems={len(probs)}")
    return rows


def nc6_status(rows):
    # a `met` cell is legitimate only with its dated-note discharge + landed
    # evidence cited; any other met/satisfied wording is inflation (AC-2)
    infl = [r["id"] for r in rows
            if re.search(r"\bmet\b|satisfied|closed", r["status"], re.I)
            and "unmet" not in r["status"].lower()
            and "dated note" not in r["status"]]
    for i in infl:
        find("medium", "NC-6", f"register status inflated for {i}", i, "CAP5.3")
    check("NC-6", "register status honesty",
          "pass" if not infl else "fail", f"inflated={infl}")


def nc7_index(pkts):
    bi = read(BUILD_INDEX)
    probs = []
    seqs = re.findall(r"^\| (\d+[a-z]?) \| ([A-Z0-9.]+[a-z]?) \| ([a-z]+) \| "
                      r"([^|]+) \| \[#(\d+)\]", bi, re.M)
    seqmap = {s: (t, pr) for s, t, k, b, pr in seqs}
    prs = {}
    for s, t, k, b, pr in seqs:
        prs.setdefault(pr, []).append(t)
    dup = {p: t for p, t in prs.items() if len(t) > 1}
    if dup:
        find("medium", "NC-7", f"BUILD_INDEX duplicate PR numbers {dup}",
             str(dup), "CAP5.3")
    for tid, seq in {"R5.42a": "215", "R5.43": "217", "R5.44": "218",
                     "R5.46": "221", "R5.47": "223", "R5.48": "225",
                     "R5.49": "226", "R5.50": "229", "R5.51": "230",
                     "R5.52": "231", "R5.53": "232", "R5.54": "235",
                     "R5.55": "237"}.items():
        if seqmap.get(seq, (None,))[0] != tid:
            probs.append(f"{tid}: no/mismatched BUILD_INDEX row at seq {seq}")
    live_records = glob.glob(os.path.join(ROOT, LIVE_DIR, "*.json"))
    for line in bi.splitlines():
        if re.match(r"\| 2\d\d", line) and "live-executed" in line:
            find("medium", "NC-7", "round-5 BUILD_INDEX row claims "
                 "live-executed", line[:160] +
                 f" (live records: {len(live_records)})", "CAP5.3")
    for m in re.findall(r"\| (\d+[a-z]?) \| [^|]+ \| [^|]+ \| [^|]+ \| [^|]+ "
                        r"\| [^|]+ \| (20\d\d-\d\d-\d\d)", bi):
        if m[1] > "2026-11-04":
            probs.append(f"seq {m[0]} landed date {m[1]} in the future")
    for p in probs:
        find("high", "NC-7", p, BUILD_INDEX, "CAP5.3")
    check("NC-7", "BUILD_INDEX consistency",
          "pass" if not probs else "fail",
          f"{len(seqmap)} rows; dup_prs={dup}; problems={probs}")
    return seqmap


def nc8_phase_log(seqmap):
    lg = read(LEDGER)
    missing = [t for s, (t, pr) in seqmap.items()
               if int(re.sub(r"[a-z]", "", s)) >= 168
               and not re.search(rf"[·—] {re.escape(t)} done —", lg)]
    for t in missing:
        find("medium", "NC-8", f"PHASE LOG has no '{t} done' entry",
             LEDGER, "CAP5.3")
    ok = ("lastCompleted:   R5.55" in lg and "nextTicket:      CAP5.1" in lg
          and "chainTip:         codex/harnessharness-evidence-r5.55" in lg)
    if not ok:
        find("medium", "NC-8", "CURRENT STATE disagrees with landed record",
             "expected R5.55/CAP5.1/r5.55", "CAP5.3")
    check("NC-8", "ledger machine-state + phase log",
          "pass" if not missing and ok else "fail",
          f"missing done={missing}; current_state_ok={ok}")


def nc9_overclaim(pkts):
    probs = []
    for n, d in pkts.items():
        if d.get("disposition") in ("withheld", "unselected"):
            if str(d.get("claim_awarded", "false")).lower() == "true":
                probs.append(f"{n}: claim_awarded true on withheld record")
            for leg in d.get("legs") or []:
                if leg.get("name") == "claims_withheld" and str(
                        (leg.get("facts") or {}).get("claim_awarded",
                        "false")).lower() == "true":
                    probs.append(f"{n}: claims_withheld.claim_awarded=true")
    for tid in ("R5.47", "R5.48", "R5.50", "R5.51", "R5.52", "R5.53",
                "R5.54", "R5.55"):
        for line in read(BUILD_INDEX).splitlines():
            if re.match(rf"\| \d+[a-z]? \| {re.escape(tid)} ", line) and \
                    "withheld" not in line and "unselected" not in line:
                probs.append(f"{tid}: BUILD_INDEX row omits withheld wording")
    for p in probs:
        find("high", "NC-9", p, "packets/BUILD_INDEX", "CAP5.3")
    check("NC-9", "over-claim scan", "pass" if not probs else "fail",
          f"problems={probs}")


def nc10_contributors(rows):
    probs = []
    bi_text = read(BUILD_INDEX)
    lg = read(LEDGER)
    for r in rows:
        for c in r["contributors"]:
            tid, k = tid_of(c), kind_of(c)
            if k in ("ticket", "capstone") and f" {tid} |" not in bi_text:
                probs.append(f"{r['id']}: contributor {tid} — no landed row")
            elif k in ("gate", "human"):
                rp = os.path.join(ROOT, READOUTS, tid + ".md")
                if not os.path.exists(rp) and f"| {tid} |" in lg:
                    probs.append(f"{r['id']}: {k} contributor {tid} — "
                                 "decision exists but readout absent")
                # pending gates/humans are owed legs, not record gaps
            # tail contributors are future owners by design — owed, not a gap
        otid = tid_of(r["owner_file"])
        if r["owner_file"] and kind_of(r["owner_file"]) in ("ticket",
                "capstone") and f" {otid} |" not in bi_text:
            probs.append(f"{r['id']}: owner {otid} — no BUILD_INDEX row")
    for p in set(probs):
        find("medium", "NC-10", p, COVERAGE_MD, "CAP5.3")
    req_ids = {r["id"] for r in rows}
    bad = set()
    for pp in glob.glob(os.path.join(ROOT, PR_DIR, "*.md")):
        for m in re.findall(r"REQ-[A-H]-\d+", open(pp, encoding="utf-8").read()):
            if m not in req_ids:
                bad.add((os.path.basename(pp), m))
    for b, m in bad:
        find("medium", "NC-10", f"PR {b} stamps unknown REQ id {m}", b, "CAP5.3")
    check("NC-10", "coverage anchor ⇄ contributing record",
          "pass" if not probs and not bad else "fail",
          f"problems={len(set(probs))}; bad req stamps={bad}")


def nc11_homes(pkts):
    """Residual predicates must have a live home (DF row or named chain row)."""
    dtext = read(DEFERRALS)
    res = []
    s1 = pkts.get("2026-11-04-r5.43-s1-useful.json", {})
    if s1.get("disposition") == "failed" and not re.search(
            r"REQ-A-003|REQ-A-104|s1_useful", dtext):
        res.append(("high", "failed S1 useful execution (REQ-A-003/A-104) has "
                    "no DEFERRALS row and 'primary only' contributors — "
                    "orphaned residual",
                    "reports/2026-11-04-r5.43-s1-useful.json",
                    "GATE-G17 scope disposition + DF row"))
    if "DF-H9-1" not in dtext:
        res.append(("high", "DF-H9-1 cited by packets/readouts/gates but no "
                    "DEFERRALS.md row", "readouts/HUMAN-H9.md; r5.50..r5.55 "
                    "df_h9_1:open", "mint verbatim from H9 readout"))
    if "241_GATE-G17__accept.md" not in read(MANIFEST):
        res.append(("high", "no GATE-G17 row to carry S3/S4 unmet scope "
                    "disposition", MANIFEST, "CAP5.3"))
    ops2 = pkts.get("2026-11-04-r5.44-ops-live-second.json", {})
    if "probe_verb_enoent" not in str(ops2.get("discharged_predicates")):
        res.append(("medium", "preflight probe predicate not discharged by "
                    "ops-live packet", str(ops2.get("discharged_predicates")),
                    "CAP5.3"))
    for sev, summ, ev, home in res:
        find(sev, "NC-11", summ, ev, home)
    check("NC-11", "owed-work homes", "pass" if not res else "fail",
          f"orphan findings={len(res)}")


def nc12_readouts():
    needed = ["GATE-G8", "GATE-G9", "GATE-G10", "GATE-G11", "GATE-G12",
              "GATE-G13", "GATE-G14", "GATE-G15", "GATE-G16", "GATE-G18",
              "HUMAN-H8", "HUMAN-H9", "HUMAN-H10", "HUMAN-H11"]
    missing = [g for g in needed
               if not os.path.exists(os.path.join(ROOT, READOUTS, g + ".md"))]
    for g in missing:
        find("medium", "NC-12", f"readout {g}.md absent", READOUTS, "CAP5.3")
    check("NC-12", "human/gate readouts", "pass" if not missing else "fail",
          f"missing={missing}")


def build_matrix(rows):
    """Independent verdicts, computed records-in before run-ledger detail."""
    out = []
    for r in rows:
        rid = r["id"]
        required = r["layer"].strip()
        owner_tid = tid_of(r["owner_file"])
        m = re.match(r"(merged-with|superseded-by)\((REQ-[A-H]-\d+)\)",
                     r["disposition"])
        owed, homes = [], []

        def owe(t, state):
            h = home_of(t)
            if h is not None:
                owed.append(f"{t}:{state}->{h}")
                homes.append(h)
        for c in r["contributors"]:
            c = tid_of(c)
            st = PACKET_STATE.get(c)
            hs = HUMAN_STATE.get(c)
            if st in ("withheld", "unselected", "failed", "inconclusive"):
                owe(c, st)
            elif hs == "withheld":
                owe(c, "withheld")
            elif c == "GATE-G17":
                owe(c, "pending")
        if PACKET_STATE.get(owner_tid) in ("withheld", "unselected", "failed"):
            owe(owner_tid, PACKET_STATE[owner_tid])
        if HUMAN_STATE.get(owner_tid) == "withheld":
            owe(owner_tid, "withheld")
        for x in sorted(set(re.findall(r"[RP]", required))):
            owed.append(f"{x}-layer->GATE-G17")
            homes.append("GATE-G17")
        if rid in R51_IDS:
            owed.append("contributor_evidence->DF-R5.1-1")
            homes.append("DF-R5.1-1")

        if not owed:
            verdict, routing = "MET", "accepted"
        elif set(homes) <= D_ROW_HOMES:
            hs_ = ";".join(sorted(set(homes)))
            verdict, routing = f"MET-ENGINEERED({hs_})", hs_
        else:
            verdict, routing = "PARTIAL", (homes[0] if homes else "GATE-G17")
        ev = ""
        if verdict.startswith("MET"):
            ev = f"docs/build/pr/{owner_tid}.md; docs/build/runs/{owner_tid}.md"
            for p in TICKET_PACKETS.get(owner_tid, []):
                ev += f"; {REPORTS}/{p}"
        out.append({
            "id": rid, "level": "MUST",
            "spec_section": (re.search(r"§\d+", r["hooks"]).group(0)
                             if re.search(r"§\d+", r["hooks"]) else ""),
            "class": r["disposition"].split("(")[0].strip(),
            "verdict": verdict, "evidence": ev,
            "owning_tickets": owner_tid or r["owner_owner"],
            "tests": f"crates: r5 suites (per pr/{owner_tid}.md)"
                     if owner_tid else "",
            "adrs": "", "routing": routing,
            "note": (f"alias of {m.group(2)}; " if m else "") +
                    ("owed: " + ";".join(owed) if owed else ""),
            "required_domain": required,
            "achieved_domain": "engineered",
            "owed_legs": ";".join(owed), "accepted_scope": "",
            "_alias_of": m.group(2) if m else None})
    by_id = {r["id"]: r for r in out}
    for r in out:
        t = by_id.get(r["_alias_of"])
        if t:
            for k in ("verdict", "routing", "achieved_domain", "owed_legs"):
                r[k] = t[k]
            r["note"] = (f"alias of {r['_alias_of']} — verdict inherited; "
                         "literal predicate folded into the target row")
        del r["_alias_of"]
    return out


def write_matrix(mat):
    os.makedirs(os.path.join(ROOT, "docs/build/reports/round5"), exist_ok=True)
    cols = ["id", "level", "spec_section", "class", "verdict", "evidence",
            "owning_tickets", "tests", "adrs", "routing", "note",
            "required_domain", "achieved_domain", "owed_legs", "accepted_scope"]
    with open(os.path.join(ROOT, MATRIX_OUT), "w", newline="",
              encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=cols)
        w.writeheader()
        for r in mat:
            w.writerow({c: r[c] for c in cols})


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--matrix", action="store_true")
    a = ap.parse_args()

    pkts = load_packets()
    ledger = read(LEDGER)
    nc1_inventory(pkts)
    nc2_fold(pkts)
    nc3_gates(pkts, ledger)
    nc4_deferrals(pkts, ledger, read(MANIFEST))
    rows = nc5_coverage(parse_coverage())
    nc6_status(rows)
    seqmap = nc7_index(pkts)
    nc8_phase_log(seqmap)
    nc9_overclaim(pkts)
    nc10_contributors(rows)
    nc11_homes(pkts)
    nc12_readouts()

    # a check that emitted findings is not a pass — honest verdict
    fcheck = {f["check"] for f in FINDINGS}
    for c in CHECKS:
        if c["check"] in fcheck and c["verdict"] == "pass":
            c["verdict"] = "fail"

    mat = build_matrix(rows) if a.matrix else []
    if a.matrix:
        write_matrix(mat)
    rollup = {}
    for r in mat:
        v = re.sub(r"\(.*", "", r["verdict"])
        rollup[v] = rollup.get(v, 0) + 1

    # findings found pre-repair and repaired by CAP5.1 itself (append-only
    # record repairs — verbatim from the already-recorded decisions)
    repairs = [
        {"finding": "DF-H9-1 cited by six withheld/unselected packets, "
                    "GATE-G15/G16 decisions, H9/H10/H11 readouts and manifest "
                    "notes, but no DEFERRALS.md row existed — 'a deferral not "
                    "in that file did not happen'",
         "repair": "minted DF-H9-1 verbatim (owner/trigger/control from "
                   "readouts/HUMAN-H9.md) — record repair, not a new decision",
         "severity_was": "high"},
        {"finding": "GATE DECISIONS GATE-G12 row still read 'Reading 2 "
                    "PENDING' though Reading 2 PASSED 2026-11-04T05:44:42Z "
                    "(readouts/GATE-G12.md; pilot packet executed on it)",
         "repair": "appended the verbatim reading-2 GATE DECISIONS row; the "
                   "old row's PENDING text is history per append-only rules",
         "severity_was": "medium"},
        {"finding": "GATE DECISIONS GATE-G15 decided_at reads "
                    "2026-11-04T14:32:56Z; readouts/GATE-G15.md + six packets' "
                    "gate_readout.decided_at + PHASE LOG + BUILD_INDEX all "
                    "carry 2026-11-04T14:32:29Z",
         "repair": "appended a dated correction row naming the verbatim "
                   "value (timestamp record corrected, decision untouched)",
         "severity_was": "medium"},
        {"finding": "failed S1 useful execution (REQ-A-003/A-104 + R5.43 "
                    "contributor legs on REQ-A-004/A-102) had no owed-work "
                    "home — packet disposition=failed, 'primary only' "
                    "contributors, no DEFERRALS row",
         "repair": "minted DF-CAP5.1-1 — residual routed to GATE-G17 scope "
                   "disposition or an earlier named re-execution admission",
         "severity_was": "high"},
    ]
    out = {
        "schema": "hh-capstone-gap-hunt/1",
        "version": "1.0.0",
        "generated_by": "docs/build/tools/cap5_gap_hunt.py",
        "base": "27699a432bea06a9fd30f88f087d4f42943a813e",
        "scope": "round-5 landed record vs coverage register/gate "
                 "decisions/deferrals",
        "checks": CHECKS,
        "findings": FINDINGS,
        "findings_repaired_this_run": repairs,
        "counts": {
            "checks": len(CHECKS),
            "checks_failed": sum(1 for c in CHECKS if c["verdict"] != "pass"),
            "findings": len(FINDINGS),
            "findings_repaired": len(repairs),
            "matrix_rows": len(mat),
            "verdict_rollup": rollup,
        },
    }
    with open(os.path.join(ROOT, PACKET_OUT), "w", encoding="utf-8") as f:
        json.dump(out, f, indent=2)
    print(json.dumps(out["counts"], indent=1))
    for c in CHECKS:
        print(f"  {c['check']} {c['verdict']:>4}  {c['name']}: "
              f"{c['detail'][:110]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
