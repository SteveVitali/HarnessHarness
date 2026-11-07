# 41_spec-review — Independent adversarial review of experiments spec v0.1
Stream: O
Researched: 2026-11-07T01:36:59Z
Author: two independent fresh-context reviewers (R-A: §0–§5 + Appendices A–D; R-B: §6–§10 + Appendix F), assembled verbatim by the coordinator
Ledger rows covered: O2
Repository baseline: main 09fe8523 + branch claude/experiments-spec 0877244d (spec v0.1)
Confidence overall: as stated per finding

The two reviews below are reproduced verbatim from the reviewers' outputs. Recomputation scripts referenced by R-B (`calc/rb*.py`) were run in the session scratchpad and are not committed; their printed results are quoted in the findings. Dispositions (accept | adapt | rebut | defer) are recorded in the spec's Appendix E.

---

# O2 review — reviewer R-A

- **Reviewer:** R-A (independent, fresh context; did not author the spec)
- **Scope:** `docs/experiments-spec.md` v0.1 §0, §1, §2, §3, §4 (AR-20 §4.10 hardest), §5, §10.1 (consistency read), Appendices A–D
- **Recorded:** 2026-11-07T01:30:18Z
- **Baseline:** worktree `MetaHarness-harnessharness-experiments`, branch `claude/experiments-spec` @ `0877244d` (spec commit) over `main` `09fe8523`; code facts checked at that tree. Note: `main` has since advanced to `185c16c7` (see RA-19).
- **Ground truth used:** brief; ledger §Q (l.144–203) and change log; notes 24–28, 30–34, 40 (targeted reads); code (`crates/hh-ledger/src/classes.rs`, `crates/hh-hosting/Cargo.toml`, `crates/hharness/Cargo.toml`, `scripts/check-removability.sh`, `scripts/check-position.sh`, `crates/hh-gateway/src/router.rs`, `crates/hh-budget/src/pricing.rs`).
- **Mechanical checks run (scratchpad scripts, read-only):**
  - Body requirement rows vs Appendix A, every ID (not a sample). Result: 354 body rows, 440 Appendix A rows (424 + 16). Every adopted ID has exactly one body row. Each hook, layer and § matches. Every merged or superseded alias is named in its target's *Absorbs* clause and carries the target's hook and layer. AC hooks are sequential within each subsection. There are no dangling REQ-X references. Per-family counts equal the §0.2 table. No mechanical defect found.
  - Note-emitted IDs vs Appendix A: every ID in the ten table-format notes is present. C1, C3, F1 and H1 use list format and were spot-checked by hand.
  - Appendix C sha256 prefixes: all 17 notes, the brief, the ledger @`cd87cd2f` and the outline trace @`cd87cd2f` match.
  - Quoted operator strings, Appendix B vs ledger: identical for every row the ledger also carries, except the Q-X47a–d family (RA-3).
  - Banned-phrase scan against `check-position.sh` `BANNED`, word-bounded: no hits outside quoted operator text and ban lists.

Severity counts: **2 blocker · 17 major · 18 minor** (37 findings).

---

## Blockers

### RA-1 — Harness-layer and participant identity across fresh per-window VMs is undesigned
- **Severity:** blocker
- **Location:** §5.2 (l.984 ff.); REQ-XC-303/304/305; REQ-XB-212 (l.633); REQ-XH-118 (l.2005); §10.1.4 windows W2/W3/W5
- **Finding:**
  - The spec builds each (harness × task-image) layer once, "ADMISSION (phase VM, egress allowed, recorded; once per harness × version × lock)" (l.984).
  - It requires every slot's ST2 to be verify-only, with a mismatch giving `withheld{admission:harness_layer_mismatch}` (REQ-XC-305).
  - It also tears down the VM across every gate wait and uses a fresh VM per window (REQ-XB-212, REQ-XH-118). The first pass therefore runs the same participants on at least three different fresh VMs: W2 (b)+(c), W3 (d) after the FP-Q gate, and W5 for the HH-native arm.
  - Nothing says how a layer built in one window reaches the next: there is no registry, no `podman save/load` through the harvest, and no persistent project resource.
  - Rebuilding is not reproducible. C4 states: "Whether `ensure_system_dependencies` resolves to the same package versions on rebuild depends on distro mirrors. Snapshot mirrors are not designed here." (`docs/design/34_harness-hosting.md:269`). B3 already budgets "1–1.5 h of overhead … for rebuild" per window (`32_compute-backend.md:296`).
  - Consequence: either every W3/W5 slot is `withheld{…layer_mismatch}`, or a rebuilt layer silently becomes a new participant mid-claim, breaking the pre-registered arm identity (REQ-XH-111) and REQ-XD-081 ("a post-freeze change is a new participant"). The same gap applies to the HH binaries (runner, helper and relay digests are slot-identity members, §3.3) that RECIPE rebuilds per window.
- **Proposed disposition:** accept. Add a REQ-XC-3xx (or AR-18 clause):
  - layers and the bundle are exported (`podman save`, sha256) into the harvested run root at the end of the admission window, and committed to the archive index (§4.10);
  - each later window `podman load`s them and verifies the image ID / config digest, which then becomes the identity the slot records (not a registry RepoDigest, which `load` drops);
  - HH binaries are built once per first pass and carried the same way, by digest;
  - a rebuild is a new participant and requires a gate.
  Budget the transfer in the W2/W3/W5 quotes.
- **Operator decision:** no.

### RA-2 — Slot-slice sizing rule lost in synthesis; worst-case reservation makes cheap arms refuse early, and refusals are scored
- **Severity:** blocker
- **Location:** §4.4 REQ-XC-113 (l.800), REQ-XC-112; §3.3 (l.467); REQ-XH-117 (l.2004); REQ-XC-003 (merged)
- **Finding:**
  - REQ-XC-113 reserves `ceil(rate_in·(body_bytes + template_overhead) + rate_out·max_output_bound)` and makes "`max_tokens` insertion … optional". C2 sets the default bound to the model cap M = 131 072 tokens (`33_model-mediation.md:349,356`). That is ≈ $0.066 of output reservation per call, plus input.
  - C2 states the necessary companion rule: "Slot caps must exceed the expected per-trial P90 plus one worst-case call, or the tail refuses calls that would actually fit" (`33_model-mediation.md:355`). That rule appears nowhere in the spec: `rg 'slot cap|P90 plus|worst-case call'` finds nothing, and no slot-slice size is defined anywhere.
  - REQ-XH-117 sizes caps "proportional to its planning $/trial". Pi's planning value is $0.051 × 1.8 ≈ $0.092/trial. A Pi slot sliced at that value would be refused (HTTP 402) after roughly the first call, once remaining < ~$0.07–0.10.
  - §3.3 scores such slots: "scored with `end_cause: budget_refused`" (l.467). The budget machinery would then manufacture unresolved outcomes, unequally across arms (cheap per-trial arms are hit hardest). That corrupts P2-FP, P3-L2 and P4-FP.
  - REQ-XC-003's literal "`max_tokens` … inserted when absent" is kept "merged" (normative by reference, §0.1) yet contradicted by its target (see RA-16).
- **Proposed disposition:** accept.
  - Add C2's rule as a SHALL: slot slice ≥ P90 $/trial (planning prior until measured) + one worst-case call reservation; per-arm cap = Σ slices of planned slots.
  - Report premature refusals (`requested`/`available`) as a per-arm covariate.
  - Make "a slot refused while remaining ≥ its consumed-to-date tail" an integrity flag that excludes the slot from primary inference rather than scoring it.
  - Re-disposition REQ-XC-003 as `superseded-by(REQ-XC-113)` with a C2-F10 ruling.
  - Re-check the §10.1.5 root-cap arithmetic with slices reserved, not mean costs.
- **Operator decision:** no (the per-arm cap values are already FP-S content).

---

## Major

### RA-3 — First-pass decision IDs do not match the ledger; "verbatim" Appendix B rows are paraphrases; P-items are not in the ledger
- **Severity:** major
- **Location:** header l.5; §0.3.1 l.54; FP-3/FP-8; §10.1.2 heading; REQ-XH-111; Appendix B l.2642, 2652–2658
- **Finding:**
  - The ledger committed before the spec (`96c743a5`) records the follow-ups as **Q-X48, Q-X49, Q-X50**, and effort as **Q-X22 "high"** (`experiments-research-ledger.md:173,199–201`; change log 2026-11-07T01:00:51Z).
  - The spec instead invents Q-X47a–d, and Appendix B says "the coordinator … owns their final numbering", which was already final.
  - Appendix B labels its decision column "verbatim", but the free text for Q-X47a–c is reworded. Ledger Q-X49: "— seeded from the shortest-declared-timeout stratum of TB 2.1; all arms share the tasks". Spec: "… stratum of TB 2.1, every arm runs the same 10 tasks, k=5".
  - The pending items Q-X47-P1/P2/P3 (ε = ±.25, the V5 level, and the Q-X15 deviation) exist only in the spec. The ledger §Q, which drives the operator dialog, has no rows for them, so they risk never being asked.
- **Proposed disposition:** accept.
  - Renumber to Q-X48/49/50 and Q-X22 throughout, or add an explicit alias table.
  - Copy the ledger's decision text byte for byte.
  - Have the coordinator add Q-X51–53 (the three pending first-pass items) to ledger §Q with recorded defaults and decision points.
- **Operator decision:** no (bookkeeping), but the three new rows must reach the operator.

### RA-4 — §4.10 slot-token handling contradicts REQ-XC-109 and will routinely stop the archive; the mask set omits the live credentials it most needs
- **Severity:** major
- **Location:** REQ-XH-103 (l.918) vs REQ-XC-109 (l.775); REQ-XC-308
- **Finding:**
  - REQ-XC-109 deposits receipts that contain slot tokens and tombstones them only on export ("Published exports SHALL tombstone slot tokens"). Only an Ollama-*key* hit refuses.
  - REQ-XH-103: "A hit [of the mask set incl. slot tokens] SHALL refuse that file's upload and stop the archive ticket".
  - The slot token sits in every cell environment as the harness's API key (REQ-XC-308). Agents routinely run `env` or `printenv`, and the output enters the next request body and so the raw receipt. Raw receipts are content-addressed and cannot be tombstoned, so the archive ticket would stop on ordinary trials.
  - The mask set is "repository detector set + Ollama key + slot tokens". It omits the HF write token in use during the very upload, and the GCP credentials present on the controller.
