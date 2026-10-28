# TypeScript cross-implementation replay (E3)

This implementation replays the committed HarnessHarness conformance corpus
using TypeScript and Node's standard library. It reads the
[`hh-xcheck-bundle/1` fixtures](../../crates/hh-xcheck/fixtures/cross-impl/), writes
an answer file, and checks against the Rust reference through `hh-xcheck`.
All **eight arms and 57 checked answers** are implemented. This tree is outside
the Cargo workspace and is not a kernel runtime dependency.

## Toolchain

Use **Node 22.18 or newer in the 22.x series, or Node 24 or newer**, plus the
repository's pinned Rust toolchain for the comparator. These Node versions run
erasable TypeScript directly; no npm packages, `package.json`, or build step
are needed. Node's [TypeScript documentation](https://nodejs.org/api/typescript.html#type-stripping)
explains the built-in type stripping. The recorded E3 run used Node `v25.2.1`.

Keep this tree within erasable TypeScript: ordinary type annotations and
`import type`, without syntax that needs code generation. Tests in
[`r4_4.rs`](../../crates/hh-xcheck/tests/r4_4.rs) check direct execution and the
absence of npm dependencies. Running under Node strips types; it does not type-check.

## Run

From the repository root:

```sh
node foreign/e3-typescript/foreign.ts crates/hh-xcheck/fixtures/cross-impl /tmp/answers-e3.json
cargo run -p hharness-xcheck -- verify crates/hh-xcheck/fixtures/cross-impl /tmp/answers-e3.json
```

The comparator's final line should be `verify: 57 passed, 0 failed`, with exit
code 0. To use the combined runner, build the comparator first:

```sh
cargo build -p hharness-xcheck
bash scripts/xcheck-e3.sh --answers /tmp/answers-e3.json
```

The script uses `target/debug/hh-xcheck`, or the binary named by `HH_XCHECK_BIN`.
Its Node check only checks the major version; use the toolchain versions above
so direct `.ts` execution works. [answers.json](answers.json) is the committed
R4.4b result; the commands write fresh results to scratch space.

## Coverage and limits

[CHECKER.md](../../crates/hh-xcheck/CHECKER.md) defines the encoding, identity
functions, replay contract, and answer comparison. The implementation is split
into a shared core and corpus adapters:

| Source | Responsibility |
|---|---|
| [lib/core.ts](lib/core.ts) | Canonical JSON, SHA-256, and `idp/1` primitives |
| [lib/trees.ts](lib/trees.ts) | `identity-trees` |
| [lib/records.ts](lib/records.ts) | `identity-records` |
| [lib/ledger.ts](lib/ledger.ts) | One WAL fold for `ledger-transcript` and `ledger-scenarios` |
| [lib/registry.ts](lib/registry.ts) | `registry-snapshot` and the shared class catalog |
| [lib/plugin.ts](lib/plugin.ts) | `plugin-manifests` |
| [lib/canonical_parse.ts](lib/canonical_parse.ts) | `canonical-parse`; timings are informational |
| [lib/hir.ts](lib/hir.ts) | `hir-documents`: parse, canonicalize, validate, and seal |

This is a corpus implementation, not a complete TypeScript runtime. HIR cases
reaching unported kinds, edges, `assumption_debt`, or `flow_contract` are omitted;
the comparator reports a missing answer instead of receiving a fabricated
verdict. The original primitives were written from the contract and fixtures,
with the Python candidate used as a structural reference.

A successful run is `foreign-run-verified` evidence for this corpus. It does
not establish general conformance, live-service operation, or release readiness.
Zero missing answers does not close the bundle's recorded `environment-pending`
cells: `verified_by[]` records replay evidence while the HUMAN-H3 prerequisite
remains separate. See [the evidence policy](../../MAINTAINING.md#evidence-classes).

## Recorded verify summary (R4.4b)

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
cells missing: 0` — all eight arms answered, zero missing answers.

## Historical implementation notes

The notes below preserve the recorded R4.4/R4.4a/R4.4b implementation and the
CF-493 evidence-vocabulary change. The E2 six-arm observations describe the
Python tree at those dates; [R4.16a later added its two missing arms](../e2-python/README.md#dated-note--2026-10-23-r416a).
The current Python implementation also answers all eight arms.

### Contract observations

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

### Recorded R4.4b result

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
