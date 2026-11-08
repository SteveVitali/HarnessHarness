#!/usr/bin/env python3
"""test_phase2b.py — R6.21 (F2-c (b) + F2-d) acceptance battery.

Legs map to the ticket ACs (checker legs live in check-site.sh):

  noise     k = 1 → "not computed — R=1"; k ≥ 2 rerun floor + swap;
            k ≥ 3 all-k/any-k; absent harness_excess → not computed  (AC-1)
  refusal   typed refusal + all-planned bounds; undefined (0 solves) (AC-1)
  pilot     name-sorted, label present, no band/group/ordering       (AC-2)
  quote     menu values equal the fixture packet (provenance)        (AC-6)
  p1-grid   30 cells matching tranche-3 verdicts; "not HH" label;
            arm-prefix caveat; cell → trial links; marks             (AC-3)
  trials    noindex; canary; withheld{dataset_private} + sha256, no
            link; readable without JS                                (AC-4/5)
  viewer    dormant source contract: click-gated fetch, SubtleCrypto
            sha256, refused{sha256_mismatch}, textContent + ANSI
            strip, size cap, no innerHTML/eval                       (AC-4)
  golden    the p1 grid fragment byte-equals the committed golden

Exit 0 green, 1 a leg failed.
"""
import json
import re
import sys
import tempfile
from pathlib import Path

SITE = Path(__file__).resolve().parent.parent
ROOT = SITE.parent.parent
sys.path.insert(0, str(SITE))

import build  # noqa: E402
from hhsite import sitepages  # noqa: E402

fails = 0


def ok(n, m=""):
    print("test_phase2b: ok [%s]%s" % (n, (" " + m) if m else ""))


def fail(n, m):
    global fails
    fails += 1
    print("test_phase2b: FAIL [%s] %s" % (n, m), file=sys.stderr)


def _page(tmp, profile="internal"):
    out = Path(tmp) / ("site-" + profile)
    inputs, digest, bodies = build.emit_bodies()
    pubs = build.load_publications(inputs)
    sitepages.build(out, inputs, digest, bodies, profile=profile,
                    publications=pubs)
    return {str(p.relative_to(out)): p.read_text()
            for p in out.rglob("*.html")}, bodies


