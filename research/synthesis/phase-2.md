# Phase 2 synthesis memo — subsystem contracts (C1–C4, D1–D5, E1–E5, F1–F2, G1–G3, H1–H7, B3–B5, I3–I4)

**Date:** 2026-09-10 · **Pass:** Phase 2 synthesis (doc 3 §7.1, §11.5) · **Gate:** **passed** · **Binding on Phase 3:** §6 of this memo, ADR-0145 (reconciliation rulings), ADR-0146 (scope changes), Ontology **v2** (`registers/ontology.md` §5b/§5c), and the amended Phase 1 ADRs listed in §1.3.

Product name per ADR-0011: **HarnessHarness** ("MetaHarness" appears in this program only inside quoted historical text and file paths; the Phase 2 naming audit found no residual product-name use — CF-321). Language-use per ADR-0050 §8: no Phase 2 artifact commits to a language, runtime, package ecosystem or framework (CF-317).

---

## 1. Decisions ratified

### 1.1 Workstream ADRs (94 proposed → 94 ratified, 17 amended, 0 rejected)

Numbering follows the brief's order (H, D, E, F, G, C, B, I). Every ADR carries the doc 3 §3.3 five evidence fields and a T-LCD statement (checked mechanically: 94/94). "amended" = ratified with a logged "Amendment log (Phase 2 synthesis)" section applying a synthesis ruling; the ruling's CF id is in the log.

| WS | ADRs | subject | amended |
|---|---|---|---|
| WS-H1 | ADR-0051…0053 | authority handles; `authorize` decision function + Π default table (OQ-101/086 closed); TCB boundary, attenuating delegation, pre-authorization | 0051 (handle lifetime across resume), 0052 (`decider` sum; Π rows from H7/H3; assessor inputs) |
| WS-H2 | ADR-0054…0056 | C2 propagation semantics; declassification/endorsement contract (D-ROBUST, remedies); flow-policy language | — |
| WS-H3 | ADR-0057…0059 | secret-visibility invariants SV-1…10 + `SecretRef`; credential broker contract; leak battery LT-01…12 + `secret_access` Π row | — |
| WS-H4 | ADR-0060…0062 | `ContainmentPolicy/1` on the environment handle; egress mediation (`decide_egress`); containment as the floor, enforcement points EP1–EP4, `amend()` | 0061 (EP spelling; kernel-minted token; dimensions), 0062 (EP1–EP4 rename; phase schedules) |
| WS-H5 | ADR-0063…0065 | extension trust model (three-leg record, location neutrality); lifecycle with declared sources + pins; model-install path | — |
| WS-H6 | ADR-0066…0068 | audit trail = ledger via `audit_view` + audit-grade catalogue; tamper-evidence contract; redaction/GC/retention + `audit_completeness` vector | 0066 (catalogue rows from neighbours) |
| WS-H7 | ADR-0069…0071 | escalation policy model (Π-1…12, never-auto set, reviewer chain); approval request/response contract (durable `pending`); leases + `approvals.requested` budget | 0071 (`approvals_exhausted` alias) |
| WS-D1 | ADR-0072…0074 | kernel admission over a `ContextPlan`; budget discipline + offloading; six reserved slots, rendering ownership (OQ-063), label reset | 0072 (`ProviderRequestPlan`; `opacity_dynamic`), 0073 (`handle_only ⇔ indexed`), 0074 (`validity_policy`; `memory_index`; `volatile_kinds`; handle items do not join the label; `prompt_layout` params) |
| WS-D2 | ADR-0075…0077 | one compaction class with a closed op sum + C0 `evict_oldest`; provenance of derivations; `lab/compaction-family-v1` | — |
| WS-D3 | ADR-0078…0080 | five addressing classes + `MemoryStore` contract; retrieval contract (filter order before any ranker); memory writes/consolidation under the ADR-0035 memory-store row | 0080 ("supersedes" → amendment to ADR-0035, confirmed) |
| WS-D4 | ADR-0081…0083 | invalidation contract + lifecycle state; supersession/revocation/conflict sets; justification-based inheritance + `MemoryStaleIndex` (OQ-020) | 0081 (`environment_epoch` dependency kind) |
| WS-D5 | ADR-0084…0086 | `ProcedureProfile/1` in `ext` (CF-038, OQ-023); three compilation targets + `procedure_target`; skills as lifted procedures | — |
| WS-E1 | ADR-0087…0089 | `ToolCapability` field discipline (ADR-0016 item 6 reworded); registry contract; cost/risk/observability/exposure metadata | 0087 (`flow_contract`, `postconditions`), 0088 (`quarantined` lifted records) |
| WS-E2 | ADR-0090…0092 | tool-interface compiler contract (exposure modes, surface families, closed transforms, `PlanMap` — OQ-066); wrapper synthesis S1–S4; result/failure rendering | — |
| WS-E3 | ADR-0093…0095 | exposure plan with `callable ⇔ revealed`; discovery capability + `catalog_index`; catalog sync/epochs | — |
| WS-E4 | ADR-0096…0099 | boundary placement (MCP/ACP/A2A); MCP edge (stateless catalogue); ACP edge + Hosting ABI baseline; `ProtocolBinding` + negotiation/trust | — |
| WS-E5 | ADR-0100…0102 | seven kernel stages + `tool_executor` contract (OQ-045 final); capture manifest + kernel-minted attribution; error taxonomy, timeouts, cancellation | — |
| WS-F1 | ADR-0103…0105 | `control_strategy` contract (CF-005 resolved); `react/minimal` Stage-0 anchor; canonical Lab comparison | 0103 (`StopReason` spellings → F2; `Cue.woken`) |
| WS-F2 | ADR-0106…0108 | control envelope (`EnvelopePolicy`, six guards, closed `StopReason`, stop protocol); retry/timeout policy (OQ-098/119); loop detection, output validation, INV-1…9 | — |
| WS-G1 | ADR-0109…0111 | task contract + completion gate + veto predicates; Validator contract; check placement + deterministic `followed` table (OQ-049) | — |
| WS-G2 | ADR-0112…0114 | claim model + divergence taxonomy; completion gate Γ (OQ-092); execution-alignment metrics (`false_completion_rate` veto) | — |
| WS-G3 | ADR-0115…0117 | critic contract over kernel-built evidence; independence vector + isolation; calibration (OQ-123) + cost governance | — |
| WS-C1 | ADR-0118…0120 | model-blind gateway over the `ProviderRequestPlan` (OQ-072, CF-044); closed stop-reason/error-class vocabularies + `WireDialect`; discovery/drift/fingerprint | — |
| WS-C2 | ADR-0121…0123 | router over the `ModelRoleTable`; retry-vs-reroute split; ensembles as procedures | 0121 (`ModelRoleTable` rows carry `profile_ref`) |
| WS-C3 | ADR-0124…0126 | `ProfileRuleInventory/1` (thirteen kinds) + Profile Compiler; profile test contract (OQ-071); expiry/revalidation | 0124 (`procedure_target`; `profile_binding` projection) |
| WS-C4 | ADR-0127…0129 | cache taxonomy K1–K6 + `CacheSemantics` as data + LC-1…9; affinity key, expected-vs-observed, `cache_policy` (OQ-097); harness-level caches | — |
| WS-B3 | ADR-0130…0132 | durability contract (checkpoint view, recovery table, leases); wakeups/suspended runs/goal continuation; healing + kill-point battery | — |
| WS-B4 | ADR-0133…0135 | branch model + fork/snapshot/rollback; speculative containment with `deferred`; nondeterminism recording + replay validity | — |
| WS-B5 | ADR-0136…0138 | environment handle (OQ-045 B5 half); identity + snapshot semantics (OQ-089); effect-operation surface + accounting (OQ-095/118) | 0138 (I4 handle operations: `set_phase`, content-addressed `upload`/`download`) |
| WS-I3 | ADR-0139…0141 | three-layer bundle format; validation + reproducibility levels; lifecycle/interop | — |
| WS-I4 | ADR-0142…0144 | environment families + `TaskRecord` + `benchmark_adapter`; anti-leakage L1–L5; grader integration + Stage 3 suite | — |

