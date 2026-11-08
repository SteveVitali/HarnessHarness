"""components — the six signature elements and their guards.

Signature elements (REQ-XF-101): needle readouts, noise-floor band,
disposition stamps, receipt rail, calibration block, graph-paper ground.
Guards (component level — the checker legs are R6.6's, never claimed
here):
  (t) validate_chart_tokens — a status token and a series token never
      share one chart (REQ-XF-104);
  scatter/frontier marks — monogram + direct label only, a series token
      raises (REQ-XF-103);
  (p) validate_stamp — a stamp is glyph + verbatim text + colour; a
      colour-only fragment raises (REQ-XF-105);
  (r) validate_skeleton — no digit inside a skeleton (REQ-XF-114).
"""
import html
import re

ESC = lambda s: html.escape(str(s), quote=True)

GLYPH_SPRITE = "assets/glyphs.svg"

STATUS_TOKENS = frozenset(
    ["st-verified", "st-inconclusive", "st-withheld", "st-failed", "st-infra"])
SERIES_TOKENS = frozenset("series-%d" % i for i in range(1, 9))


class GuardError(ValueError):
    """A component was asked to violate a spec predicate."""


# ── disposition stamps (REQ-XF-105) ────────────────────────────────────
# kind -> (css class, sprite symbol id). Eight glyphs, verbatim text,
# colour — never colour-only, never collapsed into "N/A".
DISPOSITIONS = {
    "verified": ("verified", "g-verified"),
    "inconclusive": ("inconclusive", "g-inconclusive"),
    "withheld": ("withheld", "g-withheld"),
    "failed": ("failed", "g-failed"),
    "infra": ("infra", "g-infra"),
    "unresolved": ("unresolved", "g-unresolved"),
    "absent": ("absent", "g-absent"),
    "flagged": ("flagged", "g-flagged"),
}

_STAMP_RE = re.compile(
    r'<span class="stamp ([a-z]+)" data-stamp="[a-z]+">'
    r'<svg[^>]*><use href="([^"#]+)#(g-[a-z]+)"/></svg>'
    r'<span class="stamp-text">(.+)</span></span>')


def stamp(kind, text, sprite=GLYPH_SPRITE):
    """glyph + verbatim text + colour, always together."""
    cls, sym = DISPOSITIONS[kind]
    out = ('<span class="stamp %s" data-stamp="%s">'
           '<svg class="glyph" role="img" aria-hidden="true">'
           '<use href="%s#%s"/></svg>'
           '<span class="stamp-text">%s</span></span>'
           % (cls, kind, sprite, sym, ESC(text)))
    validate_stamp(out)
    return out


def validate_stamp(fragment):
    """Mutation (p) hook: a colour-only stamp raises GuardError."""
    m = _STAMP_RE.search(fragment)
    if not m or not m.group(4).strip():
        raise GuardError("stamp without its verbatim text is colour-only")


def stamp_legend(kinds=None):
    items = "".join(stamp(k, k + "{…}" if k in
                        ("verified", "inconclusive", "withheld", "failed")
                        else k) for k in (kinds or DISPOSITIONS))
    return '<div class="stamp-filter" role="group" aria-label="disposition legend">%s</div>' % items


