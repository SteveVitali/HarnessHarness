# CAP7.1 — Pass terminal-state gap analysis (manifest 335)

Disposition: **done — executed, offline (USD $0.00)**. Records-only capstone:
per-id verdicts over the 18 routed §8.x ids, the pass's honest terminal-state
determination, DF-R6.45l-1 discharge verification, and the seam/decision sweep.
No window, spend, gate signature, or O-3 mint in scope.

- Manifest: `docs/tickets/335_CAP7.1__gap-analysis.md` (335).
- Branch `codex/harnessharness-r6.53`; depends on R7.6 + HUMAN-H21 (both landed).
- Harness: devin-cli / swe-2-high / subagent (LEDGER `harness:` verbatim).
- Routing basis: `docs/build/reports/round6/LATER_PASS_ROUTING.md` — 34 routed
  ids, the 18 §8.x subset is this pass's; a routed id is routed, never
  satisfied in Round 6.

## What landed

- **`docs/build/reports/2026-11-26-round7-gap-analysis.json`** —
  `hh-round7-gap-analysis/1` `764ec84f…` (+`.sha256`) — the canonical
  terminal-state record: 18-row verdict table, counts, the pre-registered
  determination, the DF-R6.45l-1 verification block, the sweep table.
- **`docs/build/reports/2026-11-26-round7-gap-analysis.md`** (+`.sha256`) —
  the readable companion (same verdicts + sweep rows in table form).

## The verdict table (summary — full citations in the record)

**16 satisfied · 2 typed_terminal · 0 unsatisfied · 0 pending · 0 absent.**

- satisfied: REQ-XD-225 (§8.1 tier-2 admission), REQ-XD-204 (§8.3 effort-pin
  fidelity — served census 10,602 receipts, 0 mismatches), REQ-XD-206 (§8.4 X
  design — X0 24/24 + X1 440/440 executed), REQ-XD-207 (§8.4 labels),
  REQ-XD-208 (§8.2 activation fold — measured + typed `unmeasured` residual
  rowed DF-R7.1-2), REQ-XD-209 (§8.2 inert forecast), REQ-XD-210 (§8.4 stops —
  `withheld{stop:budget}` ×222 exercised live), REQ-XD-211 (§8.5 frozen rule
  byte-pin `67009098…`), REQ-XD-212 (§8.5 packet `9c3c314c…` hash-predates
  `1d55d48a…`), REQ-XD-213 (§8.5 optimism labels), REQ-XD-215 (§8.6 fresh
  baseline arm, no reuse), REQ-XD-216 (§8.6 Holm P4-C1 not-rejected,
  unavailable retained), REQ-XD-217 (§8.6 c-prereg before subjects),
  REQ-XD-218 (§8.6 X1-measured quote, executed 35.9%/31.6%+40.4%), REQ-XD-219
  (§8.6 `inconclusive{unresolved_cell}` typed), REQ-XD-221 (§8.7 gate order —
  G35 PASSED + X1 amendment + G36 PASSED + resume amendment).
- typed_terminal: REQ-XD-205 `withheld{inert_at_admissible_window}`
  (F(32768)=0.22 < 0.25 — V8/V9 declared, never sealed, never run);
  REQ-XD-214 `not_admitted{m_eq_1}` (C₁₂ pre-declared; `selected{m=1}` mints
  no combination member; admissibility refusals committed).
- No routed id's verdict depends on a billed figure — the P4-C ≥24 h provider
  confirming read is `pending{clock}` verbatim (HUMAN-H21 settlement hygiene),
  not a routed-id dependency.

## Terminal-state determination (pre-registered form)

> **`complete`**

Every routed §8.x id carries a satisfied verdict on committed evidence or a
spec-sanctioned typed terminal disposition; zero unsatisfied/pending/absent.
CAP7.2's conditional O-3 remainder set carries **no §8.x additions** on this
record's evidence. Carried debt is named verbatim in the record (DF-R7.1-2,
DF-R7.3-1/-2/-4, DF-R7.5-1/-2, new DF-CAP7.1-1/-2, `pending{clock}`, the
site-publication pin) — typed and rowed, never silently absent.

## DF-R6.45l-1 discharge verification

