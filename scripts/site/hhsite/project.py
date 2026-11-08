"""project — fold the named phase-1 packets into hh-site-data/1 bodies.

Every value carries ``src{path, sha256, ptr}`` pointing at the exact
packet member it copied; every copied string is hygiene-checked; every
numeric leaf is an integer (counts, micro-USD, ppm). The only transforms
are the registered derivations D-1…D-8 — this module contains no
arithmetic (the code audit scans it).

Files emitted (design/39 F8; REQ-XF-120):
  manifest.json  — the packet set, its digest, per-phase state;
  phase1.json    — bounds verbatim, tranche counts, cross-check,
                   estimator outcomes, dispositions, costs;
  spend.json     — inference vs compute, per-packet cost rows;
  method.json    — the §9.2 hub sections with values or typed absents;
  claims.json    — labels, bounds and refusals for /limits/;
  changelog.json — publication-record-derived entries (none exist).
"""
from . import derive
from .packets import field
from .values import Refusal, absent, derived, text, value

VERIFIED_LABEL = "descriptive, no margin registered"
COMPUTE_ABSENT = {"awaits": "hh-compute-cost/1",
                  "reason": "unrecorded{pre_schema}",
                  "state": "not_yet_available"}


def _v(inp, ptr, unit, **kw):
    """Copy an integer field into a value object."""
    raw, src = field(inp, ptr)
    if not isinstance(raw, int) or isinstance(raw, bool):
        raise Refusal("value %s#%s is not an integer field" % (inp.path, ptr))
    return value(raw, unit, src, **kw)


def _t(inp, ptr, **kw):
    """Copy a string field into a text object."""
    raw, src = field(inp, ptr)
    if not isinstance(raw, str):
        raise Refusal("text %s#%s is not a string field" % (inp.path, ptr))
    return text(raw, src, **kw)


def _flag(inp, ptr):
    raw, src = field(inp, ptr)
    if not isinstance(raw, bool):
        raise Refusal("flag %s#%s is not a boolean field" % (inp.path, ptr))
    return value(raw, "flag", src)


def _micros(inp, ptr):
    """A packet dollar field → integer micro-USD (D-1 conversion lives
    in the registry; a non-integral micro value refuses, never rounds)."""
    raw, src = field(inp, ptr)
    try:
        micros = derive.DERIVATIONS["D-1"]["to_micros"](raw)
    except ValueError:
        raise Refusal("cost %s#%s is not integral in micro-USD"
                      % (inp.path, ptr))
    return value(micros, "micros_usd", src)


def _maybe(inp, ptr, builder):
    """A view whose estimator/field is absent from its packet is a typed
    absent — 'not computed (<reason>)', never a gap (REQ-XF-118)."""
    try:
        return builder(inp, ptr)
    except Refusal:
        return absent("not_computed",
                      reason="estimator absent from packet: %s" % ptr)


def _counts_tree(inp, ptr):
    """Walk a packet counts object into the same shape with value-object
    leaves carrying per-member pointers."""
    raw, src = field(inp, ptr)
    if isinstance(raw, dict):
        return {k: _counts_tree(inp, "%s/%s" % (ptr, k)) for k in raw}
    if isinstance(raw, list):
        return [_counts_tree(inp, "%s/%d" % (ptr, n))
                for n in range(len(raw))]
    if isinstance(raw, int) and not isinstance(raw, bool):
        return value(raw, "count", src)
    return text(str(raw), src)


def _by_id(inputs):
    return {i.id: i for i in inputs}


def bounds_list(record):
    """The thirteen honest bounds verbatim, each with its /bounds/<i>
    pointer into the record (REQ-XF-141: copied, never re-authored)."""
    out = []
    for i, b in enumerate(record.doc["bounds"]):
        src = {"path": record.path, "ptr": "/bounds/%d" % i,
               "sha256": record.sha256}
        out.append({"i": i, "text": text(b, src)})
    return out