### 1.2 Synthesis-authored ADRs

- **ADR-0145** — cross-dossier reconciliation: CF-006 closure, the composed effect path, context chain, compilation chain and CONTROL/verification seams; canonical spellings; the amendments to 21 ratified Phase 1 ADRs.
- **ADR-0146** — scope changes: R-2.7.2 split into R-2.7.2a (C0/Should) and R-2.7.2b (C2/Could); eight stage notes; SWE-bench Verified demotion recorded; 31 items → `specified-by-ADR`.

### 1.3 Phase 1 ADRs amended at Phase 2 (each carries an "Amendment log (Phase 2 synthesis)")

ADR-0016, 0017, 0019, 0020, 0021, 0022, 0026, 0030, 0031, 0034, 0035, 0036, 0037, 0038, 0039, 0040, 0041, 0043, 0044, 0047, 0050. Every amendment is additive or a wording refinement; no ratified decision is reversed; every closed sum that grows does so by dialect bump (ADR-0015 rule).

---

## 2. Contract convergence achieved

### 2.1 CF-006 — one authority scheme, five conforming consumers (brief focus 1)

H1 (ADR-0051…0053), H2 (0054…0056), D1 (0072…0074), D4 (0081…0083) and D3 (0080) all cite ADR-0033/0034/0035 and introduce no second class, label or lattice. The scheme changed only by amendment: the kernel stamps `context_label` (CF-118); check 3's domain list grows (CF-124); `pin` needs a verified signature (CF-142); the persistence-ceiling table gains a `store` dimension so production-style agent memory is expressible without weakening instruction ceilings (CF-172); `UnattendedPolicy` and `auto_review` are expressed inside the closed basis list (CF-154, CF-119). Two residual tensions were found and closed at synthesis: whether quarantined external content joins the label (it joins when rendered inline, never when delivered as a handle — CF-311) and the `decider` sum (`{policy, cache, hook, auto_reviewer, human}` — CF-312). CF-006 is **closed**.