# ── needle readout (interval-first; never a bar from zero) ─────────────
def interval_svg(lo, point, hi, domain=(0, 100), separated=True,
                 label="", width=220, height=20):
    """Interval rule with 6px caps + a needle point tick. An unseparated
    interval is an outline (two hairlines) — 'no claim' visible without
    colour. Values are caller-supplied (packet or fixture); this draws,
    never derives."""
    lo_d, hi_d = domain
    span = hi_d - lo_d
    if span <= 0:
        raise GuardError("interval domain is empty")
    x = lambda v: 4 + (v - lo_d) / span * (width - 8)
    y = height / 2
    parts = ['<svg class="needle-fig" width="%d" height="%d" role="img" '
             'aria-label="%s" viewBox="0 0 %d %d">'
             % (width, height, ESC(label or "interval"), width, height)]
    cls = "int-rule" if separated else "int-rule open"
    parts.append('<g class="%s">' % cls)
    if separated:
        parts.append('<line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" '
                     'stroke="var(--ink-2)" stroke-width="2"/>'
                     % (x(lo), y, x(hi), y))
    else:
        parts.append('<line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" '
                     'stroke="var(--ink-2)" stroke-width="1"/>'
                     '<line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" '
                     'stroke="var(--ink-2)" stroke-width="1"/>'
                     % (x(lo), y - 3, x(hi), y - 3, x(lo), y + 3, x(hi), y + 3))
    for cx in (x(lo), x(hi)):
        parts.append('<line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" '
                     'stroke="var(--ink-2)" stroke-width="2"/>'
                     % (cx, y - 3, cx, y + 3))
    parts.append('<line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" '
                 'stroke="var(--needle)" stroke-width="2" class="glyph-needle"/>'
                 % (x(point), y - 6, x(point), y + 6))
    parts.append('</g><title>%s</title></svg>' % ESC(label or "interval"))
    return "".join(parts)


def noise_floor_band(label, body=""):
    """The tinted band that shows what chance alone produces; meaning is
    carried by --floor-edge rules + a text label, never by fill alone."""
    return ('<div class="floor-band"><span class="micro">%s</span>%s</div>'
            % (ESC(label), body))


# ── chart guards (REQ-XF-103/104) ──────────────────────────────────────
def validate_chart_tokens(used_tokens):
    """One chart never mixes a status token and a series token
    (mutation (t) at component level)."""
    used = set(used_tokens)
    clash = (used & STATUS_TOKENS) and (used & SERIES_TOKENS)
    if clash:
        raise GuardError("status and series tokens in one chart: %s"
                         % sorted(used & (STATUS_TOKENS | SERIES_TOKENS)))
    return True


def scatter_mark(monogram, label, **kw):
    """Scatter/frontier identity is typographic: monogram + direct label.
    A series token raises — colour-coding harnesses fails ΔE (F2)."""
    for bad in ("series", "series_token", "color", "colour"):
        if bad in kw:
            raise GuardError("scatter marks are monogram + direct label; "
                             "series colour %r refused" % bad)
    if not monogram or not label:
        raise GuardError("scatter mark needs monogram + direct label")
    return {"monogram": monogram, "label": label}


# ── monogram chip (REQ-XF-107) ─────────────────────────────────────────
def monogram(code, name=""):
    return ('<span class="monogram" title="%s" aria-label="%s">%s</span>'
            % (ESC(name), ESC(name), ESC(code)))


# ── receipt rail (packet short sha · schema · date · link) ─────────────
def receipt_rail(receipt, prefix=""):
    return ('<aside class="receipt-rail">pkt <b>%s</b><br>%s<br>%s<br>'
            '<a href="%s">packet ↗</a></aside>'
            % (ESC(receipt.get("sha256", "—")), ESC(receipt.get("schema", "—")),
               ESC(receipt.get("date", "—")),
               ESC(prefix + receipt.get("href", "#"))))


# ── absent state + numberless skeleton (REQ-XF-114) ────────────────────
_SKEL_OPEN = re.compile(r'<div class="absent-skeleton"[^>]*>')
_DIGIT_RE = re.compile(r"\d")


def _skeleton_bodies(fragment):
    """Yield the inner markup of every .absent-skeleton block, matched by
    tag balancing so nested content cannot hide a digit."""
    pos = 0
    while True:
        m = _SKEL_OPEN.search(fragment, pos)
        if not m:
            return
        depth, i = 1, m.end()
        while depth and i < len(fragment):
            if fragment.startswith("<div", i):
                depth += 1
                i = fragment.index(">", i) + 1
            elif fragment.startswith("</div>", i):
                depth -= 1
                i += 6
            else:
                i += 1
        yield fragment[m.end():i - 6]
        pos = i


