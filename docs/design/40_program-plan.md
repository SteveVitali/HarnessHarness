# 40_program-plan — Program plan and synthesis readiness
Stream: H
Researched: 2026-11-07 (inspection and computation 2026-11-07T00:18Z–00:41Z)
Author: design worker for ledger row H1 (Claude Code subagent, claude-opus-5-5), dispatched by the experiments-program coordinator
Ledger rows covered: H1
Repository baseline: main 09fe8523. Read in worktree `claude/experiments-spec` at HEAD 823e85ab; `git diff --stat 09fe8523 HEAD -- crates scripts spec docs/adr docs/tickets docs/build` is empty, so the branch adds only program documents. No working-tree change was read or made outside this note.
Confidence overall: high for the repository state, the conflict inventory and the register contents (source inspection, plus one recomputation over committed packets). Medium for the rulings, the DAG, the linearization, the shared-pilot definition and the sizing: these are design proposals. Low for schedule and wall-time estimates, which rest on unmeasured throughput and priors.

## Scope, inputs and method

H1 integrates the sixteen accepted notes (research 24–29, design 30–39) into one program plan for the synthesizer (O1) and the decomposer (O4). It decides nothing the operator owns: every gate below is recorded with its pre-registered content and is never answered here, and every recommendation is labelled.

**Read in full:** root `AGENTS.md`; `docs/experiments-brief.md`; `docs/experiments-research-ledger.md` (row H1, the whole §Q register, the change log); `docs/experiments-OUTLINE_TRACE.md`; `docs/research/CONVENTIONS.md`; the coordinator's mechanical digest of all sixteen notes (`<scratchpad>/h1-digest.md`: headers, every finding title and Claim, the four closing sections verbatim).

**Read as bounded ranges, to resolve conflicts or verify facts used here:**
- 30 F1 and F14; 31 §1.3, §1.6, F4, F18, §3; 32 F11–F12; 33 §1.1, §3, §4; 34 §1.8–§1.9, §3, §4; 35 F2, F4–F5, F9, F17; 36 §1.4, §2.1, §2.3, §2.6, Part 7; 37 §2.5, §4.2, Part 5, Part 6; 38 Part 5, §6.4, Part 7; 39 F10, F14.
- `docs/tickets/00_MANIFEST.md`: header, `## How to build`, `## Human prerequisites`, chain rows 216–251, `## Cross-cutting invariants`, `## Operating rules` (Round 5 scope), `### Round 5 supplemental requirement ownership`, `### Round 5 decomposition decisions`, `## Plan extensions` (first and last entries).
- `docs/build/LEDGER.md` `## CURRENT STATE`; `docs/build/tools/README.md`.
- `~/agent-skills/skills/decompose-spec/SKILL.md` (objective, Phase 2, tail step 5); `~/agent-skills/skills/build-memory/layout.md` l.198–312 (manifest, ticket, marker and tail rules).

**Executed (read-only):**
- `bash scripts/deferrals-open.sh` at 2026-11-07T00:21:32Z: no output, exit 0. That is the valid zero-open-rows result.
- `python3 -I` one-off reads of `docs/build/reports/2026-11-05-parity-tranche-3.json` and the four S4 packets, to rule on the cross-check denominator (C-06).
- `ls docs/adr` (latest ADR-0422); grep of the manifest for the highest GATE and HUMAN ids (GATE-G19, HUMAN-H11).

Nothing was built, provisioned, spent, fetched or committed.

---

## Part 1 — Cross-note conflict register with rulings

Each ruling is a **design proposal** binding on synthesis unless O2 or the operator overturns it. "Governs" names the note whose text the spec adopts. Rows C-01…C-06 and C-08…C-15 are the ones the assignment named; the rest were found while integrating.

| id | conflict (notes) | ruling | consequence for spec and decomposition |
|---|---|---|---|
| C-01 | mini-swe-agent's action protocol: C1 F6 calls it text-protocol; E1 F2/F12 show `tools=[BASH_TOOL]` on every request (D2 F13 already ruled) | **E1 governs.** mini-swe-agent 2.4.6 uses native tool calls, so Ollama's GLM tool-call parser sits inside its loop. C1 F6's text-protocol statement holds only for Terminus-2. | The spec never calls mini a text-protocol harness. The parser is symmetric across L2 arms and invisible to L1 (replayed responses are already parsed). REQ-XC-013's raw-plus-parsed tool-call retention is load-bearing for mini. Pilot infra classes must separate tool-parse failures from harness failures (C3 F13). |
| C-02 | Egress profile: C2 F5/ADR-C2-C admit `model_proxy` with "exactly one socket"; B2 F18 composes model and registry sockets | **B2 governs.** One profile, `allowlist_mediated`, under ADR-B2-EGRESS, with ADR-C2-C folded in. The relay becomes `hh-cell-relay` over a closed pair list `{model, egress}`. "Exactly one socket" becomes "exactly the sockets the sealed phase schedule grants". | C2 ticket 5 merges into B2's EGRESS ticket, so the relay and helper mount flag land once. REQ-XC-102 survives as the model-channel component of REQ-XB-113. The proxy, its token, budget and receipts stay C2's. |
| C-03 | Store topology: C2 §1.1 writes the phase proxy run "in the store"; B2 F4 shows a store root is single-writer per process | **B2 governs.** The proxy and the egress mediator each own a separate store root as sole writers. Slot cost folds read them read-only. The coordinator's experiment store holds only the subject runs that `settle` reads. | REQ-XB-104 is the rule; C2's PROXY-CORE acceptance adds "opens only its own root". Cross-process read of a root another process is writing was not inspected (B2 F4 limitation), so the RUNNER-CORE spike must cover it. |
| C-04 | Podman mode: C2 §1.1 assumes rootless; B3 F10/REQ-XB-205 specify rootful through the local socket; B2 F18 flagged it | **B3 governs (rootful).** The round-5 GCE backend was rootful (A1, via B2 F18), and B3 owns the backend. | C2's socket permissions (0600) and relay live leg are validated on rootful podman on the B3 image. The word "rootless" does not enter the spec. |
| C-05 | No hh-embed edge in the runner (B2 REQ-XB-103) vs E2's fallback if the cross-process cell handoff fails (E2 F9, open question 7: relax REQ-XB-103 for HH-native slots) | **REQ-XB-103 stands.** E2-T8 starts with the detach/reattach spike. Fallback ladder, in order: (1) the spike passes, design as written; (2) the kernel child keeps cell custody through seal and verify, driven by the slot worker over binding (b) with existing `env.*` verbs, adding no crate edge (design proposal, UNVERIFIED); (3) only then an in-process binding (a) exception for HH-native slots, by an ADR amending ADR-B2-RUNNER, raised to the operator through a scoped `decompose-spec` insert. Never relaxed silently. | E2-T8's contract carries the ladder as a stop rule. REQ-XE-120 is conditional on (1) or (2). |
| C-06 | HH-cell × `tb` cross-check denominator: A2 F1 basis A = 16 tasks / 14 agree; coordinator spot-check (ledger 2026-11-06T23:20:00Z) = 17 / 15 | **A2's 16/14 is correct.** Recomputed (F3): the 17th task is `hf-model-inference`. Both of its confirmation-packet `first` slots are dispositioned `inconclusive{environment:cell_network_isolated:Connection refused}`, yet carry a receipt verdict `unresolved`. Counting that receipt reads a verdict out of an inconclusive slot, which relabels a typed disposition. **Basis-A rule, fixed precisely:** a task enters basis A iff it has ≥ 1 slot in `2026-11-05-s4-confirmation.json` with `first == true`, `disposition == "executed"` and `receipt.verdict ∈ {resolved, unresolved}`. Its HH verdict is the common verdict of those slots; if they disagree the task is `discordant{h_internal}` (none occur). It agrees iff that verdict equals tranche 3's `o_verdict`. Slots with a receipt but a non-`executed` disposition go to the confound list as `inconclusive_with_receipt`. Taking the first record per `slot_ref` across the four packets in chronological order gives the same 16/14. | P1-B pins 16/14/2. Its mutation battery adds one case: admitting `inconclusive_with_receipt` slots must yield 17/15 and fail the assertion. `hh-surface-xcheck/1` lists `hf-model-inference` as a confound. O2 re-checks. |
| C-07 | Trajectory fetch: F1 REQ-XF-009 forbids all third-party runtime requests; F2 REQ-XF-132 admits one in-site fetch from the public HF dataset | **REQ-XF-009 is the rule.** REQ-XF-132 is one declared, user-initiated, sha256-verified exception. It stays dormant until a dataset visibility flip (HUMAN, after the Q-X35 scans). Ratification picks option (a), the exception, or (b), dataset links only (Q-X43). | The `no-third-party` leg encodes exactly the declared origin set: one origin under (a), none under (b). F2-d builds the viewer behind that switch. No conflict is live before the first dataset publication. |
| C-08 | Pilot sizing: D1 F9 (S0 + P0 3×1×8 + P1 10×2×8, ≈ $44.3 planning); C3 (8×10×2 or 6×15×2, flat $6 caps); C4 §1.8 R-live (24 or 8 O trials); D2 §2.6 (O-only mini 16×3 = 48) | **One shared phase-2 pilot** (Part 2.3): one authorization, one hard cap, one packet, serving all four consumers. D2's O-only pilot becomes the mini arm extended to 16 dev tasks × 3 (C-09). C4's R-live runs inside P0. S0 runs first. Per-harness caps are proportional (D1). The gate gets a budget menu (Part 5.2), because no variant serving all consumers fits the hard cap at full planning values. | D2 K6 becomes the offline extension of the PILOT-packet ticket; there is no separate D2 spend gate (as the ledger recorded at 2026-11-06T23:22:29Z). The pilot gate decides Q-X28 and Q-X21 on the menu. |
| C-09 | Phase-3 O surface: D2 §2.1 defines O as mini 2.4.6 "at Harbor's pinned commit" with a frozen image; C4 offers its hosted original in an HH cell as "the O-equivalent H host" (C4 handoff to D2/E2), proven Harbor-equivalent by R-replay | **Design ruling: phase-3 O runs the frozen original mini-swe-agent layer under the phase-2 hosting path** (adapter C, HH cell, HH proxy, the same Harbor reward-file verifier channel as H). The Harbor-run O stays the runner-check reference (C4 R-replay/R-live). Reason: D2 REQ-XD-112's equality members (VM, proxy, verifier, account) then hold by construction, and the pilot's mini arm doubles as D2's natural corpus. **O2 must review.** | D2 REQ-XD-121/122 apply to the hosted original. C4's R-replay pass for mini becomes a precondition of D2's L1 gate. |
| C-10 | Oracle re-proof scope: D1 F4 re-proves all 87 eligible tasks before the split; B2 §1.6 re-proves "the stage-1 stratum"; B3 F12 quotes a 30-task window | **D1 governs:** exclusions precede the split, so no drawn manifest is perforated. B2's design (observe → freeze → prove; O, H-pub, H-allow; R = 3) applies to all 87. B3's quote is recomputed at 87 (Part 5.1). A banded fallback is offered at the provisioning gate: band A first, each band's seeded draw run after that band's re-proof, with per-cell dev counts fixed beforehand from declared fields only. | The REPROOF execution ticket covers 87 tasks or the gate-chosen band set. The split record (D1-d) cites the re-proof packet. |
| C-11 | n-arm protocol and quote vs landed `central_protocol`/`exposure_quote` (D1 F2); B3 F13/REQ-XB-221 routes compute quotes through `exposure_quote`; B2 F5 proposes `task-selection/1` | **One owner per concern (CC1).** `task-selection/1` (B2 SELECT, in hh-eval) owns the split, exclusion and selection records; D1's split rules and domain string are its parameters. `comparative_protocol/1` (D1-a) consumes it and owns arms, k, the schedule-randomization record and the declared-unsupported support members. `comparative-quote/1` (D1-a) is the **only** quote artifact for phases 2–4. It carries B3's compute lines and serves D2's G-L2 quote and D3's confirmation quote, built from `exposure_quote`'s primitives (components, pins, integer micros, refusal of unpriced lines). Round-5 constants stay byte-unchanged. One envelope ADR merges D1's REQ-XD-051 and B3's REQ-XB-221. | No second split, selection or quote routine. REQ-XB-221 and REQ-XD-051 merge under one primary. |
| C-12 | Pricing unit: C1 REQ-XC-008 (µ$ per 1M tokens) vs C2 REQ-XC-116 (`rate_unit ∈ {micro_per_token, micro_per_1k_tokens}`) | **C2 governs** (it refines C1). glm-5.3-flash prices to 150 / 30 / 500 µ$ per 1k tokens, all integral, consistent with the router's `DeclaredPricing` per-1k unit (C2 F11). | PRICE is the first proxy ticket. **Consumers:** proxy reservation and charge (XC-113, XC-114), slot cost fold (XC-118), phase spend ledger (XB-217), per-harness caps and quote (XD-040, XD-050), the admitted-spend evaluator (XC-117/XB-218), site spend pages in integer micros (XF-120), D3 quotes (XD-218). Each consumer's ticket cites ADR-C2-B. |
| C-13 | Harbor pin (Q-X41): B2 REQ-XB-011/123 and D1 REQ-XD-081 pin v0.25.0; C4 F3 finds `skip_install` in no tag; E1 F13 compared only v0.25.0…ff2a1ef4 | **One program-wide Harbor pin**, held in one tool-admission record inside ADR-B2-RUNNER (absorbing ADR-C4-C), with the host venv built from that pin's `uv.lock` (C4 F11). Which pin is Q-X41, at ratification. Keyed to the pin: O-surface image identity (XC-306); Terminus-2's code and host LiteLLM (XE-002/003, XC-315); the ATIF converter (XC-318); the cause map (XB-012, XB-123); the oracle O surface (XB-125); the HARBOR-O fixture job tree; the turn-1 census. | The pin must be fixed before HARBOR-O and ADMIT-CORE. Choosing a tag later than v0.25.0 adds an AC: re-run E1 F13's agent-relevant diff from v0.25.0 to the chosen pin. Option (c) (v0.25.0 without `skip_install`) forces "not image-identical" on every runner check and voids R-replay's image-equality leg. A pin change is a dated ADR amendment and creates new participant versions. |
| C-14 | `evaluate_costs`: C2 REQ-XC-117 adds an admitted-spend leg and keeps round-5 evaluators byte-stable; B3 REQ-XB-218 speaks of an "amended `evaluate_costs`" | **One new evaluator, `evaluate_admitted_costs`**, with a model leg (C2) and a compute leg (B3). The four round-5 `$0` evaluators and their recomputes stay byte-unchanged. Applied to round-5 packets (record or site), the new evaluator reports compute `unrecorded{pre_schema}`, never $0. | Owner: the RECONCILE ticket, sequenced after COST-RECORD. It is a section of the new supplement, not an amendment of the frozen round-5 supplement. |
| C-15 | Hard-cap reserve: D1 REQ-XD-040 caps the pilot at authorized × 0.8; C2 F14/REQ-XC-112 withholds a settlement margin (10 %, Q-X36) | **One root cap = authorized × (1 − m).** Proposed default m = 0.20 for the pilot, because nothing has been reconciled and C2 F14 names meter under-count risks. Proposed default m = 0.10 for later phases, once a `settled` reconciliation exists. Per-harness caps are slices under the root. | The pilot gate records m verbatim. Q-X36 is ratified as this rule, with values confirmed at each spend gate. |
| C-16 | ATIF: B1 REQ-XB-013 fails the publication check without `trajectory.json`; C4 REQ-XC-318 makes ATIF absence a reported attribute, never a failure | **C4 governs.** Proxy receipts are the transcript of record (C3 F10, C4 F12). The publication check instead requires the receipt-index transcript (REQ-XF-124) for every scored trial. | REQ-XB-013 is superseded by REQ-XC-317/318 plus REQ-XF-124. |
| C-17 | Reattempts: D1 REQ-XD-060 allows infra re-runs (≤ 2 per plan, ≤ 10 % of plans) via the engine's `ReattemptPolicy` (B2 F16); D2 REQ-XD-114 makes infra-affected L2 pairs uncertain and never reattempted (`parity_admission` needs attempt = 1) | **Reattempt policy is declared per protocol.** Phase-2 and phase-4 protocols declare ≤ 2/≤ 10 %. The L2 protocol declares zero replans. | RUNNER-CORE's AC adds a zero-replan protocol fixture. |
| C-18 | Scheduling: D1 F8 seeded k-round interleaving vs D2 REQ-XD-111 simultaneous H/O launch within a pair, in blocks | **The runner supports both declared schedule kinds.** Packing may change start times but never assignment (B2 F17), and must honour a pair-coupling constraint. | RUNNER-CORE AC: a coupled-pair fixture launches both members in the same packing step. |
| C-19 | Account sharing: D2 §2.1 requires an exclusive Ollama account during L2; D3 F14 forbids P4 concurrent with L2; C2 F17 keeps one semaphore; C1 open question 3 needs exclusivity for settlement | **No two paid collections overlap on the account**, and the operator's own Ollama use counts as overlap. Each paid window's gate records an exclusivity acknowledgement. | A HUMAN acknowledgement per window (Part 3.2). |
| C-20 | Terminus-2 model route: B2 OQ-6 suggests C2's loopback listener; C4 F10/REQ-XC-314 a host relay to the slot socket; E2 F15 the loopback lane for HH-native Terminus | **Both, by participant kind.** Phase-2 hosted Terminus-2 (Harbor's `Terminus2`, `lab_host`) uses C4's host relay to its slot socket. HH-native lanes (phases 3–4, Terminus stage two) use C2 F17's loopback listener (REQ-XE-121). Both pass through one proxy semaphore. | No proxy change is needed for either. |
| C-21 | C2 F6 names "Harbor direct" to Ollama as a validation baseline; C4 F14 routes O through the proxy | **C4 governs.** No Harbor-direct path to Ollama exists: the key stays in the proxy, budgets bind, receipts stay symmetric (REQ-XC-326). | C2's proposed validation is recast as R-replay/R-live. |
| C-22 | Allowlist provenance: B2 F12/REQ-XB-116 makes oracle-observed hosts grantable; the operator answered Q-X39a "Grant registry hosts only" | **The operator's answer governs.** Grants are standard registries plus hosts the instruction or task files name. Other oracle-observed hosts are withheld and logged. | REQ-XB-116 is amended to Q-X39a. D1 REQ-XD-014's per-task extensions cover instruction-named hosts only; keeping or excluding those five tasks stays Q-X40. |
| C-23 | Task draw: B1 REQ-XB-017 (proportional 2/18/10 ≈ 30 tasks) vs D1 F4 band census (Q-X31) | **D1 governs.** | REQ-XB-017 is superseded by REQ-XD-011/013; difficulty stratification survives inside the split. |
| C-24 | Pilot caps: C3 REQ-XC-211 flat $6 per harness vs D1 F9 proportional caps | **D1 governs.** C3's staging (P0 before P1) survives. | REQ-XC-211 merges into REQ-XD-040/041. |
| C-25 | Two edits to `check-position.sh`: A2 P1-E (`record` leg, `RECORD_BANNED`) and F2-f (shared `claims-lexicon.json`) | **Sequence F2-f after P1-E.** The lexicon refactor keeps P1-E's legs byte-identical in behaviour. | `test-check-position.sh` stays unchanged-green as F2-f's AC (F2 handoff). |
| C-26 | Round-5 compute figure: brief §2/A1 F11 "≈$1.41"; B3 F3 rate-card ≈ $1.72 over 16:32Z–22:15:40Z; B3 F4: R5.59's post-state list was a failed request | **Dated corrections, append-only, inside P1-C:** the compute figure (REQ-XB-222), the R5.59 post-state evidence note, and the coordinator's `lifecycleState: DELETE_REQUESTED` read (ledger 2026-11-06T23:30:17Z). | The phase-1 record states a rate-card estimate with billing not read (REQ-XA-124 plus REQ-XB-222). |
| C-27 | Brief §2's "the same parity machinery that just verified 30/30" vs A1 F2/F3, A2 F1, D2 F1 | **The audited account governs.** The spec never reuses the brief's phrasing. | REQ-XA-005/119/141 phrase pins; F2's `record_banned` list. |
| C-28 | Label collisions: A1 F14 (S3/S4 vs spec scenarios) plus the program's own reused tokens (S0, S1–S7, L1–L9, G1–G9, T1–T13, P0/P1, K-, H) | **A namespace table** (Part 10, design proposal) for O1. | Spec, tickets and packets use the namespaced forms. |
| C-29 | E1 A.3 "the system template ends with a newline" vs E2 F19 "not determinable" | **The turn-1 census decides.** Hand-derived goldens are labelled and replaced before G-L1 (REQ-XE-114). | None beyond REQ-XE-114. |
| C-30 | `tool_choice`: D2 F8 neutralizes it at the proxy; E2 G4b also removes it downstream | **Complementary (E2).** | None. |
| C-31 | Compaction candidates: E2 §6.1 vs D3's two composite levels | **D3's refinement is adopted;** `evict_oldest` stays the kernel fallback. | None. |
| C-32 | Codex and the Responses surface: the brief's proxy shape and C3 Q-C3-2 | **Recorded default: Codex is deferred from the pilot;** revisit at the tranche-1 gate. C2 REQ-XC-105 builds Responses only if a Responses-only harness is admitted. | No Responses ticket in Round 6. |
| C-33 | Results-packet schema owner: F2 REQ-XF-123 assigns it to D1-g, which runs only after the tranche gate, while F2-c needs it | **Define the `hh-comparative-results/1` schema in D1-b** (Round 6, offline); D1-g (Round 7) emits packets against it. | F2-c can land in Round 6 against schema-valid fixtures. |
| C-34 | B3 F11/F12 sized the pilot window on C3's 184 trials | **Recomputed for the shared pilot** (Part 5.1). | The PROV-W2 quote uses the shared-pilot trial count. |