# ── manifest.json ──────────────────────────────────────────────────────
def manifest(inputs, inputs_digest):
    return {
        "inputs_digest": inputs_digest,
        "packet_count": len(inputs),
        "packet_set": [{"id": i.id, "path": i.path, "phase": i.phase,
                        "schema": i.schema, "sha256": i.sha256}
                       for i in inputs],
        "phases": {
            "phase-1": "internal record; not outside-verified",
            "phase-2": "fixture-rendered — internal profile only; "
                       "publication pending",
            "phase-3": "not_yet_available{publication pending}",
            "phase-4": "not_yet_available{publication pending}",
        },
        "site_files": ["changelog.json", "claims.json", "manifest.json",
                       "method.json", "phase1.json", "spend.json",
                       "tfp.json"],
    }


# ── phase1.json ────────────────────────────────────────────────────────
def _basis(xcheck, name):
    b = {}
    for k in ("tasks", "agree", "discordant", "records",
              "discordant_records"):
        got = _maybe(xcheck, "/bases/%s/%s" % (name, k),
                     lambda i, p: _v(i, p, "count"))
        if isinstance(got, dict) and got.get("state"):
            continue
        b[k] = got
    rule = _maybe(xcheck, "/bases/%s/rule" % name, _t)
    if not (isinstance(rule, dict) and rule.get("state")):
        b["rule"] = rule
    tasks = _maybe(xcheck, "/bases/%s/discordant_tasks" % name, _t)
    if not (isinstance(tasks, dict) and tasks.get("state")):
        raw, src = field(xcheck, "/bases/%s/discordant_tasks" % name)
        b["discordant_tasks"] = text(", ".join(raw), src)
    per_task = _counts_tree(xcheck, "/bases/%s/per_task" % name) \
        if _present(xcheck, "/bases/%s/per_task" % name) else None
    if per_task is not None:
        b["per_task"] = per_task
    return b


def _present(inp, ptr):
    try:
        inp.resolve(ptr)
        return True
    except Refusal:
        return False


