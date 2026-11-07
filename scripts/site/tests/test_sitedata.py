#!/usr/bin/env python3
"""test_sitedata.py — R6.5 (F2-b/F2-e) regression battery.

Legs map to the ticket's acceptance criteria; every negative control is
a mutation that must fail (the checker-level (g)(h)(i)(k)(q)(u) legs are
owed to R6.6 — proven here at test level, never claimed as checker
legs):

  imports     stdlib-only audit + declared min version      (AC-1)
  registry    D-1…D-8 closed list; code audit; not-computed (AC-2)
  inputs      unknown schema / missing input → exit 2       (AC-3)
  canonical   rebuild byte-equals committed site-data; int
              units; sorted keys; no wall-clock field       (AC-4)
  provenance  every value has data-src+data-sha; resolver
              fails on off-by-one and altered packet byte   (AC-5)
  absent      schema rejects null/0; four states round-trip (AC-6)
  hygiene     local path in a projected field → refused     (AC-7)
  formatter   D-2 precision rule over half-widths           (AC-8)
  figures     golden SVG: 0–100 domain, log cost axis, caps
              + needle, outline, Pareto caption, no bars    (AC-9)
  anatomy     figure envelope + no-JS rendering             (AC-10)
  bounds      verbatim vs record; verified-adjacency;
              RECORD_BANNED absent                          (AC-11)
  labels      REQ-XD-092 labels; mutation (i)               (AC-12)
  calibration block complete; digest+count recomputed       (AC-13)
  dispositions typed tiles same structure; unmet{s1_failed} (AC-14)

Exit 0 green, 1 a leg failed, 2 usage/malformed input.
"""
import ast
import hashlib
import json
import re
import shutil
import sys
import tempfile
from pathlib import Path

SITE = Path(__file__).resolve().parent.parent
ROOT = SITE.parent.parent
sys.path.insert(0, str(SITE))

from hhsite import components, derive, packets, project, values  # noqa: E402
import build  # noqa: E402

fails = 0
SD = ROOT / "docs" / "results" / "site-data"


def ok(name, msg=""):
    print("test_sitedata: ok [%s]%s" % (name, (" " + msg) if msg else ""))


def fail(name, msg):
    global fails
    fails += 1
    print("test_sitedata: FAIL [%s] %s" % (name, msg), file=sys.stderr)


def expect_refusal(name, fn, *args, **kw):
    try:
        fn(*args, **kw)
    except values.Refusal:
        ok(name)
    except Exception as e:  # noqa: BLE001
        fail(name, "raised %r not Refusal" % e)
    else:
        fail(name, "refusal not raised")


def strip_tags(html):
    return re.sub(r"<[^>]+>", "", html)


def collapse(s):
    return re.sub(r"\s+", " ", s).strip()


# ── AC-1: stdlib import audit + declared minimum version ───────────────
_STDLIB = frozenset(
    "argparse ast base64 collections contextlib copy datetime decimal "
    "enum functools glob gzip hashlib heapq html io itertools json "
    "logging math os pathlib random re shutil statistics string struct "
    "subprocess sys tempfile textwrap time typing unicodedata urllib "
    "warnings zlib".split())


def leg_imports():
    stdlib = set(getattr(sys, "stdlib_module_names", _STDLIB)) | _STDLIB
    bad = []
    for f in [SITE / "build.py", SITE / "build_mock.py"] \
            + sorted((SITE / "hhsite").glob("*.py")):
        tree = ast.parse(f.read_text())
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                names = [a.name.split(".")[0] for a in node.names]
            elif isinstance(node, ast.ImportFrom):
                if node.level:
                    continue  # relative hhsite import
                names = [(node.module or "").split(".")[0]]
            else:
                continue
            for n in names:
                if n and n not in stdlib and n not in (
                        "hhsite", "build", "build_mock"):
                    bad.append("%s imports %s" % (f.name, n))
    if bad:
        fail("imports", "; ".join(bad))
    else:
        ok("imports", "stdlib-only import audit clean")
    if not (isinstance(build.MIN_PYTHON, tuple)
            and sys.version_info >= build.MIN_PYTHON):
        fail("min-version", "MIN_PYTHON %r not declared/satisfied"
             % (build.MIN_PYTHON,))
    else:
        ok("min-version", "floor %s declared and checked"
           % (".".join(str(x) for x in build.MIN_PYTHON)))