def skeleton(bars=("w-full", "w-wide", "w-mid", "w-short", "row")):
    """Dashed placeholder of what a page will show. Numberless: neither
    the bars nor their class names carry a digit (mutation (r))."""
    body = "".join('<div class="bar %s"></div>' % b for b in bars)
    out = ('<div class="absent-skeleton" role="img" '
           'aria-label="layout placeholder — no data">%s</div>' % body)
    validate_skeleton(out)
    return out


def validate_skeleton(fragment):
    """No digit inside a skeleton — the honest 'no data' shape."""
    for body in _skeleton_bodies(fragment):
        if _DIGIT_RE.search(body):
            raise GuardError("a skeleton carries a digit")
    return True


def absent_block(reason, schema, gate, bars=None, sprite=GLYPH_SPRITE):
    """The designed absent state: typed reason, awaited schema, gate id,
    numberless skeleton (design/39 F5). sprite is the depth-aware glyph
    sprite path (nested pages pass their ../ prefix — the links leg
    verifies the use href resolves)."""
    body = ('<div class="absent">%s'
            '<div class="absent-meta">'
            '<span class="kv">reason<b>%s</b></span>'
            '<span class="kv">awaits schema<b>%s</b></span>'
            '<span class="kv">gate<b>%s</b></span>'
            '</div>%s</div>'
            % (stamp("absent", "Not yet available", sprite)
               if reason == "not_yet_available"
               else stamp("withheld", reason, sprite),
               ESC(reason), ESC(schema), ESC(gate), skeleton(bars or ())))
    return body


# ── calibration block (REQ-XF-139 fields; R6.5 fills packet values) ────
def calibration_block(fields):
    """Page-footer title block: experiment id, site version, source
    commit, packet-set digest + file count, generator version, evidence
    class, recompute command."""
    rows = "".join('<div><dt>%s</dt><dd>%s</dd></div>'
                   % (ESC(k), ESC(v)) for k, v in fields)
    return ('<footer class="calib"><div class="shell">'
            '<span class="calib-title">Calibration block</span>'
            '<dl>%s</dl></div></footer>' % rows)


# ── provenance attributes (REQ-XF-121) ─────────────────────────────────
def prov_attrs(src):
    """Every rendered value carries data-src (path#pointer) + data-sha."""
    if not src:
        return ""
    return (' data-src="%s#%s" data-sha="%s"'
            % (ESC(src["path"]), ESC(src["ptr"]), ESC(src["sha256"])))


def data_value(obj, fmt=None):
    """Render a value object's display text with provenance attrs:
    <span class="num" data-src=… data-sha=…>text</span>. fmt formats the
    v member (a D-2 call); absent it renders the integer verbatim."""
    if "text" in obj:
        return ('<span%s>%s</span>' % (prov_attrs(obj.get("src")),
                                       ESC(obj["text"])))
    v = obj.get("v")
    disp = fmt(v) if (fmt and v is not None) else (str(v) if v is not None
                                                 else "—")
    return ('<span class="num"%s>%s</span>'
            % (prov_attrs(obj.get("src")), ESC(disp)))


# ── figure anatomy (REQ-XF-109) ────────────────────────────────────────
def figure(fid, title, subtitle, chart, receipt, howto, table):
    """The REQ-XF-109 envelope: title, definition/direction subtitle,
    chart, receipt rail, 'How to read this' (estimand, N × k, interval
    method + level, population, exclusions, source) and an always-present
    data table — pre-rendered, readable without JavaScript."""
    rows = "".join('<div><dt>%s</dt><dd>%s</dd></div>'
                   % (ESC(k), ESC(v)) for k, v in howto)
    return ('<figure class="fig" id="%s"><h3>%s</h3>'
            '<p class="subtitle">%s</p>'
            '<div class="figure-row"><div class="figure-body">%s'
            '<details class="howto"><summary>How to read this</summary>'
            '<dl>%s</dl></details>'
            '<div class="table-wrap">%s</div>'
            '</div>%s</div></figure>'
            % (ESC(fid), ESC(title), ESC(subtitle), chart, rows, table,
               receipt_rail(receipt)))


