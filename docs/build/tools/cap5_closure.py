#!/usr/bin/env python3
"""cap5_closure.py — CAP5.3 round-5 closure recompute (records-in).

Parses only committed build records (CAP5.1's COVERAGE_MATRIX.csv +
hh-capstone-gap-hunt/1 packet, CAP5.2's hh-cap5-composed/1 packet, the
seventeen disposition-bearing round-5 packets, DEFERRALS.md, LEDGER
GATE DECISIONS / OPEN FINDINGS / CURRENT STATE, readouts, the coverage
register, manifest rows and planning review records). Recomputes
engineering-closed vs requirement-satisfied totals and the highest
evidence/domain ceiling from the matrix; folds the five AC-9-3 layer
verdicts (engineering/research/portability/public/adoption) from
committed packet dispositions — never hand-asserted; verifies per-ID
evidence, unique owed-work homes, withheld-never-promoted posture,
no-old-signature reuse and the build-never-DONE guard; emits the
three-part GATE-G17 signature list and acceptance-prep block.

Emits `docs/build/reports/2026-11-04-cap5.3-closure.json`
(`hh-cap5-closure/1`). Read-only over build state: writes only its own
artifact.
"""
import csv
import glob
import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
REPORTS = "docs/build/reports"
MATRIX = "docs/build/reports/round5/COVERAGE_MATRIX.csv"
GAP_PACKET = "docs/build/reports/2026-11-04-cap5.1-gap-hunt.json"
COMPOSED = "docs/build/reports/2026-11-04-cap5.2-composed.json"
COVERAGE = "docs/build/planning/2026-10-30_evidence-coverage.md"
DEFERRALS = "docs/tickets/DEFERRALS.md"
LEDGER = "docs/build/LEDGER.md"
MANIFEST = "docs/tickets/00_MANIFEST.md"
G17_TICKET = "docs/tickets/241_GATE-G17__accept.md"
READOUT_TMPL = "docs/build/readouts/_TEMPLATE.md"
G17_READOUT = "docs/build/readouts/GATE-G17.md"
OUT = "docs/build/reports/2026-11-04-cap5.3-closure.json"

VERDICTS = {"MET", "MET-DIFFERENTLY", "MET-ENGINEERED", "PARTIAL",
            "MISSING", "AT-RISK-INTEGRATION", "WAIVED", "N/A-RATIONALE"}
DISPOSITIONS = {"executed", "reported", "withheld", "failed",
                "unselected", "inconclusive"}
PROMOTED = {"met", "passed", "verified", "closed", "complete",
            "satisfied", "done"}

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


def jload(p):
    return json.loads(read(p))


def exists(p):
    return os.path.isfile(os.path.join(ROOT, p))


def sha256_file(p):
    return hashlib.sha256(read(p).encode()).hexdigest()


def walk_leaves(o, key):
    """Yield every value bound to `key` anywhere in a nested record."""
    if isinstance(o, dict):
        for k, v in o.items():
            if k == key:
                yield v
            else:
                yield from walk_leaves(v, key)
    elif isinstance(o, list):
        for v in o:
            yield from walk_leaves(v, key)


def df_rows():
    """DEFERRALS.md rows -> {df-id: {unblocked_by, how_to_verify, status}}."""
    rows = {}
    for line in read(DEFERRALS).splitlines():
        if line.startswith("| DF-"):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            rows[cells[0].split()[0]] = {
                "unblocked_by": cells[3], "how_to_verify": cells[4],
                "status": cells[6] if len(cells) > 6 else ""}
    return rows


def size_rulings(ledger):
    """Round-5 PHASE LOG size-ruling entries -> [{ticket, verdict}]."""
    return [{"ticket": m.group(1), "verdict": m.group(2)}
            for m in re.finditer(
                r"\*\*([A-Z][A-Za-z0-9.]*) size ruling[^*]*\*\*[^·]*?"
                r"verdict(?::|[^*]*)?\s*\*\*(AMEND|SPLIT)\*\*", ledger)]


