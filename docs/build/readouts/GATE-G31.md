# GATE-G31 readout — First-pass publication
Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> Milestone gate. Pending operator review is RETURN PASS; a failed technical prerequisite is
> blockedOn. Agents never sign or infer approval. A decision here authorizes publication acts only —
> never spend, merges, or the dataset flip itself (HUMAN-H17 is a separate act).

## Criterion (verbatim)

Spec §10.3 gate table row "First-pass publication": FP-RESULTS + flagged review →
`hh-site-publication/1`; dataset visibility flip as a separate HUMAN act after both scans; terms read
(Q-H1-6). REQ-XH-105: the gate records verbatim the transcript option (Q-X43), the policy for files
with task-derived content, and that the operator has read the Hugging Face and Ollama terms.
REQ-XH-018 / REQ-XF-129: a phase's pages appear in the public profile only when a committed,
append-only `hh-site-publication/1` record pins that phase's exact input set, referencing the
operator's verbatim decision in this readout.

## Evidence (per item: source, `date -u` of the read, layer reached)

- Frozen packets (R6.49, `freeze.state=="frozen"`, `.sha256` pinned):
  `p2-fp` a8485bd8… `p3-l2` 01db3d7f… `p3-l3` 9869c904… `l2-audit` eb7ac3a1… `p4-fp` 5c425c1a…
  — read 2026-11-19T03:45Z — layer: `engineered` (committed, byte-identical to R6.48 except
  `{freeze,review}` members — r6_49 5/5).
- `hh-flagged-review/1` `2026-11-19-fp-flagged-review.json` sha256 `b4637868…` — HUMAN-H16 verbatim:
  35 cleared / 0 confirmed / "the confirmed count is a minimum" — `engineered`.
- `hh-fp-answer-scan-report/1` f836343a… (53/53 scanned, flagged-pass 0) + contamination scan
  `2026-11-19-fp-contamination-scan.json` sha256 `421399fd…` verdict `clean` (seeded controls flag
  correctly) — both scans exist at presentation — `engineered`.
- Dataset card `2026-11-19-fp-dataset-card.md` sha256 `50c417b3…` — canary + Apache-2.0 NOTICE —
  `engineered` (upload/flip = HUMAN-H17, separate act).
- Site pages render phase-3/4 with real withheld structure + per-member provenance; check-site
  15/15 internal & public profiles; nothing public — `engineered`.

## Decision content for the operator — your verbatim on items 1–4

1. **Scope** — `PASSED — publish` (as presented, or narrower verbatim) · `PASSED — publication
   withheld` · `refuse` → NOT-PASSABLE. The presented scope: phase-3/4 first-pass pages pinned to the
   five frozen packets above; every outcome a typed withhold verbatim (`window_deadline`,
   `budget`, `admission:AG3`, `protected_pair_truncated{k<5,R<5}`) — nothing landed to claim.
2. **Transcript option (Q-X43)** — state verbatim: **(a) declared in-site fetch**, or **(b) links
   only**.
3. **Policy for files with task-derived content** — state verbatim.
4. **Terms read (Q-H1-6)** — your statement that you have read the Hugging Face and Ollama terms
   (no note studied them for automated benchmarking or publishing TB-derived transcripts).

The dataset flip is **not** decided here — HUMAN-H17 is the separate act after both scans (both
exist: answer-scan `f836343a…`, contamination `421399fd…` `clean`; card `50c417b3…` staged).
Labels: first-pass + REQ-XH-122/REQ-XD-126 baseline wording + withheld/inconclusive at equal
prominence (REQ-XH-020) are already on the frozen pages.

<!-- agent-drafted:begin sha256=c907c69c0ff2347428b47d91574bfd31ab06be831f61fec6a09449c91052f891 -->
(decision content above)
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