# ── labelled log-scale cost axis (REQ-XF-108) ──────────────────────────
def cost_axis_svg(marks, lo_usd=0.01, hi_usd=100.0, label="cost — log scale",
                  width=420, height=44):
    """Cost values on a labelled log10 axis: tick labels at powers of ten,
    marks as needle ticks. $0.00 cannot plot on a log axis — a caller
    renders it as a below-floor note, never a fake position. marks is a
    list of (usd_float, text_label); no bars from zero."""
    import math
    if lo_usd <= 0 or hi_usd <= lo_usd:
        raise GuardError("log cost axis needs 0 < lo < hi")
    lo_l, hi_l = math.log10(lo_usd), math.log10(hi_usd)
    span = hi_l - lo_l
    x = lambda v: 8 + (math.log10(v) - lo_l) / span * (width - 16)
    y = height - 18
    parts = ['<svg class="needle-fig cost-axis" width="%d" height="%d" '
             'role="img" aria-label="%s" viewBox="0 0 %d %d">'
             % (width, height, ESC(label), width, height)]
    parts.append('<line x1="8" y1="%.1f" x2="%d" y2="%.1f" '
                 'stroke="var(--ink-2)" stroke-width="1"/>' % (y, width - 8, y))
    e0, e1 = int(math.floor(lo_l)), int(math.ceil(hi_l))
    for e in range(e0, e1 + 1):
        usd = 10.0 ** e
        if usd < lo_usd or usd > hi_usd:
            continue
        parts.append('<line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" '
                     'stroke="var(--ink-2)" stroke-width="1"/>'
                     % (x(usd), y - 3, x(usd), y + 3))
        parts.append('<text x="%.1f" y="%d" text-anchor="middle" '
                     'class="axis-label">$%s</text>'
                     % (x(usd), height - 4, ("%.2f" % usd).rstrip("0").rstrip(".")
                        if usd < 1 else "%d" % int(usd)))
    for usd, _t in marks:
        if usd is None or usd <= 0:
            continue
        usd = min(max(usd, lo_usd), hi_usd)
        parts.append('<line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" '
                     'stroke="var(--needle)" stroke-width="2" '
                     'class="glyph-needle"/>'
                     % (x(usd), y - 7, x(usd), y + 7))
    parts.append('<title>%s</title></svg>' % ESC(label))
    return "".join(parts)


PARETO_CAPTION = ("Pareto frontier among complete entries only · "
                  "descriptive")


