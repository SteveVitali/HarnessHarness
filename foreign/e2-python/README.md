# Python cross-implementation replay (E2)

This standard-library Python implementation independently replays the committed
HarnessHarness conformance corpus. It reads the
[`hh-xcheck-bundle/1` fixtures](../../crates/hh-xcheck/fixtures/cross-impl/), writes
an answer file, and lets the Rust `hh-xcheck` comparator check its digests and
verdicts. It covers **eight arms and 57 checked answers**. It is outside the Rust
workspace and is not a runtime dependency of the kernel.

## Run

From the repository root, with Python 3.10 or newer and the pinned Rust toolchain:

```sh
python3 foreign/e2-python/foreign.py crates/hh-xcheck/fixtures/cross-impl /tmp/answers-e2.json
cargo run -p hharness-xcheck -- verify crates/hh-xcheck/fixtures/cross-impl /tmp/answers-e2.json
```

The comparator's final line should be `verify: 57 passed, 0 failed`, with exit
code 0. A missing or different answer is a failure. The output path above is
scratch space; [answers.json](answers.json) is the committed R4.16a evidence.
No Python packages need installing: [foreign.py](foreign.py) imports only
`copy`, `functools`, `hashlib`, `json`, `pathlib`, `sys`, and `time`.

To run the Rust self-check and both foreign implementations together, also
install the [TypeScript replay's Node prerequisite](../e3-typescript/README.md#toolchain):

```sh
cargo build -p hharness-xcheck
bash scripts/xcheck-foreign.sh
```

## Coverage and limits

The [checker contract](../../crates/hh-xcheck/CHECKER.md) defines the byte encoding,
identity functions, expected answers, and comparison rules. The implementation
covers:

| Arm | Replay |
|---|---|
| `identity-trees` | Canonical, content-addressed directory trees |
| `identity-records` | Blob, text, version, and semantic identities |
| `ledger-transcript` | Event hashes and head hash from the write-ahead log |
| `registry-snapshot` | Registry replay and the fixed resolution/query vector |
| `plugin-manifests` | Decode and admission verdicts under the bundle's posture |
| `canonical-parse` | Canonical JSON parse/re-emit digests; timings are informational |
| `hir-documents` | Parse, canonicalize, validate, and seal the five HIR cases |
| `ledger-scenarios` | The shared WAL fold over three additional scenarios |

This is a corpus implementation, not a complete Python port of the runtime.
In particular, HIR cases reaching unported kinds, edges, `assumption_debt`, or
`flow_contract` are omitted; the comparator reports a missing answer. The HIR
and ledger-scenario arms were added at R4.16a using the E3 reference semantics
and this tree's own canonical JSON and identity primitives.

A successful run earns `foreign-run-verified` evidence for the tested corpus.
It does not establish general conformance, live-service operation, or release
readiness. The bundle retains `environment-pending` cells for HUMAN-H3 and
records replay evidence in `verified_by[]`; those are distinct from missing
answers. See [the evidence policy](../../MAINTAINING.md#evidence-classes).

## Historical implementation notes

The original six-arm observations and dated notes below are preserved as build
evidence. Their 49-answer count and descriptions of missing extension arms
refer to those earlier states. **Current resolution:** CHECKER.md specifies
`idp_id("hir.text", bytes)` with verbatim content, the Merkelized tree document,
and the fixed registry request vector. R4.16a added both extension arms and
recorded 57/57 answers. The original primitives were written from the contract
and fixtures; Rust sources were consulted for fixture shapes and error names.

### Original six-arm observations

- **`identify_text` (spec-text defect, resolved in E1's favor).**
  `CHECKER.md §idp/1` spells `idp_id("text", normalized)` where `normalized`
  rewrites `\r\n`/lone `\r` to `\n`. The committed `text-leaf` expectation
  instead pins `idp_id("hir.text", bytes)` over **verbatim** bytes — the E1
  semantic (`RecordKind::HirTextLeaf`'s domain tag; no CRLF rewrite exists
  on the reference path). This implementation follows the bundle's
  evidence. The normalization axis is unexercised by the committed corpus
  (no input carries a `\r` byte), so the divergence is recorded as a
  spec-text defect: `CHECKER.md`'s `identify_text` prose does not describe
  the function E1 ships. *Reported, not patched.*
- **`tree/1` canonical document.** `CHECKER.md` describes the tree document
  in the manifest's own `bytes_hex`/`entries` spelling, then says the
  `.canonical.json` members carry the implementation's rendering. The
  committed bytes show the canonical document is the Merkelized form —
  `{"content": <blob-id>}` for files, `{"tree": <subtree-id>}` for dirs.
  Implemented per the committed bytes (the `id` fold pins it).
- **Registry-snapshot request vector.** `CHECKER.md` says the scenario
  enumerates "the selectors and `query`/`slot_choices`"; `scenario.json`
  carries only `snapshot_id` + `variant_version_ids`. The six selector
  names, the `kind=variant` query, and the three `slot_choices` class ids
  (`control_strategy`, `context_policy`, `export_sink`) are fixed corpus
  contract, implemented as constants here.
- **`name_status` on query rows** is the status of the *first* name-history
  entry binding a version (entry-axis status, not name-head status — a
  yanked head does not retro-mark the earlier publish entry).
- **`summary` provenance** is shape-checked (Text leaf: `content_hash`,
  `owner`, `authority`, `provenance`), not the full `ProvenanceRecord`
  validation; the `authority ≤ external` gate is the admission-relevant
  leg.

### Original six-arm result

`verify` — **49 passed, 0 failed** (exit 0). `selfcheck` — 55 passed, 0
failed. All six `*.foreign-*` bundle cells' evidence now exists; whether
the ledger rows flip from `environment-pending` is a record-keeping call
for the operator (the cells name the HUMAN-H3 toolchain leg this run is
evidence for).

### Dated note — 2026-10-21 (R4.4b)

The R4.4b ticket (`docs/tickets/153b_R4.4b__e3-typescript-foreign-impl-part3.md`)
delivered the E3-candidate `hir-documents` arm. This tree was checked for
an existing `hir-documents` arm to port from: there is none — `foreign.py`
still carries exactly its original six `arm_*` functions (the only `hir`
spelling in the tree is the `hir.text`/`hir_dialect` identity surface the
identity-records arm already used), so nothing here was ported. Per the
ticket's boundary, the e2 tree is not edited to grow the arm — an e2
extension, if wanted, is a separate operator-visible change — and the
bundle's `hir-documents.e2-replay` cell stays `environment-pending`
(its `needs` names the e2-tree arm extension). This note records the
absence; `foreign.py` itself is unchanged.

### Dated note — 2026-10-22 (CF-493 / R4.15c)

`bundle.json` now carries the CF-493 `verified_by[]` member on every
`arms[]` and `pending_cells[]` entry: `{candidate, evidence_class,
evidence_ref}` triples naming the candidates whose recorded evidence
covers the arm or partially discharges the cell. The six original arms
and their six `*.foreign-*` cells carry this tree's entry —
`{candidate: "E2-candidate", evidence_class: "foreign-run-verified",
evidence_ref: "foreign/e2-python — PR #136 (49/49)"}`. The honesty
ceiling is unchanged: `foreign-run-verified` stays a distinct class
below the in-tree `fixture-verified` ceiling, and a pending cell's
`verified_by[]` is partial discharge only — `status` stays
`environment-pending` while its `needs` prerequisite stands. The R4.3
extension arms still carry no E2 entry — this tree cannot answer them;
their `*.e2-replay` cells' `verified_by[]` is empty and their `needs`
still names `e2-tree arm extension`.

### Dated note — 2026-10-23 (R4.16a)

The two R4.3 extension arms now land on this tree — the dated notes above
recording their absence are history, superseded here (never edited).

`foreign.py` carries eight `arm_*` functions: the six originals plus
`arm_hir_documents` — a corpus-faithful `parse → canonicalize → validate →
seal` port of the e3 reference semantics (`foreign/e3-typescript/lib/hir.ts`)
on this file's own canonical-JSON/`idp_id`/sha256 primitives — and
`arm_ledger_scenarios`, which rides the shared `_replay_wal` fold with
`arm_ledger_transcript` (the e3 `replayWal` refactor's shape, CC1). Cases
reaching corpus-unported machinery (the eight unexercised node kinds, edges,
`assumption_debt`, `flow_contract`) are omitted honestly — `verify` reports
`missing answer`, never a fabricated verdict.

The regenerated committed `answers.json` verifies **`hh-xcheck verify`
57/0** over all eight arms (`foreign-run-verified` — the distinct class
below `fixture-verified`, never conflated); `scripts/xcheck-foreign.sh`
runs E1 97/0 + E2 57/0 + E3 57/0 green. The `hir-documents.e2-replay` /
`ledger-scenarios.e2-replay` pending cells now carry the E2
`verified_by[]` record — `needs` is `HUMAN-H3` like every foreign cell
(the `e2-tree arm extension` prerequisite is met); `status` stays
`environment-pending`.