### 2.2 The effect path (brief focus 2) — one description

```
propose (F1 ControlDecision.act; G-PRE-DISPATCH guard, F2)
  → resolve   (E5 ADR-0100)   canonical args via SurfaceArgMap (A4/E2); scope_bindings (E1);
                              deterministic assessors run by the kernel; partial parse ⇒ unknown
  → authorize (H1 ADR-0052)   ordered checks over records: handle table ⊇ grant; containment
                              admits ∧ covers (H4 ADR-0062, EP4 precondition); flow check at C2
                              (H2 ADR-0054); credential binding declared (H3 ADR-0058);
                              Π(domain, risk pattern, effective authority) with H7's Π-1…12 and
                              H3's secret_access rows; `ask` → durable security.permission.pending,
                              escalation chain, leases, approvals.requested budget (H7 ADR-0070/0071);
                              decision recorded once: security.permission.decided{decider, handle_ids[], …}
  → prepare   (E5)            baseline snapshot (B4/B5), compensation plan (B2), idempotency key
                              (ADR-0031), kernel-minted attribution token (CF-213), deadline from
                              TimeoutPolicy (F2 ADR-0107), effect lease at C1 (B3 ADR-0130)
  → commit    (B2/B1)         write-ahead action.effect.committed; action.tool.started{execution_id}
  → execute   (E5 → B5)       out-of-process helper on the environment handle (ADR-0136/0138);
                              EP1/EP2 containment (H4 ADR-0060); EP3 egress mediator keyed on the
                              attribution token (H4 ADR-0061) with H3 broker injection (proxy-injected
                              primary); executors report, never decide (I-3)
  → capture   (E5 ADR-0101)   CaptureItem manifest with completeness + source; H3 redaction at
                              source before any ledger append or sink; H4 enforcement_evidence
  → observe   (B2 ADR-0030)   observed | unknown → probed → compensated/reverted/abandoned;
                              G1 local checks after the terminal event (ADR-0111);
                              G2 claim reconciliation (ADR-0112); F2 G-POST guards;
                              H6 audit-grade rows (ADR-0066); accounting (L2, C4, B5 meters)
```

No gap remains: the `prepare → authorize` order of doc 2 §11 is resolved by the `resolve`/`prepare` split (CF-216); B4's `deferred` phase (CF-287) and B3's recovery table sit on the same lifecycle; B5 adopted H4's containment slot and E5's handle operations verbatim (CF-293) and I4's phase/upload/download operations (CF-303, CF-318); the attribution token has one minter (CF-213); one error sum per boundary (CF-314); hooks and reviewers are raise-only everywhere (CF-121, CF-217).

### 2.3 The context chain (brief focus 3)

D1's kernel admission (`ContextPlan`, invariants I-ID/I-RP/I-ORDER/I-LABEL/I-NOWIDEN) is the spine. D2 runs under the kernel gauge cap (`CompactionRequired`; CF-166) and reports `context_exhausted` through F2's sum (CF-168); its derivations are `delegate`-derived (CF-164). D3 delivers memories as candidates with their own label and a `memory_index` candidate kind (CF-176); its retrieval events and overhead classes were added (CF-173, CF-174). D4's `validity_policy` supersedes D1's boolean (CF-181) and gains `environment_epoch` for C4's tool-result cache (CF-277). C3 confirms every layout choice is a `prompt_layout` parameter (CF-267) and admits a C0/Stage 2 schema slice of `compaction_reminder` (CF-271). C4's `volatile_kinds` table is a `link` check (CF-276). E3's `handle_only ⇔ indexed` (CF-203). The output is one `ProviderRequestPlan` (CF-258).

### 2.4 The compilation chain (brief focus 4) and the LCD battery

A4's pipeline (ADR-0019) → C3's Profile Compiler over a thirteen-kind `ProfileRuleInventory/1` (ADR-0124; `procedure_target` admitted, CF-316) → E2's surface compiler with the closed transform vocabulary (`parse` at C0; ADR-0022 amended, CF-197) → E4's protocol edges (ADR-0021 amended: stateless MCP catalogue, v2 + v1 ACP profile, spellings; CF-206/207/211). Surfaces are referenced by id (CF-270). The battery was applied to ADR-0124…0126 and ADR-0090…0092 with results recorded in `registers/lcd-test-battery.md` (Phase 2 application record): **all six pass on paper**; executable fixtures named for Stage 3 (T-04 two-profile compile, null-profile round-trip, E4 differential suite). CF-001 is validated on paper.