**Stands.** Dated DONE (DEFERRALS status-update row 2026-11-19; BUILD_INDEX row
321a) verified against the landed artifacts on PR #413's head (`6b14480b`):
`verify_hosts.rs` + `fp_verify_hosts` + `tests/r6_51.rs` + the committed
`hh-verify-host-derivation/1` record emitting `hosts: [download-r2.pytorch.org,
ossci-datasets.s3.amazonaws.com, yann.lecun.com]` — exactly the three
smoke1-observed hosts, unaided with provenance (`download.pytorch.org` honestly
under `non_verify_denies`). Branch containment verified by `git merge-base`
against `refs/pull/413/head`: ancestor of `origin/main` (merge `3a03b4d2`),
**not** an ancestor of this chain @ `6e7e166b` — **PR #413 rides `main`, NOT
an ancestor of this chain** — an integration fact, never owed work. The stale
OPEN lead token is recorded for CAP7.2's reconciliation, not flipped here.

## Seam/decision sweep — findings rowed

- **DF-CAP7.1-1 (minted)** — `slot_result.cost_micros` snapshots undercount
  late-landing `measurement.cost.attributed` events (X1 6,099,597 vs 6,202,514;
  P4-C 16,638,034 vs WAL 16,896,165); honest totals need the retained store,
  which sits outside the run-root harvest path.
- **DF-CAP7.1-2 (minted)** — verify-cell podman container-name collision
  across resumes (`p4c-V1-model-extraction-relu-logits-r4` rc 125 →
  `recorded{reward_absent}`; one of the three UnresolvedCell causes).
- **Dated notings appended** — DF-R7.3-2 (named unblocker H21 landed; compute
  `unavailable` → residuals stand as recorded estimates, row OPEN) and
  DF-R7.3-4 (leg (b) fails-closed repair landed at R7.5; leg (a) `rroot`
  derivation outstanding, row OPEN).
- **`docs/build/runs/R7.5.md` dated correction** — the 2026-11-25T01:00Z
  intervention addendum is duplicated verbatim (~lines 136–174); events
  occurred once; terminal counts govern.
- Carried verbatim, no new rows: stale lead tokens (DF-CAP6.1-1…5,
  DF-R6.45l-1, DF-R6.50-1, DF-R6.53-1, DF-R7.1-1, DF-G35-1, DF-R7.3-3 —
  CAP7.2's named reconciliation scope); the G36 Status-line repair `e784f583`
  + decision-row restatement `dba6b08d`; the GH013 commit-time scrub; the
  site-publication pin; `pending{clock}`.

## Verification (executed)

- `bash docs/build/tools/check-supplement-guard.sh --no-checker` — **PASS**,
  route `DISPATCH — CAP7.1` verified against committed HEAD (the pre-write
  state — `git archive HEAD` scratch checkout) and **PASS**, route
  `DISPATCH — CAP7.2` on the post-write working tree verbatim (the route
  advances past CAP7.1 — index row 335 + `lastCompleted: CAP7.1` /
  `nextTicket: CAP7.2` are consistent).
- `bash scripts/check-bm-tools.sh` — **OK**.
- `bash scripts/deferrals-open.sh` + `bm-row.sh` reads for every DF row cited
  (DF-R6.45l-1, DF-R7.1-1/-2, DF-R7.3-1/-2/-3/-4, DF-R7.5-1/-2, DF-G35-1);
  new rows re-read post-mint (19 open rows, one row each).
- `git fetch pull/413/head` → `6b14480b`; `merge-base --is-ancestor` vs
  `origin/main` → true, vs HEAD → false.
- `shasum -a 256 -c` sidecars re-verified on the two new reports.
- `git diff` reviewed on every shared-register write (BL-41) before commit.
- Not run: cargo suites / check-fp-* emitters — records-only ticket, no code
  or emitted artifact changed; each fold/emit checker last recorded green at
  its owner's landing. External build-memory validator: `AGENT_SKILLS_ROOT`
  not set in this environment — recorded as not-run, not a pass.

## Deferrals

- Opened: DF-CAP7.1-1, DF-CAP7.1-2. Closed: none (nothing this pass unblocks —
  closable-row disposition is CAP7.2's reconciliation). Carried OPEN:
  DF-R7.1-2, DF-R7.3-1/-2/-4, DF-R7.5-1/-2 + older set.

## Deviations / notes

- LEDGER/BUILD_INDEX/DEFERRALS written by python per BL-41; `git diff`
  verified before commit.
- Spend USD $0.00; no live leg, no gate act, no merge.
