"""pages — the §9.2 URL map, the page shell and the mock pages.

Every path renders; a page without its input renders the designed absent
state (typed reason, awaited schema, gate id, numberless skeleton). The
mock build adds the home page on phase-1 fixtures and one fixture
specimen page; both carry the fixture label and are internal-profile
only (REQ-XF-142 keeps them out of any public build).
"""
import hashlib

from . import GENERATOR_ID, SITE_VERSION
from .components import (ESC, absent_block, calibration_block, interval_svg,
                         monogram, noise_floor_band, receipt_rail, skeleton,
                         stamp, stamp_legend)

# Before-paint theme resolution: saved choice, then prefers-color-scheme,
# then an optional #theme= preview override for the mock-gate matrix.
# Hashed into the CSP meta at render time (design/39 F13).
THEME_INIT = (
    "try{var t=localStorage.getItem('hh-theme')}catch(e){var t=null}\n"
    "var h=(location.hash.match(/theme=(light|dark)/)||[])[1];if(h)t=h;\n"
    "if(t==='light'||t==='dark')document.documentElement.dataset.theme=t;")

CSP = ("default-src 'self'; script-src 'self' 'sha256-%s'; "
       "style-src 'self'; img-src 'self'; font-src 'self'; "
       "connect-src 'self'; base-uri 'none'; form-action 'none'")

NAV = [("Overview", ""), ("Phase 1", "phase-1/"), ("Phase 2", "phase-2/"),
       ("Phase 3", "phase-3/"), ("Phase 4", "phase-4/"), ("Method", "method/"),
       ("Spend", "spend/"), ("Limits", "limits/"), ("Data", "data/"),
       ("About", "about/")]

DONT_CLAIM = [
    "No rank ordering is claimed from a fixture or a single tranche.",
    "The phase-1 record is an internal record; it is not outside-verified.",
    "Fixture-labelled values are presentation fixtures, never results."]

EVIDENCE_VOCAB = [
    "recorded execution", "fixture-verified", "live-executed",
    "foreign-run-verified", "environment-pending"]

# ── §9.2 URL map ───────────────────────────────────────────────────────
# path -> (title, absent | None, section). absent = (reason, schema, gate)
# for data pages; None = always-present static page.
URL_MAP = [
    ("", "Overview", None, "home"),
    ("phase-1/", "Phase 1 — execution-surface replay record",
     ("not_yet_available", "hh-surface-xcheck/1 · hh-p1-estimators/1", "F2-b"), "phase-1"),
    ("phase-1/tasks/", "Phase 1 — task grid",
     ("not_yet_available", "hh-surface-xcheck/1", "F2-b"), "phase-1"),
    ("phase-2/", "Phase 2 — comparative overview",
     ("not_yet_available", "comparative-quote/1", "FP-S"), "phase-2"),
    ("phase-2/pilot/", "Phase 2 — pilot",
     ("not_yet_available", "hh-pilot-measure/1", "FP-S"), "phase-2"),
    ("phase-2/quote/", "Phase 2 — quote menu",
     ("not_yet_available", "comparative-quote/1", "FP-E"), "phase-2"),
    ("phase-2/fp/", "Phase 2 — first-pass tranche",
     ("not_yet_available", "hh-comparative-results/1", "FP-RESULTS"), "phase-2"),
    ("phase-2/fp/frontier/", "Phase 2 — accuracy vs cost frontier",
     ("not_yet_available", "hh-comparative-results/1", "FP-RESULTS"), "phase-2"),
    ("phase-2/fp/noise/", "Phase 2 — noise",
     ("not_yet_available", "hh-comparative-results/1", "FP-RESULTS"), "phase-2"),
    ("phase-2/fp/tasks/", "Phase 2 — per-task grid",
     ("not_yet_available", "hh-comparative-results/1", "FP-RESULTS"), "phase-2"),
    ("phase-2/fp/trial/", "Phase 2 — trial index",
     ("withheld{dataset_private}", "hh-receipt-index/1",
      "GATE pub-FP + dataset flip"), "phase-2"),
    ("phase-3/", "Phase 3 — HIR parity",
     ("not_yet_available", "hh-l1-conformance/1", "G-L1"), "phase-3"),
    ("phase-4/", "Phase 4 — configuration sweeps",
     ("not_yet_available", "hh-comparative-results/1", "FP-RESULTS"), "phase-4"),
    ("method/", "Method — pre-registration hub", None, "method"),
    ("spend/", "Spend ledger",
     ("unrecorded{pre_schema}", "hh-compute-cost/1", "F2-b"), "spend"),
    ("limits/", "Limits — what we don't claim", None, "limits"),
    ("changelog/", "Changelog — site versions", None, "changelog"),
    ("data/", "Data and recompute", None, "data"),
    ("about/", "About HarnessHarness", None, "about"),
    ("specimen/", "Specimen — identity fixture (internal)", None, "specimen"),
]


