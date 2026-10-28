# Changelog

All notable changes to this project are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project adheres to [Semantic Versioning](https://semver.org/).

The authoritative record of what shipped is the build record itself:
`docs/build/BUILD_INDEX.md` (one row per landed ticket) and the ticket
chain `docs/tickets/00_MANIFEST.md`. This file is the human-facing
summary, updated per release.

## [Unreleased]

### Availability note — 2026-10-27

The [v0.1.0 GitHub Release](https://github.com/SteveVitali/HarnessHarness/releases/tag/v0.1.0)
is available with four platform archives and `SHA256SUMS`. The entry below is
the original prepublication summary, retained as recorded. Archive availability
does not establish complete crates.io publication or close the engineering
launch gate; see [project status](docs/status.md) for the current distinction.

*Update 2026-10-28:* publication is complete — all 49 publishable crates are
on crates.io at `0.1.0` and the launch gate closed at `HUMAN-H7`; the verified
record is `docs/build/reports/2026-10-28-launch-record.md`.

## [0.1.0] — 2026-10-28

Released 2026-10-28 — annotated tag `v0.1.0` on merged-main
`d1325a63bab7bf0836250d9df871e2bfdd76dea2` (CI + gitleaks green, run
`40525512308`), the four-archive GitHub Release live at
`…/releases/tag/v0.1.0`, and **all 49 publishable crates on crates.io at
`0.1.0`** (each verified via the crates.io API). The full verified
record — tag proof, asset checksums, publish incidents, protection
state — is `docs/build/reports/2026-10-28-launch-record.md`.

First public release. Publication mechanics landed at L4.4 / L4.5:
every workspace crate carries the `hharness-*` name and full crates.io
metadata at version `0.1.0`; the umbrella `hharness` facade crate
re-exports the public API crates; `scripts/publish-order.sh` derives
the topological publish order over the 49-crate publish set and
dry-runs it (49/49 names free); the `release.yml` workflow builds `hh` /
`hh-kernel` / `hh-helper` for the ADR-0380 target set (macOS arm64 +
x86_64, Linux x86_64 + aarch64 — Windows excluded, see the ADR) with a
`SHA256SUMS` manifest attached to the GitHub Release. The build record
closed at **GATE-G6 PASSED** (2026-10-23): zero OPEN deferrals, zero
open backlog debt, coverage 61 MET / 5 MET-DIFFERENTLY / 0 PARTIAL =
66/66, and 34 committed `hh-live-evidence/1` records at USD 25.26 of
the round-4 USD 100 live-spend cap — see
`docs/build/readouts/GATE-G6.md`.

### Added

- The E1 reference kernel + Laboratory for harness engineering: a
  54-crate pure-std Rust workspace (modulo the ADR-0359 `ring` pin and
  the ADR-0365 rustls graph) — `hh-embed/1` typed boundary over three
  bindings (in-process, stdio JSON-RPC 2.0, loopback HTTP), the `hh`
  CLI, `hh-kernel`, `hh-helper`, `hh-web`, `hh-mcp-serve`.
- Kernel spine: canonical JSON/SHA-2 (`hharness-wire`), the formal model +
  canonicalizer (`hharness-ontology`/`hharness-identity`), the provenance label
  lattice (`hharness-provenance`), HIR/1 (`hharness-hir`), the append-only
  hash-chained event store with blob-pool AEAD (`hharness-ledger`), budget /
  registry / secrets / telemetry / monitor (Π) / containment / env /
  gateway / context / control / verification slices.
- Lab tier: experiment engine, results store, estimator, eval, the
  hermetic `benchset/stage3_v1` corpus, run bundles, hosting ABI,
  subagent / orchestrator / fleet / evolution / debt services.
- `hh-xcheck` cross-implementation replay (E1 self-check + E2/E3
  foreign-answers verify); round 4 extended the bundle with the
  HIR-documents and ledger-scenario arms and added the E3 TypeScript
  foreign implementation (`foreign/e3-typescript`).
- Packaged plugin variants under `plugins/` served out of process over
  `plugin_abi/1`.
- Gateway-backed live model path (round 4): `ModelPort` on the run path
  over HTTP transports — local Ollama, Anthropic Messages and OpenAI
  through the broker `CredentialPort`; the ADR-0365 rustls graph gives
  `hharness-tls` HTTPS/mTLS transport and honours `tls.terminate`;
  `probe_profile` live-signal routing arms.
- Live perimeter (round 4): the `hh-live-evidence/1` framework
  (`live-verify.sh` + spend ledger), GitHub Issues `work_source_adapter`
  with signed webhook ingress, OAuth device flow + the mTLS
  `CallerCredential` mediator, provider-backed `Embedder`/`Summarizer`/
  `ProviderCompaction` adapters, a live observability-sink seam, the
  live judge binding (`CriticDeclaration{kind=judge}`), remote `fetch`
  transports with adopted trust fields, and signer-key custody with
  signed checkpoints.
- The committed build record: `docs/build/` build memory, the manifest
  ticket chain, `docs/adr/` decision records, `research/` program
  ledger.
- Launch hardening (round 4): dual MIT/Apache-2.0 licensing with the
  generated `THIRD_PARTY_NOTICES.md`; CI supply-chain legs (cargo-deny,
  gitleaks, dependabot, the protected `live-verify` workflow); the
  docs-only CI fast path; and the publish-readiness set described
  above.

### Notes

- `fixture-verified` is the verified floor; `live-executed` legs are
  earned where a committed `hh-live-evidence/1` record exists
  (`docs/build/reports/live/` — 34 records, ADR-0363); the E2/E3 foreign
  replays are a distinct `foreign-run-verified` class (57/57 each over
  the eight `hh-xcheck` arms). `staging-verified` and `public` remain
  unearned — R4.18 owns the publish decision. See `README.md` → *Status
  — honesty ceiling* and `docs/build/OPERATIONAL_READINESS.md`.
  *(2026-10-28 update — R4.18: `public` is now earned for the published
  crate set, the tagged release, protected `main`, PVR and the
  `live-verify` environment, per the launch record; `staging-verified`
  stays unearned — no staging environment exists.)*

For the current capability and evidence summary, see [project status](docs/status.md).
The README status-heading reference in the original notes above is historical.
