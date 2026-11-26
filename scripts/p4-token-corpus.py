#!/usr/bin/env python3
"""p4-token-corpus.py — the R7.1 W-rule corpus extractor (manifest 326).

Reads the retained W4x run-root's `events.wal` proxy ledgers and, for
every `model.call.completed` event, opens the pinned
`hh.model_proxy.receipt/1` blob and selects ONLY the declared
non-outcome members — `proxy.slot_id`,
`proxy.usage.view["tokens.input.total"]`,
`proxy.mutations.effort` — never a trial-outcome member
(REQ-XD-205/209's input-manifest rule; a record whose selected
members name an outcome field refuses the run, it never computes).

Emits the committed `hh-p4-token-corpus/1` record: per-trial peak
upstream input tokens, per-trial call count, the proxy-pin evidence
count, and the verbatim input manifest (record classes + selected
fields + source WAL hashes). Deterministic over a fixed run-root.

Usage:
  scripts/p4-token-corpus.py <run-root> <out.json>
"""

import json
import sys
import hashlib
import collections
import os

# The selected non-outcome members, verbatim — the manifest the
# derivation record carries. Any selected member whose name (or last
# dotted segment) is an outcome field refuses the extraction.
SELECTED = [
    "proxy.slot_id",
    "proxy.usage.source",
    "proxy.usage.view.tokens.input.total",
    "proxy.mutations.effort.requested",
    "proxy.mutations.effort.applied",
    "proxy.mutations.effort.path",
]

OUTCOME_FIELDS = {
    "reward", "verdict", "outcome", "resolved", "resolve", "score",
    "pass", "passed", "success", "solved", "answer", "grade",
    "result", "label", "winner",
}


def outcome_field(name):
    n = name.strip().lower()
    last = n.rsplit(".", 1)[-1]
    return n in OUTCOME_FIELDS or last in OUTCOME_FIELDS


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    run_root, out_path = sys.argv[1], sys.argv[2]

    # The manifest itself refuses before any read.
    for member in SELECTED:
        if outcome_field(member):
            print(
                f"refused{{outcome_field:{member}}} — the input manifest names an outcome member",
                file=sys.stderr,
            )
            return 3

    wals = []
    for dirpath, _dirs, files in os.walk(run_root):
        for name in files:
            if name == "events.wal" and "/proxy-store" in dirpath:
                wals.append(os.path.join(dirpath, name))
    wals.sort()
    if not wals:
        print("pending{records} — no proxy events.wal under the run-root", file=sys.stderr)
        return 4

    wal_entries = []
    trials = collections.defaultdict(lambda: {
        "calls": 0,
        "peak_input_tokens": 0,
        "pin_fidelity": collections.Counter(),
        "usage_absent_calls": 0,
    })
    blob_dir_of = lambda w: os.path.join(
        os.path.dirname(os.path.dirname(os.path.dirname(w))), "blobs"
    )
    read_events = 0
    for wal in wals:
        wal_entries.append({
            "path": os.path.relpath(wal, run_root),
            "sha256": sha256_file(wal),
        })
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
                rec = json.load(open(blob))
            except (OSError, json.JSONDecodeError):
                continue
            proxy = rec.get("proxy", {})
            sid = proxy.get("slot_id", "")
            if not sid or sid == "w4-lane-probe":
                continue
            t = trials[sid]
            usage = proxy.get("usage", {})
            tin = None
            if usage.get("source") == "provider_per_call":
                tin = (usage.get("view") or {}).get("tokens.input.total")
            if not tin:
                t["usage_absent_calls"] += 1
            else:
                t["calls"] += 1
                t["peak_input_tokens"] = max(t["peak_input_tokens"], tin)
            mut = (proxy.get("mutations") or {}).get("effort")
            if isinstance(mut, dict):
                key = "{}→{}@{}".format(
                    mut.get("requested") if mut.get("requested") is not None else "∅",
                    mut.get("applied"),
                    mut.get("path"),
                )
                t["pin_fidelity"][key] += 1

    trial_rows = []
    pin_total = collections.Counter()
    for sid in sorted(trials):
        t = trials[sid]
        for k, n in t["pin_fidelity"].items():
            pin_total[k] += n
        trial_rows.append({
            "trial_id": sid,
            "calls_with_usage": t["calls"],
            "calls_usage_absent": t["usage_absent_calls"],
            "measured": t["calls"] > 0,
            "peak_input_tokens": t["peak_input_tokens"] if t["calls"] > 0 else None,
        })

    rec = {
        "schema": "hh-p4-token-corpus/1",
        "recorded_at": "2026-11-20",
        "ticket": "326_R7.1",
        "note": (
            "the W4x retained run-root's proxy receipts, non-outcome "
            "members only (REQ-XD-205/209): per-trial peak upstream "
            "input tokens for the F(w) derivation and the per-call "
            "effort-pin requested/applied/path evidence; calls whose "
            "provider usage is absent are counted, never zero-filled"
        ),
        "input_manifest": {
            "rule": "non-outcome token records only — an outcome member in this manifest refuses the derivation before any read",
            "record_classes": ["model.call.completed", "hh.model_proxy.receipt/1"],
            "selected_fields": SELECTED,
            "excluded_slot_ids": ["w4-lane-probe"],
            "sources": wal_entries,
        },
        "read": {
            "model_call_completed_events": read_events,
            "trials": len(trial_rows),
            "trials_measured": sum(1 for r in trial_rows if r["measured"]),
            "calls_with_usage": sum(r["calls_with_usage"] for r in trial_rows),
            "calls_usage_absent": sum(r["calls_usage_absent"] for r in trial_rows),
        },
        "trials": trial_rows,
        "effort_pin_evidence": dict(sorted(pin_total.items())),
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
    print(f"p4-token-corpus: wrote {out_path} ({len(trial_rows)} trials)")


if __name__ == "__main__":
    sys.exit(main())
