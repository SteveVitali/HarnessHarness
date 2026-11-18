# ADR-0450: Ws1 L1.1 reference semantics — recording-derived `o_rec`, channel-typed `submission`, terminal-echo reconstruction (R6.45q amendment)

- **Status:** Accepted — recorded under the GATE-G29 Reading 2 disposition ("diagnose and fix any failures and iterate until success") and the operator's "Amend fold via ADR + re-fold" selection; documents the Ws1r (GATE-G28o) fold classification and repairs the reference semantics the first fold exposed
- **Date:** 2026-11-18
- **Ticket:** R6.45q (Ws1 re-proof); amendment authored in the Ws1r window's follow-through
- **Requirement ids:** REQ-XD-104 (`compare_drives` equality), REQ-XD-109 (L1 gate), REQ-XE-113/REQ-XE-116 (stop_rule / completion-surface family), ADR-0100 I-4 (effect settle terminals)
- **Spec:** `hh-l1-conformance/1` / `hh-l1-diff/1`; Ws1 experiment contract L1.0/L1.1/L1.2 as recorded in `docs/build/runs/R6.45q.md`
- **Rung (ADR-0024 obligation):** an explicit non-contract note — this ADR governs the experiments-program L1 fold semantics and drive-doc extraction, not the canonical kernel contract ladder.

## Context

The Ws1r re-proof (GATE-G28o, replay-mode, compute-only) ran all 16 legs `exited{0}`: every `l1_1` leg traversed **100% of its recorded calls under strict byte-comparison** (596/596 positions) and every `l1_2` leg passed `divergences: 0`. The fold nevertheless reported `l1_1: fail` with 622 diffs. Exhaustive classification found four structurally distinct causes, only one of them machinery:

1. **Reference-semantics mismatch (590 diffs).** `compare_drives` compared H's replay leg against `o_a` — the durable **live** l10 capture. Its `messages[3].content` carries the *live* environment (`Nov 18` mtimes); the replay leg faithfully carries the *recording's* (`Nov 17`). A replay leg that byte-matches the recording **cannot** byte-match a later live capture of the same trajectory — environment drift is inherent to the comparison, not a parity violation.
2. **Crash-truncated corpus (2 seqs).** `ws1-corpus-02` and `ws1-corpus-26` recordings are crash-recovered truncations: the live l10 captures continued past the recording (98 vs 68 and 73 vs 49 requests). No replay leg can reproduce requests the recording does not contain; comparing the tail is unanswerable by construction.
3. **Extraction artifacts.** `o_drive` read `traj.submitted` at the top level while mini-swe-agent's `trajectory.json` nests it under `info.exit_status`/`info.submission` — O's termination degraded to `exception{unspecified}` (32 tail diffs' O side). `h_drive` substring-matched `"finish"` against `lifecycle.run.finished`, minting `termination:"submitted"` on an `infrastructure_failure` — H's side was equally synthetic.
4. **The genuine machinery gap.** Every marker-terminated recording's final call proposes `bash: echo COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT`. O's runtime intercepts that marker and terminates `submitted` **without recording the echo's observation** — no subsequent request exists to carry the tool result. H's replay queue therefore overran at the terminal effect; with no marker in the served output, `marker_detected` correctly stayed false, and the leg sent one request past the corpus end (`extra_request`, strict-endpoint refusal, `exception{unspecified}` terminal).

## Decision

1. **`o_rec` — the recording-derived reference drive.** The fold spec gains `o_rec`: `requests` from `calls[k].upstream_request.body` (upstream-bound bytes; `downstream` fallback), `termination:"submitted"` when the last recorded response proposes the submit marker, `"exception{corpus_boundary}"` otherwise. L1.1 compares H against `o_rec`; `o_a`/`o_b` remain the L1.0 stable-prefix inputs and the `l1_2` comparator, and the live comparison is retained as a **diagnostic** (env-drift is expected and labeled, never silently zeroed). Request `content_type` is not retained by the recording; `o_rec` adopts the corpus-uniform `application/json` evidenced by the captured receipts — a documented inference, not a recorded value.
2. **Channel-typed `submission`.** The drive-doc `submission` member is `{"channel":"marker"}` for marker-convention submissions and `{"channel":"content","payload":<text>}` for content submissions. Equality is channel-plus-payload JSON equality: two marker submissions are equal by class (the minted `submission_ref` is an opaque id and is deliberately not compared); content submissions still compare payload bytes.
3. **Terminal-echo reconstruction (typed).** When a recording's final call proposes `echo <literal>` and the corpus records no observation for it, the raws queue gains `{output:"<literal>\n", returncode:0, reconstructed:"echo_literal"}`. `echo` output is the deterministic semantics of the command — reconstruction is *typed and declared*, not fabricated; the `reconstructed` member marks it in every emitted raw file. Without it the recorded corpus's own submit convention can never fire — an unanswerable precondition, not a parity claim.
4. **Corpus-boundary gating.** For a seq whose recording does not end in the submission marker (crash-truncated corpus), L1.1 gates **recorded-call traversal only**: `stable = |o_rec|`, `total = ∞`, so request-level diffs at recorded positions gate while `request_count`/`termination`/`submission`/`extra_request` are emitted ungated as boundary-typed diagnostics. For marker-terminated seqs the full trajectory gate applies unchanged. The verdict stays honest both ways: a truncated seq cannot earn `submitted`, and a marker seq cannot claim pass with a missing submission.

## Consequences

- Under `o_rec`, the Ws1r legs' 590 env-drift diffs leave the gated set: the strict proxy already proved byte-identity against the recording; the fold now measures the same claim instead of a live-vs-recording mismatch.
- The terminal-echo reconstruction plus the already-armed `stop_rule=submit{marker}` (present in the bound HIR) terminates marker seqs at request *N* with `termination:"submitted"` — no extra request, no exhaustion terminal.
- Corpus-02/26 fold as `corpus_boundary` seqs: gated on traversal fidelity, boundary diffs typed and ungated.
- `o_drive`/`h_drive` extraction artifacts (nested `info.*`, substring `finish`) are fixed as part of the same change so the retained live-vs-recording diagnostic reports real values.

## Alternatives considered

- **Keep the live l10 reference and normalize timestamps.** Rejected: timestamp-scrubbing one content member cannot make a live capture the replay's ground truth; the drift is in the environment, not the format.
- **Re-record the corpus.** Rejected for this pass: the retained corpus *is* the corpus O produced; reconstruction is confined to the deterministic terminal echo and is typed.
- **Detector-side marker check (match the command, not the output).** Rejected: `stop_rule` declares output-line detection; changing the contract to command-matching would weaken it for corpora where the marker must be produced by the environment.
- **Treat truncated seqs as fail.** Rejected: the recorded scope was traversed byte-identically; the tail beyond the recording is unanswerable, not failing. The seq reports `corpus_boundary`, not `pass` on submission evidence it cannot possess.

## Revisit trigger

Any corpus member with a complete (non-truncated) recording that cannot reach `submitted` under the armed stop rule, or a future corpus where submission carries a content payload — then the `channel:"content"` arm and payload equality become the gating comparison and this ADR's marker-channel normalization no longer suffices alone.
