# `docs/results/publication/` — `hh-site-publication/1` records

Append-only publication records for the results site (design/39 F10,
REQ-XF-128/129, REQ-XH-018). **Until GATE-G22 is decided, this directory
holds no records and nothing is public.**

## Record schema — `hh-site-publication/1`

One JSON object per file, named `<phase>-<YYYY-MM-DD>.json`, or `<phase>-<YYYY-MM-DD>-flip.json`
for a superseding HUMAN-H17 visibility-flip record (CAP6.2 / DF-CAP6.1-5):

| Field | Rule |
|---|---|
| `schema` | `"hh-site-publication/1"` |
| `phase` | `phase-1` … `phase-4`; must match the filename prefix |
| `pages` | URL prefixes the record publishes; must cover `/<phase>/` (or `/` for the home readings) |
| `inputs` | `[{path, sha256}]` — the phase's exact input set, one entry per named generator input for that phase |
| `inputs_digest` | `sha256:` + sha256 over the canonical inputs list (sorted keys, compact separators — the same construction as `hh-site-data/1` provenance) |
| `gate_row` | the manifest gate row id authorizing the publish: `GATE-G22` (phase-1), `GATE-G31` (first pass — phases 2–4), `GATE-G38` (phase-4 second pass) — a phase without a manifest gate cannot publish |
| `readout` | repo-relative path of the gate readout under `docs/build/`; the file must exist and name the gate. The operator's decision lives there — never copied into the record |
| `labels` | `{evidence_class, outside_verified: false}` — `outside_verified` stays `false` until a graduation record exists |
| `dataset` | `{visibility, revision, secret_scan_ref, contamination_scan_ref}` — while `private`, the three refs are `null`; `public` requires all three plus a superseding record (HUMAN-H17) |
| `recorded_at` | Ordinary record: exact filename date. A `-flip` record: string beginning with that date, retaining its flip-act annotation |
| `supersedes` | Optional for ordinary records, required for `-flip`; full publication path or basename of the earlier same-phase record. Basenames resolve inside this directory and use the same canonical identity in the archive graph |

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

## Hash-bound input relocations

A dated `hh-site-publication-relocations/1` manifest under `relocations/`
may resolve the recorded `reports/compute/` to `reports/l1/` move for exact
pin comparison. This policy extension requires operator review of the exact
manifest and implementation; it is not a new publication gate. Existing
CAP6.2 metadata formats remain authoritative: no second filename/date
correction system is introduced.

Each entry names an immutable original publication record by full path and
SHA-256, and lists exact same-name old/new input paths and the original pin
hash. That hash, the manifest hash and actual destination bytes must agree;
if the old file is present, its bytes must also be identical. The original
input digest is validated before comparison, and the unchanged exact pin
predicate still checks every current input and hash. Original record files,
signed references, dates, archive versions and archived input pins stay
unchanged. No gate, dataset, evidence class, measurement or pin hash can be
changed through this manifest.

Unsupported fields, duplicate JSON keys/targets/mappings, missing files,
wrong hashes, changed bytes and arbitrary path substitutions fail closed.
Other relocation kinds are unsupported. The original publication validator
continues to check filename/date, gate, readout, dataset, labels and predecessor
existence/phase. Basename predecessors are qualified for the archive graph,
using the same identity already accepted by that validator.

**Enforcement boundary:** runtime checks bind original record/packet bytes to
the reviewed manifest, but do not attest operator approval or enforce the
manifest's own Git history. The current external build-memory history guard
does not cover `docs/results/publication/`, and this repository has no history
policy extending it to relocation manifests. Their append-only handling
remains an operator/review requirement, not an automatically enforced history
gate. New corrections must not silently rewrite existing manifests or signed
records.

Run `python3 scripts/site/tests/test_publication_relocations.py` for positive,
rejection and actual-public-render checks. CI runs it alongside the unchanged
site mutation battery. Synthetic passes do not close the real-data coverage
deferral in the build ledger.
