# DOC5.2 — AGENT_DOCS_CHECK (round 5)

> The DOC.2 record: agent-facing documentation (the `AGENTS.md`
> hierarchy plus the ownership/verification guidance it points at)
> refreshed **against the accepted round-5 record** — the GATE-G17
> accepted state (PASSED 2026-11-05T00:13:07Z) and the landed round-5
> machinery it accepted. Executed things documented as executed;
> withheld/unmet work stays visible; nothing here is a claim beyond
> what the records support. Offline-only; USD $0.

- **Ticket:** `docs/tickets/246_DOC5.2__doc2.md` (seq 246 of 246 — the
  final manifest row)
- **Base:** `ef62b135` (codex/harnessharness-evidence-doc5.1 tip)
- **Run ledger:** `docs/build/runs/DOC5.2.md`
- **Mode:** living-docs refresh; frozen/historical/append-only
  surfaces report-only. Only `AGENTS.md`, `crates/AGENTS.md`,
  `docs/AGENTS.md` and `MAINTAINING.md` were edited; `clients/` and
  `foreign/` AGENTS.md audited with zero drift found.

## Method

1. Structural pass: the `agent-docs` freshness detector
   (`check-agent-docs-freshness.sh .`) — **5 docs scanned, 0 issues**:
   every referenced path resolves. Structural freshness ≠ semantic
   freshness; the audit continued claim-by-claim.
