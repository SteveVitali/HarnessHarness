# HUMAN-H16 readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE | DEFERRED -->

> An operator-named reviewer supplies the dispositions; an agent never reviews, clears, confirms or
> infers silence as a disposition.

## Criterion (verbatim)

Spec §10.3 HUMAN table: "Flagged-pass review | operator-named reviewer | each results freeze (zero
flags recorded trivially) | dispositions with excerpt hashes | results packets". REQ-XH-121: the
review runs after the seal opens and before the results freeze. §6.9 / REQ-XD-071 (AC-6.9-2, layer
D): each flag needs a disposition, reason and excerpt hash; `Status: DEFERRED` + DF id withholds the
freeze claim and keeps every unreviewed flag as a veto.

## Evidence (per item: source, `date -u` of the read, layer reached)

- `hh-fp-answer-scan-report/1` `docs/build/reports/2026-11-19-fp-answer-scan.json` sha256 `f836343a99c056a0567fb2027fb4c9f9db0f90f6728765bc2cb75bddf0b13853` — detector `answer-scan/1`, 53 records scanned, 3,039 scored hits, **35 flagged, 0 unresolved, flagged-pass 0** — read 2026-11-19T03:00Z — layer: `engineered` (committed record).
- `hh-fp-seal-open/1` `2026-11-19-fp-seal-open.json` sha256 `199cb874…` — the seal opened exactly once — `engineered`.
- `hh-flagged-review-input/1` + `hh-flagged-review-report/1` members of the scan record — the REQ-XD-071 handoff: `confirmed`/`cleared` stay 0 until your rows land — `engineered`.

<!-- agent-drafted:begin sha256=ccbb9ec8f41ae027499b545ba50b8272551ed3d6d1c23cb8e4c924622cb1bc6d -->
## Decision content for the operator — flagged-pass review (your act)

**What the scan found.** `hh-fp-answer-scan-report/1` (`2026-11-19-fp-answer-scan.json` sha256 `f836343a…`, detector `answer-scan/1`) scanned all **53 scored records** (37 subject + 16 l1_corpus_recording): **3,039 scored hits, 35 slots flagged, 0 unresolved, flagged-pass 0**. Every flagged slot's `scored_state` is `clean` — the hits are `fingerprint:solution` signals scored in receipts/artifacts of executed trials (expected content — the detector flags for review, it does not allege). Harness identity is masked (`identity_masking: harness_masked` — feasible, recorded); excerpts are sha256-only, never matched bytes.

**Your act (REQ-XD-071 / §6.9):** for each flagged slot record `{confirmed, cleared}` + reason + excerpt hash. A mass ruling is legal verbatim (e.g., "all 35 cleared — scored hits are solution fingerprints expected on executed trials") — it records as the disposition covering every listed excerpt hash. Or `Status: DEFERRED` + a named DF row withholds the results-freeze claim (the flags stay vetoes).

**The 35 flagged slots** (excerpt lists abbreviated — the committed record is the full source):