def _prefix(path):
    """Relative prefix from path/index.html back to the site root."""
    return "../" * path.count("/")


def _script_hash():
    import base64
    return base64.b64encode(
        hashlib.sha256(THEME_INIT.encode()).digest()).decode()


def shell(title, path, body, labels, section="", calib=None,
          noindex=False, viewer=False, connect_extra=""):
    """Base template: head (theme init + hashed CSP + stylesheet), header
    (glyph + wordmark + nav + theme toggle), label banner, main, footer
    with the calibration block. calib=None renders the mock's fields —
    R6.5's generator passes the REQ-XF-139 packet values. noindex adds
    the robots meta trial pages carry (REQ-XF-133); default False keeps
    mock output byte-identical. viewer=True loads the dormant
    transcript-viewer script; connect_extra appends the declared
    dataset origins to connect-src (the REQ-XF-132 exception — empty
    while the index is private)."""
    csp = CSP % _script_hash()
    if connect_extra.strip():
        csp = csp.replace("connect-src 'self'",
                          "connect-src 'self' " + connect_extra.strip())
    p = _prefix(path)
    nav = "".join(
        '<a href="%s%s"%s>%s</a>'
        % (p, href, ' aria-current="page"' if (section and href and
           path.startswith(href)) or (not href and not section) else "",
           name)
        for name, href in NAV)
    banner = '<div class="banner"><div class="shell">%s</div></div>' % "".join(
        labels)
    calib = calibration_block(calib if calib is not None else [
        ("experiment id", "phase-1 replay record (fixture scope)"),
        ("site version", SITE_VERSION),
        ("source commit", "mock — pinned at build"),
        ("packet-set digest", "fixture — see /data/ · file count n/a"),
        ("generator", "%s · stdlib only" % GENERATOR_ID),
        ("evidence class", "fixture-verified — internal; not outside-verified"),
        ("recompute", "python3 scripts/site/build_mock.py --out <dir>")])
    return ("<!doctype html><html lang=\"en\"><head>"
            "<meta charset=\"utf-8\">"
            "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">"
            "<title>%s · HarnessHarness results</title>"
            "<meta name=\"description\" content=\"%s\">"
            "%s"
            "<meta http-equiv=\"Content-Security-Policy\" content=\"%s\">"
            "<meta property=\"og:title\" content=\"%s\">"
            "<meta property=\"og:image\" content=\"%sassets/og.png\">"
            "<script>%s</script>"
            "<link rel=\"icon\" type=\"image/svg+xml\" href=\"%sassets/favicon.svg\">"
            "<link rel=\"icon\" type=\"image/png\" href=\"%sassets/favicon.png\">"
            "<link rel=\"stylesheet\" href=\"%sassets/tokens.css\">"
            "<link rel=\"stylesheet\" href=\"%sassets/site.css\">"
            "<script src=\"%sassets/site.js\" defer></script>"
            "%s"
            "</head><body>"
            "<header class=\"site-head\"><div class=\"shell\">"
            "<a class=\"brand\" href=\"%s\">"
            "<svg class=\"glyph\" role=\"img\" aria-label=\"HarnessHarness glyph\">"
            "<use href=\"%sassets/glyphs.svg#hh-glyph\"/></svg>"
            "<span class=\"wordmark\">Harness<span class=\"w2\">Harness</span>"
            "<span class=\"wordmark-sub\">results</span></span></a>"
            "<nav class=\"site-nav\" aria-label=\"site\">%s</nav>"
            "<button class=\"theme-toggle\" aria-pressed=\"false\" "
            "title=\"switch theme\">theme</button>"
            "</div></header>%s"
            "<main class=\"shell\">%s</main>%s"
            "<footer class=\"site-foot\"><div class=\"shell\">"
            "<a href=\"%slimits/\">what we don't claim</a>"
            "<a href=\"%sdata/\">data &amp; recompute</a>"
            "<a href=\"%sabout/\">about</a>"
            "</div></footer></body></html>"
            % (ESC(title), ESC(title),
               '<meta name="robots" content="noindex">' if noindex else "",
               csp, ESC(title),
               p, THEME_INIT, p, p, p, p, p,
               ('<script src="%sassets/viewer.js" defer></script>' % p)
               if viewer else "",
               p or "./", p, nav, banner, body, calib, p, p, p))


