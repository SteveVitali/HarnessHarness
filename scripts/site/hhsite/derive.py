"""derive — the closed derivation registry D-1…D-8 (REQ-XF-118).

The generator implements no statistic. The only transforms a packet value
may take on the way to a page are the eight registered derivations below,
each unit-tested. A view whose estimator is absent from its packet shows
"not computed (<reason>)" — never a gap, never a fabricated number.

The code audit (``audit_source``) scans the generator modules' AST for
arithmetic or aggregating calls outside the registry (AC-9.3-2): every
numeric transform must be a registered derivation; layout/markup math
lives only in the recorded allowlist (drawing coordinates, PNG packing,
colour math).
"""
import ast
import math
from pathlib import Path


# ── D-1 unit conversion ───────────────────────────────────────────────
def _usd_str(micros):
    """D-1: integer micro-USD → "$i.ff" display (whole cents — the
    packets' cost precision is integer micro-units, so $0.00 is exact)."""
    sign = "-" if micros < 0 else ""
    m = abs(micros)
    return "%s$%d.%02d" % (sign, m // 1000000, (m % 1000000) // 10000)


def _ppm_to_percent(ppm):
    """D-1: integer ppm → percent float for plotting (display precision
    is governed by D-2; the chart itself never rounds statistics)."""
    return ppm / 10000.0


def _to_micros(dollars):
    """D-1: a packet dollar field → integer micro-USD. A non-integral
    micro value refuses (never silently rounded; the tolerance covers
    float representation error only)."""
    micros = round(dollars * 1000000)
    if abs(dollars * 1000000 - micros) > 1e-9:
        raise ValueError("not integral in micro-USD")
    return micros


# ── D-2 rounding by the precision rule (REQ-XF-021) ───────────────────
def _decimals(half_width_units, packet_digits):
    """D-2 helper: fewest coarser display — the last displayed digit is
    never finer than about one-tenth of the interval half-width, nor
    more digits than the packet holds. half_width None/<=0 → packet
    precision."""
    if packet_digits < 0:
        packet_digits = 0
    if not half_width_units or half_width_units <= 0:
        return packet_digits
    d = math.floor(math.log10(10.0 / half_width_units))
    return min(packet_digits, max(0, d))


def _fmt(v, half_width, unit, packet_digits=None):
    """D-2: format an integer-unit value at the precision rule.
    unit 'ppm' → percent display (cap 4 places — 1 ppm = 10⁻⁴ %);
    'micros_usd' → USD display (cap 6 — 1 µ$ = 10⁻⁶ $); any other unit
    displays the integer at the packet's own precision (cap 0)."""
    if unit == "ppm":
        cap = 4 if packet_digits is None else min(packet_digits, 4)
        d = _decimals((half_width or 0) / 10000.0, cap)
        return "%.*f%%" % (d, v / 10000.0)
    if unit == "micros_usd":
        cap = 2 if packet_digits is None else min(packet_digits, 6)
        d = _decimals(half_width or 0, cap)
        m = abs(v)
        frac = ("%06d" % (m % 1000000))[:d]
        return "%s$%d%s" % ("-" if v < 0 else "", m // 1000000,
                           ("." + frac) if frac else "")
    return str(v)


# ── D-3 ordering — never rank ─────────────────────────────────────────
def _order(rows, key, reverse=False):
    """D-3: ordered copies by a displayed value or by name; ordering never
    implies rank (no ordinal is emitted anywhere)."""
    return sorted(rows, key=lambda r: r[key], reverse=reverse)


# ── D-4 disposition counts within one packet ──────────────────────────
def _disposition_counts(records, key):
    """D-4: count records by their disposition field — a fold over one
    packet's rows, never an estimator."""
    counts = {}
    for r in records:
        k = r.get(key, "missing") if isinstance(r, dict) else "missing"
        counts[k] = counts.get(k, 0) + 1
    return counts


# ── D-5 Pareto membership over complete entries (descriptive) ─────────
def _pareto_members(entries, cost_key, value_key):
    """D-5: ids of Pareto-optimal entries among COMPLETE entries only
    (both fields integers); cost minimized, value maximized. Labelled
    descriptive by the caller; incomplete entries never join."""
    complete = [e for e in entries
                if isinstance(e.get(cost_key), int)
                and isinstance(e.get(value_key), int)]
    members = []
    for e in complete:
        dominated = any(
            (o[cost_key] <= e[cost_key] and o[value_key] >= e[value_key]
             and (o[cost_key] < e[cost_key] or o[value_key] > e[value_key]))
            for o in complete if o is not e)
        if not dominated:
            members.append(e.get("id"))
    return members


# ── D-6 ratio of two displayed point values (descriptive, no interval) ─
def _ratio(num, den):
    """D-6: ratio of two displayed point values at fixed 2-decimal
    display, labelled descriptive, carrying no interval. den 0 → the
    typed 'undefined' string."""
    if den == 0:
        return "undefined"
    return "%.2f" % (num / den)


# ── D-7 set membership read from packet flags ─────────────────────────
def _member(flags, name):
    """D-7: membership of name in a packet flag list — a read, never a
    computation."""
    return name in (flags or [])


# ── D-8 duration and timestamp formatting ─────────────────────────────
def _fmt_time(v, kind="duration_ms"):
    """D-8: 'duration_ms' → 'h:mm:ss'/'m:ss' text; 'verbatim' → a packet
    date/timestamp string displayed unchanged (never re-timed)."""
    if kind == "verbatim":
        return str(v)
    s = int(v) // 1000
    h, rem = s // 3600, s % 3600
    return ("%d:%02d:%02d" % (h, rem // 60, rem % 60)) if h \
        else ("%d:%02d" % (rem // 60, rem % 60))


def _fmt_interval(v, lo, hi, unit, packet_digits=None):
    """D-2 convenience: format v at the precision rule with the
    interval's own half-width (never finer than ~⅒ of it)."""
    return _fmt(v, (hi - lo) / 2, unit, packet_digits)


DERIVATIONS = {
    "D-1": {"ppm_to_percent": _ppm_to_percent, "to_micros": _to_micros,
            "usd_str": _usd_str},
    "D-2": {"fmt": _fmt, "fmt_interval": _fmt_interval},
    "D-3": {"order": _order},
    "D-4": {"counts": _disposition_counts},
    "D-5": {"pareto": _pareto_members},
    "D-6": {"ratio": _ratio},
    "D-7": {"member": _member},
    "D-8": {"fmt_time": _fmt_time},
}
DERIVATION_FUNCS = frozenset(
    f.__name__ for d in DERIVATIONS.values() for f in d.values())
EXPECTED_IDS = frozenset("D-%d" % i for i in range(1, 9))

# ── the code audit (AC-9.3-2) ──────────────────────────────────────────
# Modules whose arithmetic is layout/markup only — landed at R6.4 and
# pinned by the golden all-absent manifest. They never touch packet data.
LAYOUT_MODULES = frozenset({"raster", "tokparse", "contrast", "pages"})
# Functions in scanned modules allowed coordinate/geometry math.
LAYOUT_FUNCS = frozenset({
    ("components", "interval_svg"), ("components", "cost_axis_svg"),
    ("components", "_skeleton_bodies"), ("components", "receipt_rail"),
    # R6.20: the frontier's log-scale coordinates and deterministic
    # label placement are drawing math, recorded here — not derivations.
    ("components", "frontier_svg"),
})
# The auditor's own public API — scanned differently (it formats violation
# strings; it never transforms packet values). Not a derivation.
AUDIT_API = frozenset({"audit_source"})
_AGG_FUNCS = frozenset({"sum", "mean", "median", "stdev", "variance",
                        "pstdev", "pvariance", "fmean", "geometric_mean"})
_STR_CTOR = frozenset({"str", "repr", "chr"})


def _is_strish(node):
    return (isinstance(node, ast.Constant) and isinstance(node.value, str)) \
        or isinstance(node, ast.JoinedStr) \
        or (isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name)
            and node.func.id in _STR_CTOR)


def _numlit(node):
    return isinstance(node, ast.Constant) \
        and isinstance(node.value, (int, float)) \
        and not isinstance(node.value, bool)


def _arith_risky(node):
    """A BinOp is derivation-risk unless it is plainly string or path
    work: Add/Mult/Mod beside a string literal, joined-str or
    str()/repr()/chr() is formatting; ``path / name`` joins are Path
    operations, never statistics. Sub and Pow are numeric-only. A bare
    ``a / b`` of two identifiers is indistinguishable from a path join
    statically and is the audit's documented bound — aggregates (sum,
    mean…) and constant arithmetic are still caught."""
    op = node.op
    if _is_strish(node.left) or _is_strish(node.right):
        return False
    if isinstance(op, (ast.Sub, ast.Pow, ast.Add, ast.Mult)):
        return True
    if isinstance(op, (ast.Div, ast.FloorDiv, ast.Mod)):
        return _numlit(node.left) or _numlit(node.right)
    return False


def audit_source(paths):
    """Scan generator modules for arithmetic outside D-1…D-8. Returns a
    list of violation strings; empty is clean.
    Scanned: every module not in LAYOUT_MODULES and not a test module;
    inside a scanned module, functions in LAYOUT_FUNCS are skipped
    (recorded layout math) and registered derivation bodies are allowed
    (in derive.py every public def must be registered or the auditor)."""
    violations = []
    for path in sorted(paths):
        mod = Path(path).stem
        if mod in LAYOUT_MODULES or mod.startswith("test_"):
            continue
        tree = ast.parse(Path(path).read_text())

        def visit(node, funcname):
            allowed = (mod, funcname) in LAYOUT_FUNCS \
                or funcname in DERIVATION_FUNCS \
                or (mod == "derive" and funcname.startswith("_"))
            for child in ast.iter_child_nodes(node):
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    if mod == "derive" \
                            and child.name not in DERIVATION_FUNCS \
                            and child.name not in AUDIT_API \
                            and not child.name.startswith("_"):
                        violations.append(
                            "%s: public def %s is neither a registered "
                            "derivation nor the audit API"
                            % (mod, child.name))
                    if mod == "derive" and child.name in AUDIT_API:
                        continue  # the auditor scans, never transforms
                    visit(child, child.name)
                elif isinstance(child, (ast.BinOp, ast.AugAssign)):
                    risky = isinstance(child, ast.AugAssign) \
                        or _arith_risky(child)
                    if risky and not allowed:
                        violations.append(
                            "%s:%s %s — arithmetic outside D-1…D-8"
                            % (mod, funcname or "<module>", child.lineno))
                    visit(child, funcname)
                elif isinstance(child, ast.Call):
                    f = child.func
                    name = f.id if isinstance(f, ast.Name) \
                        else (f.attr if isinstance(f, ast.Attribute) else "")
                    if name in _AGG_FUNCS and not allowed:
                        violations.append(
                            "%s:%s %s — aggregate %s() outside D-1…D-8"
                            % (mod, funcname or "<module>", child.lineno,
                               name))
                    visit(child, funcname)
                else:
                    visit(child, funcname)

        visit(tree, "")
    return violations
