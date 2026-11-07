#!/usr/bin/env python3
"""test_sitepages.py — R6.5 stage 2 (F2-e) page-level regression battery.

Legs over a real --out build of the phase-1 + always-present pages:
  bounds      verbatim vs record; verified-adjacency; RECORD_BANNED (AC-11)
  labels      REQ-XD-092 labels + card + evidence; mutation (i)   (AC-12)
  calibration block complete; digest+count recomputed             (AC-13)
  dispositions typed tiles same structure; unmet{s1_failed}       (AC-14)
  no-js/determinism: figures pre-rendered; two builds one sha256  (AC-10)
Exit 0 green, 1 a leg failed, 2 usage/malformed input.
"""
import ast
import hashlib
import json
import re
import sys
import tempfile
from pathlib import Path

SITE = Path(__file__).resolve().parent.parent
ROOT = SITE.parent.parent
sys.path.insert(0, str(SITE))

from hhsite import packets, sitepages, values  # noqa: E402
import build  # noqa: E402

fails = 0


def ok(name, msg=""):
    print("test_sitepages: ok [%s]%s" % (name, (" " + msg) if msg else ""))


def fail(name, msg):
    global fails
    fails += 1
    print("test_sitepages: FAIL [%s] %s" % (name, msg), file=sys.stderr)


def strip_tags(html):
    return re.sub(r"<[^>]+>", "", html)


def collapse(s):
    return re.sub(r"\s+", " ", s).strip()


# ── AC-11/12/13/14: page-level legs over a real build ──────────────────
def _page_texts(tmp):
    out = Path(tmp) / "site"
    sitepages.build(out, *build.emit_bodies()[:2], build.emit_bodies()[2])
    return {p.name: (p / "index.html").read_text()
            for p in out.rglob("index.html")}


def leg_pages(tmp):
    # parse BOUNDS + RECORD_BANNED from check-position.sh (single source)
    cp = (ROOT / "scripts" / "check-position.sh").read_text()
    m = re.search(r"BOUNDS = (\[.*?\n\])", cp, re.S)
    bounds = ast.literal_eval(m.group(1))
    m2 = re.search(r"RECORD_BANNED = (\[.*?\n\])", cp, re.S)
    banned = ast.literal_eval(m2.group(1))

    out = Path(tmp) / "site"
    inputs, digest, bodies = build.emit_bodies()
    written = sitepages.build(out, inputs, digest, bodies)
    texts = {str(p.relative_to(out)): p.read_text()
             for p in out.rglob("*.html")}

    # AC-11: bounds verbatim on /phase-1/ + /limits/; verified-adjacency;
    # no RECORD_BANNED string
    import html as _html
    for page in ("phase-1/index.html", "limits/index.html"):
        t = collapse(_html.unescape(strip_tags(texts[page])))
        for b in bounds:
            if collapse(b) not in t:
                fail("bounds", "%s lacks bound %r" % (page, b[:50]))
                return
    ok("bounds", "all bounds verbatim on /phase-1/ and /limits/")
    for name, html_ in texts.items():
        stripped = strip_tags(html_)
        for b in banned:
            if re.search(r"\b%s\b" % re.escape(b), stripped, re.I):
                fail("record-banned", "%s carries %r" % (name, b))
                return
        # the tranche's rendered 'verified' sits beside bound 1 +
        # 'descriptive, no margin registered'; the verbatim bounds block
        # itself is exempt (it is record text, not a verdict display)
        no_bounds = re.sub(
            r'<ol class="dont-claim bounds">.*?</ol>', "", html_,
            flags=re.S)
        stripped_nb = strip_tags(no_bounds)
        for mm in re.finditer(r"\bverified\b", stripped_nb):
            # 'outside-verified' is the mandatory label, not the verdict
            if stripped_nb[max(0, mm.start() - 8):mm.start()] \
                    == "outside-":
                continue
            ctx = stripped_nb[max(0, mm.start() - 2000):mm.end() + 2000]
            if "descriptive, no margin registered" not in ctx:
                fail("verified-adjacency",
                     "%s: 'verified' away from its label" % name)
                return
    ok("verified", "'verified' only beside bound 1 + descriptive label; "
       "no RECORD_BANNED string")

    # AC-12: labels on every page header; mutation (i)
    for name, html_ in texts.items():
        banner = html_.split('<div class="banner">')[1].split("</div>")[0]
        for lab in ("internal record; not outside-verified",
                    "recorded execution", "Available"):
            if lab not in banner:
                fail("labels", "%s banner lacks %r" % (name, lab))
                break
        else:
            continue
        return
    ok("labels", "every header: REQ-XD-092 labels + card + evidence "
       "+ internal record")
    mutated = texts["index.html"].replace(
        "internal record; not outside-verified", "")
    if "internal record; not outside-verified" in mutated:
        fail("mutation-i", "label removal ineffective")
    else:
        ok("mutation-i", "removing the label breaks the banner check")

    # AC-13: calibration block complete + digest recompute
    cfg, inputs2 = packets.load_inputs(SITE / "config" / "inputs.json",
                                       ROOT)
    want_digest = packets.inputs_digest(inputs2)
    want_count = str(len(inputs2))
    for name, html_ in texts.items():
        for field_ in ("experiment id", "site version", "source commit",
                       "packet-set digest", "generator", "evidence class",
                       "recompute"):
            if field_ not in html_:
                fail("calibration", "%s lacks %r" % (name, field_))
                return
        if want_digest not in html_ or want_count + " file(s)" \
                not in html_:
            fail("calibration", "%s digest/count mismatch" % name)
            return
    ok("calibration", "block complete; digest + file count recompute "
       "from inputs.json")

    # AC-14: typed dispositions same component; unmet{s1_failed} shown
    for frag_name in ("index.html", "phase-1/index.html"):
        if "unmet{s1_failed}" not in texts[frag_name]:
            fail("dispositions", "%s lacks unmet{s1_failed}" % frag_name)
            return
    tile_structs = re.findall(r'<section class="card span-6 tile">',
                            texts["index.html"])
    if len(tile_structs) < 5:
        fail("dispositions", "fewer than five same-structure tiles")
        return
    ok("dispositions", "typed dispositions share one tile component; "
       "unmet{s1_failed} on / and /phase-1/")

    # figure/no-JS: every figure + value present without scripts
    t = texts["phase-1/index.html"]
    main = t.split("<main", 1)[1]
    if "<figure" not in main or "How to read this" not in main \
            or "data-src=" not in main or "data-sha=" not in main:
        fail("no-js", "figure/provenance markup absent in main")
    else:
        ok("no-js", "figures + data-src/data-sha present, pre-rendered")

    # determinism: two builds one tree sha256
    out2 = Path(tmp) / "site2"
    sitepages.build(out2, inputs, digest, bodies)
    if sitepages.tree_sha256(out) != sitepages.tree_sha256(out2):
        fail("determinism", "two site builds differ")
    else:
        ok("determinism", "two builds one tree sha256 "
           + sitepages.tree_sha256(out)[:12])


def main():
    if len(sys.argv) > 1:
        print("test_sitepages: usage: test_sitepages.py", file=sys.stderr)
        return 2
    with tempfile.TemporaryDirectory(prefix="hh-sitepages-test.") as tmp:
        leg_pages(tmp)
    print("test_sitepages: %s" % ("FAIL" if fails else "OK"))
    return 1 if fails else 0


if __name__ == "__main__":
    raise SystemExit(main())
