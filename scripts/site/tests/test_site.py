#!/usr/bin/env python3
"""test_site.py — R6.4 component-level regression battery.

Proves each spec predicate at component level (the (t)/(p)/(r) checker
legs and the no-third-party leg are R6.6's — never claimed here):
  monograms   uniqueness + coverage, mutation → fail (REQ-XF-107)
  skeleton    no digit inside .absent-skeleton, mutation (r) → fail
              (REQ-XF-114)
  charts      status+series in one chart raises, mutation (t) → fail
              (REQ-XF-104); scatter marks are monogram+label only
              (REQ-XF-103)
  stamps      colour-only stamp raises, mutation (p) → fail (REQ-XF-105)
  theme       inline hashed init script + JS-off media query (REQ-XF-101)
  fonts       no font file / @font-face / font request (REQ-XF-106)
  determinism two builds byte-identical; golden all-absent manifest
              covers every §9.2 path (REQ-XF-114, AC-9)
Exit 0 green, 1 a leg failed, 2 usage/malformed input.
"""
import hashlib
import json
import re
import shutil
import sys
import tempfile
from pathlib import Path

SITE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SITE))

from hhsite import components, pages, raster, tokparse  # noqa: E402
import build_mock  # noqa: E402

fails = 0


def ok(name, msg=""):
    print("test_site: ok [%s]%s" % (name, (" " + msg) if msg else ""))


def fail(name, msg):
    global fails
    fails += 1
    print("test_site: FAIL [%s] %s" % (name, msg), file=sys.stderr)


def expect_guard(name, fn, *args, **kw):
    try:
        fn(*args, **kw)
    except components.GuardError:
        ok(name)
    except Exception as e:  # noqa: BLE001
        fail(name, "raised %r, not GuardError" % e)
    else:
        fail(name, "mutation not caught")


# ── monograms (REQ-XF-107) ────────────────────────────────────────────
def validate_harnesses(cfg):
    errs = []
    order, table = cfg["admission_order"], cfg["harnesses"]
    for hid in order:
        if hid not in table:
            errs.append("admitted harness %r has no monogram" % hid)
    for hid in table:
        if hid not in order:
            errs.append("monogram entry %r not in admission order" % hid)
    seen, names = {}, {}
    for hid, h in table.items():
        m = h.get("monogram", "")
        if len(m) != 2:
            errs.append("monogram %r for %r is not two characters" % (m, hid))
        if m in seen:
            errs.append("monogram collision %r: %r and %r" % (m, seen[m], hid))
        seen[m] = hid
        n = h.get("name", "")
        if not n:
            errs.append("empty display name for %r" % hid)
        if n in names:
            errs.append("display-name collision %r" % n)
        names[n] = hid
    return errs


def leg_monograms():
    cfg = json.loads((SITE / "config" / "harnesses.json").read_text())
    errs = validate_harnesses(cfg)
    if errs:
        fail("monograms", "; ".join(errs))
        return
    ok("monograms", "%d harnesses, unique monograms" % len(cfg["harnesses"]))
    bad = json.loads(json.dumps(cfg))
    bad["harnesses"]["pi"]["monogram"] = "MS"
    if not validate_harnesses(bad):
        fail("monograms", "collision mutation not caught")
    bad2 = json.loads(json.dumps(cfg))
    del bad2["harnesses"]["pi"]
    if not validate_harnesses(bad2):
        fail("monograms", "uncovered-harness mutation not caught")


# ── chart guards (REQ-XF-103/104) ─────────────────────────────────────
def leg_charts():
    expect_guard("chart-t-mutation", components.validate_chart_tokens,
                 {"st-verified", "series-3"})
    components.validate_chart_tokens({"st-verified", "st-failed", "ink"})
    components.validate_chart_tokens({"series-1", "series-2", "axis"})
    ok("chart-separation", "status-only and series-only charts pass")
    expect_guard("scatter-series", components.scatter_mark,
                 "MS", "mini-swe-agent", series="series-3")
    expect_guard("scatter-nolabel", components.scatter_mark, "MS", "")
    components.scatter_mark("MS", "mini-swe-agent")
    ok("scatter-marks", "monogram + direct label only")


# ── stamps (REQ-XF-105) ───────────────────────────────────────────────
def leg_stamps():
    for kind in components.DISPOSITIONS:
        components.stamp(kind, "text")
    ok("stamps", "all eight glyphs render text+glyph+colour")
    colour_only = re.sub(r'<span class="stamp-text">.*?</span>',
                         '<span class="stamp-text"></span>',
                         components.stamp("verified", "verified{…}"))
    expect_guard("stamp-p-mutation", components.validate_stamp, colour_only)


# ── skeletons (REQ-XF-114) ────────────────────────────────────────────
def leg_skeletons():
    components.skeleton()
    ok("skeleton", "builder output digit-free")
    frag = components.skeleton().replace(
        '<div class="bar w-mid"></div>', '<div class="bar w-mid">7</div>')
    expect_guard("skeleton-r-mutation", components.validate_skeleton, frag)