---

## Part 2 — Phase and block DAG

### 2.1 Blocks

| block | content (owning notes) | spend | funded? |
|---|---|---|---|
| **B0 Bootstrap** | Round-6 plan-review gate; extend `hh-supplement-guard/1` to the experiments supplement (F5) | $0 | n/a |
| **P1 Methodology record** | A2 P1-A…P1-E (provenance, hardening run, corrections and edition ADR, study recompute, record, cards and checker) | $0, no VM | n/a |
| **P5a Site shell** | F2-a (design system + shell) → mock-review gate → F2-b (generator core, phase-1 data) → F2-e (phase-1 pages) → F2-f (claims checker, lexicon) → F2-g (Pages workflow, publication record) | $0 | n/a |
| **P2-infra** | B2 runner (7), C2 proxy (6–7), B3 recipe and cost (2–3), C4 hosting (6–8), D1 analysis and scan (D1-a, D1-b, D1-c), receipt archive (1), D2 L1 tooling (K1–K4) | $0 (offline, fixture) | n/a |
| **P2-S0** | Optional early cents-scale dialect smoke through the built proxy, laptop-local (Q-X23) | ≤ $0.05 | needs its gate |
| **P2-oracle + hosting smokes (window W1)** | B2 REPROOF over 87 tasks; C4 smoke battery and R-replay; D2 synthetic-branch O-capture | $0 model; compute | needs provisioning gate |
| **P2-pilot (window W2)** | D1-d selection and pre-registration I; one shared pilot (S0, P0 + R-live, P1 with the D2 mini extension); turn-1 census capture; PILOT-packet (D1-e outputs + D2 K6 + C4 R-live); archive upload | ≤ $60 authorized; hard cap per C-15 | **only funded block** (Q-X11) |
| **P3a HIR gap closure** | E2-T0…T10; D2 K7 (L2 wiring) | $0 | n/a |
| **P3-L1 (window W4)** | D2 K5 L1 gate run; G-L1 | $0 model; compute | needs provisioning gate |
| **Quotes** | D1-f tranche-1 quote; L2 quote (G-L2 inputs) | $0 | n/a |
| **P2-tranche (Round 7)** | tranche-1 gate; T1 window; T1 results; flagged-pass review; publication | quoted at gate | unfunded |
| **P3-L2 (Round 7)** | G-L2 gate (Q-X34); L2 window; K9 audit + L3; publication | quoted at gate | unfunded |
| **P4 sweeps (Round 7)** | D3-a…D3-i; G-P4-X; G-P4-C; `edit_file` capability if funded (Q-X44) | quoted at gates | unfunded |
| **P5b Site, later phases** | F2-c (comparative components), F2-d (grid, trial pages, viewer) in Round 6 on fixtures; per-phase publication gates as results land | $0 | n/a |
| **P3b Terminus-2 (Round 8)** | E2-T12…T16; D2 K10–K14; three ADRs; its own pilot, G-L1 and G-L2 (Q-X42) | quoted at gates | unfunded |

### 2.2 Dependency DAG (edges are "must land before")

```text
B0 ──► P1 (P1-A → P1-B → P1-C → P1-D → P1-E) ──────────────► F2-e, F2-f ──► F2-g ──► HUMAN Pages ──► GATE pub-P1
B0 ──► F2-a ──► GATE mock ──► F2-b ──┘
B0 ──► P2-infra:
        PRICE ∥ LANE ∥ RUNNER-CORE ∥ CELL-SPEC ∥ DEADLINE ∥ SELECT ∥ RECIPE
        LANE, PRICE ──► PROXY-CORE(a→b) ──► BUDGET ──► {EGRESS(+C2-5), REPLAY, COST-RECORD ──► RECONCILE}
        RUNNER-CORE, CELL-SPEC ──► VERDICT ──► EGRESS ──► HARBOR-O
        SELECT ──► D1-a ──► D1-b ──► GATE method-admission ──► (D1-f later)
        D1-b ──► F2-c ;  ARCHIVE ──► F2-d ;  D1-c (answer-scan) ∥
        {RUNNER-CORE, CELL-SPEC, EGRESS, PROXY-CORE, REPLAY} ──► ADMIT-CORE ──► {ADAPT-PY ∥ ADAPT-NODE ∥ ADAPT-MSG ∥ TERMINUS-HOST}
             ──► CAPTURE ──► SMOKE-BATTERY ──► RUNNER-CHECK
        REPLAY ──► K1 ──► K2 ──► K3 ──► K4
        RECONCILE ──► [GATE S0] ──► S0-SMOKE (optional, Q-X23)
P2-infra ──► HUMAN billing-alert ──► GATE PROV-W1 ──► W1{PROV, REPROOF, HOST-SMOKE, O-CAPTURE-synth, TEARDOWN} ──► HUMAN reconcile-W1
W1 ──► D1-d ──► HUMAN Ollama-settings, HUMAN HF-dataset ──► GATE pilot ──► W2{PROV, CENSUS, PILOT, TEARDOWN} ──► HUMAN reconcile-W2
W2 ──► PILOT-PACKET ──► {HUMAN pilot flag review, ARCHIVE-UPLOAD, D1-f, L2-QUOTE, E2-T9}
B0 ──► E2-T0 ──► (T1 ∥ T3) ──► T2 ──► T4 ──► T5 ──► T6 ──► (T7 ∥ T8) ──► T9 ──► T10 ;  T6 needs CELL-SPEC; T8 needs RUNNER-CORE, PROXY-CORE, REPLAY
{T9, K4, PILOT-PACKET(partitions), W2(census)} ──► GATE PROV-W4 ──► W4{PROV, L1-GATE-RUN, TEARDOWN} ──► GATE G-L1
{G-L1, L2-QUOTE, K7} ──► Round 7: GATE G-L2 ──► L2 ;  {D1-f, GATE method-admission} ──► Round 7: GATE tranche-1 ──► T1
{L2 verdict recorded (recommended), E2-T10, K4, D1-a} ──► Round 7: D3-a…c ──► GATE G-P4-X ──► X0/X1 ──► D3-f ──► D3-g ──► GATE G-P4-C ──► D3-h ──► D3-i
{G-L1 for mini, ideally L2 verdict} ──► Round 8: Terminus-2 block
```