def phase1(inputs):
    i = _by_id(inputs)
    parity, xcheck, est = i["parity-t3"], i["xcheck"], i["estimators"]
    repairs, study, dev = i["s4-repairs"], i["study-r559"], i["deviations"]
    record = i["record"]

    def t3count(name):
        return _v(parity, "/counts/%s" % name, "count")

    confounds = []
    for n, c in enumerate(xcheck.resolve("/confounds")):
        confounds.append({
            "disposition": _t(xcheck, "/confounds/%d/disposition" % n),
            "kind": _t(xcheck, "/confounds/%d/kind" % n),
            "task_id": _t(xcheck, "/confounds/%d/task_id" % n),
        })

    admissions = _maybe(est, "/parity_admission", lambda i_, p: [
        {"display": _t(i_, "%s/%d/display" % (p, n)),
         "rung": _t(i_, "%s/%d/rung" % (p, n)),
         "variant": _t(i_, "%s/%d/variant" % (p, n))}
        for n, a in enumerate(i_.resolve(p))])

    kl = _maybe(est, "/kl_equivalence", lambda i_, p: {
        "margins_source": _t(i_, "%s/margins_source" % p),
        "outcome": {
            "display": _t(i_, "%s/outcome/display" % p),
            "rung": _t(i_, "%s/outcome/rung" % p),
            "variant": _t(i_, "%s/outcome/variant" % p),
        },
    })

    ladders = _maybe(est, "/descriptive_bounds", lambda i_, p: [
        {"bounds": [_bound_row(i_, "%s/%d/bounds/%d" % (p, n, m))
                    for m in range(len(e["bounds"]))],
         "estimator": _t(i_, "%s/%d/estimator" % (p, n)),
         "inference": _t(i_, "%s/%d/inference" % (p, n)),
         "label": _t(i_, "%s/%d/label" % (p, n)),
         "n": _v(i_, "%s/%d/n" % (p, n), "count")}
        for n, e in enumerate(i_.resolve(p))])

    discordances = []
    for n, d in enumerate(xcheck.resolve("/discordances")):
        discordances.append({
            "o_build_stage": derived(
                "D-7",
                [{"path": xcheck.path, "ptr": "/o_build_stage",
                  "sha256": xcheck.sha256}],
                derive.DERIVATIONS["D-7"]["member"](
                    xcheck.resolve("/o_build_stage"), d["task_id"])),
            "task_id": _t(xcheck, "/discordances/%d/task_id" % n),
        })

    departures = []
    for n, d in enumerate(dev.resolve("/departures")):
        departures.append({
            "counts": _counts_tree(dev, "/departures/%d/counts" % n),
            "detail": _t(dev, "/departures/%d/detail" % n),
            "id": _t(dev, "/departures/%d/id" % n),
        })

    repairs_reg = []
    for n, r in enumerate(dev.resolve("/repairs")):
        repairs_reg.append({
            "affected_slots": [
                _t(dev, "/repairs/%d/affected_slots/%d" % (n, m))
                for m in range(len(r["affected_slots"]))],
            "class": _t(dev, "/repairs/%d/class" % n),
            "code": {"commit": _t(dev, "/repairs/%d/code/commit" % n),
                     "path": _t(dev, "/repairs/%d/code/path" % n)},
            "repair": _t(dev, "/repairs/%d/repair" % n),
        })

    disposition_counts = derived(
        "D-4", [{"path": repairs.path, "ptr": "/slots",
                 "sha256": repairs.sha256}],
        derive.DERIVATIONS["D-4"]["counts"](
            repairs.resolve("/slots"), "disposition"))

    wall_raw, wall_src = field(repairs, "/costs/wall_ms")
    composite = _counts_tree(dev, "/composite")

    return {
        "bounds": bounds_list(record),
        "composite": composite,
        "confounds": confounds,
        "costs": {
            "inference_usd": _micros(parity, "/cost_usd"),
            "repairs_exposure_micros": _v(repairs, "/costs/exposure_micros",
                                         "micros_usd"),
            "repairs_usd": _micros(repairs, "/costs/usd"),
            "repairs_wall": {
                "formatted": derive.DERIVATIONS["D-8"]["fmt_time"](wall_raw),
                "src": wall_src, "v": wall_raw},
        },
        "crosscheck": {
            "basis_A": _basis(xcheck, "A"),
            "basis_B": _basis(xcheck, "B"),
            "basis_C": _basis(xcheck, "C"),
            "o_build_stage": [text(t, {"path": xcheck.path,
                                      "ptr": "/o_build_stage/%d" % n,
                                      "sha256": xcheck.sha256})
                              for n, t in enumerate(
                                  xcheck.resolve("/o_build_stage"))],
        },
        "departures": departures,
        "discordances": discordances,
        "dispositions": {
            "repairs_epoch": disposition_counts,
            "study": {
                "disposition": _t(study, "/disposition"),
                "s1_unmet": _flag(study, "/s1_unmet"),
                "s3_unmet": _flag(study, "/s3_unmet"),
                "s4_unmet": _flag(study, "/s4_unmet"),
                "study": _t(study, "/study"),
            },
        },
        "estimator_outcomes": {
            "descriptive_bounds": ladders,
            "kl_equivalence": kl,
            "parity_admission": admissions,
        },
        "record": {
            "path": record.path,
            "sha256": record.sha256,
            "title": "Execution-surface replay record — phase 1 "
                     "(TB-core 0.1.1, $0 oracle)",
        },
        "repairs_register": repairs_reg,
        "tranche3": {
            "counts": {
                "agree": t3count("agree"),
                "discordant": t3count("discordant"),
                "h_fallback_builds": t3count("h_fallback_builds"),
                "h_image_reuse": t3count("h_image_reuse"),
                "h_resolved": t3count("h_resolved"),
                "h_unresolved": t3count("h_unresolved"),
                "n_pairs": t3count("n_pairs"),
                "o_resolved": t3count("o_resolved"),
                "o_unresolved": t3count("o_unresolved"),
            },
            "endpoint": _t(parity, "/endpoint"),
            "epoch": _t(parity, "/epoch"),
            "verdict": _t(parity, "/verdict", label=VERIFIED_LABEL),
            "verdict_basis": _t(parity, "/verdict_basis"),
        },
    }