2. Semantic pass (the ticket's "verify every claim"): each behavioral
   claim in the five agent docs and the pointed-at ownership/
   verification guidance was checked against source manifests,
   script headers/bodies, `ci.yml`, the workspace `Cargo.toml`
   (parsed, 54 members), and the committed round-5 records.

## What round 5 added that the agent docs predated

| Landed round-5 machinery | Was it named in agent guidance? | Now |
|---|---|---|
| Retained supplemental coverage/routing guard `docs/build/tools/check-supplement-guard.sh` + guarded dispatch driver (R5.1/R5.2; `docs/build/tools/README.md`) | **No** — no AGENTS.md named it | Root build-memory bullet: `PASS` + `DISPATCH — <id>` route required before/after boundaries; `HOLD` = RETURN PASS. MAINTAINING tools section points at `docs/build/tools/README.md` |
| Doc-surface checkers `check-position.sh` / `check-examples.sh` / `check-trace.sh` / `check-contributing.sh`, each with `--head` committed-HEAD mode (R5.30/31/35/45) | **No** — docs/AGENTS battery predated them | docs/AGENTS Build & Test names all four with their pinned surfaces and `--head` semantics; root docs-checks line references them |
| `run-examples.sh` offline example lane (R5.30) | No | Named in both docs-check lines (scratch stores) |
| Checker self-tests `scripts/ci/test-check-*.sh` that mutate pinned phrases | No | New docs/AGENTS gotcha 6: preserve mutation points — never weaken an `unmet` clause, promote a closed card state, or drop a marked block |
| `check-publish-metadata.sh` + `check-docsrs.sh` (ADR-0380 publish readiness, CI full path) | No | Named in MAINTAINING Releasing |
| `merge-dryrun.sh` (reconcile-build, used by REC5.3) | No | Named in MAINTAINING build-memory tools |

## Stale claims found and corrected

| surface | stale claim | refreshed to |
|---|---|---|
| `AGENTS.md` Key files | `ci.yml` ~300 lines | ~350 (actual `wc -l`: 340) |
| `crates/AGENTS.md` Key files | `crates/hh-embed/src/lib.rs` 71 | 72 (actual) |

## Claims verified unchanged (no edit — audit record)

- **Root AGENTS.md** — architecture table crate paths and
  `EmbedService::handle` (`service.rs:610`); `Cargo.toml` ~550 (533);
  `hh-codegen/main.rs` ~150 (148); `docs/README.md` ~100 (111);
  Rust 1.94 + rustfmt pin (`rust-toolchain.toml`); every Build & Test
  command exists; `cap_2_composed` acceptance example; doctor
  `connectivity:ok` + `HH_KERNEL_CMD`/`HH_STORE_ROOT`/
  `HH_WORKSPACE_ROOT` (`hh-cli/src/boundary.rs`, `cli.rs`);
  dependency admissions read from manifests (`ring =0.17.14`
  hh-secrets, rustls+webpki hh-tls, tempfile dev-dep hh-varhost);
  feature tiers (`tier-c1` in env/helper/ledger, `tier-c4` embed);
  generated-file list; `kernel_version_label` (`hh-embed-schema:137`);
  live-evidence rules (`scripts/live-verify.sh` →
  `docs/build/reports/live/`, 40+ records present); spawn-deadline
  overrides (`hh-env/src/helper.rs:155–185`); CI path-mode rules
  (ci.yml header convention + `check-ci-gating.sh`); `mergePolicy:
  OPERATOR`; logs/ gitignored; dated-marker status rule; manifest
  fork-from-preceding-branch rule.
- **crates/AGENTS.md** — 54 workspace members parsed from
  `Cargo.toml`; the crate map covers all 54 (facade `hharness`
  included); key-file counts within rounding; CC/tier conventions;
  spikes exclusion; >40 KB bounded-read rule.
- **docs/AGENTS.md** — key-file counts within rounding
  (`bm-row.sh` 178→~200, `spec-section.sh` 71 exact,
  `check-live-evidence.sh` 159→~150); edit-mode conventions;
  generator commands all exist (`adr-index.sh`,
  `gen-revisit-triggers.sh`, `gen-third-party-notices.sh`).
- **clients/AGENTS.md** — `hh-embed.ts` ~4.7k (4692), `smoke.ts`
  ~150 (154), `emit_ts.rs` ~800 (794), `e3-smoke.sh` ~100 (83→~100);
  exit 3 = node absent/<22, exit 2 = missing kernel (script read);
  `fixture-definition.json` present.
- **foreign/AGENTS.md** — `CHECKER.md` ~200 (218→~200), `foreign.py`
  ~3.6k (3630), `foreign.ts` ~50 (58→~50), `xcheck-foreign.sh` ~150
  (141→~150); exit 2 = missing checker, exit 3 = missing runtime;
  `xcheck-e3.sh` exists; committed `answers.json` ×2 present.
- **MAINTAINING.md** — evidence-class ladder and ADR-0363 live rules;
  example-check table (all three scripts exist); write-integrity
  rules; `check-backlog.sh` path exists in agent-skills; RK-06
  register path `research/registers/spec-debt.md` exists.

## Checks executed (working tree, actual counts)

(recorded in the run ledger's evidence log — living-docs, the four
doc-surface checkers, bm-tools, fmt, guard post-artifacts, freshness
detector re-run.)

## Committed-HEAD scope

`check-*.sh --head` variants and `check-quickstart.sh` read committed
HEAD in a clean clone — they validate the committed tree, never the
uncommitted working tree. This change touches no surface those
checkers pin (no README/quickstart block, no evidence card, no
contribution route, no trace page); the working-tree battery still ran
and is recorded in the run ledger.

## What this refresh does not claim

- No card state or capability was promoted: the GATE-G17 accepted
  `not_done` record stands; S1/S3/S4 remain unmet and the withheld
  stages remain withheld — nothing in the agent docs claims otherwise.
- The guard is documented as a **dispatch precondition**, not as a
  completed acceptance: `DF-R5.1-1` (its round-5 contributor-evidence
  predicate) stays OPEN alongside `DF-H9-1` and `DF-CAP5.1-1` — all
  three carry forward per GATE-G17.
- No contributor-path ritual was added: the guard/check guidance is
  scoped to manifest build tickets and docs edits respectively; the
  ordinary contributor path in CONTRIBUTING.md is unchanged.
- No unverified command was documented: every script named in the
  refreshed guidance exists and its contract was read from its own
  header/body.