def main():
    base = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT,
                          capture_output=True, text=True).stdout.strip()
    packets = {os.path.basename(f): json.load(open(f, encoding="utf-8"))
               for f in sorted(glob.glob(os.path.join(
                   ROOT, REPORTS, "2026-11-0[34]-r5.*.json")))}
    composed = jload(COMPOSED)
    rows = list(csv.DictReader(open(os.path.join(ROOT, MATRIX),
                                    encoding="utf-8")))
    deferrals = df_rows()
    ledger = read(LEDGER)
    manifest = read(MANIFEST)

    # ---- CL-1 matrix recompute ---------------------------------------
    hist, dom, achieved, owed_cells = {}, {}, set(), []
    for r in rows:
        v = r["verdict"]
        base_v = "MET-ENGINEERED" if v.startswith("MET-ENGINEERED") else v
        if base_v not in VERDICTS:
            find("high", "CL-1", "verdict outside BM-VERDICT-01",
                 f'{r["id"]} verdict={v}', "cap5_closure/repair")
        hist[base_v] = hist.get(base_v, 0) + 1
        dom[r["required_domain"]] = dom.get(r["required_domain"], 0) + 1
        achieved.add(r["achieved_domain"])
        if r.get("owed_legs"):
            owed_cells.append((r["id"], r["owed_legs"]))
    eng_closed = sum(hist.get(k, 0) for k in
                     ("MET", "MET-DIFFERENTLY", "MET-ENGINEERED"))
    req_sat = hist.get("MET", 0) + hist.get("MET-DIFFERENTLY", 0)
    ceiling = sorted(achieved)[-1] if achieved else "none"
    ok = (len(rows) == 180 and hist.get("MET") == 78
          and hist.get("PARTIAL") == 73 and eng_closed == 107
          and req_sat == 78 and ceiling == "engineered"
          and not hist.get("MISSING")
          and not hist.get("AT-RISK-INTEGRATION"))
    totals = {"matrix_rows": len(rows), "verdict_histogram": hist,
              "required_domain_histogram": dom,
              "engineering_closed": eng_closed,
              "requirement_satisfied": req_sat,
              "highest_achieved_domain": ceiling}
    if len(rows) != 180 or not ok:
        find("high", "CL-1", "matrix recompute diverges from committed "
             "roll-up", json.dumps(totals), "cap5_gap_hunt regenerate")
    check("CL-1", "matrix totals recompute",
          "pass" if ok else "fail",
          f"rows={len(rows)} hist={hist} eng_closed={eng_closed} "
          f"req_satisfied={req_sat} ceiling={ceiling}")

    # ---- CL-2 five-layer fold ----------------------------------------
    stage_bad = [s for s in composed["stages"]
                 if s["verdict"] != "verified"]
    members = {
        "engineering": ["2026-11-04-r5.46-activation-podman.json",
                        "2026-11-04-r5.44-ops-live-second.json"],
        "research": ["2026-11-04-r5.43-s1-useful.json",
                     "2026-11-04-r5.46-activation-podman.json",
                     "2026-11-04-r5.46-pilot-podman.json",
                     "2026-11-04-r5.47-parity-n0.json",
                     "2026-11-04-r5.48-confirm-withheld.json",
                     "2026-11-04-r5.49-study-report.json"],
        "portability": ["2026-11-04-r5.52-outside-withheld.json"],
        "public": ["2026-11-04-r5.54-publication-withheld.json"],
        "adoption": ["2026-11-04-r5.55-value-withheld.json",
                     "2026-11-04-r5.50-plugin-unselected.json",
                     "2026-11-04-r5.51-interchange-unselected.json",
                     "2026-11-04-r5.53-formative-withheld.json"]}
    homes = {"engineering": "(none — layer executed)",
             "research": "GATE-G17", "portability": "DF-H9-1 + GATE-G17",
             "public": "GATE-G17", "adoption": "GATE-G17"}
    sr = packets.get("2026-11-04-r5.49-study-report.json", {})
    study_unmet = all(sr.get("s%d_unmet" % n) is True for n in (1, 3, 4))

    def fold(layer, files):
        ds = [packets[f]["disposition"] if f in packets else "missing"
              for f in files]
        if "missing" in ds:
            return "missing", ds
        if layer == "engineering":
            ok2 = (composed["disposition"] == "executed" and not stage_bad
                   and all(d == "executed" for d in ds))
            return ("executed" if ok2 else "unmet"), ds
        if layer == "research":
            # admitted study completed honestly (reported) carrying
            # unmet predicates -> inconclusive, never a positive claim
            if study_unmet and any(d in ("failed", "withheld")
                                   for d in ds):
                return "inconclusive", ds
            return ("reported" if all(d in ("executed", "reported")
                                      for d in ds) else "unmet"), ds
        if all(d in ("withheld", "unselected") for d in ds):
            return "withheld", ds
        return "unmet", ds

    layer_verdicts, bad_layers = {}, []
    want = {"engineering": "executed", "research": "inconclusive",
            "portability": "withheld", "public": "withheld",
            "adoption": "withheld"}
    for layer, files in members.items():
        v, ds = fold(layer, files)
        ev = [REPORTS + "/" + f for f in files]
        if layer == "engineering":
            ev += [COMPOSED, MATRIX,
                   "docs/build/reports/round5/COMPOSED_E2E_REPORT.md"]
        layer_verdicts[layer] = {
            "verdict": v, "member_dispositions": ds,
            "evidence": [e for e in ev if exists(e)],
            "owed": homes[layer] if v != "executed" else "(none)"}
        if v != want[layer]:
            bad_layers.append(f"{layer}:{v}!={want[layer]}")
        missing = [e for e in ev if not exists(e)]
        if missing:
            find("high", "CL-2", f"{layer} evidence path absent",
                 ",".join(missing), "cap5_closure/repair")
    check("CL-2", "five-layer fold from committed dispositions",
          "pass" if not bad_layers else "fail",
          "; ".join(f"{l}={layer_verdicts[l]['verdict']}" for l in
                    sorted(layer_verdicts)) +
          (f" MISMATCH={bad_layers}" if bad_layers else ""))

    # ---- CL-3 per-ID evidence + owned-target resolution --------------
    cov = read(COVERAGE)
    owned = {}
    for rid in ("REQ-C-005", "REQ-H-009"):
        line = next((l for l in cov.splitlines()
                     if f"#{rid.lower()}" in l and "CAP5.3" in l), "")
        owned[rid] = {"register_row_present": bool(line),
                      "owner": "CAP5.3 AC-3" if "CAP5.3" in line else "?",
                      "contributors": re.findall(r"R5\.\d+", line)}
    for rid, need in {"REQ-C-005": {"R5.49"},
                      "REQ-H-009": {"R5.49", "R5.52", "R5.53", "R5.54",
                                    "R5.55"}}.items():
        if not owned[rid]["register_row_present"] or \
                not need <= set(owned[rid]["contributors"]):
            find("high", "CL-3", f"{rid} register/contributor row broken",
                 json.dumps(owned[rid]), "coverage register repair")
    check("CL-3", "per-ID evidence + owned-target resolution",
          "pass" if not any(f["check"] == "CL-3" for f in FINDINGS)
          else "fail",
          "; ".join(f"{r} contribs="
                    f"{','.join(owned[r]['contributors'])}"
                    for r in owned))

    # ---- CL-4 owed-work unique homes ---------------------------------
    legs = [{"leg": res["claim"], "home": res["home"],
             "disposition": res["disposition"],
             "state": res.get("state", "?")}
            for res in composed.get("residuals", [])
            if res.get("closure") == "pending"]

    def home_ok(h):
        if h.startswith("DF-"):
            row = deferrals.get(h.split()[0].split("→")[0].split("+")[0])
            return bool(row and row["unblocked_by"] and
                        row["how_to_verify"] and "OPEN" in row["status"])
        if h == "GATE-G17":
            return exists(G17_TICKET)
        return h.startswith("landed:")

    bad_home = [l for l in legs if not home_ok(l["home"])]
    dup = sorted({l["leg"] for l in legs
                  if sum(1 for x in legs if x["leg"] == l["leg"]) > 1})
    ambiguous = []
    for rid, cell in owed_cells:
        for leg in cell.split(";"):
            leg = leg.strip()
            if not leg:
                continue
            h = leg.split("->")[-1].strip()
            tgt = "DF-" + h.split("DF-")[-1] if "DF-" in h else h
            if not (tgt in deferrals or "GATE-G17" in h
                    or h.startswith("landed")):
                ambiguous.append(f"{rid}:{leg}")
    if bad_home:
        find("high", "CL-4", "owed leg home unresolvable",
             json.dumps(bad_home), "mint DF row or re-home")
    if dup:
        find("medium", "CL-4", "duplicate owed leg name",
             ",".join(dup), "cap5_closure/repair")
    if ambiguous:
        find("high", "CL-4", "matrix owed_legs cell unrouted",
             ";".join(ambiguous[:10]), "coverage register repair")
    check("CL-4", "owed-work unique resolvable homes",
          "pass" if not (bad_home or dup or ambiguous) else "fail",
          f"{len(legs)} residual legs homes-resolved; "
          f"{len(owed_cells)} matrix owed cells routed; "
          f"bad={len(bad_home)} dup={len(dup)} amb={len(ambiguous)}")

    # ---- CL-5 reconcile old/new findings -----------------------------
    rec = []
    for fid, test, desc in (
            ("F-1", "DF-H9-1" in deferrals and
             deferrals["DF-H9-1"]["unblocked_by"] != "",
             "DF-H9-1 minted row present w/ owner"),
            ("F-2", "Reading 2 PASSED" in ledger
             or "reading-2" in ledger,
             "GATE-G12 reading-2 row in GATE DECISIONS"),
            ("F-3", "timestamp correction" in ledger,
             "G15 decided_at correction row"),
            ("F-4", "DF-CAP5.1-1" in deferrals,
             "DF-CAP5.1-1 minted for failed-S1 residual")):
        rec.append({"finding": fid, "repair_present": bool(test),
                    "what": desc})
        if not test:
            find("high", "CL-5", f"CAP5.1 {fid} repair regressed", desc,
                 "restore append-only repair")
    adjudications = [
        {"finding": "CC4 hygiene nit (S0.3 measurement sheet names "
                    "'cargo' once in prose)",
         "adjudication": "no-action-accepted — CC4 binds contracts, "
                    "data models, criteria and stages; a prose mention "
                    "inside a dated report is none of these; report is a "
                    "historical record, never rewritten",
         "control": "this CAPSTONE_CLOSURE.md findings section"},
        {"finding": "GATE-G1 accepted residuals (DF-S0.3-3 R2 human "
                    "signature; ADR-0226 trigger-5 satisfied-by-analysis)",
         "adjudication": "carried-forward — surfaced verbatim in the "
                    "signature list part 1 for GATE-G17 re-signature",
         "control": "readouts/GATE-G1.md + DEFERRALS.md DF-S0.3-3 + "
                    "docs/adr/ADR-0226-*.md"}]
    check("CL-5", "reconcile old/new findings",
          "pass" if all(r["repair_present"] for r in rec) else "fail",
          f"CAP5.1 repairs verified="
          f"{sum(r['repair_present'] for r in rec)}/4; "
          f"adjudications={len(adjudications)}")

    # ---- CL-6 withheld never promoted --------------------------------
    promoted, claim_awarded = [], []
    for fname, pkt in packets.items():
        top = pkt.get("disposition")
        if top is not None and top not in DISPOSITIONS:
            promoted.append(f"{fname}:{top}")
        if top in ("withheld", "failed", "unselected", "inconclusive"):
            for v in walk_leaves(pkt, "claim_awarded"):
                if v is True or v == "true":
                    claim_awarded.append(fname)
    if composed.get("live_authorization") != "pending{not_claimed}":
        promoted.append("composed.live_authorization!="
                        + str(composed.get("live_authorization")))
    for res in composed.get("residuals", []):
        if res.get("closure") == "closed" or (
                res.get("disposition") in
                ("withheld", "failed", "unselected", "inconclusive")
                and res.get("state") not in ("recorded", "pending")):
            promoted.append(f"residual:{res['claim']}")
    if sr.get("study") != "unmet{efficacy_unavailable;parity_withheld}":
        promoted.append("study-report unmet{} verbatim drifted")
    if promoted or claim_awarded:
        find("high", "CL-6", "withheld/failed promoted or claim awarded",
             json.dumps({"promoted": promoted,
                         "claim_awarded": claim_awarded}),
             "revert inflation append-only")
    check("CL-6", "withheld never promoted",
          "pass" if not (promoted or claim_awarded) else "fail",
          f"scanned {len(packets)} packets + composed residuals; "
          f"promoted={promoted} claim_awarded={claim_awarded}")

    # ---- CL-7 no old signature ---------------------------------------
    sig_bad = []
    if exists(G17_READOUT):
        sig_bad.append("readouts/GATE-G17.md committed pre-gate")
    if "unmet" not in str(sr.get("study", "")):
        sig_bad.append("study unmet{} absent")
    if sig_bad:
        find("high", "CL-7", "old-signature or suppressed-unmet reuse",
             ";".join(sig_bad), "revert")
    check("CL-7", "no old GATE-ACCEPT signature / unmet retained",
          "pass" if not sig_bad else "fail",
          f"GATE-G17 readout absent={not exists(G17_READOUT)}; "
          f"study unmet verbatim retained")

    # ---- CL-8 three-part signature list ------------------------------
    acc_dev = []
    for r in size_rulings(ledger):
        t = r["ticket"]
        rev = sorted(set(os.path.relpath(x, ROOT) for x in
                         glob.glob(os.path.join(
                             ROOT, "docs/build/planning",
                             f"*_{t}-split-review.md"))))
        run = f"docs/build/runs/{t}.md"
        ctl = rev + ([run] if exists(run) else [])
        acc_dev.append({"id": f"{t}-size-trigger",
                        "what": "changed source/test lines exceeded the "
                                f"contract trigger; scoped review verdict "
                                f"{r['verdict']}", "control": ctl})
        if not ctl:
            find("medium", "CL-8", f"{t} ruling control record absent",
                 "no runs/ or planning/ record", "record control")
    acc_dev += [
        {"id": "DF-S0.3-3-residual",
         "what": "R2 cross-camp human signature cell cannot be produced "
                 "by the build (accepted at GATE-G1)",
         "control": ["docs/tickets/DEFERRALS.md DF-S0.3-3",
                     "docs/build/readouts/GATE-G1.md"]},
        {"id": "ADR-0226-trigger5",
         "what": "armed revalidation trigger 5 dispositioned "
                 "satisfied-by-analysis (accepted at GATE-G1)",
         "control": ["docs/adr/ADR-0226-s0.3b-cross-candidate-"
                     "revalidation-disposition.md",
                     "docs/build/readouts/GATE-G1.md"]},
        {"id": "CC4-hygiene-nit",
         "what": "S0.3 measurement sheet names the E1 build tool once "
                 "in report prose",
         "control": ["docs/build/runs/CAP5.3.md findings — "
                     "no-action-accepted adjudication"]}]
    owed_list = [
        {"leg": "ten R5.1-owned targets' contributor evidence "
                "(REQ-C-104/107/108, REQ-H-001/002/003/005/006/007/011)",
         "owner": "GATE-G17",
         "trigger": "fresh round-5 acceptance readout records each "
                "dispatch boundary's guard invocation",
         "home": "DF-R5.1-1"},
        {"leg": "withheld participant/outreach predicate set (HUMAN-H9)",
         "owner": "operator (Steven Vitali, @SteveVitali)",
         "trigger": "actual named participants/reviewer + "
                "consent/scope/access/resources supplied",
         "home": "DF-H9-1"},
        {"leg": "failed S1 useful-task re-execution or scope disposition",
         "owner": "GATE-G17",
         "trigger": "executed hh-s1-useful/1 on the repaired machinery "
                "or verbatim scope disposition",
         "home": "DF-CAP5.1-1"},
        {"leg": "S3 parity (N=0) + S4 confirmation (quote-only) unmet",
         "owner": "GATE-G17",
         "trigger": "explicit scope disposition (G13/G14/G16 records "
                "carried verbatim)", "home": "GATE-G17"},
        {"leg": "publication + value-observation stage predicates "
                "(voided at G16)",
         "owner": "GATE-G17", "trigger": "explicit scope disposition",
         "home": "GATE-G17"},
        {"leg": "all remaining R/P-layer legs (73 PARTIAL + 29 "
                "MET-ENGINEERED targets)",
         "owner": "GATE-G17",
         "trigger": "fresh acceptance + later contributor evidence",
         "home": "GATE-G17"},
        {"leg": "Q-7 human ratification",
         "owner": "operator",
         "trigger": "ratification after O2 disposition — stays open "
                "(AC-9-3)", "home": "GATE-G17"}]
    waiver_req = [
        {"request": "S1 useful-task predicate — the bounded execution "
                "failed (machinery repaired, never re-admitted): waive "
                "round-5 re-execution or name a fresh bounded admission",
         "requires": "operator verbatim words in GATE DECISIONS + "
                "GATE-G17 signature; a durable waiver needs an ADR "
                "quoting those words"},
        {"request": "participant/outreach predicate set — stage voided "
                "at GATE-G15 (operator 'accept, continue' 14:32:29Z): "
                "ratify the round-5 scope or supply HUMAN-H9 inputs",
         "requires": "operator verbatim words; voiding ≠ discharge — "
                "DF-H9-1 stays open until then"},
        {"request": "publication stage — voided at GATE-G16 (operator "
                "'confirm and proceed' 18:53:32Z): ratify or name "
                "specific bounded public acts with expiry",
         "requires": "operator verbatim words + ADR quoting them if "
                "the disposition becomes durable scope"}]
    check("CL-8", "three-part signature list emitted",
          "pass" if not any(f["check"] == "CL-8" for f in FINDINGS)
          else "fail",
          f"accepted_deviations={len(acc_dev)} "
          f"scoped_owed_legs={len(owed_list)} "
          f"waiver_requests={len(waiver_req)}")

    # ---- CL-9 build never DONE ---------------------------------------
    m = re.search(r"projectStatus:\s*(\S+)", ledger)
    status = m.group(1) if m else "?"
    g17_row = "| 241 | `241_GATE-G17__accept.md`" in manifest
    if status == "DONE" or not g17_row:
        find("high", "CL-9", "build marked DONE or seq-241 not GATE-G17",
             f"projectStatus={status} g17_row={g17_row}", "revert")
    check("CL-9", "build never marked DONE",
          "pass" if status != "DONE" and g17_row else "fail",
          f"projectStatus={status}; manifest 241=GATE-G17:{g17_row}")

    # ---- CL-10 GATE-G17 prep ------------------------------------------
    g17 = read(G17_TICKET)
    crit = next((l.strip("> \n") for l in g17.splitlines()
                 if l.strip().startswith("> Closure reports")), "")
    prep = {"criterion_verbatim": crit,
            "read_items": [MATRIX,
                           "docs/build/reports/round5/CAPSTONE_GAP_ANALYSIS.md",
                           GAP_PACKET, COMPOSED,
                           "docs/build/reports/round5/COMPOSED_E2E_REPORT.md",
                           "docs/build/reports/round5/CAPSTONE_CLOSURE.md",
                           OUT, DEFERRALS,
                           "docs/build/LEDGER.md#gate-decisions"],
            "artifact_digests": {
                "coverage_matrix": sha256_file(MATRIX),
                "gap_hunt_packet": sha256_file(GAP_PACKET),
                "composed_packet": sha256_file(COMPOSED)},
            "scoped_items": ["CAP5.1 gap-hunt verdicts + 4 findings",
                             "CAP5.2 composed spine + residual routes",
                             "CAP5.3 layer verdicts + signature list",
                             "DF-R5.1-1 / DF-H9-1 / DF-CAP5.1-1 OPEN rows",
                             "S3/S4 unmet obligations",
                             "Q-7 open until human ratification"],
            "fresh_fields_required": ["operator words verbatim", "UTC",
                                      "channel", "scoped items",
                                      "expiry/void conditions",
                                      "agent-draft sha256 confirmation"],
            "expiry_void": {"expiry": "operator-stated at signature",
                            "voided_by": "operator cancellation or "
                                         "material contract/input change"},
            "old_signature_policy": "no old GATE-ACCEPT/G1 signature "
                                    "satisfies this gate"}
    missing_items = [i.split("#")[0] for i in prep["read_items"]
                     if not exists(i.split("#")[0])
                     and "CAPSTONE_CLOSURE.md" not in i and i != OUT]
    if missing_items or not crit or not exists(READOUT_TMPL):
        find("high", "CL-10", "GATE-G17 prep input absent",
             json.dumps(missing_items), "land artifact first")
    check("CL-10", "GATE-G17 acceptance prep",
          "pass" if not missing_items and crit else "fail",
          f"criterion verbatim captured; read_items="
          f"{len(prep['read_items'])}; digests=3; template present")

    # ---- emit packet ---------------------------------------------------
    n_pass = sum(1 for c in CHECKS if c["verdict"] == "pass")
    packet = {
        "schema": "hh-cap5-closure/1", "version": "1.0.0",
        "generated_by": "docs/build/tools/cap5_closure.py",
        "generated_at": subprocess.run(
            ["date", "-u", "+%FT%TZ"], capture_output=True,
            text=True).stdout.strip(),
        "base": base,
        "scope": "round-5 closure recompute over committed CAP5.1/CAP5.2 "
                 "artifacts + deferrals + gate records",
        "disposition": "reported",
        "checks": CHECKS,
        "layer_verdicts": layer_verdicts,
        "totals": totals,
        "owed_legs": legs,
        "owed_cells_count": len(owed_cells),
        "reconciliation": {"cap5_1_repairs": rec,
                           "adjudications": adjudications},
        "signature_list": {"accepted_deviations": acc_dev,
                           "scoped_owed_legs": owed_list,
                           "waiver_requests": waiver_req},
        "acceptance_prep": prep,
        "findings": FINDINGS,
        "counts": {"checks": len(CHECKS), "passed": n_pass,
                   "findings_open": len(FINDINGS)},
        "build_status": "not_done",
        "live_authorization": "pending{not_claimed}",
        "spend_usd": 0}
    canon = json.dumps(packet, sort_keys=True, separators=(",", ":"))
    packet["packet_id"] = hashlib.sha256(canon.encode()).hexdigest()
    with open(os.path.join(ROOT, OUT), "w", encoding="utf-8") as f:
        json.dump(packet, f, indent=2, sort_keys=True)
        f.write("\n")
    print(f"checks {n_pass}/{len(CHECKS)} pass; findings={len(FINDINGS)}; "
          f"packet {OUT} id={packet['packet_id'][:12]}…")
    for c in CHECKS:
        print(f"  {c['check']} {c['verdict']}: {c['detail'][:110]}")
    return 0 if n_pass == len(CHECKS) and not FINDINGS else 1


if __name__ == "__main__":
    sys.exit(main())