### 2.5 CONTROL (brief focus 5)

F1 resolves CF-005 as "pluggable strategy + Core envelope" (ADR-0103/0106). F2's `EnvelopePolicy` is the typed content of `ControlBoundary.guards`; its budget dimensions are L2's (ADR-0039/0040) with `approvals.requested` from H7 (CF-224) and `context_exhausted` from D2 (CF-168); soft thresholds re-arm on `completed{status: applied | fallback_applied}` (CF-169); retries split three ways (gateway executes identical-bytes attempts, F2 supplies policy and counts, C2 reroutes — CF-256/262); F1's spellings converge on F2's `StopReason` and `check` view (CF-229); F1's `Cue` gains `woken` for B3 (CF-281). H7's approvals sit inside the envelope as a budgeted dimension with the lease and never-auto rules; unattended runs configure Π without `ask` (CF-154/156).

### 2.6 Verification provenance (brief focus 6)

One verdict record — `verification.validator.verdict` extended with `grounding`, `bundle_id`, `evidence_cited[]`, `calibration_ref`, `independence_summary` (CF-247) — is produced by G1 validators, G2's reconciler and G3's critics alike; `security.permission.reviewed` is H7's projection of it (`verdict_id`). G2's claims are `model_claim` observations with four-valued agreement (`verification.claim.reconciled{agreement}` — the name G3 assumed, CF-250); the completion gate lives on `verification.completion.decided` and `lifecycle.run.finished.status`, not in `StopReason` (CF-315). I2's grounding metrics consume these events: `claim_state_agreement`, `evidence_traceability`, `execution_alignment_failure_rate` (deterministic; `_judged` companion — CF-240), `false_completion_rate` (veto; ADR-0047 amended — CF-241), `reward_hacking_gap`, `surface_rejection_rate`, `attribution_completeness`, `audit_completeness` (vector).

### 2.7 C0 minimal variant vs extension family (brief focus 8)

Every subsystem ADR states its C0 slice and the family above it; the scope register (ADR-0146) records the C0 slices that live inside C1/C2 items. The C0 reference runtime after Phase 2 is: kernel admission + `evict_oldest` compactor + deterministic retrieval kinds (D); one gateway dialect + static router + two minimal profiles + K1/K2 caches (C); registry with lifted `unverified` records + schema-only surface compiler + `direct`/`indexed` exposure + MCP stdio (E); `react/minimal` + envelope with six guards (F); Validator contract + kernel local checks + claim substrate (G); authority handles + Π table + broker + `ContainmentPolicy/1` + audit view + per-effect approvals (H); crash recovery + `deferred` schema + environment handle with `fs_tree` snapshots (B); run bundle R0/R1 + three benchmark families (I).

---

## 3. Evidence discipline check (doc 3 §3, RK-03, RK-11)

- **Five fields:** 94/94 proposed ADRs carry (a)–(e) and a T-LCD statement (mechanical check at synthesis).
- **Provisional audit of C0 ADRs (brief focus 7):** H1 (0051–0053), H3 (0057–0059), H4 (0060–0062), H6 (0066–0068), D1 (0072–0074), E1 (0087–0089), E5 (0100–0102), F1 (0103–0105), F2 (0106–0108), C1 (0118–0120), B5 (0136–0138) and the C0 slices of every other ADR were read against the folded `sources.md` rows: every `provisional`/`vendor` row (S-103/104/105, S-062, S-070, S-065, the 2026 preprints folded at S-251…S-498) is cited for mechanism existence or as a hypothesis; the C0 decisions rest on Tier-A theory, protocol text, or source read at pinned commits (Codex 0735c51, OpenHands 3fc7b22, goose fae91d0, opencode 9f8db11, pi-mono 400d690, gemini-cli ed2ac40, harbor 7d5285b, inspect_ai 75f4891, browser-use 50f2055, langgraph e539ac1, SWE-bench 02e7a74, and the MCP/ACP/A2A repositories). No exception found.
- **Sources:** 248 new rows (S-251…S-498), 13 cross-sidecar duplicates merged, 26 path-level citations folded into existing rows, 25 promotions S → P. Unreachable primary pages recorded as row notes (S-042, S-044, S-055).
- **Language-leak audit (CF-317) and naming audit (CF-321):** clean.

---

## 4. Register state after folding (id maps)

Temp ids in every dossier, sidecar and ADR were rewritten to final ids (the full map is derivable from the per-workstream ranges below; the dedup map is in `registers/sources.md` "Synthesis notes (Phase 2)"). Ranges include ids merged by dedup where a sidecar's row mapped onto an earlier workstream's row.

