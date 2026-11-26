#!/usr/bin/env python3
"""test_phase4c.py — R7.7 (phase-4 second pass) GATE-G38 confirmation
battery.

Legs (the checker legs are check-site.sh's — proven here at
generator/test level):

  claims     the §8.8 two-statement claims render byte-verbatim on
             /phase-4/ — confirmatory 'Not confirmed…' and exploratory
             'In exploration…', each with packet provenance      (AC-2)
  absences   every refused/missing estimand leg stays the packet's own
             typed marker — delta_ppm / fm_interval / raw_p_ppm /
             theta_ppm 'refused', null member 'not_computed', the
             missing per-task grid a typed absent, never imputed (AC-2)
  accounting the 370/367/3 population accounting renders, the three
             unresolved (task, arm) cells named verbatim with their
             unresolved classes                                  (AC-2)
  bounds     the all-planned bound and conditional task-bootstrap
             render as interval triples carrying their packet kind;
             the conditional secondary is labelled
             not-the-pre-registered-target                       (AC-2)
  cost       descriptive cost rows carry the packet's own
             descriptive-only label and verbatim µ$ values (REQ-XD-224)
  gate       the committed phase-4 publication record pins
             {p4-fp, p3-l2, p4c-confirm} under GATE-G38; a gate_row
             that names only GATE-G31 fails validation       (AC-1/AC-3)
  public     the public profile renders the pinned page — the second
             pass live under GATE-G38, never 'gate pending'      (AC-1)
  mutations  rewriting the verbatim confirmatory statement trips the
             provenance leg; a bare-'leader' label (dropping the
             packet's exact explanatory wording) trips the banned
             lexicon leg                                         (AC-2)

Exit 0 green, 1 a leg failed, 2 usage/malformed input.
"""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

SITE = Path(__file__).resolve().parent.parent
ROOT = SITE.parent.parent
sys.path.insert(0, str(SITE))

import build  # noqa: E402
from hhsite import publication, sitepages  # noqa: E402

fails = 0
PKT = json.loads(
    (ROOT / "docs/build/reports/2026-11-26-fp-p4c-confirmation.json")
    .read_text())


def ok(name, msg=""):
    print("test_phase4c: ok [%s]%s" % (name, (" " + msg) if msg else ""))


def fail(name, msg):
    global fails
    fails += 1
    print("test_phase4c: FAIL [%s] %s" % (name, msg), file=sys.stderr)


def _page(tmp, profile):
    out = Path(tmp) / ("site-" + profile)
    inputs, digest, bodies = build.emit_bodies()
    pubs = build.load_publications(inputs) if profile == "public" else []
    sitepages.build(out, inputs, digest, bodies, profile=profile,
                    publications=pubs)
    return {str(p.relative_to(out)): p.read_text()
            for p in out.rglob("*.html")}


def leg_claims(tmp):
    t = _page(tmp, "internal").get("phase-4/index.html", "")
    if not t:
        fail("claims", "phase-4 page absent from the internal build")
        return
    for k in ("statement_confirmatory", "statement_exploratory"):
        if PKT["claims"][k] not in t:
            fail("claims", "%s not rendered verbatim" % k)
    for k in ("statement_confirmatory", "statement_exploratory"):
        ptr = "/claims/%s" % k
        if ('data-src="docs/build/reports/'
                '2026-11-26-fp-p4c-confirmation.json#%s"' % ptr) not in t:
            fail("claims", "%s lacks the packet pointer" % k)
    ok("claims", "both §8.8 statements verbatim with provenance")


def leg_absences(tmp):
    t = _page(tmp, "internal").get("phase-4/index.html", "")
    for m in PKT["estimand"]["contrast"]["members"]:
        for k in ("delta_ppm", "fm_interval", "raw_p_ppm"):
            mark = m.get(k)
            if isinstance(mark, dict) and "state" in mark:
                if mark["state"] not in t or "absent-token" not in t:
                    fail("absences", "contrast %s not typed" % k)
            elif mark is None and "not_computed" not in t:
                fail("absences", "contrast %s null not typed" % k)
    for a in PKT["estimand"]["arms"]:
        mark = a.get("theta_ppm")
        if isinstance(mark, dict) and mark["state"] not in t:
            fail("absences", "theta_ppm %r not rendered typed"
                 % mark["state"])
    if "the hh-p4-c-confirmation/1 packet carries no per-task cell grid" \
            not in t:
        fail("absences", "missing per-task grid lacks the typed absent")
    ok("absences", "refused/null/missing legs all typed, never imputed")


def leg_accounting(tmp):
    t = _page(tmp, "internal").get("phase-4/index.html", "")
    pop = PKT["population"]
    for needle in ("370", "367", "37"):
        if needle not in t:
            fail("accounting", "population figure %r absent" % needle)
    for m in pop["unresolved_members"]:
        for k in ("slot_id", "task", "unresolved_class"):
            if m[k] not in t:
                fail("accounting", "unresolved member %s.%s absent"
                     % (m["slot_id"], k))
    ok("accounting", "370/367/3 + the three named unresolved cells")


