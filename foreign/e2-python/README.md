# E2-candidate — a foreign `hh-xcheck` replay implementation (Python stdlib)

A **foreign implementation candidate** for the HUMAN-H3 environment leg of
the cross-implementation replay contract (spec §10.7, R2.21, ADR-0353).
It consumes the committed `hh-xcheck-bundle/1` corpus at
`crates/hh-xcheck/fixtures/cross-impl/` and emits `answers.json`, which
`hh-xcheck verify` diffs against the bundle's E1 expectations.

This tree is **outside the cargo workspace** (`foreign/` has no
`Cargo.toml`; the workspace's `members` list is explicit `crates/*` paths).
`cargo metadata` reports the unchanged 51 members and `cargo build
--workspace` is unaffected.

## Honesty note (read first)

This is a *candidate* implementation, not E1 truth. The answers it emits
are evidence **this run** produced — `verify` only ever checks; nothing in
the bundle mints a claim for it. A green `verify` is real evidence a
foreign implementation replayed the corpus byte-for-byte; it is not a new
reference.

The primitives were implemented from `CHECKER.md`'s spec text and the
committed fixture shapes. Rust sources were read to understand fixture
shapes and the error-kind vocabulary `CHECKER.md` itself defers to
(`hh-registry::extension::plugin`, §Plugin-manifest verdicts); no Rust
primitive was transliterated.

## Run

```sh
python3 foreign/e2-python/foreign.py crates/hh-xcheck/fixtures/cross-impl /tmp/answers-e2.json
cargo run -p hharness-xcheck -- verify crates/hh-xcheck/fixtures/cross-impl /tmp/answers-e2.json
# → verify: 49 passed, 0 failed   (exit 0)
```

`foreign.py` is Python 3 stdlib only (`hashlib`, `json`, `pathlib`, `sys`,
`time`). `answers.json` here is the run's committed output
(`candidate: "E2-candidate"`).

## What it implements

1. **`hh-json/1` canonical JSON** — byte-sorted keys, `,`/`:` separators,
   integers only, `\uXXXX` lowercase-hex escapes for U+0000–U+001F plus
   `\"` `\\` `\n` `\r` `\t`; everything else verbatim UTF-8. A parse
   rejects floats and unpaired surrogates (outside the dialect — refused,
   never silently coerced).
2. **`idp/1`** — `sha256("idp/1" ‖ 0x1f ‖ tag ‖ 0x1f ‖ payload)`,
   lowercase-hex, `sha256:` prefix.
3. **`tree/1`** — manifest → canonical document → `idp_id("tree", …)`. See
   *Spec ambiguities* below: the canonical document is the **Merkelized**
   form of the manifest (file `bytes_hex` → `content` blob-address; dir
   `entries` → `tree` subtree-id), per the committed `*.canonical.json`
   bytes.
4. **Ledger WAL replay** — `hash = idp_id("ledger.event", 0x00 ‖
   canonical(envelope − {"hash"}) ‖ prev_hash_utf8)`, `prev = "genesis"`;
   every `e`-frame's recorded hash is recomputed and compared.
5. **`PluginManifest/1` + registry snapshot fold** — strict manifest decode
   (unknown members, inline bodies, package-escaping paths, unpinned
   pointers, `publisher_claim`-only conformance claims, `summary.authority
   ≤ external`), then the admission order namespace → locality → tier →
   compatibility → `requests ⊆ cap` under the bundle posture
   (`first_party=true`, deny-everything cap). The registry arm replays
   `registry.json` into the record/name/revocation/stale projections and
   folds the corpus output vector (Audit resolves, snapshot-pinned Execute
   resolves, the `kind=variant` query, `slot_choices` per class) to the
   `expected-outputs.json` bytes; the answer is their SHA-256.

The plugin arm's store is the deterministic registry corpus — the same log
the `registry-snapshot` arm ships; only the class catalog
(`class_id → contract_version, tier`) is read from it.

## Spec ambiguities / deviations found

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

## Result

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
