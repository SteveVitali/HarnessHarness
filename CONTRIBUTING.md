# Contributing to HarnessHarness

Contributions to documentation, reproducible bug reports, tests, and implementation
are welcome. You do not need to understand the full specification to report a
problem or improve a guide. Start with the [README](README.md) and
[getting started](docs/getting-started.md) to try the system.

HarnessHarness keeps a detailed record of its design and implementation. The
maintainer coordinates how changes enter that record; the additional workflow
for assigned build tickets is described below.

## Report a problem or propose a change

Use the [issue templates](https://github.com/SteveVitali/HarnessHarness/issues/new/choose)
and pick the lane that matches what you have. **No lane below needs a built
binary, a finished run, a bundle, a paid account, or a prior issue — and none
needs the project's internal question register, build ledger, or manifest
ticket ritual.** Maintainers own program traceability and merging; the
manifest-ticket workflow further down applies to assigned build tickets only.

### Report a failure without a run

A build, install, or startup failure is reportable as soon as it is precise.
Include:

- the exact command you ran;
- expected versus actual output, including the exit code or signal;
- the commit (`git rev-parse HEAD`) and whether the checkout was dirty;
- the operating system and architecture;
- the Rust toolchain (`cargo --version`) plus relevant feature flags,
  backend, and prerequisites;
- sanitized stderr — see [Before you attach evidence](#before-you-attach-evidence).

Run `hh doctor` only if the binary actually starts; it can help identify
configuration problems, but it is never a prerequisite for reporting.

### Report a wrong run or receipt

When a run or its receipt disagrees with the contract, include everything
above plus:

- run, attempt, configuration, and input identifiers;
- the relevant event, grade, or procedure identifiers;
- the semantic field that is wrong, expected versus observed;
- a permitted native receipt, **or** the access/loss reason you cannot
  produce one — a withheld input, expired access, or a failed validation is
  reportable in its own right; suppressing the reason is what blocks the
  report.

`hh run export <run-id> --out <dir>` produces a native export and prints its
locator; `hh bundle validate <path>` checks staged completeness. Use the
locator the command actually returns — the export is the receipt and the
issue text is its cover note.

### Before you attach evidence

Inspect logs, bundles, and output for credentials, signed URLs, private
paths or content, model I/O, task solutions or oracles, and material whose
redistribution rights you do not hold. Attach only cleared data; withhold
sensitive identifiers and hashes where needed. A minimal synthetic
reproducer is welcome — state its limits. **A report that attaches uncleared
secrets is blocked until the material is removed.** Never post a secret
publicly; report security vulnerabilities privately through
[the security policy](SECURITY.md).

### Propose a change

For a feature, documentation fix, or substantial design change, explain the
problem, the behavior you want, and alternatives you considered. Changes to
a contract or the dependency set require an architecture decision record
(ADR); discussing the proposal first helps establish its scope. Documentation
changes are ordinary PRs — run the documentation row of the checks table in
[Make and check a change](#make-and-check-a-change).

### Show a fix works: verifyfixed

A fix keeps the original failure on the record and shows comparable evidence
on the candidate revision:

- pin the reported revision, input, recipe, and rights;
- preserve the original failure report and bytes; copy only permitted
  evidence into scratch;
- add a regression test that fails without the fix, then run the same case
  on the candidate revision;
- disclose any changed input or backend — a missing prerequisite is
  **unable to reproduce**, not fixed;
- reporter confirmation, maintainer reproduction, and outside verification
  are distinct outcomes; one does not stand in for another.

### Two routes that are not open yet

- **The optional packaged component** — the bounded
  `hharness-compact-evict-oldest` extension exercise — stays gated: the
  round-5 exercise was recorded unselected (stage voided, no claim
  awarded), and it reopens only under a fresh named selection. When it
  opens it will require observed build, seal, load, bind, and invoke
  phases, not a green exit code alone.
- **Scientific replication** is a separately admitted and separately
  resourced protocol — fresh attempts under frozen methods, pins, and
  margins — not an ordinary contribution route.

### Where a contribution lands

The maintainer reviews every path and keeps program integration; the rows
below map a starter to its owning area and the checks its PR runs.

| Starter | Owning area | Checks the PR runs |
|---|---|---|
| Setup or failure guidance | `docs/getting-started.md`, `docs/reference.md`, this file | `bash scripts/check-living-docs.sh`; the commands you changed |
| CLI or run/receipt mismatch | `crates/hh-cli`, `crates/hh-embed` | `cargo test -p <package>` plus a regression that fails without the fix |
| Bundle or receipt member defect | `crates/hh-bundle` | `cargo test -p hharness-bundle`; pins and losses stay on the record |
| Documentation and examples | `docs/`, `scripts/examples/` | `check-living-docs.sh` and the changed links or commands |

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
