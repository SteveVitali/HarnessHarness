# GATE-G8 readout — review partition and authorize the bounded tooling bootstrap

Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

*Append-only. Each operator reading is a new dated block; never edit an earlier one.*

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is approval.

## Criterion (verbatim — `docs/tickets/168_GATE-G8__bootstrap.md`, source: supplement §9)

> "Missing tooling or independent partition/O2/operator review blocks readiness; passing stock validation cannot substitute."

**Pre-registered thresholds (verbatim):** Review the partition, exact coverage, independent dispositions and readiness packet in Zed. A fresh scoped decision may authorize ONLY the two tooling contracts and admission-dossier preparation; no product semantics, collection, spending or publication. Reconfirm base, source digests and stock baseline. Guard absence is the named bootstrap exception, never a product exception.

## Evidence (per item: source, `date -u` of the read, layer reached)

- `docs/build/planning/2026-10-30_evidence-decomposition.md` — read 2026-10-30 — layer: engineered (79-row partition; reviewed aggregate recomputes exactly)
- `docs/build/planning/2026-10-30_evidence-coverage.md` + `.json` — read 2026-10-30 — layer: engineered (180 candidates → 167 primary/12 merged/1 superseded; both companions hash-identical to rereview)
- `docs/build/planning/2026-10-30_evidence-invariants.md` — read 2026-10-30 — layer: engineered (I-01–I-08 / O-01–O-08; hash-identical to rereview)
- Independent reviews: `…-partition-review-initial.md` (IP-01–04/IP-N1, changes-required), `…-coverage-review-initial.md` (COV-01–04, changes-required), `…-partition-rereview.md` (clean), `…-coverage-rereview.md` (clean) — read 2026-10-30 — layer: engineered (review evidence only; not runtime/scientific proof)
- `docs/build/planning/2026-10-30_evidence-readiness.md` + `…-validation.md` + `…-planning-ledger.md` — read 2026-10-30 — layer: engineered (remaining prerequisites enumerated; no execution evidence claimed)
- `docs/build/planning/2026-10-30_evidence-legacy-baseline.json` — read 2026-10-30 — layer: engineered (exact 8-warning scope; GATE-G1 reading-2 closure `e286311b…`)
- Repo state: `git` local `main` = `origin/main` = `001490c14a6568972d16a663bb04c07a4255d30d`; `gh pr list` empty; `gh run list` — CI run `40623318459` success on that merge; `bash scripts/deferrals-open.sh` empty — read 2026-10-30 — layer: records
- Stock validator `check-build-memory.sh .` — exit 0, 0 violations / 8 warnings = scoped baseline — run 2026-10-30 — layer: engineered
- Supplement `shasum -a 256` — `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f` — read 2026-10-30 — layer: records
- agent-skills `git rev-parse HEAD` — `e981ec57d487a2c41d416672483b5a4df15b1233`, clean tree — read 2026-10-30 — layer: records
- Harness observation — coordinator `devin-desktop / SWE-2 High` (runtime-declared; Desktop `agentPreferences` `devin-cloud→devin-swe-2-high`, `devin-cli→claude-opus-5-5-xhigh` ≠ this session); workers `subagent_general` (parent-model inheritance per harness docs); context window not self-observable → recorded unmeasured; Fusion off — read 2026-10-30 — layer: records

<!-- agent-drafted:begin sha256=259ee9f5c65ab687af116a0d3a677028052970bbc27d1a0722ca947877f1287a -->
Round-5 orientation verified at presentation (reads performed 2026-10-30, this coordinator session):

- **Base:** local `main` = `origin/main` = `001490c14a6568972d16a663bb04c07a4255d30d` — the reviewed code baseline; ancestry confirmed; zero open PRs; latest CI on that merge (run `40623318459`) success on all required jobs.
- **Supplement identity:** `docs/evidence-and-adoption-spec.md` SHA-256 `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f` — byte-identical to the ratified source.
- **Reviewed artifact identity:** aggregate `d5b58551802854b484c0bec8c71d2dfc6084eaf17b0499592387f5d91b48e39f` recomputes exactly over the four planning files at reviewed bytes (decomposition pre-addendum `8ade4d28…`) plus all 79 contracts; the only post-review delta is the dated Review-disposition addendum itself. Kinds: 43 ticket / 13 skeleton / 11 gate / 4 human / 3 capstone / 3 reconcile / 2 docs; skeletons carry no Run line.
- **Stock validation:** `check-build-memory.sh` exit 0 — zero violations, 8 warnings matching the scoped legacy baseline exactly (including the GATE-G1 `nextTicket` residual, which must never route new work). `deferrals-open.sh`: empty — zero open rows.
- **Writers:** no agent process holds a cwd in this checkout; sibling worktrees are on unrelated branches; the historical concurrent-writer finding is satisfied.
- **Harness (observed, not assumed):** coordinator = `devin-desktop / SWE-2 High` (runtime-declared identity; Desktop `agentPreferences` record `devin-cloud→devin-swe-2-high` and a separate `devin-cli→claude-opus-5-5-xhigh` default that is not this session's runtime). Workers will spawn via `run_subagent` profile `subagent_general`, which per harness documentation runs the parent's model — SWE-2 High; a usable-context figure is not exposed to the agent and is recorded **unmeasured** — every contract carries an explicit estimate + 10 k reserve (max 38 k, vs the ≤40 % sizing rule) and workers self-report their observed identity at dispatch (R5.2 exists to prove exactly this path with stub workers). Fusion: not enabled. `agent-skills` checkout `e981ec57d487a2c41d416672483b5a4df15b1233`, clean.
- **Review evidence:** initial partition review (IP-01–04, IP-N1) and coverage review (COV-01–04) found scoped findings, all repaired and closed clean by focused rereviews; records linked from the review guide.

**Scope this decision may authorize — and only this:**

1. **SETUP:** in this checkout (the ledger's recorded `buildWorktree`), create branch `codex/harnessharness-evidence` at pinned base `001490c1` and commit the exact reviewed planning artifacts — the 79 contracts, the planning packet, the upstream research/spec/design surfaces, the modified README/LEDGER/MANIFEST rows, and this pending readout — as the planning commit; the ledger `chainTip` then advances to that branch.
2. **Bootstrap:** dispatch `169_R5.1` then `170_R5.2` sequentially via `implement-spec` in fresh subagent contexts — the retained supplemental coverage/routing guard, then the actual native stub-worker dispatch/rejection/CI-failure/resume proof. No unguarded accumulating loop; a failed native proof returns a scoped dispatch decision rather than silently switching to headless.

**Not authorized by this gate:** product semantics, §9.3 ADR admission, any task/provider/cloud spend, collection, outreach, publication, or merges. GATE-G9 and every later gate/human row returns for its own decision; skeletons are never dispatched; carried unmet predicates and withheld claims remain recorded.
<!-- agent-drafted:end -->

## Signature

Operator decision (verbatim, received 2026-10-30T04:16Z via Devin Desktop chat): "passed. please proceed"
GATE DECISIONS row: 2026-10-30 | GATE-G8
Operator confirmation of agent-drafted text: not separately quoted — the PASSED decision followed the presented readout containing sha256:259ee9f5c65ab687af116a0d3a677028052970bbc27d1a0722ca947877f1287a; a later dated confirmation may be appended below.
Signed by: the operator. Recorded by devin-desktop/swe-2-high, which does not sign.