**Truly parallel in the DAG** (any order in the linear chain): P1, P5a and P2-infra; P3a and P2-infra after their named prerequisites; the four C4 adapter tickets; E2-T1 with T3, and T7 with T8. **Strictly sequential:** the window blocks (gate → PROV → runs → TEARDOWN → reconcile); W1 before D1-d, because the split follows the re-proof; the pilot before E2-T9's final goldens, the L1 gate and every quote; G-L1 before any L2 or P4 spend; the L2 verdict before G-P4-C (D3 F14); and P4 never concurrent with L2 (C-19).

### 2.3 The single shared phase-2 pilot (design proposal)

| stage | surface | design | consumers | notes |
|---|---|---|---|---|
| CENSUS | O-capture, $0 | turn-1 synthetic sequence for every pilot ∪ T1-manifest task (D2 §1.4) | D2 K3/K5, E2-T9 | runs first in W2, before any paid call |
| S0 | proxy | one call per dialect plus the seed, `tool_choice` and `cache_control` probes (C1 open question 5; C2 ticket 8) | C2 REQ-XC-119 expected served-model set; D1 | ≤ $0.05; skipped if the early S0 gate already ran it |
| P0 | H (hosted, in HH cells) | 3 smoke tasks (E/M/H of P) × 1 × 8 harnesses = 24 | D1 G3/G5/G6/G8, egress probe | — |
| R-live | O (Harbor via the proxy) | 1 task × 1 × the G9-eligible harnesses ≤ 8 (C4 OQ-2, small option) | C4 REQ-XC-325 | descriptive, `parity: none` |
| P1 | H | P = 10 band-A dev tasks × 2 attempts × 7 harnesses (all but mini) = 140 | D1 quote nuisance inputs | only harnesses passing P0 |
| P1-mini | H, which is phase 3's O (C-09) | 16 dev tasks (all 11 band-A dev ∪ all 5 band-B dev, so no seed needed) × 3 repeats = 48 | D1 (mini's quote inputs), D2 (f, $/trial, RECORD corpus ≥ 12 dev + 8 gate-A + 8 gate-B, REQ-XD-107) | replaces mini's 10 × 2; the last-priority stage |
| re-runs | H | infra re-runs ≤ 20 % of P1 within the frozen bound (D1 F9) | all | — |

- **Stop rules:** D1 F9's budget, infra, integrity and calendar rules apply unchanged. There is no outcome-based stopping.
- **Priority order under the hard cap:** S0 → P0 + R-live → P1 round 1 (all harnesses, replicate 1) → P1 round 2 → P1-mini replicate 3 and tasks 11–16 → re-runs. If the cap binds, the remaining slots are `withheld{budget}`, and the truncation lands first on D2's corpus extension, which degrades f precision gracefully (D2 §2.6 table) without touching D1's quote inputs.
- **Firewall:** pilot outcomes never select harnesses, the reference, tasks or effect sizes (D1 F10). D2's partitions are committed before H uses the dev partition (REQ-XD-106).

### 2.4 Linearization (the chain is a stacked linear chain)

**Linearization A (recommended unless the operator races the credit reset):** B0 → P1 → P5a through F2-g, then HUMAN Pages and the phase-1 publication gate → P2-infra (PRICE, LANE, RUNNER-CORE, CELL-SPEC, DEADLINE, SELECT, RECIPE, PROXY-CORE a/b, BUDGET, VERDICT, EGRESS, REPLAY, COST-RECORD, RECONCILE, [S0 gate + smoke], HARBOR-O, D1-a, D1-b, method-admission gate, D1-c, ARCHIVE, ADMIT-CORE, the adapters, TERMINUS-HOST, CAPTURE, SMOKE-BATTERY, RUNNER-CHECK, K1–K4, F2-c, F2-d) → E2-T0…T8 → W1 → D1-d → pilot HUMANs and gate → W2 → PILOT-PACKET → pilot review and archive upload → E2-T9, T10, K7 → W4 → G-L1 → D1-f, L2-QUOTE → tail.

Phase 1 lands first because it is the cheapest deliverable and unblocks the site's phase-1 content and the README link (OL-01).

**Linearization B (if the operator wants the pilot before 2026-11-22T16:30:34Z):** B0 → the P2-infra critical path (as above, minus F2-c/F2-d and K1–K4) → W1 → D1-d → pilot → then P1, P5a, P3a, W4, quotes and tail. Its critical path to the pilot is about 35–38 implement-spec runs plus four gate pauses (bootstrap, method admission, W1 provisioning, pilot spend) (Part 8). Whether that fits before the reset depends on throughput nobody has measured for this kind of ticket. The choice is a ratification-time planning decision (Q-H1-2); it authorizes no spend.

---

## Part 3 — Gates and HUMAN rows

Numbering continues from GATE-G19 and HUMAN-H11; the decomposer allocates the ids. Each gate's marker file quotes its thresholds verbatim from the ratified spec before the gated work begins (BM-TICKET-05). None is pre-answered here.

### 3.1 Gates

| gate (provisional name) | round | presented after | pre-registered decision content (never pre-answered) |
|---|---|---|---|
| R6 plan review + bootstrap | 6 | decomposition | partition, coverage register and Phase-4 review record; authorizes only the guard extension (Round-5 GATE-G8 pattern) |
| Site identity mock review (Q-X43, Q-F2-1) | 6 | F2-a | operator views the rendered mock in both themes at F2 F13's viewports; `test_tokens.py` record attached; approve "Instrument" or redirect to I-A/I-C |
| Phase-1 publication (Q-X16) | 6 | F2-g + HUMAN Pages | publish phase-1 pages: `hh-site-publication/1` pins the exact input set; labels "internal record; not outside-verified"; the `[results-site]` card transition (Q-F2-6) |
| Method admission (REQ-XD-023) | 6 | D1-b | independent numerical/routing review record; known-answer tests; comparative battery records (REQ-XD-024), with stopped scenarios never counted. Admit or refuse `comparative_protocol/1`, `comparative-quote/1`, the descriptive estimators and `slot_schedule_randomization` |
| Early S0 smoke (Q-X23, optional) | 6 | RECONCILE | verbatim spend authorization ≤ $0.05; laptop-local proxy; S0 probe list; exclusivity acknowledgement |
| Provisioning W1 | 6 | P2-infra + HUMAN billing alert | B3 quote (REQ-XB-213: planning, bound, ceiling at D); dead-man action and D (Q-X38); network posture (IAP ephemeral IPv4 default); re-proof scope (87 or banded, C-10); Q-X39b live-verify leg; allowlist provenance per Q-X39a |
| Pilot spend (+ provisioning W2) | 6 | D1-d + Ollama/HF HUMAN rows | Q-X21 (overage behaviour, exclusivity, whether the post-reset allowance is authorized); Q-X28 breadth from the Part 5.2 menu; verbatim model authorization; m (C-15); per-harness caps; W2 compute quote and D; archive-upload authorization to the private HF dataset (Q-X35); credit period end recorded (REQ-XH-007) |
| Provisioning W4 (L1) | 6 | E2-T10, K4, PILOT-PACKET | compute quote at $0 model; the gate partition digests committed; the frozen H record named |
| G-L1 | 6 | L1 gate run | `hh-l1-conformance/1`: first divergence fails; coverage M1–M19 complete; census complete; `hh-l1-canon/1` rule table frozen; stable-prefix histogram. Pass or fail only |
| GATE-ACCEPT (tail) | 6 | CAP.3 | Round-6 closure sums; accepted-deviation set |
| Tranche-1 authorization | 7 | D1-f + method admission | quote menu (REQ-XD-050); declared harness-subset rule; pre-registration hash; effort and version pins; precision-target verdict; model and compute authorization; Q-D1-4 unresolved-cell policy; Q-D1-7 contender set; Codex revisit (C-32) |
| G-L2 (Q-X34) | 7 | G-L1 + L2-QUOTE | ε (±.15 / ±.20 / other), N, T, R, block size from D2 Table B and the pilot's f bound; u-rate assumption; spend and compute; exclusivity window |
| G-P4-X | 7 | D3-a…c + exploration pre-registration | X0 + X1 spend; registry hash; W; pre-registration hash predating every X0 subject (D3 §5.2) |
| G-P4-C | 7 | D3-f, D3-g | confirmation spend; selected members (incl. combination or not); pre-registration hash |
| Publication per phase (T1, L2, P4) | 7 | each results packet + flagged review | `hh-site-publication/1`; dataset visibility flip as a separate HUMAN act after the secret and contamination scans |
| Phase-1b tranche (only if Q-X24 chooses it) | 6 or 7 | B2 runner | R ≥ 5 both surfaces, ε pre-registered, compute quote (REQ-XA-126) |
| Terminus-2 block (Q-X42) | 8 | mini G-L1 (+ L2 verdict) | scope, ADRs, tokenizer route, its pilot quote |

### 3.2 HUMAN rows

| HUMAN row (provisional name) | owner | trigger / schedule | exit | withheld until done |
|---|---|---|---|---|
| GCP billing alert (B3) | operator | before the W1 provisioning gate | budget amount and 50/90/100 % thresholds recorded; recipients `provided: yes` | every provisioning window |
| Ollama account settings + exclusivity (C1 open questions 1–3, Q-X21) | operator | before the first paid gate (early S0 or pilot) | automatic usage billing confirmed off (or recorded as on, with its consequence); exclusivity commitment for each window | paid stages |
| Dedicated Ollama key (design proposal; optional) | operator | before W2 | a key used only by the program, revocable after each phase, `provided: yes`. Whether the plan allows several keys is UNVERIFIED. It does not help settlement, which has no per-key breakdown (C1 F10) | none (risk-reduction only) |
| HF dataset repo + write token (Q-X35) | operator | before the pilot gate | private dataset repo exists; token held off-VM, `provided: yes` | archive upload; F2-d live viewer |
| GitHub Pages enablement (F2 REQ-XF-130a) | operator | after F2-g | Settings → Pages → Source: GitHub Actions; readout `provided: yes` | phase-1 publication |
| Reconcile window Wn (B3 REQ-XB-216) | operator | ≥ 24 h after each TEARDOWN | billing read recorded: within / outside / unavailable | round closure totals |
| Pilot flagged-pass review (D1 REQ-XD-071) | operator-named reviewer (Q-D1-6) | PILOT-PACKET (exit trivially satisfied, and recorded, when there are zero flags) | dispositions with excerpt hashes | pilot exploratory summaries |
| Flagged-pass review per results freeze (T1, L2 veto, X1, confirmation) | reviewer | each results freeze (Round 7) | as above | each results packet |
| Dataset visibility flip per publication (REQ-XF-130c) | operator | after each publication gate decision | flip done, revision pinned in the publication record | the transcript viewer |
| Key rotation after a phase (B3; optional) | operator | after a VM window that held the key | recorded `provided: yes/no` | none |
| Custom-domain DNS (only if Q-F2-5 chooses one) | operator | before F2-g | DNS record live | custom URL |

Account and terms-of-service acts: Claude Code's terms were dispositioned by Q-X26. No note studied Ollama's or Hugging Face's terms for benchmarking or for publishing TB-derived transcripts. The publication gate's readout should record that the operator read them (an open question, Part 7).

---

## Part 4 — ADR and amendment register (deduplicated)

Kinds: **ADR** (new admitted record); **AMEND-C** (canonical-spec amendment via the manifest protocol); **SCHEMA** (hh-embed schema change, CC7); **REC** (dated record or correction, not an ADR); **SUPP** (a section of the new experiments supplement, written by O1, needing no ADR); **INTERP** (an interpretation of a frozen text, recorded in the decision register and sent to O2). ADR numbers are allocated at implementation, from ADR-0423.