def fixture_label():
    return stamp("withheld", "fixture · internal record; not outside-verified")


def mock_label():
    return stamp("unresolved", "mock preview — internal profile")


def _absent_page(path, title, reason, schema, gate, section):
    body = ('<h1 class="display-2">%s</h1>' % ESC(title) +
            absent_block(reason, schema, gate))
    return shell(title, path, body, [mock_label()], section)


# ── home ───────────────────────────────────────────────────────────────
def _tile(t, prefix):
    if t.get("absent"):
        return ('<section class="card span-6 tile"><h3>%s</h3>%s</section>'
                % (ESC(t["title"]),
                   absent_block(t["reason"], t["schema"], t["gate"],
                                bars=("w-wide", "w-mid"))))
    st = stamp(t["stamp"]["kind"], t["stamp"]["text"])
    rail = receipt_rail(t["receipt"], prefix)
    return ('<section class="card span-6 tile"><h3>%s</h3>'
            '<div class="figure-row"><div class="figure-body">'
            '<span class="reading">%s</span><p class="small">%s</p>%s'
            '</div>%s</div></section>'
            % (ESC(t["title"]), ESC(t["value"]), ESC(t["detail"]), st, rail))


def home_page(fixture, all_absent=False):
    body = ['<div class="hero"><h1 class="display-1">A harness lab that '
            'keeps its receipts.</h1>'
            '<p class="prose">Same model, same tasks, every model call '
            'mediated and receipted, the analysis frozen before the first '
            'outcome — and every figure recomputed from a committed record '
            'you can open.</p></div>']
    body.append('<h2>Channels</h2><div class="cols">')
    if all_absent:
        chans = [
            ("P1 · execution-surface replay",
             stamp("absent", "Not yet available"),
             "awaits hh-surface-xcheck/1", "phase-1/"),
            ("P2 · comparative", stamp("absent", "Not yet available"),
             "awaits the FP-S gate", "phase-2/"),
            ("P3 · HIR parity", stamp("absent", "Not yet available"),
             "awaits the G-L1 gate", "phase-3/"),
            ("P4 · sweeps", stamp("absent", "Not yet available"),
             "awaits FP-RESULTS", "phase-4/")]
    else:
        chans = [
            ("P1 · execution-surface replay",
             stamp("verified", "recorded · internal"),
             "tb-core 0.1.1 · oracle · $0", "phase-1/"),
            ("P2 · comparative", stamp("absent", "Not yet available"),
             "awaits the FP-S gate", "phase-2/"),
            ("P3 · HIR parity", stamp("absent", "Not yet available"),
             "awaits the G-L1 gate", "phase-3/"),
            ("P4 · sweeps", stamp("absent", "Not yet available"),
             "awaits FP-RESULTS", "phase-4/")]
    for name, state, detail, href in chans:
        body.append('<section class="card span-3 tile"><h3>%s</h3>'
                    '<p>%s</p><p class="micro">%s</p>'
                    '<p class="small"><a href="%s">open →</a></p></section>'
                    % (ESC(name), state, ESC(detail), href))
    body.append('</div><h2>Readings</h2><div class="cols">')
    for t in fixture["tiles"]:
        body.append(_tile(t, ""))
    body.append('</div><h2>What we don\'t claim</h2>'
                '<ul class="dont-claim">%s</ul>'
                % "".join('<li class="prose">%s — <a href="limits/">limits</a></li>'
                          % ESC(d) for d in DONT_CLAIM))
    labels = [fixture_label()] if not all_absent else [mock_label()]
    return shell("Overview", "", "".join(body), labels)


