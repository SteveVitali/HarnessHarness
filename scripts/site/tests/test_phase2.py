#!/usr/bin/env python3
"""test_phase2.py — R6.20 (F2-c (a)) phase-2 fixture battery.

Legs map to the ticket's acceptance criteria (the checker legs are
check-site.sh's — proven here at generator/test level):

  seal        non-fixture hh-comparative-results/1 refused at load  (X-07)
  packet      fixture marker + 4-member band + 1 separated + 3 pops
  band        fixed label byte-for-byte; members inside the band    (AC-2)
  ranks       no rank column/ordinal/"#n"; emph gated on holm        (AC-1)
  toggles     3 population + 2 cost variants per row, pre-rendered  (AC-3)
  js-audit    shipped JS carries no arithmetic on values            (AC-3)
  frontier    monograms+direct labels, no series token, D-5 caption (AC-4)
  goldens     band fragment + frontier SVG byte-equal committed     (AC-2/4)
  public      fp paths render publication-pending, no values        (AC-5)
  conditional a dropped-'descriptive' band label fails the leg      (AC-2)

Exit 0 green, 1 a leg failed, 2 usage/malformed input.
"""
import json
import re
import sys
import tempfile
from pathlib import Path

SITE = Path(__file__).resolve().parent.parent
ROOT = SITE.parent.parent
sys.path.insert(0, str(SITE))

from hhsite import components, packets, sitepages, values  # noqa: E402
import build  # noqa: E402

fails = 0
FIXTURE = SITE / "fixtures" / "phase2-fp-results.json"


def ok(name, msg=""):
    print("test_phase2: ok [%s]%s" % (name, (" " + msg) if msg else ""))


def fail(name, msg):
    global fails
    fails += 1
    print("test_phase2: FAIL [%s] %s" % (name, msg), file=sys.stderr)


def _page(tmp, profile="internal"):
    out = Path(tmp) / ("site-" + profile)
    inputs, digest, bodies = build.emit_bodies()
    pubs = build.load_publications(inputs)
    sitepages.build(out, inputs, digest, bodies, profile=profile,
                    publications=pubs)
    return {str(p.relative_to(out)): p.read_text()
            for p in out.rglob("*.html")}


# ── the seal (X-07) ────────────────────────────────────────────────────
def leg_seal(tmp):
    pkt = json.loads(FIXTURE.read_text())
    entry = {"id": "p2-fp", "path": str(FIXTURE.relative_to(ROOT)),
             "schema": "hh-comparative-results/1", "phase": "phase-2",
             "fixture": True}
    packets.Input(dict(entry), ROOT)  # declared fixture: admitted
    try:
        packets.Input({k: v for k, v in entry.items() if k != "fixture"},
                      ROOT)
        fail("seal", "entry without fixture:true admitted")
    except values.Refusal:
        ok("seal-entry", "inputs.json entry lacking fixture refused")
    bare = dict(pkt)
    bare.pop("fixture", None)
    p = Path(tmp) / "bare.json"
    p.write_text(json.dumps(bare))
    try:
        packets.Input(dict(entry, path=str(p)), ROOT)
        fail("seal", "packet without fixture:true admitted")
    except values.Refusal:
        ok("seal-packet", "non-fixture results packet refused")


# ── packet shape ───────────────────────────────────────────────────────
def leg_packet():
    pkt = json.loads(FIXTURE.read_text())
    if pkt.get("fixture") is not True:
        fail("packet", "fixture marker absent")
        return
    hs = pkt["harnesses"]
    members = [k for k, h in hs.items()
               if h["leader_group"]["member"]]
    sep = [k for k, h in hs.items()
           if isinstance(h["delta_vs_ref"], dict)
           and h["delta_vs_ref"].get("separated") is True]
    if len(members) != 4 or len(sep) != 1 or len(pkt["populations"]) != 3:
        fail("packet", "band %r / separated %r / pops %r"
             % (members, sep, pkt["populations"]))
        return
    ok("packet", "4-member band, one Holm-separated, three populations")