| # | proposed title | kind | owning note(s) | first needed by | blocks implementation? |
|---|---|---|---|---|---|
| AR-01 | Retroactive: TB 2.0 → `terminal-bench-core==0.1.1` substitution, with `retro:` evidence and the catalogue-identity label correction | ADR | A1 F13, A2 F9/REQ-XA-116 (Q-X25) | P1-C | blocks P1-E publication |
| AR-02 | S4 protocol-deviation record (`hh-protocol-deviations/1`), "not an amendment" | REC | A1 F8, A2 REQ-XA-115 (Q-X25) | P1-C | blocks P1-E |
| AR-03 | Dated corrections: PARITY-TRANCHE-3, S4-CONFIRMATION, R5.59 ("null-equivalent"; post-state check), GATE-G19 (`live-executed`), DF-CAP5.1-1, round-5 compute ≈ $1.72 | REC | A1, A2 REQ-XA-114, B3 F3/F4/REQ-XB-222 | P1-C | no |
| AR-04 | Runner crate, placement, slot record, and the external-tool admission of Harbor (one pin + `uv.lock`) | ADR (B2-RUNNER, absorbing C4-C) | B2, C4 F3/F11 | RUNNER-CORE (pin fixed by HARBOR-O) | yes |
| AR-05 | `allowlist_mediated` egress profile: two sockets, `hh-cell-relay`, mediator tunnel leg, sealed phase layers (absorbs C2-C) | ADR (B2-EGRESS) | B2, C2 | EGRESS | yes |
| AR-06 | Helper cell spec: image WORKDIR, cpus/memory limits, kernel-mediated copy-in/out | ADR (B2-CELL) | B2 F8, C4 F12 | CELL-SPEC | yes |
| AR-07 | Task-declared agent and verifier deadlines above the 900 s cap; legacy path byte-identical | ADR (B2-DEADLINE) | B1 F10, B2 F13 (Q-X32 answered) | DEADLINE | yes |
| AR-08 | Model-mediation proxy: hosting-tier placement, phase proxy run, release modes, `hh.model_io/1` `proxy` member, two measurement classes in `classes.rs`, the §6.6 Adapter-B reading | ADR (C2-A) | C2 | PROXY-CORE | yes. O1/O2 decide whether the §6.6 reading needs AMEND-C as well |
| AR-09 | `PricingTable.rate_unit` (`micro_per_1k_tokens`), one rounding rule, `NoPrice{unrepresentable_rate}` | ADR (C2-B) | C1 F14, C2 F11 | PRICE | yes (every paid consumer, C-12) |
| AR-10 | `GenParamPolicy` path-qualified `pin` / `params_scope` | ADR (C2-D) | C2 F7 | PROXY-CORE | yes |
| AR-11 | Ollama upstream lane dialects, auth form from `auth_kinds`, raw forward leg | ADR (C2-E) | C1 F15, C2 F3/F6 | LANE | yes |
| AR-12 | Protocol support gate for stochastic hosted participants: declared-unsupported route/cache/seed/fingerprint members, weaker claim class | ADR | C1 open question 6, D1 REQ-XD-020, D2 §2.1 | D1-a | yes (D1-a; any L2 manifest) |
| AR-13 | Experiments envelope and quote ceilings: per-subject envelopes from task-declared timeouts; TB 2.1 stage ceilings; round-5 §9.2 constants unchanged | ADR (merges D1 REQ-XD-051 and B3 REQ-XB-221) | D1 F2, B3 F13 | D1-a | yes (every quote) |
| AR-14 | Phase spend ledger (ADR-0363 row shape + kind/authority/basis/phase/window) and `hh-live-pricing/2` compute rows | ADR | B3 F15/REQ-XB-217, C2 | COST-RECORD | yes (paid phases) |
| AR-15 | Compute recipe and `gcloud` as an admitted operator tool; no Terraform | ADR | B3 REQ-XB-201 | RECIPE | yes (W1) |
| AR-16 | Admitted-spend evaluator `evaluate_admitted_costs` (model + compute legs) and the `hh-slot-costs/1` record | SUPP | C2 F12, B3 F15 (C-14) | RECONCILE | yes (paid study reports) |
| AR-17 | Method admission of the new estimators: single-arm FM rate, rerun/swap flips, $/solve, tied-with-leader, `slot_schedule_randomization` exchangeability; one Holm family per tranche as an explicit departure from §4.3's cross-strata family (Q-D1-3) | ADR via the §4.3 route + gate | D1 F7/F8/REQ-XD-022/023 | D1-b | yes (quote; tranche analysis) |
| AR-18 | Harness admission record, two-stage bundle/layer build, S2 verify-only setup, Harbor-derived recipes with attribution | ADR (C4-A) | C3, C4 | ADMIT-CORE | yes |
| AR-19 | Terminus-2 `lab_host` hosting: exec bridge, HH Python driver under the pinned Harbor venv, `LabHost` in adapter C | ADR (C4-B) | C4 F10 | TERMINUS-HOST | yes (Terminus-2 in phase 2 only) |
| AR-20 | Receipt archive: `hh-receipt-index/1`, condenser, private-then-public HF dataset, upload path as an admitted operator tool, token handling | ADR (**no owning note designed the upload; H1 identifies the gap**) | Q-X35, F2 REQ-XF-124 | ARCHIVE | yes (archive upload, F2-d) |
| AR-21 | Model port renders the compiled `ModelSurface`; durable rendering refs (G1/G3/G6/G18–G21) | ADR (E2-A) | E2 | E2-T0 → T1/T2 | yes (P3a) |
| AR-22 | Exec over `hh-embed/1`; helper `stdio = merged`, `exec_defaults`; HH-native participant topology, with the C-05 ladder | ADR (E2-B) | E2, B2 | E2-T0 → T6/T8 | yes |
| AR-23 | `hh-tmpl/1`, `json_head_tail@1`/`text_head_tail@1`, `error_format` classes | ADR (E2-C) | E2 | E2-T0 → T3/T4 | yes |
| AR-24 | §5e.1 `stop_rule.submit{marker, line, require_returncode}` | AMEND-C | E2 F6/G10 | E2-T0 → T5 | yes |
| AR-25 | `open_session.exec` member, regenerated clients | SCHEMA | E2 G22 | E2-T6 | yes; `check-drift.sh` |
| AR-26 | Effort as a sweep factor: a declared lane parameter (no ADR) or a new Lab factor kind (ADR) | ADR (conditional) | D3 F2, Q-D3-7 | D3-a | only if the factor-kind route is chosen |
| AR-27 | Phase-4 compaction under a declared window cap W; the supplement's l.234 padding ban read as round-5-scoped | INTERP | D3 F3/F4 | D3-a | no; O2 review |
| AR-28 | Pages deployment: `pages.yml`, three SHA-pinned actions, permissions | ADR | F2 F10, Q-F2-4 | F2-g | yes (publication) |
| AR-29 | Vendored OFL fonts and a notices-generator extension | ADR (optional) | F1 Q-F1-1, F2 F3 | not v1 | no |
| AR-30 | Terminus-2 stage two: `terminal_session@1` executor; `shim` interaction-mode admission ahead of tier; `hh/handoff-summarize@1` plus tokenizer route (a vocabulary-data port is a dependency question); optional G15 parameter-route amendment | ADR ×3 (+ AMEND-C fallback) | E2 Part 5 | Round 8 (the `shim` ADR may be written in Round 6 at docs-only cost, E2 §5.2) | Round 8 only |
| AR-31 | Experiments supplement registered in the guard: second frozen source, `REQ-X[A-H]-[0-9]+`, coverage register, Round-6 routing | decomposition decision (no ADR) | H1 F5 | guard extension | yes (every dispatch) |
| AR-32 | Evidence-card register: `[replay-record]` (A2), `[results-site]` (F2 Q-F2-6), per-phase result cards (design proposal: `[harness-comparison]`, `[hir-parity]`, `[config-sweep]`), each capped at "Available — internal record; not outside-verified" | SUPP + `check-position.sh` change | A2 F11/F12, F2 Q-F2-6, H1 | P1-E, then each publication | no ADR; claims-language pins |

Totals: **22 ADRs** through Round 7. That count includes AR-01 and the conditional AR-26, and excludes the optional AR-29 and AR-30. AR-30 adds three more in Round 8 (25 in all, plus its amendment fallback). Of the 22, **21 are needed in Round 6**: AR-01, AR-04 to AR-15, AR-17 to AR-20, AR-21 to AR-23 and AR-28, with AR-19 only if Terminus-2 stays in the phase-2 harness set. AR-26 is Round 7. The rest of the register is one canonical amendment (AR-24), one schema change (AR-25), two records (AR-02, AR-03), two supplement sections (AR-16, AR-32), one interpretation (AR-27) and one decomposition decision (AR-31). Brief and ledger items covered: the never-recorded substitution (AR-01), `evaluate_costs` (AR-16, C-14) and claims graduation (AR-32).

---

## Part 5 — Budget envelope

Conventions: model "planning" values follow D1. That means C3's no-cache priors × 1.5 tail, with a further × 1.2 for infra where the source table includes it. The tranche and P4 "ceilings" use D1's $0.34 × 1.2 × 1.5 = $0.612 per trial. Compute values use B3's template with r = $0.31414/h (e2-standard-8 + 300 GB pd-balanced + IPv4) and C = 4. Figures marked † are my arithmetic over those inputs, not quotes from any note. All are UNVERIFIED priors until pilot and window receipts exist.

### 5.1 Per block

| block | trials / runs | model $ (expected · planning or ceiling) | compute $ (planning · ceiling at D) | wall | funding |
|---|---|---|---|---|---|
| P1 | records-in | $0 | $0 (no VM) | — | n/a |
| P1b (optional) | ~300 oracle runs | $0 | not quoted (≈ B3 2a′ scale) | ~1 d | gate |
| P2-S0 | ~5 calls | ≤ $0.05 | $0 (laptop) | minutes | gate |
| W1 re-proof + hosting smokes | 87 observe + 783 prove + ~40 smoke + 16 R-replay + synthetic capture ≈ 950† | $0 | ≈ $13.4 planning; ≈ $22.6 at D = 72 h†; timeout bound ≈ $65† | ≈ 1.8 d† | provisioning gate |
| W1 banded fallback (band A only) | ≈ 480 + smokes† | $0 | ≈ $7–8 planning† | ≈ 1 d† | provisioning gate |
| W2 shared pilot (V1) | 220–258 (Part 5.2) | ≈ $30.6–35.9 expected · ≈ $45.9–53.8 planning†; hard cap $48 at m = .20 | ≈ $6.8 planning; ≈ $12.6 at D = 40 h† (B3 2b scaled) | ≈ 0.9 d | **the only block with funding in sight**: $60 (Q-X11) |
| W4 L1 gate run | ~100–150 short runs | $0 | ≈ $2–5† (no note quoted it) | < 1 d | provisioning gate |
| Round 7 T1 (8 × 37 × 5) | 1,480 | $74 / $222 / $503 at $0.05/0.15/0.34 · ceiling $906 (D1 F5) | ≈ $23–35 planning† (700–1,040 s/trial) | 3.0–4.6 d | unfunded; tranche gate |
| Round 7 T1 (4 × 37 × 5) | 740 | $37 / $111 / $252 · ceiling $453 (D1 F5) | ≈ $12–18† | 1.5–2.3 d | unfunded |
| Round 7 L2, N = 150 | 300 (2 arms) | $15–120 before reserve (D2 F5) · ceiling ≈ $184† | ≈ $4.6–6.8† | ≈ 0.6–0.9 d | unfunded; G-L2 |
| Round 7 L2, N = 300 | 600 | $30–240 (D2 F5) · ceiling ≈ $367† | ≈ $9–14† | ≈ 1.2–1.8 d | unfunded |
| Round 7 P4 (X0 + X1 + confirm, m = 2) | 1,135 | $57 / $170 / $386 · ceiling ≈ $695 (D3 §4.2) | ≈ $17–26† over ≥ 2 windows | ≈ 2.3 d + gaps | unfunded; G-P4-X, G-P4-C |
| Round 7 T2/T3 | 680 / 520 at H = 8 | not quoted (longer bands) | not quoted | — | unfunded |
| Round 8 Terminus-2 | own pilot, L1, L2 | not quoted (order of P3) | not quoted | — | unfunded |
| Site, CI, Pages | — | $0 | $0 expected (Pages on a public repo; HF private storage limits UNVERIFIED) | — | n/a |

**Program totals for the recommended designs** (pilot + T1 at H = 8 + L2 at N = 300 + P4): model planning ceilings ≈ $48 + $906 + $367 + $695 ≈ **$2,016**†; expected at $0.15/trial ≈ $36 + $222 + $90 + $170 ≈ **$518**†. Compute planning ≈ **$100**† across about seven windows.

**Honest statement:** only the ≤ $60 pilot has funding in sight (Q-X11). With D1's tail-inclusive planning values the pilot is already capacity-bound. Every later tranche, L2 and phase-4 stage requires a fresh operator authorization at its gate, and an honest "declined" disposition at those gates is a valid program outcome (brief §5). Compute is quoted separately per window, and no window is authorized yet.

### 5.2 Pilot budget menu for the pilot gate (design proposal; arithmetic over D1's per-harness priors)

Per-harness no-cache priors, derived from D1 F9's caps ÷ 41.4: mini .155, Terminus-2 .181, Claude Code .087, Pi .051, Hermes .101, OpenHands .251, Kimi .128, OpenCode .140 (sum 1.094 $/task-attempt, matching D1's 1.093).

| variant | contents | trials | expected $ | planning $ (×1.5 tail, +20 % re-runs) | fits $48 cap at planning? |
|---|---|---|---|---|---|
| D1 baseline | S0 + P0 24 + P1 10×2×8 | 184–216 | ≈ 29.5 | ≈ 44.3 (D1) | yes, but D2's corpus minimum (≥ 28 O trajectories) is not met (mini gets 23) and there is no R-live |
| V1 (shared, recommended presentation) | S0 + P0 24 + R-live 8 + P1 7×10×2 + mini 16×3 | 220–258 | ≈ 30.6–35.9 | ≈ 45.9–53.8 | **no at full tail and re-runs; yes in expectation**; priority truncation (Part 2.3) |
| V1 with mini at 12 × 3 | as V1, mini 36 | 208–244 | ≈ 29–34 | ≈ 50.4 at full re-runs | no at full tail; D2 sd .092 |
| V1 minus one harness h | operator's choice at the gate | — | — | saves ≈ 42 × p_h (e.g. ≈ $10.5 for OpenHands)† | yes for p_h ≳ .14 |
| V1 + post-reset allowance | requires Q-X21 authorization | as V1 | as V1 | as V1 | cap rises with the authorization |

