#!/usr/bin/env python3
"""supplement_guard.py — hh-supplement-guard/1, the retained supplemental
coverage and routing guard (R5.1: REQ-H-001/002/003/005/006/007/011,
REQ-C-104/107/108; R6.1 GUARD-X: REQ-XH-001/002/003/004/008/024/025/026, AR-31).

READ-ONLY: it reads existing build-memory records and the two ratified
supplements, emits input digests, counts and diagnostics, and exits. It never
writes build state and is never a second state owner. The stock external
build-memory validator and the repo legacy checks are preserved unchanged;
this guard consumes the stock checker's machine-readable output, it does not
replace it.

Round 6 (GUARD-X, REQ-XH-002): the guard carries a second frozen source —
the experiments supplement and its REQ-X coverage register — beside the
Round-5 source, whose checks and counts stay byte-for-byte in force (CC1:
one guard, two sources). It routes Round 6 and enforces the round's gate,
HUMAN, skeleton, window and paid-row rules, and it folds the committed
planning checker (check_round6_coverage.py) into guard legs so one
implementation remains (SR-23).

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
GUARD_VERSION = "1.1.0"

COVERAGE_MD = "docs/build/planning/2026-10-30_evidence-coverage.md"
COVERAGE_JSON = "docs/build/planning/2026-10-30_evidence-coverage.json"
BASELINE_JSON = "docs/build/planning/2026-10-30_evidence-legacy-baseline.json"
MANIFEST = "docs/tickets/00_MANIFEST.md"
LEDGER = "docs/build/LEDGER.md"
BUILD_INDEX = "docs/build/BUILD_INDEX.md"
READOUTS_DIR = "docs/build/readouts"
TICKETS_DIR = "docs/tickets"

# Round-6 frozen source (GUARD-X, REQ-XH-002): the experiments supplement,
# its REQ-X coverage register and the frozen partition plan it was generated
# from. The register's `supplement_sha256` pins the spec bytes (the second
# frozen-source digest); the plan table is checked as a committed planning
# record, folded from check_round6_coverage.py (REQ-XH-003, SR-23/CC1).
SPEC6 = "docs/experiments-spec.md"
COVERAGE6_MD = "docs/build/planning/2026-11-07_round6-coverage.md"
COVERAGE6_JSON = "docs/build/planning/2026-11-07_round6-coverage.json"
PLAN6 = "docs/build/planning/2026-11-07_round6-decomposition.md"
DEFERRALS = "docs/tickets/DEFERRALS.md"
ROUND6 = 6

REQ_ID = re.compile(r"^REQ-[A-H]-\d+$")
REQX_ID = re.compile(r"^REQ-X[A-H]-\d+$")
AC_HOOK = re.compile(r"^AC-\d+(\.\d+)*-\d+$")   # widened from AC-\d+-\d+ (R6.1)
CHAIN_ID = re.compile(r"^(GATE|HUMAN)-[A-Z0-9]+[a-z0-9]*$|^[A-Z]+\d*(\.\d+[a-z0-9]*)?$")
# CHAIN_ID widened 2026-11-11 (R6.45b): run-time inserted gates/humans carry
# lowercase-suffixed ids (GATE-G28b — the GATE-G28b scope-decision's manifest
# row lands under the same grammar class as the R6.1 AC_HOOK widening).
LAYER = re.compile(r"^[DERP](/[DERP])*$")
CHAIN_KINDS = {"ticket", "gate", "human", "skeleton", "capstone", "reconcile", "docs"}
TAIL_KINDS = ["capstone", "capstone", "capstone", "gate",
              "reconcile", "reconcile", "reconcile", "docs", "docs"]
# A round banner may declare the decompose-spec minimal tail
# (`### Round <n> — … (…; tail=minimal)`): gap-analysis capstone, closure
# capstone, operator-acceptance gate, one docs row. Added 2026-11-07 for
# Round 6 by operator decision (planning conversation: "3-run tail
# (Recommended)"); an undeclared round keeps the full nine-role tail.
TAIL_KINDS_MINIMAL = ["capstone", "capstone", "gate", "docs"]
EXPECTED_DISPOSITIONS = {"adopted": 167, "merged-with": 12, "superseded-by": 1}
EXPECTED6_DISPOSITIONS = {"adopted": 370, "merged-with": 81, "superseded-by": 5}
EXPECTED6_CANDIDATES = 456
# The five closure ids CAP6.2 owns as first-pass ids (the rest of its adopted
# ids are routed to later passes); CAP6.1 owns none (register rule O4).
CLOSURE_IDS6 = {"REQ-XH-015", "REQ-XH-021", "REQ-XH-029", "REQ-XH-106", "REQ-XH-110"}
# Marker owners: a gate or HUMAN row owns an id only when the id is literally
# a gate-readout or HUMAN obligation (register rule O3).
MARKER_OWNERS6 = {"GATE-G26", "HUMAN-H16"}
# Disposition gates (plan §Disposition gates, SR-1): a recorded PASSED
# decision for these gates must carry one of the pre-registered forms.
# GATE-G26 is a plain PASSED (the Q-X62 waiver is named inside the readout,
# never as a chain-routing token); a `disposition <token>` form is also
# accepted and names the waiver. A basis containing FAILED or NOT PASSABLE
# stays non-PASSED (the decision parser already reads it that way).
DISPOSITION_FORMS6 = {
    "GATE-G27": [r"disposition\s+run\b", r"disposition\s+not_run\{fallback\}",
                 r"disposition\s+not_run\{pp0_hold\}"],
    "GATE-G28": [r"disposition\s+triggered\{", r"not\s+triggered;\s*Wc\s+unused"],
    "GATE-G29": [r"disposition\s+full\b", r"disposition\s+fallback\b"],
    "GATE-G30": [r"disposition\s+run\{full\}", r"disposition\s+run\{hosted-only\}",
                 r"disposition\s+end_after_w2\b"],
}
# Non-PENDING HUMAN readout tokens (REQ-XH-026; contracts defer the token
# vocabulary to this row). DEFERRED must cite an existing DEFERRALS row that
# cites the HUMAN row; DECLINED, NOT_NEEDED and N/A land the row outright.
HUMAN_NONPENDING_TOKENS6 = {"DEFERRED", "DECLINED", "NOT_NEEDED", "N/A"}
# Spend gates whose contracts must carry the REQ-XH-004 readout fields:
# model authorization, compute authorization (separately), quote id and the
# hard-cap formula.
SPEND_GATES6 = {"GATE-G26", "GATE-G30"}
SPEND_READOUT_FIELDS6 = (
    re.compile(r"model authorization", re.I),
    re.compile(r"compute authorization", re.I),
    re.compile(r"quote id", re.I),
    re.compile(r"hard-?cap formula", re.I),
)
# A Round-7/8 (later-pass) row may appear in Round 6 only as a skeleton
# (REQ-XH-001/024); a fleshed or non-skeleton later-round row is refused.
LATER_ROUND_ID6 = re.compile(r"^[A-Z]*[78]\.|^R[7-9]\.|^CAP[78]\.|^REC[78]\.|^DOC[78]\.")


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
        self.round_tail = {}
        self._paid_bodies = {}        # window body id -> its PROV gate id
        self._paid_window_names = {}  # window body id -> W1/W2/W3/Wc/W4
        self._deferrals = None

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
            # a `correction`-kind row annotates an earlier decision row; it is
            # never the gate's decision record — indexing it (last-row-wins)
            # would hide a PASSED decision or mask a real non-PASSED one.
            if cells[-1].lower() == "correction":
                continue
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
        self.round_tail = {}
        for line in text.splitlines():
            rb = re.match(r"^### Round (\d+)", line)
            if rb:
                current_round = int(rb.group(1))
                if re.search(r"\btail=minimal\b", line):
                    self.round_tail[current_round] = "minimal"
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
                "scope": cells[4], "round": current_round,
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

    # ---------------- Round-6 (GUARD-X) frozen source ----------------

    @staticmethod
    def _cells(line):
        return [c.strip() for c in re.split(r"(?<!\\)\|", line.strip())[1:-1]]

    def check_coverage6(self, spec_text, manifest_ids, kind_at):
        """REQ-X register legs (REQ-XH-003): ids sourced from spec Appendix A
        (the frozen spec carries no `id="req-…"` anchors), AC hooks resolved
        through `ac_hook_resolution: appendix-a-row`, literal-predicate
        recompute, one primary owner per adopted id."""
        cov = self.read(COVERAGE6_JSON)
        if cov is None:
            return {}
        try:
            data = json.loads(cov)
        except json.JSONDecodeError as e:
            self.err("coverage-json", f"round-6 coverage json not parseable: {e}")
            return {}
        if data.get("schema") != "hh-supplement-plan/1":
            self.err("coverage-json",
                     f"round-6 coverage schema {data.get('schema')!r} "
                     "!= 'hh-supplement-plan/1'")
        if data.get("pattern") != "REQ-X[A-H]-[0-9]+":
            self.err("coverage-json",
                     f"round-6 coverage pattern {data.get('pattern')!r} unexpected")
        sup_rel = data.get("supplement", "")
        sup_path = self.path(sup_rel)
        if not sup_rel or sup_rel != SPEC6 or not os.path.isfile(sup_path):
            self.err("coverage-json", f"round-6 supplement {sup_rel!r} missing")
        else:
            actual = sha256_file(sup_path)
            if actual != data.get("supplement_sha256"):
                self.err("frozen-source",
                         f"experiments-spec sha256 drifted: {actual[:12]} != "
                         f"{str(data.get('supplement_sha256'))[:12]}")
            self.digests[sup_rel] = actual
        # ---- spec-side tables: Appendix A, normative rows, anchors ----
        appendix, arow, srow = {}, {}, {}
        m = re.search(r"## Appendix A — Requirement index(.*?)\n<a id=\"appendix-b\">",
                      spec_text, re.S)
        azone = m.group(1) if m else ""
        if not m:
            self.err("coverage-json", "experiments spec has no Appendix A zone")
        for line in azone.split("\n"):
            mm = re.match(r"^\| (REQ-X[A-H]-\d+) \| ([^|]+?) \| ([^|]+?) \| "
                          r"([^|]+?) \| ([^|]+?) \|", line)
            if mm:
                appendix[mm.group(1)] = {
                    "disp": mm.group(2).strip(), "sec": mm.group(3).strip(),
                    "hook": mm.group(4).strip().replace(" (target)", "").strip("`"),
                    "layer": mm.group(5).strip()}
                arow[mm.group(1)] = line
        for line in spec_text.split("\n"):
            mm = re.match(r"^\| \*\*(REQ-X[A-H]-\d+)\*\* \|", line)
            if mm:
                srow[mm.group(1)] = line
        anchors = set(re.findall(r'<a id="([^"]+)"', spec_text))
        adopted_a = {i for i, v in appendix.items() if v["disp"].startswith("adopted")}
        if len(appendix) != EXPECTED6_CANDIDATES or len(adopted_a) != \
                EXPECTED6_DISPOSITIONS["adopted"]:
            self.err("coverage-count",
                     f"Appendix A has {len(appendix)} ids / {len(adopted_a)} "
                     f"adopted != {EXPECTED6_CANDIDATES}/"
                     f"{EXPECTED6_DISPOSITIONS['adopted']}")
        for i in adopted_a - set(srow):
            self.err("omitted-id", f"adopted {i} has no normative row in the spec")
        # ---- register ----
        cands = data.get("candidates", [])
        self.counts["candidates6"] = len(cands)
        ids = [c.get("id", "") for c in cands]
        for i in set(ids):
            if ids.count(i) > 1:
                self.err("duplicate-id", f"round-6 candidate id {i} appears more than once")
        by_id = {}
        for c in cands:
            if isinstance(c, dict) and "id" in c:
                by_id.setdefault(c["id"], c)
        if set(by_id) != set(appendix):
            for i in sorted(set(by_id) - set(appendix)):
                self.err("foreign-id", f"candidate {i} is not an Appendix A id")
            for i in sorted(set(appendix) - set(by_id)):
                self.err("omitted-id", f"Appendix A id {i} absent from the register")
        disps = {"adopted": 0, "merged-with": 0, "superseded-by": 0}
        for c in cands:
            cid = c.get("id", "?")
            for k, v in c.items():
                if v is None or v == "":
                    self.err("coverage-json", f"{cid}: field {k} is null or empty")
                    break
            dm = re.match(r"^(adopted|merged-with|superseded-by)", c.get("disposition", ""))
            if not dm:
                self.err("disposition", f"{cid}: unknown disposition "
                                        f"{c.get('disposition')!r}")
            else:
                disps[dm.group(1)] += 1
            a = appendix.get(cid, {})
            if a:
                if c.get("layer") != a["layer"]:
                    self.err("layer", f"{cid}: register layer {c.get('layer')!r} "
                                      f"!= Appendix A {a['layer']!r}")
                if c.get("section") != a["sec"]:
                    self.err("coverage-json", f"{cid}: register section "
                                              f"{c.get('section')!r} != Appendix A {a['sec']!r}")
                if not a["disp"].startswith(c.get("disposition", "\0")):
                    self.err("disposition", f"{cid}: register disposition "
                                            f"{c.get('disposition')!r} != Appendix A {a['disp']!r}")
            self.check_candidate6(c, by_id, a, arow, srow, anchors,
                                  manifest_ids, kind_at)
        self.counts["dispositions6"] = dict(disps)
        for k, want in EXPECTED6_DISPOSITIONS.items():
            if disps[k] != want:
                self.err("coverage-count", f"round-6 disposition {k}: {disps[k]} != {want}")
        adopted_own = [c.get("target") for c in cands if c.get("disposition") == "adopted"]
        for t in set(adopted_own):
            if adopted_own.count(t) != 1:
                self.err("owner", f"adopted target {t} owned {adopted_own.count(t)} times")
        missing = adopted_a - set(adopted_own)
        if missing:
            self.err("owner", f"adopted ids not owned as their own target: "
                              f"{sorted(missing)[:6]}")
        return by_id

    def check_candidate6(self, c, by_id, a, arow, srow, anchors,
                         manifest_ids, kind_at):
        cid = c.get("id", "?")
        disp = c.get("disposition", "")
        # literal predicate retained: verbatim row bytes + source file digest
        lr = c.get("literal_row")
        if not lr:
            self.err("alias-predicate", f"{cid}: literal predicate row is empty/deleted")
        elif c.get("source_sha256") != hashlib.sha256(lr.encode()).hexdigest():
            self.err("alias-predicate",
                     f"{cid}: literal_row sha256 does not match source_sha256")
        src = c.get("literal_source", "")
        sp = self.path(src)
        if not src or not os.path.isfile(sp):
            self.err("frozen-source", f"{cid}: literal_source {src!r} missing")
        else:
            with open(sp, "rb") as f:
                raw = f.read()
            if c.get("literal_source_sha256") != hashlib.sha256(raw).hexdigest():
                self.err("frozen-source", f"{cid}: literal_source_sha256 drifted")
            elif lr and lr.encode() not in raw:
                self.err("alias-predicate",
                         f"{cid}: literal_row is not verbatim in {src}")
            elif lr and cid not in lr:
                self.err("alias-predicate", f"{cid}: id absent from literal_row")
        if c.get("synthesis_allocated"):
            if src != SPEC6 or lr != srow.get(cid):
                self.err("alias-predicate",
                         f"{cid}: synthesis-allocated id must carry the spec's "
                         "own normative row as literal_row")
        if not c.get("requirement"):
            self.err("alias-predicate", f"{cid}: requirement text is empty")
        elif disp == "adopted":
            sc = self._cells(srow.get(cid, "|||"))
            if len(sc) < 3 or c.get("requirement") != sc[1] \
                    or c.get("acceptance") != sc[2]:
                self.err("coverage-json",
                         f"{cid}: adopted requirement/acceptance != spec cells")
        elif not c.get("literal_row") or \
                c["requirement"].split()[0] not in c.get("literal_row", ""):
            self.err("alias-predicate",
                     f"{cid}: alias requirement not drawn from its literal row")
        # anchors: appendix-a row, widened hook pattern, spec_anchor exists
        if c.get("appendix_a_row") != arow.get(cid) or \
                c.get("appendix_a_row_sha256") != \
                hashlib.sha256((arow.get(cid) or "").encode()).hexdigest():
            self.err("ac-anchor", f"{cid}: appendix_a_row/digest mismatch")
        if not AC_HOOK.fullmatch(c.get("ac_hook", "")) or \
                c.get("ac_hook") != a.get("hook"):
            self.err("ac-anchor", f"{cid}: ac_hook {c.get('ac_hook')!r} is not the "
                                  "Appendix A hook")
        if c.get("ac_hook_resolution") != "appendix-a-row":
            self.err("ac-anchor", f"{cid}: ac_hook_resolution "
                                  f"{c.get('ac_hook_resolution')!r} != 'appendix-a-row'")
        if c.get("spec_anchor") not in anchors:
            self.err("ac-anchor", f"{cid}: spec_anchor {c.get('spec_anchor')!r} "
                                  "is not a spec <a id>")
        tgt = c.get("target", "")
        if c.get("target_spec_row_sha256") != \
                hashlib.sha256((srow.get(tgt) or "").encode()).hexdigest():
            self.err("alias-predicate", f"{cid}: target_spec_row_sha256 mismatch")
        if disp == "adopted" and c.get("ac_hook", "\0") not in srow.get(cid, ""):
            self.err("ac-anchor", f"{cid}: ac_hook absent from its normative row")
        # one primary owner, a real chain row, its contract file + anchor
        primary = str(c.get("primary", "")).strip()
        if not primary or "," in primary or not CHAIN_ID.match(primary):
            self.err("owner", f"{cid}: primary owner {primary!r} is not exactly "
                              "one chain id")
        elif primary not in manifest_ids:
            self.err("owner", f"{cid}: primary owner {primary} is not a manifest "
                              "chain row")
        if disp == "adopted":
            if tgt != cid:
                self.err("disposition", f"{cid}: adopted row target {tgt!r} != own id")
        else:
            tc = by_id.get(tgt)
            if tc is None or tc.get("disposition") != "adopted":
                self.err("disposition",
                         f"{cid}: merge/supersede target {tgt} is not adopted")
            elif str(tc.get("primary", "")).strip() != primary:
                self.err("owner", f"{cid}: alias primary {primary} != target {tgt} "
                                  f"primary {tc.get('primary')}")
        pf = c.get("primary_file", "")
        if pf:
            pfp = self.path(pf)
            if not os.path.isfile(pfp):
                self.err("owner", f"{cid}: primary_file {pf} missing")
            else:
                with open(pfp, encoding="utf-8") as f:
                    pftext = f.read()
                if f"_{primary}__" not in pf:
                    self.err("owner", f"{cid}: primary_file {pf} is not the "
                                      f"{primary} contract")
                anchor = c.get("acceptance_anchor", "")
                if not anchor or f'id="{anchor}"' not in pftext:
                    self.err("ac-anchor", f"{cid}: acceptance anchor {anchor!r} "
                                          f"not present in {pf}")
                pk = c.get("primary_kind")
                if pk and pk != kind_at.get(primary):
                    self.err("owner", f"{cid}: primary_kind {pk!r} != manifest "
                                      f"kind {kind_at.get(primary)!r}")
        else:
            self.err("owner", f"{cid}: primary_file empty")
        if not LAYER.match(c.get("layer", "")):
            self.err("layer", f"{cid}: layer {c.get('layer')!r} outside D/E/R/P")
        # owner-kind rule (O2/O3): gate/human only literal marker obligations;
        # skeleton only R/P window evidence
        if disp == "adopted" and primary in manifest_ids:
            pk = kind_at.get(primary)
            if pk in ("gate", "human") and primary not in MARKER_OWNERS6:
                self.err("owner", f"{cid}: adopted id owned by marker row {primary} "
                                  "that is not a literal gate/HUMAN obligation owner")
            if pk == "skeleton" and c.get("layer") not in ("R", "P"):
                self.err("owner", f"{cid}: skeleton-owned id must be R/P layer")
        # routing (O4): routed ids are owned by CAP6.2 only
        scope = c.get("round_scope", "")
        if disp == "adopted":
            if scope.startswith("routed"):
                if primary != "CAP6.2":
                    self.err("owner", f"{cid}: routed id owned by {primary}, "
                                      "must be CAP6.2")
            elif scope == "first-pass":
                if primary == "CAP6.1" or (primary == "CAP6.2" and cid not in CLOSURE_IDS6):
                    self.err("owner", f"{cid}: first-pass id owned by {primary} "
                                      "outside the CAP6.2 closure set")
        for con in c.get("contributors", []):
            if con == primary:
                self.err("owner", f"{cid}: contributor {con} duplicates the primary")
            elif con not in manifest_ids:
                self.err("owner", f"{cid}: contributor {con} is not a manifest "
                                  "chain row")

    def check_coverage6_md(self, by_id):
        """The round-6 .md literal owner table and owner index agree with the
        json register row-for-row."""
        text = self.read(COVERAGE6_MD)
        if text is None or not by_id:
            return
        m = re.search(r"## Literal owner table\s*(.*?)(\n## |\Z)", text, re.S)
        if not m:
            self.err("coverage-md", "round-6 coverage .md has no '## Literal owner table'")
            return
        seen = 0
        for line in m.group(1).splitlines():
            if not line.startswith("|"):
                continue
            cells = self._cells(line)
            if len(cells) < 7 or not re.match(r"REQ-X[A-H]-\d+$", cells[0]):
                continue
            cid = cells[0]
            seen += 1
            c = by_id.get(cid)
            if c is None:
                self.err("coverage-md", f"md row {cid} has no json candidate")
                continue
            owners = re.findall(r"\(([^)]*tickets/[^)#]+?\.md)#([^)]+)\)", cells[2])
            if len(owners) != 1:
                self.err("owner", f"{cid}: md owner cell must link exactly one "
                                  f"primary contract (found {len(owners)})")
            else:
                if os.path.basename(owners[0][0]) != \
                        os.path.basename(c.get("primary_file", "")):
                    self.err("owner", f"{cid}: md owner file {owners[0][0]!r} != "
                                      f"json {c.get('primary_file')!r}")
                if owners[0][1] != c.get("acceptance_anchor"):
                    self.err("ac-anchor", f"{cid}: md owner anchor {owners[0][1]!r} "
                                          f"!= json {c.get('acceptance_anchor')!r}")
        self.counts["coverage6_md_rows"] = seen
        if seen != EXPECTED6_CANDIDATES:
            self.err("coverage-md", f"round-6 md owner table has {seen} rows, "
                                    f"json has {EXPECTED6_CANDIDATES}")
        mi = re.search(r"## Owner index[^\n]*\n(.*?)(\n## |\Z)", text, re.S)
        if not mi:
            self.err("coverage-md", "round-6 coverage .md has no '## Owner index'")
            return
        got = {}
        cid_re = r"(GATE-G\d+|HUMAN-H\d+|R6\.\d+[a-z]?|CAP6\.\d+|REC6\.\d+|DOC6\.\d+)"
        for line in mi.group(1).splitlines():
            cells = self._cells(line) if line.startswith("|") else []
            mm = re.match(r"\[" + cid_re + r"\]", cells[0]) if cells else None
            if mm:
                body = re.sub(r"^\d+: (routed: )?", "", cells[3])
                got[mm.group(1)] = {"REQ-" + x.strip() for x in body.split(",")}
        want = {}
        for c in by_id.values():
            if c.get("disposition") == "adopted":
                want.setdefault(c.get("primary"), set()).add(c["id"])
        for k in set(got) | set(want):
            if got.get(k) != want.get(k):
                self.err("coverage-md", f"md owner index for {k} disagrees with json")

    # ---------------- Round-6 frozen partition plan (folded SR-23/CC1) ----------------

    def check_plan6(self, by_id):
        """The frozen Round-6 partition plan as a committed planning record —
        the legs folded from docs/build/tools/check_round6_coverage.py so one
        implementation remains. Diagnostics carry the `plan-` family."""
        text = self.read(PLAN6)
        if text is None:
            return
        rows, order, owned, parse_err = {}, [], {}, []
        for line in text.split("\n"):
            if not re.match(r"^\| \d{3} \| `", line):
                continue
            c = self._cells(line)
            if len(c) != 13:
                parse_err.append(line[:60])
                continue
            seq, f, cid, kind, deps, size = \
                int(c[0]), c[1].strip("`"), c[2], c[3], c[8], c[9]
            rows[cid] = {"seq": seq, "file": f, "kind": kind,
                         "deps": [d.strip() for d in deps.split(",")],
                         "size": size, "scope": c[5], "alias": c[12]}
            order.append(cid)
            oc = c[7]
            mm = re.match(r"^(\d+)(?: \([^)]*\))?: (.*)$", oc)
            owned[cid] = (int(mm.group(1)), ["REQ-" + x.strip()
                                             for x in mm.group(2).split(",")]) \
                if mm else (0, []) if oc == "—" else (-1, [oc])
        order.sort(key=lambda x: rows[x]["seq"])
        n = len(order)
        if parse_err or n == 0:
            self.err("plan-table", f"round-6 plan row table does not parse "
                                   f"({n} rows; {parse_err[:2]})")
            return rows
        if [rows[x]["seq"] for x in order] != list(range(252, 252 + n)):
            self.err("plan-table", "plan sequences are not contiguous from 252")
        self.counts["plan6_rows"] = n
        tot = re.search(r"\*\*total rows\*\* \| \*\*(\d+)\*\*", text)
        if tot is None or int(tot.group(1)) != n:
            self.err("plan-totals", "Totals table total != row-table count")
        kinds = [rows[x]["kind"] for x in order]
        for k in ("ticket", "skeleton", "gate", "human"):
            mm = re.search(r"^\| " + k + r" \| (\d+)", text, re.M)
            if not mm or int(mm.group(1)) != kinds.count(k):
                self.err("plan-totals", f"Totals kind {k} != row table")
        mm = re.search(r"^\| capstone / reconcile / docs \| (\d+) / (\d+) / (\d+)",
                       text, re.M)
        if not mm or (int(mm.group(1)), int(mm.group(2)), int(mm.group(3))) != \
                (kinds.count("capstone"), kinds.count("reconcile"), kinds.count("docs")):
            self.err("plan-totals", "Totals capstone/reconcile/docs != row table")
        for cid, r in rows.items():
            if not CHAIN_ID.match(cid):
                self.err("plan-chain", f"plan id {cid} fails chain-id grammar")
            if not r["file"].startswith(f"{r['seq']:03d}_{cid}__"):
                self.err("plan-chain", f"{cid}: file {r['file']} does not open "
                                       "with seq+id")
            if r["kind"] not in CHAIN_KINDS:
                self.err("plan-chain", f"{cid}: kind {r['kind']!r} unknown")
        pos = {x: i for i, x in enumerate(order)}
        for cid, r in rows.items():
            if r["deps"] == ["nothing"]:
                continue
            for d in r["deps"]:
                if d not in rows or rows[d]["seq"] >= r["seq"]:
                    self.err("plan-deps", f"{cid}: dep {d} is not a backward "
                                          "chain row")
        nothing = [c for c, r in rows.items() if r["deps"] == ["nothing"]]
        if nothing != order[:1]:
            self.err("plan-deps", "only the bootstrap gate may depend on nothing")
        if n >= 5:
            if kinds[-5:] != ["capstone", "capstone", "capstone", "gate", "docs"] \
                    or "accept" not in rows[order[-2]]["file"] \
                    or "reconcile" in kinds \
                    or any(k in ("capstone", "docs") for k in kinds[:-5]):
                self.err("plan-tail", "plan tail is not the contiguous minimal "
                                      "capstone×3, accept-gate, docs shape")
        if not order or rows[order[0]]["kind"] != "gate" or order[1] != "R6.1" \
                or rows.get("R6.1", {}).get("kind") != "ticket":
            self.err("plan-rows", "plan row 1 is not the bootstrap gate followed "
                                  "by the single guard row R6.1")
        skels = [c for c, r in rows.items() if r["kind"] == "skeleton"]
        for c in skels:
            if not any(rows.get(d, {}).get("kind") == "gate" for d in rows[c]["deps"]) \
                    or "fleshed after gate" not in rows[c]["size"]:
                self.err("plan-skeleton", f"{c}: skeleton lacks a gate dep or "
                                          "'fleshed after gate'")
        if len(skels) != 5:
            self.err("plan-skeleton", f"{len(skels)} window skeletons != 5")
        for i, c in enumerate(order):
            if rows[c]["kind"] != "human":
                continue
            j = i + 1
            while j < n and rows[order[j]]["kind"] == "human":
                j += 1
            nx = order[j] if j < n else None
            if nx is None or not (rows[nx]["kind"] in ("gate", "capstone")
                                  or c in rows[nx]["deps"]):
                self.err("plan-human", f"{c}: HUMAN row not before a gate, the "
                                       "tail, or a row that depends on it")
        for c, r in rows.items():
            mm = re.match(r"(\d+)k \+ 10k", r["size"])
            if r["kind"] in ("ticket", "capstone", "reconcile", "docs") and not mm:
                self.err("plan-size", f"{c}: implement-spec row carries no size")
                continue
            if mm and int(mm.group(1)) + 10 > 105 \
                    and "operator-ruled" not in r["size"]:
                self.err("plan-size", f"{c}: row size over 95k + 10k without "
                                      "operator-ruled")
        opruled = [c for c, r in rows.items() if "operator-ruled" in r["size"]]
        if any(rows[c]["kind"] != "capstone" or c not in order[-4:]
               for c in opruled):
            self.err("plan-size", "operator-ruled oversize on a non-tail-capstone")
        mm = re.search(r"\*\*implement-spec rows\*\* \| \*\*(\d+)\*\*", text)
        n_impl = sum(1 for r in rows.values()
                     if r["kind"] in ("ticket", "capstone", "reconcile", "docs"))
        if mm is None or int(mm.group(1)) != n_impl or not 40 <= n_impl <= 50:
            self.err("plan-totals", "Totals implement-spec count wrong or "
                                    "outside the operator band 40–50")
        for c, r in rows.items():
            if re.search(r"ARCHIVE-UPLOAD|SETTLE-W", r["alias"]) \
                    and r["kind"] != "skeleton":
                self.err("plan-fold", f"{c}: ARCHIVE-UPLOAD/SETTLE act lives "
                                      "outside a window skeleton")
        bpm = re.search(r"<!-- linearization-B-prime: ([^>]*?) -->", text)
        if not bpm:
            self.err("plan-linearization", "linearization-B-prime marker missing")
        else:
            mv = bpm.group(1).split()
            if any(x not in rows for x in mv) or "GATE-G29" not in rows:
                self.err("plan-linearization", "B′ marker names an unknown id")
            else:
                ob = [x for x in order if x not in mv]
                i = ob.index("GATE-G29") + 1
                ob = ob[:i] + mv + ob[i:]
                bpos = {x: j for j, x in enumerate(ob)}
                for c, r in rows.items():
                    if r["deps"] == ["nothing"]:
                        continue
                    for d in r["deps"]:
                        if d in bpos and bpos[d] >= bpos[c]:
                            self.err("plan-linearization",
                                     f"{c}: B′ permutation breaks backward-only "
                                     f"on dep {d}")
        # row-table owned ids == json primaries, row for row
        js = {}
        for c in by_id.values():
            if c.get("disposition") == "adopted":
                js.setdefault(c.get("primary"), set()).add(c["id"])
        seen = {}
        for cid, (cnt, ids) in owned.items():
            if cnt == -1:
                self.err("plan-owner", f"{cid}: unparseable owned cell")
                continue
            if cnt != len(ids) and ids:
                self.err("plan-owner", f"{cid}: owned count {cnt} != {len(ids)}")
            for x in ids:
                seen[x] = seen.get(x, 0) + 1
            if set(ids) != js.get(cid, set()):
                self.err("plan-owner", f"{cid}: plan-table owned ids != register")
        for x, v in seen.items():
            if v > 1:
                self.err("plan-owner", f"{x}: duplicated owner in plan table")
        for x in sorted(set(js) - set(owned)):
            self.err("plan-owner", f"{x}: register owner absent from plan table")
        # shared-decision ownership: the owner row precedes every consumer
        sm = re.search(r"### Shared-decision ownership[^\n]*\n(.*?)(\n### |\Z)",
                       text, re.S)
        if not sm:
            self.err("plan-shared", "no '### Shared-decision ownership' table")
        else:
            cid_re = r"(?:GATE-G\d+|HUMAN-H\d+|R6\.\d+[a-z]?|CAP6\.\d+|REC6\.\d+|DOC6\.\d+)"
            nsh = 0
            for line in sm.group(1).splitlines():
                c = self._cells(line) if line.startswith("|") else []
                if len(c) != 3 or c[0] in ("shared decision / seam", "---") \
                        or set(c[0]) <= {"-"}:
                    continue
                own = re.findall(cid_re, c[1])
                if not own:
                    continue
                nsh += 1
                o = own[0]
                if o not in rows:
                    self.err("plan-shared", f"shared decision owner {o} not a row")
                    continue
                for x in re.findall(cid_re, c[2]):
                    if x in rows and x != o and rows[x]["seq"] <= rows[o]["seq"]:
                        self.err("plan-shared", f"shared decision {o} after "
                                                f"consumer {x}")
            if nsh < 20:
                self.err("plan-shared", f"only {nsh} shared decisions parsed")
        return rows

    # ---------------- Round-6 markers, windows and paid rows ----------------

    def _contract_text(self, row):
        tf = self.path(os.path.join(TICKETS_DIR, row["file"]))
        if not os.path.isfile(tf):
            self.err("chain-file", f"{row['id']}: contract file {row['file']} missing")
            return ""
        with open(tf, encoding="utf-8") as f:
            return f.read()

    def check_markers6(self, active, pos, kind_at, decisions, spec_text):
        """Gate markers (REQ-XH-025), HUMAN rows (REQ-XH-026), paid bodies
        (REQ-XH-001/024) and the spend-gate readout fields (REQ-XH-004)."""
        specn = re.sub(r"\s+", " ", spec_text)
        for r in active:
            t = self._contract_text(r)
            if not t:
                continue
            if r["kind"] == "gate":
                cm = re.search(r"^## Criterion \(verbatim[^\n]*\n(.*?)(?=^## |\Z)",
                               t, re.S | re.M)
                if not cm:
                    self.err("gate-criterion",
                             f"{r['id']}: no '## Criterion (verbatim …)' section")
                else:
                    for ln in cm.group(1).splitlines():
                        if ln.startswith("> |"):
                            q = re.sub(r"\s+", " ", ln[2:]).strip()
                            if q and q not in specn:
                                self.err("gate-criterion",
                                         f"{r['id']}: criterion quote absent from "
                                         f"the frozen spec: {q[:80]}")
                d = decisions.get(r["id"])
                if r["id"] in DISPOSITION_FORMS6 and d and d["passed"]:
                    body = d["row"]
                    if not any(re.search(p, body)
                               for p in DISPOSITION_FORMS6[r["id"]]):
                        self.err("gate-disposition",
                                 f"{r['id']}: PASSED decision carries no "
                                 "pre-registered disposition form")
                if r["id"] in SPEND_GATES6:
                    for pat in SPEND_READOUT_FIELDS6:
                        if not pat.search(t):
                            self.err("readout-fields",
                                     f"{r['id']}: spend-gate contract lacks the "
                                     f"'{pat.pattern}' readout field")
            elif r["kind"] == "human":
                for label, pat in (("owner", r"\*\*Owner:\*\*"),
                                   ("trigger", r"\*\*Scheduled:\*\*[^\n]*trigger"),
                                   ("exit", r"^## Exit criterion"),
                                   ("withheld", r"\*\*Withheld until done:\*\*")):
                    if not re.search(pat, t, re.M):
                        self.err("human-marker",
                                 f"{r['id']}: contract lacks the {label} marker")
                ro = self.path(os.path.join(READOUTS_DIR, f"{r['id']}.md"))
                if os.path.isfile(ro):
                    with open(ro, encoding="utf-8") as f:
                        head = f.read(4096)
                    st = re.search(r"^Status:\s*(\S+)", head, re.M)
                    if st and st.group(1) == "DEFERRED":
                        dfs = re.findall(r"DF-[A-Za-z0-9.-]+", head)
                        deftext = self._deferrals or ""
                        cited = [x for x in dfs
                                 if x in deftext and r["id"] in deftext]
                        if not cited:
                            self.err("human-deferred",
                                     f"{r['id']}: DEFERRED readout cites no "
                                     "existing DEFERRALS row naming the HUMAN row")
            if r["kind"] in ("ticket", "capstone", "reconcile", "docs"):
                # a paid body in the active round is a window skeleton or a row
                # decomposed after its gate (REQ-XH-001/024)
                if r["id"] in self._paid_bodies:
                    gate = self._paid_bodies[r["id"]]
                    gd = decisions.get(gate)
                    if r["kind"] == "ticket" and not (gd and gd["passed"]):
                        self.err("paid-body",
                                 f"{r['id']}: paid window body is fleshed but its "
                                 f"gate {gate} is not recorded PASSED")
                if LATER_ROUND_ID6.match(r["id"]):
                    self.err("paid-body",
                             f"{r['id']}: a later-round (7/8) row may appear in "
                             "Round 6 only as a skeleton")

    def check_window6(self, active, pos):
        """REQ-XH-008 window pattern: per window, PROV gate → window body → its
        HUMAN-reconcile entry, as an ordered subsequence (consolidated
        HUMAN-H18; GATE-G20 D-1/D-2 accepted)."""
        # reconcile coverage: a human row covers a window when its manifest
        # scope cell names that window's token in a reconcile context (the
        # consolidated HUMAN-H18 scope names all five; a per-window reconcile
        # row names its own)
        covers = {}
        for h in (r for r in active if r["kind"] == "human"):
            scope = h.get("scope", "")
            if not re.search(r"reconcil", scope, re.I):
                continue
            for w in ("W1", "W2", "W3", "W4", "Wc"):
                if re.search(r"\b" + w + r"\b", scope):
                    covers.setdefault(w, []).append(h)
        for body, gate in self._paid_bodies.items():
            brow = next((r for r in active if r["id"] == body), None)
            grow = next((r for r in active if r["id"] == gate), None)
            if brow is None:
                self.err("window-order", f"window body {body} is not an "
                                         "active-round row")
                continue
            if grow is None or pos[grow["id"]] >= pos[body]:
                self.err("window-order",
                         f"{body}: its PROV gate {gate} does not precede the "
                         "window body")
            wname = self._paid_window_names.get(body)
            if wname:
                recs = covers.get(wname, [])
                if not recs:
                    self.err("window-order",
                             f"{body}: no HUMAN-reconcile row covers window {wname}")
                for h in recs:
                    if pos[h["id"]] <= pos[body]:
                        self.err("window-order",
                                 f"{h['id']}: reconcile entry for {wname} sits "
                                 f"before its window body {body}")

    def _init_paid_windows(self, plan_rows):
        """Window bodies and their PROV gates from the frozen plan table:
        skeleton rows whose dep is a gate; window names from the plan alias
        (W1/W2/W3/Wc/W4{…})."""
        self._paid_bodies = {}
        self._paid_window_names = {}
        for cid, r in plan_rows.items():
            if r.get("kind") != "skeleton":
                continue
            gate = next((d for d in r["deps"]
                         if plan_rows.get(d, {}).get("kind") == "gate"), None)
            if gate:
                self._paid_bodies[cid] = gate
            wm = re.match(r"(W[1-4]|Wc)\b", r.get("alias", ""))
            if wm:
                self._paid_window_names[cid] = wm.group(1)

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
        # full-tail role presence: the nine-role acceptance tail must appear
        # contiguously among the active-round rows. Rows appended after the tail
        # by an authorized run-time extension (## Plan extensions — e.g. the
        # 2026-11-06 S4-completion rows 247-251) do not move the requirement;
        # presence, not strictly-last. (Repaired 2026-11-06 — same class as the
        # DONE-token routing repair; LEDGER OPEN FINDINGS.)
        kinds = [r["kind"] for r in active]
        tail = (TAIL_KINDS_MINIMAL
                if getattr(self, "round_tail", {}).get(active_round) == "minimal"
                else TAIL_KINDS)
        n = len(tail)
        win = next((i for i in range(len(kinds) - n + 1)
                    if kinds[i:i + n] == tail), None)
        if win is None:
            self.err("tail-roles",
                     f"active-round rows contain no contiguous tail {tail}")
        elif not any("accept" in r["file"]
                     for r in active[win:win + n] if r["kind"] == "gate"):
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
        unlanded = [r for r in active
                    if not self.landed(r, index_ids, index_seqs, decisions)]
        first = unlanded[0] if unlanded else None
        nt = id_row.get(next_ticket)
        if first is None:
            # Round complete — nextTicket holds a terminal token (DONE). A
            # chain-row value is still invalid: a completed/legacy row never
            # dispatches.
            if nt is not None:
                if nt["id"] not in active_ids:
                    self.err("old-gate-routing",
                             f"nextTicket {next_ticket} is outside the active round — "
                             "a completed/legacy row never dispatches")
                else:
                    self.err("nextticket",
                             f"nextTicket {next_ticket!r} names a landed row — "
                             "a completed round's terminal token is DONE")
            self.route = ("HOLD", "active round fully landed")
            self.note("route", "no unlanded active-round row")
        else:
            # nextTicket: active-round row, must equal the first unlanded row
            if nt is None:
                self.err("nextticket", f"nextTicket {next_ticket!r} is not a chain row")
            elif nt["id"] not in active_ids:
                self.err("old-gate-routing",
                         f"nextTicket {next_ticket} is outside the active round — "
                         "a completed/legacy row never dispatches")
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
        # append-only dated amendments: bounded diagnostic families classified
        # into the reviewed baseline with provenance (R6.1 AC-3 — the 46 legacy
        # reqcov warnings). A family explains at most `max_emitted` matching
        # warnings — a further warning is still unexplained — and is never
        # required to be emitted (emission is validator-version dependent, so
        # a vanished family is not a missing-historical-diagnostic; the
        # standing `legacy` classes keep the required-emission rule).
        families = []
        for am in bdata.get("amendments", []):
            fam = am.get("family", {})
            try:
                families.append({
                    "id": am.get("id", "?"),
                    "check": fam.get("check", ""),
                    "file": re.compile(fam.get("file_pattern", r"\b\B")),
                    "message": re.compile(fam.get("message_pattern", r"\b\B")),
                    "left": int(fam.get("max_emitted", 0)),
                    "used": 0,
                })
            except (re.error, TypeError, ValueError) as e:
                self.err("baseline", f"amendment {am.get('id')!r} malformed: {e}")
        emitted = cdata.get("warnings", [])
        self.counts["stock_warnings"] = len(emitted)
        self.counts["stock_violations"] = len(cdata.get("violations", []))
        unmatched = dict(expected)
        for w in emitted:
            sig = self.warn_signature(w.get("check", ""), w.get("file", ""),
                                      w.get("message", ""))
            if sig in unmatched:
                del unmatched[sig]
                continue
            fam = next((f for f in families
                        if f["left"] > 0 and f["check"] == w.get("check")
                        and f["file"].search(w.get("file", ""))
                        and f["message"].search(w.get("message", ""))), None)
            if fam is not None:
                fam["left"] -= 1
                fam["used"] += 1
            else:
                self.err("unexplained-diagnostic",
                         f"stock warning not in the reviewed baseline classes: "
                         f"{w.get('check')}|{w.get('file')}|{w.get('message', '')[:80]}")
        for fam in families:
            if fam["used"]:
                self.note("classified-diagnostic",
                          f"{fam['used']} warning(s) explained by dated baseline "
                          f"amendment {fam['id']}")
        for sig, cur in unmatched.items():
            self.err("missing-historical-diagnostic",
                     f"baseline class no longer emitted by the stock checker: "
                     f"{cur.get('check')}|{cur.get('file')} — expected historical "
                     "diagnostic absent (history must be preserved, not cleared)")

    # ---------------- driver ----------------

    def run(self, checker_json):
        for rel in (COVERAGE_MD, COVERAGE_JSON, BASELINE_JSON, MANIFEST, LEDGER,
                    BUILD_INDEX, COVERAGE6_MD, COVERAGE6_JSON, PLAN6, SPEC6,
                    DEFERRALS):
            self.digest(rel)
        spec_rel = "docs/evidence-and-adoption-spec.md"
        spec_text = self.read(spec_rel)
        spec6_text = self.read(SPEC6)
        ledger_text = self.read(LEDGER)
        manifest_text = self.read(MANIFEST)
        index_text = self.read(BUILD_INDEX)
        self._deferrals = self.read(DEFERRALS) or ""
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
        # Round-6 frozen source (GUARD-X): register legs over the committed
        # REQ-X register and the folded partition-plan legs; one
        # implementation remains for both (CC1).
        if spec6_text is not None:
            by_id6 = self.check_coverage6(spec6_text, manifest_ids, kind_at)
            self.check_coverage6_md(by_id6)
            plan_rows = self.check_plan6(by_id6)
            self._init_paid_windows(plan_rows)
            pos = {r["id"]: n for n, r in enumerate(rows)}
            if active_round == ROUND6:
                self.check_markers6(active, pos, kind_at, decisions, spec6_text)
                self.check_window6(active, pos)
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
