# Contributing to HarnessHarness

Contributions to documentation, reproducible bug reports, tests, and implementation
are welcome. You do not need to understand the full specification to report a
problem or improve a guide. Start with the [README](README.md) and
[getting started](docs/getting-started.md) to try the system.

HarnessHarness keeps a detailed record of its design and implementation. The
maintainer coordinates how changes enter that record; the additional workflow
for assigned build tickets is described below.

## Report a problem or propose a change

Use the [issue templates](https://github.com/SteveVitali/HarnessHarness/issues/new/choose).
For a bug, include the command you ran, expected and actual output, commit or
release version, operating system, and architecture. `hh doctor` output can help
identify configuration problems. Remove credentials and private data before
posting logs or a reproduction.

For a feature or substantial design change, explain the problem, the behavior you
want, and alternatives you considered. Changes to a contract or the dependency
set require an architecture decision record (ADR); discussing the proposal first
helps establish its scope. Report security vulnerabilities privately through
[the security policy](SECURITY.md).

## Set up a checkout

Follow the [source-build instructions](docs/getting-started.md). The repository's
[rust-toolchain.toml](rust-toolchain.toml) selects Rust 1.94 and `rustfmt`; Cargo
uses the committed lockfile. Git, a Rust toolchain, and a platform C toolchain are
needed for the build. Repository check scripts use Bash and Python 3. Optional
integration tools are described in the getting-started guide and the relevant
script; a missing live service is not a successful verification.

Use the [architecture guide](docs/architecture.md) to find the component you want
to change. Read the contract documentation in a crate's `src/lib.rs` where
present, or its entry point for a binary-only crate. Package names use
`hharness-*`, while most directories use `crates/hh-*`: for example,
`crates/hh-wire` is package `hharness-wire`.

## Make and check a change

Keep a PR focused on a problem reviewers can reproduce. Add a regression test for
changed behavior: it should fail if the fix is removed. Existing acceptance tests
live in each crate's `tests/` directory; tests implementing a build ticket use its
identifier in the filename.

Run formatting and the relevant crate's tests. For example, for `hharness-wire`:

```sh
cargo fmt --all --check
cargo test -p hharness-wire
```

Replace `hharness-wire` with the package you changed. For changes across crate
boundaries, run the workspace checks:

```sh
cargo build --workspace --all-targets
cargo test --workspace
```

Run the checks relevant to your changes:

| Change | Check |
|---|---|
| Schema source or generated bindings | `bash scripts/check-drift.sh` |
| Dependency or package metadata | `bash scripts/check-licenses.sh` and `bash scripts/gen-third-party-notices.sh --check` |
| Documentation | `bash scripts/check-living-docs.sh` and the commands/examples you changed |
| Quickstart | `bash scripts/check-quickstart.sh` on a committed candidate |
| Feature-tier boundaries | `bash scripts/check-removability.sh` |
| CI workflow steps | `bash scripts/ci/check-ci-gating.sh` |

The quickstart check clones the committed `HEAD`; uncommitted documentation is
not part of that check. CI runs additional contract and repository checks.
Describe what you ran and the results in your PR, including skipped checks and
why they could not run.

## Respect the contracts

- **One implementation for each contract.** Reuse the existing canonical JSON,
  identity, registry, and schema implementations. The CLI and other surfaces use
  the `hh-embed/1` boundary.
- **Dependencies require a decision.** The workspace uses the standard library
  plus a narrow admitted set: `ring` in `hharness-secrets`
  ([ADR-0359](docs/adr/ADR-0359-r3.7-crypto-dependency-choice-ring-0.17.14-for-the-dpop-verifier-and-the-reserved-aead-surface.md)),
  the rustls graph in `hharness-tls`
  ([ADR-0365](docs/adr/ADR-0365-r4.5c-rustls-dependency-admission-the-hh-tls-home-and-the-closed-https-mtls-scope.md)),
  and `tempfile` for tests. New dependencies or broader uses need an ADR before
  implementation.
- **Change generated content at its source.** Schema artifacts, the generated
  Rust client and `clients/ts/hh-embed.ts`, third-party notices, and the ADR
  indexes are generated. Use their generators; do not patch the output by hand.
- **Preserve historical evidence.** Follow the [documentation edit modes](docs/README.md)
  before changing a spec, ADR, ticket, research record, or build record. Living
  guides can be edited directly; historical corrections need a new dated entry.
- **Make verification claims precise.** Distinguish fixture tests from a live
  service run. Follow the [evidence policy](MAINTAINING.md#evidence-classes) when
  reporting either, and never commit credentials.

[AGENTS.md](AGENTS.md) and [crates/AGENTS.md](crates/AGENTS.md) carry the detailed
engineering constraints used by coding agents. Human contributors should consult
the relevant rules when making implementation changes.

## Open a pull request

Explain the problem and resulting behavior, link the issue or ticket when there
is one, and include validation results. Use the
[PR template](.github/pull_request_template.md); mark checks that do not apply.
The maintainer reviews contributions and merges them. Automation does not merge
PRs. [GOVERNANCE.md](GOVERNANCE.md) explains decision authority and contract changes;
[MAINTAINING.md](MAINTAINING.md) covers release and record-maintenance duties.

### If you are implementing a manifest ticket

Start every work run by reading the open-deferral index:

```sh
bash scripts/deferrals-open.sh
```

Then read the relevant deferral rows, the ticket contract, and the current state
in [the build ledger](docs/build/LEDGER.md). Use [the manifest](docs/tickets/00_MANIFEST.md)
for ordering. Ticket PRs stack on the previous ticket's branch. Keep the run
record, PR record, requirement IDs, and affected deferrals consistent with the
ticket; a gate pauses for an operator readout, and the operator merges.

Use `scripts/bm-row.sh` with a specific `DF-*`, `BL-*`, or other supported ID, and
`scripts/spec-section.sh` with a section anchor. Read relevant ranges of large
files rather than loading entire registers. If the change unblocks a deferral,
its disposition is part of the work. Append dated corrections and evidence;
never erase prior entries. Maintainer tooling for these records is documented
in [MAINTAINING.md](MAINTAINING.md#build-memory-tools).

## Licensing and conduct

The project is dual-licensed **MIT OR Apache-2.0**
([MIT](LICENSE-MIT), [Apache-2.0](LICENSE-APACHE)). By submitting a contribution,
you agree it is licensed under the same terms. **No DCO or CLA is required.**

Please follow the [Contributor Covenant 2.1 Code of Conduct](CODE_OF_CONDUCT.md).
