# Publication gate checklist — `hh-site-publication/1`

One publishing phase → one operator decision → one committed record.
A `hh-site-publication/1` record is the *only* way a phase's pages reach
the public profile (REQ-XF-128/129, design/39 F10); `pages.yml` deploys
nothing without one, and the public build refuses a record whose pinned
input set no longer matches the named generator inputs.

The agent (or the publication tooling) prepares; the operator decides.
None of the rows below is an agent act (REQ-XF-130).

## Before the record

- [ ] The phase's manifest **gate row is `PASSED` by the operator** —
      GATE-G22 (phase-1) / GATE-G31 (phase-2 first-pass) — and the
      readout under `docs/build/readouts/` names the gate and carries
      the operator's verbatim decision. The record references the
      readout; it never copies it.
- [ ] `bash scripts/check-site.sh` and `--profile public` are green on
      the proposed input set.
- [ ] **HUMAN-H12** has enabled GitHub Pages (Source: GitHub Actions)
      — a human step; a record without it deploys to nowhere but is
      still honest.
- [ ] Every input in the phase's `inputs[]` is a named generator input
      (`scripts/site/config/inputs.json`) with a recomputed sha256;
      `inputs_digest` matches (the validator recomputes).
- [ ] `labels` carries `outside_verified: false` and an evidence class
      from the evidence vocabulary (a graduation ADR is the only path
      to flipping it — C-34/C-26).
- [ ] `dataset` states the *current* visibility. While it is `private`,
      `revision`, `secret_scan_ref`, and `contamination_scan_ref` are
      `null` and nothing under the phase links into the dataset (the
      no-third-party leg refuses).

## The dataset-visibility flip (HUMAN-H17)

The dataset's public/private transition is a **separate human act**, not
a side effect of a publication record (AC-9.6-6, REQ-XH-018):

- [ ] A fresh **secret scan** over the dataset revision (`secret_scan_ref`
      names the scan's record).
- [ ] A fresh **contamination scan** over the same revision
      (`contamination_scan_ref`).
- [ ] The **revision** (exact digest/commit of the published dataset) is
      pinned in `dataset.revision`.
- [ ] A subsequent publication record carries `visibility: "public"` —
      a record, not an operator note, changes the field (records are
      append-only; flip by supersede).

## After the record

- [ ] `bash scripts/check-publication.sh` green (well-formedness,
      manifest rows, append-only supersede).
- [ ] `python3 scripts/site/build.py --check` green — site-data is
      unaffected by publication records; only `--out --profile public`
      consumes them.
- [ ] The next push to `main` deploys: `pages.yml` builds the public
      profile, runs `check-site.sh --profile public`, and only then
      reaches the guarded deploy steps.
- [ ] The changelog and `/archive/<version>/` markers derive from the
      records (deterministic — never hand-edited).
