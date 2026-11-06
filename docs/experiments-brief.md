# HarnessHarness experiments program — founding brief

**Recorded:** 2026-11-06T22:48:30Z
**Status:** Frozen charter for an upstream research/design program (`synthesize-spec`). Later changes are dated addenda; evolving work and decisions live in the [experiments research ledger](experiments-research-ledger.md).
**Inputs:** the operator's handoff brief `~/SCRATCH-hh-experiment-roadmap.md` (sha256 `4a1ca67db72a9e47ae4530e879a25db36f02b402df4fc4cb6bb628c691835628`, reproduced verbatim in §2), and the operator's answers in the planning conversation of 2026-11-06 (Claude Code session, recorded verbatim in the ledger's §Q register).
**Repository baseline:** `main` at `75883daa` (S4-30SLOT-REPAIRS, #289). The round-5 chain was concurrently closing R5.59 on `codex/harnessharness-r5.59` (`36dfcb3f`); this program does not race it (Q-X3).
**Workflow reference:** `SteveVitali/agent-skills` at `e981ec57d487a2c41d416672483b5a4df15b1233` — `synthesize-spec`, `decompose-spec` (`mode=extend`), `build-memory`, `orchestrate-build`.

## 1. Purpose

Turn HarnessHarness from a runtime whose execution surface has been checked with
deterministic gold scripts into a **lab that runs real, rigorous, scientific
agent-harness evaluations** — and show its results publicly in a form that is as
readable as the best public harness comparisons while being more verifiable.

Four phases of increasing ambition (operator intent, §2):

1. **Methodology record** of the already-executed $0 experiment, with honest bounds, linked from the README.
2. **Comparative harness evaluations** with real LLM agents built on mainstream open-source (and possibly closed-source) harnesses, with HH as the execution surface and lab.
3. **Native-harness parity via HIR** — represent simple open-source harnesses in HH's HIR and prove parity with the originals.
4. **HH-native configuration sweeps** — vary one harness variable at a time and report what performs best.

Plus a fifth deliverable added by the operator during planning (Addendum A-6): a
**static results website**, generated from committed records and hosted from the
repository via GitHub Pages.

## 2. Original handoff brief (verbatim)

> The following is the operator-provided handoff brief, reproduced byte-for-byte
> apart from the leading `> ` quote markers. Its factual claims about the
> executed experiment are **inputs to be audited** (ledger row A1), not findings
> of this program.

> # Agent scratch — HarnessHarness next experiment rounds (handoff brief)
>
> **Purpose:** this file is a brief for another agent (Claude Code, GPT-6 Astra, etc.) to
> translate into a rigorous design + implementation spec, decomposed into manifest tickets
> appended to the end of the existing orchestrate-build chain. It is NOT itself a spec.
>
> ## The goal (operator's own words, systematized)
>
> Four phases of increasingly ambitious, methodologically-rigorous experimentation:
>
> 1. **Methodology writeup** — document the already-executed experiment (below) as a
>    proof-of-correctness record living in the repo, linked from the main README. The
>    writeup must state methodology and results clearly, with the honest bounds.
>
> 2. **Comparative harness evals ($ spend admitted)** — use HarnessHarness as the lab to
>    run real LLM-agent evaluations comparing mainstream open-source (and possibly
>    closed-source) harnesses on a benchmark task set. This demonstrates the customer
>    use case: HH is a lab for running actual, rigorous, scientific agent evals.
>    Results published clearly, linked from the README. Requires a funded model budget,
>    per-model/agent admissions, and a declared tranche design (N, endpoints, margins).
>
> 3. **Native-harness parity via HIR** — represent a simple open-source harness in HH's
>    internal intermediate representation (HIR), then prove parity between the
>    HH-native representation and the original harness using the same parity machinery
>    that just verified 30/30 task verdicts (same participant, both surfaces, exact
>    directional agreement, declared margins, recorded discordances).
>
> 4. **HH-native config sweeps** — vary one harness variable at a time (compaction
>    policy, context strategy, tool surface, etc.) across many HH-native configurations,
>    measure impact systematically, and report which configurations perform best. This
>    is the "controlled experiment" use case the HIR representation enables.
>
> The operator: "this is a fairly ambitious ask" — scope, ordering, and admissions are
> the spec's job; this brief just preserves intent + context.
>
> ## What the completed experiment proved (the writeup's subject)
>
> - **Mechanism:** benchmark tasks ship a deterministic committed `solution.sh` (gold
>   script — no LLM, $0). Replaying it inside HH cells and grading with the task's own
>   verifier proves the *execution surface* is faithful — not that an agent is smart.
> - **Execution surface** = the HH cell machinery an agent runs inside: podman
>   container + containment policy (`--network none` default; admitted `declared_deps`
>   bridge variant, ADR-0421), mediated dispatches with verbatim receipts,
>   sentinel/observer integrity checks, sealed artifacts, verbatim stdout/stderr
>   capture, verifier parsing (ANSI-stripped, workdir-respecting — two real machinery
>   defects found and repaired by the experiment itself).
> - **S3 parity:** 30/30 task-verdict agreement vs the original Terminal-Bench runner,
>   zero discordance, `kl_directional_bound` ≈0.116 at N=30 — `verified`.
> - **S4 confirmation:** 300/300 slots executed with real receipts (30 tasks × 2 arms ×
>   5 rounds; 220 resolved / 80 unresolved; 60/60 first subjects). Three epochs of
>   honest records; every repair (ANSI, OOM, disk, workdir, archive-mirror,
>   vendored-repo) recorded verbatim as machinery or admission — never hidden.
> - **Cost:** $0.00 inference; ~$1.41 GCE instance-hours; zero model calls.
> - Evidence: `docs/build/reports/2026-11-05-parity-tranche-3.json`,
>   `2026-11-05-s4-confirmation.json`, `2026-11-05-s4-egress-cohort.json`,
>   `2026-11-06-s4-completion.json`, `2026-11-06-s4-30slot-repairs.json`;
>   ledgers under `docs/build/runs/PARITY-TRANCHE-3.md`, `S4-*.md`.
>
> ## Repo orientation (what to read first)
>
> - `AGENTS.md` (root) — governs everything; nested AGENTS.md per subtree. Frozen
>   behavioral contract: `spec/CANONICAL_SPEC.md` — amend only via ADR + manifest
>   protocol. `hh` is a client/renderer over `hh-embed/1`; kernel semantics stay out
>   of the CLI.
> - `docs/evidence-and-adoption-spec.md` — the round-5 spec the evidence machinery
>   implements (REQ-D-30x parity requirements, REQ-E efficacy, gates, admissions).
> - `crates/` map in `crates/AGENTS.md`; key machinery touched by this work:
>   `hh-env` (cells, driver, egress_profile.rs, verifier_bound.rs, study.rs),
>   `hh-helper` (podman backend, `--network` flag), `hh-eval` (central_protocol.rs —
>   seeded manifest selection).
>
> ## Build orchestration (how work gets decomposed + driven)
>
> - **Manifest chain:** `docs/tickets/00_MANIFEST.md` — numbered ticket files
>   (`NNN_ID__name.md`) executed in order via `implement-spec`; `docs/build/LEDGER.md`
>   tracks `nextTicket`/`lastCompleted`/`chainTip`. New work = append rows at the end
>   via a manifest extension (see the 2026-11-06 "Round-5 extension" PHASE LOG entry —
>   rows 247–251 were added the same way).
> - **orchestrate-build** drives the chain; each ticket's contract has Load/AC lines;
>   GATE-* rows are operator STOPs (never guessed past); HUMAN rows are operator work.
> - **Guard:** `bash docs/build/tools/check-supplement-guard.sh` must return
>   `PASS DISPATCH — <id>` before/after every dispatch; `HOLD` = RETURN PASS.
> - **Build memory:** committed v2 layout under `docs/build/` (runs/, reports/, pr/,
>   readouts/, planning/); `docs/tickets/DEFERRALS.md` + `scripts/deferrals-open.sh`
>   index owed work. External validator:
>   `AGENT_SKILLS_ROOT=$HOME/agent-skills` →
>   `bash "$AGENT_SKILLS_ROOT/skills/build-memory/scripts/check-build-memory.sh" .`
>   (must show 0 violations; 8 baseline warnings are normal). ADR index via
>   `adr-index.sh`; revisit triggers via `scripts/gen-revisit-triggers.sh`.
> - **Discipline invariants:** append-only records (correct with dated notes); typed
>   dispositions (`executed/failed/inconclusive/withheld/unselected/reported`) —
>   never relabel; `mergePolicy: OPERATOR` (agents never merge); every merge needs a
>   green CI (4 checks); size trigger → stop for fresh-context scoped review; spend
>   requires explicit operator authorization recorded verbatim.
>
> ## State in flight (as of 2026-11-06)
>
> - Round 5: 246/246 original rows merged; extension rows 247–251:
>   - R5.56 (egress variant) merged #285; R5.57 (verifier bound) merged #287;
>     R5.58 (S4 completion, 270→300/300 incl. 30-slot repairs) merged #288 + open #289.
>   - **Remaining:** GATE-G19 (efficacy ruling — cohort is now complete at 300/300, so
>     the bounded-vs-complete question resolved toward complete) then R5.59
>     (efficacy verdict + study recompute + **mandatory GCE deprovision**).
> - **GCE backend still provisioned:** project `harnessharness-evidence-8d7a99`, VM
>   `podman-s4-backend` us-central1-a (~$0.30/hr, ~$1.41 billed so far). Deprovision
>   verbatim: `gcloud compute instances delete podman-s4-backend
>   --project=harnessharness-evidence-8d7a99 --zone=us-central1-a --quiet &&
>   gcloud projects delete harnessharness-evidence-8d7a99 --quiet` — R5.59 owns the
>   receipt. Reusable for the $-spend phases if kept running (budget!).
> - Scratch/machinery lives in `~/tb-work/` (dataset tb-core 0.1.1, vendored fix-git
>   mirror, slot drivers in /tmp — rebuild if cleared); `~/tb-work/gce-backend.md`
>   has the full backend record.
>
> ## What the spec needs to decide (open questions for the designer)
>
> - Funding model for phase 2 (per-model budget ceilings, admission records, spend
>   telemetry in packets — machinery exists from the $0 telemetry legs).
> - Which harnesses/benchmarks enter the comparative set (admission criteria).
> - Whether phase 3's HIR parity compares task verdicts (like S3) or richer traces.
> - Phase ordering vs the remaining R5.59 closure; whether phases are one manifest
>   extension or several rounds.
> - Publication surface: README links, `docs/evidence.md` card states (pinned by
>   `check-position.sh` — see its mutation battery before touching claims language).
>
> ## Handoff task
>
> Read the above + the cited repo files. Produce: (a) a rigorous design spec per the
> repo's decompose-spec conventions, (b) a ticket decomposition appendable to
> `docs/tickets/00_MANIFEST.md`, (c) the honesty/admission scaffolding each phase
> needs (budget records, model admissions, parity declarations, publication gates).
> Preserve every invariant in the Build orchestration section.

## 3. Addenda recorded at charter time (2026-11-06, planning conversation)

These record operator intent stated while the charter was being written. The
binding wording of every decision is in the ledger's §Q register; these lines
only summarize what changed relative to §2.

- **A-1 · Process.** A full `synthesize-spec` program (new sibling ledger, fresh-context notes, one spec for all phases, adversarial review, ratification), decomposed afterwards with `decompose-spec mode=extend` as a new round appended after row 251.
- **A-2 · Phase 1 is "writeup + $0 hardening".** Pre-charter inspection found the §2 account of the executed experiment overstates parts of it (to be audited in A1): the S3 "H side" was a scratch script, not HH cells; R=1; the landed parity estimators were not run on the tranche; the S4 arms were not distinct served policies in 60 slots and the oracle participant makes the A/B contrast degenerate. Phase 1 therefore commits the scratch drivers, adds records-in cross-checks and runs the landed admission code honestly before writing the record.
- **A-3 · Funding.** Staged: pilot → measured quote → operator gate, never open-ended. The operator holds **$60 of Ollama Cloud credit** to start; it is a *pilot* budget only (tranche size and any further credit decided at a gate). Container compute is quoted separately.
- **A-4 · Participant model and architecture.** Open models via **Ollama Cloud**, constant model `glm-5.3-flash`; external harnesses run **inside HH cells** (installed-agent adapter C) with every model call mediated by an **HH model proxy** (adapter B). Credential: `OLLAMA_API_KEY` in the git-ignored `.env` of the operator's checkout — `provided: yes`; its value is never recorded.
- **A-5 · Benchmark.** **Terminal-Bench 2.1** tasks run through **Harbor** as the original-runner surface (the role `tb` played in S3); a $0 gold-script re-proof of the execution surface on the selected 2.1 tasks precedes any paid run. Terminal-Bench 1.0 (`terminal-bench-core==0.1.1`) remains only the subject of the phase-1 record.
- **A-6 · Results website.** A static site generated from committed records, deployed to GitHub Pages behind a publication gate. "I'd also like it to be beautiful" — primary inspiration the HF harness leaderboard (`huggingface.co/spaces/joelniklaus/harness-leaderboard`), secondary styling inspiration Artificial Analysis's coding-agents page (`artificialanalysis.ai/agents/coding-agents`); "as beautiful as either of those, but unique and appropriate for the HarnessHarness repo". Content: leaderboard with CIs and ties, noise analysis, per-task grid with trajectories, methodology and honesty pages, "plus whatever else you critically decide makes sense".
- **A-7 · Harness set.** Terminus-2, mini-swe-agent, at least one heavyweight OSS agent and Claude Code (closed) — "all of the above, depending on how much these experiments cost", ideally several OSS harnesses, chosen by research beyond the HF board's set.
- **A-8 · Phases 3–4.** HIR targets mini-swe-agent then Terminus-2 (staged); layered parity (deterministic request-level conformance + paired task-verdict KL parity + descriptive trace shape); sweeps are one-factor-at-a-time exploration on development tasks, then pre-registered held-out confirmation.
- **A-9 · Publication posture.** Each phase lands an in-repo record labelled internal / not outside-verified; card-state transitions are encoded in `check-position.sh`; the Pages site deploys only after a per-phase publication gate (HUMAN publication row).
- **A-10 · Operations.** A fresh GCE VM per paid phase from a committed recipe, with an unskippable teardown at phase end; research proceeds autonomously with a summary per stream, pausing only for genuine operator judgement or approval.

## 4. Fixed constraints (⚑)

- ⚑ The canonical spec and the ratified evidence-and-adoption supplement are frozen; changes go through ADR + amendment protocol. This program writes a **new** supplemental spec.
- ⚑ No spend, provisioning, publication, merge, or external write is authorized by this brief. Spend requires a recorded per-gate authorization; merges stay `mergePolicy: OPERATOR`.
- ⚑ No new dependency without an ADR (root `AGENTS.md`).
- ⚑ Honest refusal, append-only history, typed dispositions, durable-before-visible; never relabel a disposition; an unexecuted study is unmet, not null.
- ⚑ Secrets never enter any record (`provided: yes/no` only).
- ⚑ The round-5 closure (GATE-G19 → R5.59) is not raced; the manifest extension is written on top of `main` after R5.59 lands.

## 5. Success

The spec is ratified, decomposes into a Round-6 manifest extension whose every
ticket fits one fresh-context `implement-spec` run, and carries the admission
scaffolding (budget records, model admissions, parity declarations, publication
gates) each phase needs — so that an honestly negative or inconclusive result
would be as publishable as a positive one.
