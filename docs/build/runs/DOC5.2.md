# Run ledger — DOC5.2

- Spec: `docs/tickets/246_DOC5.2__doc2.md` (seq 246 of 246 · Phase Round 5 · Kind docs · Role DOC.2)
- Base: `ef62b135…` (codex/harnessharness-evidence-doc5.1 actual tip — DOC5.1 change `88e07858` + close `ef62b135`; O-02 actual SHA at SETUP)
- Branch: `codex/harnessharness-evidence-doc5.2`
- Config: `implement-spec spec=docs/tickets/246_DOC5.2__doc2.md` (autonomous, ledger on; docs-only — offline, USD $0, no production mutations)
- Harness: devin-desktop/swe-2-high/subagent
- Skills: agent-skills @ e981ec5
- Started: 2026-11-05T02:03:15Z (`date -u` at SETUP)
- Context at SETUP: ticket estimate 18k + 10k reserve; loaded = contract 246 + invariants I-01–I-08/O-01–O-08 + supplement s9 + manifest row 246 + 3 OPEN DEFERRALS rows + LEDGER CURRENT STATE + BUILD_INDEX tail + GATE-G17 record (via PHASE LOG/CAPSTONE_CLOSURE) + all five AGENTS.md files + docs/README.md + MAINTAINING.md + CONTRIBUTING.md + the docs/build/tools README + ci.yml + every script the agent docs name (wc -l + header reads) ≈ at estimate; ≤550 changed source/test lines + 24k/10k split trigger watched (docs-only scope).

## Deferrals read

`bash scripts/deferrals-open.sh` at 2026-11-05T02:0Z —
`DF-R5.1-1 | OPEN` (GATE-G17 contributor evidence, ten R5.1-owned targets),
`DF-H9-1 | OPEN` (withheld participant/outreach predicate set; minted CAP5.1),
`DF-CAP5.1-1 | OPEN` (failed S1 useful residual; minted CAP5.1).
GATE-G17's verbatim scope disposition (2026-11-05T00:13:07Z signature):
all three **carry forward OPEN** — no waiver named. This docs refresh
records them as open; it closes none and invents none.

## Orient findings

- Guard pre-dispatch: `bash docs/build/tools/check-supplement-guard.sh`
  → `hh-supplement-guard/1` v1.0.0 **PASS — route DISPATCH — DOC5.2**
  (82 active-round rows / 180 candidates / 0 violations / 8 baseline
  warnings); input digests recorded by the tool.
- Structural detector `check-agent-docs-freshness.sh .` (agent-docs
  skill): 5 docs scanned, 0 issues — all file references resolve; the
  drift is **semantic**, so the refresh ran the claim-level audit the
  ticket names (every claim verified against source/committed records).
- Accepted record (GATE-G17 PASSED 2026-11-05T00:13:07Z): engineering
  `executed` · research `inconclusive` · portability/public/adoption
  `withheld` · `build_status: not_done`.
- Round-5 landed machinery the agent docs predate: the retained
  supplemental coverage/routing guard + guarded dispatch driver under
  `docs/build/tools/` (R5.1/R5.2), the doc-surface checkers
  `check-position.sh`/`check-examples.sh`/`check-trace.sh`/
  `check-contributing.sh` each with a `--head` committed-HEAD mode
  (R5.30/31/35/45), `run-examples.sh` (R5.30 offline lane),
  `check-quickstart.sh` (L4.7), `check-publish-metadata.sh` +
  `check-docsrs.sh` (ADR-0380), `check-licenses.sh`,
  `check-live-evidence.sh` (ADR-0363 D9), `check-ci-gating.sh` +
  `scripts/ci/test-check-*.sh` self-tests. None of these is named in
  the applicable AGENTS.md guidance for the surfaces they pin.

## Requirements

Refresh applicable agent instructions (the AGENTS.md hierarchy plus the
ownership/verification guidance they point at) with actual owners,
narrow verification and gate/guard rules — matching the accepted
round-5 record; no duplicating sources, no rewriting historical
evidence; validate new round records and final state truthfully.
New artifact `docs/build/reports/round5/AGENT_DOCS_CHECK.md`.

