<!--
  Round-6 contract — experiments first pass (decompose-spec mode=extend, 2026-11-07).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Hand-off: docs/experiments-decomposition-prompt.md. Partition + coverage:
  docs/build/planning/2026-11-07_round6-{decomposition,coverage}.md.
  Insertion authorized by the operator (planning conversation, 2026-11-07): "I want you to
  continue on until you're totally done, the new tickets are inserted, there's a new PR with
  the changes". This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# HUMAN-H14 — Pre-FP-S account setup: Ollama settings and exclusivity, HF dataset and token, optional dedicated key

- **Sequence:** 299 of 325 · **Phase:** Round 6 · **Kind:** human
- **Block:** W2 · **Alias:** HUMAN Ollama-settings + HF-dataset + dedicated key (optional)
- **base_branch:** current checkout
- **Depends on:** R6.33
- **Blocks:** GATE-G26 (FP-S) and R6.40 (W2, which depends on this row)
- **Owner:** operator
- **Scheduled:** trigger — after R6.33 lands (pre-registration hashed) and before GATE-G26 is presented; one operator batch; actual UTC recorded per item (`date -u +%FT%TZ`) — the readout date feeds R6.33's FP-S readiness record
- **Withheld until done:** item 1 — every paid step; item 2 — archive upload and the transcript viewer; item 3 — none
- **Deferrals so far:** 0
- **Readout:** `docs/build/readouts/HUMAN-H14.md` (create from `_TEMPLATE.md` at presentation; one dated section per item)

> **Operator work — NOT an `implement-spec` input.** Account settings, the dataset repository and
> credentials are the operator's acts. An operator or authorized human record supplies the decision;
> an agent must not sign or assume silence is approval. No secret value is ever written anywhere.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), HUMAN table rows consolidated here (shared trigger):

> | Ollama account settings + exclusivity | operator | before FP-S (or the PS0 early gate) | automatic usage billing confirmed off; if it cannot be, the FP-S readout records that the model line is meter-enforced only (REQ-XH-120); exclusivity per window | paid steps |

> | HF dataset repo + write token | operator | before FP-S | private repo exists; fine-grained token scoped to the one dataset, off-VM, never stored by a `login` command, `provided: yes` | archive upload; transcript viewer |

> | Dedicated Ollama key (optional, Q-H1-5) | operator | before W2 | program-only key, revocable, `provided: yes` (multiple keys per plan UNVERIFIED); settlement still relies on exclusivity, because usage has no per-key breakdown | none |

Cited requirement and decision text, verbatim:

> | **REQ-XH-013** | No two paid collections SHALL run concurrently on the model account; the operator's own use of the account counts as overlap. Each paid window's gate SHALL record the operator's exclusivity acknowledgement (C-19). | Readout field; schedule check across window receipts. · `AC-4.4-10` · D |

> | Q-X21 | Ollama credit mechanics: overage behaviour, post-reset allowance, exclusivity | pending — default: in a HUMAN row before FP-S the operator confirms automatic usage billing off (or the FP-S readout records the model line as meter-enforced only) and acknowledges exclusivity; the authorization states whether the post-reset allowance or purchased credit may fund A (FP-12; Q-X54) | FP-S gate (spend) |

> | Q-H1-5 | Dedicated program-only Ollama key | operator; optional HUMAN row |

REQ-XH-120 (last sentence): "FP-S SHALL require automatic usage billing confirmed off in the Ollama HUMAN row, or record verbatim that the model line is meter-enforced only (Q-X21)." REQ-XH-102 (token clause): "The HF write token SHALL be fine-grained and scoped to the one dataset repository, read at use from the git-ignored env file named by `HH_OPERATOR_ENV_FILE`, passed per invocation through the environment, never printed, written or persisted by a `login` command, and recorded only as `provided: yes/no` (REQ-XH-023)."

## Checklist

**Item 1 — Ollama account settings and exclusivity** (required)
- [ ] Automatic usage billing confirmed **off**; if it cannot be turned off, record that verbatim so the FP-S readout states the model line is meter-enforced only (REQ-XH-120; Q-X21).
- [ ] Exclusivity acknowledged per paid window (W2, W4): no other collection and no operator use of the account overlaps a paid window (REQ-XH-013); the acknowledgement is repeated in each paid gate's readout (GATE-G26, GATE-G30).
- [ ] `OLLAMA_API_KEY provided: yes` in the git-ignored env file named by `HH_OPERATOR_ENV_FILE` (read at use, never copied; reaches a VM only through `deliver-secret`, REQ-XB-219).
- [ ] Note for GATE-G26 (not decided here): whether the post-reset allowance or purchased credit may fund A is Q-X21's FP-S decision.

**Item 2 — Hugging Face dataset repo and write token** (required)
- [ ] Private dataset repository exists (`private: yes`).
- [ ] Fine-grained write token scoped to that one dataset, kept off-VM in `HH_OPERATOR_ENV_FILE`, never stored by a `login` command — `HF token provided: yes` (REQ-XH-102, X-05).

**Item 3 — dedicated, revocable program-only Ollama key** (optional, Q-H1-5)
- [ ] Taken: `dedicated key provided: yes` (settlement still relies on exclusivity — usage has no per-key breakdown); or recorded `DECLINED`.

## Exit criterion

The row lands when items 1 and 2 are recorded complete; item 3 is recorded either way (`provided: yes`
or `DECLINED`). A partial item 1 or 2 is not completion. The readout's per-item UTC is the HUMAN-H14
input to R6.33's FP-S readiness record; nothing is back-dated.

## DEFERRALS rule

R6.40 depends on this row and cannot run before it. A `DEFERRED` status (R6.1 vocabulary, with its DF
id) withholds every paid step and the archive upload, so GATE-G26 is not presentable under it. A second
deferral needs the operator's explicit words (new date, spec amendment or ADR waiver).

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the
manifest operating rules; X-05 (secrets: `provided: yes/no` only), X-02. No code branch is created by
a HUMAN row.

## Notes

- Owns no REQ id; contributes the exclusivity record to REQ-XH-013 (owner GATE-G26).
- Consolidates the former HUMAN-H15/H16/H17 (plan merge map); the HF repository identifier's committed form (if any) is resolved at presentation against §4.10 — the token never.