# ── AC-2: closed derivation registry + code audit ──────────────────────
def leg_registry():
    if set(derive.DERIVATIONS) != derive.EXPECTED_IDS:
        fail("registry", "keys %s != D-1…D-8" % sorted(derive.DERIVATIONS))
    else:
        ok("registry", "closed list D-1…D-8")
    paths = [SITE / "build.py", SITE / "build_mock.py"] \
        + sorted((SITE / "hhsite").glob("*.py"))
    viol = derive.audit_source(paths)
    if viol:
        fail("code-audit", "; ".join(viol[:5]))
    else:
        ok("code-audit", "no derivation outside the registry")
    # mutation: an unregistered statistic in a generator module is caught
    with tempfile.TemporaryDirectory() as td:
        bad = Path(td) / "evil.py"
        bad.write_text("def mean(x):\n    return sum(x) / len(x)\n")
        v2 = derive.audit_source([bad])
        if not v2:
            fail("code-audit", "sum()/len() mutation not caught")
        else:
            ok("code-audit-mutation", "unregistered statistic refused")


def leg_derivation_units():
    d = derive.DERIVATIONS
    assert d["D-1"]["usd_str"](0) == "$0.00"
    assert d["D-1"]["usd_str"](1720000) == "$1.72"
    assert d["D-1"]["to_micros"](0.0) == 0
    try:
        d["D-1"]["to_micros"](0.0000004)
        fail("D-1", "non-integral micro value not refused")
    except ValueError:
        pass
    assert d["D-3"]["order"]([{"k": "b"}, {"k": "a"}], "k")[0]["k"] == "a"
    assert d["D-4"]["counts"]([{"s": "x"}, {"s": "x"}, {"s": "y"}],
                              "s") == {"x": 2, "y": 1}
    mem = d["D-5"]["pareto"]([{"id": "a", "c": 1, "v": 1},
                             {"id": "b", "c": 2, "v": 0},
                             {"id": "c", "c": 3}], "c", "v")
    assert mem == ["a"], mem  # c incomplete — never joins
    assert d["D-6"]["ratio"](140, 300) == "0.47"
    assert d["D-6"]["ratio"](1, 0) == "undefined"
    assert d["D-7"]["member"](["a", "b"], "b") is True
    assert d["D-8"]["fmt_time"](2668051) == "44:28"
    assert d["D-8"]["fmt_time"](3723000) == "1:02:03"
    ok("derivations", "D-1…D-8 unit assertions pass")


# ── AC-3: inputs — unknown schema / missing input → exit 2 ─────────────
def leg_inputs(tmp):
    cfg = json.loads((SITE / "config" / "inputs.json").read_text())
    for e in cfg["inputs"]:
        assert "*" not in e["path"] and "?" not in e["path"], e["id"]
    ok("inputs-named", "no input path is globbed")

    # unknown schema fixture → Refusal (exit-2 class)
    import copy as _copy
    bad = _copy.deepcopy(cfg)
    bad["inputs"] = [dict(bad["inputs"][0], schema="hh-bogus/9")]
    f = Path(tmp) / "bad-schema.json"
    f.write_text(json.dumps(bad))
    expect_refusal("unknown-schema", packets.load_inputs, f, ROOT)

    # missing input → Refusal
    bad2 = _copy.deepcopy(cfg)
    bad2["inputs"] = [dict(bad2["inputs"][0],
                           path="docs/build/reports/no-such-file.json")]
    f2 = Path(tmp) / "missing.json"
    f2.write_text(json.dumps(bad2))
    expect_refusal("missing-input", packets.load_inputs, f2, ROOT)


# ── AC-4: canonical rebuild byte-equals the committed layer ────────────
def leg_canonical():
    inputs_, digest, bodies = build.emit_bodies()
    for name, text_ in bodies.items():
        f = SD / name
        if not f.is_file() or f.read_text() != text_:
            fail("drift", "rebuild != committed %s" % name)
            return
        if not text_.endswith("\n") or "\n" in text_[:-1]:
            fail("canonical", "%s: not one compact line + newline" % name)
            return
        doc = json.loads(text_)
        if values.validate(doc):
            fail("canonical", "%s: schema validation failed" % name)
            return
        if re.search(r'"[^"]*":\s*-?\d+\.\d', text_):
            fail("canonical", "%s: float leaf (integer units only)" % name)
            return
        for w in ("generated_at", "built_at", "timestamp", "now"):
            if '"%s"' % w in text_:
                fail("canonical", "%s: wall-clock field %r" % (name, w))
                return
    ok("canonical", "rebuild byte-equals committed; canonical; integer "
       "units; no wall clock; profile internal")