| WS | ADRs | sources | open questions | conflicts |
|---|---|---|---|---|
| WS-H1 | ADR-0051…0053 | S-251…S-257 | OQ-132…OQ-138 | CF-118…CF-122 |
| WS-H2 | ADR-0054…0056 | S-258…S-273 | OQ-139…OQ-147 | CF-123…CF-129 |
| WS-H3 | ADR-0057…0059 | S-274…S-287 | OQ-148…OQ-153 | CF-130…CF-133 |
| WS-H4 | ADR-0060…0062 | S-288…S-297 (+S-258) | OQ-154…OQ-161 | CF-134…CF-139 |
| WS-H5 | ADR-0063…0065 | S-298…S-314 (+S-279) | OQ-162…OQ-168 | CF-140…CF-145 |
| WS-H6 | ADR-0066…0068 | S-315…S-325 | OQ-169…OQ-176 | CF-146…CF-151 |
| WS-H7 | ADR-0069…0071 | S-326…S-334 | OQ-177…OQ-183 | CF-152…CF-157 |
| WS-D1 | ADR-0072…0074 | S-335…S-336 | OQ-184…OQ-190 | CF-158…CF-163 |
| WS-D2 | ADR-0075…0077 | S-337…S-346 | OQ-191…OQ-198 | CF-164…CF-171 |
| WS-D3 | ADR-0078…0080 | S-347…S-352 | OQ-199…OQ-205 | CF-172…CF-178 |
| WS-D4 | ADR-0081…0083 | S-353…S-361 (+S-347/348) | OQ-206…OQ-212 | CF-179…CF-184 |
| WS-D5 | ADR-0084…0086 | S-362…S-366 | OQ-213…OQ-218 | CF-185…CF-190 |
| WS-E1 | ADR-0087…0089 | S-367…S-371 (+S-304/310) | OQ-219…OQ-225 | CF-191…CF-196 |
| WS-E2 | ADR-0090…0092 | S-372…S-378 | OQ-226…OQ-232 | CF-197…CF-201 |
| WS-E3 | ADR-0093…0095 | S-379…S-386 (+S-368) | OQ-233…OQ-240 | CF-202…CF-205 |
| WS-E4 | ADR-0096…0099 | S-387…S-392 | OQ-241…OQ-248 | CF-206…CF-211 |
| WS-E5 | ADR-0100…0102 | S-393…S-397 (+S-252) | OQ-249…OQ-254 | CF-212…CF-217 |
| WS-F1 | ADR-0103…0105 | S-398…S-400 | OQ-255…OQ-259 | CF-218…CF-222 |
| WS-F2 | ADR-0106…0108 | S-401…S-408 | OQ-260…OQ-265 | CF-223…CF-229 |
| WS-G1 | ADR-0109…0111 | S-409…S-415 | OQ-266…OQ-271 | CF-230…CF-236 |
| WS-G2 | ADR-0112…0114 | S-416…S-429 (+S-403/410) | OQ-272…OQ-279 | CF-237…CF-244 |
| WS-G3 | ADR-0115…0117 | S-430…S-439 (+S-418) | OQ-280…OQ-284 | CF-245…CF-250 |
| WS-C1 | ADR-0118…0120 | S-440…S-444 | OQ-285…OQ-291 | CF-251…CF-258 |
| WS-C2 | ADR-0121…0123 | S-445…S-458 | OQ-292…OQ-298 | CF-259…CF-264 |
| WS-C3 | ADR-0124…0126 | S-459…S-462 | OQ-299…OQ-305 | CF-265…CF-272 |
| WS-C4 | ADR-0127…0129 | S-463…S-469 (+S-457) | OQ-306…OQ-312 | CF-273…CF-279 |
| WS-B3 | ADR-0130…0132 | S-470…S-474 | OQ-313…OQ-318 | CF-280…CF-285 |
| WS-B4 | ADR-0133…0135 | S-475…S-481 | OQ-319…OQ-324 | CF-286…CF-290 |
| WS-B5 | ADR-0136…0138 | S-482…S-483 | OQ-325…OQ-330 | CF-291…CF-297 |
| WS-I3 | ADR-0139…0141 | S-484…S-491 | OQ-331…OQ-336 | CF-298…CF-302 |
| WS-I4 | ADR-0142…0144 | S-492…S-498 (+S-490) | OQ-337…OQ-345 | CF-303…CF-310 |
| synthesis | ADR-0145, ADR-0146 | — | — | CF-311…CF-321 |

Next ids: **S-499**, **OQ-346**, **CF-322**, **ADR-0147**. Ontology **v2**: 513 terms folded in §5b (status `ratified (v2, …)`), 10 vocabulary rulings in §5c.

Pre-existing rows updated: CF-001 (validated on paper), CF-005 (resolved), CF-006 (closed), CF-017 (Phase 2 audit clean; stays pre-registered for Phase 3), CF-023 (Hosting ABI baseline corrected), CF-038 (resolved), CF-044 (resolved), CF-055 (confirmed); 55 existing open questions answered or advanced (list in `registers/open-questions.md` "Synthesis notes (Phase 2)").

