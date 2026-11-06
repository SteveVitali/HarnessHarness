# Experiments program — outline trace

Each `OL-*` is an obligation extracted from the [founding brief](experiments-brief.md) §2 (the
operator's handoff brief) and its §3 addenda. Synthesis maps every obligation to the spec
section that discharges it, or to a conscious de-scope raised at ratification (Q-X20). Status
vocabulary: `open` → `mapped(<spec §>)` | `de-scoped(<reason>, Q-id)`.

| id | obligation (source) | rows | status |
|---|---|---|---|
| OL-01 | Phase 1: document the executed experiment as a proof-of-correctness record in the repo, linked from the README, stating methodology and results with the honest bounds (§2 goal 1). | A1, A2 | open |
| OL-02 | Phase 2: run real LLM-agent evaluations comparing mainstream open-source (and possibly closed-source) harnesses on a benchmark task set, with HH as the lab (§2 goal 2; A-4, A-5, A-7). | B1, B2, C1–C4, D1 | open |
| OL-03 | Phase 2 scaffolding: funded model budget, per-model/agent admissions, a declared tranche design (N, endpoints, margins) (§2 goal 2; A-3). | C1, C2, D1, H1 | open |
| OL-04 | Phase 2 results published clearly and linked from the README (§2 goal 2). | F2, H1 | open |
| OL-05 | Phase 3: represent a simple open-source harness in HIR (§2 goal 3; A-8: mini-swe-agent then Terminus-2). | E1, E2 | open |
| OL-06 | Phase 3: prove HH-native ↔ original parity with the established parity machinery — same participant, both surfaces, directional agreement, declared margins, recorded discordances (§2 goal 3; A-8 layered). | D2, E2 | open |
| OL-07 | Phase 4: vary one harness variable at a time across HH-native configurations, measure systematically, report which perform best (§2 goal 4; A-8 explore → confirm). | D3, E2 | open |
| OL-08 | Decide the funding model: per-model ceilings, admission records, spend telemetry in packets (§2 open questions; A-3). | C2, D1, H1 | open |
| OL-09 | Decide which harnesses and benchmarks enter the comparative set, with admission criteria (§2 open questions; A-5, A-7). | B1, C3 | open |
| OL-10 | Decide whether HIR parity compares task verdicts or richer traces (§2 open questions; A-8). | D2 | open |
| OL-11 | Decide phase ordering vs R5.59 and whether the phases are one manifest extension or several rounds (§2 open questions; A-1, A-10). | H1 | open |
| OL-12 | Publication surface: README links and `docs/evidence.md` card states pinned by `check-position.sh`, respecting its mutation battery (§2 open questions; A-9). | A2, F2 | open |
| OL-13 | Deliverable (a): a rigorous design spec per the repo's decompose-spec conventions (§2 handoff task). | O1–O3 | open |
| OL-14 | Deliverable (b): a ticket decomposition appendable to `docs/tickets/00_MANIFEST.md` (§2 handoff task). | O4, H1 | open |
| OL-15 | Deliverable (c): honesty/admission scaffolding per phase — budget records, model admissions, parity declarations, publication gates (§2 handoff task). | A2, C2, D1, D2, F2, H1 | open |
| OL-16 | Preserve every Build-orchestration invariant: manifest chain and guard, GATE/HUMAN semantics, build memory and validator, append-only records, typed dispositions, `mergePolicy: OPERATOR`, green CI, size trigger, verbatim spend authorization (§2 build orchestration). | H1, all | open |
| OL-17 | Backend lifecycle: provisioned compute is deprovisioned with a receipt; reuse or re-provisioning is a budgeted, recorded decision (§2 state in flight; A-10). | B3 | open |
| OL-18 | Results website: static, generated from committed records, beautiful and HH-specific, inspired by the HF harness leaderboard and Artificial Analysis, hosted from the repo via GitHub Pages behind a publication gate; content per A-6 (addendum A-6, A-9). | F1, F2 | open |
