# Round 6 — REQ-X coverage register (exact supplemental ownership)

**Status:** decomposition draft, revised after the Phase-4 split review (SR-1…SR-27; see the [plan's review record](2026-11-07_round6-decomposition.md#phase-4-review-record-2026-11-07)). It carries no implementation, research, participant or publication evidence. Written 2026-11-07T03:42:36Z with the [partition plan](2026-11-07_round6-decomposition.md), revised 2026-11-07T04:19:14Z. Supplement: [`docs/experiments-spec.md`](../../experiments-spec.md), canonical v0.3, sha256 `0bcfb3572dcc0bc921014f265ee34876f2f74bad7b6cd685abe214ebcf78362e`; pattern `REQ-X[A-H]-[0-9]+`. The id source is spec Appendix A (REQ-XH-003). The machine-readable companion is [`2026-11-07_round6-coverage.json`](2026-11-07_round6-coverage.json) (schema `hh-supplement-plan/1`: the round-5 precedent's fields with the values its retained checks recompute, plus the additive fields below).

There are 456 candidates: 370 adopted (338 source + 32 synthesis-allocated), 81 merged-with and 5 superseded-by aliases. Each adopted id has exactly one primary owner row. Each alias inherits its target's owner, and the target keeps the alias's literal predicate bytes (`literal_row`, verbatim from its source note). Scope: 336 adopted ids are first-pass; 34 are routed (22 to Round 7 and 12 to Round 8), with CAP6.1 owning their routing record before GATE-ACCEPT and REC6.2 re-reading it live. By owner kind: ticket 319, capstone 36 (CAP6.1 34, CAP6.3 2), skeleton 9, reconcile 3 (REC6.1 2, REC6.2 1), gate 2, human 1.

## Register shape (what R6.1 consumes; SR-3 option (a))

Every candidate row carries every field below, with no null or empty-string value (`contributors` is a list; empty means "primary only").

| field | content | check it serves |
|---|---|---|
| `literal_row` | verbatim source-row bytes: the research/design note row for the 424 source ids; the spec's own normative `\| **REQ-…** \|` row for the 32 synthesis-allocated ids (`literal_source: docs/experiments-spec.md`) | alias-predicate "literal row present" |
| `source_sha256` | `sha256(literal_row)`, per row | alias-predicate "literal_row sha256 matches" (the retained guard's recompute) |
| `literal_source`, `literal_source_sha256` | the source file and its whole-file digest | frozen-source provenance; verbatim-in-source check |
| `requirement`, `acceptance` | adopted: the spec's normative and acceptance cells; alias: its own literal predicate (and literal acceptance, else the target's, labelled) | "requirement text is empty" |
| `appendix_a_row`, `appendix_a_row_sha256` | the verbatim Appendix A line | id source (the frozen spec has no per-id anchors) |
| `target_spec_row_sha256` | sha256 of the target's normative row in the spec | per-row digest of the frozen predicate |
| `spec_anchor` | an existing spec `<a id>`: `s0`…`s10`, `s0-4`, `s10-1` (every §10.1.x → `s10-1`) | § hook anchors |
| `ac_hook`, `ac_hook_resolution`, `hooks` | the verbatim Appendix A hook (`AC-<§ path>-<n>`), resolved `appendix-a-row`: by (id, hook) equality with the pinned Appendix A row, and present in the target's acceptance cell | AC-hook anchors (pattern widened to `AC-\d+(\.\d+)*-\d+` by R6.1) |
| `primary`, `primary_kind`, `primary_file`, `acceptance_anchor` | one chain row; its contract file; `ac-<chain id lowercased, dots → dashes>-<target id lowercased>`, an `<a id>` the Phase-3 contract writer places at the numbered AC | owner, ac-anchor |
| `contributors`, `round_scope`, `status` | owed-evidence rows (never dispatch dependencies); `first-pass` or `routed(round-7\|round-8)`; plan status | owner, routing |

R6.1's scope parametrizes the retained guard's per-source constants (coverage file, `REQ-X[A-H]-[0-9]+` pattern, 456/370/81/5 counts), resolves ids from Appendix A rather than `id="req-…"` anchors, and resolves AC hooks through `ac_hook_resolution`; it reuses the retained checks, never a second checker (CC1).

## Completion and owner rules

A row owns a bounded deliverable. A requirement is satisfied only when **all** its predicates, its aliases' literal predicates included, have evidence at the required layer (D/E/R/P, §0.2). An E pass is never an R or P pass, and an unexecuted study is unmet, not null.

- **O1 (default):** the primary owner is the row that produces the deliverable the requirement's acceptance hook inspects (code, schema, document, packet). Where a requirement spans a seam, the owner is the seam row that completes it, and the earlier row is a contributor (REQ-XC-330 → R6.34c with R6.34b contributing; REQ-XB-104 → R6.24c with R6.15b contributing).
- **O2 (skeleton owners):** an R- or P-layer id whose inspected record *is* a paid or provisioning window's own record is owned by that window's skeleton row, and is **owed at fleshing**. These are the W1 re-proof packet (REQ-XB-125, REQ-XH-116), the W2 stage packet (REQ-XD-041, REQ-XH-010) and the W3 L1 gate-run packet (REQ-XD-107, REQ-XE-006…009). Fleshing rewrites the skeleton row in place (kind skeleton → ticket, same id), so the ownership carries over unchanged. No D- or E-layer id is owned by a skeleton.
- **O3 (marker owners):** a gate or HUMAN row owns an id only when the id is literally a gate-readout or HUMAN obligation. GATE-G26 FP-S owns REQ-XH-007 (credit-period readout fields) and REQ-XH-013 (exclusivity acknowledgement at each paid gate), with GATE-G30 as contributor. HUMAN-H23 owns REQ-XD-071 ("a HUMAN row SHALL review every flagged pass").
- **O4 (later-pass scope):** an adopted id with no Round-6 implementing obligation (§10.6, §10.1.6, §1.2) is owned by **CAP6.1**, which writes its later-pass routing record (target round, trigger gate, withheld claim) before GATE-G32 signs and creates no implementing row (REQ-XH-001). REC6.2 re-reads the register live.
- **O5 (contributors):** each contributor row names evidence the owner relies on, such as live legs, window records or downstream packets. A contributor never becomes a dispatch dependency, and the owner's AC names exactly what is owed. Contract writers create the anchor `ac-<chain id, dots → dashes>-<req id>` in the owner contract for every owned id, as the literal owner table links it.

## Machine check

The register was checked by an independent script (v2, improved after the review's mutation probes, SR-23). It reads only the spec (Appendix A, normative rows, `<a id>` anchors), the literal sources the JSON cites, the JSON, this file's literal owner table and owner index, and the decomposition's row table, Totals table and shared-decision table; it never reads the generator's data, and every check is isolated, so a missing id or malformed field prints a FAIL line instead of a traceback. The script is committed byte-identically at [`docs/build/tools/check_round6_coverage.py`](../tools/check_round6_coverage.py) (sha256 `c08de6b6200e06b3f74644655b5fdc4b3d406663ebdd88b5a1d7d880e57b2694`; coordinator moved it from the session scratchpad at decomposition so the record stays reproducible); R6.1 folds it into the guard's legs (REQ-XH-003). It was run as `python3 -I check_round6_coverage.py /Users/stevenvitali/MetaHarness-harnessharness-experiments`:

```text
PASS  spec sha256 equals register supplement_sha256
PASS  Appendix A: 456 candidates, 370 adopted; every adopted id has one normative row
PASS  row table parses (13 cells per row)
PASS  sequences contiguous from 252
PASS  Totals table total equals the row-table count
PASS  Totals table kind counts equal the row table
PASS  chain ids match guard grammar; filenames open with seq and id; kinds in the closed set
PASS  depends-on: "nothing" or chain rows only (never "—"), backward only
PASS  only the bootstrap gate depends on nothing
PASS  tail=full contiguous at end; its gate filename contains "accept"
PASS  row 1 bootstrap gate; rows 2–3 are the guard rows (R6.1, R6.1a), before any product row
PASS  five window skeletons, each depending on a preceding gate, no size/Run budget
PASS  HUMAN rows sit only before a gate, the tail, or a row that depends on them (SR-15/16)
PASS  every sized row ≤ 85k including the 10k reserve
PASS  json candidate ids unique and == Appendix A
PASS  json dispositions/layers/sections match Appendix A
PASS  json: no null or empty-string field in any candidate
PASS  json: literal_row verbatim in literal_source; source_sha256 = sha256(literal_row); file digest pinned (all 456)
PASS  json: synthesis-allocated ids carry the spec's own row as literal_row
PASS  json: adopted requirement/acceptance = spec cells; alias requirement drawn from its literal row
PASS  json: anchors explicit — spec_anchor exists, ac_hook = Appendix A hook (in the normative row), row digests match
PASS  every adopted id owned exactly once (as its own target)
PASS  every owner is a table row; primary_file, primary_kind and acceptance_anchor agree with it
PASS  aliases: target is adopted; alias inherits target owner
PASS  row-table owned ids == JSON primaries row for row (no duplicate, no dropped owner)
PASS  owner-kind rule: gate/human only literal gate/HUMAN obligations; skeleton only R/P window evidence
PASS  routed ids: owned by CAP6.1 only (before GATE-ACCEPT); first-pass ids never by CAP6.1
PASS  contributors are table rows, never the primary
PASS  md literal owner table: all 456 ids, one contract + anchor each, equal to json
PASS  md owner index agrees with json row for row
PASS  shared-decision table: each owner is a row that precedes every consumer row it names
RESULT: PASS (31/31)
```

Negative controls (temporary copies under the scratchpad, same script; each mutation must FAIL):

```text
# mutation 'row-table duplicate owner (XA-101 also listed under R6.3)' -> exit 1; RESULT: FAIL (30/31); first FAIL: row-table owned ids == JSON primaries row for row (no duplicate, no dropped owner)
# mutation 'row-table owner dropped (R6.13 cell emptied)' -> exit 1; RESULT: FAIL (30/31); first FAIL: row-table owned ids == JSON primaries row for row (no duplicate, no dropped owner)
# mutation 'json candidate REQ-XB-128 removed' -> exit 1; RESULT: FAIL (25/31); first FAIL: json candidate ids unique and == Appendix A
# mutation 'forward edge R6.2 → R6.6' -> exit 1; RESULT: FAIL (30/31); first FAIL: depends-on: "nothing" or chain rows only (never "—"), backward only
# mutation 'bootstrap "Depends on" written as —' -> exit 1; RESULT: FAIL (29/31); first FAIL: depends-on: "nothing" or chain rows only (never "—"), backward only
# mutation 'one source_sha256 altered' -> exit 1; RESULT: FAIL (30/31); first FAIL: json: literal_row verbatim in literal_source; source_sha256 = sha256(literal_row); file digest pinned (all 456)
# mutation 'one literal_row byte changed' -> exit 1; RESULT: FAIL (29/31); first FAIL: json: literal_row verbatim in literal_source; source_sha256 = sha256(literal_row); file digest pinned (all 456)
# mutation 'one alias acceptance set to null' -> exit 1; RESULT: FAIL (30/31); first FAIL: json: no null or empty-string field in any candidate
# mutation 'one ac_hook changed' -> exit 1; RESULT: FAIL (30/31); first FAIL: json: anchors explicit — spec_anchor exists, ac_hook = Appendix A hook (in the normative row), row digests match
# mutation 'HUMAN-H23 no longer needed by R6.77' -> exit 1; RESULT: FAIL (30/31); first FAIL: HUMAN rows sit only before a gate, the tail, or a row that depends on them (SR-15/16)
# mutation 'shared decision owner after its consumer (hh-l1-canon → R6.43)' -> exit 1; RESULT: FAIL (30/31); first FAIL: shared-decision table: each owner is a row that precedes every consumer row it names
# mutation 'md literal owner table row deleted' -> exit 1; RESULT: FAIL (30/31); first FAIL: md literal owner table: all 456 ids, one contract + anchor each, equal to json
# mutation 'Totals table total off by one' -> exit 1; RESULT: FAIL (30/31); first FAIL: Totals table total equals the row-table count
```

**Retained-guard compatibility (SR-3).** The retained `Guard.check_candidate` (`docs/build/tools/supplement_guard.py` at `185c16c7`) was run in-process over all 456 candidates, with the plan's row ids as `manifest_ids` and the spec's anchors: 0 `alias-predicate`, `disposition`, `layer` or `ac-anchor` errors; the only errors are 456 × `owner: primary_file … missing`, because the Phase-3 contract files do not exist yet. Its `REQ-[A-H]` alias regex and `AC-\d+-\d+` hook regex do not match the REQ-X spellings, so those two checks are skipped rather than passed; R6.1 widens them.

The generator's in-process battery agreed (PASS (19/19)): ownership totals, grammar, backward edges, tail, skeleton gates, the 85k ceiling, the HUMAN placement rule, the linearization-B permutation (backward-only) and the JSON shape checks. GUARD-X (R6.1, R6.1a) turns these into retained `hh-supplement-guard/1` legs (REQ-XH-002, REQ-XH-003). R6.1's contract also carries the pre-existing 46 legacy `reqcov` `unexplained-diagnostic` warnings as an OPEN FINDING, classified into the reviewed baseline with dated provenance, never silently (hand-off §4; coordinator ruling on SR-2).

## Owner index (row → adopted ids)

| owner | kind | alias | adopted ids owned |
|---|---|---|---|
| [R6.1](../../tickets/253_R6.1__guard-x.md) | ticket | GUARD-X (a) | 3: XH-001, XH-002, XH-003 |
| [R6.1a](../../tickets/254_R6.1a__guard-rules.md) | ticket | GUARD-X (b) | 5: XH-004, XH-008, XH-024, XH-025, XH-026 |
| [R6.2](../../tickets/255_R6.2__p1-provenance.md) | ticket | P1-A | 6: XA-101, XA-102, XA-103, XA-104, XA-105, XA-106 |
| [R6.3](../../tickets/256_R6.3__p1-hardening.md) | ticket | P1-B | 9: XA-006, XA-107, XA-108, XA-109, XA-110, XA-111, XA-112, XA-113, XH-032 |
| [R6.4](../../tickets/257_R6.4__p1-corrections.md) | ticket | P1-C + P1-D | 6: XA-114, XA-115, XA-116, XA-117, XB-222, XH-028 |
| [R6.6](../../tickets/258_R6.6__p1-record.md) | ticket | P1-E | 10: XA-008, XA-118, XA-119, XA-120, XA-121, XA-122, XA-123, XA-124, XA-125, XH-019 |
| [R6.7](../../tickets/259_R6.7__site-shell.md) | ticket | F2-a | 8: XF-101, XF-102, XF-103, XF-104, XF-105, XF-106, XF-107, XF-114 |
| [R6.8](../../tickets/261_R6.8__site-generator.md) | ticket | F2-b | 9: XF-021, XF-117, XF-118, XF-119, XF-120, XF-121, XF-122, XF-125, XF-139 |
| [R6.9](../../tickets/262_R6.9__site-phase1.md) | ticket | F2-e | 5: XF-108, XF-109, XF-116, XF-141, XH-020 |
| [R6.10](../../tickets/263_R6.10__claims-checker.md) | ticket | F2-f | 11: XF-009, XF-126, XF-127, XF-134, XF-135, XF-137, XF-138, XH-022, XD-003, XD-092, XC-014 |
| [R6.11](../../tickets/264_R6.11__pages-workflow.md) | ticket | F2-g | 7: XF-128, XF-129, XF-130, XF-140, XF-142, XH-018, XC-120 |
| [R6.13](../../tickets/265_R6.13__price.md) | ticket | PRICE | 1: XC-116 |
| [R6.14](../../tickets/266_R6.14__lane.md) | ticket | LANE | 1: XC-104 |
| [R6.15a](../../tickets/267_R6.15a__runner-core.md) | ticket | RUNNER-CORE (a) | 5: XB-101, XB-102, XB-103, XB-108, XB-120 |
| [R6.15b](../../tickets/268_R6.15b__runner-topology.md) | ticket | RUNNER-CORE (b) | 1: XB-105 |
| [R6.15c](../../tickets/269_R6.15c__runner-schedule.md) | ticket | RUNNER-CORE (c) | 9: XB-121, XB-127, XB-128, XB-214, XD-060, XD-111, XH-016, XH-017, XH-125 |
| [R6.16](../../tickets/270_R6.16__cell-spec.md) | ticket | CELL-SPEC | 3: XB-106, XB-107, XB-117 |
| [R6.17](../../tickets/271_R6.17__deadline.md) | ticket | DEADLINE | 2: XB-118, XB-119 |
| [R6.18](../../tickets/272_R6.18__select.md) | ticket | SELECT | 3: XB-122, XD-011, XH-012 |
| [R6.19a](../../tickets/273_R6.19a__recipe-provision.md) | ticket | RECIPE-a | 10: XB-201, XB-202, XB-203, XB-204, XB-205, XB-206, XB-208, XB-209, XB-212, XB-223 |
| [R6.19b](../../tickets/274_R6.19b__recipe-teardown.md) | ticket | RECIPE-b | 8: XB-207, XB-210, XB-211, XB-219, XB-220, XB-224, XH-009, XH-023 |
| [R6.20a](../../tickets/275_R6.20a__proxy-core.md) | ticket | PROXY-CORE-a | 9: XC-007, XC-101, XC-103, XC-105, XC-106, XC-107, XC-108, XC-109, XC-208 |
| [R6.20b](../../tickets/276_R6.20b__proxy-release.md) | ticket | PROXY-CORE-b | 6: XC-110, XC-111, XC-119, XC-123, XC-124, XC-322 |
| [R6.21](../../tickets/277_R6.21__budget.md) | ticket | BUDGET | 9: XC-004, XC-112, XC-113, XC-114, XC-115, XC-126, XC-210, XH-005, XH-120 |
| [R6.41](../../tickets/278_R6.41__l1-canon.md) | ticket | K1 | 1: XD-105 |
| [R6.22](../../tickets/279_R6.22__replay.md) | ticket | REPLAY | 2: XC-122, XC-125 |
| [R6.23](../../tickets/280_R6.23__verdict.md) | ticket | VERDICT | 5: XB-109, XB-110, XB-111, XB-112, XA-127 |
| [R6.24a](../../tickets/281_R6.24a__egress-profile.md) | ticket | EGRESS (a; absorbs C2 EGRESS) | 3: XB-113, XB-115, XB-129 |
| [R6.24b](../../tickets/282_R6.24b__egress-mediator.md) | ticket | EGRESS (b) | 2: XB-114, XB-116 |
| [R6.24c](../../tickets/283_R6.24c__runner-supervision.md) | ticket | RUNNER-SUPERVISION (seam row) | 1: XB-104 |
| [R6.25](../../tickets/284_R6.25__cost-record.md) | ticket | COST-RECORD | 6: XB-213, XB-215, XB-216, XB-217, XH-006, XH-118 |
| [R6.26](../../tickets/285_R6.26__reconcile.md) | ticket | RECONCILE | 5: XC-006, XC-117, XC-118, XC-121, XC-127 |
| [R6.27](../../tickets/286_R6.27__harbor-o.md) | ticket | HARBOR-O | 5: XB-123, XB-124, XC-306, XC-315, XH-014 |
| [R6.28](../../tickets/287_R6.28__reproof-kit.md) | ticket | REPROOF machinery (B2 #8 code half) | 2: XB-126, XH-027 |
| [R6.29](../../tickets/288_R6.29__comparative-protocol.md) | ticket | D1-a | 8: XC-010, XD-002, XD-004, XD-020, XD-021, XD-050, XD-051, XH-030 |
| [R6.30a](../../tickets/289_R6.30a__estimators.md) | ticket | D1-b (a) | 6: XD-001, XD-022, XD-030, XD-031, XD-061, XF-123 |
| [R6.30b](../../tickets/290_R6.30b__method-dossier.md) | ticket | D1-b (b) | 2: XD-023, XD-024 |
| [R6.31a](../../tickets/292_R6.31a__archive-index.md) | ticket | ARCHIVE (a) | 2: XF-124, XH-101 |
| [R6.31b](../../tickets/293_R6.31b__archive-upload-kit.md) | ticket | ARCHIVE (b) | 4: XH-102, XH-103, XH-104, XH-107 |
| [R6.32a](../../tickets/294_R6.32a__site-leaderboard.md) | ticket | F2-c (a) | 3: XF-110, XF-111, XF-115 |
| [R6.32b](../../tickets/295_R6.32b__site-noise-quote.md) | ticket | F2-c (b) | 2: XF-112, XF-113 |
| [R6.33](../../tickets/296_R6.33__site-grid-viewer.md) | ticket | F2-d | 3: XF-131, XF-132, XF-133 |
| [R6.34a](../../tickets/297_R6.34a__admit-record.md) | ticket | ADMIT-CORE (a) | 5: XC-202, XC-214, XC-301, XC-302, XD-081 |
| [R6.34b](../../tickets/298_R6.34b__admit-build.md) | ticket | ADMIT-CORE (b) | 3: XC-303, XC-304, XC-305 |
| [R6.34c](../../tickets/299_R6.34c__binary-sets.md) | ticket | BINARY-SETS (seam row) | 1: XC-330 |
| [R6.35](../../tickets/300_R6.35__adapt-chat.md) | ticket | ADAPT-CHAT-PY + ADAPT-CHAT-NODE (first-pass harnesses) | 8: XC-307, XC-308, XC-309, XC-310, XC-320, XE-001, XD-082, XD-121 |
| [R6.36](../../tickets/301_R6.36__adapt-messages.md) | ticket | ADAPT-MESSAGES | 1: XC-327 |
| [R6.37](../../tickets/302_R6.37__capture.md) | ticket | CAPTURE | 4: XC-207, XC-317, XC-318, XC-328 |
| [R6.38](../../tickets/303_R6.38__smoke-battery.md) | ticket | SMOKE-BATTERY | 7: XC-203, XC-311, XC-312, XC-321, XC-323, XC-324, XC-329 |
| [R6.39](../../tickets/305_R6.39__runner-check.md) | ticket | RUNNER-CHECK | 3: XC-325, XC-326, XH-031 |
| [R6.40](../../tickets/306_R6.40__answer-scan.md) | ticket | D1-c | 2: XD-070, XC-313 |
| [R6.42](../../tickets/307_R6.42__l1-replay-corpus.md) | ticket | K2 + K3 | 1: XE-004 |
| [R6.43](../../tickets/308_R6.43__l1-runner.md) | ticket | K4 | 3: XD-103, XD-104, XD-109 |
| [R6.44](../../tickets/309_R6.44__fp-continuation.md) | ticket | FP runner rules (REQ-XH-112/117/126) | 5: XH-112, XH-117, XH-126, XD-040, XD-042 |
| [R6.45](../../tickets/310_R6.45__fp-holds.md) | ticket | FP holds (REQ-XH-123/124/127) | 3: XH-123, XH-124, XH-127 |
| [R6.46](../../tickets/311_R6.46__fp-seal.md) | ticket | FP seal (REQ-XH-121) | 1: XH-121 |
| [R6.47](../../tickets/312_R6.47__fp-select.md) | ticket | SELECT commit (D1-d part 1) | 3: XD-010, XD-012, XD-014 |
| [R6.12](../../tickets/315_R6.12__pub-p1-record.md) | ticket | PUB-P1 | 1: XF-136 |
| [R6.48](../../tickets/318_R6.48__w1-run.md) | skeleton | W1{PROV … TEARDOWN} (REPROOF, HOST-SMOKE, CENSUS) | 2: XB-125, XH-116 |
| [R6.58](../../tickets/319_R6.58__fp-prereg.md) | ticket | FP-PREREG (D1-d part 2) | 4: XH-111, XH-113, XH-115, XD-115 |
| [R6.50](../../tickets/321_R6.50__wire-fidelity.md) | ticket | E2-T1 | 2: XE-107, XE-108 |
| [R6.51](../../tickets/322_R6.51__hh-tmpl.md) | ticket | E2-T3 | 2: XE-112, XE-113 |
| [R6.52](../../tickets/323_R6.52__compiled-surface.md) | ticket | E2-T2 (a) | 2: XE-103, XE-104 |
| [R6.52a](../../tickets/324_R6.52a__surface-tools.md) | ticket | E2-T2 (b) | 2: XE-105, XE-106 |
| [R6.53a](../../tickets/325_R6.53a__typed-renderers.md) | ticket | E2-T4 (a) | 2: XE-110, XE-111 |
| [R6.53b](../../tickets/326_R6.53b__observation-plumbing.md) | ticket | E2-T4 (b) | 1: XE-109 |
| [R6.54](../../tickets/328_R6.54__strategy-submit.md) | ticket | E2-T5 | 3: XE-115, XE-116, XE-117 |
| [R6.55](../../tickets/329_R6.55__exec-embed.md) | ticket | E2-T6 (a) | 1: XE-118 |
| [R6.55a](../../tickets/330_R6.55a__exec-defaults.md) | ticket | E2-T6 (b) | 1: XE-119 |
| [R6.57](../../tickets/331_R6.57__native-topology.md) | ticket | E2-T8 (a; rung record) | 1: XE-120 |
| [GATE-G26](../../tickets/335_GATE-G26__fp-s.md) | gate | GATE FP-S | 2: XH-007, XH-013 |
| [R6.60](../../tickets/337_R6.60__w2-run.md) | skeleton | W2{PROV, LOAD, PS0, PP0, CORPUS, TEARDOWN} | 2: XD-041, XH-010 |
| [R6.62](../../tickets/339_R6.62__fp-record-partition.md) | ticket | FP-RECORD-PARTITION | 3: XH-011, XH-114, XD-106 |
| [R6.56](../../tickets/340_R6.56__compaction-slot.md) | ticket | E2-T7 | 1: XE-122 |
| [R6.57a](../../tickets/341_R6.57a__native-participant.md) | ticket | E2-T8 (b) | 3: XE-121, XE-126, XD-102 |
| [R6.63](../../tickets/342_R6.63__mini-hir.md) | ticket | E2-T9 | 5: XE-101, XE-102, XE-114, XE-123, XD-123 |
| [R6.64](../../tickets/343_R6.64__sweep-glue.md) | ticket | E2-T10 | 2: XE-124, XE-125 |
| [R6.65](../../tickets/344_R6.65__d3-fp.md) | ticket | D3-FP (D3-a + D3-c, first-pass scope) | 3: XD-201, XD-202, XD-220 |
| [R6.68](../../tickets/346_R6.68__w3-run.md) | skeleton | W3{PROV, LOAD, H-SET BUILD+EXPORT, L1 run, TEARDOWN} (K5, E2-T11) | 5: XE-006, XE-007, XE-008, XE-009, XD-107 |
| [R6.66](../../tickets/352_R6.66__l2-wiring.md) | ticket | K7 | 8: XD-101, XD-110, XD-112, XD-113, XD-114, XD-116, XD-117, XD-118 |
| [R6.67](../../tickets/353_R6.67__site-phase34.md) | ticket | F2-h | 5: XF-143, XD-126, XD-222, XH-119, XH-122 |
| [R6.72](../../tickets/354_R6.72__fp-quote-e.md) | ticket | FP-E quote (D1-f first-pass form) | 3: XD-043, XD-044, XD-062 |
| [R6.75](../../tickets/359_R6.75__fp-results-comparative.md) | ticket | FP-RESULTS (a; D1-g first-pass form) | 3: XD-005, XD-090, XD-224 |
| [R6.76](../../tickets/360_R6.76__fp-results-parity.md) | ticket | FP-RESULTS (b; K9) | 2: XD-119, XD-120 |
| [HUMAN-H23](../../tickets/361_HUMAN-H23__flagged-review.md) | human | HUMAN flagged-pass review | 1: XD-071 |
| [R6.78](../../tickets/365_R6.78__pub-fp-record.md) | ticket | PUB-FP | 1: XH-105 |
| [CAP6.1](../../tickets/371_CAP6.1__cap1.md) | capstone | CAP.1 | 34: routed: XA-126, XC-314, XC-316, XD-013, XD-052, XD-204, XD-205, XD-206, XD-207, XD-208, XD-209, XD-210, XD-211, XD-212, XD-213, XD-214, XD-215, XD-216, XD-217, XD-218, XD-219, XD-221, XD-225, XE-002, XE-010, XE-011, XE-012, XE-013, XE-128, XE-129, XE-130, XE-131, XE-132, XH-033 |
| [CAP6.3](../../tickets/373_CAP6.3__cap3.md) | capstone | CAP.3 | 2: XH-021, XH-110 |
| [REC6.1](../../tickets/375_REC6.1__rec1.md) | reconcile | REC.1 | 2: XH-029, XH-106 |
| [REC6.2](../../tickets/376_REC6.2__rec2.md) | reconcile | REC.2 | 1: XH-015 |

## Later-pass routing (rule O4; owner CAP6.1, re-read by REC6.2)

| id | layer | routed to | trigger / reason |
|---|---|---|---|
| REQ-XA-126 | R | round-7 | conditional on Q-X24 (recorded default: records-in only; phase 1b unscheduled) |
| REQ-XC-314 | E | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XC-316 | E | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-013 | D | round-7 | first pass draws a 10-task band-A sub-manifest (§10.1.6); tranche census is later-pass |
| REQ-XD-052 | D | round-7 | tranche gates are later-pass; the first-pass analogue is FP-S/FP-E (REQ-XH-111) |
| REQ-XD-204 | E | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-205 | E | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-206 | R | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-207 | R | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-208 | E | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-209 | E | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-210 | R | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-211 | D | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-212 | E | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-213 | D | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-214 | E | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-215 | R | round-7 | confirmation window is later-pass (explore → confirm); the first-pass shared-baseline departure (§10.1.6 row "REQ-XD-215, REQ-XD-220", AR-33) is recorded by R6.58's pre-registration and cited here |
| REQ-XD-216 | R | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-217 | D | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-218 | E | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-219 | D | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-221 | D | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-225 | D | round-7 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-002 | D | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-010 | R | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-011 | R | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-012 | R | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-013 | D | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-128 | E | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-129 | E | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-130 | E | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-131 | E | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-132 | D | round-8 | later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XH-033 | R | round-7 | first pass governed by REQ-XH-116 (FP-7); the 87-task re-proof precedes any later-pass draw |

## Literal owner table

| Literal ID | Disposition / target | Primary owner / numbered acceptance | Required layer | Supplemental hooks | Contributing evidence owners | Status |
|---|---|---|---|---|---|---|
| REQ-XA-001 | merged-with(REQ-XA-119) | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-119) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-002 | merged-with(REQ-XA-111) | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-111) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-6` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-003 | merged-with(REQ-XA-109) | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-109) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-4` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-004 | merged-with(REQ-XA-107) | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-107) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-005 | merged-with(REQ-XA-119) | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-119) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-006 | adopted | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-006) | E | [§2.5](../../experiments-spec.md#s2); `AC-2.5-10` | R6.30a, R6.75 | planned; no execution evidence |
| REQ-XA-007 | superseded-by(REQ-XA-115) | [R6.4 AC](../../tickets/257_R6.4__p1-corrections.md#ac-r6-4-req-xa-115) | D | [§2.4](../../experiments-spec.md#s2); `AC-2.4-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-008 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-008) | E | [§2.5](../../experiments-spec.md#s2); `AC-2.5-11` | primary only | planned; no execution evidence |
| REQ-XA-009 | merged-with(REQ-XA-127) | [R6.23 AC](../../tickets/280_R6.23__verdict.md#ac-r6-23-req-xa-127) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-9` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-010 | merged-with(REQ-XB-101) | [R6.15a AC](../../tickets/267_R6.15a__runner-core.md#ac-r6-15a-req-xb-101) | E | [§3.2](../../experiments-spec.md#s3); `AC-3.2-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-011 | merged-with(REQ-XA-103) | [R6.2 AC](../../tickets/255_R6.2__p1-provenance.md#ac-r6-2-req-xa-103) | E | [§2.2](../../experiments-spec.md#s2); `AC-2.2-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-012 | merged-with(REQ-XB-215) | [R6.25 AC](../../tickets/284_R6.25__cost-record.md#ac-r6-25-req-xb-215) | E | [§3.10](../../experiments-spec.md#s3); `AC-3.10-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-013 | merged-with(REQ-XA-116) | [R6.4 AC](../../tickets/257_R6.4__p1-corrections.md#ac-r6-4-req-xa-116) | D | [§2.4](../../experiments-spec.md#s2); `AC-2.4-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-014 | merged-with(REQ-XA-121) | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-121) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-4` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-015 | merged-with(REQ-XA-125) | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-125) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-8` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-016 | merged-with(REQ-XA-108) | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-108) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XA-101 | adopted | [R6.2 AC](../../tickets/255_R6.2__p1-provenance.md#ac-r6-2-req-xa-101) | E | [§2.2](../../experiments-spec.md#s2); `AC-2.2-1` | primary only | planned; no execution evidence |
| REQ-XA-102 | adopted | [R6.2 AC](../../tickets/255_R6.2__p1-provenance.md#ac-r6-2-req-xa-102) | E | [§2.2](../../experiments-spec.md#s2); `AC-2.2-2` | primary only | planned; no execution evidence |
| REQ-XA-103 | adopted | [R6.2 AC](../../tickets/255_R6.2__p1-provenance.md#ac-r6-2-req-xa-103) | E | [§2.2](../../experiments-spec.md#s2); `AC-2.2-3` | primary only | planned; no execution evidence |
| REQ-XA-104 | adopted | [R6.2 AC](../../tickets/255_R6.2__p1-provenance.md#ac-r6-2-req-xa-104) | D | [§2.2](../../experiments-spec.md#s2); `AC-2.2-4` | primary only | planned; no execution evidence |
| REQ-XA-105 | adopted | [R6.2 AC](../../tickets/255_R6.2__p1-provenance.md#ac-r6-2-req-xa-105) | E | [§2.2](../../experiments-spec.md#s2); `AC-2.2-5` | primary only | planned; no execution evidence |
| REQ-XA-106 | adopted | [R6.2 AC](../../tickets/255_R6.2__p1-provenance.md#ac-r6-2-req-xa-106) | E | [§2.2](../../experiments-spec.md#s2); `AC-2.2-6` | primary only | planned; no execution evidence |
| REQ-XA-107 | adopted | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-107) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-1` | primary only | planned; no execution evidence |
| REQ-XA-108 | adopted | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-108) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-2` | primary only | planned; no execution evidence |
| REQ-XA-109 | adopted | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-109) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-4` | primary only | planned; no execution evidence |
| REQ-XA-110 | adopted | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-110) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-5` | primary only | planned; no execution evidence |
| REQ-XA-111 | adopted | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-111) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-6` | primary only | planned; no execution evidence |
| REQ-XA-112 | adopted | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-112) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-7` | primary only | planned; no execution evidence |
| REQ-XA-113 | adopted | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xa-113) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-8` | primary only | planned; no execution evidence |
| REQ-XA-114 | adopted | [R6.4 AC](../../tickets/257_R6.4__p1-corrections.md#ac-r6-4-req-xa-114) | D | [§2.4](../../experiments-spec.md#s2); `AC-2.4-1` | primary only | planned; no execution evidence |
| REQ-XA-115 | adopted | [R6.4 AC](../../tickets/257_R6.4__p1-corrections.md#ac-r6-4-req-xa-115) | D | [§2.4](../../experiments-spec.md#s2); `AC-2.4-2` | primary only | planned; no execution evidence |
| REQ-XA-116 | adopted | [R6.4 AC](../../tickets/257_R6.4__p1-corrections.md#ac-r6-4-req-xa-116) | D | [§2.4](../../experiments-spec.md#s2); `AC-2.4-3` | primary only | planned; no execution evidence |
| REQ-XA-117 | adopted | [R6.4 AC](../../tickets/257_R6.4__p1-corrections.md#ac-r6-4-req-xa-117) | E | [§2.4](../../experiments-spec.md#s2); `AC-2.4-4` | primary only | planned; no execution evidence |
| REQ-XA-118 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-118) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-1` | primary only | planned; no execution evidence |
| REQ-XA-119 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-119) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-2` | primary only | planned; no execution evidence |
| REQ-XA-120 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-120) | E | [§2.5](../../experiments-spec.md#s2); `AC-2.5-3` | primary only | planned; no execution evidence |
| REQ-XA-121 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-121) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-4` | primary only | planned; no execution evidence |
| REQ-XA-122 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-122) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-5` | primary only | planned; no execution evidence |
| REQ-XA-123 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-123) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-6` | primary only | planned; no execution evidence |
| REQ-XA-124 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-124) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-7` | primary only | planned; no execution evidence |
| REQ-XA-125 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xa-125) | D | [§2.5](../../experiments-spec.md#s2); `AC-2.5-8` | primary only | planned; no execution evidence |
| REQ-XA-126 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xa-126) | R | [§2.5](../../experiments-spec.md#s2); `AC-2.5-9` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): conditional on Q-X24 (recorded default: records-in only; phase 1b unscheduled) |
| REQ-XA-127 | adopted | [R6.23 AC](../../tickets/280_R6.23__verdict.md#ac-r6-23-req-xa-127) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-9` | R6.48 | planned; no execution evidence |
| REQ-XB-001 | merged-with(REQ-XB-122) | [R6.18 AC](../../tickets/272_R6.18__select.md#ac-r6-18-req-xb-122) | E | [§3.1](../../experiments-spec.md#s3); `AC-3.1-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-002 | merged-with(REQ-XB-124) | [R6.27 AC](../../tickets/286_R6.27__harbor-o.md#ac-r6-27-req-xb-124) | E | [§3.1](../../experiments-spec.md#s3); `AC-3.1-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-003 | merged-with(REQ-XB-117) | [R6.16 AC](../../tickets/270_R6.16__cell-spec.md#ac-r6-16-req-xb-117) | E | [§3.1](../../experiments-spec.md#s3); `AC-3.1-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-004 | merged-with(REQ-XD-010) | [R6.47 AC](../../tickets/312_R6.47__fp-select.md#ac-r6-47-req-xd-010) | D | [§6.2](../../experiments-spec.md#s6); `AC-6.2-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-005 | merged-with(REQ-XB-125) | [R6.48 AC](../../tickets/318_R6.48__w1-run.md#ac-r6-48-req-xb-125) | R | [§3.8](../../experiments-spec.md#s3); `AC-3.8-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-006 | merged-with(REQ-XB-106) | [R6.16 AC](../../tickets/270_R6.16__cell-spec.md#ac-r6-16-req-xb-106) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-007 | merged-with(REQ-XB-110) | [R6.23 AC](../../tickets/280_R6.23__verdict.md#ac-r6-23-req-xb-110) | E | [§3.4](../../experiments-spec.md#s3); `AC-3.4-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-008 | merged-with(REQ-XB-113) | [R6.24a AC](../../tickets/281_R6.24a__egress-profile.md#ac-r6-24a-req-xb-113) | E | [§3.5](../../experiments-spec.md#s3); `AC-3.5-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-009 | merged-with(REQ-XB-118) | [R6.17 AC](../../tickets/271_R6.17__deadline.md#ac-r6-17-req-xb-118) | E | [§3.6](../../experiments-spec.md#s3); `AC-3.6-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-010 | merged-with(REQ-XB-121) | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xb-121) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-6` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-011 | merged-with(REQ-XB-123) | [R6.27 AC](../../tickets/286_R6.27__harbor-o.md#ac-r6-27-req-xb-123) | E | [§3.7](../../experiments-spec.md#s3); `AC-3.7-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-012 | merged-with(REQ-XB-120) | [R6.15a AC](../../tickets/267_R6.15a__runner-core.md#ac-r6-15a-req-xb-120) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-5` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-013 | superseded-by(REQ-XC-318) | [R6.37 AC](../../tickets/302_R6.37__capture.md#ac-r6-37-req-xc-318) | E | [§5.6](../../experiments-spec.md#s5); `AC-5.6-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-014 | merged-with(REQ-XC-302) | [R6.34a AC](../../tickets/297_R6.34a__admit-record.md#ac-r6-34a-req-xc-302) | E | [§5.1](../../experiments-spec.md#s5); `AC-5.1-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-015 | merged-with(REQ-XD-092) | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xd-092) | D | [§6.11](../../experiments-spec.md#s6); `AC-6.11-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-016 | merged-with(REQ-XD-070) | [R6.40 AC](../../tickets/306_R6.40__answer-scan.md#ac-r6-40-req-xd-070) | E | [§6.9](../../experiments-spec.md#s6); `AC-6.9-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-017 | superseded-by(REQ-XD-011) | [R6.18 AC](../../tickets/272_R6.18__select.md#ac-r6-18-req-xd-011) | E | [§6.2](../../experiments-spec.md#s6); `AC-6.2-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-018 | merged-with(REQ-XB-123) | [R6.27 AC](../../tickets/286_R6.27__harbor-o.md#ac-r6-27-req-xb-123) | E | [§3.7](../../experiments-spec.md#s3); `AC-3.7-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-101 | adopted | [R6.15a AC](../../tickets/267_R6.15a__runner-core.md#ac-r6-15a-req-xb-101) | E | [§3.2](../../experiments-spec.md#s3); `AC-3.2-1` | primary only | planned; no execution evidence |
| REQ-XB-102 | adopted | [R6.15a AC](../../tickets/267_R6.15a__runner-core.md#ac-r6-15a-req-xb-102) | E | [§3.2](../../experiments-spec.md#s3); `AC-3.2-2` | primary only | planned; no execution evidence |
| REQ-XB-103 | adopted | [R6.15a AC](../../tickets/267_R6.15a__runner-core.md#ac-r6-15a-req-xb-103) | E | [§3.2](../../experiments-spec.md#s3); `AC-3.2-3` | primary only | planned; no execution evidence |
| REQ-XB-104 | adopted | [R6.24c AC](../../tickets/283_R6.24c__runner-supervision.md#ac-r6-24c-req-xb-104) | E | [§3.2](../../experiments-spec.md#s3); `AC-3.2-4` | R6.15b | planned; no execution evidence |
| REQ-XB-105 | adopted | [R6.15b AC](../../tickets/268_R6.15b__runner-topology.md#ac-r6-15b-req-xb-105) | E | [§3.2](../../experiments-spec.md#s3); `AC-3.2-5` | primary only | planned; no execution evidence |
| REQ-XB-106 | adopted | [R6.16 AC](../../tickets/270_R6.16__cell-spec.md#ac-r6-16-req-xb-106) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-1` | primary only | planned; no execution evidence |
| REQ-XB-107 | adopted | [R6.16 AC](../../tickets/270_R6.16__cell-spec.md#ac-r6-16-req-xb-107) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-2` | primary only | planned; no execution evidence |
| REQ-XB-108 | adopted | [R6.15a AC](../../tickets/267_R6.15a__runner-core.md#ac-r6-15a-req-xb-108) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-3` | primary only | planned; no execution evidence |
| REQ-XB-109 | adopted | [R6.23 AC](../../tickets/280_R6.23__verdict.md#ac-r6-23-req-xb-109) | E | [§3.4](../../experiments-spec.md#s3); `AC-3.4-1` | primary only | planned; no execution evidence |
| REQ-XB-110 | adopted | [R6.23 AC](../../tickets/280_R6.23__verdict.md#ac-r6-23-req-xb-110) | E | [§3.4](../../experiments-spec.md#s3); `AC-3.4-2` | primary only | planned; no execution evidence |
| REQ-XB-111 | adopted | [R6.23 AC](../../tickets/280_R6.23__verdict.md#ac-r6-23-req-xb-111) | E | [§3.4](../../experiments-spec.md#s3); `AC-3.4-3` | primary only | planned; no execution evidence |
| REQ-XB-112 | adopted | [R6.23 AC](../../tickets/280_R6.23__verdict.md#ac-r6-23-req-xb-112) | E | [§3.4](../../experiments-spec.md#s3); `AC-3.4-4` | primary only | planned; no execution evidence |
| REQ-XB-113 | adopted | [R6.24a AC](../../tickets/281_R6.24a__egress-profile.md#ac-r6-24a-req-xb-113) | E | [§3.5](../../experiments-spec.md#s3); `AC-3.5-1` | primary only | planned; no execution evidence |
| REQ-XB-114 | adopted | [R6.24b AC](../../tickets/282_R6.24b__egress-mediator.md#ac-r6-24b-req-xb-114) | E | [§3.5](../../experiments-spec.md#s3); `AC-3.5-2` | primary only | planned; no execution evidence |
| REQ-XB-115 | adopted | [R6.24a AC](../../tickets/281_R6.24a__egress-profile.md#ac-r6-24a-req-xb-115) | E | [§3.5](../../experiments-spec.md#s3); `AC-3.5-3` | primary only | planned; no execution evidence |
| REQ-XB-116 | adopted | [R6.24b AC](../../tickets/282_R6.24b__egress-mediator.md#ac-r6-24b-req-xb-116) | E | [§3.5](../../experiments-spec.md#s3); `AC-3.5-4` | primary only | planned; no execution evidence |
| REQ-XB-117 | adopted | [R6.16 AC](../../tickets/270_R6.16__cell-spec.md#ac-r6-16-req-xb-117) | E | [§3.1](../../experiments-spec.md#s3); `AC-3.1-2` | primary only | planned; no execution evidence |
| REQ-XB-118 | adopted | [R6.17 AC](../../tickets/271_R6.17__deadline.md#ac-r6-17-req-xb-118) | E | [§3.6](../../experiments-spec.md#s3); `AC-3.6-1` | primary only | planned; no execution evidence |
| REQ-XB-119 | adopted | [R6.17 AC](../../tickets/271_R6.17__deadline.md#ac-r6-17-req-xb-119) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-4` | primary only | planned; no execution evidence |
| REQ-XB-120 | adopted | [R6.15a AC](../../tickets/267_R6.15a__runner-core.md#ac-r6-15a-req-xb-120) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-5` | primary only | planned; no execution evidence |
| REQ-XB-121 | adopted | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xb-121) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-6` | primary only | planned; no execution evidence |
| REQ-XB-122 | adopted | [R6.18 AC](../../tickets/272_R6.18__select.md#ac-r6-18-req-xb-122) | E | [§3.1](../../experiments-spec.md#s3); `AC-3.1-1` | primary only | planned; no execution evidence |
| REQ-XB-123 | adopted | [R6.27 AC](../../tickets/286_R6.27__harbor-o.md#ac-r6-27-req-xb-123) | E | [§3.7](../../experiments-spec.md#s3); `AC-3.7-1` | primary only | planned; no execution evidence |
| REQ-XB-124 | adopted | [R6.27 AC](../../tickets/286_R6.27__harbor-o.md#ac-r6-27-req-xb-124) | E | [§3.1](../../experiments-spec.md#s3); `AC-3.1-3` | primary only | planned; no execution evidence |
| REQ-XB-125 | adopted | [R6.48 AC](../../tickets/318_R6.48__w1-run.md#ac-r6-48-req-xb-125) | R | [§3.8](../../experiments-spec.md#s3); `AC-3.8-1` | R6.28, R6.47 | planned; owed at fleshing (skeleton row; fleshing in place inherits this ownership) |
| REQ-XB-126 | adopted | [R6.28 AC](../../tickets/287_R6.28__reproof-kit.md#ac-r6-28-req-xb-126) | D | [§3.8](../../experiments-spec.md#s3); `AC-3.8-2` | R6.48 | planned; no execution evidence |
| REQ-XB-127 | adopted | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xb-127) | E | [§3.2](../../experiments-spec.md#s3); `AC-3.2-6` | primary only | planned; no execution evidence |
| REQ-XB-128 | adopted (synthesis-allocated) | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xb-128) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-17` | primary only | planned; no execution evidence |
| REQ-XB-129 | adopted (synthesis-allocated) | [R6.24a AC](../../tickets/281_R6.24a__egress-profile.md#ac-r6-24a-req-xb-129) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-9` | primary only | planned; no execution evidence |
| REQ-XB-201 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-201) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-1` | primary only | planned; no execution evidence |
| REQ-XB-202 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-202) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-2` | primary only | planned; no execution evidence |
| REQ-XB-203 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-203) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-3` | primary only | planned; no execution evidence |
| REQ-XB-204 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-204) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-4` | primary only | planned; no execution evidence |
| REQ-XB-205 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-205) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-5` | primary only | planned; no execution evidence |
| REQ-XB-206 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-206) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-6` | primary only | planned; no execution evidence |
| REQ-XB-207 | adopted | [R6.19b AC](../../tickets/274_R6.19b__recipe-teardown.md#ac-r6-19b-req-xb-207) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-7` | primary only | planned; no execution evidence |
| REQ-XB-208 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-208) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-8` | primary only | planned; no execution evidence |
| REQ-XB-209 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-209) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-9` | primary only | planned; no execution evidence |
| REQ-XB-210 | adopted | [R6.19b AC](../../tickets/274_R6.19b__recipe-teardown.md#ac-r6-19b-req-xb-210) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-10` | primary only | planned; no execution evidence |
| REQ-XB-211 | adopted | [R6.19b AC](../../tickets/274_R6.19b__recipe-teardown.md#ac-r6-19b-req-xb-211) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-11` | primary only | planned; no execution evidence |
| REQ-XB-212 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-212) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-12` | R6.73 | planned; no execution evidence |
| REQ-XB-213 | adopted | [R6.25 AC](../../tickets/284_R6.25__cost-record.md#ac-r6-25-req-xb-213) | E | [§3.10](../../experiments-spec.md#s3); `AC-3.10-1` | primary only | planned; no execution evidence |
| REQ-XB-214 | adopted | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xb-214) | E | [§3.10](../../experiments-spec.md#s3); `AC-3.10-2` | primary only | planned; no execution evidence |
| REQ-XB-215 | adopted | [R6.25 AC](../../tickets/284_R6.25__cost-record.md#ac-r6-25-req-xb-215) | E | [§3.10](../../experiments-spec.md#s3); `AC-3.10-3` | primary only | planned; no execution evidence |
| REQ-XB-216 | adopted | [R6.25 AC](../../tickets/284_R6.25__cost-record.md#ac-r6-25-req-xb-216) | E | [§3.10](../../experiments-spec.md#s3); `AC-3.10-4` | primary only | planned; no execution evidence |
| REQ-XB-217 | adopted | [R6.25 AC](../../tickets/284_R6.25__cost-record.md#ac-r6-25-req-xb-217) | E | [§3.10](../../experiments-spec.md#s3); `AC-3.10-5` | primary only | planned; no execution evidence |
| REQ-XB-218 | merged-with(REQ-XC-117) | [R6.26 AC](../../tickets/285_R6.26__reconcile.md#ac-r6-26-req-xc-117) | E | [§4.5](../../experiments-spec.md#s4); `AC-4.5-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-219 | adopted | [R6.19b AC](../../tickets/274_R6.19b__recipe-teardown.md#ac-r6-19b-req-xb-219) | E | [§3.11](../../experiments-spec.md#s3); `AC-3.11-1` | primary only | planned; no execution evidence |
| REQ-XB-220 | adopted | [R6.19b AC](../../tickets/274_R6.19b__recipe-teardown.md#ac-r6-19b-req-xb-220) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-13` | primary only | planned; no execution evidence |
| REQ-XB-221 | merged-with(REQ-XD-051) | [R6.29 AC](../../tickets/288_R6.29__comparative-protocol.md#ac-r6-29-req-xd-051) | D | [§6.7](../../experiments-spec.md#s6); `AC-6.7-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XB-222 | adopted | [R6.4 AC](../../tickets/257_R6.4__p1-corrections.md#ac-r6-4-req-xb-222) | D | [§2.4](../../experiments-spec.md#s2); `AC-2.4-5` | primary only | planned; no execution evidence |
| REQ-XB-223 | adopted | [R6.19a AC](../../tickets/273_R6.19a__recipe-provision.md#ac-r6-19a-req-xb-223) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-14` | primary only | planned; no execution evidence |
| REQ-XB-224 | adopted (synthesis-allocated) | [R6.19b AC](../../tickets/274_R6.19b__recipe-teardown.md#ac-r6-19b-req-xb-224) | E | [§3.9](../../experiments-spec.md#s3); `AC-3.9-18` | primary only | planned; no execution evidence |
| REQ-XC-001 | merged-with(REQ-XC-104) | [R6.14 AC](../../tickets/266_R6.14__lane.md#ac-r6-14-req-xc-104) | E | [§4.1](../../experiments-spec.md#s4); `AC-4.1-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-002 | merged-with(REQ-XC-107) | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-107) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-4` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-003 | superseded-by(REQ-XC-113) | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xc-113) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-004 | adopted | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xc-004) | D | [§4.4](../../experiments-spec.md#s4); `AC-4.4-6` | primary only | planned; no execution evidence |
| REQ-XC-005 | merged-with(REQ-XC-121) | [R6.26 AC](../../tickets/285_R6.26__reconcile.md#ac-r6-26-req-xc-121) | E | [§4.7](../../experiments-spec.md#s4); `AC-4.7-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-006 | adopted | [R6.26 AC](../../tickets/285_R6.26__reconcile.md#ac-r6-26-req-xc-006) | E | [§4.7](../../experiments-spec.md#s4); `AC-4.7-2` | primary only | planned; no execution evidence |
| REQ-XC-007 | adopted | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-007) | E | [§4.3](../../experiments-spec.md#s4); `AC-4.3-3` | primary only | planned; no execution evidence |
| REQ-XC-008 | merged-with(REQ-XC-116) | [R6.13 AC](../../tickets/265_R6.13__price.md#ac-r6-13-req-xc-116) | E | [§4.5](../../experiments-spec.md#s4); `AC-4.5-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-009 | merged-with(REQ-XC-119) | [R6.20b AC](../../tickets/276_R6.20b__proxy-release.md#ac-r6-20b-req-xc-119) | E | [§4.6](../../experiments-spec.md#s4); `AC-4.6-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-010 | adopted | [R6.29 AC](../../tickets/288_R6.29__comparative-protocol.md#ac-r6-29-req-xc-010) | D | [§4.6](../../experiments-spec.md#s4); `AC-4.6-3` | primary only | planned; no execution evidence |
| REQ-XC-011 | superseded-by(REQ-XC-115) | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xc-115) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-4` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-012 | merged-with(REQ-XC-106) | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-106) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-013 | merged-with(REQ-XC-108) | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-108) | E | [§4.3](../../experiments-spec.md#s4); `AC-4.3-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-014 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xc-014) | D | [§6.11](../../experiments-spec.md#s6); `AC-6.11-3` | primary only | planned; no execution evidence |
| REQ-XC-015 | merged-with(REQ-XD-043) | [R6.72 AC](../../tickets/354_R6.72__fp-quote-e.md#ac-r6-72-req-xd-043) | E | [§6.6](../../experiments-spec.md#s6); `AC-6.6-6` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-101 | adopted | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-101) | E | [§4.1](../../experiments-spec.md#s4); `AC-4.1-1` | R6.24c | planned; no execution evidence |
| REQ-XC-102 | merged-with(REQ-XB-113) | [R6.24a AC](../../tickets/281_R6.24a__egress-profile.md#ac-r6-24a-req-xb-113) | E | [§3.5](../../experiments-spec.md#s3); `AC-3.5-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-103 | adopted | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-103) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-1` | primary only | planned; no execution evidence |
| REQ-XC-104 | adopted | [R6.14 AC](../../tickets/266_R6.14__lane.md#ac-r6-14-req-xc-104) | E | [§4.1](../../experiments-spec.md#s4); `AC-4.1-2` | primary only | planned; no execution evidence |
| REQ-XC-105 | adopted | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-105) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-2` | primary only | planned; no execution evidence |
| REQ-XC-106 | adopted | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-106) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-3` | primary only | planned; no execution evidence |
| REQ-XC-107 | adopted | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-107) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-4` | primary only | planned; no execution evidence |
| REQ-XC-108 | adopted | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-108) | E | [§4.3](../../experiments-spec.md#s4); `AC-4.3-1` | primary only | planned; no execution evidence |
| REQ-XC-109 | adopted | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-109) | E | [§4.3](../../experiments-spec.md#s4); `AC-4.3-2` | R6.24c | planned; no execution evidence |
| REQ-XC-110 | adopted | [R6.20b AC](../../tickets/276_R6.20b__proxy-release.md#ac-r6-20b-req-xc-110) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-5` | primary only | planned; no execution evidence |
| REQ-XC-111 | adopted | [R6.20b AC](../../tickets/276_R6.20b__proxy-release.md#ac-r6-20b-req-xc-111) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-6` | primary only | planned; no execution evidence |
| REQ-XC-112 | adopted | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xc-112) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-1` | primary only | planned; no execution evidence |
| REQ-XC-113 | adopted | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xc-113) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-2` | primary only | planned; no execution evidence |
| REQ-XC-114 | adopted | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xc-114) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-3` | primary only | planned; no execution evidence |
| REQ-XC-115 | adopted | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xc-115) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-4` | primary only | planned; no execution evidence |
| REQ-XC-116 | adopted | [R6.13 AC](../../tickets/265_R6.13__price.md#ac-r6-13-req-xc-116) | E | [§4.5](../../experiments-spec.md#s4); `AC-4.5-1` | primary only | planned; no execution evidence |
| REQ-XC-117 | adopted | [R6.26 AC](../../tickets/285_R6.26__reconcile.md#ac-r6-26-req-xc-117) | E | [§4.5](../../experiments-spec.md#s4); `AC-4.5-2` | primary only | planned; no execution evidence |
| REQ-XC-118 | adopted | [R6.26 AC](../../tickets/285_R6.26__reconcile.md#ac-r6-26-req-xc-118) | E | [§4.5](../../experiments-spec.md#s4); `AC-4.5-3` | primary only | planned; no execution evidence |
| REQ-XC-119 | adopted | [R6.20b AC](../../tickets/276_R6.20b__proxy-release.md#ac-r6-20b-req-xc-119) | E | [§4.6](../../experiments-spec.md#s4); `AC-4.6-1` | primary only | planned; no execution evidence |
| REQ-XC-120 | adopted | [R6.11 AC](../../tickets/264_R6.11__pages-workflow.md#ac-r6-11-req-xc-120) | D | [§4.6](../../experiments-spec.md#s4); `AC-4.6-2` | primary only | planned; no execution evidence |
| REQ-XC-121 | adopted | [R6.26 AC](../../tickets/285_R6.26__reconcile.md#ac-r6-26-req-xc-121) | E | [§4.7](../../experiments-spec.md#s4); `AC-4.7-1` | primary only | planned; no execution evidence |
| REQ-XC-122 | adopted | [R6.22 AC](../../tickets/279_R6.22__replay.md#ac-r6-22-req-xc-122) | E | [§4.9](../../experiments-spec.md#s4); `AC-4.9-1` | primary only | planned; no execution evidence |
| REQ-XC-123 | adopted | [R6.20b AC](../../tickets/276_R6.20b__proxy-release.md#ac-r6-20b-req-xc-123) | E | [§4.8](../../experiments-spec.md#s4); `AC-4.8-1` | primary only | planned; no execution evidence |
| REQ-XC-124 | adopted | [R6.20b AC](../../tickets/276_R6.20b__proxy-release.md#ac-r6-20b-req-xc-124) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-5` | primary only | planned; no execution evidence |
| REQ-XC-125 | adopted | [R6.22 AC](../../tickets/279_R6.22__replay.md#ac-r6-22-req-xc-125) | E | [§4.9](../../experiments-spec.md#s4); `AC-4.9-2` | primary only | planned; no execution evidence |
| REQ-XC-126 | adopted (synthesis-allocated) | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xc-126) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-11` | primary only | planned; no execution evidence |
| REQ-XC-127 | adopted (synthesis-allocated) | [R6.26 AC](../../tickets/285_R6.26__reconcile.md#ac-r6-26-req-xc-127) | E | [§4.7](../../experiments-spec.md#s4); `AC-4.7-4` | R6.61, R6.74 | planned; no execution evidence |
| REQ-XC-201 | merged-with(REQ-XC-302) | [R6.34a AC](../../tickets/297_R6.34a__admit-record.md#ac-r6-34a-req-xc-302) | E | [§5.1](../../experiments-spec.md#s5); `AC-5.1-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-202 | adopted | [R6.34a AC](../../tickets/297_R6.34a__admit-record.md#ac-r6-34a-req-xc-202) | D | [§5.1](../../experiments-spec.md#s5); `AC-5.1-2` | primary only | planned; no execution evidence |
| REQ-XC-203 | adopted | [R6.38 AC](../../tickets/303_R6.38__smoke-battery.md#ac-r6-38-req-xc-203) | E | [§5.1](../../experiments-spec.md#s5); `AC-5.1-3` | primary only | planned; no execution evidence |
| REQ-XC-204 | merged-with(REQ-XC-304) | [R6.34b AC](../../tickets/298_R6.34b__admit-build.md#ac-r6-34b-req-xc-304) | E | [§5.2](../../experiments-spec.md#s5); `AC-5.2-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-205 | merged-with(REQ-XC-311) | [R6.38 AC](../../tickets/303_R6.38__smoke-battery.md#ac-r6-38-req-xc-311) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-6` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-206 | merged-with(REQ-XC-105) | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-105) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-207 | adopted | [R6.37 AC](../../tickets/302_R6.37__capture.md#ac-r6-37-req-xc-207) | E | [§4.3](../../experiments-spec.md#s4); `AC-4.3-4` | primary only | planned; no execution evidence |
| REQ-XC-208 | adopted | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-208) | E | [§4.3](../../experiments-spec.md#s4); `AC-4.3-5` | primary only | planned; no execution evidence |
| REQ-XC-209 | merged-with(REQ-XC-106) | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-106) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-210 | adopted | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xc-210) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-7` | primary only | planned; no execution evidence |
| REQ-XC-211 | merged-with(REQ-XD-040) | [R6.44 AC](../../tickets/309_R6.44__fp-continuation.md#ac-r6-44-req-xd-040) | E | [§6.6](../../experiments-spec.md#s6); `AC-6.6-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-212 | merged-with(REQ-XC-014) | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xc-014) | D | [§6.11](../../experiments-spec.md#s6); `AC-6.11-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-213 | merged-with(REQ-XC-107) | [R6.20a AC](../../tickets/275_R6.20a__proxy-core.md#ac-r6-20a-req-xc-107) | E | [§4.2](../../experiments-spec.md#s4); `AC-4.2-4` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-214 | adopted | [R6.34a AC](../../tickets/297_R6.34a__admit-record.md#ac-r6-34a-req-xc-214) | D | [§5.1](../../experiments-spec.md#s5); `AC-5.1-4` | primary only | planned; no execution evidence |
| REQ-XC-301 | adopted | [R6.34a AC](../../tickets/297_R6.34a__admit-record.md#ac-r6-34a-req-xc-301) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-1` | primary only | planned; no execution evidence |
| REQ-XC-302 | adopted | [R6.34a AC](../../tickets/297_R6.34a__admit-record.md#ac-r6-34a-req-xc-302) | E | [§5.1](../../experiments-spec.md#s5); `AC-5.1-1` | primary only | planned; no execution evidence |
| REQ-XC-303 | adopted | [R6.34b AC](../../tickets/298_R6.34b__admit-build.md#ac-r6-34b-req-xc-303) | E | [§5.2](../../experiments-spec.md#s5); `AC-5.2-1` | primary only | planned; no execution evidence |
| REQ-XC-304 | adopted | [R6.34b AC](../../tickets/298_R6.34b__admit-build.md#ac-r6-34b-req-xc-304) | E | [§5.2](../../experiments-spec.md#s5); `AC-5.2-2` | primary only | planned; no execution evidence |
| REQ-XC-305 | adopted | [R6.34b AC](../../tickets/298_R6.34b__admit-build.md#ac-r6-34b-req-xc-305) | E | [§5.2](../../experiments-spec.md#s5); `AC-5.2-3` | primary only | planned; no execution evidence |
| REQ-XC-306 | adopted | [R6.27 AC](../../tickets/286_R6.27__harbor-o.md#ac-r6-27-req-xc-306) | E | [§5.2](../../experiments-spec.md#s5); `AC-5.2-4` | primary only | planned; no execution evidence |
| REQ-XC-307 | adopted | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xc-307) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-2` | primary only | planned; no execution evidence |
| REQ-XC-308 | adopted | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xc-308) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-3` | primary only | planned; no execution evidence |
| REQ-XC-309 | adopted | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xc-309) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-4` | primary only | planned; no execution evidence |
| REQ-XC-310 | adopted | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xc-310) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-5` | primary only | planned; no execution evidence |
| REQ-XC-311 | adopted | [R6.38 AC](../../tickets/303_R6.38__smoke-battery.md#ac-r6-38-req-xc-311) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-6` | primary only | planned; no execution evidence |
| REQ-XC-312 | adopted | [R6.38 AC](../../tickets/303_R6.38__smoke-battery.md#ac-r6-38-req-xc-312) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-7` | primary only | planned; no execution evidence |
| REQ-XC-313 | adopted | [R6.40 AC](../../tickets/306_R6.40__answer-scan.md#ac-r6-40-req-xc-313) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-8` | primary only | planned; no execution evidence |
| REQ-XC-314 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xc-314) | E | [§5.5](../../experiments-spec.md#s5); `AC-5.5-1` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XC-315 | adopted | [R6.27 AC](../../tickets/286_R6.27__harbor-o.md#ac-r6-27-req-xc-315) | E | [§5.5](../../experiments-spec.md#s5); `AC-5.5-2` | primary only | planned; no execution evidence |
| REQ-XC-316 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xc-316) | E | [§5.5](../../experiments-spec.md#s5); `AC-5.5-3` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XC-317 | adopted | [R6.37 AC](../../tickets/302_R6.37__capture.md#ac-r6-37-req-xc-317) | E | [§5.6](../../experiments-spec.md#s5); `AC-5.6-1` | primary only | planned; no execution evidence |
| REQ-XC-318 | adopted | [R6.37 AC](../../tickets/302_R6.37__capture.md#ac-r6-37-req-xc-318) | E | [§5.6](../../experiments-spec.md#s5); `AC-5.6-2` | primary only | planned; no execution evidence |
| REQ-XC-319 | merged-with(REQ-XD-070) | [R6.40 AC](../../tickets/306_R6.40__answer-scan.md#ac-r6-40-req-xd-070) | E | [§6.9](../../experiments-spec.md#s6); `AC-6.9-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XC-320 | adopted | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xc-320) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-9` | primary only | planned; no execution evidence |
| REQ-XC-321 | adopted | [R6.38 AC](../../tickets/303_R6.38__smoke-battery.md#ac-r6-38-req-xc-321) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-10` | primary only | planned; no execution evidence |
| REQ-XC-322 | adopted | [R6.20b AC](../../tickets/276_R6.20b__proxy-release.md#ac-r6-20b-req-xc-322) | E | [§5.3](../../experiments-spec.md#s5); `AC-5.3-11` | primary only | planned; no execution evidence |
| REQ-XC-323 | adopted | [R6.38 AC](../../tickets/303_R6.38__smoke-battery.md#ac-r6-38-req-xc-323) | E | [§5.8](../../experiments-spec.md#s5); `AC-5.8-1` | primary only | planned; no execution evidence |
| REQ-XC-324 | adopted | [R6.38 AC](../../tickets/303_R6.38__smoke-battery.md#ac-r6-38-req-xc-324) | E | [§5.7](../../experiments-spec.md#s5); `AC-5.7-1` | R6.48 | planned; no execution evidence |
| REQ-XC-325 | adopted | [R6.39 AC](../../tickets/305_R6.39__runner-check.md#ac-r6-39-req-xc-325) | R | [§5.7](../../experiments-spec.md#s5); `AC-5.7-2` | R6.48 | planned; no execution evidence |
| REQ-XC-326 | adopted | [R6.39 AC](../../tickets/305_R6.39__runner-check.md#ac-r6-39-req-xc-326) | E | [§5.7](../../experiments-spec.md#s5); `AC-5.7-3` | primary only | planned; no execution evidence |
| REQ-XC-327 | adopted | [R6.36 AC](../../tickets/301_R6.36__adapt-messages.md#ac-r6-36-req-xc-327) | E | [§5.4](../../experiments-spec.md#s5); `AC-5.4-1` | primary only | planned; no execution evidence |
| REQ-XC-328 | adopted | [R6.37 AC](../../tickets/302_R6.37__capture.md#ac-r6-37-req-xc-328) | E | [§5.6](../../experiments-spec.md#s5); `AC-5.6-3` | primary only | planned; no execution evidence |
| REQ-XC-329 | adopted | [R6.38 AC](../../tickets/303_R6.38__smoke-battery.md#ac-r6-38-req-xc-329) | E | [§5.7](../../experiments-spec.md#s5); `AC-5.7-4` | primary only | planned; no execution evidence |
| REQ-XC-330 | adopted (synthesis-allocated) | [R6.34c AC](../../tickets/299_R6.34c__binary-sets.md#ac-r6-34c-req-xc-330) | E | [§5.2](../../experiments-spec.md#s5); `AC-5.2-5` | R6.34b, R6.48, R6.68 | planned; no execution evidence |
| REQ-XD-001 | adopted | [R6.30a AC](../../tickets/289_R6.30a__estimators.md#ac-r6-30a-req-xd-001) | R | [§6.1](../../experiments-spec.md#s6); `AC-6.1-1` | primary only | planned; no execution evidence |
| REQ-XD-002 | adopted | [R6.29 AC](../../tickets/288_R6.29__comparative-protocol.md#ac-r6-29-req-xd-002) | E | [§6.1](../../experiments-spec.md#s6); `AC-6.1-2` | primary only | planned; no execution evidence |
| REQ-XD-003 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xd-003) | D | [§6.1](../../experiments-spec.md#s6); `AC-6.1-3` | primary only | planned; no execution evidence |
| REQ-XD-004 | adopted | [R6.29 AC](../../tickets/288_R6.29__comparative-protocol.md#ac-r6-29-req-xd-004) | D | [§6.1](../../experiments-spec.md#s6); `AC-6.1-4` | primary only | planned; no execution evidence |
| REQ-XD-005 | adopted | [R6.75 AC](../../tickets/359_R6.75__fp-results-comparative.md#ac-r6-75-req-xd-005) | R | [§6.1](../../experiments-spec.md#s6); `AC-6.1-5` | R6.73 | planned; no execution evidence |
| REQ-XD-010 | adopted | [R6.47 AC](../../tickets/312_R6.47__fp-select.md#ac-r6-47-req-xd-010) | D | [§6.2](../../experiments-spec.md#s6); `AC-6.2-1` | primary only | planned; no execution evidence |
| REQ-XD-011 | adopted | [R6.18 AC](../../tickets/272_R6.18__select.md#ac-r6-18-req-xd-011) | E | [§6.2](../../experiments-spec.md#s6); `AC-6.2-2` | primary only | planned; no execution evidence |
| REQ-XD-012 | adopted | [R6.47 AC](../../tickets/312_R6.47__fp-select.md#ac-r6-47-req-xd-012) | E | [§6.2](../../experiments-spec.md#s6); `AC-6.2-3` | primary only | planned; no execution evidence |
| REQ-XD-013 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-013) | D | [§6.2](../../experiments-spec.md#s6); `AC-6.2-4` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): first pass draws a 10-task band-A sub-manifest (§10.1.6); tranche census is later-pass |
| REQ-XD-014 | adopted | [R6.47 AC](../../tickets/312_R6.47__fp-select.md#ac-r6-47-req-xd-014) | D | [§6.2](../../experiments-spec.md#s6); `AC-6.2-5` | primary only | planned; no execution evidence |
| REQ-XD-020 | adopted | [R6.29 AC](../../tickets/288_R6.29__comparative-protocol.md#ac-r6-29-req-xd-020) | E | [§6.3](../../experiments-spec.md#s6); `AC-6.3-1` | primary only | planned; no execution evidence |
| REQ-XD-021 | adopted | [R6.29 AC](../../tickets/288_R6.29__comparative-protocol.md#ac-r6-29-req-xd-021) | E | [§6.3](../../experiments-spec.md#s6); `AC-6.3-2` | primary only | planned; no execution evidence |
| REQ-XD-022 | adopted | [R6.30a AC](../../tickets/289_R6.30a__estimators.md#ac-r6-30a-req-xd-022) | E | [§6.4](../../experiments-spec.md#s6); `AC-6.4-1` | primary only | planned; no execution evidence |
| REQ-XD-023 | adopted | [R6.30b AC](../../tickets/290_R6.30b__method-dossier.md#ac-r6-30b-req-xd-023) | D | [§6.4](../../experiments-spec.md#s6); `AC-6.4-2` | primary only | planned; no execution evidence |
| REQ-XD-024 | adopted | [R6.30b AC](../../tickets/290_R6.30b__method-dossier.md#ac-r6-30b-req-xd-024) | E | [§6.4](../../experiments-spec.md#s6); `AC-6.4-3` | primary only | planned; no execution evidence |
| REQ-XD-030 | adopted | [R6.30a AC](../../tickets/289_R6.30a__estimators.md#ac-r6-30a-req-xd-030) | R | [§6.5](../../experiments-spec.md#s6); `AC-6.5-1` | primary only | planned; no execution evidence |
| REQ-XD-031 | adopted | [R6.30a AC](../../tickets/289_R6.30a__estimators.md#ac-r6-30a-req-xd-031) | D | [§6.5](../../experiments-spec.md#s6); `AC-6.5-2` | primary only | planned; no execution evidence |
| REQ-XD-040 | adopted | [R6.44 AC](../../tickets/309_R6.44__fp-continuation.md#ac-r6-44-req-xd-040) | E | [§6.6](../../experiments-spec.md#s6); `AC-6.6-3` | primary only | planned; no execution evidence |
| REQ-XD-041 | adopted | [R6.60 AC](../../tickets/337_R6.60__w2-run.md#ac-r6-60-req-xd-041) | R | [§6.6](../../experiments-spec.md#s6); `AC-6.6-4` | R6.38a, R6.44, R6.45 | planned; owed at fleshing (skeleton row; fleshing in place inherits this ownership) |
| REQ-XD-042 | adopted | [R6.44 AC](../../tickets/309_R6.44__fp-continuation.md#ac-r6-44-req-xd-042) | R | [§6.6](../../experiments-spec.md#s6); `AC-6.6-5` | primary only | planned; no execution evidence |
| REQ-XD-043 | adopted | [R6.72 AC](../../tickets/354_R6.72__fp-quote-e.md#ac-r6-72-req-xd-043) | E | [§6.6](../../experiments-spec.md#s6); `AC-6.6-6` | R6.60 | planned; no execution evidence |
| REQ-XD-044 | adopted | [R6.72 AC](../../tickets/354_R6.72__fp-quote-e.md#ac-r6-72-req-xd-044) | D | [§6.6](../../experiments-spec.md#s6); `AC-6.6-7` | primary only | planned; no execution evidence |
| REQ-XD-050 | adopted | [R6.29 AC](../../tickets/288_R6.29__comparative-protocol.md#ac-r6-29-req-xd-050) | E | [§6.7](../../experiments-spec.md#s6); `AC-6.7-1` | primary only | planned; no execution evidence |
| REQ-XD-051 | adopted | [R6.29 AC](../../tickets/288_R6.29__comparative-protocol.md#ac-r6-29-req-xd-051) | D | [§6.7](../../experiments-spec.md#s6); `AC-6.7-2` | primary only | planned; no execution evidence |
| REQ-XD-052 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-052) | D | [§6.7](../../experiments-spec.md#s6); `AC-6.7-3` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): tranche gates are later-pass; the first-pass analogue is FP-S/FP-E (REQ-XH-111) |
| REQ-XD-060 | adopted | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xd-060) | E | [§6.8](../../experiments-spec.md#s6); `AC-6.8-1` | primary only | planned; no execution evidence |
| REQ-XD-061 | adopted | [R6.30a AC](../../tickets/289_R6.30a__estimators.md#ac-r6-30a-req-xd-061) | R | [§6.8](../../experiments-spec.md#s6); `AC-6.8-2` | primary only | planned; no execution evidence |
| REQ-XD-062 | adopted | [R6.72 AC](../../tickets/354_R6.72__fp-quote-e.md#ac-r6-72-req-xd-062) | E | [§6.8](../../experiments-spec.md#s6); `AC-6.8-3` | R6.60 | planned; no execution evidence |
| REQ-XD-070 | adopted | [R6.40 AC](../../tickets/306_R6.40__answer-scan.md#ac-r6-40-req-xd-070) | E | [§6.9](../../experiments-spec.md#s6); `AC-6.9-1` | primary only | planned; no execution evidence |
| REQ-XD-071 | adopted | [HUMAN-H23 AC](../../tickets/361_HUMAN-H23__flagged-review.md#ac-human-h23-req-xd-071) | D | [§6.9](../../experiments-spec.md#s6); `AC-6.9-2` | R6.40 | planned; no execution evidence |
| REQ-XD-080 | merged-with(REQ-XC-302) | [R6.34a AC](../../tickets/297_R6.34a__admit-record.md#ac-r6-34a-req-xc-302) | E | [§5.1](../../experiments-spec.md#s5); `AC-5.1-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XD-081 | adopted | [R6.34a AC](../../tickets/297_R6.34a__admit-record.md#ac-r6-34a-req-xd-081) | D | [§5.1](../../experiments-spec.md#s5); `AC-5.1-5` | primary only | planned; no execution evidence |
| REQ-XD-082 | adopted | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xd-082) | E | [§6.10](../../experiments-spec.md#s6); `AC-6.10-1` | R6.36, R6.20a | planned; no execution evidence |
| REQ-XD-090 | adopted | [R6.75 AC](../../tickets/359_R6.75__fp-results-comparative.md#ac-r6-75-req-xd-090) | R | [§6.11](../../experiments-spec.md#s6); `AC-6.11-1` | R6.73 | planned; no execution evidence |
| REQ-XD-091 | merged-with(REQ-XF-134) | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-134) | E | [§9.4](../../experiments-spec.md#s9); `AC-9.4-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XD-092 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xd-092) | D | [§6.11](../../experiments-spec.md#s6); `AC-6.11-2` | primary only | planned; no execution evidence |
| REQ-XD-101 | adopted | [R6.66 AC](../../tickets/352_R6.66__l2-wiring.md#ac-r6-66-req-xd-101) | R | [§7.4](../../experiments-spec.md#s7); `AC-7.4-1` | primary only | planned; no execution evidence |
| REQ-XD-102 | adopted | [R6.57a AC](../../tickets/341_R6.57a__native-participant.md#ac-r6-57a-req-xd-102) | E | [§7.4](../../experiments-spec.md#s7); `AC-7.4-2` | R6.43 | planned; no execution evidence |
| REQ-XD-103 | adopted | [R6.43 AC](../../tickets/308_R6.43__l1-runner.md#ac-r6-43-req-xd-103) | E | [§7.4](../../experiments-spec.md#s7); `AC-7.4-3` | primary only | planned; no execution evidence |
| REQ-XD-104 | adopted | [R6.43 AC](../../tickets/308_R6.43__l1-runner.md#ac-r6-43-req-xd-104) | E | [§7.4](../../experiments-spec.md#s7); `AC-7.4-4` | primary only | planned; no execution evidence |
| REQ-XD-105 | adopted | [R6.41 AC](../../tickets/278_R6.41__l1-canon.md#ac-r6-41-req-xd-105) | E | [§7.4](../../experiments-spec.md#s7); `AC-7.4-5` | primary only | planned; no execution evidence |
| REQ-XD-106 | adopted | [R6.62 AC](../../tickets/339_R6.62__fp-record-partition.md#ac-r6-62-req-xd-106) | D | [§7.4](../../experiments-spec.md#s7); `AC-7.4-6` | primary only | planned; no execution evidence |
| REQ-XD-107 | adopted | [R6.68 AC](../../tickets/346_R6.68__w3-run.md#ac-r6-68-req-xd-107) | R | [§7.4](../../experiments-spec.md#s7); `AC-7.4-7` | R6.43, R6.48, R6.60 | planned; owed at fleshing (skeleton row; fleshing in place inherits this ownership) |
| REQ-XD-108 | merged-with(REQ-XC-122) | [R6.22 AC](../../tickets/279_R6.22__replay.md#ac-r6-22-req-xc-122) | E | [§4.9](../../experiments-spec.md#s4); `AC-4.9-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XD-109 | adopted | [R6.43 AC](../../tickets/308_R6.43__l1-runner.md#ac-r6-43-req-xd-109) | E | [§7.4](../../experiments-spec.md#s7); `AC-7.4-8` | primary only | planned; no execution evidence |
| REQ-XD-110 | adopted | [R6.66 AC](../../tickets/352_R6.66__l2-wiring.md#ac-r6-66-req-xd-110) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-1` | primary only | planned; no execution evidence |
| REQ-XD-111 | adopted | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xd-111) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-2` | R6.73 | planned; no execution evidence |
| REQ-XD-112 | adopted | [R6.66 AC](../../tickets/352_R6.66__l2-wiring.md#ac-r6-66-req-xd-112) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-3` | primary only | planned; no execution evidence |
| REQ-XD-113 | adopted | [R6.66 AC](../../tickets/352_R6.66__l2-wiring.md#ac-r6-66-req-xd-113) | D | [§7.5](../../experiments-spec.md#s7); `AC-7.5-4` | primary only | planned; no execution evidence |
| REQ-XD-114 | adopted | [R6.66 AC](../../tickets/352_R6.66__l2-wiring.md#ac-r6-66-req-xd-114) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-5` | primary only | planned; no execution evidence |
| REQ-XD-115 | adopted | [R6.58 AC](../../tickets/319_R6.58__fp-prereg.md#ac-r6-58-req-xd-115) | D | [§7.5](../../experiments-spec.md#s7); `AC-7.5-6` | primary only | planned; no execution evidence |
| REQ-XD-116 | adopted | [R6.66 AC](../../tickets/352_R6.66__l2-wiring.md#ac-r6-66-req-xd-116) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-7` | primary only | planned; no execution evidence |
| REQ-XD-117 | adopted | [R6.66 AC](../../tickets/352_R6.66__l2-wiring.md#ac-r6-66-req-xd-117) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-8` | primary only | planned; no execution evidence |
| REQ-XD-118 | adopted | [R6.66 AC](../../tickets/352_R6.66__l2-wiring.md#ac-r6-66-req-xd-118) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-9` | primary only | planned; no execution evidence |
| REQ-XD-119 | adopted | [R6.76 AC](../../tickets/360_R6.76__fp-results-parity.md#ac-r6-76-req-xd-119) | R | [§7.6](../../experiments-spec.md#s7); `AC-7.6-1` | R6.73 | planned; no execution evidence |
| REQ-XD-120 | adopted | [R6.76 AC](../../tickets/360_R6.76__fp-results-parity.md#ac-r6-76-req-xd-120) | R | [§7.6](../../experiments-spec.md#s7); `AC-7.6-2` | R6.73 | planned; no execution evidence |
| REQ-XD-121 | adopted | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xd-121) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-10` | primary only | planned; no execution evidence |
| REQ-XD-122 | merged-with(REQ-XE-001) | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xe-001) | D | [§7.1](../../experiments-spec.md#s7); `AC-7.1-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XD-123 | adopted | [R6.63 AC](../../tickets/342_R6.63__mini-hir.md#ac-r6-63-req-xd-123) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-11` | primary only | planned; no execution evidence |
| REQ-XD-124 | merged-with(REQ-XE-013) | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-013) | D | [§7.8](../../experiments-spec.md#s7); `AC-7.8-5` | primary only | alias: literal predicate retained at the target (REQ-XH-003); routed with target |
| REQ-XD-125 | merged-with(REQ-XE-125) | [R6.64 AC](../../tickets/343_R6.64__sweep-glue.md#ac-r6-64-req-xe-125) | E | [§8.2](../../experiments-spec.md#s8); `AC-8.2-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XD-126 | adopted | [R6.67 AC](../../tickets/353_R6.67__site-phase34.md#ac-r6-67-req-xd-126) | D | [§7.7](../../experiments-spec.md#s7); `AC-7.7-1` | primary only | planned; no execution evidence |
| REQ-XD-201 | adopted | [R6.65 AC](../../tickets/344_R6.65__d3-fp.md#ac-r6-65-req-xd-201) | E | [§8.1](../../experiments-spec.md#s8); `AC-8.1-1` | primary only | planned; no execution evidence |
| REQ-XD-202 | adopted | [R6.65 AC](../../tickets/344_R6.65__d3-fp.md#ac-r6-65-req-xd-202) | D | [§8.1](../../experiments-spec.md#s8); `AC-8.1-2` | primary only | planned; no execution evidence |
| REQ-XD-203 | merged-with(REQ-XE-125) | [R6.64 AC](../../tickets/343_R6.64__sweep-glue.md#ac-r6-64-req-xe-125) | E | [§8.2](../../experiments-spec.md#s8); `AC-8.2-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XD-204 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-204) | E | [§8.3](../../experiments-spec.md#s8); `AC-8.3-1` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-205 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-205) | E | [§8.3](../../experiments-spec.md#s8); `AC-8.3-2` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-206 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-206) | R | [§8.4](../../experiments-spec.md#s8); `AC-8.4-1` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-207 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-207) | R | [§8.4](../../experiments-spec.md#s8); `AC-8.4-2` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-208 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-208) | E | [§8.2](../../experiments-spec.md#s8); `AC-8.2-2` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-209 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-209) | E | [§8.2](../../experiments-spec.md#s8); `AC-8.2-3` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-210 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-210) | R | [§8.4](../../experiments-spec.md#s8); `AC-8.4-3` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-211 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-211) | D | [§8.5](../../experiments-spec.md#s8); `AC-8.5-1` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-212 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-212) | E | [§8.5](../../experiments-spec.md#s8); `AC-8.5-2` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-213 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-213) | D | [§8.5](../../experiments-spec.md#s8); `AC-8.5-3` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-214 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-214) | E | [§8.5](../../experiments-spec.md#s8); `AC-8.5-4` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-215 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-215) | R | [§8.6](../../experiments-spec.md#s8); `AC-8.6-1` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): confirmation window is later-pass (explore → confirm); the first-pass shared-baseline departure (§10.1.6 row "REQ-XD-215, REQ-XD-220", AR-33) is recorded by R6.58's pre-registration and cited here |
| REQ-XD-216 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-216) | R | [§8.6](../../experiments-spec.md#s8); `AC-8.6-2` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-217 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-217) | D | [§8.6](../../experiments-spec.md#s8); `AC-8.6-3` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-218 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-218) | E | [§8.6](../../experiments-spec.md#s8); `AC-8.6-4` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-219 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-219) | D | [§8.6](../../experiments-spec.md#s8); `AC-8.6-5` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-220 | adopted | [R6.65 AC](../../tickets/344_R6.65__d3-fp.md#ac-r6-65-req-xd-220) | E | [§8.7](../../experiments-spec.md#s8); `AC-8.7-1` | R6.57a, R6.58 | planned; no execution evidence |
| REQ-XD-221 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-221) | D | [§8.7](../../experiments-spec.md#s8); `AC-8.7-2` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XD-222 | adopted | [R6.67 AC](../../tickets/353_R6.67__site-phase34.md#ac-r6-67-req-xd-222) | D | [§8.8](../../experiments-spec.md#s8); `AC-8.8-1` | primary only | planned; no execution evidence |
| REQ-XD-223 | merged-with(REQ-XD-070) | [R6.40 AC](../../tickets/306_R6.40__answer-scan.md#ac-r6-40-req-xd-070) | E | [§6.9](../../experiments-spec.md#s6); `AC-6.9-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XD-224 | adopted | [R6.75 AC](../../tickets/359_R6.75__fp-results-comparative.md#ac-r6-75-req-xd-224) | D | [§8.8](../../experiments-spec.md#s8); `AC-8.8-2` | primary only | planned; no execution evidence |
| REQ-XD-225 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-225) | D | [§8.1](../../experiments-spec.md#s8); `AC-8.1-3` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-001 | adopted | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xe-001) | D | [§7.1](../../experiments-spec.md#s7); `AC-7.1-1` | primary only | planned; no execution evidence |
| REQ-XE-002 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-002) | D | [§7.8](../../experiments-spec.md#s7); `AC-7.8-1` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-003 | merged-with(REQ-XD-121) | [R6.35 AC](../../tickets/300_R6.35__adapt-chat.md#ac-r6-35-req-xd-121) | E | [§7.5](../../experiments-spec.md#s7); `AC-7.5-10` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XE-004 | adopted | [R6.42 AC](../../tickets/307_R6.42__l1-replay-corpus.md#ac-r6-42-req-xe-004) | E | [§7.1](../../experiments-spec.md#s7); `AC-7.1-2` | R6.48 | planned; no execution evidence |
| REQ-XE-005 | merged-with(REQ-XD-105) | [R6.41 AC](../../tickets/278_R6.41__l1-canon.md#ac-r6-41-req-xd-105) | E | [§7.4](../../experiments-spec.md#s7); `AC-7.4-5` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XE-006 | adopted | [R6.68 AC](../../tickets/346_R6.68__w3-run.md#ac-r6-68-req-xe-006) | R | [§7.3](../../experiments-spec.md#s7); `AC-7.3-20` | R6.63 | planned; owed at fleshing (skeleton row; fleshing in place inherits this ownership) |
| REQ-XE-007 | adopted | [R6.68 AC](../../tickets/346_R6.68__w3-run.md#ac-r6-68-req-xe-007) | R | [§7.3](../../experiments-spec.md#s7); `AC-7.3-21` | R6.53a, R6.63 | planned; owed at fleshing (skeleton row; fleshing in place inherits this ownership) |
| REQ-XE-008 | adopted | [R6.68 AC](../../tickets/346_R6.68__w3-run.md#ac-r6-68-req-xe-008) | R | [§7.3](../../experiments-spec.md#s7); `AC-7.3-22` | R6.54, R6.63 | planned; owed at fleshing (skeleton row; fleshing in place inherits this ownership) |
| REQ-XE-009 | adopted | [R6.68 AC](../../tickets/346_R6.68__w3-run.md#ac-r6-68-req-xe-009) | R | [§7.3](../../experiments-spec.md#s7); `AC-7.3-23` | R6.54, R6.63 | planned; owed at fleshing (skeleton row; fleshing in place inherits this ownership) |
| REQ-XE-010 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-010) | R | [§7.8](../../experiments-spec.md#s7); `AC-7.8-2` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-011 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-011) | R | [§7.8](../../experiments-spec.md#s7); `AC-7.8-3` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-012 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-012) | R | [§7.8](../../experiments-spec.md#s7); `AC-7.8-4` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-013 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-013) | D | [§7.8](../../experiments-spec.md#s7); `AC-7.8-5` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-014 | merged-with(REQ-XD-201) | [R6.65 AC](../../tickets/344_R6.65__d3-fp.md#ac-r6-65-req-xd-201) | E | [§8.1](../../experiments-spec.md#s8); `AC-8.1-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XE-101 | adopted | [R6.63 AC](../../tickets/342_R6.63__mini-hir.md#ac-r6-63-req-xe-101) | D | [§7.2](../../experiments-spec.md#s7); `AC-7.2-1` | primary only | planned; no execution evidence |
| REQ-XE-102 | adopted | [R6.63 AC](../../tickets/342_R6.63__mini-hir.md#ac-r6-63-req-xe-102) | D | [§7.2](../../experiments-spec.md#s7); `AC-7.2-2` | primary only | planned; no execution evidence |
| REQ-XE-103 | adopted | [R6.52 AC](../../tickets/323_R6.52__compiled-surface.md#ac-r6-52-req-xe-103) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-1` | primary only | planned; no execution evidence |
| REQ-XE-104 | adopted | [R6.52 AC](../../tickets/323_R6.52__compiled-surface.md#ac-r6-52-req-xe-104) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-2` | primary only | planned; no execution evidence |
| REQ-XE-105 | adopted | [R6.52a AC](../../tickets/324_R6.52a__surface-tools.md#ac-r6-52a-req-xe-105) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-3` | primary only | planned; no execution evidence |
| REQ-XE-106 | adopted | [R6.52a AC](../../tickets/324_R6.52a__surface-tools.md#ac-r6-52a-req-xe-106) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-4` | primary only | planned; no execution evidence |
| REQ-XE-107 | adopted | [R6.50 AC](../../tickets/321_R6.50__wire-fidelity.md#ac-r6-50-req-xe-107) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-5` | primary only | planned; no execution evidence |
| REQ-XE-108 | adopted | [R6.50 AC](../../tickets/321_R6.50__wire-fidelity.md#ac-r6-50-req-xe-108) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-6` | primary only | planned; no execution evidence |
| REQ-XE-109 | adopted | [R6.53b AC](../../tickets/326_R6.53b__observation-plumbing.md#ac-r6-53b-req-xe-109) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-7` | primary only | planned; no execution evidence |
| REQ-XE-110 | adopted | [R6.53a AC](../../tickets/325_R6.53a__typed-renderers.md#ac-r6-53a-req-xe-110) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-8` | primary only | planned; no execution evidence |
| REQ-XE-111 | adopted | [R6.53a AC](../../tickets/325_R6.53a__typed-renderers.md#ac-r6-53a-req-xe-111) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-9` | primary only | planned; no execution evidence |
| REQ-XE-112 | adopted | [R6.51 AC](../../tickets/322_R6.51__hh-tmpl.md#ac-r6-51-req-xe-112) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-10` | primary only | planned; no execution evidence |
| REQ-XE-113 | adopted | [R6.51 AC](../../tickets/322_R6.51__hh-tmpl.md#ac-r6-51-req-xe-113) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-11` | primary only | planned; no execution evidence |
| REQ-XE-114 | adopted | [R6.63 AC](../../tickets/342_R6.63__mini-hir.md#ac-r6-63-req-xe-114) | D | [§7.4](../../experiments-spec.md#s7); `AC-7.4-9` | primary only | planned; no execution evidence |
| REQ-XE-115 | adopted | [R6.54 AC](../../tickets/328_R6.54__strategy-submit.md#ac-r6-54-req-xe-115) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-12` | primary only | planned; no execution evidence |
| REQ-XE-116 | adopted | [R6.54 AC](../../tickets/328_R6.54__strategy-submit.md#ac-r6-54-req-xe-116) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-13` | primary only | planned; no execution evidence |
| REQ-XE-117 | adopted | [R6.54 AC](../../tickets/328_R6.54__strategy-submit.md#ac-r6-54-req-xe-117) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-14` | primary only | planned; no execution evidence |
| REQ-XE-118 | adopted | [R6.55 AC](../../tickets/329_R6.55__exec-embed.md#ac-r6-55-req-xe-118) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-15` | primary only | planned; no execution evidence |
| REQ-XE-119 | adopted | [R6.55a AC](../../tickets/330_R6.55a__exec-defaults.md#ac-r6-55a-req-xe-119) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-16` | primary only | planned; no execution evidence |
| REQ-XE-120 | adopted | [R6.57 AC](../../tickets/331_R6.57__native-topology.md#ac-r6-57-req-xe-120) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-17` | GATE-G26 | planned; no execution evidence |
| REQ-XE-121 | adopted | [R6.57a AC](../../tickets/341_R6.57a__native-participant.md#ac-r6-57a-req-xe-121) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-18` | primary only | planned; no execution evidence |
| REQ-XE-122 | adopted | [R6.56 AC](../../tickets/340_R6.56__compaction-slot.md#ac-r6-56-req-xe-122) | E | [§7.3](../../experiments-spec.md#s7); `AC-7.3-19` | primary only | planned; no execution evidence |
| REQ-XE-123 | adopted | [R6.63 AC](../../tickets/342_R6.63__mini-hir.md#ac-r6-63-req-xe-123) | D | [§7.2](../../experiments-spec.md#s7); `AC-7.2-3` | primary only | planned; no execution evidence |
| REQ-XE-124 | adopted | [R6.64 AC](../../tickets/343_R6.64__sweep-glue.md#ac-r6-64-req-xe-124) | E | [§8.1](../../experiments-spec.md#s8); `AC-8.1-4` | primary only | planned; no execution evidence |
| REQ-XE-125 | adopted | [R6.64 AC](../../tickets/343_R6.64__sweep-glue.md#ac-r6-64-req-xe-125) | E | [§8.2](../../experiments-spec.md#s8); `AC-8.2-1` | primary only | planned; no execution evidence |
| REQ-XE-126 | adopted | [R6.57a AC](../../tickets/341_R6.57a__native-participant.md#ac-r6-57a-req-xe-126) | E | [§7.2](../../experiments-spec.md#s7); `AC-7.2-4` | primary only | planned; no execution evidence |
| REQ-XE-127 | merged-with(REQ-XD-205) | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xd-205) | E | [§8.3](../../experiments-spec.md#s8); `AC-8.3-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); routed with target |
| REQ-XE-128 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-128) | E | [§7.8](../../experiments-spec.md#s7); `AC-7.8-6` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-129 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-129) | E | [§7.8](../../experiments-spec.md#s7); `AC-7.8-7` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-130 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-130) | E | [§7.8](../../experiments-spec.md#s7); `AC-7.8-8` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-131 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-131) | E | [§7.8](../../experiments-spec.md#s7); `AC-7.8-9` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XE-132 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xe-132) | D | [§7.8](../../experiments-spec.md#s7); `AC-7.8-10` | primary only | routed to round-8 by CAP6.1 (REC6.2 re-reads): later-pass scope per §10.6 / §10.1.6; Round 6 builds no implementing row |
| REQ-XF-001 | merged-with(REQ-XF-121) | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-121) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-5` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-002 | merged-with(REQ-XF-118) | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-118) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-003 | merged-with(REQ-XF-109) | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-109) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-004 | merged-with(REQ-XF-110) | [R6.32a AC](../../tickets/294_R6.32a__site-leaderboard.md#ac-r6-32a-req-xf-110) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-005 | merged-with(REQ-XF-105) | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-105) | E | [§9.1](../../experiments-spec.md#s9); `AC-9.1-5` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-006 | merged-with(REQ-XF-134) | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-134) | E | [§9.4](../../experiments-spec.md#s9); `AC-9.4-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-007 | merged-with(REQ-XF-116) | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-116) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-9` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-008 | merged-with(REQ-XF-109) | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-109) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-009 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-009) | E | [§9.5](../../experiments-spec.md#s9); `AC-9.5-1` | primary only | planned; no execution evidence |
| REQ-XF-010 | merged-with(REQ-XF-137) | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-137) | E | [§9.5](../../experiments-spec.md#s9); `AC-9.5-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-011 | merged-with(REQ-XF-109) | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-109) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-012 | merged-with(REQ-XF-109) | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-109) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-013 | merged-with(REQ-XF-112) | [R6.32b AC](../../tickets/295_R6.32b__site-noise-quote.md#ac-r6-32b-req-xf-112) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-5` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-014 | merged-with(REQ-XF-108) | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-108) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-1` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-015 | merged-with(REQ-XF-131) | [R6.33 AC](../../tickets/296_R6.33__site-grid-viewer.md#ac-r6-33-req-xf-131) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-11` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-016 | merged-with(REQ-XF-132) | [R6.33 AC](../../tickets/296_R6.33__site-grid-viewer.md#ac-r6-33-req-xf-132) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-12` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-017 | merged-with(REQ-XF-139) | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-139) | E | [§9.5](../../experiments-spec.md#s9); `AC-9.5-4` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-018 | merged-with(REQ-XF-114) | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-114) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-7` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-019 | merged-with(REQ-XF-127) | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-127) | E | [§9.4](../../experiments-spec.md#s9); `AC-9.4-2` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-020 | merged-with(REQ-XF-107) | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-107) | E | [§9.1](../../experiments-spec.md#s9); `AC-9.1-7` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-021 | adopted | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-021) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-10` | primary only | planned; no execution evidence |
| REQ-XF-022 | merged-with(REQ-XF-138) | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-138) | E | [§9.5](../../experiments-spec.md#s9); `AC-9.5-3` | primary only | alias: literal predicate retained at the target (REQ-XH-003); inherits target owner |
| REQ-XF-101 | adopted | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-101) | D | [§9.1](../../experiments-spec.md#s9); `AC-9.1-1` | primary only | planned; no execution evidence |
| REQ-XF-102 | adopted | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-102) | E | [§9.1](../../experiments-spec.md#s9); `AC-9.1-2` | primary only | planned; no execution evidence |
| REQ-XF-103 | adopted | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-103) | E | [§9.1](../../experiments-spec.md#s9); `AC-9.1-3` | primary only | planned; no execution evidence |
| REQ-XF-104 | adopted | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-104) | E | [§9.1](../../experiments-spec.md#s9); `AC-9.1-4` | primary only | planned; no execution evidence |
| REQ-XF-105 | adopted | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-105) | E | [§9.1](../../experiments-spec.md#s9); `AC-9.1-5` | primary only | planned; no execution evidence |
| REQ-XF-106 | adopted | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-106) | E | [§9.1](../../experiments-spec.md#s9); `AC-9.1-6` | primary only | planned; no execution evidence |
| REQ-XF-107 | adopted | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-107) | E | [§9.1](../../experiments-spec.md#s9); `AC-9.1-7` | primary only | planned; no execution evidence |
| REQ-XF-108 | adopted | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-108) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-1` | primary only | planned; no execution evidence |
| REQ-XF-109 | adopted | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-109) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-2` | primary only | planned; no execution evidence |
| REQ-XF-110 | adopted | [R6.32a AC](../../tickets/294_R6.32a__site-leaderboard.md#ac-r6-32a-req-xf-110) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-3` | primary only | planned; no execution evidence |
| REQ-XF-111 | adopted | [R6.32a AC](../../tickets/294_R6.32a__site-leaderboard.md#ac-r6-32a-req-xf-111) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-4` | primary only | planned; no execution evidence |
| REQ-XF-112 | adopted | [R6.32b AC](../../tickets/295_R6.32b__site-noise-quote.md#ac-r6-32b-req-xf-112) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-5` | primary only | planned; no execution evidence |
| REQ-XF-113 | adopted | [R6.32b AC](../../tickets/295_R6.32b__site-noise-quote.md#ac-r6-32b-req-xf-113) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-6` | primary only | planned; no execution evidence |
| REQ-XF-114 | adopted | [R6.7 AC](../../tickets/259_R6.7__site-shell.md#ac-r6-7-req-xf-114) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-7` | primary only | planned; no execution evidence |
| REQ-XF-115 | adopted | [R6.32a AC](../../tickets/294_R6.32a__site-leaderboard.md#ac-r6-32a-req-xf-115) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-8` | primary only | planned; no execution evidence |
| REQ-XF-116 | adopted | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-116) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-9` | primary only | planned; no execution evidence |
| REQ-XF-117 | adopted | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-117) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-1` | primary only | planned; no execution evidence |
| REQ-XF-118 | adopted | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-118) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-2` | primary only | planned; no execution evidence |
| REQ-XF-119 | adopted | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-119) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-3` | primary only | planned; no execution evidence |
| REQ-XF-120 | adopted | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-120) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-4` | primary only | planned; no execution evidence |
| REQ-XF-121 | adopted | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-121) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-5` | primary only | planned; no execution evidence |
| REQ-XF-122 | adopted | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-122) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-6` | primary only | planned; no execution evidence |
| REQ-XF-123 | adopted | [R6.30a AC](../../tickets/289_R6.30a__estimators.md#ac-r6-30a-req-xf-123) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-7` | primary only | planned; no execution evidence |
| REQ-XF-124 | adopted | [R6.31a AC](../../tickets/292_R6.31a__archive-index.md#ac-r6-31a-req-xf-124) | E | [§4.10](../../experiments-spec.md#s4); `AC-4.10-1` | primary only | planned; no execution evidence |
| REQ-XF-125 | adopted | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-125) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-8` | primary only | planned; no execution evidence |
| REQ-XF-126 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-126) | E | [§9.4](../../experiments-spec.md#s9); `AC-9.4-1` | primary only | planned; no execution evidence |
| REQ-XF-127 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-127) | E | [§9.4](../../experiments-spec.md#s9); `AC-9.4-2` | primary only | planned; no execution evidence |
| REQ-XF-128 | adopted | [R6.11 AC](../../tickets/264_R6.11__pages-workflow.md#ac-r6-11-req-xf-128) | E | [§9.6](../../experiments-spec.md#s9); `AC-9.6-1` | primary only | planned; no execution evidence |
| REQ-XF-129 | adopted | [R6.11 AC](../../tickets/264_R6.11__pages-workflow.md#ac-r6-11-req-xf-129) | E | [§9.6](../../experiments-spec.md#s9); `AC-9.6-2` | primary only | planned; no execution evidence |
| REQ-XF-130 | adopted | [R6.11 AC](../../tickets/264_R6.11__pages-workflow.md#ac-r6-11-req-xf-130) | D | [§9.6](../../experiments-spec.md#s9); `AC-9.6-3` | primary only | planned; no execution evidence |
| REQ-XF-131 | adopted | [R6.33 AC](../../tickets/296_R6.33__site-grid-viewer.md#ac-r6-33-req-xf-131) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-11` | primary only | planned; no execution evidence |
| REQ-XF-132 | adopted | [R6.33 AC](../../tickets/296_R6.33__site-grid-viewer.md#ac-r6-33-req-xf-132) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-12` | primary only | planned; no execution evidence |
| REQ-XF-133 | adopted | [R6.33 AC](../../tickets/296_R6.33__site-grid-viewer.md#ac-r6-33-req-xf-133) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-13` | primary only | planned; no execution evidence |
| REQ-XF-134 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-134) | E | [§9.4](../../experiments-spec.md#s9); `AC-9.4-3` | primary only | planned; no execution evidence |
| REQ-XF-135 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-135) | E | [§9.4](../../experiments-spec.md#s9); `AC-9.4-4` | primary only | planned; no execution evidence |
| REQ-XF-136 | adopted | [R6.12 AC](../../tickets/315_R6.12__pub-p1-record.md#ac-r6-12-req-xf-136) | D | [§9.6](../../experiments-spec.md#s9); `AC-9.6-4` | primary only | planned; no execution evidence |
| REQ-XF-137 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-137) | E | [§9.5](../../experiments-spec.md#s9); `AC-9.5-2` | primary only | planned; no execution evidence |
| REQ-XF-138 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xf-138) | E | [§9.5](../../experiments-spec.md#s9); `AC-9.5-3` | primary only | planned; no execution evidence |
| REQ-XF-139 | adopted | [R6.8 AC](../../tickets/261_R6.8__site-generator.md#ac-r6-8-req-xf-139) | E | [§9.5](../../experiments-spec.md#s9); `AC-9.5-4` | primary only | planned; no execution evidence |
| REQ-XF-140 | adopted | [R6.11 AC](../../tickets/264_R6.11__pages-workflow.md#ac-r6-11-req-xf-140) | E | [§9.5](../../experiments-spec.md#s9); `AC-9.5-5` | primary only | planned; no execution evidence |
| REQ-XF-141 | adopted | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xf-141) | E | [§9.2](../../experiments-spec.md#s9); `AC-9.2-14` | primary only | planned; no execution evidence |
| REQ-XF-142 | adopted | [R6.11 AC](../../tickets/264_R6.11__pages-workflow.md#ac-r6-11-req-xf-142) | E | [§9.6](../../experiments-spec.md#s9); `AC-9.6-5` | primary only | planned; no execution evidence |
| REQ-XF-143 | adopted (synthesis-allocated) | [R6.67 AC](../../tickets/353_R6.67__site-phase34.md#ac-r6-67-req-xf-143) | E | [§9.3](../../experiments-spec.md#s9); `AC-9.3-9` | primary only | planned; no execution evidence |
| REQ-XH-001 | adopted | [R6.1 AC](../../tickets/253_R6.1__guard-x.md#ac-r6-1-req-xh-001) | D | [§10.6](../../experiments-spec.md#s10); `AC-10.6-1` | primary only | planned; no execution evidence |
| REQ-XH-002 | adopted | [R6.1 AC](../../tickets/253_R6.1__guard-x.md#ac-r6-1-req-xh-002) | E | [§10.6](../../experiments-spec.md#s10); `AC-10.6-2` | R6.1a | planned; no execution evidence |
| REQ-XH-003 | adopted | [R6.1 AC](../../tickets/253_R6.1__guard-x.md#ac-r6-1-req-xh-003) | E | [§10.6](../../experiments-spec.md#s10); `AC-10.6-3` | primary only | planned; no execution evidence |
| REQ-XH-004 | adopted | [R6.1a AC](../../tickets/254_R6.1a__guard-rules.md#ac-r6-1a-req-xh-004) | D | [§10.3](../../experiments-spec.md#s10); `AC-10.3-1` | GATE-G25, GATE-G26, GATE-G27, GATE-G28, GATE-G30 | planned; no execution evidence |
| REQ-XH-005 | adopted | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xh-005) | E | [§4.4](../../experiments-spec.md#s4); `AC-4.4-8` | primary only | planned; no execution evidence |
| REQ-XH-006 | adopted | [R6.25 AC](../../tickets/284_R6.25__cost-record.md#ac-r6-25-req-xh-006) | E | [§3.10](../../experiments-spec.md#s3); `AC-3.10-6` | primary only | planned; no execution evidence |
| REQ-XH-007 | adopted | [GATE-G26 AC](../../tickets/335_GATE-G26__fp-s.md#ac-gate-g26-req-xh-007) | D | [§4.4](../../experiments-spec.md#s4); `AC-4.4-9` | GATE-G30 | planned; no execution evidence |
| REQ-XH-008 | adopted | [R6.1a AC](../../tickets/254_R6.1a__guard-rules.md#ac-r6-1a-req-xh-008) | D | [§3.9](../../experiments-spec.md#s3); `AC-3.9-15` | R6.19b | planned; no execution evidence |
| REQ-XH-009 | adopted | [R6.19b AC](../../tickets/274_R6.19b__recipe-teardown.md#ac-r6-19b-req-xh-009) | D | [§3.9](../../experiments-spec.md#s3); `AC-3.9-16` | primary only | planned; no execution evidence |
| REQ-XH-010 | adopted | [R6.60 AC](../../tickets/337_R6.60__w2-run.md#ac-r6-60-req-xh-010) | R | [§6.6](../../experiments-spec.md#s6); `AC-6.6-1` | R6.38a, R6.72 | planned; owed at fleshing (skeleton row; fleshing in place inherits this ownership) |
| REQ-XH-011 | adopted | [R6.62 AC](../../tickets/339_R6.62__fp-record-partition.md#ac-r6-62-req-xh-011) | D | [§6.6](../../experiments-spec.md#s6); `AC-6.6-2` | primary only | planned; no execution evidence |
| REQ-XH-012 | adopted | [R6.18 AC](../../tickets/272_R6.18__select.md#ac-r6-18-req-xh-012) | D | [§6.12](../../experiments-spec.md#s6); `AC-6.12-1` | primary only | planned; no execution evidence |
| REQ-XH-013 | adopted | [GATE-G26 AC](../../tickets/335_GATE-G26__fp-s.md#ac-gate-g26-req-xh-013) | D | [§4.4](../../experiments-spec.md#s4); `AC-4.4-10` | GATE-G30, HUMAN-H15 | planned; no execution evidence |
| REQ-XH-014 | adopted | [R6.27 AC](../../tickets/286_R6.27__harbor-o.md#ac-r6-27-req-xh-014) | D | [§5.9](../../experiments-spec.md#s5); `AC-5.9-1` | primary only | planned; no execution evidence |
| REQ-XH-015 | adopted | [REC6.2 AC](../../tickets/376_REC6.2__rec2.md#ac-rec6-2-req-xh-015) | D | [§10.3](../../experiments-spec.md#s10); `AC-10.3-2` | primary only | planned; no execution evidence |
| REQ-XH-016 | adopted | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xh-016) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-7` | primary only | planned; no execution evidence |
| REQ-XH-017 | adopted | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xh-017) | E | [§3.3](../../experiments-spec.md#s3); `AC-3.3-8` | primary only | planned; no execution evidence |
| REQ-XH-018 | adopted | [R6.11 AC](../../tickets/264_R6.11__pages-workflow.md#ac-r6-11-req-xh-018) | D | [§9.6](../../experiments-spec.md#s9); `AC-9.6-6` | primary only | planned; no execution evidence |
| REQ-XH-019 | adopted | [R6.6 AC](../../tickets/258_R6.6__p1-record.md#ac-r6-6-req-xh-019) | D | [§9.6](../../experiments-spec.md#s9); `AC-9.6-7` | primary only | planned; no execution evidence |
| REQ-XH-020 | adopted | [R6.9 AC](../../tickets/262_R6.9__site-phase1.md#ac-r6-9-req-xh-020) | E | [§9.6](../../experiments-spec.md#s9); `AC-9.6-8` | primary only | planned; no execution evidence |
| REQ-XH-021 | adopted | [CAP6.3 AC](../../tickets/373_CAP6.3__cap3.md#ac-cap6-3-req-xh-021) | D | [§4.7](../../experiments-spec.md#s4); `AC-4.7-3` | R6.25, R6.26 | planned; no execution evidence |
| REQ-XH-022 | adopted | [R6.10 AC](../../tickets/263_R6.10__claims-checker.md#ac-r6-10-req-xh-022) | D | [§9.4](../../experiments-spec.md#s9); `AC-9.4-5` | primary only | planned; no execution evidence |
| REQ-XH-023 | adopted | [R6.19b AC](../../tickets/274_R6.19b__recipe-teardown.md#ac-r6-19b-req-xh-023) | E | [§3.11](../../experiments-spec.md#s3); `AC-3.11-2` | primary only | planned; no execution evidence |
| REQ-XH-024 | adopted | [R6.1a AC](../../tickets/254_R6.1a__guard-rules.md#ac-r6-1a-req-xh-024) | D | [§10.3](../../experiments-spec.md#s10); `AC-10.3-3` | primary only | planned; no execution evidence |
| REQ-XH-025 | adopted | [R6.1a AC](../../tickets/254_R6.1a__guard-rules.md#ac-r6-1a-req-xh-025) | D | [§10.3](../../experiments-spec.md#s10); `AC-10.3-4` | primary only | planned; no execution evidence |
| REQ-XH-026 | adopted | [R6.1a AC](../../tickets/254_R6.1a__guard-rules.md#ac-r6-1a-req-xh-026) | D | [§10.3](../../experiments-spec.md#s10); `AC-10.3-5` | primary only | planned; no execution evidence |
| REQ-XH-027 | adopted | [R6.28 AC](../../tickets/287_R6.28__reproof-kit.md#ac-r6-28-req-xh-027) | D | [§10.3](../../experiments-spec.md#s10); `AC-10.3-6` | R6.38a, R6.45, R6.48 | planned; no execution evidence |
| REQ-XH-028 | adopted | [R6.4 AC](../../tickets/257_R6.4__p1-corrections.md#ac-r6-4-req-xh-028) | D | [§2.4](../../experiments-spec.md#s2); `AC-2.4-6` | primary only | planned; no execution evidence |
| REQ-XH-029 | adopted | [REC6.1 AC](../../tickets/375_REC6.1__rec1.md#ac-rec6-1-req-xh-029) | D | [§10.6](../../experiments-spec.md#s10); `AC-10.6-4` | primary only | planned; no execution evidence |
| REQ-XH-030 | adopted | [R6.29 AC](../../tickets/288_R6.29__comparative-protocol.md#ac-r6-29-req-xh-030) | E | [§6.7](../../experiments-spec.md#s6); `AC-6.7-4` | primary only | planned; no execution evidence |
| REQ-XH-031 | adopted | [R6.39 AC](../../tickets/305_R6.39__runner-check.md#ac-r6-39-req-xh-031) | D | [§7.5](../../experiments-spec.md#s7); `AC-7.5-12` | primary only | planned; no execution evidence |
| REQ-XH-032 | adopted | [R6.3 AC](../../tickets/256_R6.3__p1-hardening.md#ac-r6-3-req-xh-032) | E | [§2.3](../../experiments-spec.md#s2); `AC-2.3-3` | primary only | planned; no execution evidence |
| REQ-XH-033 | adopted | [CAP6.1 AC](../../tickets/371_CAP6.1__cap1.md#ac-cap6-1-req-xh-033) | R | [§3.8](../../experiments-spec.md#s3); `AC-3.8-3` | primary only | routed to round-7 by CAP6.1 (REC6.2 re-reads): first pass governed by REQ-XH-116 (FP-7); the 87-task re-proof precedes any later-pass draw |
| REQ-XH-101 | adopted (synthesis-allocated) | [R6.31a AC](../../tickets/292_R6.31a__archive-index.md#ac-r6-31a-req-xh-101) | E | [§4.10](../../experiments-spec.md#s4); `AC-4.10-2` | primary only | planned; no execution evidence |
| REQ-XH-102 | adopted (synthesis-allocated) | [R6.31b AC](../../tickets/293_R6.31b__archive-upload-kit.md#ac-r6-31b-req-xh-102) | E | [§4.10](../../experiments-spec.md#s4); `AC-4.10-3` | R6.59, R6.61, R6.69, R6.71, R6.74 | planned; no execution evidence |
| REQ-XH-103 | adopted (synthesis-allocated) | [R6.31b AC](../../tickets/293_R6.31b__archive-upload-kit.md#ac-r6-31b-req-xh-103) | E | [§4.10](../../experiments-spec.md#s4); `AC-4.10-4` | R6.24c | planned; no execution evidence |
| REQ-XH-104 | adopted (synthesis-allocated) | [R6.31b AC](../../tickets/293_R6.31b__archive-upload-kit.md#ac-r6-31b-req-xh-104) | E | [§4.10](../../experiments-spec.md#s4); `AC-4.10-5` | primary only | planned; no execution evidence |
| REQ-XH-105 | adopted (synthesis-allocated) | [R6.78 AC](../../tickets/365_R6.78__pub-fp-record.md#ac-r6-78-req-xh-105) | P | [§4.10](../../experiments-spec.md#s4); `AC-4.10-6` | GATE-G31, HUMAN-H24 | planned; no execution evidence |
| REQ-XH-106 | adopted (synthesis-allocated) | [REC6.1 AC](../../tickets/375_REC6.1__rec1.md#ac-rec6-1-req-xh-106) | D | [§4.10](../../experiments-spec.md#s4); `AC-4.10-7` | primary only | planned; no execution evidence |
| REQ-XH-107 | adopted (synthesis-allocated) | [R6.31b AC](../../tickets/293_R6.31b__archive-upload-kit.md#ac-r6-31b-req-xh-107) | E | [§4.10](../../experiments-spec.md#s4); `AC-4.10-8` | primary only | planned; no execution evidence |
| REQ-XH-110 | adopted (synthesis-allocated) | [CAP6.3 AC](../../tickets/373_CAP6.3__cap3.md#ac-cap6-3-req-xh-110) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-1` | R6.25, HUMAN-H14, HUMAN-H18, HUMAN-H19, HUMAN-H20, HUMAN-H22 | planned; no execution evidence |
| REQ-XH-111 | adopted (synthesis-allocated) | [R6.58 AC](../../tickets/319_R6.58__fp-prereg.md#ac-r6-58-req-xh-111) | R | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-2` | R6.73 | planned; no execution evidence |
| REQ-XH-112 | adopted (synthesis-allocated) | [R6.44 AC](../../tickets/309_R6.44__fp-continuation.md#ac-r6-44-req-xh-112) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-3` | R6.73 | planned; no execution evidence |
| REQ-XH-113 | adopted (synthesis-allocated) | [R6.58 AC](../../tickets/319_R6.58__fp-prereg.md#ac-r6-58-req-xh-113) | R | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-4` | R6.73, R6.76 | planned; no execution evidence |
| REQ-XH-114 | adopted (synthesis-allocated) | [R6.62 AC](../../tickets/339_R6.62__fp-record-partition.md#ac-r6-62-req-xh-114) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-5` | R6.60, R6.57a | planned; no execution evidence |
| REQ-XH-115 | adopted (synthesis-allocated) | [R6.58 AC](../../tickets/319_R6.58__fp-prereg.md#ac-r6-58-req-xh-115) | R | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-6` | R6.65, R6.73 | planned; no execution evidence |
| REQ-XH-116 | adopted (synthesis-allocated) | [R6.48 AC](../../tickets/318_R6.48__w1-run.md#ac-r6-48-req-xh-116) | R | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-7` | R6.47, R6.58 | planned; owed at fleshing (skeleton row; fleshing in place inherits this ownership) |
| REQ-XH-117 | adopted (synthesis-allocated) | [R6.44 AC](../../tickets/309_R6.44__fp-continuation.md#ac-r6-44-req-xh-117) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-8` | primary only | planned; no execution evidence |
| REQ-XH-118 | adopted (synthesis-allocated) | [R6.25 AC](../../tickets/284_R6.25__cost-record.md#ac-r6-25-req-xh-118) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-9` | R6.47, R6.72 | planned; no execution evidence |
| REQ-XH-119 | adopted (synthesis-allocated) | [R6.67 AC](../../tickets/353_R6.67__site-phase34.md#ac-r6-67-req-xh-119) | D | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-10` | primary only | planned; no execution evidence |
| REQ-XH-120 | adopted (synthesis-allocated) | [R6.21 AC](../../tickets/277_R6.21__budget.md#ac-r6-21-req-xh-120) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-11` | R6.24c, GATE-G26, GATE-G30 | planned; no execution evidence |
| REQ-XH-121 | adopted (synthesis-allocated) | [R6.46 AC](../../tickets/311_R6.46__fp-seal.md#ac-r6-46-req-xh-121) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-12` | R6.62 | planned; no execution evidence |
| REQ-XH-122 | adopted (synthesis-allocated) | [R6.67 AC](../../tickets/353_R6.67__site-phase34.md#ac-r6-67-req-xh-122) | D | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-13` | primary only | planned; no execution evidence |
| REQ-XH-123 | adopted (synthesis-allocated) | [R6.45 AC](../../tickets/310_R6.45__fp-holds.md#ac-r6-45-req-xh-123) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-14` | R6.72, R6.73 | planned; no execution evidence |
| REQ-XH-124 | adopted (synthesis-allocated) | [R6.45 AC](../../tickets/310_R6.45__fp-holds.md#ac-r6-45-req-xh-124) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-15` | R6.73 | planned; no execution evidence |
| REQ-XH-125 | adopted (synthesis-allocated) | [R6.15c AC](../../tickets/269_R6.15c__runner-schedule.md#ac-r6-15c-req-xh-125) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-16` | primary only | planned; no execution evidence |
| REQ-XH-126 | adopted (synthesis-allocated) | [R6.44 AC](../../tickets/309_R6.44__fp-continuation.md#ac-r6-44-req-xh-126) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-17` | R6.72 | planned; no execution evidence |
| REQ-XH-127 | adopted (synthesis-allocated) | [R6.45 AC](../../tickets/310_R6.45__fp-holds.md#ac-r6-45-req-xh-127) | E | [§10.1](../../experiments-spec.md#s10-1); `AC-10.1-18` | primary only | planned; no execution evidence |