# ── fonts + third-party scan (REQ-XF-106) ─────────────────────────────
FONT_RE = re.compile(
    r"@font-face|\.woff2?|\.ttf|\.otf|fonts\.googleapis|fonts\.gstatic|"
    r"@import|url\(\s*['\"]?https?://|src=\"https?://|src='https?://",
    re.I)


def leg_fonts(built):
    hits = []
    for f in Path(built).rglob("*"):
        if f.is_file():
            for m in FONT_RE.finditer(f.read_bytes().decode("utf-8", "replace")):
                hits.append("%s: %s" % (f.name, m.group(0)))
    if hits:
        fail("fonts", "font request/third-party ref: " + "; ".join(hits[:5]))
    else:
        ok("fonts", "no font file, @font-face or font request in output")


# ── theme (REQ-XF-101) ────────────────────────────────────────────────
def leg_theme(built):
    index = (Path(built) / "index.html").read_text()
    if pages.THEME_INIT not in index:
        fail("theme", "inline before-paint theme script absent")
    if "sha256-" not in index or "Content-Security-Policy" not in index:
        fail("theme", "CSP hash for the init script absent")
    if 'class="theme-toggle" aria-pressed=' not in index:
        fail("theme", "toggle is not a <button aria-pressed>")
    css = (SITE / "assets" / "tokens.css").read_text()
    if "prefers-color-scheme" not in css:
        fail("theme", "JS-off media-query theme absent")
    ok("theme", "init hashed, aria-pressed toggle, media-query fallback")


# ── determinism + golden all-absent site (REQ-XF-114, AC-9) ───────────
def leg_build(tmp):
    a, b = Path(tmp) / "a", Path(tmp) / "b"
    build_mock.build(a, "internal")
    build_mock.build(b, "internal")
    if build_mock.tree_sha256(a) != build_mock.tree_sha256(b):
        fail("determinism", "two mock builds differ")
        return
    ok("determinism", "two builds, one tree sha256 "
       + build_mock.tree_sha256(a)[:12])
    for png in ("favicon.png", "og.png"):
        if (a / "assets" / png).read_bytes() != (b / "assets" / png).read_bytes():
            fail("png-stability", png + " differs across builds")
    ok("png-stability", "favicon.png + og.png byte-stable")

    leg_fonts(a)
    leg_theme(a)

    # golden all-absent: every §9.2 path (specimen is internal-only)
    g = Path(tmp) / "absent"
    build_mock.build(g, "all-absent")
    missing, digits = [], []
    for path, title, absent, section in pages.URL_MAP:
        if path == "specimen/":
            continue
        f = g / path / "index.html" if path else g / "index.html"
        if not f.exists():
            missing.append(path or "/")
            continue
        text = f.read_text()
        if absent:
            reason, schema, gate = absent
            for needle in (reason, schema, gate):
                if needle not in text:
                    fail("absent-fields", "%s lacks %r" % (path, needle))
        for body in components._skeleton_bodies(text):
            if re.search(r"\d", body):
                digits.append(path or "/")
    if missing:
        fail("url-map", "paths missing: " + ", ".join(missing))
    else:
        ok("url-map", "every §9.2 path incl. /method/ renders")
    if digits:
        fail("skeleton-digits", "digit inside skeleton: " + ", ".join(digits))
    else:
        ok("skeleton-digits", "no digit in any built skeleton")

    # golden manifest compare
    manifest = (SITE / "golden" / "all-absent.manifest")
    if not manifest.exists():
        fail("golden", "golden/all-absent.manifest not committed")
        return
    lines = [l.split() for l in manifest.read_text().splitlines() if l.strip()]
    tree_line = [l[1] for l in lines if l[0] == "tree"]
    file_map = {l[1]: l[0] for l in lines if l[0] != "tree"}
    drift = []
    for f in sorted(g.rglob("*")):
        if f.is_file():
            rel = str(f.relative_to(g))
            got = hashlib.sha256(f.read_bytes()).hexdigest()
            if file_map.get(rel) != got:
                drift.append(rel)
    if drift:
        fail("golden", "all-absent drift: " + ", ".join(drift[:5]))
    elif not tree_line or tree_line[0] != build_mock.tree_sha256(g):
        fail("golden", "tree sha256 disagrees with manifest")
    else:
        ok("golden", "all-absent build matches committed golden "
           + tree_line[0][:12])


def main():
    if len(sys.argv) > 1:
        print("test_site: usage: test_site.py", file=sys.stderr)
        return 2
    leg_monograms()
    leg_charts()
    leg_stamps()
    leg_skeletons()
    with tempfile.TemporaryDirectory(prefix="hh-site-test.") as tmp:
        leg_build(tmp)
    print("test_site: %s" % ("FAIL" if fails else "OK"))
    return 1 if fails else 0


if __name__ == "__main__":
    raise SystemExit(main())
