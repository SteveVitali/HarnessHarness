# HUMAN-H14 readout
Status: COMPLETE   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim)

Spec §10.3 HUMAN table rows consolidated here (shared trigger — after R6.33 lands, before GATE-G26):

> | Ollama account settings + exclusivity | operator | before FP-S (or the PS0 early gate) | automatic usage billing confirmed off; if it cannot be, the FP-S readout records that the model line is meter-enforced only (REQ-XH-120); exclusivity per window | paid steps |

> | HF dataset repo + write token | operator | before FP-S | private repo exists; fine-grained token scoped to the one dataset, off-VM, never stored by a `login` command, `provided: yes` | archive upload; transcript viewer |

> | Dedicated Ollama key (optional, Q-H1-5) | operator | before W2 | program-only key, revocable, `provided: yes` (multiple keys per plan UNVERIFIED); settlement still relies on exclusivity, because usage has no per-key breakdown | none |

> **REQ-XH-013** — No two paid collections SHALL run concurrently on the model account; the operator's own use of the account counts as overlap. Each paid window's gate SHALL record the operator's exclusivity acknowledgement (C-19).
> **REQ-XH-120** (last sentence) — FP-S SHALL require automatic usage billing confirmed off in the Ollama HUMAN row, or record verbatim that the model line is meter-enforced only (Q-X21).
> **REQ-XH-102** (token clause) — the HF write token SHALL be fine-grained and scoped to the one dataset repository, read at use from the git-ignored env file named by `HH_OPERATOR_ENV_FILE`, passed per invocation through the environment, never printed/written/persisted by a `login` command, recorded only as `provided: yes/no` (REQ-XH-023).

## Evidence (per item: source, `date -u` of the read, layer reached)

- Item 1 — Ollama account settings + exclusivity — operator's own account act — read at presentation — layer: `provided` (billing off / meter-enforced only verbatim; exclusivity ack; `OLLAMA_API_KEY provided: yes`).
- Item 2 — HF dataset repo + write token — operator's own act — read at presentation — layer: `provided` (private repo exists; fine-grained token scoped to the one dataset, `provided: yes`).
- Item 3 — dedicated Ollama key (optional, Q-H1-5) — operator's own act — read at presentation — layer: `provided: yes` or `DECLINED`.

<!-- agent-drafted:begin sha256=f25240a161b68617453dbc4d9917280c39f7b3e85a5bd67921e35824a8160def -->
## Decision content for the operator

**One operator batch, three items.** Items 1 and 2 are required (every paid step and the archive upload are withheld until both are recorded complete); item 3 is recorded either way. A partial item 1 or 2 is not completion. The readout's per-item `date -u` is the HUMAN-H14 input to R6.33's FP-S readiness record — nothing is back-dated. R6.40 depends on this row; a `DEFERRED` (with its DF id) withholds every paid step + the archive upload and makes GATE-G26 not presentable.

**Item 1 — Ollama account settings + exclusivity (required):**
- Is automatic usage billing **off**? If it cannot be turned off, say so verbatim — the FP-S readout then records the model line as **meter-enforced only** (REQ-XH-120).
- Exclusivity per paid window (W2, W4): no other collection and no operator use of the account overlaps a paid window (REQ-XH-013). Acknowledge it; it's repeated in each paid gate's readout (GATE-G26, GATE-G30).
- `OLLAMA_API_KEY provided: yes` in the git-ignored env file named by `HH_OPERATOR_ENV_FILE` (read at use, never copied; reaches a VM only through `deliver-secret`).

**Item 2 — HF dataset repo + write token (required):**
- A private dataset repository exists (`private: yes`).
- A fine-grained write token scoped to that one dataset is in `HH_OPERATOR_ENV_FILE`, off-VM, never stored by a `login` command — `HF token provided: yes` (REQ-XH-102).

**Item 3 — dedicated program-only Ollama key (optional, Q-H1-5):**
- `provided: yes` (settlement still relies on exclusivity — usage has no per-key breakdown) or `DECLINED`.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decisions (verbatim, received 2026-11-10T21:42:46Z via interactive chat):
- Item 1a — billing: "auto-reload of credits is on and billing is set up" → automatic usage billing cannot be confirmed off; recorded verbatim → the model line is **meter-enforced only** (REQ-XH-120; Q-X21). Item 1a complete by the verbatim meter-enforced path.
- Item 1b — exclusivity: "Acknowledged" (informed re-answer after the REQ-XH-013 explanation) — no other collection and no operator use of the model account overlaps a paid window (W2, W4); acknowledgement repeated in GATE-G26/G30 readouts.
- Item 1c — OLLAMA_API_KEY: "OLLAMA_API_KEY lives in ~/MetaHarness-harnessharness/.env" → provided: yes in the env file HH_OPERATOR_ENV_FILE names.
- Item 2 — HF dataset repo + write token: "ok did those 3 steps. proceed" — private dataset repo created, fine-grained write token scoped to it placed as HF_WRITE_TOKEN in ~/MetaHarness-harnessharness/.env (verified present in .env, value never read) → provided: yes. Item 2 complete.
- Item 3 — dedicated program-only Ollama key: "provided: yes" (settlement still relies on exclusivity — usage has no per-key breakdown).
Operator confirmation of agent-drafted text (verbatim, 2026-11-10T21:42:46Z): "sounds good. please raise whatever you need from me interactively" + "ok did those 3 steps. proceed" — covers sha256:f25240a161b6
GATE DECISIONS row: 2026-11-10T21:42:46Z | HUMAN-H14
Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator), which does not sign.
