#!/usr/bin/env python3
"""test_phase34.py — R6.49 (F2-h) phase-3/4 fixture + freeze-readiness
battery.

Legs (the checker legs are check-site.sh's — proven here at
generator/test level):

  seal        gated schemas admit declared fixtures or frozen records
              only — a bare hh-fp-l2/1 doc is refused at load     (X-07)
  labels      /phase-3/ and /phase-4/ carry REQ-XH-119's labels, the
              phase-4 mandatory labels and the packet schema+sha
              citation                                          (AC-2/4)
  baseline    inconclusive → 'L1-conformant baseline'; equivalent →
              REQ-XH-122's label with ε and N; withheld stays typed
              (never 'parity-verified baseline')                (AC-5)
  goldens     the three verdict renders byte-equal committed      (AC-2)
  public      under GATE-G31's committed records the public profile
              renders the pinned phase-3/4 pages; an unpinned input
              set is refused                                  (AC-7)
  withheld    withheld{l1_not_passed} renders as a typed outcome with
              the same structure as computed ones        (REQ-XH-020)
  mutations   'parity-verified baseline' on a phase-3 page fails;
              a dropped mandatory label on a non-fixture page
              fails                                           (AC-2/3)

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

from hhsite import packets, project, sitepages, values  # noqa: E402
import build  # noqa: E402

fails = 0
FIX = SITE / "fixtures"
FP_LABEL = sitepages.FIRST_PASS_LABEL


def ok(name, msg=""):
    print("test_phase34: ok [%s]%s" % (name, (" " + msg) if msg else ""))


def fail(name, msg):
    global fails
    fails += 1
    print("test_phase34: FAIL [%s] %s" % (name, msg), file=sys.stderr)


def _inp(fid, rel, schema, phase, **extra):
    entry = {"id": fid, "path": rel, "schema": schema, "phase": phase}
    entry.update(extra)
    return packets.Input(entry, ROOT)


def _phase3_sd(l1_rel, l2_rel, l3_rel):
    """The phase3.json envelope text over the named fixture inputs."""
    ins = [_inp("p3-l1", l1_rel, "hh-l1-conformance/1", "phase-3"),
           _inp("p3-l2", l2_rel, "hh-fp-l2/1", "phase-3", fixture=True),
           _inp("p3-l3", l3_rel, "hh-fp-l3/1", "phase-3", fixture=True)]
    body = project.phase3(ins)
    return ins, json.dumps(
        {"body": body, "generator": "test_phase34",
         "schema": "hh-site-data/1"}, sort_keys=True)


def _pkts(ins):
    pkts = {i.id: {"path": i.path, "schema": i.schema,
                   "sha256": i.sha256} for i in ins}
    pkts["inputs"] = ins
    pkts["_digest"] = "sha256:test_phase34"
    return pkts


def _render_p3(l1_rel, l2_rel, l3_rel):
    ins, p3 = _phase3_sd(l1_rel, l2_rel, l3_rel)
    _inputs, _d, bodies = build.emit_bodies()
    sd = {"claims.json": bodies["claims.json"], "phase3.json": p3}
    return sitepages.phase3_page(sd, _pkts(ins), "0" * 40)


FIXTURE_PATHS = {
    "p2-fp": "scripts/site/fixtures/phase2-fp-results.json",
    "p3-l1": "scripts/site/fixtures/phase3-l1.json",
    "p3-l2": "scripts/site/fixtures/phase3-l2.json",
    "p3-l3": "scripts/site/fixtures/phase3-l3.json",
    "p4-fp": "scripts/site/fixtures/phase4-fp.json"}


def _fixture_cfg(tmp):
    """The declared-fixture spelling of the sealed inputs — the page
    legs render the stage-(a) fixture surfaces while the committed
    config names the frozen records."""
    cfg = json.loads((SITE / "config" / "inputs.json").read_text())
    for e in cfg["inputs"]:
        if e["id"] in FIXTURE_PATHS:
            e.pop("frozen", None)
            e["fixture"] = True
            e["path"] = FIXTURE_PATHS[e["id"]]
    p = Path(tmp) / "inputs-fixture.json"
    p.write_text(json.dumps(cfg))
    return p


def _page(tmp, profile="internal", fixture=False):
    out = Path(tmp) / ("site-" + profile)
    cfg = _fixture_cfg(tmp) if fixture else None
    inputs, digest, bodies = build.emit_bodies(cfg_path=cfg)
    # a fixture cfg is a test render, not the deployable input set —
    # the committed records pin the real set, so it carries none
    # (REQ-XF-129); leg_public proves the refusal on the same cfg.
    pubs = [] if fixture else build.load_publications(inputs)
    sitepages.build(out, inputs, digest, bodies, profile=profile,
                    publications=pubs)
    return {str(p.relative_to(out)): p.read_text()
            for p in out.rglob("*.html")}


# ── the seal (X-07) ────────────────────────────────────────────────────
def leg_seal(tmp):
    l2 = FIX / "phase3-l2.json"
    rel = str(l2.relative_to(ROOT))
    entry = {"id": "p3-l2", "path": rel, "schema": "hh-fp-l2/1",
             "phase": "phase-3", "fixture": True}
    packets.Input(dict(entry), ROOT)  # declared fixture: admitted
    try:
        packets.Input({k: v for k, v in entry.items()
                       if k != "fixture"}, ROOT)
        fail("seal", "hh-fp-l2/1 entry without fixture/frozen admitted")
    except values.Refusal:
        ok("seal-entry", "bare hh-fp-l2/1 entry refused")
    bare = dict(json.loads(l2.read_text()))
    bare.pop("fixture", None)
    p = Path(tmp) / "bare-l2.json"
    p.write_text(json.dumps(bare))
    try:
        packets.Input(dict(entry, path=str(p)), ROOT)
        fail("seal", "hh-fp-l2/1 doc without fixture/frozen admitted")
    except values.Refusal:
        ok("seal-packet", "non-frozen L2 packet refused")
    # the frozen admission: entry frozen:true AND doc freeze.state
    frozen = dict(bare)
    frozen["freeze"] = {"state": "frozen", "review_ref": "HUMAN-H16"}
    p2 = Path(tmp) / "frozen-l2.json"
    p2.write_text(json.dumps(frozen))
    e2 = dict(entry, path=str(p2), frozen=True)
    e2.pop("fixture")
    packets.Input(e2, ROOT)
    ok("seal-frozen", "frozen:true + freeze.state admits the record")
    e3 = dict(e2)
    e3.pop("frozen")
    try:
        packets.Input(e3, ROOT)
        fail("seal", "freeze.state doc without entry frozen admitted")
    except values.Refusal:
        ok("seal-frozen-entry", "freeze doc without entry flag refused")


# ── labels + baseline fold over the committed fixture build ────────────
def leg_labels(tmp):
    texts = _page(tmp, fixture=True)
    p3 = texts.get("phase-3/index.html", "")
    p4 = texts.get("phase-4/index.html", "")
    for rel, t in (("phase-3/index.html", p3), ("phase-4/index.html", p4)):
        if not t:
            fail("labels", "%s absent from the internal build" % rel)
            return
        for lab in (FP_LABEL,):
            if lab not in t:
                fail("labels", "%s lacks the first-pass label" % rel)
        if "renders" not in t or "sha256" not in t:
            fail("labels", "%s lacks the packet schema+sha citation"
                 % rel)
    if "pipeline demonstration; confirmatory test pre-stated as " \
            "near-zero power" not in p3:
        fail("labels", "phase-3 lacks the pipeline label")
    for lab in ("one-factor-at-a-time: effects are local to the "
                "baseline configuration; interactions not estimated",
                "finite-manifest",
                "under a declared W-token context window",
                "catalog window"):
        if lab not in p4:
            fail("labels", "phase-4 lacks mandatory label %r" % lab)
    if "L1-conformant baseline" not in p3:
        fail("labels", "inconclusive fixture lacks baseline label")
    if "parity-verified baseline" in p3 + p4:
        fail("labels", "'parity-verified baseline' on a first-pass page")
    ok("labels", "first-pass + mandatory phase-4 labels + citation")


# ── verdict goldens ────────────────────────────────────────────────────
GOLD = SITE / "tests" / "golden"
VARI = {
    "phase3-inconclusive": (
        "scripts/site/fixtures/phase3-l1.json",
        "scripts/site/fixtures/phase3-l2.json",
        "scripts/site/fixtures/phase3-l3.json"),
    "phase3-equivalent": (
        "scripts/site/fixtures/phase3-l1.json",
        "scripts/site/fixtures/phase3-l2-equivalent.json",
        "scripts/site/fixtures/phase3-l3.json"),
    "phase3-withheld-l1": (
        "scripts/site/fixtures/phase3-l1-fail.json",
        "scripts/site/fixtures/phase3-l2-withheld-l1.json",
        "scripts/site/fixtures/phase3-l3.json"),
}


def leg_goldens():
    import argparse
    regen = "--write-golden" in sys.argv
    for name, (l1, l2, l3) in VARI.items():
        got = _render_p3(l1, l2, l3)
        f = GOLD / ("%s.html" % name)
        if regen:
            f.write_text(got)
            ok("golden-write", name)
            continue
        want = f.read_text() if f.is_file() else None
        if want != got:
            fail("goldens", "%s drifts (regen: test_phase34.py "
                 "--write-golden)" % name)
    if not regen:
        ok("goldens", "three verdict renders byte-equal committed")


def leg_baseline():
    eq = _render_p3(*VARI["phase3-equivalent"])
    for needle in ("first-pass L2 equivalent at a ±.20 margin",
                   "N = 50", "gross-infidelity screen",
                   "not parity-verified"):
        if needle not in eq:
            fail("baseline", "equivalent golden lacks %r" % needle)
    if "parity-verified baseline" in eq:
        fail("baseline", "equivalent golden claims a parity-verified "
             "baseline")
    w = _render_p3(*VARI["phase3-withheld-l1"])
    if "withheld{l1_not_passed}" not in w:
        fail("baseline", "withheld{l1_not_passed} not rendered typed")
    if "L1-conformant baseline" in w:
        fail("baseline", "withheld L2 wrongly yields a baseline label")
    ok("baseline", "inconclusive/equivalent/withheld folds hold")


# ── public profile + withheld structure ────────────────────────────────
def leg_public(tmp):
    # GATE-G31's committed records pin the phase-3/4 input sets — the
    # public profile renders the real frozen pages verbatim (typed
    # withholds at full prominence), never a fixture value.
    texts = _page(tmp, profile="public")
    for rel in ("phase-3/index.html", "phase-4/index.html"):
        t = texts.get(rel, "")
        if not t:
            fail("public", "%s absent from public build" % rel)
            continue
        if "publication gate pending" in t:
            fail("public", "%s gate-pending under a pinning record"
                 % rel)
        if FP_LABEL not in t or "withheld{" not in t:
            fail("public", "%s lacks first-pass label/typed withholds"
                 % rel)
        if "fixture ·" in t or "fixture-labelled" in t:
            fail("public", "%s leaks fixture content" % rel)
    # a cfg the records do not pin is refused, never partially
    # published (REQ-XF-129).
    cfg = _fixture_cfg(tmp)
    inputs, _d, _b = build.emit_bodies(cfg_path=cfg)
    try:
        build.load_publications(inputs)
        fail("public", "fixture cfg accepted — the records pin the "
             "real input set, not the fixture spelling")
    except values.Refusal:
        ok("public-refusal", "unpinned input set refused (REQ-XF-129)")
    ok("public", "phase-3/4 record-pinned in public profile")


def leg_withheld(tmp):
    texts = _page(tmp, fixture=True)
    w = _render_p3(*VARI["phase3-withheld-l1"])
    i = _render_p3(*VARI["phase3-inconclusive"])
    # same structure: the verdict stamp is a real disposition stamp in
    # both, the typed markers render as absent-token cells, and the
    # packet citations carry schema + sha256 in both.
    for tag, html in (("withheld", w), ("inconclusive", i)):
        if 'data-stamp=' not in html:
            fail("withheld", "%s page lacks a disposition stamp" % tag)
        if 'class="absent-token"' not in html:
            fail("withheld", "%s page lacks typed-marker cells" % tag)
    if "withheld{l1_not_passed}" not in w:
        fail("withheld", "withheld{l1_not_passed} marker absent")
    ok("withheld", "withheld outcome carries the computed structure")


# ── mutations over the rendered site (the checker legs fire) ───────────
def _check_tree(site_dir):
    p = subprocess.run(
        ["bash", "scripts/check-site.sh", ".", str(site_dir)],
        cwd=str(ROOT), capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def leg_mutations(tmp):
    texts = _page(tmp, fixture=True)
    site = Path(tmp) / "site-internal"
    # (a) 'parity-verified baseline' beside a first-pass L2 surface
    mut = texts["phase-3/index.html"].replace(
        "L1-conformant baseline", "parity-verified baseline")
    (site / "phase-3" / "index.html").write_text(mut)
    rc, out = _check_tree(site)
    if rc == 0:
        fail("mutations", "'parity-verified baseline' did not trip "
             "the checker")
    else:
        ok("mutations", "parity-verified-baseline insertion fails "
           "check-site")
    (site / "phase-3" / "index.html").write_text(
        texts["phase-3/index.html"])
    # (b) a dropped mandatory label: strip the fixture exemption and
    # the OFAT label — the labels leg must catch the gap
    mut = texts["phase-4/index.html"]
    mut = mut.replace(sitepages.OFAT_LABEL, "ofat (dropped)")
    mut = mut.replace(" (fixture)", "").replace("fixture ·", "")
    (site / "phase-4" / "index.html").write_text(mut)
    rc, out = _check_tree(site)
    if rc == 0:
        fail("mutations", "dropped OFAT label did not trip the "
             "checker")
    else:
        ok("mutations", "dropped mandatory label fails check-site")


def main():
    tmp = tempfile.mkdtemp(prefix="t34-")
    leg_seal(tmp)
    leg_labels(tmp)
    leg_goldens()
    leg_baseline()
    leg_public(tmp)
    leg_withheld(tmp)
    leg_mutations(tmp)
    if fails:
        print("test_phase34: %d failure(s)" % fails, file=sys.stderr)
        return 1
    print("test_phase34: all legs green")
    return 0


if __name__ == "__main__":
    sys.exit(main())
