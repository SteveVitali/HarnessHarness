# `docs/results/publication/` — `hh-site-publication/1` records

Append-only publication records for the results site (design/39 F10,
REQ-XF-128/129, REQ-XH-018). **Until GATE-G22 is decided, this directory
holds no records and nothing is public.**

## Record schema — `hh-site-publication/1`

One JSON object per file, named `<phase>-<YYYY-MM-DD>.json`:

| Field | Rule |
|---|---|
| `schema` | `"hh-site-publication/1"` |
| `phase` | `phase-1` … `phase-4`; must match the filename prefix |
| `pages` | URL prefixes the record publishes; must cover `/<phase>/` (or `/` for the home readings) |
| `inputs` | `[{path, sha256}]` — the phase's exact input set, one entry per named generator input for that phase |
| `inputs_digest` | `sha256:` + sha256 over the canonical inputs list (sorted keys, compact separators — the same construction as `hh-site-data/1` provenance) |
| `gate_row` | the manifest gate row id authorizing the publish: `GATE-G22` (phase-1), `GATE-G31` (phase-2 first-pass) — a phase without a manifest gate cannot publish |
| `readout` | repo-relative path of the gate readout under `docs/build/`; the file must exist and name the gate. The operator's decision lives there — never copied into the record |
| `labels` | `{evidence_class, outside_verified: false}` — `outside_verified` stays `false` until a graduation record exists |
| `dataset` | `{visibility, revision, secret_scan_ref, contamination_scan_ref}` — while `private`, the three refs are `null`; `public` requires all three plus a superseding record (HUMAN-H17) |
| `recorded_at` | ISO date; must equal the filename date |
| `supersedes` | optional — repo-relative path of the earlier record for the same phase (append-only supersede by reference) |

Example (fixture shape — the real record appears only after GATE-G22):

```json
{
  "schema": "hh-site-publication/1",
  "phase": "phase-1",
  "pages": ["/", "/phase-1/"],
  "inputs": [{"path": "docs/build/reports/<packet>.json", "sha256": "…"}],
  "inputs_digest": "sha256:…",
  "gate_row": "286_GATE-G22__pub-p1",
  "readout": "docs/build/readouts/GATE-G22.md",
  "labels": {"evidence_class": "recorded execution",
             "outside_verified": false},
  "dataset": {"visibility": "private", "revision": null,
              "secret_scan_ref": null, "contamination_scan_ref": null},
  "recorded_at": "2026-11-08"
}
```

## Semantics

- **Append-only.** Records are never edited or deleted; a correction is
  a new record naming its predecessor in `supersedes`.
- **Pin, don't launder.** A record pins *exactly* the input set it
  published. If a named packet's current bytes no longer match the
  pinned sha256, the public build **refuses** — it never re-pins the
  changed bytes silently (REQ-XF-129). Republishing drifted inputs is a
  new record, a new operator gate.
- **Derived surfaces.** Changelog entries and `/archive/<version>/`
  markers render deterministically from the record set
  (REQ-XF-140); superseded versions stay reachable and marked.
- **Verification** is the generator's `run_check` over the pinned
  packets plus the site checker — the record does not restate expected
  values.
- **Validate** with `bash scripts/check-publication.sh`; the checklist
  for an operator publish is [`CHECKLIST.md`](CHECKLIST.md).
