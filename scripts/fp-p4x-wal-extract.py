#!/usr/bin/env python3
"""fp-p4x-wal-extract.py — the R7.4 (manifest 330) verbatim member
projection of the committed X1 session WAL into the model-events
extract the fold reads.

    python3 -I scripts/fp-p4x-wal-extract.py <session-wal.tar.gz> <out.jsonl>

The committed `2026-11-24-fp-p4x1-session-wal.tar.gz` holds the sealed
run WAL (`runs/p4x-x1-r3/events.wal`). The fold's fidelity and
usage-source legs need only the model-call surface — this projection
keeps, per event and in WAL order:

- `measurement.cost.attributed` → class, participant_ref (the slot
  id), model_ref (served effort/model/route), charged_to, roles (the
  per-call token usage sources — `tokens.input.uncached`,
  `tokens.output.visible`, `rounding`), money.micro_units;
- `model.call.completed` → class, model_call_id, served_model,
  disposition, usage;
- `model.call.failed` / `model.call.attempt.failed` → class,
  model_call_id, disposition/detail as recorded.

Every member is copied verbatim from the WAL event — no value is
synthesized. Deterministic: same tar, byte-identical output. Offline;
no network, no clock.
"""

import json
import sys
import tarfile

WANT = {
    "measurement.cost.attributed",
    "model.call.completed",
    "model.call.failed",
    "model.call.attempt.failed",
}


def project(event):
    cls = event.get("class")
    payload = event.get("payload", {})
    if cls == "measurement.cost.attributed":
        att = payload.get("attribution", {})
        return {
            "class": cls,
            "participant_ref": att.get("participant_ref"),
            "model_ref": att.get("model_ref", payload.get("model_ref")),
            "charged_to": att.get("charged_to"),
            "roles": payload.get("roles"),
            "micro_units": (payload.get("money") or {}).get("micro_units"),
        }
    if cls == "model.call.completed":
        return {
            "class": cls,
            "model_call_id": payload.get("model_call_id"),
            "served_model": payload.get("served_model"),
            "disposition": payload.get("disposition"),
            "usage": payload.get("usage"),
        }
    return {
        "class": cls,
        "model_call_id": payload.get("model_call_id"),
        "detail": payload.get("detail", payload.get("reason")),
    }


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: fp-p4x-wal-extract.py <wal.tar.gz> <out.jsonl>")
    tar_path, out_path = sys.argv[1], sys.argv[2]
    rows = 0
    with tarfile.open(tar_path, "r:gz") as tar, open(out_path, "w") as out:
        wal = next(m for m in tar.getmembers() if m.name.endswith("events.wal"))
        src = tar.extractfile(wal)
        for raw in src:
            line = json.loads(raw.decode("utf-8"))
            if line.get("k") != "e":
                continue
            ev = line["v"]
            if ev.get("class") in WANT:
                out.write(
                    json.dumps(project(ev), separators=(",", ":"), sort_keys=True)
                    + "\n"
                )
                rows += 1
    print(f"fp-p4x-wal-extract: {rows} model events from {wal.name}")


if __name__ == "__main__":
    main()
