# E3-candidate — a foreign `hh-xcheck` replay implementation (TypeScript, node stdlib)

A **foreign implementation candidate** for the HUMAN-H3 environment leg of
the cross-implementation replay contract (spec §10.7, R2.21, ADR-0353).
It consumes the committed `hh-xcheck-bundle/1` corpus at
`crates/hh-xcheck/fixtures/cross-impl/` and emits `answers.json`, which
`hh-xcheck verify` diffs against the bundle's E1 expectations.

This is **part 1 (R4.4)**: the shared `lib/` core plus the
identity-trees, identity-records, ledger-transcript, registry-snapshot
and canonical-parse arms. R4.4a completes the remaining arms
(plugin-manifests, hir-documents, ledger-scenarios) against the same core
— `lib/` is the one spelling; it is extended, never re-implemented.

This tree is **outside the cargo workspace** (`foreign/` has no
`Cargo.toml`; the workspace's `members` list is explicit `crates/*`
paths). `cargo metadata` reports the unchanged 51 members and `cargo
build --workspace` is unaffected.

## Honesty note (read first)

This is a *candidate* implementation, not E1 truth. The answers it emits
are evidence **this run** produced — `verify` only ever checks; nothing
in the bundle mints a claim for it. A green `verify` is real evidence a
foreign implementation replayed the corpus byte-for-byte; it is not a new
reference.

The evidence class is **`foreign-run-verified`** — a foreign toolchain's
recorded replay of the committed `hh-xcheck` bundle (the PR #136 E2
precedent) — a distinct class **below** the in-tree fixture-verified
ceiling, never conflated with it. Nothing here is `staging-verified`,
`live-executed` or `public`.

The primitives were implemented from `CHECKER.md`'s spec text and the
committed fixture shapes, with `foreign/e2-python` read as the reference
structure. No Rust code was transliterated.

## Toolchain

- **node ≥ 22** (recorded on node `v25.2.1`). Run directly under node's
  type stripping — `.ts` imports, type annotations and `import type`
  only. **Erasable TypeScript only**: no enums, no namespaces, no
  parameter properties, no decorators (node's strip-types rejects them at
  run time — the replay leg in `crates/hh-xcheck/tests/r4_4.rs` is the
  check that fails if one lands).
- **Stdlib only**: `node:crypto`, `node:fs`, `node:path`, `node:process`,
  `node:buffer`. No `package.json`, no `node_modules`, no build step —
  the `e3_tree_carries_no_npm_dependency` test asserts it.

## Run

```sh
node foreign/e3-typescript/foreign.ts crates/hh-xcheck/fixtures/cross-impl /tmp/answers-e3.json
cargo run -p hh-xcheck -- verify crates/hh-xcheck/fixtures/cross-impl /tmp/answers-e3.json
# or one shot: bash scripts/xcheck-e3.sh [--answers PATH]
```

`answers.json` here is the run's committed output
(`candidate: "E3-candidate"`).

## Recorded verify output (part-1 scope, verbatim)

```
candidate: E3-candidate
ok    verify identity-trees binary-content        (×22 cases — all ok)
ok    verify identity-records blob … text-leaf    (×4 cases — all ok)
ok    verify ledger-transcript event_hashes
ok    verify ledger-transcript head_hash
ok    verify registry-snapshot outputs_sha256
ok    verify canonical-parse frame-16k-0 … frame-64k-1  (×6 cases — all ok)
FAIL  verify plugin-manifests <14 cases> — missing answer
FAIL  verify hir-documents <5 cases> — missing answer
FAIL  verify ledger-scenarios <3 cases> — missing answer
verify: 35 passed, 22 failed
```

Per-arm: **identity-trees 22/22 · identity-records 4/4 ·
ledger-transcript 2/2 · registry-snapshot 1/1 · canonical-parse 6/6 =
35/35 covered cases green.** The 22 `missing answer` FAILs are exactly
the arms R4.4a owns — the E3 tree answers nothing it did not replay
(honest pending, never a fabricated or vacuous pass). `hh-xcheck verify`
exits 1 on this part-1 answers file by design; `scripts/xcheck-e3.sh`
reconciles: exit 0 iff every FAIL is a `missing answer` on an arm the
tree did not answer.

## What it implements