---

## 5. Open items carried forward (owner · due · blocking?)

All Phase 2 conflicts are dispositioned (193 workstream-flagged: 189 resolved, 4 `noted`; 11 synthesis-raised: 10 resolved, 1 `noted`). Open questions that Phase 3 must answer before the stage they block:

**Blocking Stage 1–2 (schema/kernel):**
- OQ-132 (`ResourcePattern` grammar: path roots, command classes, host sets, secret names; subset decidability) — WS-H1/H4/H3/E1 · Stage 1 handle table.
- OQ-133 (closed command grammar in the TCB; classifiable shells; default `unknown`) — WS-H1/E5 · Stage 1 list; Stage 3 cost.
- OQ-142 (unattended Π rows and remedies without `approval`) — WS-H1/H7 · joins OQ-101 rows at Stage 1.
- OQ-149 (`SecretRef` as HIR/1 leaf kind or constrained `Ref`) — WS-A3/H3 · dialect decision at Stage 1.
- OQ-165 (hooks as `ControlBoundary` guards or `HarnessRule` triggers) — WS-F1/H5 · Stage 1.
- OQ-186 (which capability reads offload handles: one kernel `read_artifact`) — WS-D1/E1 · Stage 2.
- OQ-219 (`SchemaDialect` values and the C0 keyword subset) — WS-A4/E2/E1 · Stage 1.
- OQ-223 (`unknown_domain` for hint-lifted effects) — WS-A3/E1 · Stage 1.
- OQ-235, OQ-240 (tool-plane event names; surface record fields) — WS-B1/E3/E1 · Stage 1.
- OQ-201 (Π treatment of `memory_write{store: memory}`) — WS-H1/D3 · Stage 1.
- OQ-344 (`applies_to_families` on `MetricDeclaration`/`VariantRecord`) — WS-A3/I2/I4 · Stage 1.
- OQ-170 (kernel signer key custody) — WS-H6/L5 · Stage 2.
- OQ-178, OQ-160 (`ActionPattern` projection per capability class; egress approval-cache lifetime) — WS-H7/H1/H4 · Stage 2.
- OQ-307 (`static_hash` and `role_map` collapse) — WS-C4/C3 · Stage 2.

**Blocking Stage 3–4 (Lab):** OQ-157 (default `isolation_class` for Lab runs) — WS-B5/I4; OQ-150 (TLS termination for `proxy_injected`) — WS-H4/H3; OQ-210 (memory `followed` detector classes) — WS-D3/G1; OQ-246 (approval persistence across the ACP edge) — WS-E4/H7; OQ-256, OQ-261 (control `reproducibility` estimator; outcome-class projection confirmations) — WS-F1/F2/I2; OQ-272, OQ-273 (gate vocabulary; autonomous validators) — WS-G2/F2/L2; OQ-336, OQ-338, OQ-342 (foreign verifiers; fresh SWE-style pool; parity margins) — WS-I4/I7; OQ-177 (`ApproverGrant` authority class) — WS-H7/L8 (C1/Stage 4); OQ-323 (counterfactual constants) — WS-I7.

**Program-level (non-blocking, advanced at Phase 2):** OQ-010, OQ-012, OQ-016, OQ-017, OQ-030, OQ-048 (closed; representation CF-313), OQ-055, OQ-103, OQ-107, OQ-109, OQ-111, OQ-112, OQ-113, OQ-122, OQ-128 — owners recorded in the register.