- **Proposed disposition:** accept.
  - Refuse only on *live* credentials: the Ollama key, the HF token, GCP credentials and detector hits.
  - Slot tokens must be verified revoked (via `security.credential.revoked`). They are then permitted in the private raw archive and tombstoned in every derived or public file.
  - The public flip (REQ-XH-105) may expose only sanitized derivatives; raw receipts stay private.
- **Operator decision:** no.

### RA-5 — §4.10 has no remediation path for a secret discovered after upload; "append-only" forbids it
- **Severity:** major
- **Location:** REQ-XH-104 (l.919), REQ-XH-106
- **Finding:**
  - The spec says "Uploads are append-only: a re-upload creates a new revision … supersedes the earlier one by reference", and nothing may be deleted without a closure decision.
  - A leaked key in revision r stays readable in the dataset history; HF repos are git-backed.
  - No step rotates the leaked credential, purges or squashes the revision, or records the incident. Key rotation (§3.11) is "optional".
  - This is the one AR-20 path where append-only and "secrets never recorded" collide.
- **Proposed disposition:** accept. Add REQ-XH-107 (synthesis-allocated):
  - on any post-upload secret finding: rotate the credential immediately (a HUMAN row, mandatory);
  - history-purge the dataset revision as a dated, operator-recorded destructive act;
  - the index records a tombstone superseding entry with the reason, and no value.
- **Operator decision:** yes (authorizes a destructive act class in advance).

### RA-6 — The authoritative run history has no archive home; the "recomputable" claims cannot be met; index role enum is incomplete
- **Severity:** major
- **Location:** REQ-XH-101 (l.916); REQ-XF-124 (l.915); REQ-XC-118; §1.1 ("records anyone can recompute")
- **Finding:**
  - REQ-XH-101 archives "proxy … raw bodies, `hh.model_io/1` blobs, egress logs, native harness logs, ATIF".
  - It does not cover the hash-chained ledgers of the phase proxy run, the mediator run, the slot stores or the experiment store. REQ-XC-118's `costs.model` is "a recomputable fold over the phase proxy run", and REQ-XB-114's per-host counts are "folds over those rows".
  - The spec puts those stores neither in git nor in the dataset, so after VM teardown and laptop retention lapse, no fold is recomputable.
  - REQ-XF-124's role enum `raw_receipts|transcript|atif|egress_log` has no role for "native harness logs" or ledger segments, which REQ-XH-101 requires.
- **Proposed disposition:** accept.
  - Add the harvested store roots (ledger segments plus blob pool, with the harvest manifest) to REQ-XH-101.
  - Add roles `native_log` and `store_segment` to REQ-XF-124.
  - State which committed packets are recomputable from what.
- **Operator decision:** no.

### RA-7 — Paid raw evidence sits single-copy on the operator laptop from W2 until the end of the pass
- **Severity:** major
- **Location:** §10.2 DAG l.2029 (`FP-RESULTS ──► HUMAN flagged-pass review ──► ARCHIVE-UPLOAD`); REQ-XH-102/106
- **Finding:**
  - The only upload happens after all results and the flagged-pass review. Every harvest from W2, W3 and W5 therefore exists only on the controller disk for the whole pass. The ledger change log records that disk at 98 %.
  - The VM copy is deleted at teardown (REQ-XB-210). The W4 L1 corpus and replay recordings also depend on that single copy.
  - Losing it loses paid, unrepeatable evidence. REQ-XH-106's retention-until-verified rule does nothing before the first upload.
- **Proposed disposition:** accept. Make ARCHIVE-UPLOAD per window: a private upload plus read-back immediately after each HUMAN reconcile, authorized once at FP-S. Keep the public flip at the publication gate.
- **Operator decision:** no (FP-S already carries the upload authorization).

### RA-8 — Model settlement after teardown is unplaced, and its "two agreeing reads" rule cannot detect provider lag
- **Severity:** major
- **Location:** §4.1 l.714; §4.7 REQ-XC-121 (l.861); REQ-XB-210 teardown sequence
- **Finding:**
  - The proxy is "the only holder of the Ollama key" (l.714), and C2 puts the settlement poller "in the proxy (the key holder)" (`33_model-mediation.md:427`).
  - The proxy dies with the VM at teardown. `/api/usage` is "hourly at best, lagging" (l.710).
  - The nine-step teardown has no wait-for-settlement step, and the HUMAN reconcile row covers GCP billing only.
  - The settlement rule "two consecutive agreeing reads at least one minute apart" passes on two equally stale reads taken before the hour bucket posts. `settled` would then be recorded falsely, and REQ-XH-005 would drop m to .10 on that basis.
- **Proposed disposition:** accept.
  - Define a controller-side settlement act (key read at use from the operator env file, ≥ 24 h after the window, like the compute reconcile) recorded into the phase spend ledger. Amend "only holder" to "only holder during a window".
  - Require settlement reads to start ≥ 2 h after the last call's hour boundary and to include the full window's hour buckets.
- **Operator decision:** no.

### RA-9 — "Hard-bounded" first-pass totals overstate what the mechanisms bound
- **Severity:** major
- **Location:** §10.1.1 l.1888 ("Compute is bounded by GCE-enforced D per window"); §10.1.5 l.1977 ("Hard-bounded (root cap + Σ ceilings) … $62.7"); REQ-XH-110
- **Finding:**
  - The root cap bounds HH's *meter*, not Ollama's bill. §4.4 itself says Anthropic-compat counts are tokenizer approximations and m exists because the meter can under-count.
  - The bill is bounded at $48/(1−m) = $60 only if the under-count is ≤ 20 %. That is UNVERIFIED until the first settlement, and Q-X21 (refuse vs auto-bill at exhaustion) is open.
  - GCE max-run-duration with action STOP stops the instance, not billing. The 300 GB `pd-balanced` disk keeps billing (≈ $0.04/h) until teardown. The §10.1.5 ceilings set the residual term h_r = 0, and the admitted "stopped, disk retained" end state (REQ-XB-210) is open-ended.
  - The same "hard" phrasing is carried into REQ-XH-110's "enforced only by".
- **Proposed disposition:** accept.
  - Relabel the row "Bounded by mechanism (meter cap + instance dead-man)".
  - State the bill bound's conditions: under-count ≤ m, and disk residual h_r declared per window with a maximum, e.g. 48 h.
  - Add h_r to each window's $_D.
  - Make the disk-retained deviation time-boxed in the ceiling.
- **Operator decision:** no.

### RA-10 — A dead-man STOP traps the window's paid evidence behind a new authorization; there is no pre-D drain
- **Severity:** major
- **Location:** REQ-XB-209 (l.630: "Restarting a dead-man-stopped VM SHALL require a new authorization"); REQ-XB-210 steps 1–2; §10.1.5 ("D sits below the timeout bound in the agent windows")
- **Finding:**
  - The first-pass D values deliberately sit below the timeout bound, so a dead-man stop is a planned possibility.
  - Harvest (teardown step 2) needs a running VM over IAP ssh. After a STOP, the receipts, proxy ledger and slot stores are reachable only by restarting, which needs a new authorization.
  - The runner has no rule to stop admissions and harvest at D − Δ.
  - Result: a routine overrun strands paid evidence and adds an unbudgeted restart.
- **Proposed disposition:** accept.
  - Add a runner rule: stop admitting at D − (max agent+verify bound of in-flight slots + harvest time), and harvest incrementally after each slot.
  - Pre-authorize at each window gate a bounded "harvest-and-teardown-only restart" (≤ 1 h, no slot admission), quoted in $_D.
- **Operator decision:** yes (Q-X38 policy; the pre-authorized restart class).

### RA-11 — The first-pass compute allocation (≤ $15) has no contingency for re-proof repair cycles or the uncounted W1 work
- **Severity:** major
- **Location:** REQ-XB-125 (l.576: machinery discordance "is repaired, never excluded"); FP-9; REQ-XH-118; §10.1.5 W1 row l.1963
- **Finding:**
  - A machinery discordance on a first-pass task cannot be replaced; REQ-XH-116 replaces only exclusions. It needs a code repair and a re-run, i.e. another provisioning window.
  - Round 5 needed four epochs to clear comparable machinery defects (§2.1).
  - W1's 117 trials omit:
    - the observe pass of §3.8 (pass 1, run before the freeze);
    - admission builds of ~3 harnesses × ~14 images (§5.2);
    - the Rust build (B3: "tens of minutes … unmeasured");
    - the five-variant smoke battery per harness.
  - Σ ceilings is already $14.73 against $15, so a single W1′ (≈ $3.3) breaks FP-9.
- **Proposed disposition:** adapt. Either:
  - (a) quote W1 with the observe pass, builds and smokes explicitly, and hold one repair window (W1′) inside the $15 by shrinking another D; or
  - (b) declare that a machinery discordance in the first pass is handled by a recorded scope decision at a gate (replace with the next task and disclose), not an unbudgeted repair.
  - Option (b) needs an operator ruling because it relaxes REQ-XB-125 for the pass.
- **Operator decision:** yes if (b).

### RA-12 — S-3 grants hosts beyond Q-X39a's literal text, and its grant decision precedes the observation that discovers the hosts
- **Severity:** major
- **Location:** §0.3.3 S-3; §3.5 verify row (l.517) and l.520; REQ-XB-116; PROV-W1 gate content (l.2049)
- **Finding:**
  - Q-X39a (binding): "standard registries … plus hosts the instruction/task files name; other oracle-observed hosts withheld and logged" (ledger l.190).
  - The spec grants "oracle-observed verify-phase hosts" (§3.5 verify row; REQ-XB-116) on S-3's reading that the question scoped only the agent phase. That reading is plausible, but it widens a binding answer and is routed only to O2, not to the operator.
  - Separately, S-3 makes "an operator grant for that host" a PROV-W1 decision (l.2049: "S-3 grants"). The hosts are only discovered by the observe pass *inside* W1 (§3.8 pass 1), and the allowlist is frozen inside W1 before the prove pass.
  - A mid-W1 grant needs an operator gate, which REQ-XH-118 forbids without a teardown. The grant option is therefore unreachable in the first pass.
