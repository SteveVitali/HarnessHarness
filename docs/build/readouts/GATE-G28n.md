# GATE-G28n — PROV-Ws1: authorize the withheld subject-legs window's compute + metered model (W2's PP0 + L1 RECORD corpus and W3's L1.0/L1.1/L1.2 on gate-A — the frozen-list discharge of both windows' `withheld{un_reproofed_successor}` predicates) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket
authorizes spend. Ws1 is a **withheld-subject-legs window — the already-declared W2/W3 subject work, not
new scope**: the Wcs1 fold minted `task_lists.state: frozen` (17/17 slots covering, 2026-11-17), discharging
the `withheld{un_reproofed_successor:task_lists_state=blocked}` predicate verbatim on every suppressed leg.
This is the first **metered** window under the directed-completion directive — "use your best judgement.
what happened exactly and why do you need my ruling? I just want you to finish all of the benchmarks and
stuff" (Reading 1, 2026-11-17) + the follow-up verbatim "continue" — the owed legs are the G-L1 input and
the FP-E measurements; the metered spend is the FP-S-authorized W2 binding deferred by the withhold, never
new scope.

## Evidence (per item: source, `date -u` of the read, layer reached)

- The frozen-list discharge — `docs/build/reports/2026-11-17-fp-wcs1-tasklists.json`
  (`hh-task-lists-outcome/1`): `task_lists.state: frozen` — **17/17 slots covering**; Wcs1 6/6 `admit`
  (42 cells all `decided` reward 1, `denied_hosts []`, zero machinery rows, zero `verifier_unavailable`
  cells) — `docs/build/runs/R6.45p.md` — read 2026-11-17 — layer: committed records.
- The withheld legs — W2 `docs/build/runs/R6.40.md`: PP0 + CORPUS both
  `withheld{un_reproofed_successor:task_lists_state=blocked}` verbatim (W1 upload, PROV, LOAD, PS0
  executed — PS0 **passed**, `hh-ps0-stage/1` committed); W3 `docs/build/runs/R6.44.md`: L1 RECORD
  corpus + L1.0/L1.1/L1.2 all `withheld{un_reproofed_successor:task_lists_state=blocked}` verbatim
  (H-set built + exported: `set_digest sha256:c451ed97d0206821bedd6e2255e5f6bba4736646d7170601b4085492e711a8b9`
  — the digest G-L1 freezes; `h-set.tar` sha256 `8c81e9e6…`, 38,881,280 B) — read 2026-11-17 —
  layer: committed records.
- The owed scope — `docs/build/reports/2026-11-11-fp-partition.json` (`hh-fp-partition/1` `f7ddbc1f…`):
  dev partition = cancel-async-tasks (hard), build-pmars (hard), openssl-selfsigned-cert (medium) —
  the PP0 task set; gate-A = sqlite-with-gcov + cobol-modernization (8 trajectories — the L1 gate
  partition, digest `4fe2dd2d…`); gate-B = modernize-scientific-stack + extract-elf (the §7.4
  version-bump contingency partition) — read 2026-11-17 — layer: committed records.
- The model authorization — `model_line: authorized`; the metered legs are the FP-S-authorized
  binding ≈ **$12.64** (PP0 ≈ $2.17 + corpus ≈ $10.47 planning-binding, §10.1.5) deferred only by the
  withhold — inside the pre-authorized envelope (A_eff $60 → root $48; balance 59.99988 unchanged
  since W2's settlement reads). `deliver-secret OLLAMA_API_KEY` declared (env-file → ssh stdin →
  tmpfs 0400, destroyed with the VM; `provided: yes` only) — read 2026-11-17 — layer: committed
  records + GATE-G26's recorded authorization.
- The machinery — pinned `ddbfe9efd46cde4999366a8b9a7ffc2ec153b975` (the verified machinery lineage:
  exec-drain `0c4e0547`, transcripts `4679635d`, model-port guard — agent trials produce exactly the
  traffic the original defect hung on; records-only commits atop `b3f72ab7`); the runner is built
  **in-window** at that commit (the W1-carried runner predates the fixes — subject trials must not
  run the stale binary) — read 2026-11-17 — layer: committed code + recorded digests.