**Accepted tensions and notes:** CF-171 (D2 dossier length), CF-201 (stale `imported` spelling in a Phase 1 dossier), CF-279 (an unverified Codex comment about cache-preserving trims), CF-302 (WS-A1 positioning analogy kept as history), CF-319 (brief's "27 workstreams" is a stale count; 31 is correct).

**Risks:** RK-07 exercised and held at Phase 2; RK-09/RK-11 checks clean; RK-03 provisional discipline held; no new risk.

---

## 6. Settled for Phase 3 — binding on WS-J1…J6, WS-K1…K4, WS-L5

Phase 3 dossiers cite these by ADR id. Each item names what Phase 3 may rely on and what it must not re-decide.

### 6.1 The environment handle (ADR-0136…0138, with ADR-0060…0062, ADR-0100…0102, ADR-0142)

- The harness is **outside** the environment; one environment protocol over three backend placements (local helper process, container, remote service); native runs are never harness-inside (OpenHands agent-server is admitted only as a hosted `container-installed` participant).
- The kernel-owned `EnvironmentRecord{semantic_id, version_id}` is the environment factor (`configuration_id.environment_ref = semantic_id`); `EnvHandleId` is the session; `SnapshotRecord` is content-addressed with closed `SnapshotKind` (`fs_tree`, `path_baseline` at C0). Lifecycle state machine L1–L7 with self-heal as ledgered replacement plus a `world_state.discontinuity` record.
- Handle operations available to Phase 3: `open/attach/detach/close`, `snapshot/diff/restore`, `list_detached`, `on_kernel_loss`, `health/status/meters/connection_info`, `upload/download` (content-addressed), `derive` (modes with invariants D1–D6), `set_phase(setup | agent | verify, NetPolicy)`, `verify_environment` on resume, `effective()`/`amend()` on the containment slot. The handle carries `containment: Ref<ContainmentPolicy/1>` (deny-wins layered meet; `enforcement_evidence` tri-state; strict mode for Lab runs) and the H3 credential-binding obligations.
- Effects execute only through the out-of-process helper (OQ-045 final; no ADR-0050 C4 trigger); egress passes EP3 keyed on the kernel-minted attribution token; meters `env.reserved/active/suspended_ms`, `network.calls`, `network.bytes_out/in` are kernel dimensions.
- **Do not re-decide:** topology, identity layers, the helper protocol minimum, the isolation-class vocabulary (H4's enum with the H5/E1 mapping — ontology §5c).

### 6.2 Event-store views the Lab and surfaces may rely on (ADR-0026 as amended; ADR-0066…0068, ADR-0130, ADR-0139)

- The ledger remains the single authoritative store; every Phase 2 subsystem added **projections**, never stores: `handle_table` (H1), `effect_ledger`, `audit_view` with the audit-grade catalogue and the `audit_completeness` vector (H6), `checkpoint` and `restore` (B3), `exposure plan`/catalog epochs (E3), `MemoryStaleIndex` and validity views (D4), health/cooldown (C2), realized model set (C2/C3), claim ledger + gate evaluations (G2), critic verdicts (G3), cache resolutions (C4), environment lifecycle (B5).
- New durable event classes and payload extensions are consolidated in the ADR-0026 Phase 2 amendment log; producers are named there. `security.permission.pending` is durable; `requested{rendering}` stays ephemeral. Tamper evidence: per-run chain → compact-range tree heads → kernel-signed checkpoints → cross-run anchors; witnesses/receipts are C2.
- **Do not re-decide:** one declaration per seam (ADR-0048/0145); the `decider` and `StopReason` sums; content-free audit envelopes (Rule C) with kernel-only producers (Rule P).

### 6.3 The scorecard after Phase 2 (ADR-0045…0047 as amended; ADR-0111, ADR-0114, ADR-0117, ADR-0129, ADR-0059, ADR-0068, ADR-0101)

- Headline metrics remain deterministic-only at C0. Phase 2 added class-scoped declarations with detectors: `claim_state_agreement`, `evidence_traceability`, `execution_alignment_failure_rate` (+ `_judged`), `false_completion_rate` (**veto**), `honest_failure_rate`, `divergence_profile`; `reward_hacking_gap`; `surface_rejection_rate`; `attribution_completeness` (**veto**); `audit_completeness` (**vector veto**); `memory.revoked_delivered` (**veto**) and validity/compliance metrics; LT-01…12 leak battery (**veto**); `reacquisition_rate`, `repeated_action_rate`, `recall_probe@compaction`; `control.reproducibility`, `boundary_drift`; cache agreement/avoided-cost; approval-rate telemetry; the kill-point recovery metrics.
- Veto invariants added to ADR-0047 §5: `false_completion`; the tiering of vetoes by stage is OQ-128 (Phase 3, WS-I2/J5).
- Judges: no default judge; `CriticDeclaration.independence` mandatory; `CalibrationRecord` with expiry; `critic_experiment` is the standard removal test; user simulators are instrument models (`charged_to = instrument`).
- `MatchSpec.cache_policy` is mandatory (`cold_start` default); the engine refuses mixed-warmth arms.

### 6.4 The bundle (ADR-0038 as amended; ADR-0139…0141)

- Three layers: hashed manifest, content-addressed payload, detached attestations; identity by tree rule; `MemberRef`/claim/`unpinned` rules for every member; complete vs valid; `validate_bundle` S1–S9; basis-derived `max_supported_level`; `reproduce` per level with `ReproReport{independent}`.
- Members Phase 3 must emit or consume: `traces[run]` as a `LedgerExport` tree with separate checkpoints; `extensions[]` with trust records and surface pins; `ProfileTestReport`; `pricing` snapshots; `TaskRecord`, `SuiteManifest`, `SuiteValidityRecord`, `SplitAssignmentRecord`, `ParityReport`; `profile_binding` in the manifest; `bundle_ref`/`claimed_level` as **derived annotations** on results rows (WS-J5), never inside row bytes.
- C0/Stage 3 slice: manifest schema, `check_completeness`, `bundle(kind = run)`, R0/R1 for native runs (ADR-0146).

### 6.5 What the registries need (ADR-0023/0036/0037 with ADR-0087…0089, ADR-0063…0065, ADR-0124…0126, ADR-0118, ADR-0121)

WS-J2's registry service must hold, as immutable versioned records under the ADR-0036/0037 identity model: `ToolCapability` versions with `status ∈ {quarantined, sealed, revoked}` and `source_kind` lifting; `ExtensionRecord`/`ExtensionTrustRecord` with pins and attestation refs; `ModelProfile/1` (thirteen rule kinds; capability additions), `ProfileTestReport`, expiry state; `WireDialect` descriptors with debt records (C1 registry); `ModelRoleTable`; `ProcedureProfile/1` records with name histories (`NameCollision` at `resolve`); `SurfaceFamily` variants referenced by id; `VariantRecord.capability_declaration` distinct from `ToolCapability`. Registry events: `lifecycle.capability.*`, `security.extension.*`, profile status transitions.

### 6.6 Protocol edges (ADR-0096…0099, ADR-0021 as amended)

MCP is the sole standardized tool-supply channel (client C0/Stage 3 stdio; Streamable HTTP + OAuth C1; the MCP **server** surface WS-K3 exposes a stateless canonical catalogue per bundle — `listChanged` on bundle change only, run-time selection is client-side, Lab operations via server-minted handles); ACP is the session boundary (v2 agent + v1 compatibility profile with loss report; `_hh/*` extension methods; permission transport per `effect_id`); A2A is the delegation edge (C2). `ProtocolBinding` records with probe-first pinning; unknown protocol data preserved in `ext` at edges and reported in loss reports; peers `unverified` until attested; claims never decide.

### 6.7 ACP verbs available for the Hosting ABI (ADR-0098, CF-023 corrected, CF-208)

- **Baseline (ACP v2 stable core, always present):** session `new`/`load`/`prompt`/`cancel`, `session/request_permission` (per `effect_id`, options as typed outcomes), `tool_call_update` stream (first update creates), agent/plan/mode updates, `usage_update` notifications, `initialize` capability negotiation, `_hh/*` extension methods.
- **Capability-declared (default `unknown`, T-LCD-07):** `steer`, exact `account`, `fork`, `compaction`, `subagents`, `trajectory_export`.
- The internal ecosystem boundary (WS-L1 §6.5; ADR-0050 D3) keeps its full verb set and now carries `idempotency_key` on every work-injecting verb — it is a different binding from the Hosting ABI and Phase 3 must not conflate them (WS-J6, WS-K4).
- Approval persistence across an edge (`allow_always`-style selections) is OQ-246 (WS-E4/H7) — Phase 3 must answer it before the Stage 4 ACP artefact.

### 6.8 Rules of the road for Phase 3 dossiers

1. Cite ADRs by number; the ontology (v2) is the vocabulary tie-breaker; use the §5c canonical names (`ProviderRequestPlan`, `ModelRoleTable`, `decider`, `StopReason`, EP1–EP4, control envelope vs deterministic-envelope strategy).
2. Register additions go to a sidecar with temp ids (`S-WS-XX-NN`, `OQ-WS-XX-NN`, `CF-WS-XX-NN`); synthesis renumbers from S-499 / OQ-346 / CF-322; ADRs from ADR-0147.
3. Every ADR carries the five §3.3 fields, a T-LCD statement and ADR-0024 rungs; every closed-sum growth is a dialect bump, never `ext`.
4. ADR-0050 §8 language-use constraint applies unchanged; CF-017's Phase 3 exposure is WS-K2/K4 — those dossiers must show the constraint was honoured.
5. Precedent file paths are evidence, never dependencies; every claim of production behaviour names a pinned commit.
6. A Phase 3 dossier that needs a Phase 1/2 amendment proposes it as a CF row for synthesis; it does not edit ratified ADRs.

---

## 7. Gate check (doc 3 §6; LEDGER Phase 2 row)

| criterion | result |
|---|---|
| All 31 workstreams `done` with ADRs dispositioned | **yes** — 94 ratified (17 amended), 0 rejected; LEDGER rows updated |
| L3/H1/H2/D1/D4 authority scheme ratified together and consistent | **yes** — CF-006 closed (ADR-0145 §A); amendments to ADR-0034/0035 logged |
| Effect path B2↔E5↔H1↔H4↔B5 composes | **yes** — §2.2 / ADR-0145 §B |
| Context chain D1↔D2↔D3↔D4↔C3↔C4 composes | **yes** — §2.3 / ADR-0145 §C |
| LCD-trap battery applied to C3/E2 with recorded results | **yes** — six rows pass on paper (`registers/lcd-test-battery.md`) |
| CF-005 resolved | **yes** — ADR-0103/0106 |
| No C0 ADR rests on provisional evidence | **yes** — §3 |
| All Phase 2 conflicts dispositioned | **yes** — CF-118…CF-321 (resolved/noted); CF-017 stays pre-registered by design |
| Neutrality + naming audits | **clean** — CF-317, CF-321 |

**Gate: passed.** The orchestrator commits at the phase boundary (RK-10).
