# Contributing

Thanks for your interest. HarnessHarness is built ticket-by-ticket under
a manifest chain with committed build records — read `AGENTS.md` first;
it is the contract for working in this repo.

## Build & test

`rust-toolchain.toml` pins Rust 1.94 (with `rustfmt`).

```sh
cargo build --workspace               # all crates + binaries
cargo build --workspace --all-targets # what CI builds
cargo test --workspace                # the full suite
cargo test -p hh-<crate>              # one crate's tests
cargo fmt --all --check               # CI gate
bash scripts/check-drift.sh           # schema artifacts: regenerate; drift fails
bash scripts/check-licenses.sh        # every workspace package MIT OR Apache-2.0
bash scripts/check-living-docs.sh     # no personal paths in living docs
bash scripts/gen-third-party-notices.sh --check   # notices drift check
```

## Conventions that gate CI

- **Hermetic modulo ADR** — no external dependency beyond the named,
  pinned admissions (`ring` in `hh-secrets`, ADR-0359; the rustls graph
  in `hh-tls`, ADR-0365; `tempfile` as a dev-dep). Any new dependency is
  its own ADR *before* the code.
- **Generated files are never hand-edited** — `schema/*.json`,
  `crates/hh-embed-client-generated/src/generated.rs`,
  `THIRD_PARTY_NOTICES.md`, `docs/adr/README.md`. Change the source and
  regenerate.
- **CI step modes (ADR-0362)** — a `ci.yml` step reading committed
  `docs/`/`research/` content carries `# ci-mode: both — <reason>`;
  every other step after `id: path` carries
  `if: steps.path.outputs.mode != 'fast'`. `check-ci-gating.sh` enforces.
- **Tests** land as acceptance batteries `crates/hh-*/tests/<ticket>.rs`;
  every new behaviour gets a test that fails if it is removed.

## The build record

This repo commits its own build history: `docs/build/` (run ledgers,
BUILD_INDEX, DEFERRALS-driven owed work), `docs/tickets/` (the manifest
chain), `docs/adr/` (decision records), `research/` (the program
ledger). These are append-only provenance — amend by adding dated
entries, never by rewriting landed files. If your change is a build
ticket, follow `docs/build/LEDGER.md` + the ticket contract; drive the
build-memory tools via `scripts/deferrals-open.sh`, `scripts/bm-row.sh`,
`scripts/spec-section.sh` — never read the registers whole.

## Licensing — contributions

The project is dual-licensed **MIT OR Apache-2.0** (`LICENSE-MIT`,
`LICENSE-APACHE`). By submitting a contribution you agree it is licensed
under the same `MIT OR Apache-2.0` terms. **No DCO or CLA is required.**

## Conduct

This project follows the Contributor Covenant 2.1 — see
`CODE_OF_CONDUCT.md`.