- **Proposed disposition:** adapt.
  - Put the verify-phase reading to the operator as a Q-X39 clarification.
  - For the first pass, record "no grants; exclude as `runner_infeasible:allowlist_gap{host}`, replace from the order" as the operative rule.
  - Drop "S-3 grants" from the PROV-W1 content, or move the grant decision to a post-W1 gate with an H-allow re-prove window.
- **Operator decision:** yes.

### RA-13 — Q-X39a's "GitHub releases" grant is host-level and so grants all of `github.com`, including the public TB 2.1 solutions repo; B2's per-task restriction was lost
- **Severity:** major
- **Location:** §3.5 agent row (l.516), "Answer-key sites are explicitly denied (`hub.harborframework.com`, `tbench.ai`)"; REQ-XB-116
- **Finding:**
  - The registry channel is CONNECT, not terminated (`tls_opaque`). It "reveals only host names, so a granted shared host (`github.com` …) cannot be path-filtered" (`31_experiment-runner.md:399`).
  - B2 had proposed "`github.com` is granted only to tasks whose instruction requires it, with an `approved_host_body` residual plus scan" (`31_experiment-runner.md:592`).
  - The spec grants GitHub releases to every task under Q-X39a. That makes `git clone https://github.com/harbor-framework/terminal-bench-2-1`, which carries `solution/solve.sh` for every task, reachable in every paid cell.
  - The explicit deny list omits the largest answer-key source.
  - Q-X30's purpose was "answer-fetching risk". The operator likely did not see that "GitHub releases" equals `github.com`.
- **Proposed disposition:** adapt.
  - Ask the operator whether "GitHub releases" should be granted host-wide (with the residual stated per result) or only per task.
  - Either way, add `github.com/harbor-framework/*` and TB mirrors to the integrity scan's signatures (clone or URL patterns plus solution fingerprints).
  - Name `github.com` in each result's "allowlisted agent network" label.
- **Operator decision:** yes.

### RA-14 — Paid R-live on the Harbor O surface runs agents with open bridge egress, contrary to Q-X30, and the spec does not say so
- **Severity:** major
- **Location:** §5.7 R-live (l.~1084); §10.1.4 step (b); REQ-XC-326
- **Finding:**
  - Q-X30 (binding) covers "Agent-phase network policy for paid runs": "agents reach only the HH model proxy plus declared package registries, enforced and logged per slot".
  - C4 designs R-live as Harbor jobs whose model route is "a host relay on the podman bridge gateway" (`34_harness-hosting.md:196`). Harbor's O-surface agent container therefore sits on a bridge network with TB 2.1's `allow_internet = true`.
  - That is a paid agent run with unmediated egress and no per-slot logging.
  - The spec's R-live text omits the network posture entirely.
- **Proposed disposition:** adapt.
  - State R-live's egress posture explicitly.
  - Either run O-surface agent jobs under an equivalent restriction (Harbor podman `--network none` plus host relay, if Harbor supports it at the pin) or record a scoped operator exception to Q-X30 for R-live (3 trials) with the residual declared.
- **Operator decision:** yes (exception to a binding answer).

### RA-15 — Reserved spellings used two ways, against ruling S-12
- **Severity:** major
- **Location:** REQ-XC-302 (l.975 "missing gate (G1–G9 per role)"), REQ-XC-214 (l.978 "gate G9"), REQ-XC-311 (l.1021 "fail G5"), REQ-XC-324 (l.1091 "G3/G5/G6 … G8"); §4.3 l.751 "D3-allowlisted response headers"; §10.3 l.2048 / Appendix B Q-X23 "early S0 gate"
- **Finding:**
  - S-12 says inherited text "is rewritten into" the namespaced spellings, and Appendix D reserves **G1–G23 for HIR gaps**, with admission gates as **AG1–AG9**.
  - Four §5 requirements still say G1–G9. §7 uses G1–G9 as gaps in the same document (e.g. l.1401–1409). The coverage register and validator truth tables would bind the wrong objects.
  - "D3-allowlisted" means ADR-0363 D3, but D3 is also the phase-4 note's row id.
  - "S0 gate" collides with the S1–S4 = round-5-scenarios-only rule and with PS0.
- **Proposed disposition:** accept. Rewrite to AG1–AG9, "ADR-0363 D3 allowlist" and "PS0 early gate". Extend the Appendix D namespace table to cover gate and ADR-decision references.
- **Operator decision:** no.

### RA-16 — "Merged" aliases whose literal predicates contradict or miss their targets, made normative by §0.1
- **Severity:** major
- **Location:** §0.1 l.17 (alias text "normative by reference"); §0.2 dispositions; Appendix A
- **Finding:**
  - §0.1 makes every merged alias's literal predicate normative, "owned by the target". Four merges contradict or miss their target:
    - (a) **REQ-XA-007** — "departures from frozen reattempt bounds are recorded as dated amendments" (`24_executed-record-audit.md:263`). Target REQ-XA-115: "SHALL NOT be called an amendment".
    - (b) **REQ-XC-003** — `max_tokens` "inserted when absent". Target REQ-XC-113: "optional".
    - (c) **REQ-XC-011** — 429/5xx are "infrastructure failures for reattempt policy". Target REQ-XC-115 restates this, but §5.8 scores "Upstream 429/5xx the harness survives".
    - (d) **REQ-XA-010** — "Drivers … use no absolute paths, and take inputs from committed fixtures". It is merged into the *archive* requirement REQ-XA-101, which preserves absolute paths; S-2 says the predicate binds REQ-XB-101 and REQ-XA-106, but Appendix A still targets REQ-XA-101 (`AC-2.2-1`).
  - Under the coverage register (REQ-XH-003 "aliases keep their literal predicate bytes"), these become unsatisfiable or mis-owned obligations.
- **Proposed disposition:** accept.
  - (a)–(c): re-disposition as `superseded-by(target)` with a §0.3 ruling citing the refining note. (a) depends on pending Q-X25.
  - (d): retarget to REQ-XB-101 (or split it), and move `driver_commit` to REQ-XA-127.
  - Run the same "merged-but-REFINES" audit over all 12 C1 aliases C2 marks "REFINES".
- **Operator decision:** no; (a) follows Q-X25.

### RA-17 — REQ-XC-101's acceptance cannot pass at baseline: a workspace crate already takes a normal dependency on `hharness-hosting`
- **Severity:** major
- **Location:** REQ-XC-101 (l.718): "No workspace crate SHALL take a normal dependency on `hharness-hosting`" · acceptance "`scripts/check-removability.sh` passes"
- **Finding:**
  - The facade crate `hharness` declares `hharness-hosting = { path = "../hh-hosting", … }` under `[dependencies]` (`crates/hharness/Cargo.toml:55`, from L4.5 / ADR-0380).
  - Running the removability script's hosting leg offline (`cargo metadata --no-deps` + its Python) at this tree prints `['hharness -> hharness-hosting']`, i.e. that leg fails today.
  - The requirement as written is false before any program work, and its acceptance hook cannot pass without an unrelated change.
- **Proposed disposition:** adapt. Restate the rule as "no crate other than the published facade `hharness` …", and record the facade carve-out (or the pre-existing leg failure) as a dated BL/DF row the PROXY-CORE ticket must reconcile. Verify whether `check-removability.sh` is wired into CI; it does not appear in `.github/workflows/ci.yml`.
- **Operator decision:** no.

### RA-18 — "Pilot"-keyed obligations have no first-pass firing condition and are missing from the §10.1.6 adaptation table
- **Severity:** major
- **Location:** REQ-XC-210 (l.805 "until the pilot's P0 stage"); REQ-XC-322 (l.1026 "The pilot SHALL report …"); REQ-XC-325 (l.1092 "a stage of the shared pilot"); REQ-XH-005 (l.806 "pilot default m = 0.20"); REQ-XH-007 (l.807 "A pilot gate readout"); REQ-XH-102 (l.917 "authorized at the pilot gate"); REQ-XD-043 / REQ-XC-015 (pilot outputs); §10.1.6
- **Finding:**
  - FP-4 says the pilot "is replaced in the first pass". §10.1.6 maps only REQ-XH-010 and REQ-XD-040/041/044.
  - The listed requirements still name a pilot, P0 stage or pilot gate that does not exist in Round 6.
  - A decomposer cannot assign their gates or ACs: which gate authorizes the upload, when cached-share measurement ends no-cache reservations, and where the queue p99 is reported. This is a coverage-of-every-row defect for the first pass.
- **Proposed disposition:** accept. Add each to §10.1.6, mapping "pilot gate" to FP-S, "P0 stage" to step (b) PS0/PP0, and "pilot report" to the FP-Q readout. Alternatively, rewrite the bodies as pass-neutral ("the first spend gate of the pass").
- **Operator decision:** no.