# ── method hub ─────────────────────────────────────────────────────────
def method_page():
    secs = [("Pre-registration", "not_yet_available",
             "comparative_protocol/1", "FP-PREREG"),
            ("Parity status (L1 / L2)", "not_yet_available",
             "hh-l1-conformance/1", "G-L1"),
            ("Egress allowlist policy", "not_yet_available",
             "hh-compute-decl/1", "PROV-W1"),
            ("Scan coverage — Scanned / Unreadable / No trajectory / "
             "Scan error / Not scanned", "not_yet_available",
             "answer-scan/1", "FP-RESULTS"),
            ("Missingness and completion envelopes", "not_yet_available",
             "hh-comparative-results/1", "FP-RESULTS"),
            ("Discordances and repairs register", "not_yet_available",
             "hh-protocol-deviations/1", "F2-b")]
    body = ['<h1 class="display-2">Method</h1>',
            '<p class="prose">Pre-registration (protocol hash, freeze time '
            'vs first outcome time), parity status, egress policy, scan '
            'coverage, missingness, discordances and repairs — one hub, '
            'each section rendering its designed absent state until its '
            'record exists.</p><div class="cols">']
    for name, reason, schema, gate in secs:
        body.append('<section class="card span-6"><h3>%s</h3>%s</section>'
                    % (ESC(name), absent_block(reason, schema, gate,
                                               bars=("w-wide", "w-mid"))))
    body.append('</div>')
    return shell("Method", "method/", "".join(body), [mock_label()], "method")


# ── always-present static pages ────────────────────────────────────────
def limits_page():
    items = "".join('<li class="prose">%s</li>' % ESC(d) for d in DONT_CLAIM)
    bounds = "".join('<li class="prose">%s</li>' % ESC(b) for b in [
        "“internal record; not outside-verified” until a graduation record exists",
        "“descriptive, no margin registered” on every tranche-3 reading",
        "“exploratory; measures, never ranks” on pilot pages",
        "“undefined (0 solves)” wherever $/solve has no solves",
        "“not computed — R=⟨k⟩” wherever noise lacks replicates"])
    body = ('<h1 class="display-2">Limits — what we don\'t claim</h1>'
            '<h2>Never claimed</h2><ul class="dont-claim">%s</ul>'
            '<h2>Mandatory labels</h2><ul class="dont-claim">%s</ul>'
            '<h2>Refusals render, they never hide</h2>'
            '<p class="prose">A refused contrast prints its typed refusal '
            'and the all-planned bounds. Negative, inconclusive, declined '
            'and withheld outcomes carry the same structure and prominence '
            'as positive ones.</p>' % (items, bounds))
    return shell("Limits", "limits/", body, [mock_label()], "limits")


def changelog_page():
    body = ('<h1 class="display-2">Changelog</h1>'
            '<div class="card"><h3>%s — mock preview</h3>'
            '<p class="small">Internal mock build of the “Instrument — the '
            'calibrated readout” identity on phase-1 fixtures, for the '
            'GATE-G21 identity review. No publication records exist; the '
            'public changelog derives from hh-site-publication/1 records '
            'and is empty by design.</p></div>' % ESC(SITE_VERSION))
    return shell("Changelog", "changelog/", body, [mock_label()], "changelog")


