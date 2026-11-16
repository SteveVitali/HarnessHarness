<!--
  Round-6 contract — H-allow hardening insert (decompose-spec mode=extend,
  orchestrate-build §2.4; insert spec
  docs/build/planning/2026-11-15_hallow-hardening-insert.md sha256 ff39c66f…).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Insert authorized by operator ruling R-2; the smoke envelope is R-3's
  authorization (interactive, 2026-11-15 — recorded in the GATE DECISIONS
  rows). This contract CITES the spec; it does not copy a design. Executed
  contracts are frozen: amend by appending a dated note.
-->
# GATE-G28i — PROV-SMOKE: the non-evidence smoke envelope's compute authorization (R-3 — up to 3 smoke windows on the exact window recipe, $0 model, results never re-proof evidence)

- **Sequence:** 309l of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** smoke · **Alias:** GATE PROV-SMOKE
- **base_branch:** current checkout
- **Depends on:** R6.45j, R6.45k
- **Readout:** `docs/build/readouts/GATE-G28i.md` (owed — the envelope presentation lands before smoke 1's PROV)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval. The readout's status
> is set from the operator's R-3 words as recorded — the agent signs nothing.

## The authorization this readout records (R-3, verbatim per the session's rulings)

The non-evidence smoke envelope, **amended R-3** as recorded in the GATE DECISIONS row:

- **$0 model** — `model_line: none`, no `deliver-secret`, no model key in-window, no metered calls (REQ-XB-219's rule applies verbatim).
- **Up to 3 smoke windows** on the exact window recipe (`hh-compute-decl/1` `mode: full`), each a distinct `window: smoke1|smoke2|smoke3`.
- **Each ceiling ≤ 1 250 000 µ$** by the §3.10 formula (r·(D + restart) + residual disk at h_r + egress — re-minted from the live rate table, never hand-computed); **Σ ceilings ≤ 3 500 000 µ$**.
- **In-window code changes permitted** (A1's window class) — smoke windows are *non-evidence*: results are labelled "non-evidence smoke (internal; never re-proof evidence)", never folded into `hh-reproof-packet/1` or `hh-reproof-register/1`, never cited as re-proof or parity evidence.

## What the readout must carry

1. R-3's ruling verbatim as the compute authorization (the GATE DECISIONS row cited).
2. The A1 label rule stated verbatim: every smoke artifact is labelled "non-evidence smoke (internal; never re-proof evidence)".
3. **Each smoke's declaration and quote** — minted by the one quote path (`receipt_lib.py quoteid` over the canonical `hh-compute-quote/1`) — **appended to the readout as an agent-drafted row with its hash, under the envelope, before that window's PROV.** A quote outside the envelope stops for a separate operator answer.
4. The prediction-track table (A3 — each prior window's readout-predicted outcome vs the actual outcome) and the §6 re-scope ladder, so this readout meets the amended gate-content rule.
5. Dispositions: `envelope_authorized` (the smokes may PROV under it) | `not_run`.

## Terms (pre-registered)

- Window shape per smoke: `mode: full`, e2-standard-8, debian-12, the declared podman/crun/netavark pins; boot disk 150 GiB unless the image set needs more; **D = 1.5 h** dead-man; ≤ 0.5 h harvest-only restart; h_r 6 h. (Ceiling example from the insert spec: 314 140·(1.5 + 0.5) + 150·137·6 + 0.12·1 GiB ≈ 871 580 µ$ — the live rate table governs.)
- Runner: the chain tip that includes R6.45j and R6.45k (both landed — PRs open, CI green — before smoke 1).
- Members: H-allow r = 1 on `prove-plus-comm`, `torch-tensor-parallelism`, `query-optimize`, and `pytorch-model-cli` only if epoch-4 was minted; H-pub r = 1 on `query-optimize` only.
- Exit: SMOKE-PASS → the next row (GATE-G28j); envelope exhausted without SMOKE-PASS → `blockedOn: H-allow smoke envelope exhausted` + a §6 diagnosis readout — no further spend.

## Out of scope

- Any evidence-window act: the smoke results are never re-proof evidence, never a `hh-reproof-packet/1` input, never a frozen-list outcome.
- Wc9 itself — GATE-G28j + R6.45m own it; this envelope authorizes only the smoke loop.
- The conformance leg — R6.45k's `egress-real-clients` is its own registered $0 leg, cited here only as smoke-context evidence.