1. **`hh-json/1` canonical JSON** (`lib/core.ts`) — byte-sorted keys
   (UTF-8 order), `,`/`:` separators, integers only, `\uXXXX`
   lowercase-hex escapes for U+0000–U+001F plus `\"` `\\` `\n` `\r` `\t`;
   everything else verbatim UTF-8. A parse refuses non-integer numbers
   and unpaired surrogates (outside the dialect — refused, never
   silently coerced).
2. **`idp/1`** — `sha256("idp/1" ‖ 0x1f ‖ tag ‖ 0x1f ‖ payload)` via
   `node:crypto`, lowercase-hex, `sha256:` prefix.
3. **`tree/1`** (`lib/trees.ts`) — manifest → Merkelized canonical
   document → `idp_id("tree", …)` (file `bytes_hex` → `content`
   blob-address; dir `entries` → `tree` subtree-id), per the committed
   `*.canonical.json` bytes.
4. **Identity records** (`lib/records.ts`) — `address_blob`,
   `identify_text`, and the `record` form's
   `version_id`/`semantic_id` fold.
5. **Ledger WAL replay** (`lib/ledger.ts`) — `hash = idp_id("ledger.event",
   0x00 ‖ canonical(envelope − {"hash"}) ‖ prev_hash_utf8)`,
   `prev = "genesis"`; every `e`-frame's recorded hash is recomputed and
   compared; frames must be canonical JSON (a drifted frame fails,
   never silently reserialized). `replayWal` is the one fold R4.4a's
   `ledger-scenarios` reuses per case dir.
6. **Registry snapshot fold** (`lib/registry.ts`) — `registry.json`
   replayed into the record/name/revocation/stale-by-dependency
   projections; the fixed corpus output vector (Audit resolves, six
   snapshot-pinned Execute selectors, `kind = variant` query,
   `slot_choices` for `control_strategy`/`context_policy`/`export_sink`
   under `contract_version: "1.0"`) folded to the
   `expected-outputs.json` bytes; the answer is their SHA-256.
7. **Canonical-parse** (`lib/canonical_parse.ts`) — decode + re-emit +
   digest per frame; `parse_ns`/`rehash_ns` recorded via
   `process.hrtime.bigint()`, never gated.

## Spec ambiguities / discrepancies observed

- **`identify_text` — resolved in the contract text.** E2 recorded a
  spec-text defect: an earlier CHECKER.md spelled `idp_id("text",
  normalized)` with a `\r\n`/`\r` → `\n` rewrite while the committed
  expectation pinned `idp_id("hir.text", bytes)` verbatim. The current
  CHECKER.md §idp/1 itself spells `idp_id("hir.text", bytes)` over
  verbatim content bytes and notes the corpus exercises no `\r` — the
  contract text was amended to the E1 semantic. E3 implements
  `hir.text`/verbatim: **no deviation remains to record.**
- **`tree/1` canonical document** — same resolution path as E2: the
  canonical document is the Merkelized form of the manifest (committed
  `*.canonical.json` bytes pin it; the `id` fold agrees).
- **Registry-snapshot request vector** — `scenario.json` carries only
  `snapshot_id` + `variant_version_ids`; the six selectors, the
  `kind=variant` query and the three `slot_choices` class ids are fixed
  corpus contract (constants in `lib/registry.ts`), matching E2's read.
- **`name_status` on query rows** — the first name-history entry binding
  a version (entry-axis status), matching E2's read.
- No new spec-text discrepancies were found implementing the part-1
  arms; no answer was patched to force a pass.

## Result

`verify` over the part-1 answers — **35 passed, 22 failed** (exit 1):
every covered case green; the 22 failures are the R4.4a arms' cells
failing honestly as `missing answer` (`plugin-manifests` 14,
`hir-documents` 5, `ledger-scenarios` 3). `xcheck-e3` end-to-end:
`covered-arm failures: 0; pending (R4.4a) cells missing: 22` → `ok`.

The five `*.foreign-replay`-class bundle cells this run discharges
(identity-trees, identity-records, ledger-transcript, registry-snapshot
answers + the canonical-parse replay) have `foreign-run-verified`
evidence for the E3 leg; whether `bundle.json`'s `pending_cells[]` rows
flip is a record-keeping call for the operator/GATE-G7 vocabulary ruling
(CF-493) — the cells name the HUMAN-H3 toolchain leg this run is
evidence for, and the cell vocabulary stays as recorded.