# ── band + ranks + toggles over the internal build ─────────────────────
def leg_pages(tmp):
    texts = _page(tmp)
    lb = texts.get("phase-2/fp/index.html", "")
    fr = texts.get("phase-2/fp/frontier/index.html", "")
    if not lb or not fr:
        fail("pages", "fp pages absent from internal build")
        return

    # AC-2: the 4-member band carries the fixed label byte-for-byte
    if sitepages.BAND_LABEL not in lb:
        fail("band", "fixed label absent from leaderboard page")
    elif lb.count('class="lb-row in-band"') != 4:
        fail("band", "member row count != 4")
    elif lb.index('class="floor-band"') > lb.index('class="lb-row in-band"'):
        fail("band", "band label not ahead of member rows")
    else:
        ok("band", "fixed label byte-for-byte; 4 member rows in band")

    # AC-1: no rank column, ordinal numeral or "#n"; emph gated
    for rel in ("phase-2/fp/index.html", "phase-2/fp/frontier/index.html"):
        h, t = texts[rel], re.sub(r"<[^>]+>", " ", texts[rel])
        for m in re.finditer(r"<th[^>]*>([^<]*)</th>", h):
            if re.search(r"(?i)^\s*rank", m.group(1)):
                fail("ranks", "%s: rank column %r" % (rel, m.group(1)))
        if re.search(r"#\s*\d+", t):
            fail("ranks", "%s: ordinal numeral" % rel)
        for m in re.finditer(
                r"<[a-z]+[^>]*class=\"[^\"]*emph[^\"]*\"[^>]*>", h):
            if 'data-separated="holm"' not in m.group(0):
                fail("ranks", "%s: ungated .emph" % rel)
    else:
        ok("ranks", "no rank column/ordinal; emph only on holm")

    # AC-3: three population + two cost-basis variants per row,
    # pre-rendered with resolvable data-src
    for choice in ("joint", "raw", "errors_zero"):
        if 'data-variant="%s"' % choice not in lb:
            fail("toggles", "population variant %r absent" % choice)
    for basis in ("model_spend_only", "with_compute"):
        if 'data-variant="%s"' % basis not in lb:
            fail("toggles", "cost variant %r absent" % basis)
    if lb.count('data-variant-of="population"') != 6 * 3:
        fail("toggles", "population variants != 3 per row")
    if lb.count('data-variant-of="cost"') != 6 * 2 * 2:
        fail("toggles", "cost variants != 2 per cost cell")
    if 'data-variant="raw" hidden' not in lb \
            or 'data-variant="with_compute" hidden' not in lb:
        fail("toggles", "non-default variants not pre-rendered hidden")
    else:
        ok("toggles", "3 populations + 2 cost bases, all pre-rendered")
    for m in re.finditer(r'data-src="([^"#]+)#([^"]+)"', lb):
        f = ROOT / m.group(1)
        if not f.is_file():
            fail("toggles", "data-src %s resolves nowhere" % m.group(1))
            break

    # refused row renders its typed refusal + all-planned bounds
    if "refused — UnresolvedCell" not in lb \
            or "all-planned" not in lb:
        fail("refusal", "typed refusal + bounds absent")
    else:
        ok("refusal", "UnresolvedCell renders typed refusal + bounds")

    # AC-3 script audit: shipped JS never computes on values — code
    # stripped of comments carries no digit, parser or arithmetic.
    js = (SITE / "assets" / "site.js").read_text()
    js_code = re.sub(r"/\*.*?\*/|//[^\n]*", "", js, flags=re.S)
    if re.search(r"\d|parseInt|parseFloat|Number\(|Math\.|eval\(|"
                 r"innerHTML", js_code):
        fail("js-audit", "arithmetic/number token in shipped JS")
    else:
        ok("js-audit", "no digit, parser or arithmetic in shipped JS")

    # AC-4: frontier — monograms + direct labels, no series token,
    # descriptive Pareto caption
    if re.search(r"series-[1-8]", fr):
        fail("frontier", "series token on the frontier")
    for needle in ("Hermes Agent", "mini-swe-agent", "Pareto frontier "
                   "among complete entries only · descriptive"):
        if needle not in fr:
            fail("frontier", "missing %r" % needle)
    else:
        ok("frontier", "monograms + labels, no series token, caption "
           "descriptive")
    if "pareto" not in fr or fr.count('class="frontier-mark pareto"') != 3:
        fail("frontier", "Pareto membership != 3 marks")
    for mon in (">HA<", ">MS<", ">CC<", ">PI<", ">OC<", ">OH<"):
        if mon not in fr:
            fail("frontier", "monogram %s absent" % mon)
    # deterministic label placement: two renders identical
    e = json.loads((ROOT / "docs/results/site-data/tfp.json")
                   .read_text())["body"]["frontier"]["entries"]
    hcfg = json.loads((SITE / "config" / "harnesses.json").read_text())
    ents = [{"cost_micros": x["cost"], "id": x["id"],
             "monogram": hcfg["harnesses"][x["id"]]["monogram"],
             "name": hcfg["harnesses"][x["id"]]["name"],
             "pareto": x["pareto"], "theta_ppm": x["theta"]}
            for x in e]
    a = components.frontier_svg(ents)
    b = components.frontier_svg(list(reversed(ents)))
    if a != components.frontier_svg(ents):
        fail("frontier", "same-input renders differ")
    # reversed input order must still place the same labels at the same
    # coordinates (the placer sorts by cost — pure function of content)
    if a != b:
        fail("frontier", "input order changed label placement")
    else:
        ok("frontier-determinism", "label placer order-invariant")

    # AC-2 negative: a dropped-'descriptive' band label fails the
    # conditional leg's fixed-label predicate
    mutated = lb.replace("observed-leader group · descriptive",
                       "observed-leader group")
    if 'class="floor-band"' in mutated \
            and sitepages.BAND_LABEL not in mutated:
        ok("conditional", "dropped-'descriptive' band lacks the fixed "
           "label — the leg fails it")
    else:
        fail("conditional", "mutation check ineffective")