**Schedule risk:** the $60 is the plan's included allowance for 2026-10-22T16:30:34Z → 2026-11-22T16:30:34Z. It does not roll over (C1 F8). At 2026-11-07T00:32Z that leaves about 15.7 days for ratification, decomposition, the P2-infra build, W1 and the pilot. A provider reset is never new authorization (REQ-XC-004). Missing the reset is not a technical failure, but it moves the pilot into the next period only under a fresh authorization.

---

## Part 6 — Requirement inventory

### 6.1 Candidates emitted

| note | range | count |
|---|---|---|
| 24 A1 | REQ-XA-001…016 | 16 |
| 30 A2 | REQ-XA-101…127 | 27 |
| 25 B1 | REQ-XB-001…018 | 18 |
| 31 B2 | REQ-XB-101…127 | 27 |
| 32 B3 | REQ-XB-201…223 | 23 |
| 26 C1 | REQ-XC-001…015 | 15 |
| 33 C2 | REQ-XC-101…125 | 25 |
| 27 C3 | REQ-XC-201…214 | 14 |
| 34 C4 | REQ-XC-301…329 | 29 |
| 35 D1 | REQ-XD-001…092 (non-contiguous: 001–005, 010–014, 020–024, 030–031, 040–044, 050–052, 060–062, 070–071, 080–082, 090–092) | 36 |
| 36 D2 | REQ-XD-101…126 | 26 |
| 37 D3 | REQ-XD-201…225 | 25 |
| 28 E1 | REQ-XE-001…014 | 14 |
| 38 E2 | REQ-XE-101…132 | 32 |
| 29 F1 | REQ-XF-001…022 | 22 |
| 39 F2 | REQ-XF-101…142 | 42 |
| **subtotal (A–F)** | | **391** |
| 40 H1 (this note) | REQ-XH-001…033 | 33 |
| **total candidates** | | **424** |

Counts come from the `## Spec requirements emitted` sections (grep of `REQ-X[A-H]-[0-9]{3}` row heads, 2026-11-07T00:31Z).

### 6.2 Merge groups synthesis must resolve (primary ← merged/superseded)

| group | primary (proposed) | merged-with / superseded-by | basis |
|---|---|---|---|
| Phase-1 record | REQ-XA-101…127 | XA-001…016 refined (A2 handoff: "refined, not contradicted"): 002→111, 003→109/112, 004→107/108, 005→119, 007→115, 009→127, 010/011→101/103/106, 012→XB-215, 013→116/105, 014→121–123, 015→125, 016→108 | A2 |
| Benchmark identity and verdict | XB-109/110/117/118/122–125 | XB-001→122, 002→124, 003→117, 005→125 and XD-010, 006→106, 007→110 (refined classification), 009→118, 011→123, 012→120 and XD-060 and XC-323 (one mapping family), 018→123 | B2 handoff |
| Draw | XD-011/013 | XB-017 **superseded** | C-23 |
| ATIF | XC-317/318 + XF-124 | XB-013 **superseded** | C-16 |
| Proxy | XC-101…125 | C2 marks REFINES: 104→001, 106→012, 107→002, 108→013, 113→003, 115→011, 116→008, 119→009, 121→005, 123→011; 004/006/007/010/014/015 adopted unchanged | C2 handoff |
| Egress | XB-113 | XC-102 (model-channel component); XB-008 | C-02 |
| Admission record | XC-302 | XC-201 (refined by 302), XD-080 (fields), XB-014, XC-204→304/305 | C4, D1 |
| Pilot | XD-040/041 | XC-211 (caps superseded), XC-015→XD-043, XD-115 (L2 pilot quantities folded into the shared pilot packet) | C-08, C-24 |
| Effort pin | XC-107 (proxy) + XD-082 (phase-2 value) | XC-002, XC-213, XC-309, XD-204 (phase-4 variant) | C2, D1, D3 |
| HF labelling | XC-014 | XC-212 | C1, C3 |
| Answer scan | XD-070/071 | XB-016, XC-319 (inputs), XD-223 (phase-4 reuse) | D1 |
| Costs | XC-117 (evaluator) + XB-218 (compute predicates) → one primary | XC-118 ↔ XB-108 (slot cost record); XB-217 ↔ XC-112 (ledger vs root cap) | C-14, C-15 |
| Envelope | one primary (AR-13) | XD-051, XB-221 | C-11 |
| Selection and protocol | XB-122 (selection) + XD-020 (protocol) | XD-011 (split rules as parameters) | C-11 |
| Parity and replay interface | XC-122 | XD-108 (interface statement) | D2/C2 |
| Declared-delta and liveness | XE-125 | XD-125, XD-203 (executed-leg extension) | D2, D3, E2 |
| LiteLLM and O image | XD-121 | XE-003, XC-315 (Terminus host) | D2, C4, E1 |
| Harness references | XE-001/002 | XD-122 (configuration), XE-123 (document encodes Q-X29) | E1, D2, E2 |
| Canonicalization | XD-105 | XE-005 | D2, E1 |
| Window and compaction | XD-205 | XE-127, XE-013 (Terminus window), XD-124 | D3, E2, D2 |
| Sweep registry | XD-201 | XE-014 | D3, E1 |
| Claims and labels | XF-134 (lexicon) + XD-092 (labels) | XD-091, XA-120, XB-015, XF-116, XF-007→116 | F2, D1 |
| Site | XF-101…142 | XF-001…022, mapped one-to-one in F2's table (001→121, 002→118 …, 022→138); XF-009 kept with XF-132 as its declared exception (C-07) | F2 |
| Terminus staging | XE-132 | XD (D2 §5.3 sequencing), XH-001 | E2, D2 |

Expect about 60 aliases (merged or superseded) out of 424 candidates. That leaves about 360 adopted targets. The Round-5 precedent split 180 literal candidates into 167 adopted, 12 merged and 1 superseded (manifest `### Round 5 supplemental requirement ownership`).

### 6.3 Proposed requirement families

- **Pattern:** `req_id_pattern: REQ-X[A-H]-[0-9]+`. It is disjoint from Round 5's `REQ-[A-H]-[0-9]+`: the second character `X` cannot match `[A-H]` followed by `-`. Keep every candidate id verbatim and append-only (ledger execution contract); do not enable mixed-namespace `req_index_literal_from` (Round-5 precedent).
- **Families:** XA phase-1 record; XB benchmark, runner and compute; XC model mediation and harness hosting; XD methods (phase 2: 0xx, phase 3: 1xx, phase 4: 2xx); XE HIR native representation; XF results site; **XH program** (gating, sequencing, budget records, publication). XG stays reserved and unused.
- **Coverage register:** a literal owner/alias table plus a machine-readable companion, as Round 5 did (`2026-10-30_evidence-coverage.md`/`.json`). One primary owner, one numbered AC and one layer per adopted id; aliases keep their literal predicate bytes. The guard extension validates it (F5).

---

## Part 7 — Pending §Q consolidation

"Ratification" means Q-X20 at O3. "Gate" means the named build gate. "Recorded default" means the recommendation stands unless the operator overrides it, with no separate decision needed.

| id | recommended default (source) | decide at | why there |
|---|---|---|---|
| Q-X20 | ratify the reviewed spec | ratification | process |
| Q-X21 | operator answers overage behaviour and exclusivity via the HUMAN row; whether the post-reset allowance is authorized | **pilot gate** (spend). At ratification the operator states only whether the schedule should race the reset (Q-H1-2, planning) | spend is authorized only at a gate |
| Q-X22 | pin `high` for phases 2–3; `low`/`max` as phase-4 levels (C2, D1 F15, D2, D3) | ratification | enters every renderer and the proxy lane |
| Q-X23 | yes: an optional early S0 gate after RECONCILE (≤ $0.05, laptop-local); otherwise S0 runs first in W2 | the S0 gate | spend |
| Q-X24 | records-in only for phase 1; phase 1b unscheduled (A2 F8) | ratification | sets P1 scope |
| Q-X25 | deviation record, not an amendment; retroactive edition ADR before P1-E (A2 F9) | ratification | blocks P1-C |
| Q-X27 | latest stable at the pre-registration freeze with exact locks; mini 2.4.6 fixed; LiteLLM pinned (D1 F14) | ratification | admission records |
| Q-X28 | the gate chooses from the Part 5.2 menu (D1 recommends 8 × 10 × 2) | pilot gate | spend |
| Q-X29 | Harbor in-agent defaults plus declared external caps (D2 R4); LiteLLM resolved once and frozen (D2 R5) | ratification | the mini HIR document encodes it (REQ-XE-123) |
| Q-X33 | A2's recommendations: no useful-task state change in phase 1; operations reason text updated | ratification | P1-E card text |
| Q-X34 | present ±.15 / ±.20 with D2 Table B and the pilot's f bound; **no default** | G-L2 | the margin is the claim |
| Q-X36 | rule C-15: m = .20 for the pilot, .10 after the first settled reconciliation | ratification (rule); each spend gate (value) | both |
| Q-X38 | STOP dead-man; D from the authorized quote; tear down across gate waits; billing alert (e.g. $50/month at 50/90/100 %); console-read reconciliation (B3) | ratification (policy); first provisioning gate (amounts) | both |
| Q-X39b | register a `live-verify.sh` leg so the re-proof can earn `live-executed`; otherwise "recorded execution" | W1 provisioning gate | B2 OQ-7 |
| Q-X40 | reference mini-swe-agent; dev pool 20; one Holm family per tranche; frozen bound with residual-refuses-pairs; keep the 5 allowlist-extended tasks with instruction-named hosts plus scan; reviewer = operator; contender set off (D1) | ratification (design); tranche-1 gate (unresolved-cell policy, contender set) | split precedes the pilot |
| Q-X41 | the first stable tag containing `skip_install` if one exists at the pre-registration freeze, else admitted commit `ff2a1ef4`, with that fallback fixed now (C4 OQ-1, C-13) | **ratification** | blocks HARBOR-O and ADMIT-CORE |
| Q-X42 | Terminus-2 HIR as a later gated block (Round 8) (E2 F16) | ratification | round structure |
| Q-X43 | identity at the mock gate; system fonts in v1; transcripts (a) the gated exception, or (b) links only (C-07); Pages via Actions + ADR; default project URL; a `[results-site]` card (F2) | mock gate (identity); ratification (rest) | sets F2 tickets |
| Q-X44 | D3's defaults: compaction under declared W; effort `low`/`max` levels; 11 band-A dev tasks at k = 5; confirm m = 2 on 37; `edit_file` withheld unless funded; exploration after the L2 verdict | ratification (scope); G-P4-X / G-P4-C (sizes) | Round 7 only |

New questions surfaced by H1 are listed under Open questions (Q-H1-1…6).

---

## Part 8 — Decomposition readiness and sizing

### 8.1 First-cut inventory, Round 6 (names without ids; kinds per BM-TICKET)

| block | rows (kind) | depends |
|---|---|---|
| B0 | R6 plan-review/bootstrap (gate); GUARD-X: extend `hh-supplement-guard/1` to the experiments supplement, REQ-X coverage register and Round-6 routing (ticket) | decomposition |
| P1 | P1-A provenance (ticket); P1-B hardening run (ticket; pins 16/14/2 per C-06); P1-C corrections + AR-01/02/03 (ticket); P1-D study recompute (ticket); P1-E record, cards, checker (ticket) | GUARD-X; P1-C needs Q-X25 |
| P5a | F2-a shell (ticket); mock review (gate); F2-b generator core (ticket); F2-e phase-1 pages (ticket, after P1-E); F2-f claims checker + lexicon (ticket, after P1-E, C-25); F2-g Pages workflow + AR-28 (ticket); Pages enablement (human); phase-1 publication (gate) | P1 |
| P2-infra runner | RUNNER-CORE + AR-04 (ticket; spike + C-17/C-18 ACs); CELL-SPEC + AR-06; DEADLINE + AR-07; SELECT; VERDICT; EGRESS + AR-05 (absorbs C2-5); HARBOR-O (pin fixed) | GUARD-X; EGRESS after PROXY-CORE |
| P2-infra proxy | PRICE + AR-09; LANE + AR-11; PROXY-CORE-a + AR-08/10 (server, gate, receipts); PROXY-CORE-b (release modes, recovery); BUDGET; REPLAY; RECONCILE + AR-16 | PRICE, LANE first |
| P2-infra compute | RECIPE-a provision/verify + AR-15; RECIPE-b teardown/inventory/`compute-open.sh`; COST-RECORD + AR-14 | — |
| P2-infra methods | D1-a protocol + quote + AR-12/13; D1-b analysis II + battery + `hh-comparative-results/1` schema (C-33); method admission (gate); D1-c `answer-scan/1` | SELECT |
| P2-infra archive | ARCHIVE `hh-receipt-index/1` + condenser + AR-20 | REPLAY |
| P2-infra hosting | ADMIT-CORE + AR-18; ADAPT-CHAT-PY; ADAPT-CHAT-NODE; ADAPT-MESSAGES (Claude Code terms); TERMINUS-HOST + AR-19; CAPTURE; SMOKE-BATTERY (code + offline checker); RUNNER-CHECK | runner and proxy cores |
| P2-infra L1 tooling | K1 `l1-canon`; K2 observation-replay executor; K3 synthetic corpus + census generator; K4 L1 runner | REPLAY |
| P5b | F2-c comparative components (after D1-b); F2-d grid + trial pages + viewer (after ARCHIVE) | — |
| P2-S0 (optional) | Ollama settings (human); early S0 (gate); S0-SMOKE (ticket, execution) | RECONCILE |
| W1 | billing alert (human); provisioning W1 (gate); PROV-W1 (ticket); REPROOF (ticket, execution); HOST-SMOKE run (ticket, execution); O-CAPTURE synthetic (ticket, execution); TEARDOWN-W1 (ticket, mandatory AC); reconcile-W1 (human) | P2-infra |
| Pilot | D1-d selection + pre-registration I (ticket); HF dataset (human); pilot spend + W2 (gate); PROV-W2; CENSUS capture; PILOT run (execution); TEARDOWN-W2; reconcile-W2 (human); PILOT-PACKET (D1-e outputs + D2 K6 partitions + C4 R-live comparison) (ticket); pilot flagged-pass review (human); ARCHIVE-UPLOAD (ticket, execution) | W1 |
| P3a | E2-T0 ADR bundle + AR-21–24 (ticket, docs); E2-T1…T8 (8 tickets; T6 + AR-25); E2-T9 mini document + representation ledger (after PILOT-PACKET for O goldens); E2-T10 sweep glue; K7 L2 wiring | per E2 Part 7 |
| W4 | provisioning W4 (gate); PROV-W4; L1-GATE-RUN (K5, execution); TEARDOWN-W4; reconcile-W4 (human); G-L1 (gate) | E2-T10, K4, PILOT-PACKET |
| Quotes | D1-f tranche-1 quote (ticket); L2-QUOTE (ticket) | PILOT-PACKET, method admission |
| Tail (full) | CAP6.1, CAP6.2, CAP6.3 (capstone); GATE-ACCEPT (gate); REC6.1–6.3 (reconcile); DOC6.1, DOC6.2 (docs) | all |