def _bound_row(inp, ptr):
    """One descriptive bound row: delta/minus/plus ppm pairs from the
    estimator packet, each a {lo, hi} interval value object."""
    raw, src = field(inp, ptr)

    def pair(name):
        return value(None, "ppm",
                     {"path": src["path"], "ptr": "%s/%s" % (ptr, name),
                      "sha256": src["sha256"]},
                     lo=raw[name][0], hi=raw[name][1],
                     interval={"kind": "descriptive_bound",
                               "level_ppm": raw["eta_ppm"]})
    return {
        "delta": pair("delta"),
        "eta": value(raw["eta_ppm"], "ppm",
                     {"path": src["path"],
                      "ptr": "%s/eta_ppm" % ptr,
                      "sha256": src["sha256"]}),
        "minus": pair("minus"),
        "plus": pair("plus"),
    }


# ── spend.json ─────────────────────────────────────────────────────────
def spend(inputs):
    i = _by_id(inputs)
    parity, repairs, study = i["parity-t3"], i["s4-repairs"], i["study-r559"]
    dev, record = i["deviations"], i["record"]

    entries = [
        {"name": "parity tranche 3",
         "exposure_micros": absent(
             "not_computed",
             reason="exposure field absent from packet"),
         "usd": _micros(parity, "/cost_usd")},
        {"name": "s4-30slot-repairs",
         "exposure_micros": _v(repairs, "/costs/exposure_micros",
                               "micros_usd"),
         "usd": _micros(repairs, "/costs/usd")},
        {"name": "study report (R5.59 fold)",
         "exposure_micros": _v(study, "/exposure_micros", "micros_usd"),
         "usd": _v(study, "/exposure_micros", "micros_usd")},
    ]

    b10 = record.doc["bounds"][9]
    bound10 = text(b10, {"path": record.path, "ptr": "/bounds/9",
                         "sha256": record.sha256})
    reattempt_raw, _ = field(dev, "/departures/0/counts/reattempted")
    slots_raw, _ = field(dev, "/composite/slots")

    return {
        "bound_10": bound10,
        "compute": dict(COMPUTE_ABSENT),
        "entries": entries,
        "inference_usd": _micros(parity, "/cost_usd"),
        "notes": [
            "Inference cost is structured in every packet (REQ-XA-124).",
            "Compute for round 5 is unrecorded{pre_schema} — the "
            "hh-compute-cost/1 schema does not exist yet.",
            "The VM's rate-card estimate lives only in bound 10 of the "
            "record, verbatim above; billing was never read.",
        ],
        "reattempt_share": {
            "derived": "D-6",
            "inputs": [{"path": dev.path,
                        "ptr": "/departures/0/counts/reattempted",
                        "sha256": dev.sha256},
                       {"path": dev.path, "ptr": "/composite/slots",
                        "sha256": dev.sha256}],
            "label": "descriptive — ratio of two displayed point values, "
                     "no interval",
            "ratio": derive.DERIVATIONS["D-6"]["ratio"](reattempt_raw,
                                                      slots_raw),
        },
    }