# ── accuracy-vs-cost frontier (REQ-XF-103/108) ─────────────────────────
def frontier_svg(entries, lo_usd=0.01, hi_usd=10.0, width=560,
                 height=320, label="accuracy vs cost — frontier"):
    """The frontier chart: fixed 0–100% rate domain on y, a labelled
    log10 cost axis on x. Entries are dicts {id, monogram, name,
    theta_ppm, cost_micros, pareto} — the caller (site-data) supplies
    every value incl. D-5 Pareto membership; this draws, never derives.
    Identity is typographic (REQ-XF-103): monogram + direct label; a
    series token raises via scatter_mark. The label placer is a pure
    function of the cost-ordered entry list — two renders place labels
    identically (right of the mark, left past the 72% column, vertical
    stagger by stable order parity)."""
    import math
    from . import derive
    if lo_usd <= 0 or hi_usd <= lo_usd:
        raise GuardError("log cost axis needs 0 < lo < hi")
    ml, mr, mt, mb = 48, 14, 14, 38
    pw, ph = width - ml - mr, height - mt - mb
    lo_l, hi_l = math.log10(lo_usd), math.log10(hi_usd)
    xspan = hi_l - lo_l
    fx = lambda usd: ml + (math.log10(usd) - lo_l) / xspan * pw
    fy = lambda pct: mt + (100.0 - pct) / 100.0 * ph
    pct_of = derive.DERIVATIONS["D-1"]["ppm_to_percent"]
    parts = ['<svg class="needle-fig frontier" width="%d" height="%d" '
             'role="img" aria-label="%s" viewBox="0 0 %d %d">'
             % (width, height, ESC(label), width, height)]
    parts.append('<rect x="%d" y="%d" width="%d" height="%d" fill="none"'
                 ' stroke="var(--ink-3)" stroke-width="1"/>'
                 % (ml, mt, pw, ph))
    for pct in (0, 25, 50, 75, 100):
        parts.append('<line x1="%d" y1="%.1f" x2="%d" y2="%.1f" '
                     'stroke="var(--ink-3)" stroke-width="0.5"/>'
                     % (ml, fy(pct), ml + pw, fy(pct)))
        parts.append('<text x="%d" y="%.1f" text-anchor="end" '
                     'class="axis-label">%d%%</text>'
                     % (ml - 6, fy(pct) + 3, pct))
    e0, e1 = int(math.floor(lo_l)), int(math.ceil(hi_l))
    for e in range(e0, e1 + 1):
        usd = 10.0 ** e
        if usd < lo_usd or usd > hi_usd:
            continue
        parts.append('<line x1="%.1f" y1="%d" x2="%.1f" y2="%d" '
                     'stroke="var(--ink-3)" stroke-width="0.5"/>'
                     % (fx(usd), mt, fx(usd), mt + ph))
        lab = ("$%.2f" % usd).rstrip("0").rstrip(".") if usd < 1 \
            else "$%d" % int(usd)
        parts.append('<text x="%.1f" y="%d" text-anchor="middle" '
                     'class="axis-label">%s</text>'
                     % (fx(usd), height - 8, lab))
    pts = []
    for n, en in enumerate(entries):
        scatter_mark(en["monogram"], en["name"])
        usd = en["cost_micros"] / 1000000.0
        if usd <= 0:
            continue  # $0 cannot plot on a log axis — caller notes it
        pts.append((fx(min(max(usd, lo_usd), hi_usd)),
                    fy(pct_of(en["theta_ppm"])), en, n))
    ordered = sorted(pts, key=lambda p: (p[0], -p[1], p[2]["id"]))
    pareto_pts = [p for p in ordered if p[2]["pareto"]]
    if pareto_pts:
        pl = ' '.join('%.1f,%.1f' % (x, y) for x, y, _e, _n in pareto_pts)
        parts.append('<polyline points="%s" fill="none" '
                     'stroke="var(--needle)" stroke-width="1" '
                     'class="pareto-line"/>' % pl)
    for lx_i, (x, y, en, _n) in enumerate(ordered):
        side = -1 if x > ml + pw * 0.72 else 1
        parts.append('<g class="frontier-mark%s">'
                     % (" pareto" if en["pareto"] else ""))
        parts.append('<rect x="%.1f" y="%.1f" width="22" height="16" '
                     'rx="3" fill="var(--panel)" '
                     'stroke="var(--ink-2)" stroke-width="1"/>'
                     % (x - 11, y - 8))
        parts.append('<text x="%.1f" y="%.1f" text-anchor="middle" '
                     'class="mono-chip">%s</text>'
                     % (x, y + 3.5, ESC(en["monogram"])))
        lx = x + 16 * side
        anchor = "start" if side > 0 else "end"
        ly = y + 4 + (12 if lx_i % 2 else -8)
        parts.append('<text x="%.1f" y="%.1f" text-anchor="%s" '
                     'class="frontier-label">%s</text>'
                     % (lx, ly, anchor, ESC(en["name"])))
        parts.append('</g>')
    parts.append('<title>%s</title></svg>' % ESC(label))
    return "".join(parts)