### RA-19 — Baseline drift: `main` has a newer commit that re-states the phrasings phase 1 corrects
- **Severity:** major
- **Location:** header l.5 (baseline `09fe8523`); §2.4 REQ-XA-114 (correction list); §10.6 ("Append after row 251 on `main`")
- **Finding:**
  - `main` is now `185c16c7` (BL-RECONCILE, #291).
  - Its BL-50 dated note, in `docs/build/BACKLOG.md` and `docs/build/pr/BL-RECONCILE.md`, says "parity verified 30/30 tranche-3" and "efficacy computed, honest null-equivalent". Those are the exact readings A1/A2 correct and `RECORD_BANNED` lists.
  - REQ-XA-114's append-only correction list (PARITY-TRANCHE-3, S4-CONFIRMATION, R5.59, GATE-G19, DF-CAP5.1-1) does not include BL-50 or the BL-RECONCILE records. `RECORD_BANNED` scans only the record, `evidence.md` and the spine, so the new text is uncorrected and unchecked.
- **Proposed disposition:** accept. Rebase the baseline to `185c16c7`, add BL-50's note and `runs/BL-RECONCILE.md` to REQ-XA-114, and re-check that row 251 is still the manifest tail (it is at `185c16c7`).
- **Operator decision:** no.

---

## Minor

### RA-20 — False factual claim about repairs in committed code
- **Severity:** minor
- **Location:** §2.1 l.220
- **Finding:** The spec says "`rg strip_ansi|task_workdir|PytestParser crates scripts` returns nothing". At this tree it returns four files under `crates/hh-env/tests/fixtures/r5_49/*.json` (packet data). The intended claim (no committed *code* implements them) holds.
- **Proposed disposition:** accept. Reword to "no Rust or script source implements them; the only hits are fixture packets".
- **Operator decision:** no.

### RA-21 — BOUNDS statement 3 omits which `parity_admission` rung refused
- **Severity:** minor
- **Location:** §2.6 statement 3 (l.315)
- **Finding:** "the landed `parity_admission` refuses this design with [`AdapterFloor{repeats:1}`]". By §2.3 that is the labelled *design-probe* rung; the strict records-only rung refuses with `MissingKey{pins.rights_ref}`. A reader takes AdapterFloor as the records-only outcome.
- **Proposed disposition:** accept. Name the rung, or state both outcomes.
- **Operator decision:** no.

### RA-22 — REQ-XB-125 wording contradicts its own exclusion rule
- **Severity:** minor
- **Location:** REQ-XB-125 (l.576); §3.8 R row
- **Finding:** "SHALL admit every task" conflicts with the same row's O-failure exclusion. Separately, "R = 3 (minimum 2)" conflicts with the admit rule "3/3 … 3/3 … 3/3".
- **Proposed disposition:** accept. Use "SHALL decide every task (admit, or exclude on a typed basis)", and state the admit rule for R = 2.
- **Operator decision:** no.

### RA-23 — Reserved-string drift between the body and Appendix D
- **Severity:** minor
- **Location:** REQ-XC-207 (l.777); REQ-XC-325 (l.1092); REQ-XB-216; REQ-XH-032; §4.8; Appendix D
- **Finding:**
  - REQ-XC-207 says "`failed`/infra … never scored", while §3.3 and §5.8 use `inconclusive{infra:<c>}`.
  - REQ-XC-325's label "per-harness runner check — fidelity smoke, `parity: none`" differs from Appendix D's "fidelity check, parity: none", so the phrase pin will fail.
  - Appendix D lacks `unavailable{reason}`, `within`/`outside` (REQ-XB-216), `discordant{h_internal}` (REQ-XH-032) and `released: client_gone` (§4.8).
- **Proposed disposition:** accept. Unify the spellings and add the missing strings to Appendix D.
- **Operator decision:** no.

### RA-24 — REQ-XC-327's acceptance cannot fail
- **Severity:** minor
- **Location:** REQ-XC-327 (l.1041)
- **Finding:** "SM-pos shows no successful request to an Anthropic host" is guaranteed by `--network none` plus the allowlist, so it tests nothing about Claude Code's configuration.
- **Proposed disposition:** accept. Assert instead that the mediator rows show every Anthropic-host attempt denied and logged, and the count is in `expected_denied`; and that every receipt's route is `/v1/messages` on the proxy.
- **Operator decision:** no.

### RA-25 — REQ-XH-118's shape rule is circular
- **Severity:** minor
- **Location:** REQ-XH-118 (l.2005)
- **Finding:** "smallest declared VM shape whose memory and vCPU satisfy REQ-XB-214" is circular, because REQ-XB-214 derives C from the shape, so any shape qualifies with C = 1. The window D values assume C = 4. A smaller shape would push windows past D into `backend_terminated`.
- **Proposed disposition:** accept. State a C floor (C ≥ 4, or the quote's C) and require W_p ≤ D at the chosen shape.
- **Operator decision:** no.

### RA-26 — Secret least-privilege gaps on the VM and controller
- **Severity:** minor
- **Location:** §3.11; REQ-XB-219 (l.684); REQ-XH-102
- **Finding:**
  - The Ollama key is not barred from $0 windows (W1, W4).
  - "operator env file" has no declared path. Q-X6 names `~/MetaHarness-harnessharness/.env`, inside the other session's worktree.
  - Nothing forbids the HF CLI's `login`, which writes the token to `~/.cache/huggingface/token`.
  - No HF token scope (fine-grained, single repo) is specified.
- **Proposed disposition:** accept.
  - `deliver-secret` only for windows with an authorized model line.
  - A declared env-file path variable that is read, never copied.
  - Token passed per invocation via environment; `login` forbidden.
  - The token is fine-grained, scoped to the one dataset, and recorded `provided: yes`.
- **Operator decision:** no.

### RA-27 — Fresh-project-per-phase is undefined for windows that serve several phases
- **Severity:** minor
- **Location:** REQ-XB-212; S-11; §0.3.1 l.55 ("per paid phase")
- **Finding:** W2/W3 serve phase 2 and phase-3 O; W5 serves phase-3 H and phase 4; W1/W4 are $0. §0.3.1 says "per paid phase", but REQ-XB-212 says "each phase".
- **Proposed disposition:** accept. Add an explicit first-pass project map (e.g. one project per window, or one for W1–W5 under a single "first-pass" phase id) and reconcile the wording.
- **Operator decision:** no (an S-11 interpretation).

### RA-28 — Selection timing is inconsistent between the DAG and REQ-XH-116
- **Severity:** minor
- **Location:** §10.2 l.2025 (`W1 ──► FP-PREREG (selection, …)`); REQ-XH-116; REQ-XH-033
- **Finding:** The DAG places selection after W1, while W1 re-proofs "the drawn first-pass tasks". The dev/held-out split also precedes any full re-proof, which REQ-XH-033's unconditioned text forbids.
- **Proposed disposition:** accept. SELECT commits the split, the draw and the replacement order before PROV-W1. FP-PREREG freezes the post-replacement list. Add the REQ-XH-033 carve-out to its own text, not only to §10.1.6.
- **Operator decision:** no.

### RA-29 — Harbor pin: diff scope too narrow; "fixed now" overstates a pending decision
- **Severity:** minor
- **Location:** REQ-XH-014 (l.1128); §5.9 l.1124 ("Fallback rule (fixed now …)")
- **Finding:** The diff re-run on a later tag covers only E1 F13's two agents (mini-swe-agent, Terminus-2). The first pass also uses Harbor's Pi and Claude Code adapters (renderers mirror them, REQ-XC-307). Separately, "fixed now" is said of a rule whose question, Q-X41, is pending.
- **Proposed disposition:** accept. Extend the diff to every admitted adapter, and say "recorded default (Q-X41 pending)".
- **Operator decision:** no.

### RA-30 — The "Expected (cached priors)" column uses low-effort figures under a `high` pin
- **Severity:** minor
- **Location:** §10.1.5 l.1938 and l.1944
- **Finding:** The column uses mini $0.043 and Pi $0.023, which are HF low-effort values, under a `high` pin. The text itself cites HF $0.057 at `high` for mini. C3 l.75 gives Pi $0.028 at `high`. The column understates expected spend while the prose says priors are used "without a low-effort discount".
- **Proposed disposition:** accept. Use the high-effort cached priors, or relabel the column "low-effort cached (lower bound)".
- **Operator decision:** no.

### RA-31 — External tools are called admitted before their ADRs exist, and some are missing from the register
- **Severity:** minor
- **Location:** §1.4 l.199
- **Finding:** "Harbor and `gcloud` are admitted external tools" is true only after AR-04 and AR-15 land. `uv` (REQ-XC-315 `uv sync --frozen`), the HF upload tool (AR-20) and the pinned podman package set are also external tools, and no AR row lists them.
- **Proposed disposition:** accept. Say "to be admitted by AR-04/AR-15/AR-20", and add `uv` and the podman packages to an AR row.
- **Operator decision:** no.

### RA-32 — `hh-cell-relay` as the container's main process: portability and fault-attribution gaps
- **Severity:** minor
- **Location:** §3.3 ST1 (l.425 "relay = main process"); §3.3 outcome table
- **Finding:** The relay binary is injected as PID 1 into arbitrary task images, but nothing specifies linking (static or musl, which would be a `rust-toolchain.toml` target change) or a libc floor. Also, an agent command that kills PID 1 turns an agent fault into "relay death → `infrastructure_failure` → replan", which re-runs the paid participant.
- **Proposed disposition:** accept. Specify the relay build target, and classify relay death after an agent-issued signal as `executed` with `end_cause: crashed` (flagged), not infra.
- **Operator decision:** no.

### RA-33 — Q-X31's "keep all nominally" vs the a-priori QEMU exclusions
- **Severity:** minor
- **Location:** §3.1 l.380 (two QEMU "pre-registered exclusions")
- **Finding:** Q-X31 (binding): "Let's keep all nominally". Excluding two tasks a priori from the 89 shrinks the nominal population without an operator answer.
- **Proposed disposition:** adapt. Keep the two tasks in the nominal manifest with a typed `runner_infeasible:…` disposition reported in every denominator, or confirm the exclusion with the operator at ratification.
- **Operator decision:** confirm at ratification.

### RA-34 — The public dataset flip omits the canary and licence obligations
- **Severity:** minor
- **Location:** REQ-XH-105
- **Finding:** B1 F4's implication: "Published HH records may redistribute task text only under Apache-2.0 terms and must keep the canary" (`25_tb21-harbor.md:173`). The site prints the canary (REQ-XF-133), but the public dataset has no dataset-card, canary or NOTICE requirement.
- **Proposed disposition:** accept. Require a dataset card carrying the TB canary string, its meaning and the Apache-2.0 attribution/NOTICE before the flip.
- **Operator decision:** no.

### RA-35 — CC1 risk: second canonicalizer and second pricing representation
- **Severity:** minor
- **Location:** REQ-XC-122 (`json_canonical` built-in); REQ-XC-116 (`rate_unit`)
- **Finding:** REQ-XC-122's `json_canonical` built-in is not tied to the existing canonical-JSON implementation. REQ-XC-116 adds `rate_unit` to `PricingTable` while the router's `DeclaredPricing` keeps its own µ$/1k fields (`crates/hh-gateway/src/router.rs:1659–1687`).
- **Proposed disposition:** adapt. Require `json_canonical` to call the workspace canonicalizer (CC1). Require proxy pricing to read one table type, with `DeclaredPricing` either derived from it or declared out of scope.
- **Operator decision:** no.

### RA-36 — Billing-alert default does not match the first-pass compute cap
- **Severity:** minor
- **Location:** Appendix B Q-X38 default; HUMAN "GCP billing alert" (l.2064)
- **Finding:** "$50/month at 50/90/100 %" fires its first alert at $25, above the entire first-pass compute allocation ($15).
- **Proposed disposition:** accept. Set the first-pass default alert amount to the compute authorization ($15).
- **Operator decision:** no (Q-X38 is already pending).

### RA-37 — The Q-X23 default puts the key and proxy on the laptop for a paid act
- **Severity:** minor
- **Location:** Appendix B Q-X23 default ("≤ $0.05, laptop-local"); §10.3 l.2048
- **Finding:** Q-X37 (binding) placed the proxy and key on the VM "during paid phases". A laptop-local paid smoke is outside that literal scope but is a paid act with the key used locally.
- **Proposed disposition:** adapt. State at the Q-X23 decision that the S0 smoke runs laptop-local by exception, or run it as PS0 on W2.
- **Operator decision:** yes (folded into Q-X23).

---

## Overall assessment

The spec is not ready to ratify.

**Mechanically it is sound.**
- Every adopted ID, hook, layer and alias in Appendix A reconciles with the body.
- Every Appendix C hash matches.
- Every quoted operator string matches the ledger, except the Q-X47a–d family.

**Two blockers.**
- **RA-1:** no harness-layer or binary identity across fresh per-window VMs.
- **RA-2:** the lost slot-slice sizing rule would let worst-case reservations manufacture scored `budget_refused` failures.

Either alone would invalidate the first pass's pre-registered contrasts; both have small, specifiable fixes.

**The AR-20 design (§4.10) needs four fixes before ratification:**
- RA-4: slot-token and mask-set conflict;
- RA-5: no post-upload secret remediation;
- RA-6: authoritative stores not archived;
- RA-7: single-copy evidence until the end of the pass.

**Operator-fidelity items need answers, not just O2 dispositions:**
- RA-12: S-3 widens Q-X39a;
- RA-13: "GitHub releases" equals all of `github.com`;
- RA-14: R-live egress versus Q-X30;
- RA-10: pre-authorized harvest restart.

**Remaining majors are honesty and coverage fixes:**
- RA-3: Q-X48–50 numbering and verbatim text;
- RA-9: "hard-bounded" wording;
- RA-16: merged-but-contradicted aliases;
- RA-17: hosting-dependency acceptance already false at baseline;
- RA-18: unmapped pilot obligations;
- RA-19: baseline drift to `185c16c7`.

With the two blockers and the §4.10 fixes folded in, and the RA-10/12/13/14 operator questions added to ledger §Q, §0–§5 would be ratifiable.


---

# Adversarial review — reviewer R-B

- **Reviewer:** R-B (independent, fresh context; did not author the spec)
- **Scope:** `docs/experiments-spec.md` v0.1 §0.3 and §1 (context), §6, §7, §8, §9, §10 (especially §10.1, §10.3, §10.5, §10.6), Appendix F with `docs/experiments-OUTLINE_TRACE.md`. §0–§5 and Appendices A–D belong to the other reviewer.
- **Date:** 2026-11-07T01:18:22Z (`date -u +%FT%TZ`)
- **Baseline:** worktree `/Users/stevenvitali/MetaHarness-harnessharness-experiments`, HEAD `0877244d` (spec v0.1) on `96c743a5` / `09fe8523`. Note: `main` is now `185c16c7` (BL-RECONCILE; touches `docs/build/LEDGER.md`, `BACKLOG.*`). `scripts/deferrals-open.sh` printed nothing (zero open rows) at this baseline.
- **Procedure:** `~/agent-skills/skills/synthesize-spec/modes/review.md`: internal consistency, ledger coverage, MVP realism, self-contradiction.
- **Recomputation code** (stdlib Python, run with `python3 -I`, no network), under `/private/tmp/claude-501/-Users-stevenvitali-MetaHarness-harnessharness/4210edcb-e16b-4f68-bf87-84c31bfd2161/scratchpad/calc/`:
  - `rb.py`: budget tables.
  - `rb2.py`: sign-flip attainability and power at T = 10 under D1's `board` mixture.
  - `rb3.py`: KL TOST P(`equivalent`) at N = 50. It reproduces Table A's half-width: .246 at c = 6 against the table's .244 continuous value. `kl.py` was not used.

---

## Blockers

### RB-1 · blocker · §10.6 l.2154 vs REQ-XH-001 (l.2173) and REQ-XH-024 (l.2081)

**Finding.** The spec breaks its own decomposition rule for paid bodies.
- §10.6 says: "No Round-6 paid body is a skeleton, because the spec fixes every first-pass design."
- REQ-XH-001 says: "No round SHALL decompose a paid execution body before its gate is recorded, unless that body is a `Kind: skeleton`."
- REQ-XH-024 says: "Paid bodies after a gate SHALL be either skeletons (no Run line) or rows in a round decomposed after that gate."

The W2, W3 and W5 run rows sit after FP-S, FP-Q and FP-E. They are decomposed in Round 6, before those gates are recorded. "The design is fixed" is not an exception either rule grants. The validator's skeleton checks (AC-10.3-3, AC-10.6-1) will fail on the very rows that make up the first pass. §10.1.6 does not list REQ-XH-001 or REQ-XH-024 as adapted.

**Proposed disposition.** Adapt by choosing one of two options and stating it in §10.1.6 and §10.6:
- (a) Make the W2/W3/W5 run rows `Kind: skeleton` and fill them by a scoped `decompose-spec` insert after each gate, as Round 5 did with its plan extensions.
- (b) Add an explicit first-pass exception to REQ-XH-001 and REQ-XH-024: "a paid body whose complete design is fixed in a ratified pre-registration MAY be decomposed before its gate, with its Run line inert until the gate PASSES (guard-enforced)". Add a guard fixture for it.

(b) changes a build-orchestration invariant (OL-16), so the operator must decide.

**Operator decision needed:** y.

### RB-2 · blocker · FP-5 (l.138), §7.5 l.1497 (REQ-XD-111), REQ-XH-113 (l.2000), §10.1.3

**Finding.** The time-separated L2 design conflicts with the ratified §4.4 independence rule, and no ADR is proposed for the departure. §0.1 says the ratified §4.4 predicates "apply unchanged except where §10.5 proposes an ADR". `docs/evidence-and-adoption-spec.md:300` says:

> "Across actual pairs must be independent… Known common shocks, cache/carryover or reused subjects require refusal or another admitted design."

The landed code admits only `REGISTERED_DEPENDENCE = "independent_pairs"` (`crates/hh-eval/src/parity_admission.rs:52-55`). It lists "shared shocks" as `UnsupportedDependence` (l.496-498), and it relies on a declared `independence_rationale_ref` (l.156-159).

Under the first-pass design every O first attempt runs in W2/W3 and every H first attempt runs in W5. Any window-level shock is therefore a common shock to every pair and aligned with the arm label. That makes it a bias, not noise. Concrete paths:
1. **Queue mix.** Queue wait counts against the agent timeout (§6.8). In W2/W3, O shares the ≤ 3-request proxy with Pi and Claude Code. In W5, H shares it with V5. Timeout rates, and so capability, differ by window composition.
2. **Undetectable served-model drift.** Served weights are `unknown{no_served_fingerprint}` (§7.5). Ollama can change quantization or routing without a catalog change. A catalog-snapshot equality check (AC-10.1-4) cannot detect that, so the typed unknown becomes a direct threat to the claim.
3. **The credit reset** (2026-11-22) and provider load are likely to fall between W3 and W5. The gap holds FP-RECORD-PARTITION, E2-T9, E2-T10, PROV-W4, W4, G-L1 and FP-E (l.2026-2028).

§10.1.3 calls this "pending operator and O2 review". However:
- §10.5 holds no ADR or method admission for it.
- The method-admission gate (l.2047) does not list it.
- Appendix B has no Q row for it. Q-X47-P1…P3 cover ε, V5 and Q-X15 only.

**Proposed disposition.** Adapt. Either:
- (a) Redesign so that L2 is simultaneous. Record a cheap O-only L1 corpus on **development** tasks before FP-S. D2's own design was 16 dev tasks × 3 (`docs/design/36_hir-parity-method.md:396`); a reduced 28-trajectory version costs ≈ $7.8 planning at $0.279/trial. Then run the O and H arms (and V5) interleaved in one window after G-L1. This also removes RB-11. It costs room for (f): cumulative planning becomes ≈ $64 > $56.
- (b) Keep time separation, but add an ADR (method admission under §4.3/§4.4) that states the shared-shock rationale. Add a pre-registered validity veto: drift in the window-composition metrics (median queue wait, infra rate, timeout rate by window) beyond declared thresholds sets `inconclusive{time_separation_confound}`. Add a Q-X47-P4 row.

**Operator decision needed:** y.

---

## Major

### RB-3 · major · FP-5, §7 preamble, §10.1.3 P3-L2, §10.1.6 l.1989, Appendix B Q-X34 (l.2679)

**Finding.** The recorded-default margin ε = ±.25 is presented as making the verdict "likely `inconclusive`" and as fixed "from D2's tables". Neither holds.
- D2 Table B does not tabulate N = 50 or ε = .25. The spec's own §7.5 table says "not tabulated" (l.1487).
- Recomputed with `rb3.py` (exact trinomial enumeration over (c+, c−) with the K_η bound, η = .025, strict verdict rule):

| scenario at N = 50, ε = ±.25 | P(`equivalent`) |
|---|---|
| Δ = 0, q± = .117 | **.217** |
| Δ = 0, q± = .05 | **.886** |
| true Δ = +.10 (q+ = .167, q− = .067) | **.116** |
| true Δ = +.15 | .051 |

The q prior is HF's low-effort flip from a different serving setup, and the spec itself says so. So "likely inconclusive" depends on that prior. If Ollama's discordance is lower, `equivalent` becomes the likely outcome.

An `equivalent` at ±.25 would earn the dated label "Parity-verified baseline" (REQ-XD-126, l.1524). That margin is wider than the whole HF harness spread of 45.6–67.8 %, which §7.5 itself uses to argue that even ±.15 "excludes a gross infidelity, not a harness-sized difference". There is also a ≈ 12 % chance of `equivalent` under a true 10-point gap.

±.25 is outside Q-X34's offered options (±.15 / ±.20; ledger l.185), and REQ-XD-115 says "No default margin exists".

**Proposed disposition.** Adapt:
- Quote the table above in §10.1.3. REQ-XH-119 requires the expected precision to be stated before data.
- Either:
  - drop the recorded default ε and present FP-S with the menu {±.15, ±.20, ±.25} and these probabilities; or
  - keep ±.25 but make the first-pass `equivalent` verdict yield only a scoped label, never "Parity-verified baseline": e.g. "first-pass L2 equivalent at a ±.25 margin (gross-infidelity screen)", with a REQ-XD-126 first-pass clause and a mutation test.

**Operator decision needed:** y (Q-X34 / Q-X47-P1).

### RB-4 · major · FP-8 (l.141), §10.1.5 l.1938-1958

**Finding.** The first-pass budget is planned on low-effort cost priors while claiming otherwise. FP-8 says:

> "Cost priors are used without any low-effort discount"

The priors are HF **low-effort** values. `docs/research/27_harness-survey.md:95-107` is captioned "(Low effort, $/trial.)". mini-swe-agent's no-cache $0.155 is T × 0.15 at low effort. The "cached" column uses $0.043, although the same bullet cites "HF measured $0.057 at `high`". The high-effort observed values are mini $0.057, Pi $0.028, Claude Code $0.044 (`27_harness-survey.md:75`).

The spec's own "high-effort sensitivity" column is therefore the honest planning line at the pinned effort `high` (Q-X22, Q-X47d). On that line, recomputed with `rb.py`:
- cumulative planning through (e) is **$56.90**, which is above the $48 root cap and above the $56 cap at a $70 authorization;
- through (f) it is $75.59.

So the claim "Against a $48 root cap… steps (a)–(e) fit at planning values" holds only at low-effort planning. At `high` the HH-native arm, which carries both P3-L2 and the phase-4 baseline, does not fit at planning on either authorization. Expected spend still fits: no-cache × 1.34 ≈ $42.

**Proposed disposition.** Adapt:
- Rewrite FP-8 to "planning priors derive from low-effort HF token counts; the high-effort sensitivity column is the binding planning line".
- Restate the "Reading the model table" bullets on that line.
- Use $0.057 / $0.028 / $0.044 (or ×1.34) in the expected column.
- State the consequence for the FP-S authorization ($60 vs $70, and whether (e) is protected by dropping Claude Code or (f)).

**Operator decision needed:** y (authorization size, Q-X21).

### RB-5 · major · §10.1.5 compute table (l.1961-1968), FP-9, REQ-XH-118 (l.2005) vs §3.10 l.650 and REQ-XB-209 (l.630)

**Finding.** The compute "hard bound" leaves out a term of the spec's own ceiling formula and has no headroom for any repeated window.
- §3.10's ceiling is `$_D = r·D + S·r_disk·h_r + 0.12·E_gib`. §10.1.5 computes only `r·D + 0.12` per window: recomputed, Σ = $14.736.
- The residual-disk term is missing. The default dead-man action is STOP (REQ-XB-209; B3 recommends STOP, `docs/design/32_compute-backend.md:169`), which keeps the 300 GiB pd-balanced disk at ≈ $0.041/h. A residual of just 2 h per window adds $0.41, giving Σ ≈ $15.15, which fails REQ-XH-118's AC (Σ ceiling_micros ≤ 15 000 000). Twenty-four hours per window adds ≈ $4.93.
- With $0.26 of headroom, nothing that needs another window has a budget:
  - a W1 re-proof that overflows into replacement successors;
  - a PP0 repair re-run (REQ-XD-041);
  - an L1 fail → fix → rerun on gate-B (REQ-XD-106);
  - a harvest after a dead-man stop, which needs a VM restart, and that "SHALL require a new authorization" (REQ-XB-209).

  OL-17 ("re-provisioning is a budgeted, recorded decision") is therefore unmet in the first pass.
- Egress (≤ 1 GiB per window) is an assumption, not a GCE-enforced cap. Harvest over IAP is egress.

"Hard-bounded … compute $14.73" (l.1977) is overstated.

**Proposed disposition.** Adapt:
- Include the S·r_disk·h_r term with a declared h_r per window.
- Either set the first-pass dead-man action to DELETE after an in-guest harvest (Q-X38), or bound h_r by a teardown deadline.
- Re-sum against $15, or move one contingency window into the authorized amount, for example by lowering the model authorization.
- State that egress is estimated, not capped.

**Operator decision needed:** y (Q-X38, contingency allocation).

### RB-6 · major · §10.1.1 "Enforcement", REQ-XC-112 (l.799), REQ-XB-212 (l.633), HUMAN rows (l.2065-2066), §4.7 l.680

**Finding.** The model-side hard bound has three unclosed paths.
1. **Auto-billing.** The Ollama HUMAN row exits on "automatic usage billing confirmed off, **or recorded on with consequence**". With billing on, nothing on the provider side stops at the authorized amount. Non-proxy use (the operator's own use, which counts per REQ-XH-013, or a leaked key) is unmetered. §4.7 l.680 adds that usage has no per-key breakdown, so the "optional" dedicated key cannot separate it. For a $100 **maximum** (Q-X47), the exit must be "off", or the gate must record that the ceiling is not provider-enforced.
2. **No cap carry-over rule.** REQ-XC-112 says "Each paid phase SHALL hold one proxy run whose root budget pool equals the root cap". The first pass spends across W2, W3 and W5, on fresh VMs with the proxy on the VM. The spec has no rule that W_n's root pool = cap − Σ prior consumed, with orphaned calls at `bounded.hi`. A window whose harvest fails (dead-man) has unknown consumption, and nothing stops the next window from starting at the full cap.
3. **Phase membership.** W5 mixes phase 3 (L2 H side) and phase 4 (V5). REQ-XB-212 requires a fresh project per phase, and REQ-XH-013 forbids concurrent paid collections. Which project, and which proxy run, W5 belongs to is undefined.

**Proposed disposition.** Adapt:
- Add a first-pass requirement: "the first pass is one paid phase for REQ-XC-112 and REQ-XB-212 purposes; each window's proxy root pool = first-pass root cap − Σ prior consumed-or-bounded-hi from harvested ledgers; a window with an unharvested ledger blocks the next paid window."
- Make auto-billing-off a FP-S precondition, or record the downgrade verbatim.
- Add REQ-XH-013 and REQ-XB-212 to §10.1.6.

**Operator decision needed:** y (Q-X21).

### RB-7 · major · REQ-XH-117 (l.2004), §10.1.4 (d) "protect-e" (l.1926), priority (l.1931), REQ-XH-112 AC (l.1999)

**Finding.** The per-arm caps and drop rules are underspecified and inconsistent, and they can cascade.
1. **No per-arm cap values.** None are given for any first-pass arm. If the caps are proportional to planning $/trial **and** partition the $48 root, each arm's cap is ≈ 85 % of its own planning cost (48 / 56.4), and the drop rule fires on every arm at planning values. If the caps do not partition the root, the root cap can cut (e) mid-window, outside any gate.
2. **The reference arm can be dropped.** The drop rule applies to "each first-pass arm", including original mini-swe-agent. That arm is the P2-FP reference, the L1 corpus (≥ 28 trajectories, REQ-XD-107) and the L2 O side (R ≥ 5). Dropping it, or "protect-e" running (d) "only for arms whose projected cost leaves (e) funded", leaves at most 20 mini trials. Then L1 cannot meet its corpus floor, the L2 S3Floor refuses, and (e) loses its purpose.
3. **Contradictory truncation order.** "protect-e" reverses the declared order "(f), then (e), then (d)" and contradicts AC-10.1-3, which withholds "exactly the later steps".
4. **Undefined projection.** REQ-XH-112 checks "projected planning cost", but FP-Q projects "from measured $/trial". Which applies is undefined.

**Proposed disposition.** Adapt:
- Tabulate per-arm caps (µ$) in the pre-registration.
- Exempt mini from cost-only dropping in (b)–(d), or declare that a mini drop withholds P2-FP, P3-L1, P3-L2 and P4-FP with typed reasons.
- Restrict "protect-e" to non-reference arms and state that it amends the priority order.
- Define "projected cost" as a single formula: measured mean $/trial × 1.5 × 1.2, with the planning prior as the floor.

**Operator decision needed:** y.

### RB-8 · major · §10.1.4 (e)/(f), FP-6, REQ-XH-115 (l.2002)

**Finding.** Interleaving (e) and (f) in one W5 schedule (needed for P4-FP's exchangeability) makes "truncation lands on (f) before (e)" unenforceable inside the window. REQ-XH-112 checks only "before each step". If root-cap exhaustion arrives mid-W5 (in expectation (f) fits; at planning it does not, per RB-4), it truncates both arms at once. The HH-native arm then has R < 5 on some tasks: the L2 S3Floor refuses and REQ-XD-002 refuses. Including (f) thus puts (e), the higher-priority claim, at risk.

**Proposed disposition.** Adapt. Add an in-window rule: before each round, if spend-to-date + remaining (e) projected + remaining (f) projected exceeds the remaining cap, withhold all remaining (f) slots (`withheld{budget}`) and continue (e). Alternatively include (f) at FP-E only if the planning (not expected) cost of (e)+(f) fits. Add a fixture for it.

**Operator decision needed:** n.

### RB-9 · major · §10.1.3 P2-FP and P4-FP (l.1912), REQ-XD-024 (l.1205), REQ-XH-119

**Finding.** At T = 10 the confirmatory families are, in most tranches, structurally unable to reject anything, and the spec does not say so.
- The sharp-null sign flip (`crates/hh-eval/src/stats.rs:594`) enumerates task-level deltas. Zero-delta tasks contribute nothing, so with m non-tied tasks the minimum two-sided p is 2/2^m. Holm's first threshold of .025 needs m ≥ 7.
- `rb2.py` uses D1's `board` mixture Beta(.575, .384) with σ = .75, k = 5, T = 10, 3,000 datasets:
  - mean non-tied tasks = 5.1;
  - **P(m ≥ 7) ≈ .20**, so in ≈ 80 % of tranches no rejection is possible regardless of effect;
  - P(raw p ≤ .025) ≈ .003 at Δ = 0, .018 at Δ ≈ .09, .048 at Δ ≈ .13, .146 at Δ ≈ .20;
  - median percentile-bootstrap half-width ≈ .14.
- "power for a 10-point gap low" understates a value of ≈ .02.
- REQ-XD-024's battery grid (H ∈ {2,4,8}, T ∈ {20,37,48}) does not cover the first-pass design (H = 3, T = 10), yet it "SHALL run on the exact pipeline before the first confirmatory subject". §10.1.6 does not adapt it.

**Proposed disposition.** Adapt:
- Add T = 10, H = 3 (and H = 2 with a dropped arm) to the battery for the first pass.
- Quote P(m ≥ 7), the power table and the half-width in the pre-registration and in REQ-XH-119's label.
- Consider labelling P2-FP and P4-FP "pipeline demonstration; confirmatory test pre-stated as near-zero power". No AC change; this is an honesty addition.

**Operator decision needed:** n (y if the operator wants to drop (f) for its ≈ 0 inferential yield).

### RB-10 · major · §10.1.4 Firewall (l.1933), FP-4, REQ-XH-112, FP-Q/FP-E gate content (l.2049-2052)

**Finding.** "Cost-only" continuation is not outcome-blind, and the human gates are not blinded.
1. **Cost tracks failure.** On TB 2.1, failures run longer (to the agent timeout) and make more calls, while successes end at the submit marker. Per-arm measured $/trial therefore correlates with the arm's failure rate. The Claude Code drop rule, "protect-e" and the (f) inclusion decision are partly outcome-driven. This is what FP-4 relies on when it argues rounds 1–2 are not a pilot.
2. **The operator can see outcomes.** No requirement stops round-1–2 verdicts from being visible in committed slot records, gate readouts or the internal site profile when the operator picks the FP-Q menu row or decides (f). A human menu choice with outcomes visible is outcome-based selection, which the "Firewall" sentence does not prevent.

The Holm family stays conservative (unavailable members retained), but the descriptive record and which claims exist become outcome-dependent.

**Proposed disposition.** Adapt:
- Add an outcome-masking requirement: verdict and reward fields of first-pass slots are sealed (withheld from readouts, site internal profile and committed summaries) until FP-RESULTS. Gate readouts carry only cost fields, with a fixture.
- Replace "No gate or rule reads an outcome field" with a disclosure that cost is outcome-correlated, so drop decisions are reported as "cost-triggered; cost correlates with failure length".

**Operator decision needed:** n.

### RB-11 · major · REQ-XH-114 (l.2001), REQ-XD-107 (l.1462), REQ-XD-119 (l.1517), REQ-XH-012 (l.1337), REQ-XD-102 (l.1457)

**Finding.** Reusing the original-mini trials for L1 creates selection and independence problems the spec does not address.
1. **Same tasks, same trials.** The dev partition H is developed against is drawn from the same 10 held-out tasks and the **same 50 trials** that form L2's O side. That is a configuration step reading another phase's held-out outcomes, which REQ-XH-012 forbids. §10.1.6 does not adapt REQ-XH-012; REQ-XH-114 only "discloses" the overlap.
2. **Partitions share tasks.** The partition is by trajectory (`36_hir-parity-method.md:123` does not say by task). Dev, gate-A and gate-B then share tasks, so the gate re-tests instance text that H was tuned on.
3. **The audit is not independent.** The L1-on-L2 audit "replay[s] sampled O recordings (e.g. 10 % of pairs)". In the first pass 28 of the 50 O recordings are already L1 corpus, 12 of them dev. A 5-recording audit will likely replay recordings H was built against.
4. **H is frozen after O data exist.** §4.4 needs a pre-data admission packet naming the actual participant (`evidence-and-adoption-spec.md:340`). The H identity is frozen at G-L1, after the O-side data exist. REQ-XD-102 ("No paid L2 pair SHALL run before G-L1") is broken for the O half and is not listed in §10.1.6.
5. **"Pilot" wording.** REQ-XD-107 still says "pilot RECORD trajectories".

**Proposed disposition.** Adapt:
- Partition **by task**: for example 3/2/2 tasks gives 15/10/10 trajectories, which meets ≥ 12/8/8.
- Reserve the remaining 3 tasks' 15 O trajectories as the only L1-on-L2 audit pool.
- Strip verdict and reward fields from the dev partition delivered to H development.
- Add REQ-XH-012, REQ-XD-102, REQ-XD-107 and REQ-XD-119 to §10.1.6 with first-pass text.
- State in the L2 packet that H's identity is fixed by rule ("the version that passes G-L1") before O data, and frozen by digest after.

RB-2(a) removes most of this.

**Operator decision needed:** n.

### RB-12 · major · §9.2 (l.1725-1726), §9.3 `hh-site-data/1` (l.1754), §9.4 `site_banned` (l.1784) vs REQ-XF-105 (l.1709), §10.6 Round 6 "publication"

**Finding.** First-pass publication of phases 3 and 4 is not buildable as specified, and the claims lexicon blocks a legitimate L2 verdict.
1. **Missing contracts and tickets.**
   - `hh-site-data/1` lists only `phase1.json`, `t⟨n⟩.json`, `trials-t⟨n⟩.json`, `spend`, `method`, `claims` and `changelog`, with no phase-3 or phase-4 projection.
   - No site-facing packet schema exists for the L2 verdict, L3 or P4-FP. Q-F2-8 is open: `docs/design/39_results-site.md:892` says "Phase-4 page contract (no decision)", and l.443 says phase 4 is "not designed in detail".
   - The F2 tickets F2-a…F2-g have no phase-3 or phase-4 page ticket (`39_results-site.md:814-820`).
   - REQ-XF-118 forbids computing anything in the generator.

   So the `/phase-3/` and `/phase-4/` pages that §10.6 publishes in Round 6 have no contract and no owner.
2. **"equivalent" is banned site-wide.** `site_banned` includes "equivalent" and "no difference" and applies to every visible string (REQ-XF-134). REQ-XF-105 requires every disposition to be "rendered verbatim", and §1.3 says L2 can yield `equivalent`. A favourable L2 outcome would therefore fail CI on the site: an asymmetry in claims handling that conflicts with REQ-XH-020's equal-prominence rule.
3. **"leader" collides with a mandatory label.** Bare "leader" is banned, but REQ-XF-111's mandatory fixed label "observed-leader group…" contains "leader" at a word boundary.

**Proposed disposition.** Adapt:
- Add `phase3.json` and `phase4.json` to `hh-site-data/1`. Name the L2 source packet (landed `parity-admission/1` plus the directional-KL report) and a P4-FP results schema (reuse `hh-comparative-results/1` with a family-of-one).
- Add a Round-6 site ticket for the phase-3/4 pages.
- Move "equivalent" to `conditional`, allowed only inside "L2 KL verdict `equivalent` (ε = ±…, N = …, family …)".
- Define "bare leader" (e.g. not preceded by "observed-").

**Operator decision needed:** n (Q-F2-7 ratifies the lexicon).

### RB-13 · major · §10.1.6 adaptation table (l.1981-1995), §10.5 register

**Finding.** The adaptation table omits requirements the first pass breaks, and §10.5 has no record for the first-pass departures.

Breached but missing from the table: REQ-XH-001/024 (RB-1), REQ-XD-102, REQ-XH-012, REQ-XD-107, REQ-XD-119 (RB-11), REQ-XC-112, REQ-XB-212, REQ-XH-013 (RB-6), REQ-XD-024 (RB-9), and REQ-XD-012 (the pilot set P, which no longer exists).

Under §0.1, departures from the ratified §4.3–§4.5 need an ADR. §10.5 has none for:
- time-separated pairs (§4.4);
- a T = 10, H = 3 family below the §4.5 precision target;
- the family-of-one P4 contrast;
- the shared baseline.

REQ-XH-110…119 are synthesis-allocated rulings with no ADR or REC home. The method-admission gate content (l.2047) does not cover these departures.

**Proposed disposition.** Adapt:
- Complete §10.1.6.
- Add AR-33 "First-pass design and departures (Q-X47)" (ADR via the §4.3 route, first needed by FP-PREREG) and list its items in the method-admission gate.

**Operator decision needed:** n (y for the substance via RB-1/RB-2).

### RB-14 · major · §10.2 DAG, §7.3 ticket order, C-05, §10.5 AR-24

**Finding.** The phase-3 build is on the paid critical path with no declared fallback.
- The HH-native arm (e) needs the whole E2 chain (T0 → … → T10, including the T8 detach/reattach spike) plus K1–K4, a canonical amendment (AR-24, AMEND-C, a frozen-spec change) and a possible rung-3 ADR raised to the operator.
- The DAG sequences correctly (G-L1 before FP-E) but starts paid W2/W3 (FP-S) before the E2 chain is known to be feasible.
- If the T8 spike lands on rung 3, or AR-24's amendment stalls, the O-side data sit unused. The O–H gap grows (RB-2), and nothing says when the L2 cohort is abandoned.
- No gate row exists for AR-24's canonical amendment.

**Proposed disposition.** Adapt:
- Add an FP-S precondition that E2-T8's ladder rung is recorded (rung 1 or 2), or move FP-S after E2-T8.
- Add a declared expiry: if (e) has not started within D days of W3, or a catalog change occurs, P3-L2 is `withheld{time_separation_expired}` and P4-FP runs alone.
- Add the AR-24 amendment-approval gate to §10.3.

**Operator decision needed:** y (sequencing vs the credit reset, Q-X46).

---

## Minor

### RB-15 · minor · §10.1.5 step (b) (l.1946) vs §10.1.4 (b) (l.1924)

**Finding.** The step (b) budget line does not match the step's stated content.
- The content is PP0 "3 dev tasks × 1 × 3 hosted arms" plus "R-live (1 task × 3 on O)", which gives mini 6, Pi 3, Claude Code 3. That costs $1.39 no-cache (with PS0), $2.42 planning and $3.24 at sensitivity.
- The table's $1.22 / $2.16 matches a 4/4/4 split instead.
- §6.6 defines R-live as "1 task × 1 per AG9-eligible harness".
- The automatic (b) → (c) continuation keys on this planning line.

**Proposed disposition.** Fix either the R-live text or the row values.

**Operator decision needed:** n.

### RB-16 · minor · §0.3.1 l.56, FP-3/FP-8, Appendix B l.2697-2700

**Finding.** Operator-decision IDs and verbatim text do not match the ledger.
- The spec cites "Q-X47a–d". The ledger records them as **Q-X48, Q-X49, Q-X50**, with effort as **Q-X22** (`docs/experiments-research-ledger.md:173, 199-201, 223`).
- Q-X47-P1…P3 exist only in the spec.
- The verbatim cells mix operator text with coordinator gloss after "—": the Q-X48 harness list additions and the Q-X50 "15 % reserve". Whether the operator chose the 15 % reserve is therefore ambiguous.

**Proposed disposition.** Renumber to the ledger ids and split verbatim from gloss. Add Q-X47-P1…P4 rows (P4 = time separation, RB-2) to ledger §Q.

**Operator decision needed:** n.

### RB-17 · minor · §10.2 l.2025 vs REQ-XH-116 and REQ-XD-010

**Finding.** The task draw is placed after the re-proof it defines.
- The DAG places "FP-PREREG (selection…)" **after** W1, but W1's re-proof scope is "the drawn first-pass tasks" plus successors.
- Successors are re-proofed "as needed" inside a fixed-D window that has no budget for them.
- Replacement within the difficulty cell (to keep stratification) is not stated.

**Proposed disposition.** Commit the split, the draw and the replacement order before PROV-W1. Pre-re-proof K successors per difficulty cell inside W1's quote. State replacement within the cell.

**Operator decision needed:** n.

### RB-18 · minor · §10.1.5 l.1970, §6.8, C-17

**Finding.** Retry handling across windows is undefined.
- Retry placement across windows is undefined. A W3 `inconclusive{backend_terminated}` or infra slot has no later phase-2 window to retry in, and D is set below the timeout bound.
- The populations clash:
  - phase 2 counts the first admissible retry, while L2 uses the first attempt with zero replans;
  - in W5, phase 4 permits retries while L2 forbids them for the same HH-native trials.

**Proposed disposition.** State that retries run in the same window or become unresolved. State that L2 always reads the pre-assigned first attempt, and that a retry subject is a new subject used only by P2/P4.

**Operator decision needed:** n.

### RB-19 · minor · REQ-XF-129, REQ-XF-130, REQ-XH-020 (l.1867-1873), REQ-XH-110 AC (l.1997)

**Finding.** Some acceptance layers are mislabelled, and the $100 check can pass on estimates.
- P-layer requirements carry fixture or manifest ACs (E or D evidence), which would let fixture passes count toward the P accounting.
- REQ-XH-110 is labelled layer D yet needs a real spend recompute. Its AC sums "settled-or-metered" and "billed-or-estimated" amounts, so the $100 check can pass on estimates even when a bill exists, and orphaned calls are not taken at `bounded.hi`.

**Proposed disposition.** Relabel the layers. In AC-10.1-1, use billed figures where available and `bounded.hi` for orphans.

**Operator decision needed:** n.

### RB-20 · minor · §10.3 HUMAN rows

**Finding.** HUMAN rows are missing or incomplete.
- There is no HUMAN row for **purchasing Ollama credit** when model spend above $60 is authorized (FP-12). The spec does not say whether unused purchased credit counts against the $100.
- The dedicated key is "optional", although settlement cannot separate per-key use (l.680).
- The DAG shows HUMAN reconcile only after W1, while the table requires it after every TEARDOWN.

All other expected rows are present: GCP billing alert, Ollama settings, HF dataset, Pages, flagged-pass review, dataset flip.

**Proposed disposition.** Add an "Ollama credit purchase (if authorized)" row with an exit that records the amount and how unused credit is treated. Show reconcile-Wn for every window.

**Operator decision needed:** y (unused-credit accounting).

### RB-21 · minor · Appendix F / `docs/experiments-OUTLINE_TRACE.md`

**Finding.** Four outline obligations are not fully discharged as claimed.
- **OL-01** asks for a "proof-of-correctness record". The spec bans that phrase (S-8) and builds a recorded-execution record. That is a conscious re-scope, but the scope note is "—".
- **OL-16** includes "size trigger → stop for fresh-context scoped review" (brief l.126). No spec section mentions it (`rg "size trigger"` finds no hits), yet Appendix F claims §0.1, §1.4, §10.3 and §10.6 discharge OL-16.
- **OL-07** ("report which perform best") cannot be met by a family-of-one. It should be marked `de-scoped(first pass, Q-X47-P3)` conditional on later funding, not `mapped`.
- **OL-17**'s re-provisioning budget is absent in the first pass (RB-5).

**Proposed disposition.** Add scope notes for OL-01, OL-07 and OL-17. Add a size-trigger clause to §10.6 (worker sizing rule plus a stop-for-review trigger).

**Operator decision needed:** n.

### RB-22 · minor · §10.2 DAG l.2019; Linearization B

**Finding.** The DAG drops dependencies and reuses a stale critical-path count.
- `D1-c ∥ ARCHIVE ∥ F2-c ∥ F2-d` drops F2-c's dependencies on F2-b (after GATE mock) and D1-b, and F2-d's on F2-b and ARCHIVE (`39_results-site.md:816-817`). A mock-gate redirect would orphan component work.
- The "35–38 implement-spec runs" for Linearization B is H1's count for the shared-pilot plan (`40_program-plan.md:144`), not recounted for the first pass. The first pass adds Claude Code admission, FP-PREREG and the HUMAN rows, and drops five adapters.

**Proposed disposition.** Restore the edges and recount the critical path, or label the figure "H1 estimate, not recounted".

**Operator decision needed:** n.

### RB-23 · minor · Status/baseline (l.5), §10.6 handoff (l.2159)

**Finding.** The baseline is stale.
- `main` has advanced to `185c16c7` (BL-RECONCILE). It edits `docs/build/LEDGER.md`, BACKLOG and BUILD_INDEX, and records BL-50 as still open.
- The handoff "append after row 251 on main" still holds (row 251 is the last manifest row), but `decompose-spec mode=extend` will read the newer LEDGER.
- The handoff does not name the frozen-digest step or the `decomposition-prompt.md` artifact that synthesize-spec's close writes.

**Proposed disposition.** Rebase to the current `main`. Cite BL-50's relation, if any, to the program. Name the decomposition-prompt hand-off file and the frozen digest.

**Operator decision needed:** n.

### RB-24 · minor · §10.1.4 (e)/(f)

**Finding.** The HH-native and V5 arms get no paid smoke. Their first paid calls are W5's schedule. PP0's infra rule ("≥ 1 of 3 trials infra-failed holds that harness") covers only the hosted arms; L1 and D3-FP are $0 replay and do not exercise the live HH-native proxy lane.

**Proposed disposition.** Add a ≤ $0.05 PS0-style live probe of the HH-native lane at W5 start, before the first scheduled slot, plus an infra hold rule for the HH-native and V5 arms.

**Operator decision needed:** n.

### RB-25 · minor · §10.1.3 "each pre-registered separately"

**Finding.** The four claims share trials and are not independent. P2-FP and P3-L2 share mini trials; P3-L2 and P4-FP share HH-native trials. No statement says that no cross-claim familywise control is claimed, or that the claims are dependent. A single unlucky window moves several claims together.

**Proposed disposition.** Add one sentence to REQ-XH-113 and the pre-registration.

**Operator decision needed:** n.

---

## Counts

| severity | count | findings |
|---|---|---|
| blocker | 2 | RB-1, RB-2 |
| major | 12 | RB-3 … RB-14 |
| minor | 11 | RB-15 … RB-25 |

## Overall assessment

The first-pass plan has the right goal: one seeded task set, every arm on it, k = 5, one pre-registration, cost-gated steps. The budget table's arithmetic is right as far as it goes; every row except (b) recomputes. But the plan overstates what it guarantees:
- **The $100 bound is not hard as specified.** The residual-disk term is omitted, auto-billing is permitted, there is no carry-over rule for the root cap across windows, and there is no contingency for any repeated window.
- **Model planning rests on low-effort priors** while claiming not to. At the pinned `high` effort, the HH-native arm, which carries both the parity and phase-4 claims, does not fit the $48 or $56 cap at planning values.

**The trial reuse is the weakest part:**
- Time-separated L2 pairs conflict with the ratified §4.4 independence rule ("known common shocks… require refusal or another admitted design"), and no ADR proposes the departure.
- H is developed against recordings of the same trials it is then compared with.
- The recorded-default margin ε = ±.25 is not "likely inconclusive" in any robust sense: P(`equivalent`) ≈ .22 at the central prior and ≈ .89 at q = .05. It could earn "Parity-verified baseline" on a margin wider than the entire harness spread.
- At T = 10 the confirmatory phase-2 and phase-4 families cannot reject anything in about 80 % of draws, and the spec does not say so.

**Governance problems:**
- The spec contradicts its own REQ-XH-001/024 skeleton rule.
- It ships a site lexicon that would block publishing a favourable L2 verdict.
- It has no contract for the phase-3/4 pages it promises to publish.

**What it does well:** the operator-decision departures from Q-X15 (and ε, V5) are flagged pending, and no gate content is pre-answered.

It is not ready to ratify until RB-1 and RB-2 are resolved and RB-3 to RB-8 are folded in. Most fixes are textual; RB-2(a) is the one design change worth putting to the operator, because it removes RB-2 and RB-11 together for about $8 of planning headroom.
