#!/usr/bin/env python3
"""p4-activation-corpus.py — the R7.9 activation+usage corpus extractor
(manifest 342; the DF-R7.1-2 unblocker's corpus half).

Reads a retained run-root — the proxy `events.wal`s plus the pinned
`hh.model_proxy.receipt/1` and `upstream_request` blobs, and the slot
dirs' `events.json` kernel event records plus `store/blobs` model-view
blobs — and emits the committed `hh-p4-activation-corpus/1` record:

- **usage leg** — per-call `proxy.usage.source` and
  `proxy.usage.view["tokens.input.uncached"]` coverage per arm
  (member-absent and `absent` sources are counted, never filled);
- **activation leg** — the three conditional knobs' declared metrics:

    * V3 `cmd-120` — share of first subjects with ≥ 1
      `action.tool.completed` over the 30 s baseline bound
      (`execution_ms > 30000`);
    * V4 `obs-2.5k` — exact counterfactual rendering difference:
      `context.observation.recorded` model views whose measured output
      differs under the level's 2500/1250/1250 vs the baseline's
      10000/5000/5000 — the deterministic rule is `measure ≥ 2500`
      (2500 ≤ m < 10000: only the level truncates; m ≥ 10000: both
      truncate with different head/tail/elided_chars — always a
      difference; the truncated blob's `elided_chars` recovers
      `measure = elided_chars + 10000` exactly);
    * V5 `rsn-drop` — share of upstream-bound requests carrying ≥ 1
      non-empty reasoning member (`reasoning_content`/`reasoning`/
      `thinking`) the drop knob exercises.

Selected members are the manifest-declared non-outcome fields only —
the manifest itself refuses before any read, and a record whose
selected members name an outcome field refuses the run (the sibling
p4-token-corpus.py rule, REQ-XD-205/209). Deterministic over a fixed
run-root. Offline; no network, no clock.

Usage:
  scripts/p4-activation-corpus.py <run-root> <out.json>
"""

import json
import sys
import hashlib
import collections
import os

SCHEMA = "hh-p4-activation-corpus/1"

# The selected non-outcome members, verbatim — the manifest the
# derivation record carries. Any selected member whose name (or last
# dotted segment) is an outcome field refuses the extraction.
SELECTED = [
    "proxy.slot_id",
    "proxy.surface",
    "proxy.disposition",
    "proxy.usage.source",
    "proxy.usage.view.tokens.input.total",
    "proxy.usage.view.tokens.input.uncached",
    "proxy.usage.view.tokens.output.total",
    "proxy.upstream.request_body_ref",
    "upstream_request.messages[].reasoning_content",
    "upstream_request.messages[].reasoning",
    "upstream_request.messages[].thinking",
    "action.tool.completed.execution_ms",
    "action.tool.completed.status",
    "context.observation.recorded.model_view.ref",
    "context.observation.recorded.model_view.renderer",
]

OUTCOME_FIELDS = {
    "reward", "verdict", "outcome", "resolved", "resolve", "score",
    "pass", "passed", "success", "solved", "answer", "grade",
    "result", "label", "winner",
}

# The declared activation parameters — the committed registry's own
# constants (V3 baseline 30000; V4 baseline 10000/5000/5000 vs level
# 2500/1250/1250; V5 reasoning spellings).
V3_BASELINE_MS = 30000
V4_BASELINE = {"threshold": 10000, "head": 5000, "tail": 5000}
V4_LEVEL = {"threshold": 2500, "head": 1250, "tail": 1250}
V5_REASONING_SPELLINGS = ("reasoning_content", "reasoning", "thinking")

LANE_PROBE_SLOTS = {"w4-lane-probe", "p4x-lane-probe", "p4c-lane-probe"}


def outcome_field(name):
    n = name.strip().lower()
    last = n.rsplit(".", 1)[-1].rstrip("[]")
    return n in OUTCOME_FIELDS or last in OUTCOME_FIELDS


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def arm_of(slot_id):
    """The arm grouping key — the second dash segment, verbatim; slot
    ids name their arm there (`w4-pi-*`→pi, `r79-claude-01`→claude)."""
    parts = slot_id.split("-")
    return parts[1] if len(parts) > 1 else slot_id