# ── method.json ────────────────────────────────────────────────────────
def method(inputs):
    i = _by_id(inputs)
    repairs, dev = i["s4-repairs"], i["deviations"]

    departures = []
    for n, d in enumerate(dev.resolve("/departures")):
        departures.append({"detail": _t(dev, "/departures/%d/detail" % n),
                           "id": _t(dev, "/departures/%d/id" % n)})

    repairs_reg = []
    for n, r in enumerate(dev.resolve("/repairs")):
        repairs_reg.append({
            "class": _t(dev, "/repairs/%d/class" % n),
            "repair": _t(dev, "/repairs/%d/repair" % n),
        })

    return {
        "sections": [
            {"name": "Pre-registration",
             "state": absent("not_yet_available",
                             awaits="comparative_protocol/1",
                             gate="FP-PREREG",
                             reason="no pre-registration packet")},
            {"name": "Parity status (L1 / L2)",
             "state": absent("not_yet_available",
                             awaits="hh-l1-conformance/1",
                             gate="G-L1",
                             reason="L1 gate not reached")},
            {"name": "Egress allowlist policy",
             "values": {
                 "admission_mechanism": _t(repairs,
                                           "/admission/mechanism"),
                 "egress_admitted": _v(repairs, "/counts/egress_admitted",
                                       "count"),
                 "egress_none": _v(repairs, "/counts/egress_none",
                                   "count"),
                 "egress_profile": _t(repairs,
                                      "/admission/egress_profile"),
             }},
            {"name": "Scan coverage — Scanned / Unreadable / "
                     "No trajectory / Scan error / Not scanned",
             "state": absent("not_yet_available",
                             awaits="answer-scan/1",
                             gate="FP-RESULTS",
                             reason="no scan packet")},
            {"name": "Missingness and completion envelopes",
             "state": absent("not_yet_available",
                             awaits="hh-comparative-results/1",
                             gate="FP-RESULTS",
                             reason="no results packet")},
            {"name": "Discordances and repairs register",
             "values": {"departures": departures,
                        "repairs": repairs_reg}},
        ],
    }


# ── claims.json ────────────────────────────────────────────────────────
# Mandatory labels (REQ-XD-092 as they apply to this phase) and the three
# "what we don't claim" lines — generator constants, never packet values.
LABELS_ALWAYS = ["internal record; not outside-verified"]
LABELS_RESULTS = [
    "finite-manifest scope",
    "allowlisted agent network",
    "fewer than 89 tasks and other departures, so not "
    "leaderboard-comparable",
]
DONT_CLAIM = [
    "No rank ordering is claimed from a fixture or a single tranche.",
    "The phase-1 record is an internal record; it is not outside-verified.",
    "A sound negative, inconclusive, declined or withheld outcome is as "
    "publishable as a positive one, with the same structure and "
    "prominence.",
]


def claims(inputs):
    i = _by_id(inputs)
    est, record = i["estimators"], i["record"]
    refusals = []
    for n, a in enumerate(est.resolve("/parity_admission")):
        refusals.append({
            "display": _t(est, "/parity_admission/%d/display" % n),
            "rung": _t(est, "/parity_admission/%d/rung" % n),
            "variant": _t(est, "/parity_admission/%d/variant" % n)})
    kl_raw, kl_src = field(est, "/kl_equivalence")
    refusals.append({
        "display": text(kl_raw["outcome"]["display"],
                        {"path": kl_src["path"],
                         "ptr": "/kl_equivalence/outcome/display",
                         "sha256": kl_src["sha256"]}),
        "rung": text(kl_raw["outcome"]["rung"],
                     {"path": kl_src["path"],
                      "ptr": "/kl_equivalence/outcome/rung",
                      "sha256": kl_src["sha256"]}),
        "variant": text(kl_raw["outcome"]["variant"],
                        {"path": kl_src["path"],
                         "ptr": "/kl_equivalence/outcome/variant",
                         "sha256": kl_src["sha256"]}),
    })
    return {
        "bounds": bounds_list(record),
        "dont_claim": DONT_CLAIM,
        "labels": {"always": LABELS_ALWAYS,
                   "card_state": "Available — internal record; "
                                 "not outside-verified",
                   "evidence_class": "recorded execution",
                   "results": LABELS_RESULTS},
        "refusals": refusals,
    }


