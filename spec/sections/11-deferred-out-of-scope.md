## 11. Deferred and out-of-scope items

*Authority: ADR-0006 (N1–N13; amended P0 CF-019, P3 CF-342), ADR-0003, ADR-0002, ADR-0007, ADR-0009/ADR-0050 (language decision; §6 triggers; §8 constraint), ADR-0179 (OQ-130), ADR-0198 (spec-debt register), ADR-0207 (D-1…D-6), ADR-0208/ADR-0209, ADR-0048/0145/0183, `registers/scope.md`, `registers/open-questions.md`, `synthesis/phase-4.md` §5–§6. Vocabulary per `registers/ontology.md` §6.*

### 11.1 The rule this section discharges

Doc 3 §1.4/§7.3: **every scope item ends Phase 5 as `specified` or `deferred(ADR-####)` — never dropped**; doc 3 §8: **any strategic question unresolved at Phase 5 becomes an explicit deferral ADR**. This section records (a) the thirteen non-goals as out-of-scope boundaries, each with its in-scope neighbour; (b) scope-register deferrals and the tier placements a reader might mistake for them; (c) every open question surviving Phase 4, grouped by subsystem and stage, with the stage it blocks or its program-level standing; (d) the language decision's revalidation triggers and its one deliberately unbound commitment, the surface ecosystem.

Vocabulary: "deferred" here is the register status `deferred(ADR-####)`, unrelated to the runtime `deferred` effect phase (§05a; ADR-0134), the exposure mode `deferred` (§05d; ADR-0093) or the approval response `defer` (§05g; ADR-0070). Maturity is `maturity ∈ {instrument-grade, research-grade}` (ADR-0208 §C.5; CF-453): a `research-grade` item is specified and conditional, never deferred (ADR-0002; ADR-0209 decision 3).

### 11.2 Out of scope: the canonical non-goals N1–N13

