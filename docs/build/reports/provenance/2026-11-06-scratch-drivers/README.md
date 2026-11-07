# 2026-11-06 scratch-driver archive — historical provenance, unmaintained

Evidence class: recorded execution — historical provenance; never rebuilt;
internal record, not outside-verified.

This directory preserves, byte-identical and unmaintained, the 71 scratch
drivers, inputs and logs that produced the phase-1 parity-tranche and S4
cohort packets, together with the raw per-slot/per-pair record set those
packets were folded from (`raw/`). It is committed so the packets are
recomputable **records-in** (spec §2.2, REQ-XA-101…106); nothing here runs.

- `files/` — the 71 archive files, byte-identical to the operator-disk
  archive, same relative paths.
- `SHA256SUMS` — the archive manifest, byte-identical to the operator-disk
  manifest. Its own sha256 is
  `f80859977e5a276603f0cedfb2c04c47193bddf577c8d27e5c9f355c03a863e5` (71 lines).
  Verify: `cd files && shasum -a 256 -c ../SHA256SUMS` → 71/71 `OK`.
- `index.json` — `hh-scratch-driver-archive/1`: per-file digests, sizes,
  language, role, what each file produced, version coverage and why it does
  not compile in-repo.
- `raw/` — the committed raw-record set (`hh-p1-raw-records/1`):
  520 S4 per-slot JSONs (`s4-confirm` 300 / `s4-egress` 140 /
  `s4-completion` 50 / `s4-repairs` 30), tranche-3 O-side
  `results.json`/`run_metadata.json` (36 files across 5 run dirs) and the 31
  tranche-3 H-side records — 587 files, ~4.8 MB, with `raw/SHA256SUMS`.
  Bulk artifacts (build records, casts, logs, mirrors) and the 80-task
  dataset are content-addressed only in `raw/index.json:not_committed` and
  are labelled not committed.
- `dataset-digests.json` — `hh-dataset-digests/1`: `sha256tree/1` content
  digests of the 30-task tranche set and the 80-task catalogue, recorded
  **beside** (never replacing) the manifest's FNV-1a `catalogue_identity`
  `sha256:bd18094423eaca3c`. Task content is never committed (the dataset
  carries a training-corpus canary).
- `extra/gce-backend.md` + `extra/SHA256SUMS` — the operator's GCE backend
  record the completion/repairs epochs ran on; outside the 71-file
  manifest. The forward compute recipe is owned by the compute-backend
  design, not by this archive.

## Why these files are neither built nor workspace members

- **Not buildable as committed.** The three Rust crates
  (`parity-driver/`, `s4-cell/`, `s4-helper-egress/`) are standalone crates
  whose `[dependencies]` are absolute path-deps into a shared, moving
  checkout (`/Users/…/MetaHarness-harnessharness/crates/*` — A1 F15).
  Building them would require editing them, and an edited copy would no
  longer be the provenance. The Python and bash drivers bake absolute
  dataset, tool-venv, socket and work-dir paths the same way; the archive
  is preserved verbatim, absolute paths included (ruling S-2).
- **Final-only versions.** For `s4-cell` and `parity-h-side.py` (and the
  helper variant/orchestrators patched in place), only the last patched
  sources survive — earlier versions that produced earlier epochs are lost
  (A1 F15); `index.json:version_coverage` marks them `final-only`.
- **Outside the workspace.** They are not Cargo workspace members and
  nothing under `docs/` is walked by Cargo or CI; `cargo metadata`'s
  member set is unchanged and no CI job references this tree
  (REQ-XA-102).
- **Superseded.** The committed runner of experiments-spec §3.2 is the
  maintained path for every future execution. These drivers are kept only
  as provenance for the records they produced.

## What each file produced

`index.json` carries `role` and `produced` per file. In short:
`parity-driver/` materialized the three seeded manifests;
`parity-h-side.py` was the H-side reference podman runner (never HH) for
tranches 1–3; `s4-cell/` + `s4-*-orch.py` drove the four S4 epochs' slots
through HH cells; `s4-helper-egress/` was the egress epoch's patched
helper variant; `s4-*-assemble.py` folded the raw slot records into the
four committed packets; the `*.log`/`*.err`/pairs/batch files are hashed
run evidence and driver inputs.