# ── changelog.json ─────────────────────────────────────────────────────
def changelog(inputs, generator_version):
    return {
        "entries": [],
        "generator_version": generator_version,
        "note": "No hh-site-publication/1 records exist; changelog "
                "entries derive from publication records and generator "
                "versions (REQ-XF-140) and are empty by design today.",
    }


# ── tfp.json — the phase-2 first-pass tranche (R6.20; F2-c (a)) ────────
# The fixture packet is admitted under the seal (packets.FIXTURE_GATED_
# SCHEMAS): it is a presentation fixture, never a first-pass result.
# Every rendered quantity is copied with its per-field pointer; the only
# transforms are the registered derivations (D-3 ordering, D-5 Pareto
# membership). A packet absent stays a typed absent — the packet's own
# state word is carried verbatim in ``absent_state``.
#
# The packet's leader_group labels are not copied: the observed-leader
# band's wording is the spec-fixed site label (REQ-XF-111), rendered by
# the page, not stored here.


def _iv3(inp, unit, kind, level, point_ptr, lo_ptr, hi_ptr, label=None):
    """One (point, lo, hi) quantity as three value objects sharing the
    quantity's lo/hi/interval meta — each src'd to its own packet field
    so every rendered numeral carries resolvable provenance."""
    lo_raw, lo_src = field(inp, lo_ptr)
    hi_raw, hi_src = field(inp, hi_ptr)
    iv = {"kind": kind, "level_ppm": level}
    # Wrapper keys are "low"/"high" — a container must never reuse the
    # value-object member names lo/hi/interval (the intervals leg would
    # read the container itself as an estimate).
    out = {
        "high": value(hi_raw, unit, hi_src, lo=lo_raw, hi=hi_raw,
                      interval=iv),
        "low": value(lo_raw, unit, lo_src, lo=lo_raw, hi=hi_raw,
                     interval=iv),
    }
    if point_ptr is not None:
        raw, src = field(inp, point_ptr)
        if not isinstance(raw, int) or isinstance(raw, bool):
            raise Refusal("point %s#%s is not an integer field"
                          % (inp.path, point_ptr))
        out["point"] = value(raw, unit, src, lo=lo_raw, hi=hi_raw,
                             interval=iv, label=label)
    return out


def _absent_of(raw):
    """Carry a packet typed absent verbatim: the packet's state word is
    the evidence — never mapped onto a different site vocabulary."""
    out = {"absent_state": raw.get("state")}
    if raw.get("reason") is not None:
        out["reason"] = raw["reason"]
    if raw.get("variant") is not None:
        out["variant"] = raw["variant"]
    return out


def _cost_basis(pkt, ptr):
    """One cost basis block: per-trial and per-solve with intervals,
    provenance and label — verbatim packet values."""
    out = {}
    for name in ("per_trial", "per_solve"):
        raw, src = field(pkt, "%s/%s" % (ptr, name))
        if isinstance(raw, dict) and raw.get("state"):
            out[name] = _absent_of(raw)
            continue
        iv, _ = field(pkt, "%s/%s/interval" % (ptr, name))
        out[name] = _iv3(pkt, "micros_usd", iv["kind"],
                         iv["level_ppm"], "%s/%s/micros" % (ptr, name),
                         "%s/%s/interval/lo_ppm" % (ptr, name),
                         "%s/%s/interval/hi_ppm" % (ptr, name))
    out["provenance"] = _t(pkt, "%s/provenance" % ptr)
    out["label"] = _t(pkt, "%s/label" % ptr)
    return out


