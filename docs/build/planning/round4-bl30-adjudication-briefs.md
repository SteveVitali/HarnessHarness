# BL-30 adjudication briefs (R4.15, 2026-10-22): the 7 open CFs + OQ-170 custody + OQ-388 ratification — BL-30's REC.2-scope rulings

<!-- Brief-file convention OWNED BY R4.8 (docs/build/planning/round4-bl33-ws-l6-briefs.md;
     ticket 157): this file reuses its table columns, adopt-all-except line and the four
     disposition tokens verbatim. This ticket never rules — `recording-act` rows only cite
     an operator ruling already in LEDGER GATE DECISIONS or a landed ADR; everything else
     is a proposal for GATE-G7. -->

**Operator act: "adopt all except: <ids>"**

Disposition tokens (closed set): `resolve-with-evidence` (landed evidence answers it —
cite tests/ADRs) · `adopt-interim-as-final` (the interim rule in force becomes the
ratified answer — quoted) · `descope-by-ADR` (permanently out of the build — ADR text +
authorising operator words named) · `needs-code-leg(<row>)` (post-GATE-G7 conform row +
the exact change). Every citation re-checked at base `840fb79`. Conform rows: `R4.15b`
(164e — records conform for BL-30 CF records + the OQ-388 ratification), `R4.15c` (164h —
code legs; manifest routes CF-492/CF-493 here), `R4.15e` (164m — signer-key custody).

## Batch-adoptable table