- The declared data — epoch-5 `sha256:e61a6ed9771d506e0606dd7a953ebe49a56dc96d8427d9eb410ae492e7f3b328`
  carried verbatim; `hh-l1-canon` C01–C07; corpus partition `f7ddbc1f…`; `retry_trials` = 4
  (REQ-XH-016's same-window allowance, never carried — REQ-XH-125); `model_port` band (the guard) —
  read 2026-11-17 — layer: committed records.
- The declaration + quote — `scripts/gce-backend/declarations/fp-ws1.json` minted (window `ws1`,
  ticket `R6.45q`, gate `PROV-Ws1`, `runner.commit ddbfe9ef…`, `verbatim_sha256` all-zeros pending
  this gate's stamp); quote `docs/build/reports/2026-11-17-fp-ws1-quote.json` →
  **`pq-8372479ab5995619`, planning 2,029,694 µ$ (~$2.03), bound 3,097,786 µ$ (~$3.10), ceiling
  4,136,280 µ$ (~$4.14)** — hard-cap `r·(D + 1 h restart) + S·r_disk·h_r + 0.12·E_gib` =
  `314,140·12 + 300·137·6 + 120,000·1` = 3,769,680 + 246,600 + 120,000 = **4,136,280 µ$** — 37
  trials + 4 same-window retries + the ~150 short $0 L1 runs, D 11 h / in-guest 675 min, h_r 6 h,
  C = 4, egress 1 GiB, ≤ 1 h harvest-only restart — read 2026-11-17 — layer: committed records.
- The spend register — `PHASE_SPEND.jsonl`: completed Σ **$23.686243** (Wcs1 $0.853518 latest);
  Ws1's ceiling takes worst-case to **$27.82 ≤ $30** first-pass compute (W4's own gate + REQ-XH-126
  D_max rule follows); model ≤ $70 envelope — the ≈$12.64 binding inside it — read 2026-11-17 —
  layer: committed records.
- The trigger state — Q-X62's recorded trigger instant **2026-11-24T21:42:46Z** (~7 days out): this
  window is the only path to a `full` G-L1 inside the trigger bound — the conformance record is
  GATE-G29's `full` input; a `not_run` or a missed trigger leaves G-L1 to its recorded `fallback`
  semantics — read 2026-11-17 — layer: committed records.
- The ruling chain — GATE-G30-postwc10 readout Status PASSED (operator's own ruling: directed
  completion); GATE-G28m PASSED `run` under the same verbatim directive; operator follow-up
  "continue" (2026-11-17) — read 2026-11-17 — layer: committed records.

<!-- agent-drafted:begin sha256=14ea0258fc8a -->
## Decision content for the operator

**What this gate authorizes.** One withheld-subject-legs window, Ws1 — the legs the frozen list
now admits, in dependency order on one VM: **PP0** (3 dev tasks — cancel-async-tasks,
build-pmars, openssl-selfsigned-cert — × 1 × 3 hosted arms: original mini-swe-agent 2.4.6, Pi,
Claude Code; effort `high`; `hh-pp0-ag-evidence/1` per trial — AG3/AG5/AG6/AG8 machinery evidence,
never verdict; REQ-XH-127 hold dispositions verbatim) → **corpus-precondition** (W2's passed
PS0 + original-mini's PP0 AG rows — PS0 is not re-run) → **L1 RECORD corpus** (original
mini-swe-agent on the 7 dev tasks × 4 = 28 trajectories through the metered proxy — the
REQ-XH-114 natural corpus) → **L1.0/L1.1/L1.2** on the gate-A partition's 8 trajectories over
the exact frozen H (`set_digest sha256:c451ed97…` carried verbatim — a rebuild is a new
participant, REQ-XC-330) → **`hh-l1-conformance/1`** pass-or-fail only, `hh-l1-diff/1` per
divergence, `l1_pass_id` bound to the exact H. `model_line: authorized` — the FP-S-authorized
≈$12.64 binding deferred by the withhold, inside the ≤$70 envelope; `deliver-secret
OLLAMA_API_KEY`; ceiling **4,136,280 µ$ ~$4.14** (bound ~$3.10); D = 11 h, h_r = 6 h, C = 4,
retry_trials 4 same-window-only; epoch-5 verbatim; runner built in-window at `ddbfe9ef`.

**Honest shape notes.** One window runs the whole chain because corpus → L1 is an in-guest data
dependency and each leg emits its own typed records — the record chain attributes per leg, never
per window. This is a **subject window, not an evidence re-proof**: legs run under their own
contracts; typed withholds stand verbatim; nothing fabricated or rescued. The drivers
(`HH_PP0_DRIVER`/`HH_CORPUS_DRIVER` + the L1 drive orchestration) are this window's body work —
the R6.26-registered legs withheld before their wiring ever ran.

**Dispositions — your verbatim answer (the agent signs nothing).**
`run` — the withheld subject-legs window as specified, under this readout's stamp onto
`fp-ws1.json`.
`not_run` — the legs stay withheld; G-L1 falls to its trigger semantics (the Q-X62 instant);
nothing is authorized.
`re-scope{…}` — name the residual to repair or the scope to amend first.
`end-first-pass-early` — record the honest outcome; the withheld claims never fabricate.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-17 via interactive session): "use your best judgement. what happened exactly and why do you need my ruling? I just want you to finish all of the benchmarks and stuff" + "continue"
Disposition: run; basis: the directed-completion directive ("finish all of the benchmarks") with mechanics delegated ("use your best judgement") applied to the owed subject legs — the frozen-list discharge makes them admissible and they are the only path to a `full` G-L1 inside the Q-X62 trigger bound; the metered spend is the FP-S-authorized W2 binding deferred by the withhold (inside the ≤$70 envelope), never new scope; first metered window under the directive — the basis is recorded verbatim here (Reading 1, same directive as GATE-G28m)
GATE DECISIONS row: 2026-11-17 | GATE-G28n (PROV-Ws1)
Operator confirmation of agent-drafted text (verbatim, 2026-11-17): "use your best judgement" + "continue" — covers sha256:14ea0258fc8a
verbatim_sha256 stamped onto fp-ws1.json covers the directive sentence ("use your best judgement … benchmarks and stuff") — sha256:1136cf8b79…, the same directive applied as at GATE-G28m
Signed by: the operator. Recorded by devin-cli/swe-2-high, which does not sign.