def _harness_row(pkt, hid):
    """One §9.2 leaderboard row over the packet's harness entry."""
    base = "/harnesses/%s" % hid
    th_iv, _ = field(pkt, "%s/theta/interval" % base)
    th_kind, th_level = th_iv["kind"], th_iv["level_ppm"]
    b_iv, _ = field(pkt, "%s/theta/bounds" % base)
    theta = _iv3(pkt, "ppm", th_kind, th_level,
                 "%s/theta/ppm" % base,
                 "%s/theta/lo_ppm" % base,
                 "%s/theta/hi_ppm" % base)
    theta["bounds"] = {
        "high": _v(pkt, "%s/theta/bounds/hi_ppm" % base, "ppm"),
        "kind": _t(pkt, "%s/theta/bounds/kind" % base),
        "low": _v(pkt, "%s/theta/bounds/lo_ppm" % base, "ppm"),
    }
    theta["label"] = _t(pkt, "%s/theta/label" % base)

    pops = {
        "raw": {
            "rate": _v(pkt, "%s/secondary_populations/raw/rate_ppm"
                       % base, "ppm"),
            "label": _t(pkt, "%s/secondary_populations/raw/label"
                        % base),
        },
        "errors_zero": {
            "bound_lo": _v(pkt, "%s/secondary_populations/errors_zero/"
                           "bound_lo_ppm" % base, "ppm"),
            "bound_hi": _v(pkt, "%s/secondary_populations/errors_zero/"
                           "bound_hi_ppm" % base, "ppm"),
            "label": _t(pkt, "%s/secondary_populations/errors_zero/"
                        "label" % base),
        },
    }

    d_raw, d_src = field(pkt, "%s/delta_vs_ref" % base)
    if isinstance(d_raw, dict) and d_raw.get("state"):
        delta = _absent_of(d_raw)
    else:
        d_iv = d_raw["interval"]
        delta = _iv3(pkt, "ppm", d_iv["kind"], d_iv["level_ppm"],
                     "%s/delta_vs_ref/ppm" % base,
                     "%s/delta_vs_ref/interval/lo_ppm" % base,
                     "%s/delta_vs_ref/interval/hi_ppm" % base)
        delta["p_raw"] = _v(pkt, "%s/delta_vs_ref/p_raw_ppm" % base,
                            "ppm")
        delta["p_holm"] = _v(pkt, "%s/delta_vs_ref/p_holm_ppm" % base,
                             "ppm")
        delta["separated"] = _flag(pkt,
                                   "%s/delta_vs_ref/separated" % base)
        delta["label"] = _t(pkt, "%s/delta_vs_ref/label" % base)

    lg = {
        "member": _flag(pkt, "%s/leader_group/member" % base),
        "diff": _iv3(pkt, "ppm",
                     pkt.resolve("%s/leader_group/diff_interval/kind"
                                 % base),
                     pkt.resolve("%s/leader_group/diff_interval/"
                                 "level_ppm" % base),
                     None,
                     "%s/leader_group/diff_interval/lo_ppm" % base,
                     "%s/leader_group/diff_interval/hi_ppm" % base),
    }

    esc = lambda k: k.replace("~", "~0").replace("/", "~1")
    counts = {
        "analysis_basis": _t(pkt, "%s/counts/analysis_basis" % base),
        # version keys are packet member names — plain strings, never
        # data-src'd (a pointer resolves to the count, not the key).
        "intended": [
            {"count": _v(pkt, "%s/counts/intended/%s" % (base, esc(k)),
                         "count"),
             "version": k}
            for k in sorted(pkt.resolve("%s/counts/intended" % base))],
        "served": [
            {"count": _v(pkt, "%s/counts/served/%s" % (base, esc(k)),
                         "count"),
             "version": k}
            for k in sorted(pkt.resolve("%s/counts/served" % base))],
    }

    completion = {m: _v(pkt, "%s/completion/%s" % (base, m), "count")
                  for m in ("planned", "first", "recovered",
                            "unresolved")}

    integrity = {
        "cleared": _v(pkt, "%s/integrity/cleared" % base, "count"),
        "confirmed": _v(pkt, "%s/integrity/confirmed" % base, "count"),
        "coverage": _v(pkt, "%s/integrity/coverage_ppm" % base, "ppm"),
        "detector": _t(pkt, "%s/integrity/detector" % base),
        "flagged": _v(pkt, "%s/integrity/flagged" % base, "count"),
        "label": _t(pkt, "%s/integrity/label" % base),
    }

    row = {
        "agent_time": absent(
            "not_computed",
            reason="hh-comparative-results/1 carries no agent-time "
                   "field"),
        "completion": completion,
        "costs": {"model_spend_only": _cost_basis(pkt, "%s/cost" % base)},
        "counts": counts,
        "delta": delta,
        "id": hid,
        "integrity": integrity,
        "leader_group": lg,
        "populations": pops,
        "theta": theta,
    }
    if _present(pkt, "%s/cost_with_compute" % base):
        row["costs"]["with_compute"] = _cost_basis(
            pkt, "%s/cost_with_compute" % base)
    return row