def main():
    with tempfile.TemporaryDirectory(prefix="hh-p2b.") as tmp:
        texts, bodies = _page(tmp)
        tfp = json.loads(bodies["tfp.json"])["body"]
        trials = json.loads(bodies["trials-fp.json"])["body"]
        fixture = json.loads(
            (SITE / "fixtures" / "phase2-fp-results.json").read_text())

        # ── AC-1: noise ───────────────────────────────────────────
        no = texts.get("phase-2/fp/noise/index.html", "")
        if not no:
            fail("noise", "noise page absent")
        else:
            if "not computed — R=1" not in no:
                fail("noise", "k=1 row lacks 'not computed — R=1'")
            for s in ("rerun flip — the noise floor", "harness_excess",
                      "all-k", "any-k"):
                if s not in no:
                    fail("noise", "missing %r" % s)
            if no.count("not_computed") < 2:
                fail("noise", "absent estimator not rendered")
            else:
                ok("noise", "k ladder renders all branches")

        # ── AC-1: refusal + undefined ─────────────────────────────
        lb = texts.get("phase-2/fp/index.html", "")
        if "refused — UnresolvedCell" not in lb \
                or "all-planned" not in lb:
            fail("refusal", "typed refusal/bounds missing on leaderboard")
        elif "undefined (0 solves)" not in lb:
            fail("refusal", "'undefined (0 solves)' absent")
        else:
            ok("refusal", "typed refusal + bounds + undefined (0 solves)")

        # ── AC-2: pilot ───────────────────────────────────────────
        pl = texts.get("phase-2/pilot/index.html", "")
        if not pl:
            fail("pilot", "pilot page absent")
        else:
            arms = re.findall(r'<tr data-arm="([^"]+)"', pl)
            if arms != sorted(arms) or not arms:
                fail("pilot", "rows not name-sorted")
            for mark in ("floor-band", "lb-band-label", "in-band",
                         'data-separated="holm"'):
                if mark in pl:
                    fail("pilot", "ranks mark %r present" % mark)
            if "exploratory; measures, never ranks" not in pl:
                fail("pilot", "mandatory label absent")
            else:
                ok("pilot", "name-sorted, labelled, no ordering marks")

        # ── AC-6: quote ───────────────────────────────────────────
        qp = texts.get("phase-2/quote/index.html", "")
        qfix = json.loads(
            (SITE / "fixtures" / "phase2-quote.json").read_text())
        if not qp:
            fail("quote", "quote page absent")
        else:
            from hhsite import derive
            usd = derive.DERIVATIONS["D-1"]["usd_str"]
            miss = [usd(r["model"]["expected_micros"])
                    for r in qfix["rows"]
                    if usd(r["model"]["expected_micros"]) not in qp]
            if miss or qfix["rows"][0]["verdict"] not in qp \
                    or qfix["envelope_label"] not in qp:
                fail("quote", "menu values != fixture packet")
            else:
                ok("quote", "menu rows + verdicts verbatim")

        # ── AC-3: phase-1 grid ────────────────────────────────────
        g = texts.get("phase-1/tasks/index.html", "")
        p1 = json.loads(bodies["phase1.json"])["body"]
        # ground truth = the packet the site consumed, resolved through
        # its declared input (not a hardcoded path).
        t3_rel = next(e["path"] for e in json.loads(
            (SITE / "config" / "inputs.json").read_text())["inputs"]
                      if e["id"] == "parity-t3")
        t3 = json.loads((ROOT / t3_rel).read_text())
        if not g:
            fail("p1-grid", "grid absent")
        else:
            if len(p1["tasks"]) != 30:
                fail("p1-grid", "cell count %d != 30" % len(p1["tasks"]))
            for i, c in enumerate(p1["tasks"]):
                pr = t3["pairs"][i]
                if c["o_verdict"]["text"] != pr["o_verdict"] \
                        or c["h_verdict"]["text"] != pr["h_verdict"] \
                        or c["task_id"]["text"] != pr["task_id"]:
                    fail("p1-grid", "task %d verdict mismatch" % i)
                    break
            for s in ("not HH", "o-", "h-", "arm prefix", "prefix"):
                if s not in g:
                    fail("p1-grid", "missing %r" % s)
            if g.count("../trial/") < 30:
                fail("p1-grid", "trial links missing")
            if sitepages.CANARY not in g:
                fail("p1-grid", "canary absent")
            else:
                ok("p1-grid", "30 cells, verdicts match, caveats + marks")

        # ── AC-4/5: trial pages ───────────────────────────────────
        tr = texts.get("phase-1/trial/fix-git/index.html", "")
        fr = texts.get("phase-2/fp/trial/fp-alpha-hermes-agent/"
                       "index.html", "")
        if 'name="robots" content="noindex"' not in tr \
                or 'name="robots" content="noindex"' not in fr:
            fail("trials", "noindex absent")
        if sitepages.CANARY not in tr or sitepages.CANARY not in fr:
            fail("trials", "canary absent")
        if "withheld{dataset_private}" not in fr \
                or "huggingface.co" in fr:
            fail("trials", "private index leaks a link")
        if "transcript" not in fr or "sha256" not in fr:
            fail("trials", "receipt sha256 absent")
        else:
            ok("trials", "noindex + canary + withheld sha256, no link")

        # ── AC-4: dormant viewer source contract ──────────────────
        js = (SITE / "assets" / "viewer.js").read_text()
        for banned in ("innerHTML", "outerHTML", "insertAdjacentHTML",
                       "document.write", "eval(", "src="):
            if banned in js:
                fail("viewer", "banned construct %r" % banned)
        for need in ('addEventListener("click"', "crypto.subtle.digest",
                     "refused{sha256_mismatch}", "textContent",
                     "withheld{dataset_private}"):
            if need not in js and need not in fr:
                fail("viewer", "missing %r" % need)
        if js.count("fetch(") != 1 \
                or not (js.index("function load") < js.index("fetch(")
                        < js.index("addEventListener")) \
                or "addEventListener" not in js \
                or "click" not in js.split("addEventListener", 1)[1]:
            fail("viewer", "fetch outside click handler")
        else:
            ok("viewer", "click-gated fetch, sha256, text-only, cap")

        # ── golden: the phase-1 grid fragment ─────────────────────
        m = re.search(r'<table class="data grid">.*?</table>', g, re.S)
        golden = SITE / "golden" / "p1-tasks-grid.html"
        frag = m.group(0) if m else ""
        if not golden.is_file():
            fail("golden", "p1-tasks-grid.html not committed")
        elif golden.read_text().rstrip("\n") != frag:
            fail("golden", "grid drift")
        else:
            ok("golden", "p1 grid byte-equal committed golden")

        # fp grid sanity
        fg = texts.get("phase-2/fp/tasks/index.html", "")
        if fg.count("grid-cell") < 60 or 'data-variant' in fg \
                and "withheld{dataset_private}" not in texts.get(
                    "phase-2/fp/trial/index.html", ""):
            fail("fp-grid", "fp grid/links wrong")
        else:
            ok("fp-grid", "60 linked cells")

    print("test_phase2b: %s" % ("FAIL" if fails else "OK"))
    return 1 if fails else 0


if __name__ == "__main__":
    raise SystemExit(main())