## Historical re-run recipe (not maintained)

A re-run is a **fresh execution, never replay**: the products are new
packets with new slot refs and timestamps, never byte-identical copies of
the committed ones.

Prerequisites:

- podman with an applehv machine (the recorded machine name `podman`);
- `uv tool install terminal-bench==0.2.18` (the version found in the local
  tool environment — the drivers import
  `terminal_bench.parsers.pytest_parser` from the uv tool venv);
- the local TB-core dataset copy, identified only by
  `dataset-digests.json` (`run_metadata.json` records
  `dataset_name: null`, `commit_hash: unknown`).

Steps:

1. Check out the HH commit nearest the epoch being re-run (dated table
   below; the original drivers path-depended on a floating shared
   worktree, so these are nearest-recorded commits, not recorded pins).
2. Copy each needed crate out of `files/` and rewrite its `Cargo.toml`
   path-deps to that checkout — **this is the declared non-verbatim
   edit**: the only sanctioned deviation from the archived bytes.
3. Re-supply the hard-coded inputs verbatim (list below) at the paths the
   drivers expect, or patch the constants — each patch is a new deviation
   to record.
4. Run the documented orchestrator argv from the epoch's run ledger; the
   two GCE epochs additionally require the provisioning gate.

Hard-coded inputs a re-run must supply or patch (A1 §5 inventory):

- `parity-driver/`: quotas 5/16/9, the arms/lanes/phases declared in code,
  `exclusion_publication_ref` fixed to `PARITY-TRANCHE.md`, and the
  FNV-1a catalogue identity printed with a `sha256:` prefix.
- Batch inputs (`parity-tasks.txt`, `parity-taskids*.txt`,
  `parity-batch-*.txt`, `parity-t3-b*.txt`): generated from the local
  dataset copy.
- `parity-h-side.py`: the dataset path, the terminal-bench uv tool venv
  path, its output directory, default podman networking; only the final
  (t3) version is archived — the t1/t2 driver versions are lost.
- `parity-t3-pipeline.sh` / `parity-t3-diskwatch.sh`:
  `DOCKER_HOST=unix:///tmp/podman-api/podman.sock`, machine name
  `podman`, `/tmp` log paths.
- `s4-cell/`: `window_cap: 400` and `occupancy > 400`, the `cd /app`
  exec default (the declared-workdir defect the repairs epoch patched),
  `gcloud compute ssh` work-dir sync; only the final patched version is
  archived — the confirmation/egress-era sources are not recoverable.
- `s4-helper-egress/`: `S4_HELPER_NETWORK` / argv network selection.
- `s4-*-orch.py`: `HH_HELPER_BIN` pointing at a locally built
  `hh-helper`, the frozen `/tmp/parity-manifest-v3.json`, the prior
  packet read from the checkout's `docs/`, the dataset/`s4-*`/`vendored`
  work dirs, the GCE socket `/tmp/podman-gce/podman.sock`, and the SSH
  forward recipe + port (`extra/gce-backend.md`).
- `s4-*-assemble.py`: the `REPO`/`OUT` constants writing the packet into
  the checkout, and the epoch's `slots/` dir.

Per-epoch HH commits (nearest recorded — the path-deps floated):

| Epoch | Nearest recorded HH commit |
|---|---|
| parity tranches t1–t3, S4 confirmation, S4 egress | `0b17e266` (the packets' own landing commit, 2026-11-06) |
| S4 completion (GCE) | `7e70b0dc` — requires R5.56 `90fde635` (`declared_deps` bridge) and R5.57 `814b3d50` (verifier bound) machinery |
| S4 30-slot repairs (GCE) | `75883daa` |

Provisioning gate: the local applehv epochs need only podman. The
completion and repairs epochs additionally need the GCE recipe in
`extra/gce-backend.md` — an operator-authorized provisioning + spend gate
of the same class R5.58 ran under, with teardown. No re-run may provision
without that gate.

## Verification

```sh
cd docs/build/reports/provenance/2026-11-06-scratch-drivers
(cd files && shasum -a 256 -c ../SHA256SUMS)   # 71/71 OK
(cd raw && shasum -a 256 -c SHA256SUMS)        # 587/587 OK
python3 -I docs/build/tools/p1_records.py --check   # RC-1…RC-6
```