ADR-0006 decision 3 is the only non-goals list (ADR-0003's five are N1, N2, N3, N4, N13); §1.5 carries it as positioning. Here each is a boundary with rationale and the section specifying its near side — nothing is dropped.

| id | out of scope | rationale | in scope instead — specified in |
|---|---|---|---|
| **N1** | General-purpose framework for others' core loops | LCD trap by construction (doc 3 §1.1); no mature harness imports a framework for its loop (S-074, `provisional (mech)`; CF-009); "framework" reserved for what others build on top (ADR-0008 rule 2) | Reference runtime as instrument substrate (§1.1; ADR-0003 d.4); `hh-embed/1` for hosts embedding the kernel (§7; ADR-0176…0179) |
| **N2** | Universal Harness ABI treating hosted participants as decomposable | Doc 2 §5.11; ADR-0001/0004; T-LCD-06/-07 | Hosting ABI `hh-hosting/1` — thin, observational, removable (§6; ADR-0164…0166) |
| **N3** | Hosted collaboration / managed multi-device hosting product | Plane owned by Omnigent S-118, Cloudflare S-056, AWS S-058 (CF-013) | Local single-principal surfaces (§7; ADR-0170…0175); multi-tenant fleets = D-2 (§11.3) |
| **N4** | Public leaderboard *service* | HAL S-135, Inspect S-137, Harness-Bench S-062 exist; hosting serves joint science and interoperation (ADR-0005) | Lab leaderboard definition + snapshot views over the results store, published as ADR-0141 exports (R-2.10.5; §6; ADR-0163; CF-342) |
| **N5** | New tool/agent/client protocol | Doc 2 §5.10; S-035, S-153 | MCP/ACP/A2A as lowering targets and ABI substrates (§3 ADR-0021; §05d ADR-0096…0099) |
| **N6** | Behavioural interchangeability of models | Doc 2 §4; family-specific conditioning in every audited runtime (S-049, S-047, S-107/S-110/S-145) | One definition compiled per Model Profile with expirable rules (§3 ADR-0020; §05b ADR-0124…0126) |
| **N7** | Claim that generic beats bespoke | S-075 (`provisional`); CF-009 | The Lab measures the gap under matched budget (§6; §10; ADR-0041/0046) |
| **N8** | Autonomous self-deployment | ADR-0002 (i)–(iv); doc 2 §11 | Governed pipeline: proposal ≠ deployment, human `seal`, G1–G10, `SelfModificationRefused` (§05h; ADR-0194/0195; ADR-0053 D-5; ADR-0182 X6) |
| **N9** | Model-training stack | ADR-0002; ADR-0202 d.1 — HarnessHarness never trains | `training_export/1`, `SnapshotClaim`, compatibility guard; `research-grade`, conditional (§05h; ADR-0202…0204; ADR-0209) |
| **N10** | IDE, chat product, end-user assistant | Doc 3 §2.11 | Generated-client surfaces as kernel projections (§7; ADR-0167…0179) |
| **N11** | Merely an evaluation harness | Doc 2 §1; ontology §4a; ADR-0006 option 2 rejected | The runtime harness is the object of study (§2); evaluation is one plane (§10) |
| **N12** | Language/runtime/framework commitment before WS-L1 | RK-09; ADR-0009 | **Discharged** by ADR-0050; its §8 governs every ecosystem reference (§4; §8; §11.5) |
| **N13** | Benchmark author | WS-A1 §6.1; ADR-0005; CF-019 | Benchmarks integrated, never invented (§05h/§10; ADR-0142…0144); C2 families §11.3 |

Reversing N1 re-opens the LCD trap and is the reversal ADR-0006 makes deliberate; widening into N1, N2, N5, N6 or N12 is out of bounds for every dossier (phase-0 memo §5.1) and, for the spec, possible only by a later ADR amending ADR-0006 (its reversibility clause). No Phase 1–4 ADR widened a non-goal; CF-342 only narrowed N4 to the *service*.

### 11.3 Scope-register deferrals and tier placements

**Finding.** After ADR-0209 and ADR-0210, `registers/scope.md` has exactly **one** row with status `deferred(ADR-####)`: R-2.12.4 is `deferred(ADR-0210)` (register row and 2026-09-11 log entry); the other 64 rows (65 after the R-2.7.2 split) are `specified`/`specified-by-ADR` (ADR-0012…0047, 0051…0144, 0147…0182, 0185…0207; scope ADRs 0146/0184/0209); no item was added or removed; ADR-0010 is the only MoSCoW change. §11.1's rule — every item ends `specified` or `deferred(ADR-####)` — is discharged. This follows ADR-0002: the C4 cluster is fully specified, last on the ladder, never stubbed; R-2.9.8 is `research-grade, conditional` (ADR-0209 d.3), not deferred.

**The one deferred row.** R-2.12.4 *Packaging, licensing, OSS governance, docs & community* (WS-L6; Should) is `deferred(ADR-0210)`: WS-L6 did not run at Phase 5 and ADR-0210 records the interim rule and an entry test for every question it owns (ADR-0050 §7 scheduled it for Phase 5). It owns gaps other sections cite: the product-name qualifier and collision re-check (ADR-0011; OQ-035 follow-up; §1.7), `<hh-prefix>` and the A2A extension URI (OQ-068; §3), pricing-table governance (OQ-093), the extension trust root (OQ-162), retention durations (OQ-173; OQ-085 half), detached-payload trust (OQ-332), sunset defaults (OQ-411), the executable name (OQ-383), spec-debt revalidation ownership (OQ-445; ADR-0198 D4). **`deferred(ADR-0210)`** — WS-L6 is the deferred owner; ADR-0210 records one entry test per owned question (OQ-035 follow-up, OQ-068/394/218, OQ-093, OQ-162, OQ-173, OQ-332, OQ-383/384, OQ-389, OQ-411, OQ-445) and the interim rule in force; re-entry as `specified` is a scope ADR citing those tests.

**Tier placements inside specified items (not deferrals).** Ratified ADRs place sub-mechanisms "deferred to C*n*"; each is a stage placement under the extension DAG (ADR-0182 X1–X6; T-LCD-06) and is specified in its owning section.

| ADR | placed later | tier / condition | section |
|---|---|---|---|
| ADR-0023 | Interpreted policy leaves | C1 unless OQ-077 admits them; C0 = `Text`/`CompiledPayload` | §3 |
| ADR-0018/0021 | Open Agent Spec, A2A targets | C2, declared-lossy | §3 |
| ADR-0051 | Per-value capabilities | C2, same lattice | §05g |
| ADR-0067 | Witnesses, receiver receipts | C2 (OQ-171: check dispute resolution against N1–N13) | §05g |
| ADR-0073/0022 | Executed offload | C2 | §05c |
| ADR-0116 | Untrusted-monitor protocols | C4 with WS-I5/H1 (OQ-280 rule: ADR-0195 G7) | §05f |
| ADR-0129 | Similarity-keyed semantic cache | C2 with mandatory verifier | §05b |
| ADR-0144 | `browser_computer`, `search_research`, τ²-bench dual control, `persistent_multi_episode` | C2 (OQ-341) | §05h/§10 |
| ADR-0153 | Registry federation | C2; import-first ratified (OQ-357) | §6 |
| ADR-0175 | MCP `tasks` carrier, ledger resources | C2 (OQ-397) | §7 |
| ADR-0179 d.5 | Non-local hosts | Fourth binding by ADR with WS-H3/H4 rules (OQ-401) | §7 |
| ADR-0199/0209 | Attribution M2–M5 | C4/Stage 6; M1 C2/Stage 5 | §05h |

**The ratified deferral set: ADR-0207 D-1…D-6.** The only pre-Phase-5 deferral ruling; D-1…D-6 sit outside the build ladder (ADR-0209 d.2) and enter only by an ADR passing the entry test.

| id | deferred | why (ADR-0207 D7) | interim | entry test |
|---|---|---|---|---|
| **D-1** | Human directory and rotations | Identity-provider concern outside every contract, inside N1 (CF-446) | `ApproverGrant`s, static `EscalationTarget`s | `PrincipalDirectory` class contract by ADR with two out-of-process precedents (OQ-462) |
| **D-2** | Multi-tenant fleets | Sessions single-principal at C2 (ADR-0174); N3 | One principal per session | Tenant boundary as a `readers` partition, no new authority class |
| **D-3** | Value-at-risk pricing of irreversibility | One `provisional` preprint | `ext.effects.external_irreversible` counter (promotion to ADR-0039 list at Phase 5 if two consumers — CF-445, OQ-463) | Pricing rule with debt record + `ComparisonReport{benefit_kind = artifact_benefit}` vs the counter |
| **D-4** | Dispatch prioritization by expected value | Owner WS-F4 (ADR-0207 D7) | `SchedulingPolicy.order` is the slot | ADR-0211: a `compute_policy` variant binding `SchedulingPolicy.order` at the ratified seam, evaluated at `matched_total` against `static` order with `TaskValue` as the value input |
| **D-5** | Reconciler-side source mirroring | Needs a principal-less writer (ADR-0053 D-5) | Mediated tracker writes with `ActionPattern`s (ADR-0206) | AC-L8-06 with the procedure's effects recorded |
| **D-6** | Cross-item shared-state coordination | Owner WS-F5 (ADR-0207 D7); the F3/L8 seam is intra- vs inter-activation (ADR-0208 §D.4) | None named; ownership is per coordination object within one activation (ADR-0191) | ADR-0211: an `OwnershipRecord` outliving an activation, fenced through `resource(key)` leases with OQ-431 escrow, proven at `matched_total` with `lost_write_count = 0` |

### 11.4 Open questions unresolved after Phase 4

**Count.** The register holds 466 rows (next OQ-467). After the Phase 4 fold, 264 are `open` and 14 are half-open (OQ-025, 085, 107, 112, 113, 124, 154, 165, 173, 189, 242, 281, 331, 350). Nineteen rows resolved in a Phase 4 note but still reading `open` in the status cell (OQ-007, 008, 009, 012, 013, 017, 026, 027, 082, 135, 147, 216, 278, 304, 313, 323, 347, 382, 390) were rewritten to `resolved (Phase 4)` at Phase 5 (register hygiene; ADR-0210 log) and are not counted. Classification follows the memos (phase-4 §5; phase-3 §5 "stands"; phase-2 §5 residue): a question **blocks a stage** when a C0–C3 contract cannot be built there without it and a MUST-data placeholder is in force until the named measurement; otherwise it is **program-level**. Doc 3 §8 requires a deferral ADR for every blocking question open at Phase 5; none existed after Phase 4 (ADR-0208: "nothing is ratified as deferred here"). Phase 5 authored them: ADR-0212 (Stage 0–1), ADR-0213 (Stage 2–3), ADR-0214 (Stage 4–6), ADR-0215 (program-level candidates and OQ-440), beside ADR-0210 (R-2.12.4) and ADR-0211 (D-1…D-6); each row below names its ADR, and each ADR row is a `registers/spec-debt.md` entry (§11.6).

#### 11.4.1 Stage-blocking questions (deferral ADR required)

| stage | OQ | subsystem · owner | in force until closed | deferral ADR |
|---|---|---|---|---|
| **0** | OQ-131, OQ-402 | language decision, `hh-embed/1` · WS-L1 role, WS-K4, Stage-0 ticket | spike S1/S2 as acceptance check; bounds MUST-data from S2 (ADR-0179 D6); `unmeasured-by-spike` marks | ADR-0212 |
| **1** | OQ-132 | monitor · WS-H1/H4/H3/E1 | handle table awaits `ResourcePattern` grammar (AT-H1-04) | ADR-0212 |
| | OQ-133 | monitor · WS-H1/E5 | partial parse ⇒ `unknown` (ADR-0100); cost at Stage 3 | ADR-0212 |
| | OQ-142 | monitor/approvals · WS-H1/H7 | unattended `ask → deny` (ADR-0052; ADR-0208 §D.5) | ADR-0212 |
| | OQ-149 | HIR/credentials · WS-A3/A5/H3 | `SecretRef` per ADR-0057; leaf vs `Ref` open (LT-08) | ADR-0212 |
| | OQ-165 (HIR half) | control/HIR · WS-F1/A3 | output sum `GuardVerdict` fixed (ADR-0181 D6); placement open | ADR-0212 |
| | OQ-201 | monitor/memory · WS-H1/D3 | AC-D3-8 awaits a Π row | ADR-0212 |
| | OQ-219 | compiler · WS-A4/E2/E1 | JSON-Schema 2020-12 default; C0 keyword subset unlisted | ADR-0212 |
| | OQ-223 | HIR/monitor · WS-A3/H1 | hint-lifted effects take `world` from `openWorldHint` | ADR-0212 |
| | OQ-235, OQ-240 | tool plane/ledger · WS-B1/E3/E1 | names and record fields provisional; hosted lifting table re-keyed on close; §05d gap | ADR-0212 |
| **2** | OQ-160 | approvals/egress · WS-H7/H4/L3 | cache scope undecided; never `principal`-persisted | ADR-0213 |
| | OQ-170 | audit/credentials · WS-H3/H6/L4 | signer custody unassigned (ADR-0067 checkpoints) | ADR-0213 |
| | OQ-178 | capabilities/leases · WS-E1/E5/A4 | `ActionPattern` per class; L8 form fixed (ADR-0206 D3) | ADR-0213 |
| | OQ-186 | context/capabilities · WS-E1/H1/D1 | one kernel `read_artifact` assumed | ADR-0213 |
| | OQ-307 | caching · WS-C4/C3 | `static_hash` contents (LC-2) | ADR-0213 |
| | OQ-387 | monitor/trust · WS-H1/H5/K1 | `unknown` narrows as `untrusted` | ADR-0213 |
| **3** | OQ-030 | Hosting ABI/results · WS-J6 | ATIF vs superset; marker `applied_ops` (ADR-0075); J5 import rows | ADR-0213 |
| | OQ-113 (margins) | scorecard/bundle · WS-I2/I7/J4 | consumers fixed (ADR-0140/0142/0199 D6); margins pre-registration data | ADR-0213 |
| | OQ-150 | egress · WS-H4/B5/E5 | TLS termination site (LT-02) | ADR-0213 |
| | OQ-157 | benchmarks/environments · WS-I4/B5/J3 | default `isolation_class` | ADR-0213 |
| | OQ-210 | ontology/verification · WS-A2/G1/B1/D4 | memory `followed` detectors (T-LCD-13) | ADR-0213 |
| | OQ-273 | verification/monitor · WS-G1/H1/F2 | autonomous validators at the gate | ADR-0213 |
| | OQ-336, OQ-338, OQ-342 | benchmarks · WS-I4/I3/G1/I2 | foreign verifiers under N13; fresh SWE-style pool; parity margins | ADR-0213 |
| | OQ-363 | matched budget · WS-L2/J3/I2 | before Stage-3 `lab/control-strategy-family-v1` (AC-J3-9) | ADR-0213 |
| | OQ-360, 366, 367, 375 | experiments/analysis/results · WS-J3/J4/I2/J5 | placeholders: re-attempts; `draws` 2000/200; `T_clt` 100, `T_bca` 30; admission floors | ADR-0213 |
| | OQ-407 / OQ-075 | extensibility/accounting · WS-L2/I1/L5 | `hot_path` ceiling (AC-L5-9) | ADR-0213 |
| **4** | OQ-246 | approvals across an edge · WS-H7/H1/L3/E4 | `allow_once`/`deny_once` only (OQ-386/405 wait) | ADR-0214 |
| | OQ-116 | hosting/audit · WS-J6/L6/H6 | hosted `model_io` consent unassigned | ADR-0214 |
| | OQ-177 | provenance/approvals · WS-L3/H1/H7 | reviewer authority under `ApproverGrant` | ADR-0214 |
| | OQ-242 (pin) | ACP edge · WS-K4/J6 | removal test fixed (ADR-0197); v2 pin open | ADR-0214 |
| | OQ-378…381 | Hosting ABI · WS-J6 | `StopReason` lifting; placement; re-probe; liveness | ADR-0214 |
| | OQ-388 | ledger projections · WS-B1/K2 | latency bound at 10⁵ events | ADR-0214 |
| | OQ-389, 392, 401 | surfaces · WS-K4/K2/I1/H3/H4 | token delivery; `SinkPolicy` defaults; localhost-only | ADR-0210 |
| | OQ-400 | Hosting ABI/MCP server · WS-J6/K3/B5 | supply-surface lifecycle | ADR-0214 |
| | OQ-413 | subagents · WS-F3/F2/C3/D1 | `ReturnContract.summary.max_tokens`; C1 admits `none` | ADR-0214 |
| | OQ-417 | accounting/HIR · WS-L2/A3/F3 | message caps `ext` vs kernel list (ADR-0191 M-2) | ADR-0214 |
| | OQ-418 | approvals/monitor · WS-H7/H1/F3 | child inherits parent `attendance` | ADR-0214 |
| | OQ-424 | extensibility/control · WS-L5/F1/F4 | `bind` hot-path ceiling | ADR-0214 |
| | OQ-430 | merge/verification · WS-G1/G2/F5 | G-4 default (ADR-0192) | ADR-0214 |
| | OQ-432 | memory lifecycle · WS-D4/H7/F5 | parent `supersede` basis (ADR-0082) | ADR-0214 |
| | OQ-433 | ledger · WS-B1/B3/F5 | HLC mandatory in `share` (ADR-0193 K-2); placement open | ADR-0214 |
| | OQ-434 | Hosting ABI/coordination · WS-J6/F5 | `ChildOutcome` lifting; else `unknown` | ADR-0214 |
| | OQ-453 | Hosting ABI/attribution · WS-J6/I5/I7 | `observation_substitution` provisional (ADR-0200 V9) | ADR-0214 |
| **5** | OQ-368 | analysis/profiles · WS-J4/I6/J6 | `max_age` 90 d; re-probe = `revalidation.schedule` | ADR-0214 |
| | OQ-420 | scheduler · WS-F4/C1/J3 | `bandit` window/discount/`min_n` | ADR-0214 |
| | OQ-441 | assumption debt · WS-I6/C3/I2 | `DebtPolicy` proposed defaults | ADR-0214 |
| | OQ-446 | debt/identity · WS-I6/H1/K1/K2 | `PrincipalRef` vs `TeamRef` (joins D-1) | ADR-0214 |
| **6** | OQ-435, OQ-436 | evolution · WS-I5/I2/J3/J4 | S3 `min_flip_share` 0.5; S5 `retention` margin | ADR-0214 |
| | OQ-438 | evolution/diff · WS-A3/L4/I5 | re-propose against moved head (ADR-0195 D12) | ADR-0214 |
| | OQ-448 | attribution · WS-I7/J4/J3 | OQ-323 constants until A13 `power` | ADR-0214 |
| | OQ-450 | attribution/scheduler · WS-F4/I7/L2 | refuse-vs-degrade is campaign data | ADR-0214 |
| | OQ-457, OQ-458 | co-evolution · WS-I6/I2/C3 | `k_cycles`, margins; `scope.snapshots` default (ADR-0203 D1) | ADR-0214 |

Every placeholder is a conditioned rule (T-LCD-05): the closing deferral ADR carries an assumption-debt record whose `removal_test` is the named measurement (ADR-0197 D6; ADR-0198 D1) and a spec-debt row (§11.6).

#### 11.4.2 Phase-5 deferral candidates (phase-4 memo §5; program-level)

Non-blocking; the memo instructs §7.2-11 to ratify them (§6.3 item 5). The interim rule is ratified in each case.

| OQ | question | owner · entry/decision test | deferral ADR |
|---|---|---|---|
| OQ-422 | Certaindex-style probes: `Validator{kind: judge}` vs `router_predictor` | WS-G3/C2 · calibration and independence for `predictor` variants | ADR-0215 |
| OQ-423 | Cross-design evaluation budget (OQ-025 half) | WS-I5/F4 · portfolio allocator over `adaptive_search` designs; within-campaign = `SlotAllocationPolicy` (ADR-0196 D4) | ADR-0215 |
| OQ-425 (OQ-292) | HIR/2 `route`/`effort` `DecisionPoint` vs bind step | WS-A3/F1 · HIR/2 backlog with `derived-from.hypothesis_ref` (CF-416) | ADR-0215 |
| OQ-429 | `ResourceKey` namespace registration | WS-F5/E1/J2 · `NamespaceRecord` rule; schema is ADR-0191 D1 | ADR-0215 |
| OQ-431 | Ownership escrow when a grantor never restores | WS-L8/B3/F5 · owner fallback (ADR-0207 O-5); escrow held (ADR-0193 D3) | ADR-0215 |
| OQ-437 | `live_split` design kind | WS-J3/J4 · unpaired estimator with `budget_match`; `split` canaries `exploratory` at 6a (CF-421) | ADR-0215 |
| OQ-439 | Evolving the evolution service | WS-H1/I5/L5 · validators disjoint from every campaign suite | ADR-0215 |
| OQ-443 | Coalition credit (ADR-0200 M3) into `RemovalVerdict` | WS-I7/I6/J4 · pairwise designs under the OQ-364 ceiling | ADR-0215 |
| OQ-449 | Provider-side noise coupling as probed capabilities | WS-C1/C3/I7 · gateway declaration + probe; M5 `exploratory` | ADR-0215 |
| OQ-451 | Calibrated judged outcomes in `confirmatory` attribution | WS-G3/I2/I7 · per-domain calibration; V5 labels `judged` | ADR-0215 |
| OQ-456 | Attached-mode reward subscription over Group R | WS-K4 · ADR-0178 experimental opt-in with removal test | ADR-0215 |
| OQ-459 | Judged rewards export vs deterministic-only first cut | WS-G3/I2 · OQ-127 interaction; ADR-0202 D5 | ADR-0215 |
| OQ-461 | Fleet sharding and migration | WS-L8/B1/H6 · one fleet per definition + source set | ADR-0211 |
| OQ-462 | `PrincipalDirectory` contract (D-1) | WS-L8/L3/H7 · two out-of-process precedents | ADR-0211 |
| OQ-466 | Responder simulation for `lab/org-policy-v1` | WS-J3/G3/L8 · calibration set + independence axes (ADR-0116) | ADR-0215 |
| D-2…D-6 | §11.3 | ADR-0207 D7 entry tests | ADR-0211 |
| OQ-440 | Promotion test `research-grade → instrument-grade` | WS-I5/L7/J2 · fixed in the readiness report | ADR-0215 |
| OQ-445 | Spec-debt mechanics; post-Phase-5 revalidation owner | Phase 5 synthesis / WS-L6 · ADR-0198 D1 | ADR-0210 |

#### 11.4.3 Program-level open questions by subsystem and stage

Answered by the owner when the tagged stage is reached; interim rules are in the register cell.

| subsystem (section) | open questions · stage |
|---|---|
| Foundations, HIR, compilation, identity (§2, §3, §8) | OQ-005 (frozen Phase 5); OQ-061 (C1 at 6c), 109, 184, 191, 198, 200, 213 (HIR/2), 217 (HIR/2 `Call`), 259 (C1), 326, 327, 469 (Stage 1; `ValidatesRecord` dialect home — interim: a verification-plane record, not an HIR/1 edge attribute; §05f.1) |
| Runtime, durability (§05a) | OQ-024 (advanced), 115, 166, 225, 230 (Stage 5), 263 (Stage 1 spike), 295, 315, 317 (C2), 318, 319, 320, 322, 324, 325 (Stage 1 `never`/`fail_run`), 329, 397 (C2) |
| Model plane, caching (§05b) | OQ-111, 122, 188, 192, 196, 238, 264 (C1), 274 (C2), 275, 286, 287, 288, 289 (C1), 290, 291, 296, 299 (interim: `range` unsupported at C0; fixtures use `exact`/`prefix`/`any`), 300, 301, 302, 305, 306, 308, 309, 310, 311, 312 (C2), 362 — Stage 3–5 |
| Context, memory (§05c) | OQ-010, 168, 181, 193, 194, 197, 199 (C2), 203, 205, 207, 209 (Stage 3), 211, 212 (→ 352), 215 (Stage 3), 283 |
| Tools, action, protocol edges (§05d) | OQ-055, 068 (L6), 069, 077 (C1), 138, 146, 163, 164, 183, 218 (L6), 220, 221, 222 (C2), 224, 226, 227, 228, 231, 232, 234, 237, 243 (C2), 244, 245, 249, 250, 251 (Stage 1), 254, 260, 267, 277, 408 (Phase 5) |
| Control, orchestration (§05e) | OQ-189 (half), 255 (Stage 3), 256 (Stage 3; WS-I2 with WS-F1/B4 — interim: `control.reproducibility` labelled by replay mode, not headline-eligible), 262, 269, 316, 415, 416 (Stage 5), 426, 428 (Stage 4) |
| Verification (§05f) | OQ-145 (C2), 167, 176, 268 (C1), 271 (C2), 279, 281 (half), 282 (C2), 284, 345 |
| Security, governance (§05g) | OQ-103 (→ 139), 107 (half), 136, 140, 141 (Stage 3), 143 (Stage 3), 151, 154 (C2), 155 (C2), 156, 162 (L6), 169 (Stage 2), 171 (C2), 172, 182, 330, 334 (C2), 386/405 (after 246), 395 |
| Measurement, evolution, debt, attribution, co-evolution (§05h, §10) | OQ-093 (L6), 121, 127, 153 (Stage 3), 253, 270, 280, 337, 339, 341 (C2), 025 (→ 423), 350 (half), 358 (Stage 3), 364, 374 |
| Harness Lab, registry, results, hosting (§6) | OQ-085, 112, 124, 173, 331 (halves), 100, 108, 144, 152, 158, 159, 175 (C2), 190, 276, 343 (Stage 4), 352, 355, 356 (Phase 5; every Phase 2 class owner owes a `ConformanceSuite`), 357 (C2), 361 (C1), 373, 376, 377 (Stage 4) |
| Surfaces, embedding (§7) | OQ-383 (L6), 384 (Phase 5), 396, 398 (Stage 5), 399, 406 |
| Extensibility, program (§8, §9, §11) | OQ-332 (L6), 335, 409 (Phase 5), 410 (ADR-0050 trigger 3), 411 (L6), 412 |

### 11.5 The language decision: revalidation triggers and the surface-binding deferral

ADR-0050 (ratified 2026-09-10; amended P2, P3) is the only decision naming ecosystems; its §8 confines every other artifact to references "by ADR id and layer" for host requirements, protocol-SDK availability and sandbox primitives. The decision is polyglot split E5a with a **late-bound surface layer**: kernel and helper bind at Stage 0–2, the laboratory at Stage 3, surfaces at Stage ≥ 5. Its win share (0.746 < 0.8) was settled by tie-breaker T4, so it pre-registers re-run conditions (§6, AC7; owner: the WS-L1 role via the synthesis agent), carried into the spec-debt register as the ADR-0050 row's `revalidation.on` list (ADR-0198 D2).

| trigger | condition | state after Phase 4 |
|---|---|---|
| 1 | WS-A3 makes statically checked closed sums or effect typing *required* (C6 gate) | **Unfired**; ADR-0015 unchanged; CF-112 stands |
| 2 | Sandbox helper moved in-process (C4 gate) | **Closed unfired**: OQ-045 final = out-of-process (ADR-0100/0136) |
| 3 | WASI 1.0 ships (S-165) — re-score C4(c), plugin substrate | **Pending, external**; `component_model` placement adopts on it (OQ-410; C2) |
| 4 | A relied-upon Tier-1/official MCP/ACP/A2A SDK deprecated or demoted | **Unfired** per S-155/156/157 (2026-09-10); re-checked each synthesis pass |
| 5 | Stage-0 spike C5/C7/C12 outside assumed ± 1 | **Pending, Stage 0**; no spike run (doc 3 §11.0); OQ-131 clears the `unmeasured-by-spike` marks or re-runs ADR-0009 steps 5–7; OQ-402 takes S2 bounds; reversal cost low |
| 6 | A surface cannot be served by a generated client over the local transport (Q-L1-14 narrowed) | **Did not fire** (OQ-130; ADR-0179 d.4; ADR-0170 d.7): every surface needs only the D2 record set + Group L; desktop/IDE claim falsifiable at Stage ≥ 5 by AC-K4-11 |
| 7 | ADR-0009's own trigger set | Stands; subsumed by 1–4 |
| (d) | Primary environment class becomes browser/computer-use first | Conditionality, not a trigger: C4/C8 ranges re-derived (ADR-0009 (d)); coding/terminal stays primary (ADR-0144; N13) |

**The surface-binding deferral (OQ-130).** ADR-0050 D1 declares the web-native class as surface *default* and defers the commitment: WS-K2 binds at Stage ≥ 5 when the kernel↔surfaces boundary is first exercised; reversibility is zero until then, low after (regenerate the client — §5). Phase 3 confirmed rather than closed it: every surface is a generated client of `hh-embed/1` over one of three bindings (ADR-0179), so nothing in §7 depends on the outcome. Deliberately unbound: (i) the surface ecosystem; (ii) the residual RK-09 exposure — CF-017 resolved with "residual exposure = WS-K2 Stage ≥ 5 binding", CF-116 an accepted tension; (iii) desktop/IDE operation lists (AC-K4-11). Binding is an amendment to ADR-0050's log by WS-K2, never a new language decision, unless trigger 6 fires then.

ADR-0050 §8 is unchanged by every pass (CF-317, CF-392, CF-465): no artifact, this spec included, names a language, runtime, package, library API or build tool in a contract, data model, criterion or stage plan; boundary placement is stated in D2 terms and `hh-embed/1` verbs. The readiness report re-runs the language-leak audit (AC8).

### 11.6 Spec-debt rows and the deferral-ADR obligation

ADR-0198 D1 creates `registers/spec-debt.md` at Phase 5 synthesis — **generated at the Phase 5 fixer pass (2026-09-11): 216 rows, one per ADR-0001…ADR-0216, with the D3 gate evaluated in its header (no C0 row `hypothesized`; T1–T5 on every row; T5 names the model generation on every C4 row)** — one `AssumptionDebtRecord{debt_class: spec_decision}` per ratified ADR, (d) as `hypothesis`, (a)/(c) sources as `evidence_refs`, triggers in the ADR-0050 form (source superseded/refuted; `provisional` replicated or failed; a stage measurement contradicting (d); an amending ADR; a model-generation change), `removal_test{kind: revalidation_review}`. Its readiness-gate rule (D3; doc 3 §7.3) binds this section's outputs: no C0 ADR `hypothesized`; every C0 ADR with a `provisional` ref carries triggers; every C4 ADR names its assumed model generation. Consequences:

1. **Every deferral ADR from §11.3–§11.4 (ADR-0210…ADR-0216) carries a spec-debt row** (phase-4 memo §6.3 item 5; rows present in the generated register) whose `removal_test` is the entry test or measurement and whose `owner` is the tabled workstream; a deferral without an entry test is refused as a conditioned rule without a removal test is refused at `seal` (ADR-0197 D6; T-LCD-05 reflexively).
2. **The ADR-0050 row** carries triggers 1–7 and `expiry{condition: evidence_refresh_due}` keyed to the SDK-tier verification date; `evidence_max_age` is OQ-445.
3. **Post-Phase-5 ownership** of `revalidation_review` (ADR-0198 D4: WS-L6) is open (OQ-445) and joins the R-2.12.4 gap.

A deferred item entering later must be removable without touching a lower tier (ADR-0182 X2/X4; T-LCD-06) and may not be closed by widening a non-goal (§11.2) or adding a second execution contract (ADR-0179 d.2).

### 11.7 Scope coverage and cross-references

| scope item | disposition here |
|---|---|
| **R-2.12.3** Language/ecosystem selection (Must, as decision) | **Specified (ADR-0050)**; triggers and surface deferral §11.5 |
| **R-2.12.4** Packaging, licensing, OSS governance, docs & community (Should) | **`deferred(ADR-0210)`** — entry tests in ADR-0210; §11.3 |
| R-2.12.5 Thesis & naming | Non-goals restated §11.2; text §1 |
| R-2.12.6 Organizational layer (C4) | D-1…D-6 §11.3; contracts §05e/§05h |
| R-2.6.3…5, R-2.9.5…8 (C3/C4) | Specified with maturity flags (ADR-0209); Stage-6 rows §11.4.1; candidates §11.4.2 |
| all others | `specified-by-ADR`; tier placements §11.3 |

Obligations: §1.5 stays the positioning copy of N1–N13 (no fourteenth); §3 and §05d carry the OQ-068/OQ-240 gaps listed here; §7 owns the three-binding contract and AC-K4-11; §9 places each §11.4.1 placeholder at the stage it blocks and D-1…D-6 outside the ladder; §10 pre-registers the Stage-3 measurements closing OQ-360/366/367/375 and OQ-363; the readiness report carries the RK-09 audit, the OQ-440 test and the `spec-debt.md` gate. The assembler authored ADR-0210…ADR-0215, rewrote the nineteen stale status cells (§11.4) and moved R-2.12.4 to `deferred(ADR-0210)`; no scope row violates doc 3 §1.4. The readiness pass re-checks that every deferral row has its `registers/spec-debt.md` entry (generated; ADR-0210…ADR-0216 present), confirms the mechanically derived `evidence_grade`s with their owners, and re-runs the OQ-440 obligation. The Phase 5 fixer additionally authored ADR-0216 (OQ-465 second half, OQ-467, OQ-468) and logged CF-468…CF-478.