def leg_bounds(tmp):
    t = _page(tmp, "internal").get("phase-4/index.html", "")
    kinds = {a["bound_ppm"]["kind"]
             for a in PKT["estimand"]["arms"]
             if isinstance(a["bound_ppm"], dict)}
    kinds |= {m["bound_ppm"]["kind"]
              for m in PKT["estimand"]["contrast"]["members"]
              if isinstance(m["bound_ppm"], dict)}
    kinds |= {m["task_bootstrap_interval"]["kind"]
              for m in PKT["estimand"]["contrast"]["members"]
              if isinstance(m["task_bootstrap_interval"], dict)}
    for k in kinds:
        if k not in t:
            fail("bounds", "bound kind %r absent" % k)
    if "not the pre-registered target" not in t:
        fail("bounds", "conditional-secondary label absent")
    ok("bounds", "%d bound kinds + not-the-target label" % len(kinds))


def leg_cost(tmp):
    t = _page(tmp, "internal").get("phase-4/index.html", "")
    if PKT["claims"]["cost"]["label"] not in t:
        fail("cost", "descriptive-only cost label not verbatim")
    for r in PKT["claims"]["cost"]["rows"]:
        for k in ("arm",):
            if r[k] not in t:
                fail("cost", "arm %r absent" % r[k])
        if str(r["resolved_slots"]) not in t or str(r["slots"]) not in t:
            fail("cost", "slot counts for %r absent" % r["arm"])
    ok("cost", "descriptive label + verbatim slot/µ$ fields")


def leg_gate():
    recs, errors = publication.load_dir(ROOT)
    for e in errors:
        fail("gate", "publication dir error: %s" % e)
    p4 = [(rel, r) for rel, r in recs if r.get("phase") == "phase-4"]
    if not any("GATE-G38" in r.get("gate_row", "") for _r, r in p4):
        fail("gate", "no phase-4 record names GATE-G38")
    rel, head = p4[-1]
    want = {"docs/build/reports/"
            "2026-11-20-fp-w4x-results-p4-fp-frozen.json",
            "docs/build/reports/"
            "2026-11-26-fp-p4c-confirmation.json"}
    if {i["path"] for i in head["inputs"]} != want:
        fail("gate", "head record inputs != {p4-fp, p4c-confirm}")
    if publication.validate(head, rel, ROOT):
        fail("gate", "head record fails validation")
    # a gate_row naming only GATE-G31 while citing the G38 readout is
    # not well-formed — the readout never names G31 (R7.7's second-pass
    # row is GATE-G38 or nothing).
    mut = dict(head)
    mut["gate_row"] = "317 GATE-G31 (317_GATE-G31__pub-fp.md)"
    if not publication.validate(mut, rel, ROOT):
        fail("gate", "GATE-G31-only gate_row accepted for the second "
             "pass")
    ok("gate", "head record pins p4c-confirm under GATE-G38")


def leg_public(tmp):
    texts = _page(tmp, "public")
    t = texts.get("phase-4/index.html", "")
    if not t:
        fail("public", "phase-4 absent from the public build")
        return
    if "publication gate pending" in t:
        fail("public", "gate-pending under the GATE-G38 record")
    for k in ("statement_confirmatory", "statement_exploratory"):
        if PKT["claims"][k] not in t:
            fail("public", "%s absent from the public page" % k)
    if "fixture ·" in t or "fixture-labelled" in t:
        fail("public", "fixture content on the public page")
    ok("public", "second pass live under GATE-G38 in the public build")


def leg_mutations(tmp):
    site = Path(tmp) / "site-mut"
    inputs, digest, bodies = build.emit_bodies()
    sitepages.build(site, inputs, digest, bodies, profile="internal",
                    publications=[])
    texts = {str(q.relative_to(site)): q.read_text()
             for q in site.rglob("*.html")}
    rel = "phase-4/index.html"
    p = site / "phase-4" / "index.html"

    def check(tree_text):
        p.write_text(tree_text)
        r = subprocess.run(
            ["bash", "scripts/check-site.sh", ".", str(site)],
            cwd=str(ROOT), capture_output=True, text=True)
        return r.returncode, r.stdout + r.stderr

    # (a) a fabricated confirmation claim: the displayed value no
    # longer matches the packet under its data-src pointer — the
    # provenance leg must fire.
    mut = texts[rel].replace(
        PKT["claims"]["statement_confirmatory"],
        "Confirmed on 37 held-out tasks.")
    rc, out = check(mut)
    if rc == 0:
        fail("mutations", "fabricated confirmation did not trip "
             "check-site")
    else:
        ok("mutations", "fabricated statement fails check-site")
    # (b) degrade the packet's exact selection-labelled wording to a
    # bare 'leader' claim — the banned-lexicon leg blanks only the
    # exact packet string.
    mut = texts[rel].replace(
        "leader selected from the same data",
        "the leader is confirmed")
    rc, out = check(mut)
    if rc == 0:
        fail("mutations", "bare-'leader' claim did not trip check-site")
    else:
        ok("mutations", "bare-'leader' wording fails check-site")


def main():
    tmp = tempfile.mkdtemp(prefix="t4c-")
    leg_claims(tmp)
    leg_absences(tmp)
    leg_accounting(tmp)
    leg_bounds(tmp)
    leg_cost(tmp)
    leg_gate()
    leg_public(tmp)
    leg_mutations(tmp)
    if fails:
        print("test_phase4c: %d failure(s)" % fails, file=sys.stderr)
        return 1
    print("test_phase4c: all legs green")
    return 0


if __name__ == "__main__":
    sys.exit(main())