def tfp(inputs):
    """tfp.json — leaderboard + frontier projection of the first-pass
    tranche fixture (REQ-XF-115; no new derivation: D-3 ordering by the
    displayed theta, D-5 Pareto membership over complete entries)."""
    i = _by_id(inputs)
    pkt = i["p2-fp"]

    rows = [_harness_row(pkt, hid)
            for hid in sorted(pkt.resolve("/harnesses"))]
    sortable = [{"id": r["id"], "theta": r["theta"]["point"]["v"]}
                for r in rows]
    order_ids = [s["id"] for s in derive.DERIVATIONS["D-3"]["order"](
        sortable, "theta", reverse=True)]
    by_id = {r["id"]: r for r in rows}
    rows = [by_id[k] for k in order_ids]

    fentries = [{"cost": r["costs"]["model_spend_only"]
                 ["per_trial"]["point"]["v"],
                 "id": r["id"],
                 "theta": r["theta"]["point"]["v"]}
                for r in rows]
    pareto = derive.DERIVATIONS["D-5"]["pareto"](fentries, "cost",
                                               "theta")

    return {
        "family": {
            "alpha": _v(pkt, "/family/alpha_ppm", "ppm"),
            "id": _t(pkt, "/family/id"),
            "method": _t(pkt, "/family/method"),
            "reference": _t(pkt, "/family/reference"),
        },
        "fixture": True,
        "frontier": {
            "entries": [
                {"cost": e["cost"],
                 "id": e["id"],
                 "pareto": e["id"] in pareto,
                 "theta": e["theta"]}
                for e in fentries],
            "pareto_members": derived(
                "D-5",
                [{"path": pkt.path, "ptr": "/harnesses",
                  "sha256": pkt.sha256}],
                pareto),
        },
        "labels": {k: _t(pkt, "/labels/%s" % k)
                   for k in sorted(pkt.resolve("/labels"))},
        "metric": _t(pkt, "/metric"),
        "order": derived(
            "D-3",
            [{"path": pkt.path, "ptr": "/harnesses",
              "sha256": pkt.sha256}],
            order_ids),
        "populations": [_t(pkt, "/populations/%d" % n)
                        for n in range(len(pkt.resolve("/populations")))],
        "rows": rows,
        "tranche": {"band": _t(pkt, "/tranche/band"),
                    "id": _t(pkt, "/tranche/id")},
    }


# file name -> (projection, consumed input ids). The envelope records the
# inputs each file consumed; manifest consumes all.
FILES = {
    "phase1.json": (phase1, ("parity-t3", "s4-repairs", "study-r559",
                            "xcheck", "estimators", "deviations",
                            "record")),
    "spend.json": (spend, ("parity-t3", "s4-repairs", "study-r559",
                           "deviations", "record")),
    "method.json": (method, ("s4-repairs", "deviations")),
    "claims.json": (claims, ("estimators", "record")),
    "tfp.json": (tfp, ("p2-fp",)),
}