**Round 6 count:** about **102 rows**. That is about 85 implement-spec rows (code, execution and tail), 10 gates and 7–8 HUMAN rows. The likely merges (the three C4 adapters into one or two; PROXY-CORE and RECIPE kept unsplit) bring it to 92–102. No Round-6 row is a skeleton: every paid body after a Round-6 gate is the pilot, whose design is fixed by the spec. Phase-1b, if chosen, adds a gate plus a skeleton.

### 8.2 Round 7 and Round 8 (decomposed later from the same ratified spec)

| round | contents | rows (estimate) | skeletons |
|---|---|---|---|
| **Round 7 — paid continuation** | tranche-1 gate; T1 window (PROV, run, TEARDOWN, reconcile), T1 results, flagged review, publication + dataset flip; G-L2; L2 window, K9 audit + L3, publication; D3-a (+ split), D3-b, D3-c, optional `edit_file`; G-P4-X; P4 windows; D3-d…i; G-P4-C; reviews; publication; optional T2/T3 cycles; full tail | ≈ 45–55 | none if decomposed after its gates. If the operator wants Round 7 appended now instead, every post-gate body (T1 run/results, L2 run/audit, D3-d/e/h/i) must be a `Kind: skeleton` behind its gate (BM-TICKET-03) |
| **Round 8 — Terminus-2** | gate; E2-T12…T16; D2 K10–K14; AR-30; Terminus pilot, L1 and L2 windows and gates; publication; tail | ≈ 35–40 | the L2 run, unless decomposed after G-L2 |

**Program total:** ≈ 180–195 rows, of which ≈ 145–155 are implement-spec rows.

### 8.3 Round structure recommendation

**Round 6 now (funded scope plus every $0 block), Round 7 after the paid-continuation gates, Round 8 for Terminus-2.** Reasons:
1. Only the pilot has funding in sight (Q-X11). Rows for T1, L2 and P4 written now would be skeletons for weeks, and Round 5 shows where that leads: many "stage voided" disposition rows (manifest rows 229–237).
2. The manifest is already about 160 KB and the LEDGER about 845 KB (`wc -c`). AGENTS.md already requires row-level reads. A ~190-row extension in one round makes every read heavier and keeps one round open across months of funding decisions.
3. Round 6 closes with real results: the phase-1 record, the site shell, the infra, the re-proof, the pilot, G-L1 and quotes for every paid continuation. Its capstone and acceptance therefore close a coherent evidence set.
4. Each later round's decomposition reads the gate decisions it depends on, so no paid body is guessed.

The cost is two more `decompose-spec mode=extend` sessions; Round 5's 247–251 extension shows one can be done the same day.

### 8.4 Tail recommendation: `tail=full` for each round

1. The retained guard's `tail-roles` check requires, for the active round, "capstone ×3, operator-acceptance gate, reconcile ×3, docs ×2 in order" (`docs/build/tools/README.md`). A minimal tail would need a guard change.
2. Like Round 5, the program combines native execution, methods, paid studies and publication (Round-5 decomposition decisions).
3. REC rows must read live state. REC6.1 reads `compute-open.sh`, the Ollama balance and usage, and the HF dataset visibility. REC6.2 reads landed code, ADRs and the REQ-X coverage. REC6.3 reads stack CI and branches.

### 8.5 Sizing hazards (dispatch_target = subagent; Round-5 rule: context estimate plus a 10k verification reserve ≤ 40 % of the worker context)

Pre-split: PROXY-CORE (C2 says "largest"), RECIPE (B3 hint), F2-c (F2 F14), D3-a (D3 Part 6). Watch: D1-b (estimators plus battery), E2-T2/T4 (rendering cuts across the model port; CC1 risk), ADMIT-CORE, RUNNER-CORE (spike plus kill-points). Execution tickets (REPROOF, PILOT, L1-GATE-RUN) must contain no new code; their machinery lands in earlier tickets (D3 Part 6 pattern).

---

## Part 9 — Outline trace mapping

| OL | discharged by (blocks / notes) | status proposal | de-scope candidate to raise at ratification |
|---|---|---|---|
| OL-01 | P1 (A1, A2; P1-A…E); README link via REQ-XA-122 | mapped | — |
| OL-02 | P2-infra, W1, pilot (Round 6); T1 (Round 7) — B1, B2, B3, C1–C4, D1 | mapped, conditional | **yes:** if the tranche gate declines, phase 2 delivers infrastructure, re-proof and an exploratory pilot that "measures, never ranks"; no comparative claim |
| OL-03 | D1 (pilot, quote, tranche design), C1/C2 budget, H1 Part 5 | mapped | — |
| OL-04 | F2 site + D1-g results packet + README link after the first publication (REQ-XF-136) | mapped, conditional on a publication gate | follows OL-02 |
| OL-05 | P3a (E1, E2) for mini-swe-agent; Round 8 for Terminus-2 | mapped (mini); staged (Terminus, Q-X42) | Terminus-2 HIR is a later round, not in Round 6 |
| OL-06 | D2 L1 (Round 6, $0 model); L2 (Round 7, paid) | mapped; L2 conditional | **yes:** without L2 funding, the phase-3 claim is "L1-conformant baseline" only (REQ-XD-126) |
| OL-07 | D3 / P4 (Round 7, paid) | mapped, conditional | **yes:** without G-P4-X funding, phase 4 is the design plus offline machinery only |
| OL-08 | C2 (budget, receipts, reconciliation), B3 (ledger), D1 (caps), REQ-XH-004…008 | mapped | — |
| OL-09 | C3 (survey, gates), C4 (admission record), B1/D1 (task admission, exclusions) | mapped | — |
| OL-10 | D2 (L1 request-level + L2 verdicts + L3 traces) | mapped | — |
| OL-11 | H1 Part 2 and Part 8 (after R5.59, which landed as 09fe8523; Rounds 6/7/8) | mapped | — |
| OL-12 | A2 (`record` leg, `[replay-record]` card), F2 (lexicon, site checker, `[results-site]` card), AR-32 | mapped | — |
| OL-13 | O1–O3 | open (synthesis) | — |
| OL-14 | H1 Part 8 + O4 | mapped (readiness) | — |
| OL-15 | per phase: P1 (deviation record, edition ADR, bounds); P2 (admission records, budget, pre-registration, scan, publication gate); P3 (L1/L2 packets, typed unknowns, veto layer); P4 (registry, two pre-registrations); site publication records | mapped | — |
| OL-16 | GUARD-X, full tail, REQ-XH-001…033, every ticket's universal AC | mapped | — |
| OL-17 | B3 recipe; per-window GATE/PROV/TEARDOWN/HUMAN-reconcile rows; `compute-open.sh` at closure | mapped | — |
| OL-18 | F1, F2; the mock-review gate | mapped; "beautiful" unestablished until the mock gate | — |

---

## Part 10 — Terminology namespace (design proposal for O1)