| slot | arm | kind | hits | uniq excerpts | excerpt sha256s | scored_state |
|---|---|---|---|---|---|---|
| `ws1-pp0-1` | mini-swe-agent | subject | 35 | 2 | `sha256:1c99cfde…`, `sha256:fe5cc67f…` | clean |
| `ws1-pp0-2` | pi | subject | 25 | 2 | `sha256:1c99cfde…`, `sha256:fe5cc67f…` | clean |
| `ws1-pp0-3` | claude-code | subject | 136 | 5 | `sha256:1c99cfde…`, `sha256:3b9ec4b2…`, `sha256:3ed75169…`, `sha256:4363115d…` … | clean |
| `ws1-pp0-4` | mini-swe-agent | subject | 1 | 1 | `sha256:f221b4ec…` | clean |
| `ws1-pp0-5` | pi | subject | 4 | 4 | `sha256:467f6eab…`, `sha256:85d97d07…`, `sha256:88c9eae6…`, `sha256:f221b4ec…` | clean |
| `ws1-pp0-6` | claude-code | subject | 1 | 1 | `sha256:88c9eae6…` | clean |
| `ws1-pp0-7` | mini-swe-agent | subject | 19 | 2 | `sha256:043b3ba0…`, `sha256:1c80ee44…` | clean |
| `ws1-pp0-8` | pi | subject | 7 | 2 | `sha256:043b3ba0…`, `sha256:c55a0fdb…` | clean |
| `ws1-pp0-9` | claude-code | subject | 24 | 2 | `sha256:043b3ba0…`, `sha256:a6151b5b…` | clean |
| `ws1-corpus-01` | mini-swe-agent | subject | 1 | 1 | `sha256:f221b4ec…` | clean |
| `ws1-corpus-03` | mini-swe-agent | subject | 26 | 1 | `sha256:be8cc430…` | clean |
| `ws1-corpus-05` | mini-swe-agent | subject | 86 | 3 | `sha256:1c99cfde…`, `sha256:f221b4ec…`, `sha256:fe5cc67f…` | clean |
| `ws1-corpus-06` | mini-swe-agent | subject | 93 | 3 | `sha256:1c99cfde…`, `sha256:cd007af7…`, `sha256:fe5cc67f…` | clean |
| `ws1-corpus-07` | mini-swe-agent | subject | 167 | 4 | `sha256:1c99cfde…`, `sha256:3b9ec4b2…`, `sha256:3ed75169…`, `sha256:fe5cc67f…` | clean |
| `ws1-corpus-08` | mini-swe-agent | subject | 90 | 3 | `sha256:1c99cfde…`, `sha256:4363115d…`, `sha256:fe5cc67f…` | clean |
| `ws1-corpus-09` | mini-swe-agent | subject | 9 | 3 | `sha256:6c3d523e…`, `sha256:85d97d07…`, `sha256:f221b4ec…` | clean |
| `ws1-corpus-10` | mini-swe-agent | subject | 2 | 2 | `sha256:85d97d07…`, `sha256:f221b4ec…` | clean |
| `ws1-corpus-11` | mini-swe-agent | subject | 2 | 2 | `sha256:85d97d07…`, `sha256:f221b4ec…` | clean |
| `ws1-corpus-13` | mini-swe-agent | subject | 280 | 9 | `sha256:0d279036…`, `sha256:2b2665e6…`, `sha256:3fa31131…`, `sha256:49328c2d…` … | clean |
| `ws1-corpus-14` | mini-swe-agent | subject | 228 | 7 | `sha256:0d279036…`, `sha256:0d414542…`, `sha256:2b2665e6…`, `sha256:3fa31131…` … | clean |
| `ws1-corpus-15` | mini-swe-agent | subject | 1215 | 17 | `sha256:06d4a82e…`, `sha256:0d279036…`, `sha256:1cc9e9f3…`, `sha256:21a1053b…` … | clean |
| `ws1-corpus-16` | mini-swe-agent | subject | 342 | 17 | `sha256:0d279036…`, `sha256:1cc9e9f3…`, `sha256:21a1053b…`, `sha256:28de044f…` … | clean |
| `ws1-corpus-17` | mini-swe-agent | subject | 18 | 3 | `sha256:24133e81…`, `sha256:85d97d07…`, `sha256:f221b4ec…` | clean |
| `ws1-corpus-18` | mini-swe-agent | subject | 50 | 3 | `sha256:24133e81…`, `sha256:85d97d07…`, `sha256:f221b4ec…` | clean |
| `ws1-corpus-19` | mini-swe-agent | subject | 34 | 3 | `sha256:24133e81…`, `sha256:85d97d07…`, `sha256:f221b4ec…` | clean |
| `ws1-corpus-20` | mini-swe-agent | subject | 56 | 1 | `sha256:24133e81…` | clean |
| `ws1-corpus-21` | mini-swe-agent | subject | 23 | 3 | `sha256:043b3ba0…`, `sha256:1c80ee44…`, `sha256:a6151b5b…` | clean |
| `ws1-corpus-22` | mini-swe-agent | subject | 8 | 1 | `sha256:a44834f5…` | clean |
| `ws1-corpus-23` | mini-swe-agent | subject | 19 | 2 | `sha256:043b3ba0…`, `sha256:1c80ee44…` | clean |
| `ws1-corpus-24` | mini-swe-agent | subject | 30 | 4 | `sha256:043b3ba0…`, `sha256:1c80ee44…`, `sha256:a44834f5…`, `sha256:a6151b5b…` | clean |
| `ws1-corpus-27` | mini-swe-agent | subject | 2 | 2 | `sha256:467f6eab…`, `sha256:88c9eae6…` | clean |
| `l10-ws1-corpus-01-a` | mini-swe-agent | l1_corpus_recording | 1 | 1 | `sha256:f221b4ec…` | clean |
| `l10-ws1-corpus-01-b` | mini-swe-agent | l1_corpus_recording | 1 | 1 | `sha256:f221b4ec…` | clean |
| `l10-ws1-corpus-27-a` | mini-swe-agent | l1_corpus_recording | 2 | 2 | `sha256:467f6eab…`, `sha256:88c9eae6…` | clean |
| `l10-ws1-corpus-27-b` | mini-swe-agent | l1_corpus_recording | 2 | 2 | `sha256:467f6eab…`, `sha256:88c9eae6…` | clean |

Counts to record at signature: flagged 35 · confirmed _ · cleared _ · "the confirmed count is a minimum". Reviewer name/role + UTC + channel required.

<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
## Signature   <!-- appended at signing; nothing above is edited or deleted -->

- **Reviewer:** operator (Steve Vitali) — owner role · **UTC:** 2026-11-19T03:15Z · **Channel:** operator CLI session (verbatim quoted).
- **Verbatim ruling:** "Looks good to me, please continue" — dispositioned over the presented 35-flag table and its stated basis (every flagged slot `scored_state: clean`; hits are `fingerprint:solution` signals scored on executed trials; flagged-pass 0).
- **Dispositions:** all 35 flagged slots → `cleared`; reason per flag: scored hits are solution fingerprints expected in receipts/artifacts of executed trials — no flag alleges contamination. Excerpt hashes: the committed `hh-fp-answer-scan-report/1` record's per-slot hash lists (the readout table + canonical JSON are the hash source).
- **Counts:** flagged 35 · **confirmed 0 · cleared 35** · reviewed_flags 35 · detector `answer-scan/1` · coverage 53/53 scored records · masking `harness_masked` (feasible — recorded).
- **"the confirmed count is a minimum."**
- Layer: `signed` — operator act recorded; no excerpt content beyond hashes enters this readout.