def data_page(fixture):
    rows = "".join(
        '<tr><td class="name">%s</td><td class="name">%s</td>'
        '<td class="num">%s</td></tr>'
        % (ESC(t["receipt"].get("packet", "—")), ESC(t["receipt"]["schema"]),
           ESC(t["receipt"]["sha256"]))
        for t in fixture["tiles"] if not t.get("absent") and t.get("receipt"))
    body = ('<h1 class="display-2">Data and recompute</h1>'
            '<h2>Packets shown on this mock (fixture-labelled)</h2>'
            '<div class="table-wrap"><table class="data"><thead><tr>'
            '<th>packet</th><th>schema</th><th class="num">sha256</th>'
            '</tr></thead><tbody>%s</tbody></table></div>'
            '<h2>Recompute</h2><p class="prose"><code>python3 '
            'scripts/site/build_mock.py --out &lt;dir&gt;</code> — stdlib '
            'only, deterministic; two runs yield one tree sha256. '
            'hh-site-data/1 is generated by F2-b (not yet present).</p>'
            % rows)
    return shell("Data", "data/", body, [fixture_label()], "data")


def about_page():
    vocab = "".join('<li class="small">%s</li>' % ESC(v) for v in EVIDENCE_VOCAB)
    body = ('<h1 class="display-2">About HarnessHarness</h1>'
            '<p class="prose">HarnessHarness is a Rust runtime and '
            'experiment lab for building and evaluating agent harnesses. '
            'This site publishes its measured results — each figure backed '
            'by a committed packet you can recompute.</p>'
            '<h2>Evidence vocabulary</h2><ul>%s</ul>'
            '<p class="prose"><a href="https://github.com/SteveVitali/'
            'HarnessHarness">Repository ↗</a></p>' % vocab)
    return shell("About", "about/", body, [mock_label()], "about")


# ── specimen page (every REQ-XF-101 element, all eight stamps, both
#    themes side by side; fixture label; internal only) ─────────────────
def specimen_page(fixture, harnesses):
    msets = "".join(
        '<div class="swatch" data-theme="%s"><span class="micro">theme — %s</span>%s'
        '<p>%s</p><p>%s</p></div>'
        % (theme, theme, stamp_legend(),
           interval_svg(41, 52, 63, label="separated interval — needle at point"),
           interval_svg(44, 50, 58, separated=False,
                        label="unseparated interval — outline form"))
        for theme in ("light", "dark"))
    rail_demo = receipt_rail(fixture["specimen_receipt"], "")
    body = ['<h1 class="display-2">Specimen — the Instrument identity</h1>',
            '<p class="prose">One fixture page carrying every signature '
            'element of REQ-XF-101 in both themes, for GATE-G21. Fixture '
            'labelled; internal profile only.</p>',
            '<h2>Disposition stamps — all eight glyphs</h2>%s' % msets,
            '<h2>Noise-floor band</h2>',
            noise_floor_band(
                "observed-leader group · descriptive · 95% interval of "
                "difference covers 0 · leader selected from the same data",
                '<div class="card">band members sit here — fill carries no '
                'meaning; the edge rules and this label do</div>'),
            '<h2>Receipt rail</h2>',
            '<div class="figure-row"><div class="figure-body">'
            '<p class="prose">The narrow gutter carries packet short sha, '
            'schema, date and link; it folds to one line under 900 px.</p>'
            '</div>%s</div>' % rail_demo,
            '<h2>Cards, chips, table, monograms</h2>'
            '<div class="cols"><section class="card span-6">'
            '<div class="chips"><button class="chip" aria-pressed="true">joint</button>'
            '<button class="chip" aria-pressed="false">raw</button>'
            '<button class="chip" disabled title="no packet">errors=0</button></div>'
            '<div class="table-wrap"><table class="data"><thead><tr>'
            '<th>harness</th><th class="num">resolve</th><th>disposition</th>'
            '</tr></thead><tbody>%s</tbody></table></div></section></div>'
            % "".join('<tr><td class="name">%s %s</td><td class="num">%s</td>'
                      '<td>%s</td></tr>'
                      % (monogram(harnesses["harnesses"][hid]["monogram"],
                                  harnesses["harnesses"][hid]["name"]),
                         ESC(harnesses["harnesses"][hid]["name"]),
                         ESC(row[0]), stamp(row[1][0], row[1][1]))
                      for hid, row in zip(fixture["specimen_harnesses"],
                                          fixture["specimen_rows"])),
            '<h2>Graph-paper ground</h2>',
            '<p class="small">decorative — the hero above and the skeletons '
            'below carry it; data surfaces never do.</p>', skeleton()]
    return shell("Specimen", "specimen/", "".join(body), [fixture_label()])
