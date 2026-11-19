# AGENT_DOCS_CHECK — DOC6.1 stage 2 (DOC.2)

Record class: ticket artifact · authored 2026-11-19 (UTC) · branch
`codex/harnessharness-doc6.1` · stage commit `361f46a6` (agent docs) ·
offline, USD $0.00. Stage 2 invoked `agent-docs` **refresh** mode
(`~/.claude/skills/agent-docs`, scope `.`, no `--deep`) and followed it
completely.

## Freshness detector (phase 0 → validate)

`bash ~/.claude/skills/agent-docs/scripts/check-agent-docs-freshness.sh .`

- **Phase 0:** 5 docs scanned (`AGENTS.md`, `crates/AGENTS.md`,
  `docs/AGENTS.md`, `clients/AGENTS.md`, `foreign/AGENTS.md`) —
  **0 issues found** (0 critical / 0 moderate / 0 minor;
  `/tmp/agent-docs-freshness.json`).
- **After edits (shared tail 2.1):** 5 docs scanned — **0 issues
  found**. AC-2 holds: 0 critical; the root "Build memory" section is
  present (deferrals-first, append-only, stacked-PR rules intact).

With 0 structural issues the refresh mode reports fresh; the ticket's
deliverable additionally required the hierarchy to carry the landed
Round-6 surface **where docs scope requires it** — surgical,
claim-verified additions only (no re-voice, no template churn):

| Doc | Addition | Verified against |
|---|---|---|
| `AGENTS.md` (root) | Architecture rows: experiment apparatus — `crates/hh-runner/` (coordinator + slot workers + `fp_*` emitters), `hh-model-proxy` (`hharness-hosting`), `allowlist_mediated` egress + `hh-cell-relay` (`hh-env`/`hh-helper`); results site — `scripts/site/`, `docs/results/site-data/`, `docs/results/publication/`, `check-site.sh` | per-crate `Cargo.toml` `[[bin]]` entries; `crates/AGENTS.md` map row for `hharness-runner`; `scripts/check-site.sh` usage line |
| `AGENTS.md` (root) | Build memory: Round-7 entry points — `docs/build/reports/round6/LATER_PASS_ROUTING.md` (34 routed ids, owner+trigger, "routed is never satisfied") and `docs/build/planning/2026-11-19_round7-decision-memo.md` | both files exist committed; CAP6.2/CAP6.3 run records |
| `crates/AGENTS.md` | `hharness-helper` row notes `hh-cell-relay`; `hharness-hosting` row notes `hh-model-proxy` | `[[bin]]` names in each Cargo.toml |
| `docs/AGENTS.md` | Build & Test gains `check-site.sh --profile internal|public` (+`--head` guidance) and names `check-publication.sh`/`site/build.py --emit` | `check-site.sh` usage line; `docs/README.md` mode table |

Consistency sweep (2.2): the new root rows name the same binaries the
crates map rows now carry; `docs/AGENTS.md`'s added checker matches the
mode table's `site-data/` generated rule — no contradictions introduced.
CLAUDE.md bridge (2.3): `CLAUDE.md` is the `@AGENTS.md` import — already
present, nothing to add.

## Exact remaining state

- Frozen/historical/append-only surfaces were **not** touched (mode
  table honoured): `docs/build/`, `docs/tickets/`, `docs/adr/README.md`
  (generated — currently in sync per DF-CAP6.1-5's DONE row),
  `spec/`, `research/`.
- `docs/build/logs/` ignored; nothing force-added.
- No coordinator-owned files were written (LEDGER / BUILD_INDEX /
  DEFERRALS). Proposed text for the round-closure entries lives in
  `docs/build/runs/DOC6.1.md`.
- Owed-work state at this record: every DF row's latest dated
  disposition is DONE (the six lead-OPEN tokens reconcile at round
  closure — coordinator-owned per MAINTAINING.md); BL-50 stays owed to
  the operator under GATE-G32 (b); routed later-pass scope stays
  `routed, never satisfied` per `LATER_PASS_ROUTING.md`. No blanket
  "done" is claimed while those stand.
- `clients/AGENTS.md` and `foreign/AGENTS.md` required no edits —
  structurally fresh and their scoped claims verify (R6.38's
  `open_session.exec` is generated-surface detail, not agent-doc
  scope).