| id | proposed disposition | one-line basis | conform row |
|---|---|---|---|
| CF-476 | adopt-interim-as-final | the reporting-rule restatement is the operative reading and is ratified: §10.2 keeps its six verbatim gates (ADR-0046 D7); `outcome_bounds`/`multiplicity` stand as a reporting rule (ADR-0157 (e) fields; ADR-0158) with the `robust` verdict bound to non-`static` `compute_policy` defaults only (ADR-0190 D6); the general-C4-admissibility question is OQ-471's (below) | R4.15b |
| CF-487 | adopt-interim-as-final | the spec-only interim reading is the operative derivation and is ratified: `refused_by_kernel` = a kernel refusal of the invocation after `hello` and before a turn runs (the ADR-0176 D5 errors — `Refused{reason}`, `InsufficientBudget`, `AuthorityViolation`, `WouldBlock`, `Draining`); it opens no turn and carries `result.reason` | R4.15b |
| CF-488 | resolve-with-evidence | the landed `hh-embed/1` grouping is the operative contract and answers the placement question: `list_leases` → Group R (a read), `amend` → Group W (workspace draft), `archive`/`grant`/`revoke_lease` → Group S (state-mutating writes — staged `Refused{stage_pending}`; `crates/hh-embed-schema/src/ops.rs:485-493, 825-827, 866-880`); CC7's schema source is authoritative — the ADR-0216 interim placement (W/W/W/W+S) was written pre-landing and is conformed by amendment | R4.15b |
| CF-490 | resolve-with-evidence | **ruled — recording act:** the register's own resolution rules "the row data is authoritative" (R2.25 sweep); the spec-debt header parenthetical is corrected this ticket — the sole `hypothesized` record is ADR-0202 (C4), never ADR-0049 | — (landed this ticket) |
| CF-491 | needs-code-leg(R4.15b) | the named packages' ratifications are now proposed (OQ-235/240 adopt-interim-as-final, R4.11 brief; OQ-417/-433 on R4.13's BL-37 brief): the leg appends amendment-log entries to ADR-0183 (d)/ADR-0208 (d) recording that the assumed closures landed as interim adoptions, and answers the two spec-debt rows' `fired-unanswered` grades with dated notes (grades never rewritten) | R4.15b |
| CF-492 | needs-code-leg(R4.15c) | mint `seal` as a member of §3.3.8's closed `stage` set — the producing check IS the stage-5 seal self-check (`hh_compiler::seal`'s re-encode gate), so `seal` names it honestly where the landed `validate:6b` is a borrowed spelling; the leg conforms §3.3.8's set + the emitter to report `stage: "seal"` for `C-SEAL-1` | R4.15c |
| CF-493 | needs-code-leg(R4.15c) | mint a per-candidate discharge member on `hh-xcheck-bundle/1` — `verified_by[]` (candidate → evidence class/ref) on `arms[]` and `pending_cells[]` — so the landed E2 `foreign-run-verified` evidence (PR #136, 49/49) is expressed while E3/extension/C12 cells stay `environment-pending`; the leg extends the bundle record, repopulates the committed bundle, regenerates per CC7 | R4.15c |
| OQ-170 | needs-code-leg(R4.15e) | **concrete custody proposal below:** kernel-held Ed25519 signer key (extends ADR-0359 D2's scope — named), file-backed per installation (0600), `signer_key_ids` populated, co-signed `rotation` checkpoint (`CheckpointKind::Rotation` exists); the `AuditSigner`/`AuditKeyResolver` seam is unchanged — custody stays outside `hh-ledger` | R4.15e |
| OQ-388 | adopt-interim-as-final | the measured bound + per-projection maintenance policy — landed (ADR-0305 D1) and spec-folded (ADR-0331 D4) — is ratified verbatim (ratification text below); `ac_r_2_2_1_16_latency_1e5_fixture` enforces it in CI | R4.15b |
| OQ-470 | adopt-interim-as-final | CF-487's registered ratification leg rides it: the post-`hello` pre-turn reading is appended to ADR-0169 as its D6 amendment (the OQ-470 act the register names) | R4.15b |
| OQ-471 | adopt-interim-as-final | CF-476's registered follow-on: `outcome_bounds.verdict = robust` + multiplicity stay a **reporting rule** bound to non-`static` `compute_policy` defaults (ADR-0190 D6) — no general C4 admissibility gate is minted; minting one would need enforcement machinery no landed surface carries | R4.15b |

## Recording acts this ticket (ruled — not proposals)

| id | act | authority |
|---|---|---|
| DF-S0.3-3 / BL-02 (HUMAN-H4) | the operator's decline recorded verbatim ("Record a decline instead", LEDGER GATE DECISIONS 2026-10-20 H4 row) on the S0.3 sheet readout; ADR-0371 waives the human-signature cell and discharges GATE-ACCEPT item 7; DF-S0.3-3 closed `WONTFIX`-class waived-by-ADR; BL-02's cell records it for R4.16's flip | operator ruling, verbatim, GATE DECISIONS 2026-10-20 |
| CF-490 | spec-debt.md counts parenthetical corrected to ADR-0202 (C4); dated CF note + transition-log row | the register's R2.25 resolution ("row data is authoritative") |
| DF-S1.24-3 | **DONE** 2026-10-06 — `debt.hypothesis` stays a plain string at `ModelProfile/1`; divergence ruled intentional (ADR-0331 D1); `hypothesis_typed` carries the structured form; a `/2` bump may unify at the owning cadence, not owed | ADR-0331 D1 (landed) |
| DF-S1.26-2 | **DONE** — `detached:"parked"` is the declared result member, no `ExitClass` widening (ADR-0331 D2; A-1 ticked + applied; R2.1 closing pass asserted `exit_class: ok` + `detached == "parked"`) | ADR-0331 D2 (landed) |
| CF-489 | **resolved** 2026-10-16 — `DebtPolicy::default()` conformed to §5h.6's placeholders under the operator's direction ruling (R3.9); the OQ-441 `DebtIndex` re-pin stays WS-I6's | operator ruling 2026-10-14 → R3.9 (landed) |

## Per-id basis

- **CF-476 — §10.2 gate 7.** The conflict was a refusal attributed to a decision that
  does not contain it; the register's resolution — gate 7 removed from the verbatim
  table, `outcome_bounds`/`multiplicity` restated as the ADR-0157 (e)/ADR-0158 reporting
  rule with ADR-0190 D6's scope — has been the operative reading since the Phase-5
  hygiene pass and nothing landed contradicts it (R2.24 re-check). Adopting it as final
  ratifies the reading; the residual "does `robust` gate every C4 proposal" question is
  OQ-471's own row (proposed `adopt-interim-as-final` — reporting rule stands; the
  general-gate alternative would mint admissibility machinery no surface implements).
- **CF-487 — `refused_by_kernel` derivation.** ADR-0169 D6 is under-specified, not
  contradicted; the interim reading fills the derivation gap and has been operative
  since 2026-09-11. Adopting it as final = the ADR-0169 amendment-log append at R4.15b
  (OQ-470's named act) quoting the reading verbatim.
- **CF-488 — the five fixer ops' group membership.** Interim text (ADR-0216/OQ-468):
  `list_leases`/`revoke_lease`/`grant` → Group W, `archive{run}` → Group S. Landed:
  `amend` Group W implemented; `list_leases` Group R implemented; `archive`/`grant`/
  `revoke_lease` Group S staged `Refused{stage_pending}` (`hh-embed-schema/src/ops.rs`;
  `schema/hh-embed-1.schema.json`). The landed grouping is coherent with the group
  axis — reads → R, state mutations → S, workspace drafts → W — and the generated
  schema is the operative contract (CC7): conforming the schema to the stale interim
  placement would churn the boundary for a placement-only difference. *Proposed:*
  ratify the landed grouping; R4.15b conforms the ADR-0216 interim placement text and
  the §7.4 Group-W table by amendment. *Declined alternative:* re-group the schema to
  the interim placement — `needs-code-leg(R4.15c)` if the operator rules that way
  instead. OQ-468's R4.15c stable-signature leg (R4.11's `needs-code-leg` proposal)
  takes the adopted grouping as its input either way.
- **CF-491 — the fired-unanswered (d) clauses.** ADR-0183 (d) promised "OQ-235 rules
  tool-plane spellings before Stage 3"; ADR-0208 (d) promised the OQ-417/-433/-235/-240
  cluster "close before Stage 4"; both stages landed on the interim readings
  (ADR-0252 §6; `hlc` member + `MessagingPolicy` data in force). The ratifications now
  exist as proposals: OQ-235/240 `adopt-interim-as-final` with drafted ratification
  text (R4.11's BL-35 brief); OQ-417/-433 on R4.13's BL-37 brief. *Proposed leg
  (R4.15b):* append amendment-log entries to ADR-0183/ADR-0208 recording that the
  assumed closures landed as GATE-G7-adopted interim ratifications, and append dated
  answering notes to the two spec-debt rows' `fired-unanswered` grades — the grades are
  history, never rewritten.
- **CF-492 — the `seal` stage spelling.** `C-SEAL-1 NonCanonical`'s sole producer is
  `hh_compiler::seal`'s re-encode gate; §3.3.8's closed set has no `seal` spelling, so
  the armed stage-6b plan translation reports `validate:6b` under the "stage names the
  producing check" convention (ADR-0358 D5). *Proposed leg (R4.15c):* mint `seal` in
  the §3.3.8 `stage` set (spec-text conform at R4.15b) and conform the emitter to
  report `stage: "seal"` for `C-SEAL-1` — the honest producer name. *Declined
  alternative:* re-home `C-SEAL-1` under an existing spelling — none names the seal
  gate; `validate:6b` attributes the diagnostic to a check that did not produce it.
- **CF-493 — the bundle status vocabulary.** `arms[].status` spells only
  `e1-self-checked`; `pending_cells[].status` only `environment-pending` — the E2
  `foreign-run-verified` evidence (PR #136: `hh-xcheck verify` 49/49, 2026-10-14)
  cannot be expressed without under- or over-stating. *Proposed leg (R4.15c):* mint
  `verified_by[]` (candidate → `{evidence_class, evidence_ref}`) on `arms[]` and
  `pending_cells[]`; repopulate the committed bundle with E2's discharge; the
  LEDGER-OF-6 *format* half (machine-readable pending sets understating partial foreign
  discharge — BL-41's finding) rides this member; LEDGER-OF-6's *practice* half (the
  standing dated lead-token sweep at round closure) is BL-41's own policy at L4.2, not
  this row's.
- **OQ-170 — the custody proposal** — see below (the ticket's named deliverable).
- **OQ-388 — the ratification text** — see below; `adopt-interim-as-final` with the
  interim measured bound + policy quoted verbatim as the ratified answer.

## OQ-170 — the concrete signer-key custody proposal (for GATE-G7; implement leg R4.15e)

ADR-0213 reserved OQ-170 to WS-H3/H6/L4; ADR-0331 D3 ruled custody stays external with
the landed seam as the conformant interim. The round-4 re-charter moves the
ratification to the operator at GATE-G7 — **the operator is the ratifying authority**;
ADR-0331 D3's reservation is answered by the operator's adopt/amend/decline act. The
proposal (recommended option first, alternatives with trade-offs after):

- **Which process holds the key — the kernel.** `hh-kernel` holds the run signer key
  in-process; `hh-ledger` sees only the landed `AuditSigner`/`AuditKeyResolver` seam
  (`crates/hh-ledger/src/audit.rs:301-318` — `key_id` + `sign(preimage)`, never key
  material). The kernel resolves the key through
  `CredentialBroker::kernel_use(KernelPurpose::LedgerSigning)` — exactly the path
  DF-S2.5-1's entry test names — and `hh-secrets` is the custody home: the only crate
  admitted to `ring` (ADR-0359 D2), so key material has one owner crate.
- **Provisioning per installation — file-backed, operator-provisioned.** The install /
  provisioning step writes `<HH_STORE_ROOT>/secrets/ledger-signer.<key_id>.pkcs8`,
  mode `0600`, owner-only — minted on the host (`Ed25519KeyPair::generate_pkcs8` +
  `SystemRandom`) or operator-placed; the provisioning act records `provided: yes/no`,
  never key bytes, in any ledger. Absent a provisioned key the landed honest state
  stands unchanged: `signer_key_ids` empty, `audit_view` reports `checkpoints:
  n/a{no signer_key_ids declared}` — no fabricated signer, ever.
- **Algorithm — Ed25519 via `ring::signature`, an ADR-0359 D2 scope extension (named
  honestly).** Production signing with a persistent key exceeds the admitted
  "verification + test-time fixture signing" scope; R4.15e carries the scope-extension
  amendment (same crate, same `=0.17.14` pin, no new dependency — D2's own revisit
  trigger names the sibling-scope act). The checkpoint signature's `alg_ref` member
  extends `{hmac-sha256}` → `{hmac-sha256, ed25519}` (additive — CC8; `render_sig`/
  `parse_sig` + the verify arm branch on the declared alg). Asymmetric is the point: `AuditKeyResolver`/
  `KeyTable` then hold **public** bytes only — a verifier (incl. a foreign
  cross-impl replay) cannot forge.
- **Rotation — the co-signed `rotation` checkpoint.** `CheckpointKind::Rotation`
  already exists (`audit.rs:268-284`, "Key rotation boundary"). The incumbent key
  co-signs the `rotation` checkpoint that introduces the successor `key_id`; the run
  manifest's `signer_key_ids` accumulates (current + retired); `KeyTable` retains
  retired public keys so historical checkpoints still verify. A rotation checkpoint
  without the incumbent's co-signature is `BadSignature` — rotation is itself an
  auditable ledger event (CC3).
- **`idp`-rotation interaction (ADR-0036) — none.** The signer `key_id` is a custody
  coordinate, not an identity: idp/1 rotation moves content addressing
  (`VersionedRef`), never the audit signer; neither rotation forces the other.
  `WitnessDecl` custody is external identically (the R2.23 note) — witness keys adopt
  the same per-installation file convention when witness declarations land.
- **WS-H3 secret isolation.** The key file lives under the store root's `secrets/`
  custody domain; no secret bytes in ledger state, manifests or projections — the
  seam's `key_id`/`sig` fields only (secrets enter no ledger: `provided: yes/no`).

| option | strength | cost |
|---|---|---|
| **kernel-held Ed25519 (recommended)** | asymmetric — resolvers hold public bytes; foreign/third-party verify cannot forge; production-grade answer to "which process" | ADR-0359 D2 scope extension; `alg` member extension (both additive, one crate) |
| kernel-held `hmac-sha256` | zero scope growth — the landed `CHECKPOINT_ALG` + `FixedSigner` shape | symmetric: every resolver holds sign power; exported audit can't distinguish verifier from signer |
| helper-held key | process isolation | the helper is per-spawn ephemeral — custody continuity and per-run availability couple wrongly; WS-H3 reserves the helper model |
| external key store (OS keychain/HSM/KMS) | strongest custody boundary | unprovisionable under the hermetic build; a platform dependency — a future admission, not R4.15e's |

*Proposed disposition:* `needs-code-leg(R4.15e)` — R4.15e implements the adopted
ruling (the recommended Ed25519 leg: `hh-secrets` Ed25519 signer behind
`kernel_use`, the `alg` member extension, provisioning verb, co-signed rotation; or
the HMAC fallback if the operator declines the scope bump — the table above records
that trade).

## OQ-388 — the latency-bound ratification text (proposed verbatim)

> "OQ-388 is ratified: the per-projection maintenance policy stands as landed —
> `by_event_id`, `by_class` and `ir_index` are incrementally maintained at append and
> rebuild-identical; every other projection (`trace_view`, `branch_tree`, the run
> index) is a shared-fold rebuild over the durable prefix. The query-latency bound for
> `read(filter)`/`project(until_seq)` on a 10⁵-event run is **result-proportional**:
> the measured 100-of-10⁵ class+`ir_ref` filtered read at ≈15 ms —
> `ac_r_2_2_1_16_latency_1e5_fixture` (`crates/hh-ledger/tests/s4_13.rs`; ADR-0305 D1;
> spec-folded per ADR-0331 D4) — is the ratified bound; the fixture enforces it in CI
> and a regression past it fails the acceptance battery. This act discharges
> DF-S4.13-1's ratification half; R4.15b records the register close."

`adopt-interim-as-final` is the disposition: the interim (landed + spec-folded) rule
quoted becomes the ratified answer — nothing new is minted; the WS-B1/K2 ratification
act the register names is the operator's adopt under the round-4 authority.

## What this brief does not decide

No register status change, no amendment-log append, no schema/spec conform lands as a
proposal — every row above the recording-act table is a proposal the operator adopts,
amends or declines at GATE-G7; the conforms are R4.15b's (records), R4.15c's (code) and
R4.15e's (custody implementation). The interim rules stay in force unchanged until
then. BL-30's flip is R4.16's accounting after GATE-G7 adoption and the conforms land.
The two recording acts (H4 decline → DF-S0.3-3 waived-by-ADR-0371; CF-490 header fix)
land *this* ticket because an operator ruling already on the record fully determines
each — cited verbatim at the acts, never extended.

> *Reconciled 2026-10-22 (GATE-G7 fleshing):* CF-491's amendment-log leg lands at **R4.15b** (164e) with the BL-30 records; CF-492/CF-493 land at **R4.15c** (164h); OQ-170 at **R4.15e** (164m); OQ-388's ratification record at **R4.15b** (164e).
