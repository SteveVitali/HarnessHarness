# GATE-G27 — PROV-W3: authorize the W3 L1 window or record why it does not run · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim from the spec)

Spec §10.3 gate table: "**PROV-W3 + G-L1** | first | E2-T10, K4, partition commit | compute quote, listing
the H-set build and export and the load; frozen H record named (the H-set digest frozen at G-L1,
REQ-XC-330); `hh-l1-conformance/1`: first divergence fails, BM1–BM19 coverage complete, census complete,
rule table frozen; pass or fail only". **This gate carries the PROV-W3 part**; G-L1 is GATE-G29's.

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the
model authorization, the compute authorization (separately), the quote id and the hard-cap formula.

## Evidence (per item: source, `date -u` of the read, layer reached)

- W3 quote — `docs/build/reports/2026-11-08-fp-w3-quote.json` — read 2026-11-11 — layer: committed record.
- W3 declaration — `scripts/gce-backend/declarations/fp-w3.json` (`authorization.gate PROV-W3`,
  `model_line: none`, quote `pq-c0890eab0910fbc0`, ceiling 2,408,510 µ$, `verbatim_sha256` unstamped) —
  read 2026-11-11.
- W2 closure — `docs/build/runs/R6.40.md`: harvest verified, index `archive_verified` (rev `7667c207…`),
  SETTLE-W2 `pending` clock-honest, proxy ledger harvested (`run-root/proxy-store/` + `usage.txt` in the
  verified manifest) — layer: committed records.
- Inputs — R6.42 partition `f7ddbc1f…` + dev projection `07042712…` (committed); R6.43 sweep registry +
  V5 seal (committed); R6.28 K2–K4 + `hh-l1-conformance/1` (landed); R6.41 E2-T7/E2-T8(b) (landed);
  R6.23 carried-set kit (landed) — layer: merged records.
- Frozen-list state — `docs/build/reports/2026-11-10-fp-prereg.json` `task_lists.state: blocked` — all 20
  W1 re-proofs `o_fail` — machinery-failure evidence, not agreement data; unchanged since R6.40.
- Open deferrals — `bash scripts/deferrals-open.sh`: DF-R6.23-1 OPEN (gate-citation predicate; this gate
  cites W2's `archive_verified` index — its GATE-G27 share; row stays OPEN for R6.44/Wc/W4).

<!-- agent-drafted:begin sha256=7f2502c40d0f5c59 -->
## Decision content for the operator

**The honest caveat — same predicate as W2.** `task_lists.state: blocked` is unchanged — every dev/held
slot sits `blocked{un_reproofed_successor}`/`blocked{no_adjacent_donor}` (all 20 W1 re-proofs `o_fail`;
no O-surface repair/Wc re-proof has run). W3's L1 corpus leg (its main purpose — the 28-trajectory
`hh-l1-conformance/1` input for G-L1) is a subject leg and withholds verbatim under the predicate
(`withheld{un_reproofed_successor}`), exactly as W2's PP0/corpus did. **W3's non-subject work — the
H-set build + export + the carried load — runs regardless.** So `run` produces the H-set artifact +
load + a withheld corpus + teardown; the corpus (and therefore G-L1's input) still needs the O-surface
repair + Wc re-proof to produce a frozen list. If W3 runs before the list freezes, a corpus re-run is
needed later for G-L1 — the authorize-now ruling at GATE-G26 set this precedent.

**1. Authorizations (REQ-XH-004).** W3 is **$0 model** (§10.1.4 W3 row — the L1 corpus measures the
harness's model-view identity on a replay/deterministic model, not metered calls; `model_line: none` in
the declaration). **Compute** authorization verbatim and separate: quote **`pq-c0890eab0910fbc0`**,
ceiling **2,408,510 µ$ (~$2.41)**; §3.10 hard-cap `r·(D + 1 h restart) + S·r_disk·h_r + 0.12·E_gib` =
`314,140·6.5 + 300·137·6 + 0.12·1` = 2,041,910 + 246,600 + 120,000 = **2,408,510 µ$**.

**2. Quote content (REQ-XH-118).** Planning 1,413,630 µ$; bound 3,889,680 µ$; ceiling 2,408,510 µ$ with
restart (1 h), h_r (6 h) and egress (1 GiB) terms; the quote lists the **H-set build and export** and
the **load** (of the W1-carried infrastructure set) explicitly; **C = 4** (memory 26/6 GiB, vCPU 8,
in-flight 4, wait 0.5); **W_p ≤ D − 0.65 h** admission cutoff; **Σ five windows' ceilings = 19,793,630 µ$
≤ 20,000,000** ($20, Q-X54) — W1 at billed ~2,124,750 µ$ + W2 at ~201,242 µ$ actuals.

**3. Dead-man and restart (REQ-XB-224).** One harvest-and-teardown-only restart ≤ 1 h inside the ceiling;
h_r = 6 h teardown deadline; **no `deliver-secret`** ($0 window — no model key).

**4. Frozen H record named (REQ-XC-330).** The H set built once at W3's start — the **`hh-kernel`
participant build + the mini-swe-agent HIR + compiled artifacts + the V5 phase-4 level** (R6.43's
`2026-11-11-fp-sweep-v5.json`, `LevelSpec.ref_` sealed) — carried as a `hh-carried-set` with its
`set_digest` **frozen by digest at G-L1**; every W4 HH-native/V5 slot records the H-set digest; a rebuild
is a new participant and needs a gate.

**5. Inputs landed and cited.** R6.42 partition commit `f7ddbc1f…` (dev/gate-A/gate-B digests) + sealed
dev projection `07042712…`; R6.43 `sweep-registry/1` V1–V9 + V5 `LevelSpec.ref_` + declared-delta/liveness
refusals; R6.28 K2–K4 + `hh-l1-conformance/1`; R6.41 E2-T7/E2-T8(b) HH-native participant; R6.23
carried-set kit — all merged.

**6. Previous window closed.** W2's harvest manifest verified; index `archive_verified` (revision
`7667c207cbdec4cd196d591e359cf79f668c6869`); settlement `pending` clock-honest (never early-settled);
proxy ledger harvested (`run-root/proxy-store/` + `usage.txt` in the verified manifest). **This gate
cites W2's `archive_verified` index — DF-R6.23-1's GATE-G27 share; the row stays OPEN for R6.44/Wc/W4.**