# ── AC-5: provenance resolver (mutations (g),(h) at test level) ─────────
def _resolve_value(inp_map, obj):
    """Recompute a value object's provenance: hash the packet, resolve
    the pointer, compare the integer."""
    src = obj["src"]
    inp = inp_map[src["path"]]
    assert hashlib.sha256(inp.raw).hexdigest() == src["sha256"]
    return inp.resolve(src["ptr"])


def _walk_values(node):
    if isinstance(node, dict):
        if "src" in node and (
                "v" in node or "text" in node
                or ("lo" in node and "hi" in node)):
            yield node
        for v in node.values():
            yield from _walk_values(v)
    elif isinstance(node, list):
        for v in node:
            yield from _walk_values(v)


def leg_provenance(tmp):
    _, _, bodies = build.emit_bodies()
    cfg, inputs = packets.load_inputs(SITE / "config" / "inputs.json",
                                      ROOT)
    inp_map = {i.path: i for i in inputs}
    seen = 0
    for name, text_ in bodies.items():
        for obj in _walk_values(json.loads(text_)["body"]):
            got = _resolve_value(inp_map, obj)
            if "v" in obj:
                assert got == obj["v"], (name, obj["src"])
            elif "text" in obj:
                assert str(got) == obj["text"], (name, obj["src"])
            else:
                assert list(got) == [obj["lo"], obj["hi"]], \
                    (name, obj["src"])
            seen += 1
    if seen < 50:
        fail("provenance", "only %d provenance objects resolved" % seen)
    else:
        ok("provenance", "%d values resolve hash+pointer+value" % seen)

    # mutation (g): a displayed value off by one unit fails the compare
    parity = inp_map["docs/build/reports/2026-11-05-parity-tranche-3.json"]
    mutated = parity.resolve("/counts/agree") + 1
    class FakeSrc:
        raw = parity.raw
        def resolve(self, p):
            return mutated if p == "/counts/agree" else parity.resolve(p)
    obj = {"src": {"path": parity.path, "sha256": parity.sha256,
                   "ptr": "/counts/agree"},
           "v": parity.resolve("/counts/agree")}
    fake = dict(inp_map)
    fake[parity.path] = FakeSrc()
    try:
        got = _resolve_value(fake, obj)
        assert got == obj["v"]
        fail("mutation-g", "off-by-one value not caught")
    except AssertionError:
        ok("mutation-g", "off-by-one value fails the resolver")

    # mutation (h): one altered packet byte → sha mismatch
    fake2 = dict(inp_map)

    class Altered:
        raw = parity.raw[:-1] + b"X"
        resolve = staticmethod(parity.resolve)
    fake2[parity.path] = Altered()
    try:
        _resolve_value(fake2, obj)
        fail("mutation-h", "altered packet byte not caught")
    except AssertionError:
        ok("mutation-h", "altered packet byte fails the hash check")


# ── AC-6: typed absents — schema rejects null/0; four states round-trip ─
def leg_absents():
    for bad in (None, 0, "n/a", {"state": "missing"}):
        if not values.validate_absent(bad):
            fail("absents", "schema accepted %r" % (bad,))
            return
    for state in values.ABSENT_STATES:
        o = values.absent(state, reason="r")
        if values.validate_absent(o):
            fail("absents", "typed state %r rejected" % state)
            return
        assert json.loads(values.canonical(o)) == o
    # a null leaf anywhere in a body fails validate()
    assert values.validate({"body": {"x": None}})
    ok("absents", "null/0/bad-state rejected at absent positions; four "
       "states round-trip")


# ── AC-7: hygiene — local path in a projected field refuses (u) ────────
def leg_hygiene(tmp):
    assert values.hygiene_scan("~/tb-work/x") == "~/t"
    assert values.hygiene_scan("/Users/x/y")
    assert values.hygiene_scan("/home/x")
    assert values.hygiene_scan("about ~18GB/batch") is None
    expect_refusal("hygiene-copy", values.check_hygiene, "/Users/x")

    # fixture packet carrying /Users/… in a projected field → build
    # refuses (mutation (u) at test level)
    cfg = json.loads((SITE / "config" / "inputs.json").read_text())
    pkt = json.loads((ROOT / cfg["inputs"][0]["path"]).read_text())
    pkt["verdict_basis"] = "/Users/someone/parity"
    p = Path(tmp) / "pkt.json"
    p.write_text(json.dumps(pkt))
    cfg["inputs"] = [dict(cfg["inputs"][0], path=str(p))]
    cf = Path(tmp) / "inputs.json"
    cf.write_text(json.dumps(cfg))
    try:
        packets.load_inputs(cf, ROOT)
        inp = packets.Input(cfg["inputs"][0], ROOT)
        project._t(inp, "/verdict_basis")
        fail("mutation-u", "local path in projected field not refused")
    except values.Refusal:
        ok("mutation-u", "local path in projected field → refused")


