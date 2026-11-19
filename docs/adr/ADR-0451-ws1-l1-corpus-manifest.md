# ADR-0451: Ws1 `hh-l1-corpus/1` manifest construction and `corpus_digest` semantics (R6.45q completion)

- **Status:** Accepted — recorded under the GATE-G29 Reading 2 disposition ("diagnose and fix any failures and iterate until success"); completes the corpus leg's fold tooling the Ws1/Ws1r windows exercised but never wired
- **Date:** 2026-11-18
- **Ticket:** R6.45q (Ws1 withheld-subject-legs window) — completion of the REQ-XD-107 corpus manifest for the G-L1 presentation
- **Requirement ids:** REQ-XD-107 (L1 corpus composition), REQ-XD-109 (`hh-l1-conformance/1` coverage table), REQ-XD-102 (pass id binding), REQ-XH-114 (natural corpus + partition)
- **Spec:** `hh-l1-corpus/1`, `hh-turn1-census/1`, `hh-l1-conformance/1`; §7.4 corpus legs
- **Rung (ADR-0024 obligation):** an explicit non-contract note — this ADR governs the experiments-program L1 corpus manifest and fold-input semantics, not the canonical kernel contract ladder.

## Context

Spec §7.4 (REQ-XD-107) defines the L1 corpus as three legs: the RECORD trajectories of the original (28 first-pass members over the 7 development tasks, partitioned 3/2/2 dev/gate-A/gate-B), the 19 authored synthetic branch fixtures covering BM1–BM19, and a turn-1 census of every manifest task. The gate criterion (§10.3, G-L1) presents `hh-l1-conformance/1` carrying "BM1–BM19 coverage complete, census complete, rule table frozen."

Both Ws1's original fold and the first ADR-0450 re-fold emitted `hh-l1-conformance/1` with `coverage.branches: {}`, `census_tasks: 0`, `corpus_digest: ""` and `h_identity.digest: ""`: the corpus leg never produced the `hh-l1-corpus/1` manifest the fold's `coverage`/`census_covered` inputs consume, the fold driver never emitted the corpus or frozen-H digests, and no binary surface invoked `check_corpus`. The conformance record therefore passed/failed on leg evidence while its identity and coverage members were vacuous — `l1_pass_id(h, canon, corpus_digest)` bound empty inputs, weakening REQ-XD-102's "pass for the exact H" binding.

W1 already executed the REQ-XD-107 turn-1 census at $0 (`hh-turn1-census/1`, 89 manifest tasks, every entry `responded: true`) — the first-pass corpus's census leg is complete as a record; the fold simply had no wiring to cite it.

## Decision

1. **Corpus manifest tooling.** `hh-w1` gains two subcommands:
   - `corpus-manifest --fixtures-dir D --recordings-dir D --census-tasks csv --out f` folds a fixtures dir and a recordings dir into an `hh-l1-corpus/1` doc: every member is enumerated with `provenance` (`synthetic` fixture / `record` trajectory) and a content `digest` (`sha256:` of the member's bytes-on-disk); fixture `branches` are copied verbatim from the authored `bm_fixtures()` generator, and a fixtures-dir member whose stem names no authored fixture **refuses** — the manifest never declares coverage it cannot attribute. `census_tasks` carries the manifest task ids.
   - `corpus-check --corpus f --tasks csv [--out f]` runs the committed `check_corpus` refusal verbatim: every BM1–BM19 branch covered by a declared entry, every manifest task covered by a census entry; emits the coverage map.
2. **`corpus_digest` = `sha256:` of the canonical manifest bytes.** Because every member's content digest is folded into the manifest, the manifest digest transitively binds every corpus byte: recording bytes, fixture bytes, and the census task set. The fold spec's `corpus_digest` adopts this value; `l1_pass_id` then binds the real corpus identity rather than `""`.
3. **Natural-corpus entries carry no declared `branches`.** The fixtures are the corpus's enumerated branch coverage (REQ-XD-107: "synthetic branch fixtures covering BM1–BM19"); record trajectories are corpus members whose incidental branch coverage is not declared — declared coverage stays attributable, never inferred.
4. **`h_id.digest` = the frozen H-set digest** (`sha256:c451ed97…`, REQ-XC-330 — the G-L1 named freeze). `o_id.digest` stays empty: the contract names no O digest; O's identity is its participant ref and `version:"original"`, and the recordings are bound inside the corpus manifest.
5. **`census_covered` = 7** (all manifest tasks), cited from W1's `hh-turn1-census/1` — census completeness is the executed census record covering the manifest tasks, not a re-execution.

## Consequences

- The gate-A `hh-l1-conformance/1` re-fold under the populated spec mints `pass_id: l1-pass:sha256:80bfafee…` bound to the exact frozen H digest and the corpus digest — REQ-XD-102's "matching pass id" is now reproducible from recorded values, and `native.rs`'s L2 scheduling check recomputes against the same digest.
- `coverage.branches` carries the complete BM1–BM19 map; `census_tasks: 7`; `canon_version: hh-l1-canon/1` (the frozen C01–C07 rule table) is the "rule table frozen" member.
- Earlier conformance records carrying vacuous coverage/digest members are superseded, not rewritten — they remain durable history of their fold inputs.

## Alternatives considered

- **Compute per-recording branch coverage.** Rejected: incidental natural coverage is real but undeclared in the contract; declaring it would attribute branches the corpus was not assembled to cover and adds an unvalidated analysis surface to a gate artifact.
- **`corpus_digest` = partition digest `f7ddbc1f…`.** Rejected: the partition commit binds task membership (REQ-XD-106), not corpus content; the manifest digest binds the content itself. Both are cited at the gate and neither substitutes for the other.
- **Re-run the census leg for the fold.** Rejected: W1's census already executed over the full 89-task manifest at $0; re-execution would produce the same `hh-turn1-census/1` shape with no new evidence.

## Revisit trigger

A later-pass corpus where branch coverage must be attributed to natural recordings (not authored fixtures), a corpus member whose on-disk bytes differ from its digested bytes, or a canon version bump — then the manifest members and digest must be re-emitted and the pass id re-minted on an unused partition (REQ-XD-106).