def load_json(path):
    with open(path) as f:
        return json.load(f)


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    run_root, out_path = sys.argv[1], sys.argv[2]

    for member in SELECTED:
        if outcome_field(member):
            print(
                f"refused{{outcome_field:{member}}} — the input manifest names an outcome member",
                file=sys.stderr,
            )
            return 3

    # ── sources: proxy WALs + slot event records ──────────────────
    wals = []
    slot_events = []
    for dirpath, _dirs, files in os.walk(run_root):
        for name in files:
            p = os.path.join(dirpath, name)
            if name == "events.wal" and "/proxy-store" in dirpath.replace(
                os.sep, "/"
            ):
                wals.append(p)
            elif name == "events.json" and "/slots/" in dirpath.replace(
                os.sep, "/"
            ):
                slot_events.append(p)
    wals.sort()
    slot_events.sort()
    if not wals and not slot_events:
        print(
            "pending{records} — no proxy events.wal and no slot events.json under the run-root",
            file=sys.stderr,
        )
        return 4

    sources = [
        {"path": os.path.relpath(p, run_root), "sha256": "sha256:" + sha256_file(p)}
        for p in wals + slot_events
    ]

    blob_dir_of = lambda w: os.path.join(
        os.path.dirname(os.path.dirname(os.path.dirname(w))), "blobs"
    )

    # ── leg 1: per-call usage + the V5 request scan ───────────────
    calls = []           # per-call rows
    v5_total = 0
    v5_exercised = 0
    v5_per_arm = collections.defaultdict(lambda: [0, 0])  # arm → [exercised, total]
    read_events = 0
    for wal in wals:
        blob_dir = blob_dir_of(wal)
        for line in open(wal):
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                continue
            if row.get("k") != "e":
                continue
            ev = row["v"]
            if ev.get("class") != "model.call.completed":
                continue
            read_events += 1
            ref = (ev.get("payload") or {}).get("response_ref", "")
            blob = os.path.join(blob_dir, ref.split(":", 1)[-1])
            try:
                rec = load_json(blob)
            except (OSError, json.JSONDecodeError):
                continue
            proxy = rec.get("proxy", {})
            sid = proxy.get("slot_id", "")
            if not sid or sid in LANE_PROBE_SLOTS:
                continue
            usage = proxy.get("usage", {}) or {}
            view = usage.get("view", {}) or {}
            uncached = view.get("tokens.input.uncached")
            calls.append({
                "slot_id": sid,
                "arm": arm_of(sid),
                "surface": proxy.get("surface"),
                "usage_source": usage.get("source"),
                "tokens_input_uncached": (
                    uncached if isinstance(uncached, int) else None
                ),
                "uncached_member": (
                    "present" if isinstance(uncached, int) else "absent"
                ),
            })
            # V5 — the upstream-bound request's reasoning presence.
            up = proxy.get("upstream", {}) or {}
            rref = up.get("request_body_ref")
            if not rref:
                continue
            rblob = os.path.join(blob_dir, rref.split(":", 1)[-1])
            try:
                req = load_json(rblob)
            except (OSError, json.JSONDecodeError):
                continue
            msgs = req.get("messages")
            if not isinstance(msgs, list):
                continue
            v5_total += 1
            va = v5_per_arm[arm_of(sid)]
            va[1] += 1
            has = False
            for m in msgs:
                if not isinstance(m, dict):
                    continue
                for spelling in V5_REASONING_SPELLINGS:
                    v = m.get(spelling)
                    if v is None:
                        continue
                    if (isinstance(v, str) and v) or (
                        isinstance(v, (list, dict)) and len(v) > 0
                    ):
                        has = True
            if has:
                v5_exercised += 1
                va[0] += 1

    # ── leg 2: slot events — V3 durations + V4 observation measures ──
    subjects = {}        # slot_id → {arm, completions, over_baseline}
    v4_total = 0
    v4_exercised = 0
    v4_truncated = 0
    v4_unreadable = 0
    for path in slot_events:
        slot_dir = os.path.dirname(path)
        sid = os.path.basename(slot_dir)
        blob_dir = os.path.join(slot_dir, "store", "blobs")
        try:
            evs = load_json(path)
        except (OSError, json.JSONDecodeError):
            continue
        sub = subjects.setdefault(
            sid, {"arm": arm_of(sid), "completions": 0, "over_baseline": 0,
                  "statuses": {}}
        )
        for e in evs:
            cls = e.get("class")
            pay = e.get("payload") or {}
            if cls == "action.tool.completed":
                sub["completions"] += 1
                st = pay.get("status")
                if isinstance(st, str):
                    sub["statuses"][st] = sub["statuses"].get(st, 0) + 1
                ms = pay.get("execution_ms")
                if isinstance(ms, int) and ms > V3_BASELINE_MS:
                    sub["over_baseline"] += 1
            elif cls == "context.observation.recorded":
                mv = pay.get("model_view") or {}
                ref = mv.get("ref")
                if not (isinstance(ref, str) and ref.startswith("sha256:")):
                    continue
                bp = os.path.join(blob_dir, ref.split(":", 1)[-1])
                try:
                    rec = load_json(bp)
                except (OSError, json.JSONDecodeError):
                    v4_unreadable += 1
                    continue
                v4_total += 1
                out = rec.get("output")
                if isinstance(out, str):
                    measure = len(out)
                else:
                    ec = rec.get("elided_chars")
                    if isinstance(ec, int):
                        # baseline-truncated: measure = elided + baseline
                        # threshold (the template's own arithmetic).
                        measure = ec + V4_BASELINE["threshold"]
                        v4_truncated += 1
                    else:
                        v4_unreadable += 1
                        continue
                # The deterministic counterfactual: measure ≥ the
                # level's threshold ⇒ the two renders differ.
                if measure >= V4_LEVEL["threshold"]:
                    v4_exercised += 1

    # ── rollups ──────────────────────────────────────────────────
    per_arm = collections.defaultdict(lambda: {
        "calls": 0,
        "calls_provider_per_call": 0,
        "calls_usage_absent": 0,
        "calls_uncached_member": 0,
        "uncached_total": 0,
    })
    for c in calls:
        a = per_arm[c["arm"]]
        a["calls"] += 1
        if c["usage_source"] == "provider_per_call":
            a["calls_provider_per_call"] += 1
        elif c["usage_source"] == "absent":
            a["calls_usage_absent"] += 1
        if isinstance(c["tokens_input_uncached"], int):
            a["calls_uncached_member"] += 1
            a["uncached_total"] += c["tokens_input_uncached"]

    subject_rows = []
    v3_subjects_total = 0
    v3_subjects_exercised = 0
    for sid in sorted(subjects):
        s = subjects[sid]
        exercised = s["over_baseline"] > 0
        v3_subjects_total += 1
        if exercised:
            v3_subjects_exercised += 1
        subject_rows.append({
            "slot_id": sid,
            "arm": s["arm"],
            "completions": s["completions"],
            "over_baseline_completions": s["over_baseline"],
            "exercised": exercised,
            "statuses": dict(sorted(s["statuses"].items())),
        })

    rec = {
        "schema": SCHEMA,
        "ticket": "342_R7.9",
        "note": (
            "the DF-R7.1-2 corpus half: per-call usage coverage "
            "(tokens.input.uncached member census — counted, never "
            "filled) plus the three conditional knobs' activation "
            "inputs from the retained run-root's non-outcome records"
        ),
        "input_manifest": {
            "rule": "non-outcome fields only — an outcome member in this manifest refuses the derivation before any read",
            "record_classes": [
                "model.call.completed",
                "hh.model_proxy.receipt/1",
                "upstream_request (hh.model_io/1 blob)",
                "action.tool.completed",
                "context.observation.recorded",
            ],
            "selected_fields": SELECTED,
            "excluded_slot_ids": sorted(LANE_PROBE_SLOTS),
            "activation_params": {
                "V3": {
                    "baseline_timeout_ms": V3_BASELINE_MS,
                    "exercise": "subject with ≥ 1 completion execution_ms > baseline_timeout_ms",
                    "declared_metric": "share of first subjects with ≥ 1 command > 30 s",
                },
                "V4": {
                    "baseline": V4_BASELINE,
                    "level": V4_LEVEL,
                    "exercise": "observation whose output measure ≥ level.threshold — the two deterministic renders differ",
                    "declared_metric": "exact counterfactual rendering difference",
                },
                "V5": {
                    "reasoning_spellings": list(V5_REASONING_SPELLINGS),
                    "exercise": "upstream-bound request carrying ≥ 1 non-empty reasoning member",
                    "declared_metric": "share of requests with a dropped non-empty reasoning member",
                },
            },
            "sources": sources,
        },
        "read": {
            "model_call_completed_events": read_events,
            "calls": len(calls),
            "slots_with_events_json": len(subjects),
            "v4_observation_blobs_unreadable": v4_unreadable,
        },
        "usage": {
            "per_arm": {k: per_arm[k] for k in sorted(per_arm)},
        },
        "calls": calls,
        "activation": {
            "V3": {
                "exercised": v3_subjects_exercised,
                "total": v3_subjects_total,
                "unit": "subjects",
                "subjects": subject_rows,
            },
            "V4": {
                "exercised": v4_exercised,
                "total": v4_total,
                "unit": "observations",
                "baseline_truncated": v4_truncated,
            },
            "V5": {
                "exercised": v5_exercised,
                "total": v5_total,
                "unit": "requests",
                "per_arm": {
                    k: {"exercised": v[0], "total": v[1]}
                    for k, v in sorted(v5_per_arm.items())
                },
            },
        },
    }
    body = json.dumps(rec, sort_keys=True, separators=(",", ":")) + "\n"
    with open(out_path, "w") as f:
        f.write(body)
    sha = hashlib.sha256(body.encode()).hexdigest()
    sidecar = out_path[:-5] + ".sha256" if out_path.endswith(".json") else out_path + ".sha256"
    with open(sidecar, "w") as f:
        f.write(
            f"{sha}  {out_path}\n"
            f"# recompute: shasum -a 256 -c {out_path[:-5]}.sha256 (run from the repo root — the record is canonical JSON)\n"
        )
    print(
        f"p4-activation-corpus: wrote {out_path} "
        f"({len(calls)} calls, {len(subjects)} subjects, "
        f"V3 {v3_subjects_exercised}/{v3_subjects_total} "
        f"V4 {v4_exercised}/{v4_total} "
        f"V5 {v5_exercised}/{v5_total})"
    )


if __name__ == "__main__":
    main()