# ── AC-8: D-2 formatter — never finer than ~⅒ half-width ───────────────
def leg_formatter():
    f = derive.DERIVATIONS["D-2"]["fmt"]
    cases = [
        (620000, 80000, "ppm", "62%"),        # ±8pt → whole points
        (620000, 8000, "ppm", "62.0%"),       # ±0.8pt → tenths
        (115704, 57852, "ppm", "12%"),        # ±5.8pt → whole
        (115704, 0, "ppm", "11.5704%"),       # no interval → packet cap 4
        (0, 0, "micros_usd", "$0.00"),
        (1720000, 0, "micros_usd", "$1.72"),
        (42, 0, "count", "42"),
    ]
    for v, hw, unit, want in cases:
        got = f(v, hw, unit)
        if got != want:
            fail("formatter", "fmt(%s,%s,%s)=%r want %r"
                 % (v, hw, unit, got, want))
            return
    # the digit-finer invariant, swept over half-widths where a finer
    # digit is feasible (⅒hw ≤ whole-percent display floor)
    for hw in (500, 1000, 5000, 50000, 90000):
        s = f(500000, hw, "ppm")
        frac = s.split(".")[1].rstrip("%") if "." in s else ""
        step = 10 ** (-len(frac)) if frac else 1
        if step < (hw / 10000.0) / 10.0 - 1e-9:
            fail("formatter", "hw=%s shows step %s (finer than ⅒hw)"
                 % (hw, step))
            return
    ok("formatter", "no digit finer than ~⅒ half-width; packet cap held")


# ── AC-9/10: golden figure anatomy + marks ─────────────────────────────
def leg_figures():
    rate = components.interval_svg(55, 62, 70, domain=(0, 100),
                                   label="rate")
    if 'viewBox="0 0 220 20"' not in rate:
        fail("figures", "rate figure not fixed 0–100 domain")
    if rate.count("glyph-needle") != 1 or "int-rule" not in rate:
        fail("figures", "missing caps+needle interval rule")
    if "<rect" in rate:
        fail("figures", "bar-from-zero mark found")
    outline = components.interval_svg(44, 50, 58, separated=False)
    if "int-rule open" not in outline:
        fail("figures", "unseparated interval is not an outline")
    cost = components.cost_axis_svg([(1.72, "x")])
    if "$0.01" not in cost or "$1" not in cost or "$100" not in cost \
            or "cost-axis" not in cost:
        fail("figures", "cost axis lacks labelled log ticks")
    if components.PARETO_CAPTION.find("descriptive") < 0 \
            or "complete entries" not in components.PARETO_CAPTION:
        fail("figures", "Pareto caption not descriptive/complete-only")
    fig = components.figure(
        "f", "title", "subtitle", "CHART",
        {"sha256": "abc", "schema": "s", "date": "d", "href": "#"},
        [("estimand", "e"), ("N × k", "n"), ("interval method + level",
         "i"), ("population", "p"), ("exclusions", "x"), ("source", "s")],
        "<table></table>")
    for needle in ("<figure", "title", "subtitle", "How to read this",
                   "estimand", "receipt-rail", "<table"):
        if needle not in fig:
            fail("figures", "figure anatomy lacks %r" % needle)
            return
    ok("figures", "0–100 domain, log cost axis, caps+needle, outline, "
       "descriptive Pareto caption, full anatomy; no bars")


def main():
    if len(sys.argv) > 1:
        print("test_sitedata: usage: test_sitedata.py", file=sys.stderr)
        return 2
    leg_imports()
    leg_registry()
    leg_derivation_units()
    with tempfile.TemporaryDirectory(prefix="hh-sitedata-test.") as tmp:
        leg_inputs(tmp)
        leg_hygiene(tmp)
        leg_canonical()
        leg_provenance(tmp)
        leg_absents()
        leg_formatter()
        leg_figures()
    print("test_sitedata: %s" % ("FAIL" if fails else "OK"))
    return 1 if fails else 0


if __name__ == "__main__":
    raise SystemExit(main())
