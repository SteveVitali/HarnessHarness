#!/usr/bin/env python3
"""supplement_guard.py — hh-supplement-guard/1, the retained Round-5 supplemental
coverage and routing guard (R5.1; REQ-H-001/002/003/005/006/007/011, REQ-C-104/107/108).

READ-ONLY: it reads existing build-memory records and the ratified supplement, emits
input digests, counts and diagnostics, and exits. It never writes build state and is
never a second state owner. The stock external build-memory validator and the repo
legacy checks are preserved unchanged; this guard consumes the stock checker's
machine-readable output, it does not replace it.

Usage:
  supplement_guard.py [--root DIR] [--checker-json FILE | --no-checker] [--json]

Exit: 0 = all checks pass (HOLD routing is a legal verdict, not a failure);
      1 = at least one error diagnostic; 2 = invocation/prerequisite error.
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys

GUARD_SCHEMA = "hh-supplement-guard/1"
GUARD_VERSION = "1.0.0"

COVERAGE_MD = "docs/build/planning/2026-10-30_evidence-coverage.md"
COVERAGE_JSON = "docs/build/planning/2026-10-30_evidence-coverage.json"
BASELINE_JSON = "docs/build/planning/2026-10-30_evidence-legacy-baseline.json"
MANIFEST = "docs/tickets/00_MANIFEST.md"
LEDGER = "docs/build/LEDGER.md"
BUILD_INDEX = "docs/build/BUILD_INDEX.md"
READOUTS_DIR = "docs/build/readouts"
TICKETS_DIR = "docs/tickets"

REQ_ID = re.compile(r"^REQ-[A-H]-\d+$")
CHAIN_ID = re.compile(r"^(GATE|HUMAN)-[A-Z0-9]+$|^[A-Z]+\d*(\.\d+[a-z0-9]*)?$")
LAYER = re.compile(r"^[DERP](/[DERP])*$")
CHAIN_KINDS = {"ticket", "gate", "human", "skeleton", "capstone", "reconcile", "docs"}
TAIL_KINDS = ["capstone", "capstone", "capstone", "gate",
              "reconcile", "reconcile", "reconcile", "docs", "docs"]
EXPECTED_DISPOSITIONS = {"adopted": 167, "merged-with": 12, "superseded-by": 1}


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


class Guard:
    def __init__(self, root):
        self.root = root
        self.diags = []          # (severity, code, message)
        self.digests = {}
        self.counts = {}
        self.route = ("HOLD", "not evaluated")

    def err(self, code, msg):
        self.diags.append(("error", code, msg))

    def note(self, code, msg):
        self.diags.append(("note", code, msg))

    def path(self, rel):
        return os.path.join(self.root, rel)

    def read(self, rel):
        p = self.path(rel)
        if not os.path.isfile(p):
            self.err("missing-input", f"required input missing: {rel}")
            return None
        with open(p, encoding="utf-8") as f:
            return f.read()

    def digest(self, rel):
        p = self.path(rel)
        if os.path.isfile(p):
            self.digests[rel] = sha256_file(p)

    # ---------------- inputs ----------------

    def load_state(self, text):
        """LEDGER CURRENT STATE fenced key/value block."""
        state = {}
        m = re.search(r"^## CURRENT STATE\s*```\s*(.*?)```", text, re.S | re.M)
        if not m:
            self.err("ledger-state", "LEDGER.md has no CURRENT STATE fenced block")
            return state
        for line in m.group(1).splitlines():
            kv = re.match(r"^([A-Za-z]+):\s+(.*)$", line.strip())
            if kv:
                state[kv.group(1)] = kv.group(2).strip()
        for key in ("nextTicket", "lastCompleted", "chainTip", "buildBranchBase",
                    "pinnedBaseSha", "round", "blockedOn", "projectStatus"):
            if key not in state:
                self.err("ledger-state", f"CURRENT STATE lacks key '{key}'")
        return state

    def load_decisions(self, text):
        """GATE DECISIONS rows: id -> {passed, quotes, line}."""
        m = re.search(r"^## GATE DECISIONS\s*(.*?)\n## ", text, re.S | re.M)
        if not m:
            self.err("gate-decisions", "LEDGER.md has no GATE DECISIONS region")
            return {}
        dec = {}
        for line in m.group(1).splitlines():
            if not line.startswith("|"):
                continue
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) < 5 or cells[0] in ("---", "") or cells[0].startswith("-"):
                continue
            if cells[0].lower() == "date" or cells[1].lower() == "ticket":
                continue
            gid = cells[1]
            body = " | ".join(cells[2:])
            passed = "PASSED" in body and "NOT PASSABLE" not in body and "FAILED" not in body
            quotes = re.findall(r'"([^"\n]+)"', line)
            dec[gid] = {"passed": passed, "quotes": quotes, "row": line}
        return dec

    def load_index(self, text):
        """BUILD_INDEX landed rows -> set of ticket ids and seqs."""
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

    def load_manifest(self, text):
        """All chain rows in document order; active_round marked per '### Round <n>'."""
        rows = []   # {seq, file, id, kind, phase, gate, round}
        current_round = None
        for line in text.splitlines():
            rb = re.match(r"^### Round (\d+)", line)
            if rb:
                current_round = int(rb.group(1))
                continue
            if line.startswith("## "):
                current_round = None
            if not line.startswith("|"):
                continue
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) < 6 or cells[0] in ("#", "---") or cells[0].startswith("-"):
                continue
            fm = re.match(r"^`?(\d+[a-z]*_([A-Za-z0-9.-]+)__[^`]+?\.md)`?$", cells[1])
            if not fm:
                continue
            rows.append({
                "seq": cells[0], "file": fm.group(1), "id": fm.group(2),
                "kind": cells[3], "phase": cells[2], "gate": cells[5],
                "round": current_round,
            })
        return rows

    # ---------------- checks ----------------

    def check_coverage(self, spec_text, manifest_ids):
        cov = self.read(COVERAGE_JSON)
        if cov is None:
            return None
        try:
            data = json.loads(cov)
        except json.JSONDecodeError as e:
            self.err("coverage-json", f"coverage json not parseable: {e}")
            return None
        if data.get("schema") != "hh-supplement-plan/1":
            self.err("coverage-json",
                     f"coverage schema {data.get('schema')!r} != 'hh-supplement-plan/1'")
        if data.get("pattern") != "REQ-[A-H]-[0-9]+":
            self.err("coverage-json", f"coverage pattern {data.get('pattern')!r} unexpected")
        sup_rel = data.get("supplement", "")
        sup_path = self.path(sup_rel)
        if not sup_rel or not os.path.isfile(sup_path):
            self.err("coverage-json", f"supplement {sup_rel!r} missing")
        else:
            actual = sha256_file(sup_path)
            if actual != data.get("supplement_sha256"):
                self.err("frozen-source",
                         f"supplement sha256 drifted: {actual[:12]} != "
                         f"{str(data.get('supplement_sha256'))[:12]}")
            self.digests[sup_rel] = actual
        cands = data.get("candidates", [])
        self.counts["candidates"] = len(cands)
        ids = [c.get("id", "") for c in cands]
        if len(ids) != 180:
            self.err("coverage-count", f"{len(ids)} candidates != 180")
        for i in ids:
            if ids.count(i) > 1:
                self.err("duplicate-id", f"candidate id {i} appears more than once")
                break
        spec_ids = set(re.findall(r'id="(req-[a-h]-\d+)"', spec_text))
        for i in ids:
            if not REQ_ID.match(i):
                self.err("foreign-id", f"candidate id {i!r} fails REQ-[A-H]-[0-9]+ grammar")
            elif i.lower() not in spec_ids:
                self.err("omitted-id", f"candidate {i} has no literal registry row in the supplement")
        for sid in spec_ids:
            if sid.upper() not in ids:
                self.err("omitted-id", f"supplement registry id {sid.upper()} absent from coverage")
        disps = {"adopted": 0, "merged-with": 0, "superseded-by": 0}
        by_id = {}
        for c in cands:
            cid = c.get("id", "")
            by_id[cid] = c
            m = re.match(r"^(adopted|merged-with|superseded-by)", c.get("disposition", ""))
            if not m:
                self.err("disposition", f"{cid}: unknown disposition {c.get('disposition')!r}")
                continue
            disps[m.group(1)] += 1
        self.counts["dispositions"] = dict(disps)
        for k, want in EXPECTED_DISPOSITIONS.items():
            if disps[k] != want:
                self.err("coverage-count", f"disposition {k}: {disps[k]} != {want}")
        spec_anchors = set(re.findall(r'id="([^"]+)"', spec_text))
        for c in cands:
            self.check_candidate(c, by_id, spec_anchors, manifest_ids)
        return by_id

    def check_candidate(self, c, by_id, spec_anchors, manifest_ids):
        cid = c.get("id", "?")
        disp = c.get("disposition", "")
        dm = re.match(r"^(merged-with|superseded-by)\((REQ-[A-H]-\d+)\)", disp)
        # literal predicate retained (aliases included)
        if not c.get("literal_row"):
            self.err("alias-predicate", f"{cid}: literal predicate row is empty/deleted")
        elif c.get("source_sha256") != hashlib.sha256(c["literal_row"].encode()).hexdigest():
            self.err("alias-predicate",
                     f"{cid}: literal_row sha256 does not match source_sha256")
        if not c.get("requirement"):
            self.err("alias-predicate", f"{cid}: requirement text is empty")
        # exactly one primary owner
        primary = str(c.get("primary", "")).strip()
        if not primary or "," in primary or not CHAIN_ID.match(primary):
            self.err("owner", f"{cid}: primary owner {primary!r} is not exactly one chain id")
        elif primary not in manifest_ids:
            self.err("owner", f"{cid}: primary owner {primary} is not a manifest chain row")
        if disp == "adopted":
            if c.get("target") != cid:
                self.err("disposition", f"{cid}: adopted row target {c.get('target')!r} != own id")
        elif dm:
            tgt = dm.group(2)
            if c.get("target") != tgt:
                self.err("disposition",
                         f"{cid}: disposition target {tgt} != target field {c.get('target')!r}")
            tc = by_id.get(tgt)
            if tc is None or tc.get("disposition") != "adopted":
                self.err("disposition", f"{cid}: merge/supersede target {tgt} is not adopted")
            elif str(tc.get("primary", "")).strip() != primary:
                self.err("owner",
                         f"{cid}: alias primary {primary} != target {tgt} primary {tc.get('primary')}")
        # primary file + acceptance anchor resolve
        pf = c.get("primary_file", "")
        if pf:
            pfp = self.path(pf)
            if not os.path.isfile(pfp):
                self.err("owner", f"{cid}: primary_file {pf} missing")
            else:
                with open(pfp, encoding="utf-8") as f:
                    pftext = f.read()
                if f"_{primary}__" not in pf:
                    self.err("owner", f"{cid}: primary_file {pf} is not the {primary} contract")
                anchor = c.get("acceptance_anchor", "")
                if not anchor or f'id="{anchor}"' not in pftext:
                    self.err("ac-anchor",
                             f"{cid}: acceptance anchor {anchor!r} not present in {pf}")
        else:
            self.err("owner", f"{cid}: primary_file empty")
        # layer vocabulary
        layer = c.get("layer", "")
        if not LAYER.match(layer):
            self.err("layer", f"{cid}: layer {layer!r} outside D/E/R/P vocabulary")
        elif len(set(layer.split("/"))) != len(layer.split("/")):
            self.err("layer", f"{cid}: layer {layer!r} repeats a domain")
        # supplemental hook anchors resolve
        for ac in re.findall(r"AC-\d+-\d+", c.get("hooks", "")):
            if ac.lower() not in spec_anchors:
                self.err("ac-anchor", f"{cid}: hook {ac} has no anchor in the supplement")
        for s in re.findall(r"\u00a7(\d+)", c.get("hooks", "")):
            if f"s{s}" not in spec_anchors:
                self.err("ac-anchor", f"{cid}: hook \u00a7{s} has no anchor in the supplement")
        # contributors are real chain rows, never the owner
        for con in c.get("contributors", []):
            if con == primary:
                self.err("owner", f"{cid}: contributor {con} duplicates the primary owner")
            elif con not in manifest_ids:
                self.err("owner", f"{cid}: contributor {con} is not a manifest chain row")

    def check_coverage_md(self, by_id):
        """The .md owner table and the .json register must agree row-for-row."""
        text = self.read(COVERAGE_MD)
        if text is None or by_id is None:
            return
        m = re.search(r"## Literal owner table\s*(.*?)(\n## |\Z)", text, re.S)
        if not m:
            self.err("coverage-md", "coverage .md has no '## Literal owner table'")
            return
        seen = 0
        for line in m.group(1).splitlines():
            if not line.startswith("|"):
                continue
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) < 7 or cells[0].startswith("-") or "Literal ID" in cells[0]:
                continue
            seen += 1
            idm = re.search(r"(REQ-[A-H]-\d+)", cells[0])
            cid = idm.group(1) if idm else cells[0]
            c = by_id.get(cid)
            if c is None:
                self.err("coverage-md", f"md row {cid} has no json candidate")
                continue
            if cells[1].split(";")[0].strip() != c.get("disposition", "").split(";")[0].strip():
                self.err("coverage-md",
                         f"{cid}: md disposition {cells[1]!r} != json {c.get('disposition')!r}")
            owners = re.findall(r"\(([^)]*tickets/[^)#]+?\.md)#([^)]+)\)", cells[2])
            if len(owners) != 1:
                self.err("owner",
                         f"{cid}: md owner cell must link exactly one primary contract "
                         f"(found {len(owners)})")
            else:
                want_file = os.path.basename(c.get("primary_file", ""))
                if os.path.basename(owners[0][0]) != want_file:
                    self.err("owner",
                             f"{cid}: md owner file {owners[0][0]!r} != json {want_file!r}")
                if owners[0][1] != c.get("acceptance_anchor"):
                    self.err("ac-anchor",
                             f"{cid}: md owner anchor {owners[0][1]!r} != json "
                             f"{c.get('acceptance_anchor')!r}")
            if cells[3] != c.get("layer"):
                self.err("layer", f"{cid}: md layer {cells[3]!r} != json {c.get('layer')!r}")
            md_cons = re.findall(r"\(([^)]*tickets/[^)#]+?\.md)\)", cells[5])
            md_ids = sorted(re.match(r"\d+[a-z]*_([A-Za-z0-9.-]+)__", os.path.basename(f)).group(1)
                            for f in md_cons
                            if re.match(r"\d+[a-z]*_([A-Za-z0-9.-]+)__", os.path.basename(f)))
            if cells[5] == "primary only":
                if c.get("contributors"):
                    self.err("coverage-md",
                             f"{cid}: md says 'primary only' but json names "
                             f"contributors {c.get('contributors')}")
            elif md_ids != sorted(c.get("contributors", [])):
                self.err("coverage-md",
                         f"{cid}: md contributors {md_ids} != json {c.get('contributors')}")
        self.counts["coverage_md_rows"] = seen
        if seen != len(by_id):
            self.err("coverage-md", f"md table has {seen} rows, json has {len(by_id)}")

    def check_chain(self, rows, active_round):
        ids = [r["id"] for r in rows]
        for i in set(ids):
            if ids.count(i) > 1:
                self.err("chain-id", f"chain id {i} appears in more than one manifest row")
        active = [r for r in rows if r["round"] == active_round]
        if not active:
            self.err("chain", f"no manifest rows under '### Round {active_round}' banner")
            return [], {}
        pos = {r["id"]: n for n, r in enumerate(rows)}
        seqs = [r["seq"] for r in rows]
        for s in set(seqs):
            if seqs.count(s) > 1:
                self.err("chain-id", f"manifest sequence {s} is not unique")
        self.counts["chain_rows"] = len(rows)
        self.counts["active_round_rows"] = len(active)
        kind_at = {}
        for r in rows:
            kind_at[r["id"]] = r["kind"]
            if r["kind"] not in CHAIN_KINDS:
                self.err("chain-kind", f"{r['id']}: kind {r['kind']!r} unknown")
            if not CHAIN_ID.match(r["id"]):
                self.err("chain-id", f"{r['id']}: id fails chain-id grammar")
            # filenames carry the zero-padded sequence; normalize before compare
            file_seq = re.sub(r"^0+(\d)", r"\1", r["file"].split("_", 1)[0])
            if file_seq != r["seq"]:
                self.err("chain-file",
                         f"{r['id']}: file {r['file']} does not open with its sequence {r['seq']}")
        for r in active:
            tf = self.path(os.path.join(TICKETS_DIR, r["file"]))
            if not os.path.isfile(tf):
                self.err("chain-file", f"{r['id']}: contract file {r['file']} missing")
                continue
            with open(tf, encoding="utf-8") as f:
                ttext = f.read()
            dm = re.search(r"^- \*\*Depends on:\*\*\s*(.+)$", ttext, re.M)
            if not dm:
                self.err("dependency", f"{r['id']}: contract lacks a 'Depends on:' header")
                deps = []
            else:
                deps = [d.strip() for d in dm.group(1).split(",")]
            if r["kind"] == "skeleton":
                if re.search(r"^- \*\*Run:\*\*", ttext, re.M):
                    self.err("skeleton",
                             f"{r['id']}: skeleton contract carries a Run line — "
                             "skeletons never dispatch")
            for d in deps:
                if d.lower() == "nothing":
                    continue
                if d not in pos:
                    self.err("dependency", f"{r['id']}: 'Depends on' id {d!r} is not a chain row")
                elif pos[d] >= pos[r["id"]]:
                    self.err("forward-dependency",
                             f"{r['id']}: 'Depends on' {d} points at a later/equal chain row")
        # full-tail role presence: last nine active rows carry the retained roles
        tail = [r["kind"] for r in active[-9:]]
        if tail != TAIL_KINDS:
            self.err("tail-roles",
                     f"active-round tail kinds {tail} != required {TAIL_KINDS}")
        elif not any("accept" in r["file"] for r in active[-9:] if r["kind"] == "gate"):
            self.err("tail-roles", "tail gate row is not the operator-acceptance role")
        return active, kind_at

    def landed(self, row, index_ids, index_seqs, decisions):
        """Completion evidence per kind; returns True/False."""
        k = row["kind"]
        if k == "gate":
            d = decisions.get(row["id"])
            return bool(d and d["passed"])
        if k == "human":
            if row["id"] in decisions:
                return True
            ro = self.path(os.path.join(READOUTS_DIR, f"{row['id']}.md"))
            if os.path.isfile(ro):
                with open(ro, encoding="utf-8") as f:
                    head = f.read(4096)
                st = re.search(r"^Status:\s*(\S+)", head, re.M)
                return bool(st and st.group(1) != "PENDING")
            return False
        return row["id"] in index_ids or row["seq"] in index_seqs

    def check_routing(self, state, rows, active, kind_at, index_ids, index_seqs, decisions):
        if not active:
            return
        next_ticket = state.get("nextTicket", "")
        last_completed = state.get("lastCompleted", "")
        id_row = {r["id"]: r for r in rows}
        active_ids = {r["id"] for r in active}
        # legacy GATE-G1 completion evidence is byte-preserved
        ro_path = self.path(os.path.join(READOUTS_DIR, "GATE-G1.md"))
        if not os.path.isfile(ro_path):
            self.err("legacy-completion", "readouts/GATE-G1.md missing")
        else:
            self.digests[os.path.join(READOUTS_DIR, "GATE-G1.md")] = sha256_file(ro_path)
            with open(ro_path, encoding="utf-8") as f:
                ro_text = f.read()
            if "2026-09-15 — Reading 2" not in ro_text or "PASSED" not in ro_text \
                    or "ADR-0226" not in ro_text:
                self.err("legacy-completion",
                         "GATE-G1 readout lost its second 2026-09-15 PASSED/ADR-0226 evidence")
        # a recorded gate decision that is not PASSED stops dependent dispatch
        for r in active:
            if r["kind"] == "gate":
                d = decisions.get(r["id"])
                if d is not None and not d["passed"]:
                    self.err("failed-gate",
                             f"{r['id']}: recorded GATE DECISIONS row is not PASSED — "
                             "a pending or failed gate stops dependent dispatch")
        # lastCompleted must have real completion evidence
        lc = id_row.get(last_completed)
        if lc is None:
            self.err("last-completed", f"lastCompleted {last_completed!r} is not a chain row")
        elif not self.landed(lc, index_ids, index_seqs, decisions):
            self.err("last-completed",
                     f"lastCompleted {last_completed} has no landed/decision evidence")
        # nextTicket: active-round row, first unlanded
        nt = id_row.get(next_ticket)
        if nt is None:
            self.err("nextticket", f"nextTicket {next_ticket!r} is not a chain row")
        elif nt["id"] not in active_ids:
            self.err("old-gate-routing",
                     f"nextTicket {next_ticket} is outside the active round — "
                     "a completed/legacy row never dispatches")
        unlanded = [r for r in active
                    if not self.landed(r, index_ids, index_seqs, decisions)]
        first = unlanded[0] if unlanded else None
        if first is None:
            self.route = ("HOLD", "active round fully landed")
            self.note("route", "no unlanded active-round row")
        elif next_ticket != first["id"]:
            self.err("nextticket",
                     f"nextTicket {next_ticket!r} is not the active round's first "
                     f"unlanded row ({first['id']})")
        # verdict on the first unlanded row
        if first is not None:
            if first["kind"] == "skeleton":
                self.err("skeleton-dispatch",
                         f"first unlanded row {first['id']} is a skeleton — "
                         "skeletons never dispatch; its gate decision is owed")
                self.route = ("REFUSED", f"skeleton {first['id']}")
            elif first["kind"] in ("gate", "human"):
                self.route = ("HOLD", f"{first['kind']} {first['id']} pending — RETURN PASS")
            else:
                unmet = []
                tf = self.path(os.path.join(TICKETS_DIR, first["file"]))
                deps = []
                if os.path.isfile(tf):
                    with open(tf, encoding="utf-8") as f:
                        ttext = f.read()
                    dm = re.search(r"^- \*\*Depends on:\*\*\s*(.+)$", ttext, re.M)
                    if dm:
                        deps = [d.strip() for d in dm.group(1).split(",")]
                    if not re.search(r"^- \*\*Run:\*\*", ttext, re.M):
                        self.err("dispatch", f"{first['id']}: contract has no Run line")
                for d in deps:
                    if d.lower() == "nothing":
                        continue
                    dr = id_row.get(d)
                    if dr is None:
                        unmet.append(f"{d} (unknown id)")
                    elif not self.landed(dr, index_ids, index_seqs, decisions):
                        unmet.append(d)
                if unmet:
                    for d in unmet:
                        dk = id_row.get(d, {}).get("kind", "?")
                        self.err("pending-gate" if dk in ("gate", "human") else
                                 "pending-prerequisite",
                                 f"{first['id']} dependency {d} ({dk}) has no landed/"
                                 f"passed evidence — pending or failed prerequisites stop dispatch")
                    self.route = ("REFUSED", f"{first['id']} unmet dependencies: {unmet}")
                else:
                    self.route = ("DISPATCH", first["id"])
        # signature freshness: active-round gate words must not replay an old signature
        old_quotes = set()
        for gid, d in decisions.items():
            if gid not in active_ids:
                old_quotes.update(q.strip().lower() for q in d["quotes"])
        rd = self.path(READOUTS_DIR)
        if os.path.isdir(rd):
            for fn in sorted(os.listdir(rd)):
                if not fn.endswith(".md") or fn.startswith("_"):
                    continue
                rid = fn[:-3]
                with open(os.path.join(rd, fn), encoding="utf-8") as f:
                    for ln in f:
                        if "Operator decision (verbatim" in ln:
                            q = re.findall(r'"([^"\n]+)"', ln)
                            if rid in active_ids:
                                for s in q:
                                    if s.strip().lower() in old_quotes:
                                        self.err("signature-reuse",
                                                 f"{rid}: readout signature {s!r} replays a "
                                                 "pre-round verbatim — old acceptance signatures "
                                                 "cannot satisfy new acceptance")
                            else:
                                old_quotes.update(s.strip().lower() for s in q)
        for gid, d in decisions.items():
            if gid in active_ids and kind_at.get(gid) == "gate":
                for s in d["quotes"]:
                    if s.strip().lower() in old_quotes:
                        self.err("signature-reuse",
                                 f"{gid}: GATE DECISIONS verbatim {s!r} replays a pre-round "
                                 "signature — old acceptance signatures cannot satisfy new scope")

    def check_git_ancestry(self, state):
        # Stacked tickets advance chainTip past buildBranchBase (the setup base
        # branch is immutable; see the stock ledger convention and ADR-0382's
        # ancestry consequence). "Agree" therefore means: both refs resolve and
        # chainTip descends from (or equals) buildBranchBase — equality being
        # the degenerate pre-first-ticket case — plus pinnedBaseSha is an
        # ancestor of chainTip. (R5.2/ADR-0383: the earlier literal-equality
        # check held only before the first ticket landed and refused every
        # stacked state.)
        base = state.get("pinnedBaseSha", "")
        tip = state.get("chainTip", "")
        build_base = state.get("buildBranchBase", "")
        if not re.fullmatch(r"[0-9a-f]{40}", base):
            self.err("chain-tip", f"pinnedBaseSha {base!r} is not a 40-hex sha")
            return
        probe = subprocess.run(["git", "-C", self.root, "rev-parse", "--git-dir"],
                               capture_output=True)
        if probe.returncode != 0:
            self.note("git-ancestry", "skipped — root is not a git checkout")
            return
        try:
            subprocess.run(["git", "-C", self.root, "rev-parse", "--verify",
                            f"{tip}^{{commit}}"], check=True, capture_output=True)
            subprocess.run(["git", "-C", self.root, "rev-parse", "--verify",
                            f"{build_base}^{{commit}}"],
                           check=True, capture_output=True)
        except subprocess.CalledProcessError:
            self.err("chain-tip",
                     f"chainTip {tip!r} or buildBranchBase {build_base!r} "
                     "does not resolve to a commit")
            return
        r = subprocess.run(["git", "-C", self.root, "merge-base", "--is-ancestor",
                            build_base, tip], capture_output=True)
        if r.returncode != 0:
            self.err("chain-tip",
                     f"chainTip {tip!r} does not descend from buildBranchBase "
                     f"{build_base!r} — the chain tip must sit on the base stack")
        r = subprocess.run(["git", "-C", self.root, "merge-base", "--is-ancestor",
                            base, tip], capture_output=True)
        if r.returncode != 0:
            self.err("git-ancestry",
                     f"pinnedBaseSha {base[:12]} is not an ancestor of chainTip {tip}")

    # ---------------- historical diagnostic baseline ----------------

    @staticmethod
    def warn_signature(check, file, message):
        norm = re.sub(r"'[^']*'", "?", message.lower())
        norm = re.sub(r"\d+", "#", norm)
        norm = re.sub(r"\s+", " ", norm).strip()
        toks = norm.split(" ")[:5]
        return f"{check}|{file}|{' '.join(toks)}"

    def check_baseline(self, checker_json):
        base = self.read(BASELINE_JSON)
        if base is None:
            return
        try:
            bdata = json.loads(base)
        except json.JSONDecodeError as e:
            self.err("baseline", f"legacy baseline json not parseable: {e}")
            return
        if bdata.get("schema") != "hh-legacy-diagnostic-baseline/1":
            self.err("baseline", "baseline schema marker unexpected")
        g1 = bdata.get("gate_g1_completion", {})
        ro_rel = g1.get("file", os.path.join(READOUTS_DIR, "GATE-G1.md"))
        ro_path = self.path(ro_rel)
        if os.path.isfile(ro_path):
            if sha256_file(ro_path) != g1.get("sha256"):
                self.err("legacy-completion",
                         "GATE-G1 readout sha256 differs from the recorded baseline — "
                         "legacy completion evidence is not byte-preserved")
        # expected classes: check|file|message-signature, one emitted warning each
        expected = {}
        for entry in bdata.get("legacy", []):
            cur = entry.get("current") or entry.get("baseline") or {}
            sig = self.warn_signature(cur.get("check", ""), cur.get("file", ""),
                                      cur.get("message", ""))
            expected[sig] = cur
        self.counts["baseline_classes"] = len(expected)
        if checker_json is None:
            self.note("baseline", "skipped — no stock checker output provided "
                                  "(--checker-json or AGENT_SKILLS_ROOT)")
            return
        try:
            cdata = json.loads(checker_json)
        except json.JSONDecodeError as e:
            self.err("baseline", f"stock checker output not parseable json: {e}")
            return
        if cdata.get("schema") != "build-memory-check/2":
            self.err("baseline",
                     f"checker output schema {cdata.get('schema')!r} != build-memory-check/2")
        for v in cdata.get("violations", []):
            self.err("stock-violation",
                     f"stock checker violation {v.get('check')}: {v.get('message')}")
        emitted = cdata.get("warnings", [])
        self.counts["stock_warnings"] = len(emitted)
        self.counts["stock_violations"] = len(cdata.get("violations", []))
        unmatched = dict(expected)
        for w in emitted:
            sig = self.warn_signature(w.get("check", ""), w.get("file", ""),
                                      w.get("message", ""))
            if sig in unmatched:
                del unmatched[sig]
            else:
                self.err("unexplained-diagnostic",
                         f"stock warning not in the reviewed baseline classes: "
                         f"{w.get('check')}|{w.get('file')}|{w.get('message', '')[:80]}")
        for sig, cur in unmatched.items():
            self.err("missing-historical-diagnostic",
                     f"baseline class no longer emitted by the stock checker: "
                     f"{cur.get('check')}|{cur.get('file')} — expected historical "
                     "diagnostic absent (history must be preserved, not cleared)")

    # ---------------- driver ----------------

    def run(self, checker_json):
        for rel in (COVERAGE_MD, COVERAGE_JSON, BASELINE_JSON, MANIFEST, LEDGER, BUILD_INDEX):
            self.digest(rel)
        spec_rel = "docs/evidence-and-adoption-spec.md"
        spec_text = self.read(spec_rel)
        ledger_text = self.read(LEDGER)
        manifest_text = self.read(MANIFEST)
        index_text = self.read(BUILD_INDEX)
        if spec_text is None or ledger_text is None or manifest_text is None \
                or index_text is None:
            return self.finish()
        state = self.load_state(ledger_text)
        decisions = self.load_decisions(ledger_text)
        index_ids, index_seqs = self.load_index(index_text)
        rows = self.load_manifest(manifest_text)
        try:
            active_round = int(state.get("round", "0"))
        except ValueError:
            self.err("ledger-state", f"round {state.get('round')!r} is not an integer")
            active_round = 0
        manifest_ids = {r["id"] for r in rows}
        by_id = self.check_coverage(spec_text, manifest_ids)
        self.check_coverage_md(by_id)
        active, kind_at = self.check_chain(rows, active_round)
        self.check_routing(state, rows, active, kind_at,
                           index_ids, index_seqs, decisions)
        self.check_git_ancestry(state)
        self.check_baseline(checker_json)
        return self.finish()

    def finish(self):
        errors = [d for d in self.diags if d[0] == "error"]
        verdict = "FAIL" if errors else "PASS"
        return {
            "schema": GUARD_SCHEMA,
            "guard_version": GUARD_VERSION,
            "verdict": verdict,
            "route": {"verdict": self.route[0], "detail": self.route[1]},
            "counts": self.counts,
            "input_digests": self.digests,
            "diagnostics": [
                {"severity": s, "code": c, "message": m} for s, c, m in self.diags
            ],
        }


def run_stock_checker(root):
    script = os.environ.get("CHECK_BUILD_MEMORY")
    if not script:
        skills = os.environ.get("AGENT_SKILLS_ROOT")
        if skills:
            script = os.path.join(
                skills, "skills/build-memory/scripts/check-build-memory.sh")
    if not script or not os.path.isfile(script):
        return None, ("stock checker not found (set CHECK_BUILD_MEMORY or "
                      "AGENT_SKILLS_ROOT)")
    try:
        out = subprocess.run(["bash", script, "."], cwd=root, check=False,
                             capture_output=True, text=True, timeout=300)
    except Exception as e:  # noqa: BLE001 - report, never crash the guard
        return None, f"stock checker run failed: {e}"
    json_path = None
    for line in out.stdout.splitlines():
        if line.strip().startswith("JSON:"):
            json_path = line.split("JSON:", 1)[1].strip()
    if not json_path or not os.path.isfile(json_path):
        return None, "stock checker emitted no JSON sidecar"
    with open(json_path, encoding="utf-8") as f:
        return f.read(), None


def main():
    ap = argparse.ArgumentParser(description="hh-supplement-guard/1 — read-only "
                                             "Round-5 coverage/routing guard")
    ap.add_argument("--root", default=".")
    ap.add_argument("--checker-json", help="consume a captured build-memory-check/2 "
                                           "json instead of running the checker")
    ap.add_argument("--no-checker", action="store_true",
                    help="skip the stock-checker baseline class comparison")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument("--version", action="store_true")
    args = ap.parse_args()
    if args.version:
        print(f"{GUARD_SCHEMA} tool {GUARD_VERSION}")
        return 0
    root = os.path.abspath(args.root)
    checker_json = None
    if args.checker_json:
        try:
            with open(args.checker_json, encoding="utf-8") as f:
                checker_json = f.read()
        except OSError as e:
            print(f"check-supplement-guard: cannot read {args.checker_json}: {e}",
                  file=sys.stderr)
            return 2
    elif not args.no_checker:
        checker_json, problem = run_stock_checker(root)
        if checker_json is None:
            print(f"check-supplement-guard: prerequisite error — {problem}",
                  file=sys.stderr)
            return 2
    g = Guard(root)
    result = g.run(checker_json)
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print(f"check-supplement-guard: {result['schema']} v{result['guard_version']}"
              f" — verdict {result['verdict']}")
        for rel, d in sorted(result["input_digests"].items()):
            print(f"  input {rel} sha256:{d[:16]}…")
        print(f"  counts {json.dumps(result['counts'], sort_keys=True)}")
        print(f"  route: {result['route']['verdict']} — {result['route']['detail']}")
        for d in result["diagnostics"]:
            mark = "✗" if d["severity"] == "error" else "·"
            print(f"  {mark} [{d['code']}] {d['message']}")
        if result["verdict"] == "PASS":
            print("  ✓ all supplemental coverage/routing checks pass")
    return 0 if result["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