## Acceptance criteria

- AC-1 (D/E; positive): refresh agent instructions with actual owners,
  narrow verification and gate/guard rules; no source duplication, no
  historical rewrite; validate round records + final state truthfully.
- AC-2 (D/E; negative): no frozen/historical edits, invented tool
  path, stale runtime authority, broad test or unguarded resume
  instruction; uncompleted scheduled/human work stays visible.
- AC-3 (D/E; ownership+evidence): primary targets — none (contributing
  slice). Record achieved D/E separately from owed R/P; no full-target
  predicate claimed satisfied.
- AC-4 (E/D; scoped verification): fails-without for new behavior;
  docs-only uses living-docs/row/link checks; exact commands + counts;
  missing prerequisite is not a pass; scratch stores.
- AC-5 (D/E; phase gate): green in scope; requirement IDs stamped
  (none); owed items stay DEFERRALS rows; run/PR/BUILD_INDEX/LEDGER
  agree; ≤2KiB phase log; no deferred predicate becomes MET by close.

## Plan

1. Claim-level audit of the five AGENTS.md files + the pointed-at
   ownership/verification guidance (MAINTAINING.md) against actual
   code, manifests, scripts, CI and the committed round-5 record.
2. Surgical edits only: name the landed checkers and the retained
   guard where an agent acting on that surface would need them;
   correct measured line-count drift past tolerance.
3. `reports/round5/AGENT_DOCS_CHECK.md` — the contract artifact:
   claim-by-claim audit, executed checks, committed-HEAD scope.
4. Verify: living-docs, check-position/examples/trace/contributing
   (docs surfaces untouched by intent — they must stay green),
   check-bm-tools, guard post-artifacts, fmt, freshness detector re-run.
5. Commit, PR (base doc5.1), ci-boundary → `logs/ci-doc5.2.json`,
   closeout (BUILD_INDEX 246 / LEDGER DOC5.2→none (last row) / phase log).

## Stale claims found and corrected

| surface | stale claim | refreshed to |
|---|---|---|
| `AGENTS.md` Build & Test | docs checks = `check-living-docs.sh` + quickstart only | adds the four doc-surface checkers with `--head` modes and `run-examples.sh` |
| `AGENTS.md` Build memory | no mention of the retained routing guard | guard invocation + `PASS`/`DISPATCH` route requirement before/after boundaries, `HOLD` = RETURN PASS |
| `AGENTS.md` Key files | `ci.yml` ~300 lines | ~350 (actual 340) |
| `docs/AGENTS.md` Build & Test | battery predates the round-5 doc checkers | adds check-position/examples/trace/contributing + `--head` semantics + quickstart/run-examples scope |
| `docs/AGENTS.md` gotchas | no warning about pinned mutation points | gotcha: checkers' self-tests mutate exact phrases (unmet clause, gated-lane, marked blocks) |
| `crates/AGENTS.md` Key files | `hh-embed/src/lib.rs` 71 | 72 (actual) |
| `MAINTAINING.md` tools | no guard/merge-dryrun pointer | `docs/build/tools/` guard+driver pointer; `merge-dryrun.sh` named |
| `MAINTAINING.md` Releasing | publish readiness checks unnamed | `check-publish-metadata.sh` + `check-docsrs.sh` (ADR-0380) |

## Claims verified unchanged (audit, no edit)

