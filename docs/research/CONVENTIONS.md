# Evidence and adoption — research & design note conventions

These conventions apply to the program in [the ledger](../research-ledger.md).
They adapt the build-memory research-note template to repository inspection,
external research, experiment design and isolated feasibility work. This guide
is living; accepted notes are frozen records. Correct an accepted note with a
dated follow-up note and link it from the ledger, retaining the original.

## Required header

```text
# <NN>_<slug> — <title>
Stream: <ledger stream>
Researched: <actual UTC date>
Author: <agent/session identifier>
Ledger rows covered: <IDs>
Repository baseline: <commit; relevant working-tree changes if any>
Confidence overall: high | medium | low
```

Use the reserved `NN_<slug>.md` output path in the ledger. Research findings go
under `docs/research/`; design findings go under `docs/design/`. New follow-up
units get new paths; do not overwrite an accepted note. Keep central ledgers
under the coordinator's ownership.

## Required material finding

```text
### F<n> — <short claim>
Claim: <one sentence>
Status: VERIFIED | PARTIALLY VERIFIED | UNVERIFIED | CONTRADICTED | INACCESSIBLE
Evidence kind: source inspection | recorded execution | inference | design proposal
Evidence: <read source, exact locator/version and what it supports>
Retrieved: <actual UTC inspection/fetch timestamp>
Implication for the spec: <required behavior or unresolved design choice>
Outline delta: CONFIRMS | CORRECTS | EXTENDS | CONTRADICTS OL-<nn> — <detail>
Limitations: <what this evidence does not establish>
```

For a web source, give the exact URL, publication/version date when available,
and actual retrieval time. Prefer primary sources for technical claims. For a
repository source, give a repository-relative link, symbol or bounded line
locator, commit and actual inspection time. For execution evidence, give the
command, environment/prerequisites, source revision, result, artifact location,
and the timestamp recorded by the execution. Inspection time is not run time.
State when a previously recorded execution was merely inspected.

Nothing cited that was not read. A proposal is not a VERIFIED implementation;
use UNVERIFIED with its rationale and proposed validation. A checked fact can
support a proposal without proving it. Distinguish a test's assertions from the
broader property it may suggest. Never report an unrun command as passing.

## Required closing sections

- **Answer and completion assessment:** Answer the unit's question, state what is settled, and distinguish technical blockers from uncertainty that is acceptable for this row.
- **Open questions:** Include owner, affected units and whether an operator decision is needed. An empty section explicitly says none.
- **Spec requirements emitted:** Use `REQ-<STREAM>-<n>` candidate IDs, with rationale, source findings, affected OL IDs and a proposed acceptance method; explicitly say none when appropriate. The coordinator allocates unique numbers per stream before dispatch. H1 maps these candidates into the final supplemental requirement convention without altering existing IDs.
- **Handoff:** Dependencies resolved, proposed follow-up rows, conflicts with other notes and any capability/portfolio disposition changes.

## Scientific and operational evidence

A protocol must identify its question, claim, tasks, treatment/control, grading,
resource constraints, repetition/assignment method, analysis, uncertainty,
failure/exclusion rules, stopping rule, practical effect threshold and known
validity limits. State which choices remain provisional until pilot evidence.
Freeze the confirmatory protocol before collecting confirmatory outcomes;
preserve dated changes and do not silently reuse pilot data as confirmation.

Use the repository's evidence vocabulary, including fixture, live and foreign
conformance distinctions. A successful provider call, trace, bundle verification
or experiment service invocation does not itself prove a successful task,
improvement over baseline, secure deployment, or adoption benefit. Link each
published claim to the specific observation or analysis that supports it.

Receipts must support the procedure actually promised: inspection, verification,
recomputation, replay or fresh execution. Document nondeterminism and comparison
tolerances. Preserve negative results, failed attempts, unsupported paths and
skips with reasons. Redact secrets and private data without presenting a
sanitized artifact as byte-identical to the original; define access and integrity
handling in the receipt design.

## Completion and coordination

Before accepting a unit, the coordinator checks its sources, scoped conclusions,
dependencies, candidate requirements and outline deltas. Every done row links to
its accepted note. Change-log timestamps and `updatedAt` use the actual UTC
clock; `lastCompleted` names the latest accepted unit and `nextUnit` names an open,
dependency-ready unit or the appropriate skill phase. Respect operator pauses.

The specification reviewer uses a fresh context with the brief, ledger, notes
and proposed spec, without the author's conversational justification. Every
review finding receives an explicit accept/adapt/rebut/defer disposition with
evidence. Ratification and subsequent build authorization remain operator acts.
