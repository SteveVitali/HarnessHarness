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
            "phase-2": "not_yet_available{publication pending}",
            "phase-3": "not_yet_available{publication pending}",
            "phase-4": "not_yet_available{publication pending}",
        },
        "site_files": ["changelog.json", "claims.json", "manifest.json",
                       "method.json", "phase1.json", "spend.json"],
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
}