- Root: purpose/spec freeze; architecture table crate paths +
  `EmbedService::handle`; `Cargo.toml` ~550 (533); codegen ~150 (148);
  `docs/README.md` ~100 (111); Rust 1.94 pin; build/test/fmt/drift/
  removability/bm-tools commands; scoped-package examples;
  `cap_2_composed`; doctor connectivity/HH_* env vars (`boundary.rs`,
  `cli.rs`); dependency admissions (`ring =0.17.14` hh-secrets,
  rustls graph hh-tls, tempfile dev-dep hh-varhost — manifests read);
  CC7/CC10/durable-before-visible/honest-refusal/tier features
  (tier-c1 env/helper/ledger, tier-c4 embed — manifests read);
  generated-file list; deferrals-first + `bm-row.sh` usage;
  kernel_version_label semantics (hh-embed-schema:137); write-integrity
  rule; live-evidence class rules (reports/live/ + live-verify.sh);
  manifest-workflow scope; logs/ gitignored; dated-marker status rule;
  stale-helper + spawn-deadline env overrides (helper.rs:155–185);
  ci-mode exact rules (ci.yml header + ADR-0362); mergePolicy OPERATOR.
- crates/: 54 workspace members (Cargo.toml parsed — map complete,
  facade included); crate descriptions spot-checked against package
  `description`s; dependency/tier/class-table conventions; gotchas
  incl. spikes exclusion + >40 KB bounded-read rule.
- docs/: key-file line counts within rounding (bm-row 178→~200,
  spec-section 71 exact, check-live-evidence 159→~150); edit-mode
  table; generated-file regeneration commands (`adr-index.sh`,
  `gen-revisit-triggers.sh`, `gen-third-party-notices.sh` all exist);
  committed-HEAD semantics for `--head`/quickstart.
- clients/: hh-embed.ts ~4.7k (4692); smoke.ts ~150 (154);
  emit_ts.rs ~800 (794); e3-smoke.sh ~100 (83 → ~100 per rounding);
  exit 3 node-absent/<22, exit 2 missing kernel — verified in script;
  fixture-definition.json present; node:http transport rule.
- foreign/: CHECKER.md ~200 (218 → ~200 per rounding); foreign.py
  ~3.6k (3630); foreign.ts ~50 (58 → ~50); xcheck-foreign.sh ~150
  (141 → ~150); xcheck-e3.sh exists; exit 2 missing checker, exit 3
  missing runtime — verified; committed answers.json ×2 present.

## Progress

- 2026-11-05T02:03Z SETUP — deferrals read (3 OPEN, carried verbatim);
  branch `codex/harnessharness-evidence-doc5.2` @ `ef62b135`; guard
  PASS DISPATCH DOC5.2 (82/180/0 viol/8 warn); freshness detector
  5 docs / 0 structural issues → semantic claim audit performed
  (every claim above verified against source/manifests/scripts/CI).

## Gap table

| item (spec §) | required | achieved | status | evidence |
|---|---|---|---|---|
| AC-1 refresh agent instructions w/ actual owners, narrow verification, gate/guard rules; no duplication/rewrite; round records validated truthfully | D/E | engineered | met | four living files refreshed (AGENTS.md, crates/AGENTS.md, docs/AGENTS.md, MAINTAINING.md) + `reports/round5/AGENT_DOCS_CHECK.md`; every added command read from its own script; pointers (not copies) to `docs/build/tools/README.md`; clients/foreign audited with zero drift |
| AC-2 no frozen/historical edits, invented tool path, stale runtime authority, broad test or unguarded resume instruction; uncompleted work stays visible | D/E | engineered | met | only living files touched; every named script exists + contract read; guard text names the DISPATCH precondition (narrows, never unguards); three OPEN deferrals carried verbatim; no card state promoted |
| AC-3 primary targets → checks/evidence; achieved D/E vs owed R/P | D/E | engineered | met | none — contributing slice; the slice's own D/E recorded here; no full-target predicate claimed; R5.1's guard contributor predicate stays DF-R5.1-1 OPEN |
| AC-4 fails-without; docs-only living-docs/row/link checks; commands + counts | E/D | engineered | met | the checkers pin the surfaces these edits describe — a weakened unmet clause or dropped marked block flips check-position/examples/self-tests; counts in evidence log |
| AC-5 universal phase gate | D/E | engineered | met | run/PR/BUILD_INDEX/LEDGER agree at closeout; phase log ≤2KiB; three OPEN deferrals carried verbatim; ci recorded at close |

## Invariant / operating-clause gap rows

