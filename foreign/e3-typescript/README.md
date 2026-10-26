# E3-candidate — a foreign `hh-xcheck` replay implementation (TypeScript, node stdlib)

A **foreign implementation candidate** for the HUMAN-H3 environment leg of
the cross-implementation replay contract (spec §10.7, R2.21, ADR-0353).
It consumes the committed `hh-xcheck-bundle/1` corpus at
`crates/hh-xcheck/fixtures/cross-impl/` and emits `answers.json`, which
`hh-xcheck verify` diffs against the bundle's E1 expectations.

**Part 1 (R4.4)** landed the shared `lib/` core plus the
identity-trees, identity-records, ledger-transcript, registry-snapshot
and canonical-parse arms. **Part 2 (R4.4a)** wired the plugin-manifests
and ledger-scenarios arms against the same core — `lib/` is the one
spelling; it is extended, never re-implemented. **Part 3 (R4.4b**
— `docs/tickets/153b_R4.4b__e3-typescript-foreign-impl-part3.md`)
landed the eighth and final arm, `hir-documents` (`lib/hir.ts`):
a corpus-faithful HIR/1 decode → canonicalize → validate → seal
pipeline reproducing `crates/hh-xcheck/src/hir_replay.rs`'s answer
contract.

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
cargo run -p hharness-xcheck -- verify crates/hh-xcheck/fixtures/cross-impl /tmp/answers-e3.json
# or one shot: bash scripts/xcheck-e3.sh [--answers PATH]
```

`answers.json` here is the run's committed output
(`candidate: "E3-candidate"`).

## Recorded verify output (R4.4b scope, verbatim)

```
candidate: E3-candidate
ok    verify identity-trees binary-content … whitespace-names  (×22 cases — all ok)
ok    verify identity-records blob … text-leaf                 (×4 cases — all ok)
ok    verify ledger-transcript event_hashes
ok    verify ledger-transcript head_hash
ok    verify registry-snapshot outputs_sha256
ok    verify plugin-manifests invalid/claim-not-publisher.json … valid/third-party-subprocess.json  (×14 cases — all ok)
ok    verify canonical-parse frame-16k-0 … frame-64k-1           (×6 cases — all ok)
ok    verify hir-documents gated-tool-no-contract
ok    verify hir-documents hosted-agent
ok    verify hir-documents minimal-native
ok    verify hir-documents missing-provenance
ok    verify hir-documents unresolved-ref
ok    verify ledger-scenarios blob-refs
ok    verify ledger-scenarios causal-chain
ok    verify ledger-scenarios writer-handover
verify: 57 passed, 0 failed
```

Per-arm: **identity-trees 22/22 · identity-records 4/4 ·
ledger-transcript 2/2 · registry-snapshot 1/1 · plugin-manifests 14/14 ·
canonical-parse 6/6 · ledger-scenarios 3/3 · hir-documents 5/5 =
57/57 cases green.** `hh-xcheck verify` exits 0 on this answers file;
`scripts/xcheck-e3.sh` reports `covered-arm failures: 0; pending (R4.4b)
cells missing: 0` — all eight arms answered, zero pending cells.

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
8. **Plugin manifests** (`lib/plugin.ts`, R4.4a) — the strict
   `PluginManifest/1` decode (closed member set, pinned `records[]`
   `{version_id, kind, ext?}` shape checks, no inline bodies, no
   package-escaping paths, `code-pointer` pins, `fetched_at` uint,
   all-`v`-stripped versions, shorter-prefix-is-less `versionCmp`,
   first-`-` range split, empty tier digits refused) then the admission
   order decode → namespace → locality → tier → compatibility →
   `requests ⊆ cap` under the bundle
   posture (`first_party=true`, deny-everything requests cap). Verdicts
   spell `admit:<id>` / `refuse:<Kind>`; the class catalog comes from
   `RegistryReplay.classCatalog()` — the same `registry.json` fold the
   snapshot arm ships (CC1, never a second registry read).
9. **Ledger scenarios** (`lib/ledger.ts` `armLedgerScenarios`, R4.4a) —
   the R4.3 WAL corpus (`causal-chain`, `blob-refs`, `writer-handover`)
   replayed through the one `replayWal` fold per case dir; answers carry
   `<case>` `{event_hashes, head_hash}` plus the informational
   `<case>.wal_sha256` / `<case>.event_count` members `verify` skips.
10. **HIR/1 documents** (`lib/hir.ts`, R4.4b) — the
    `hir_replay.rs` pipeline **parse → canonicalize → validate → seal**
    (the empirical E1 order) over the five committed legs, sealed at the
    scenario's fixed `sealed_at = 1700000000`. The port decodes and
    re-emits byte-exactly: canonical-JSON admission
    (`NonCanonicalInput`), the `ProvenanceRecord` codec
    (`MissingProvenance`, origin/authority/scope/taint/readers/
    derivation/attestation members), `Ref`/`ProfileRef`/
    `ComponentVariantRef`, the five exercised kind bodies —
    `AgentProcess` (native + hosted), `Budget`, `HarnessRule`,
    `Permission`, `ToolCapability` — slots/boundary/capability-
    declaration/supplies/rule-action/validity/issuer/grant/effect/
    resource/scope-binding members, `ToolSurface`, and the
    node/document envelope. Validation covers root+ref resolution
    (`UnresolvedRef` — the `unresolved-ref` leg), provenance ceilings,
    `Text` leaf authority, budget registry/parent containment,
    hosted-agent `version_identity` recomputation, and the capability
    member battery (`tools::validate_capability`); seal performs the
    definition-authority endorsement (`hir:seal` attestation), ref
    pinning, hosted `version_identity` minting, and emits the
    `definition_version_id` — `gated-tool-no-contract` honestly refuses
    `ContributionUndeclared`. Errors join first-raised,
    duplicate-collapsed `HirError::kind_str` spellings; `skipped` gating
    matches the driver. **Honest bound:** a case reaching surface the
    corpus does not exercise — an unexercised registered kind, any edge,
    `assumption_debt`, or `flow_contract` — is *omitted* (the bundle's
    `missing answer` signal), never a fabricated verdict.

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

`verify` over the eight-arm answers — **57 passed, 0 failed** (exit 0):
every arm green including all five `hir-documents` cases. `xcheck-e3`
end-to-end: `covered-arm failures: 0; pending (R4.4b) cells missing: 0`
→ `ok`.

All eight arms' E3-leg cells now carry `foreign-run-verified` evidence
(the R4.4 five `*.foreign-replay`/`foreign-timing` cells, R4.4a's
`plugin-manifests.foreign-checker` and `ledger-scenarios.e3-replay`,
plus R4.4b's `hir-documents.e3-replay`). Whether `bundle.json`'s
`pending_cells[]` rows flip is a record-keeping call for the
operator/GATE-G7 vocabulary ruling (CF-493) — the cells name the
HUMAN-H3 toolchain leg this run is evidence for, and the cell vocabulary
stays as recorded.

**e2-side record (R4.4a, verbatim):** the committed
`foreign/e2-python/foreign.py` carries exactly the six original `arm_*`
functions — `arm_ledger_transcript` is the only WAL consumer; there is
no `arm_ledger_scenarios`/`arm_hir_documents`. The e2 tree cannot answer
R4.3's extension arms today (`*.e2-replay` cells record
`needs: "e2-tree arm extension"`); an e2-tree extension is its own
operator-visible change — the tree is not edited here.

**e2-side record (R4.4b, 2026-10-21):** the e2 tree was checked for an
existing `hir-documents` arm to port from — there is still none (the
dated note appended to `foreign/e2-python/README.md` records the
absence); `hir-documents.e2-replay` stays `environment-pending` and the
e2 tree is unchanged apart from that note.

### Dated note — 2026-10-22 (CF-493 / R4.15c)

CF-493 landed the `verified_by[]` member it named: every `bundle.json`
`arms[]` and `pending_cells[]` entry now carries
`{candidate, evidence_class, evidence_ref}` records. This tree's entry —
`{candidate: "E3-candidate", evidence_class: "foreign-run-verified",
evidence_ref: "foreign/e3-typescript — R4.4b (57/57)"}` — appears on all
eight arms and on the six original `*.foreign-*` cells plus
`hir-documents.e3-replay` / `ledger-scenarios.e3-replay`. The honesty
vocabulary is unchanged: `status` stays `environment-pending` — a
pending cell's `verified_by[]` records partial discharge, never the
cell's closure; the HUMAN-H3 `needs` prerequisite still stands.
`foreign-run-verified` remains a class below `fixture-verified`, and
nothing here is `staging-verified`, `live-executed` or `public`.
