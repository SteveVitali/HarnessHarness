# Changelog

All notable changes to this project are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project adheres to [Semantic Versioning](https://semver.org/).

The authoritative record of what shipped is the build record itself:
`docs/build/BUILD_INDEX.md` (one row per landed ticket) and the ticket
chain `docs/tickets/00_MANIFEST.md`. This file is the human-facing
summary, updated per release.

## [Unreleased]

## [0.1.0] — unreleased

First public release; publication mechanics (crate renames under the
`hharness` prefix, crates.io metadata, the GitHub Release) are the L4.4 /
L4.5 tickets' scope.

### Added

- The E1 reference kernel + Laboratory for harness engineering: a
  52-crate pure-std Rust workspace (modulo the ADR-0359 `ring` pin and
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
  foreign-answers verify).
- Packaged plugin variants under `plugins/` served out of process over
  `plugin_abi/1`.
- The committed build record: `docs/build/` build memory, the manifest
  ticket chain, `docs/adr/` decision records, `research/` program
  ledger.

### Notes

- The verified ceiling is **fixture-verified**: exercised end-to-end
  against fixtures, with `live-executed` legs only under committed
  `hh-live-evidence/1` records. See `README.md` → *Status — honesty
  ceiling* and `docs/build/OPERATIONAL_READINESS.md`.