- **I-01** held — frozen sources untouched (spec, research, packets,
  readouts, ADRs); living surfaces only; no schema/generated edits.
- **I-02** held — docs-only; no kernel semantics touched.
- **I-03..I-05** n-a — no subject/grade/treatment/method surface.
- **I-06** held — dispositions quoted verbatim from the accepted
  record (`executed`, `withheld`, `unselected`, `reported`, `failed`);
  no promotion.
- **I-07** held — D/E recorded separately from owed R/P; the guard's
  contributor predicate stays owed (DF-R5.1-1); no target claimed.
- **I-08** held — no optional exercise awards a claim; $0; no
  inherited permissions; merge authority stays OPERATOR.
- **O-01** held — deferrals first; bounded reads; `date -u` stamps.
- **O-02** held — single writer; forked actual doc5.1 tip `ef62b135`;
  shell/python writes + git-diff verification on every write.
- **O-03** held — no gate inferred; GATE-G17's verbatim dispositions
  carried; `HOLD` documented as RETURN PASS.
- **O-04** held — guard PASS DISPATCH DOC5.2 pre-dispatch and re-run
  post-artifacts (identical input digests — no state touched).
- **O-05** held — docs battery + freshness detector; counts recorded;
  no runtime claimed.
- **O-06** held — current-head CI read at close; pending/unknown
  never called green.
- **O-07** held — stock validator + row tools + retained guard run
  separately; historical records append-only.
- **O-08** held — no production mutation; credentials never recorded.

## Evidence log

- `bash scripts/deferrals-open.sh` → 3 OPEN (DF-R5.1-1, DF-H9-1, DF-CAP5.1-1).
- `bash docs/build/tools/check-supplement-guard.sh` → PASS, route
  DISPATCH — DOC5.2 (82 rows / 180 cand / 0 viol / 8 warn), pre- and
  post-artifacts (identical input digests).
- `check-agent-docs-freshness.sh .` → 5 docs scanned, 0 issues
  (pre- and post-edit).
- Claim verification reads: `Cargo.toml` parsed (54 members; crate
  map complete); `rust-toolchain.toml` channel 1.94; manifests read
  for `ring =0.17.14` (hh-secrets:43), rustls+webpki (hh-tls:26–27),
  tempfile dev-dep (hh-varhost dev-deps), tier-c1/tier-c4 features;
  `kernel_version_label` hh-embed-schema:137; `EmbedService::handle`
  service.rs:610; HH_* env vars hh-cli boundary.rs/cli.rs;
  HH_HELPER_*_DEADLINE_MS hh-env/src/helper.rs:155–185; every named
  script's header/body read; ci.yml mode convention + step gates;
  agent-skills script paths (`check-backlog.sh`, `merge-dryrun.sh`,
  `adr-index.sh`) verified to exist; committed `answers.json` ×2 and
  `fixture-definition.json` present.
- `wc -l` audit vs documented approximations: ci.yml 340 (~300→~350
  corrected); `hh-embed/src/lib.rs` 72 (71→72 corrected); all other
  counts within guideline rounding.
- `bash scripts/check-living-docs.sh` → OK — 456 living files, no
  personal paths.
- `bash scripts/check-position.sh` → OK — 13 legs green.
- `bash scripts/check-examples.sh` → OK — 4 cards, 6 pins recomputed,
  29 runner check ids, displayed block equivalent.
- `bash scripts/check-trace.sh` → OK — all legs green.
- `bash scripts/check-contributing.sh` → OK — 12 check ids green
  (no_ritual leg holds — contributor path unimposed).
- `bash scripts/check-bm-tools.sh` → OK.
- `bash scripts/ci/check-ci-gating.sh .github/workflows/ci.yml` → OK
  (14 hits allowlisted).
- `cargo fmt --all --check` → clean (docs-only; no Rust touched —
  run per AC-4 anyway).
- `check-build-memory.sh .` → 0 violations (8 baseline warnings —
  same class set the guard records as its scoped baseline).
- `git diff` verified after every write (write-integrity rule).

## Evidence report

(to fill at close)