**Dispositions.** `run` — items 1–6 met and authorized verbatim; R6.44 fleshes for the W3 body (H-set
build/export + load + the corpus leg under the frozen-list predicate). `not_run{fallback}` — a Q-X62
trigger record exists (none does — the 14-day trigger fires 2026-11-24, not yet). `not_run{pp0_hold}` —
a held original mini-swe-agent withheld the corpus (W2's PP0 withheld `un_reproofed_successor`, not an
AG/infra hold — the predicates differ). Under either `not_run`, R6.44 fleshes to `unused{…}`.
Your verbatim answer dispositions it — `run` continues the authorize-now pattern (H-set built, corpus
withheld); a hold/refusal stops the W3 window until the frozen list exists.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-11T08:00:03Z, channel: interactive): "you're authorized to proceed"
Disposition: PASSED — run; basis: "items 1–6 met and authorized verbatim; R6.44 fleshes for the W3 body"
Recorded verbatim: W3 $0 model (model_line: none, replay/deterministic for the L1 identity); compute
pq-c0890eab0910fbc0, ceiling 2,408,510 µ$, formula 314,140x6.5 + 300x137x6 + 0.12x1; H-set frozen
record named (hh-kernel participant + mini-swe-agent HIR + artifacts + V5 level, digest frozen at G-L1);
restart <=1h, h_r 6h, no deliver-secret; W2 closed (index archive_verified 7667c207…, proxy ledger
harvested+verified); frozen-list predicate carried verbatim (corpus leg withholds verbatim until the
O-surface repair + Wc re-proof produce a frozen list).
verbatim_sha256 = sha256_text("you're authorized to proceed") = be8c1ebbe3768df899ca6a3ee796da1cbf49f429207e59e4866f63e8226843c6 — stamped fp-w3.json.
GATE DECISIONS row: 2026-11-11T08:00:03Z | GATE-G27
Operator confirmation of agent-drafted text (verbatim, 2026-11-11T08:00:03Z): "you're authorized to proceed" — covers
sha256:7f2502c40d0f5c59. Signed by: the operator. Recorded by Devin (SWE-2 High, coordinator), which
does not sign.
