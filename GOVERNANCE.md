# Governance

HarnessHarness is maintained by the repository owner, currently
[@SteveVitali](https://github.com/SteveVitali). Contributors propose changes
through issues and pull requests; the maintainer approves design decisions,
releases, and merges. See [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution
workflow and [MAINTAINING.md](MAINTAINING.md) for maintainer duties.

## Authority

Review ownership is recorded in [CODEOWNERS](CODEOWNERS). During the
manifest build the operator's recorded rulings are the approval of record,
kept verbatim in `docs/build/LEDGER.md` GATE DECISIONS; PRs to the build
chain are stacked (`mergePolicy: OPERATOR`) — the operator merges, and
automation never merges. Post-launch the maintainer holds the same acts.

## Decisions are ADRs

- Every consequential decision lands as a `docs/adr/ADR-NNNN-<slug>.md`
  ([template](docs/adr/_TEMPLATE.md): Context / Decision / Consequences / Alternatives /
  Revisit trigger — the trigger section is mandatory and its standing set
  is the generated [catalogue](docs/adr/REVISIT_TRIGGERS.md)).
- ADRs are immutable once landed: a change is a *new* ADR; a retro-fitted
  record says so in its title. A landed ADR is never edited — dated
  re-checks of a fired trigger live under `docs/build/reports/`, not in
  the ADR body.
- `docs/adr/README.md` is generated (`build-memory adr-index`) and
  `docs/adr/REVISIT_TRIGGERS.md` is generated
  (`scripts/gen-revisit-triggers.sh`) — neither is hand-edited.
- Approver of record: the maintainer (during the build: the operator's
  gate rulings, recorded verbatim in GATE DECISIONS).

## The spec and the frozen corpus

[spec/CANONICAL_SPEC.md](spec/CANONICAL_SPEC.md) is the frozen behavioural
contract. It is amended only through the manifest protocol: an ADR carries the ruling plus a
verbatim amendment log (ADR-0210's log is the shape), the spec text takes
the marker the protocol prescribes, and the amendment lands through the
ticket chain — never an in-place edit outside that protocol.

`research/` — the program ledger, research ADRs, registers, dossiers — is
frozen at the readiness gate. A change there is a new research record, not
an edit of a landed one.

## The build record

`docs/build/`, `docs/tickets/`, `docs/adr/` are append-only provenance:
entries are amended by appending dated notes, never rewritten; a correction
is a new dated entry naming the sha and line it corrects. Owed work lives
in `docs/tickets/DEFERRALS.md` (a deferral not in the file did not happen;
owed state is read from the dated markers via `scripts/deferrals-open.sh`)
and `docs/build/BACKLOG.csv` (every owed id homed exactly once —
the [backlog checker](MAINTAINING.md#build-memory-tools) enforces).
`docs/build/LEDGER.md` is the machine state (`nextTicket`, `chainTip`, gate decisions).

## Roles

- **Maintainer** — approves ADRs and spec amendments, owns the release
  mechanics and the RK-06 spec-debt revalidation cadence per
  `MAINTAINING.md`, and discharges or re-arms revisit triggers on the
  recorded evidence.
- **Contributors and agents** — propose and implement changes under
  [CONTRIBUTING.md](CONTRIBUTING.md) and the engineering rules in
  [AGENTS.md](AGENTS.md). Work on a manifest ticket follows its scoped
  contract and chain. Every new behaviour carries a test that fails if
  it is removed; a gate is a pause, never a block; secrets never enter
  any ledger.
- **Operator** — during the manifest build, the human whose recorded
  rulings gate the chain (GATE-*, HUMAN-* rows); post-launch this is the
  maintainer.