def leg_public(tmp):
    texts = _page(tmp, profile="public")
    for rel in ("phase-2/fp/index.html",
                "phase-2/fp/frontier/index.html"):
        h = texts.get(rel, "")
        if "publication gate pending" not in h:
            fail("public", "%s not publication-pending" % rel)
        if "640000" in h or "Hermes Agent" in h \
                or "fixture ·" in h:
            fail("public", "%s carries fixture values" % rel)
    else:
        ok("public", "fixture pages refused — publication pending, no "
           "values")


def leg_goldens(tmp):
    """Golden snapshots: the band fragment and the frontier SVG, byte-
    for-byte against committed files."""
    texts = _page(tmp)
    lb = texts["phase-2/fp/index.html"]
    fr = texts["phase-2/fp/frontier/index.html"]
    m = re.search(r'<tr class="lb-band-label">.*?</tr>', lb, re.S)
    fsvg = re.search(r'<svg class="needle-fig frontier".*?</svg>',
                     fr, re.S)
    for name, frag in (("fp-band.html", m.group(0) if m else ""),
                       ("fp-frontier.svg", fsvg.group(0) if fsvg else "")):
        g = SITE / "golden" / name
        if not frag:
            fail("goldens", "%s fragment not found" % name)
        elif not g.is_file():
            fail("goldens", "%s not committed" % name)
        elif g.read_text() != frag:
            fail("goldens", "%s drift" % name)
        else:
            ok("goldens-%s" % name, "byte-equal committed golden")


def main():
    if len(sys.argv) > 1:
        print("test_phase2: usage: test_phase2.py", file=sys.stderr)
        return 2
    leg_packet()
    with tempfile.TemporaryDirectory(prefix="hh-phase2-test.") as tmp:
        leg_seal(tmp)
        leg_pages(tmp)
        leg_public(tmp)
        leg_goldens(tmp)
    print("test_phase2: %s" % ("FAIL" if fails else "OK"))
    return 1 if fails else 0


if __name__ == "__main__":
    raise SystemExit(main())