| token today | users | collision | proposed spelling |
|---|---|---|---|
| S1–S4 | round-5 spec scenarios | A1 F14 (records' "S3"/"S4") | keep for spec scenarios only; records say "parity tranche" / "confirmation cohort" (REQ-XA-015) |
| S1–S7 | B2 slot lifecycle; C4 S2/S4 probes | scenarios | `ST1…ST7` (slot stage) |
| S0, P0, P1 | D1 pilot stages | P1 phase, P1-A tickets | `PS0`, `PP0`, `PP1` (pilot stage) |
| S-pos … S-hang | C4 smoke variants | scenarios | `SM-pos … SM-hang` |
| L1–L3 | D2 parity layers | D3 levels | keep `L1`/`L2`/`L3` for parity layers |
| L1–L9 | D3 sweep levels | parity layers | `V1…V9` (variant levels) |
| G1–G9 | C3 admission gates | manifest `GATE-G<k>`, `G-L1` | `AG1…AG9` |
| T1–T3 | D1 tranches | E2-T*, Terminus branches T1–T13, theses T1–T7 | `TR1…TR3` |
| T1–T13 / M1–M19 | D2 branch fixtures | tranches | `BT1…BT13` / `BM1…BM19` |
| P (set), D (pool) | D1 | phases | `pilot set`, `dev pool` in prose |
| H, O | surfaces; H = harness count | — | keep; harness count spelled `n_h` |
| E2-T0…T16, K1–K14, D1-a…g, D3-a…i, F2-a…g, P1-A…F | ticket-shaping names | — | decomposer replaces them with chain ids; keep as aliases in `## Decomposition decisions` |

---

## Findings

### F1 — The build state is ready for a Round-6 extension, but the dispatch guard is bound to the round-5 supplement
Claim: R5.59 has landed (`nextTicket: DONE`, `lastCompleted: R5.59`), deferrals are empty and the last ids are GATE-G19, HUMAN-H11 and ADR-0422. `hh-supplement-guard/1` validates only the round-5 supplement's 180-candidate coverage and requires a full tail on the active round.
Status: VERIFIED
Evidence kind: source inspection + recorded execution
Evidence: `docs/build/LEDGER.md` `## CURRENT STATE` (l.15–37: `nextTicket: DONE`, `lastCompleted: R5.59`, `chainTip: codex/harnessharness-r5.59`, `dispatchTarget: subagent`, `mergePolicy: OPERATOR`). `bash scripts/deferrals-open.sh` → empty, exit 0 (2026-11-07T00:21:32Z). `ls docs/adr` → latest `ADR-0422-r5.57-verifier-bound.md`. Manifest grep → GATE-G19, HUMAN-H11. `docs/build/tools/README.md` Checks table (`coverage-*`: "The 180 literal candidates/aliases: 167 adopted / 12 merged / 1 superseded; frozen supplement sha256"; `tail-roles`: "capstone ×3, operator-acceptance gate, reconcile ×3, docs ×2 in order"). `wc -c`: manifest 159,454 B, LEDGER 845,441 B.
Retrieved: 2026-11-07T00:21Z–00:24Z
Implication for the spec: The first Round-6 ticket after its bootstrap gate extends the guard to a second frozen supplement with `REQ-X[A-H]-[0-9]+` and a REQ-X coverage register (AR-31, REQ-XH-002). Each round takes `tail=full` (Part 8.4).
Outline delta: EXTENDS OL-16 — the guard is part of the build invariants the program must preserve.
Limitations: I did not read `supplement_guard.py`'s code, so the size of the extension is not estimated. The LEDGER was read only at CURRENT STATE.

### F2 — Thirty-four cross-note conflicts have rulings; four carry operator-visible consequences
Claim: The sixteen notes conflict in 34 places (Part 1). All but four resolve by applying one note's verified text. The four that change scope, cost or evidence are C-08 (shared pilot), C-09 (phase-3 O surface), C-10 (87-task re-proof) and C-13 (Harbor pin).
Status: UNVERIFIED (rulings are design proposals; the conflicts themselves are VERIFIED from the cited texts)
Evidence kind: source inspection + design proposal
Evidence: Each Part 1 row cites its note findings. The previously recorded rulings are D2 F13 (C-01), B2 F18 (C-02, C-03), C4 handoff (C-13), and the ledger change log at 2026-11-06T23:20:00Z (C-06) and 23:22:29Z (C-08).
Retrieved: 2026-11-07T00:25Z–00:38Z
Implication for the spec: The spec's decision register carries Part 1 as binding rulings, each citing its notes. O2 reviews C-05, C-06, C-09, C-10, C-11 and C-15 specifically.
Outline delta: EXTENDS OL-11, OL-14, OL-16.
Limitations: The rulings rest on the notes as accepted. Later note revisions are not reflected.

### F3 — The HH-cell × `tb` basis-A denominator is 16/14; the 17/15 count reads a verdict out of an inconclusive slot
Claim: Over the confirmation packet's `first` slots, 16 tasks have an `executed` disposition with a resolved/unresolved receipt, and 14 agree with tranche 3. Admitting slots whose disposition is `inconclusive{…}` but which carry a receipt adds `hf-model-inference` (two network-isolated slots, receipt `unresolved`, O `unresolved`) and gives 17/15.
Status: VERIFIED
Evidence kind: recorded execution (read-only computation over committed packets)
Evidence:
- **Command:** `python3 -I - <<EOF …` (one-off, not committed).
- **Inputs:** `docs/build/reports/2026-11-05-parity-tranche-3.json` `pairs[].{task_id,o_verdict,o_failure_mode}` (22 resolved / 8 unresolved); `2026-11-05-s4-confirmation.json` `slots[]` (300 slots; 60 `first`); the egress, completion and 30-slot-repairs packets (520 records in total).
- **Results:** executed-first basis → 16 tasks, 14 agree, discordant {`grid-pattern-transform`, `swe-bench-langcodes`}. Any-receipt-first basis → 17 tasks, 15 agree; the extra task is `hf-model-inference`, whose `arm:o-tb-oracle` and `arm:h-podman-oracle` first slots are `inconclusive{environment:cell_network_isolated:Connection refused}` with receipt `{channel: verbatim, verdict: unresolved, verdict_reason: some_tests_failed}`; tranche-3 O verdict `unresolved`. A2's first-record-per-`slot_ref` rule over the four packets → 16 / 14.
Retrieved: 2026-11-07T00:24Z
Implication for the spec: The basis-A rule in C-06 becomes REQ-XH-032. P1-B's battery includes the mutation that admits `inconclusive_with_receipt` and must fail.
Outline delta: CONFIRMS OL-01 (A2 F1's 16/14); CORRECTS the coordinator's spot-check reading.
Limitations: The computation used the packets' `first` flag and `disposition` strings as written. I did not re-derive them from raw slot records.

### F4 — A single shared pilot is definable, but at D1's planning values it cannot serve D1, D2 and C4 fully under a $48 hard cap
Claim: Folding D2's O-only corpus (mini 16 × 3) and C4's R-live (8 O trials) into D1's pilot gives 220–258 trials, ≈ $30.6–35.9 expected and ≈ $45.9–53.8 at planning. D1's own design is ≈ $44.3 planning against a hard cap of $60 × 0.8 = $48.
Status: UNVERIFIED (arithmetic over priors; design proposal)
Evidence kind: design proposal (computation)
Evidence: D1 F9 (per-harness caps = 41.4 × planning $/trial, summing to ≈ $45.3; P0 $4.9; P1 $32.8; re-runs $6.6; total $44.3; cap = authorized × 0.8). D2 §2.6 (16 × 3 = 48 O trials; corpus ≥ 12 dev + 8 gate-A + 8 gate-B, REQ-XD-107). C4 §1.8 (R-live 24 or 8). C1 F8 (period end 2026-11-22T16:30:34Z). Computation in Part 5.2.
Retrieved: 2026-11-07T00:37Z
Implication for the spec: The spec defines one pilot (Part 2.3) with a declared priority order and `withheld{budget}` truncation. The pilot gate receives the Part 5.2 menu. No second pilot authorization exists for D2 or C4.
Outline delta: EXTENDS OL-03, OL-08 — one pilot packet feeds the tranche quote, the L1 corpus, the L2 quote and the runner check.
Limitations: All per-trial values are C3 no-cache priors scaled by D1's tail factor. Cheap-band and cached costs are probably lower (D1 F9 limitations); band-B dev tasks in P1-mini may cost more.

### F5 — Round 6 should open with a plan-review gate and a guard extension, then deliver phase 1 before infrastructure unless the operator races the credit reset
Claim: P1 (five $0 tickets) is independent of P2-infra and unblocks the site's phase-1 content. The P2 critical path to the pilot is about 35–38 runs plus four gate pauses, against ≈ 15.7 days to the credit reset.
Status: UNVERIFIED (design proposal; the schedule is inference)
Evidence kind: design proposal
Evidence: A2 F14 (P1-A…E dependencies). B2 §3 order; C2 §4 order; C4 §4 order; B3 hints; D1 F17; D2 Part 7; E2 Part 7; F2 F14. Manifest `## Operating rules` Round-5 scope (GATE-G8 bootstrap precedent). C1 F8. The clock at writing is 2026-11-07T00:32:25Z.
Retrieved: 2026-11-07T00:32Z
Implication for the spec: The spec states both linearizations (Part 2.4). Ratification picks one (Q-H1-2); neither authorizes spend.
Outline delta: EXTENDS OL-11.
Limitations: No throughput figure for code tickets of this size was measured. The round-5 chain's pace was not analysed.

### F6 — Seventeen gates and eleven HUMAN row types cover the program; none is pre-answered
Claim: Part 3 enumerates every operator gate with its pre-registered content, and every HUMAN act with owner and trigger.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: B3 handoff (GATE-PROV, HUMAN-BILLING-ALERT, HUMAN-RECONCILE, HUMAN-KEY-ROTATE). D1 F17 (method admission, pilot spend, tranche-1, flagged review, publication). D2 Part 7 (G-pilot, G-L1, G-L2, publication). D3 §5.2 (G-P4-X, G-P4-C). F2 F14 and REQ-XF-130 (mock review, Pages, publication, dataset flip). C2 ticket 8 and Q-X23 (S0). A2 REQ-XA-126 (phase 1b). E2 F16 (Terminus block). `layout.md` l.274–286 (markers).
Retrieved: 2026-11-07T00:38Z
Implication for the spec: The spec's gate table quotes thresholds verbatim for the marker files. The dedicated-key HUMAN row is optional and marked as a design proposal.
Outline delta: EXTENDS OL-15, OL-16.
Limitations: Whether the Ollama plan allows several API keys, and HF private-storage limits, are UNVERIFIED. Ollama and HF terms of service were not studied by any note.

### F7 — The deduplicated register has 32 entries, of which 22 are ADRs; one design item had no owner
Claim: The ADR, amendment, schema, record and supplement items proposed across the notes reduce to 32 register entries (Part 4). The HF-dataset receipt-archive upload path (AR-20) is required by Q-X35 and F2 REQ-XF-124, but no note designed it.
Status: VERIFIED (inventory from the cited notes); UNVERIFIED (merges and AR-20's design)
Evidence kind: source inspection + design proposal
Evidence: C2 §3 (ADR-C2-A…E); B2 Answer (ADR-B2-RUNNER/EGRESS/CELL/DEADLINE); C4 §3 (ADR-C4-A/B/C); E2 Answer (1 amendment, 3 ADRs, 1 schema change) and Part 5.2 (three Terminus ADRs); D1 F2 and handoff (support-gate and envelope ADRs); B3 handoff (ledger, quote ceilings, `gcloud`); A2 F9 and REQ-XA-115/116; F2 F3 and F10 (fonts, Pages); D3 Q-D3-7 and F4; ledger row H1 ("TB 2.0→1.0 substitution never recorded, `evaluate_costs`, claims graduation"); F2 REQ-XF-124 ("the Q-X35 archive ticket").
Retrieved: 2026-11-07T00:38Z
Implication for the spec: O1 writes AR-20's section (archive index, condenser, upload tool admission, token kept off the VM). Every ADR lands with its implementing ticket, except E2-T0's docs-only bundle and AR-01 in P1-C.
Outline delta: EXTENDS OL-15, OL-16.
Limitations: Whether AR-08's §6.6 reading needs a canonical amendment as well is left to O1/O2.

### F8 — Only the pilot is fundable now; the recommended full program carries ≈ $2.0k of model planning ceilings
Claim: At the notes' recommended designs, model planning ceilings sum to ≈ $2,016 (≈ $518 expected at $0.15/trial), and compute planning to ≈ $100. Only the ≤ $60 pilot has funding in sight.
Status: UNVERIFIED (priors)
Evidence kind: design proposal (computation)
Evidence: D1 F5 (T1 table); D2 F5 (L2 costs); D3 §2.5 and §4.2 (P4 envelope ≈ $695 ceiling); B3 F12 (template and r = $0.31414/h); Q-X11 ("$60 is a pilot only"). Part 5 arithmetic.
Retrieved: 2026-11-07T00:37Z
Implication for the spec: Every paid block beyond the pilot sits behind a gate that records a verbatim authorization. "Declined" is a valid typed disposition (REQ-XH-020, REQ-XH-025).
Outline delta: EXTENDS OL-03, OL-08.
Limitations: The tranche, L2 and P4 per-trial costs are HF-derived priors on a different serving setup (C1 F16). T2/T3 and Terminus are unquoted.

### F9 — 424 candidate requirements need about 60 aliases; one direct contradiction (ATIF) is resolved
Claim: The notes emit 391 candidates plus H1's 33. The merge groups in Part 6.2 (about 60 aliases) cover refinements stated by the notes themselves and the rulings in Part 1. The only direct contradiction between requirement texts is REQ-XB-013 vs REQ-XC-318 (C-16).
Status: VERIFIED (counts); UNVERIFIED (the alias assignment is a proposal)
Evidence kind: source inspection + design proposal
Evidence: grep of `REQ-X[A-H]-[0-9]{3}` row heads within each note's `## Spec requirements emitted` (2026-11-07T00:31Z). C2 handoff (REFINES list); F2 Spec requirements (F1 mapping); B2 handoff; A2 handoff; C4 REQ-XC-302/304/315; B1 REQ-XB-013 vs C4 REQ-XC-318 texts (digest).
Retrieved: 2026-11-07T00:31Z
Implication for the spec: O1 builds a coverage register with literal ids, aliases and one primary owner each (Part 6.3).
Outline delta: EXTENDS OL-13, OL-14.
Limitations: The alias list is not exhaustive at the single-id level. O1 must check every id.

### F10 — Nineteen §Q rows are pending; four must be decided at ratification for Round 6 to decompose cleanly
Claim: Q-X25 (blocks P1-C), Q-X41 (blocks HARBOR-O and ADMIT-CORE), Q-X42 (round structure) and Q-X22 (enters every renderer and the lane) need answers at ratification. The others can be recorded defaults or wait for their gates (Part 7).
Status: UNVERIFIED (design proposal)
Evidence kind: source inspection + design proposal
Evidence: Ledger §Q (Q-X20…Q-X44 rows as written at 2026-11-07T00:17:16Z); the cited notes' recommendations.
Retrieved: 2026-11-07T00:38Z
Implication for the spec: The spec's decision register lists each pending row with its default and decision point.
Outline delta: EXTENDS OL-11, OL-13.
Limitations: None beyond the recommendations' own.

### F11 — Three rounds fit the funding and record-size constraints better than one
Claim: Round 6 (≈ 92–102 rows) can contain everything funded or $0, closing with G-L1 and the quotes. Round 7 (≈ 45–55) and Round 8 (≈ 35–40) are decomposed after their gates, which avoids months-long skeletons. The program totals ≈ 180–195 rows.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: Part 8 inventory. Round-5 rows 229–237 ("Record the … disposition (stage voided)"). The manifest's `## Plan extensions` round-append precedent (rows 247–251, same day). `decompose-spec` SKILL Phase 2 (skeletons after gates; never pre-answer). File sizes in F1.
Retrieved: 2026-11-07T00:38Z
Implication for the spec: The spec's handoff section states the round structure, and `decomposition-prompt.md` scopes O4 to Round 6 (Q-H1-1).
Outline delta: EXTENDS OL-11, OL-14.
Limitations: Row counts are first-cut. Merges or splits at Phase-4 review will move them by about ±10 %.

### F12 — Program-level requirements close gaps no stream owned
Claim: Gating, sequencing, budget records, cross-phase reuse, terminology and publication need program-level requirements (REQ-XH-001…033) that no A–F note emitted.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal
Evidence: Parts 1–10 of this note.
Retrieved: 2026-11-07T00:39Z
Implication for the spec: The spec adopts REQ-XH as family XH.
Outline delta: EXTENDS OL-15, OL-16.
Limitations: The acceptance methods assume the guard extension (AR-31) and the publication record (REQ-XF-129) exist.

---

## Answer and completion assessment

**Answer.**
1. **Conflicts.** 34 cross-note conflicts carry rulings (Part 1). The basis-A cross-check is fixed at 16/14 with an exact inclusion rule (C-06, F3). One shared pilot replaces the four pilot designs (C-08, Part 2.3). Phase-3 O is the hosted original (C-09, for O2 review). The re-proof covers all 87 tasks before the split (C-10). One quote, one selection routine and one envelope ADR serve every phase (C-11). The hard cap uses a single reserve (C-15).
2. **DAG.** Blocks, dependencies, parallelism and two linearizations are in Part 2.
3. **Gates and HUMAN rows.** Part 3, none pre-answered.
4. **Register.** 32 deduplicated entries, 22 of them ADRs through Round 7 (21 needed in Round 6), plus three Terminus-2 ADRs in Round 8 (Part 4). The archive upload path (AR-20) had no designing note and is assigned to O1.
5. **Budget.** Part 5: only the pilot is fundable; its shared variant is capacity-bound under $48; the credit reset is a schedule risk.
6. **Requirements.** 424 candidates, about 60 aliases; family XH added; pattern `REQ-X[A-H]-[0-9]+` (Part 6).
7. **Pending §Q.** Part 7.
8. **Sizing.** Round 6 ≈ 92–102 rows, Round 7 ≈ 45–55, Round 8 ≈ 35–40; `tail=full` per round (Part 8).
9. **Outline.** All 18 obligations are mapped, with three de-scope candidates (OL-02/04, OL-06, OL-07) that are conditional on funding (Part 9).

**Settled:** the repository and build state (F1), the conflict inventory, the register contents, the requirement counts, the basis-A denominator (recorded computation) and the gate and HUMAN inventory.

**Design proposals, acceptable uncertainty for this row:** the rulings, the pilot definition and menu, the linearization, the round structure, the sizing and the cost arithmetic. Each has a named validation: O2 review, the pilot gate, or Phase-4 review in `decompose-spec`.

**Technical blockers:** none for synthesis. For decomposition: ratification answers to Q-X22, Q-X25, Q-X41 and Q-X42, and the Round-6 scope choice (Q-H1-1).

## Open questions

- **Q-H1-1 — Round structure** (operator, at ratification; affects O3, O4). Round 6 now with funded and $0 scope; Round 7 after the paid-continuation gates; Round 8 for Terminus-2 (recommended). The alternative is one round with skeletons behind gates.
- **Q-H1-2 — Race the credit reset?** (operator, at ratification; planning only; affects O4's linearization). Linearization A (phase 1 first) or B (pilot critical path first).
- **Q-H1-3 — Phase-3 O surface (C-09)** (O2 review; no operator decision unless O2 rebuts). The hosted original in an HH cell vs a Harbor-run O.
- **Q-H1-4 — Re-proof scope (C-10)** (operator, at the W1 provisioning gate). All 87 in one window, or banded.
- **Q-H1-5 — Dedicated Ollama key** (operator; optional HUMAN row). Create a program-only key before W2?
- **Q-H1-6 — Terms of service for publication** (operator, at the first publication gate that includes transcripts; owner O1/F2). Ollama's and Hugging Face's terms for automated benchmarking and for publishing TB-derived transcripts were not studied by any note. The gate readout should record that they were read.
- Carried unchanged: every pending §Q row in Part 7, with its decision point.

## Spec requirements emitted

| ID | Requirement (candidate) | Rationale / source | OL | Acceptance |
|---|---|---|---|---|
| REQ-XH-001 | The ratified experiments supplement covers phases 1–4 and the site. It is decomposed by `decompose-spec mode=extend` as Round 6 (funded and $0 scope), Round 7 (paid continuation) and Round 8 (Terminus-2), each from the same spec. No round decomposes a paid execution body before its gate is recorded, unless that body is a `Kind: skeleton`. | F11; Q-X42 | OL-11, OL-14 | Manifest review: Round-7/8 rows absent from Round 6, or skeletons with no Run line (validator) |
| REQ-XH-002 | The first ticket after Round 6's bootstrap gate extends `hh-supplement-guard/1` to the experiments supplement: second frozen-source digest, REQ-X coverage register, Round-6 routing. No product ticket dispatches before it lands. | F1; AR-31 | OL-16 | Guard fixture battery with a REQ-X case; `check-supplement-guard.sh` returns `DISPATCH` for the second Round-6 row only after the extension |
| REQ-XH-003 | Requirement ids keep `REQ-X[A-H]-[0-9]+` verbatim. A coverage register assigns each adopted id exactly one primary owner, numbered AC and layer. Aliases keep their literal predicate bytes. | F9; Part 6 | OL-13, OL-16 | Guard `coverage-*`/`owner`/`alias-predicate` checks over the REQ-X register |
| REQ-XH-004 | Every paid execution ticket is preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Brief §4; D1 F11; B3 REQ-XB-213 | OL-03, OL-08, OL-16 | Readout template fields; guard refuses dispatch of an execution row without a PASSED gate |
| REQ-XH-005 | The proxy root cap is authorized × (1 − m), with m declared at each spend gate (pilot default .20; later default .10 once a `settled` reconciliation exists). Per-harness caps are slices under it. | C-15 | OL-08 | Proxy budget property test with m from the gate record; readout field `m` |
| REQ-XH-006 | Model credit never pays compute. Each kind has its own ceiling in the phase spend ledger. | B3 REQ-XB-217; Q-X11 | OL-08, OL-17 | Ledger validator per-kind cumulative ≤ ceiling |
| REQ-XH-007 | A pilot gate readout records the provider credit period (start and end UTC) and whether the authorization covers one period or several. A provider reset is never new authorization. | C1 F8, REQ-XC-004 | OL-03, OL-08 | Readout fields; review |
| REQ-XH-008 | Every provisioning window is a sequence GATE-PROV → PROV → runs → TEARDOWN (mandatory AC) → HUMAN-reconcile. `compute-open.sh` is empty at every phase boundary and at each round's CAP.3. | B3 F14, REQ-XB-211 | OL-17 | Manifest pattern check; closure cites empty `compute-open.sh` output |
| REQ-XH-009 | A provisioning window is torn down before any gate row whose wait is not bounded inside its dead-man D, unless the gate readout records that the VM stays up and why. | B3 REQ-XB-212; Q-X38 | OL-17 | Receipts show no window spanning a gate without that readout line |
| REQ-XH-010 | Phase 2 has one pilot (Part 2.3): one authorization, one hard cap, one packet, with the declared stage priority order and `withheld{budget}` truncation. D2's O-corpus and C4's R-live are stages of it. | C-08; F4 | OL-03, OL-06 | Pilot packet lists all stages and their dispositions; no second pilot gate exists |
| REQ-XH-011 | Pilot outcomes never select harnesses, the reference, tasks, levels or assumed effects. D2's gate partitions are committed by digest before H uses the dev partition. | D1 F10; D2 REQ-XD-106 | OL-15 | Quote input manifest refuses outcome fields; partition commit precedes E2-T9 |
| REQ-XH-012 | Held-out tasks may serve phases 2, 3 and 4 only under each phase's own pre-registration. No selection or configuration step reads another phase's held-out outcomes. | D1 F4; D3 REQ-XD-212 | OL-07, OL-15 | Selection-packet input manifest refusal test |
| REQ-XH-013 | No two paid collections run concurrently on the model account. Each paid window's gate records the operator's exclusivity acknowledgement. | C-19 | OL-06, OL-08 | Readout field; schedule check across window receipts |
| REQ-XH-014 | One program-wide Harbor pin lives in the runner's tool-admission record. A change is a dated ADR amendment and creates new participant versions. Choosing a tag later than v0.25.0 requires re-running E1's agent-relevant diff. | C-13; Q-X41 | OL-02, OL-05, OL-06 | ADR presence; slot records carry the pin; diff record attached |
| REQ-XH-015 | The spec's decision register carries every Part-1 ruling with its cited notes. A later change to a ruling is a dated register entry, never a silent edit. | F2 | OL-13, OL-16 | O2 dispositions reference ruling ids; history check |
| REQ-XH-016 | The reattempt policy is declared per protocol: phase-2 and phase-4 protocols ≤ 2 per plan and ≤ 10 % of plans; the L2 protocol allows zero replans. | C-17 | OL-06, OL-16 | Runner fixture with a zero-replan protocol: an infra-failed slot stays uncertain |
| REQ-XH-017 | The runner supports two declared schedule kinds, seeded k-round interleaving and pair-coupled simultaneous launch in blocks, and records actual start order. | C-18 | OL-03, OL-06 | Runner fixtures for both kinds; coupled pair launched in one packing step |
| REQ-XH-018 | Each phase that publishes has a publication GATE, an `hh-site-publication/1` record and, where transcripts exist, a separate HUMAN dataset-visibility flip after the secret and contamination scans. | F2 REQ-XF-129/130; Q-X35 | OL-04, OL-15, OL-18 | Manifest rows present; publication record validator |
| REQ-XH-019 | `docs/evidence.md` card transitions follow one register (AR-32). No program card exceeds "Available — internal record; not outside-verified" without the graduation steps, and each transition is pinned in `check-position.sh` with a mutation. | A2 F12; F2 Q-F2-6 | OL-12 | `test-check-position.sh` mutations per card |
| REQ-XH-020 | Negative, inconclusive, declined and withheld outcomes appear in every phase record and site page with the same structure and prominence as positive ones. | Brief §5; A2; F2 F5 | OL-15, OL-18 | Site fixtures per typed disposition; claims check |
| REQ-XH-021 | Each round's closure states, per kind (model, compute) and per phase, the authorized, spent (metered), billed or settled amounts and the reconciliation status, folded from the phase spend ledgers. | B3 F15; C2 F14 | OL-08, OL-17 | CAP.3 closure recompute from the ledgers |
| REQ-XH-022 | Spec, tickets and packets use the Part-10 namespaced terms. The round-5 scenario ids S1–S4 are never reused for program stages. | C-28; A1 F14 | OL-12 | Claims-lexicon check over records and site for the reserved spellings |
| REQ-XH-023 | Secrets (Ollama key, HF token, GCP credentials) appear only as `provided: yes/no`. The Ollama key reaches a VM only through `deliver-secret`. The HF write token never reaches a VM. | B3 REQ-XB-219; Q-X35/Q-X37 | OL-15 | Leak sweep over harvested artifacts and records; recipe shim test |
| REQ-XH-024 | Paid bodies after a gate are either skeletons (no Run line) or rows in a round decomposed after that gate. | `decompose-spec` Phase 2; BM-TICKET-03 | OL-14, OL-16 | Validator skeleton checks |
| REQ-XH-025 | Every gate marker quotes its thresholds verbatim from the spec before the gated work begins. A pending gate is RETURN PASS, never `blockedOn`. | `layout.md` markers; brief §2 | OL-16 | Guard `nextticket`/`pending-prerequisite` checks; marker review |
| REQ-XH-026 | Every HUMAN row names owner, trigger, exit and the claims it withholds. Code tickets proceed past it only by opening a DEFERRALS row that cites it. | BM-TICKET-05 | OL-16 | Validator marker checks |
| REQ-XH-027 | A live leg's evidence earns `live-executed` only through a registered `scripts/live-verify.sh` leg producing `hh-live-evidence/1`. Otherwise it is labelled recorded execution. | ADR-0363; B2 REQ-XB-126; C2 REQ-XC-125 | OL-12, OL-15 | `check-live-evidence.sh` |
| REQ-XH-028 | The phase-1 corrections ticket records, append-only, the round-5 compute correction, the R5.59 post-state evidence note and the coordinator's project `lifecycleState` observation. | C-26; B3 F3/F4 | OL-17, OL-01 | `check-history.sh` additions only; record cites the notes |
| REQ-XH-029 | Each round's REC rows read live state: open compute (`compute-open.sh`), model-account balance and usage, dataset visibility, stack CI. Statements without a cited read are removed. | BM-TAIL-04; Part 8.4 | OL-16, OL-17 | Tail live-read ACs |
| REQ-XH-030 | The single quote artifact `comparative-quote/1` serves every paid gate in phases 2–4 (tranche, G-L2, G-P4-X/C) and carries both model and compute lines. No other quote routine exists. | C-11 | OL-03, OL-08 | Code review; each gate readout cites a `comparative-quote/1` id |
| REQ-XH-031 | Phase-3 O runs the frozen original layer under the phase-2 hosting path, with the same proxy, VM and verifier channel as H. Its Harbor-equivalence is the C4 R-replay pass recorded before G-L1. | C-09 (O2 review) | OL-06 | Participant-record equality check (REQ-XD-112); R-replay record precedes G-L1 |
| REQ-XH-032 | The phase-1 basis-A cross-check counts a task iff it has a `first` confirmation slot with `disposition == "executed"` and a resolved/unresolved receipt. Slots with a receipt but another disposition are confounds (`inconclusive_with_receipt`), never verdicts. | C-06; F3 | OL-01, OL-15 | P1-B pins 16/14/2; the inclusion mutation (17/15) fails |
| REQ-XH-033 | The TB 2.1 oracle re-proof covers every eligible task before the split, or, if the provisioning gate chooses the banded variant, each band before that band's seeded draw, with per-cell dev counts fixed beforehand from declared fields. | C-10; D1 F4 | OL-02, OL-09 | Split record cites a re-proof packet covering its band(s); timestamps order |

## Handoff

- **Dependencies resolved:** every A–F row (24–39) consumed through the digest plus the bounded body reads listed under Scope. The §Q register is consolidated in Part 7. The ledger change log's carried items are dispositioned: E1's L1 capture spike is subsumed by K3–K5, the W1 synthetic capture and the W2 census; B1a image measurement is folded into the W1 prepull and observe receipts; D2's pilot is folded into the shared pilot.
- **For O1 (synthesis):** adopt Parts 1–10 as the decision register, the gate and HUMAN tables, the ADR register (AR-01…32), the budget envelope, the requirement families and coverage-register plan, the namespace table and REQ-XH-001…033. Write AR-20 (archive upload) and the AR-32 card register, which no stream note designed.
- **For O2 (review):** C-05, C-06, C-09, C-10, C-11, C-15; AR-08's possible §6.6 amendment; AR-27 (the l.234 reading); the Part 5.2 arithmetic.
- **For O3 (ratification):** the operator decisions are Q-X22, Q-X25, Q-X41, Q-X42, Q-H1-1 and Q-H1-2. The remaining recorded defaults (Q-X24, Q-X27, Q-X29, Q-X33, Q-X36/38 policy, Q-X40 design, Q-X43 non-identity, Q-X44 scope) stand unless overridden.
- **For O4 (decompose-spec):** Round 6 per Part 8.1, `tail=full`, `dispatch_target=subagent`, the Round-5 sizing rule, pre-splits per Part 8.5, and gate and HUMAN numbering continuing after GATE-G19 and HUMAN-H11.
- **Conflicts with other notes:** all are recorded in Part 1. This note corrects the coordinator's 17/15 spot-check reading (F3).
- **Capability and portfolio dispositions:** none changed. Nothing here is fixture-, live- or foreign-run-verified. The one computation is a read-only recount of committed packets.
