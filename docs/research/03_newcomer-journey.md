# 03_newcomer-journey — From a working fixture to a useful, inspectable task

Stream: A — Capability and evidence audit  
Researched: 2026-10-29 UTC  
Author: /root/a3_journey  
Ledger rows covered: A3  
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; uncommitted planning and accepted A1/B1/B2 notes present; no implementation changes.  
Confidence overall: high for the reproduced paths and cited request mismatches; medium for proposed priorities; no observed newcomer cohort.

The existing first-run documentation is substantially useful and honest. Its
fixture runs and can be inspected after restarting the kernel. The next step
needs both explanation and engineering: help does not discover Lab commands,
`compare report` sends an invalid request, and an exported successful fixture
fails R0 reproduction because lifecycle vocabularies disagree. A guided trace
and a useful task remain worthwhile, but “pure docs over proven machinery” is
not a safe estimate for the whole proposed sequence.

## Scope, procedure and execution receipts

Read the deferral index first: `bash scripts/deferrals-open.sh` exited 0 with
empty output. Read root/docs instructions, brief and approval addendum,
[A3/dependencies](../research-ledger.md), [conventions](CONVENTIONS.md),
[accepted A1](01_capability-inventory.md), relevant [B1](04_audiences-and-jobs.md)
findings, [outline](../OUTLINE_TRACE.md), current guides and bounded source.
The operator-supplied alternative proposal was read directly; its numbered
suggestions are dispositioned below. Its private attachment location is omitted.
Source inspection checkpoint: **2026-10-29T18:21:05Z** (all citations below were
read by this checkpoint). Historical test/receipt references are not new runs.

`cargo build --offline -p hharness-cli -p hharness-kernel` exited 0; Cargo
reported 21.89 s. These freshly built binaries were used for every probe.
Host: Darwin arm64; rustc 1.94.1, Cargo 1.94.1, Python 3.14.8. Toolchain and
Cargo dependencies were already cached. This does not establish clean-install
speed, release-binary behavior or another platform. No provider/network model
calls, paid work, external writes, broad benchmark or Rust test suite occurred.
The parent's earlier clean-clone quickstart check was **not** an A3 execution.

Scratch root retained for coordinator review: `/tmp/hh-a3-20261029-mw4_qz0d`
(`$S` below). It contains `records.json` (argv/time/exit), `environment.json`,
`requests.ndjson`, `replay.ndjson`, `read-result.json`, `receipts/*.stdout` and
`*.stderr`, `store/`, `workspace/`, `bundle/`, and `clean-store/` if materialized
by the command. These are ephemeral local receipts, not committed release or
ADR-0363 live evidence. Raw local receipt paths must be sanitized before sharing.

For a fresh reproduction, set `S=$(mktemp -d)` and run `mkdir -p "$S/workspace"`.
All runtime commands ran from the repository root with:

```sh
export HH_KERNEL_CMD="$PWD/target/debug/hh-kernel"
export HH_STORE_ROOT="$S/store" HH_WORKSPACE_ROOT="$S/workspace"
export HH_PRINCIPAL=a3-offline-audit
```

`requests.ndjson` used the README's four JSON-RPC requests: `hello`,
`open_session` with the unmodified `clients/ts/fixture-definition.json`,
`submit` invoking `hh.submit {}`, and `read` from seq 1 with limit 300.
Only client name/idempotency keys changed to `a3-audit`, `a3-open`, `a3-submit`;
store/workspace were scratch paths. This was an adapted scoped execution, not
the verbatim clone/build quickstart. `$R` below was the returned
`run-1793297991091-000000-7bcc80ab`; `sess-1` was process-local.

The following portable generator recreates the exact RPC shapes. Run it with
`python3 "$S/rpc.py" > "$S/requests.ndjson"` after saving this code as `$S/rpc.py`.
After the offline run, set/export `R` to its returned `run_id`, then run
`python3 "$S/rpc.py" > "$S/replay.ndjson"`. Start with `R` unset. Run the command
table in order; capture stdout/stderr separately to retain the verdicts.

```python
import json, os
from pathlib import Path

def rpc(i, method, params):
    return dict(jsonrpc="2.0", id=i, method=method, params=params)
hello = rpc(1, "hello", dict(contract_major=1, client=dict(name="a3-audit", version="1", kind="cli"), capabilities=dict(experimental=True)))
if os.getenv("R"):
    calls = [hello, rpc(2, "open_session", dict(spec=dict(kind="attach", run_id=os.environ["R"], read_only=True), idempotency_key="a3-attach")), rpc(3, "replay", dict(session_id="sess-1", driver_mode="deterministic"))]
else:
    doc = json.loads(Path("clients/ts/fixture-definition.json").read_text())
    spec = dict(kind="new", definition=dict(kind="document", document=doc), environment=dict(kind="connection_info", connection_info=dict(**{"class":"local_host"})), attendance=dict(value="async", source="declared"))
    calls = [hello, rpc(2, "open_session", dict(spec=spec, idempotency_key="a3-open")), rpc(3, "submit", dict(session_id="sess-1", input=[dict(kind="invoke", capability="hh.submit", args={})], idempotency_key="a3-submit")), rpc(4, "read", dict(session_id="sess-1", cursor=dict(kind="seq", seq=1), limit=300))]
for call in calls:
    print(json.dumps(call, separators=(",", ":")))
```

| Receipt stem / exact command after the environment above | Exit | Observed result |
|---|---:|---|
| `environment`: `sh -c 'uname -sm; rustc --version; cargo --version; python3 --version'` | 0 | Versions above |
| `help`: `./target/debug/hh --help` | 64 | `invocation_error`, usage mentions only run/approval/version/doctor |
| `experiment-help`: `./target/debug/hh experiment --help` | 64 | `unknown_command` |
| `doctor`: `./target/debug/hh doctor` | 0 | `connectivity:ok`, kernel 0.1.0 |
| `compare-report`: `./target/debug/hh compare report baseline candidate` | 65 | `SchemaViolation`, `/spec`, `missing_field` |
| `validate-hir`: `./target/debug/hh definition validate clients/ts/fixture-definition.json` | 65 | `/source.assembly: unknown AssemblySource member` |
| `offline-run`: `./target/debug/hh-kernel serve < "$S/requests.ndjson"` | 0 | RPC results; terminal `succeeded`, stop reason `completed` |
| `status`: `./target/debug/hh run status "$R"` | 0 | `finished:true`, 57 events |
| `events`: `./target/debug/hh run events "$R"` | 0 | Persisted event stream with terminal outcome |
| `context`: `./target/debug/hh run inspect "$R" context_view` | 0 | Context projection, four items |
| `replay`: `./target/debug/hh-kernel serve < "$S/replay.ndjson"` | 0 | RPC replay result: `mode:deterministic`, `reasons:[]` |
| `bundle-create`: `./target/debug/hh bundle create "$R" --sink "$S/bundle"` | 0 | Bundle delivered; visible `compile_error` for unresolved `sha256:profile` |
| `bundle-validate`: `./target/debug/hh bundle validate "$S/bundle"` | 0 | `complete:true`, `max_supported_level:R0` |
| `bundle-reproduce`: `HH_STORE_ROOT="$S/clean-store" ./target/debug/hh bundle reproduce "$S/bundle" --level R0` | 0 | **`verdict:not_reproduced`, `outcome:drift`**; declared `finished`, derived `succeeded` |
| `events-after`: `./target/debug/hh run events "$R"` | 0 | 61 events; original 57-event prefix unchanged |

Runtime probes executed **18:19:49–18:20:24 UTC**; exact per-probe times are in
`records.json`. `replay.ndjson` sent a fresh `hello`, then
`open_session{spec:{kind:"attach",run_id:$R,read_only:true},idempotency_key:"a3-attach"}`,
then `replay{session_id:"sess-1",driver_mode:"deterministic"}`. The two later
bundle/export records plus two replay lifecycle records account for the four
new events; no new `model.*` or `action.effect.*` rows appeared.

Counts: 15 recorded environment/runtime invocations, 11 exit-zero and four
nonzero (two help failures, one comparison defect, one expected input-shape
rejection). One of the exit-zero invocations has a negative reproduction
verdict. These are audit observations, **not 15 passing product tests**.
Build and deferral checks separately passed. Provider, release install,
full experiment execution, plugin build, web and foreign-client executions
were not attempted; no unavailable prerequisite was counted as a pass.

Note validation: `bash scripts/check-living-docs.sh` exited 0 (387 living files,
no personal paths); a scoped link/anchor check passed 34/34 links; the embedded
RPC generator matched both executed request files after JSON decoding (2/2).
The new-file `git diff --no-index` was reviewed; its exit 1 denotes a difference,
not a test failure. No Rust test/format verification claim is made for this note.

## Journey and material findings

### F1 — Setup works, but the first success is a lifecycle demonstration

Claim: The existing source-build route reaches an inspectable successful scripted run; it does not deliver an independently checked useful user artifact.

Status: VERIFIED  
Evidence kind: recorded execution; source inspection  
Evidence: `doctor`, `offline-run`, `status`, `events`, `context` receipts above; [README offline block](../../README.md#try-an-offline-run), [getting-started §1–2](../getting-started.md), and [fixture](../../clients/ts/fixture-definition.json). The guide already explains deterministic modeling, session versus run IDs, durable storage and terminal outcomes. [Installation guidance](../getting-started.md#install-from-a-release) still requires the repository fixture when using released binaries.  
Retrieved: 2026-10-29T18:21:05Z; executions as listed above.  
Implication for the spec: Preserve the smoke path as setup; add a selected useful task with a checkable artifact, explicit adaptation steps and a failure interpretation. Do not call setup time “time to useful value.”  
Outline delta: CONFIRMS OL-01; EXTENDS OL-05, OL-17.  
Limitations: No outside-user observation or clean installation; a scripted completion does not establish provider performance or task correctness.

### F2 — Discoverability is an interface gap; authoring confusion is partly explained

Claim: Both help probes fail to provide a usable command map, while the quickstart HIR is deliberately a different input type from definition-authoring commands.

Status: VERIFIED  
Evidence kind: recorded execution; source inspection  
Evidence: `help`, `experiment-help`, `validate-hir` receipts; [cli.rs `parse_args`, lines 350–362](../../crates/hh-cli/src/cli.rs); [lab.rs `cmd_definition_validate`, lines 260–285](../../crates/hh-cli/src/lab.rs); [AssemblySource module](../../crates/hh-lab/src/assembly/source.rs) specifies the wrapper. [Reference CLI entry points](../reference.md#cli-entry-points) honestly directs readers to source for exact operands and warns about the HIR distinction.  
Retrieved: 2026-10-29T18:21:05Z; executions above.  
Implication for the spec: Selected workflows need command/subcommand help or an equally discoverable complete recipe; show each file's schema and a supported editing/validation loop. The HIR rejection is expected validation, not evidence the validator is broken.  
Outline delta: EXTENDS OL-01, OL-17, OL-18.  
Limitations: No requirement to redesign every command; actual abandonment rates are unknown.

### F3 — The proposed Lab walkthrough is not ready as a documentation-only change

Claim: `compare report` reproducibly omits a mandatory analysis spec, and experiment claim/launch/settle have separately verified static request mismatches.

Status: VERIFIED  
Evidence kind: recorded execution for comparison; source inspection for experiment wrappers  
Evidence: `compare-report` receipt returns `/spec` before resolving the arbitrary baseline/candidate values; [lab.rs `cmd_compare_report`, lines 915–940](../../crates/hh-cli/src/lab.rs) sends only those two fields, whereas [analysis_ops.rs `lab_analysis_analyze`, lines 165–190](../../crates/hh-embed/src/analysis_ops.rs) requires `spec` then tasks. [lab.rs `cmd_experiment_op`, lines 762–781](../../crates/hh-cli/src/lab.rs) sends `experiment_id`, optional `cell`/`run_id`; [experiment_ops.rs, lines 715–800](../../crates/hh-embed/src/experiment_ops.rs) requires `run_plan_id` and launch `lease_id`. Its `experiment_run_id` (lines 590–609) resolves an open experiment first, so a fabricated ID would only prove `PlanNotFound`; that misleading reproduction was not attempted. [Exemplars](../../crates/hh-lab/src/exemplars.rs), lines 1–40 and 230–316, are parameterized document shapes, not turnkey task execution.  
Retrieved: 2026-10-29T18:21:05Z; comparison execution above.  
Implication for the spec: Repair/regression-test the selected CLI path or supply a checked SDK/RPC runner; then demonstrate fixture teaching separately from the real task/grader bridge owned by E1.  
Outline delta: CORRECTS OL-03; EXTENDS OL-08, OL-20.  
Limitations: Claim/launch/settle not executed; no full Lab lifecycle or analysis report was produced. This does not show the underlying engine is absent.

### F4 — An anatomy walkthrough is feasible, but must use the actual event order

Claim: The recorded fixture supports a concrete permission/effect narrative, but the alternative proposal's proposed `action.effect.decided` step and simple model-then-tool ordering do not describe this trace.

Status: VERIFIED  
Evidence kind: recorded execution  
Evidence: `events` has `action.tool.proposed` at seq 14 before `model.call.completed` at 15; seq 22–27 are effect intended → `security.permission.decided{allow}` → authorized → prepared → committed → observed. A second sequence occurs at 28–33. There are five `action.effect.*` classes, each twice; no `action.effect.decided`. Terminal seq 56 is `succeeded/completed`; 57 events were retained. [Architecture, decisions and durable state](../architecture.md#decisions-and-durable-state) supplies the conceptual explanation.  
Retrieved: 2026-10-29T18:21:05Z; execution above.  
Implication for the spec: Annotate selected event IDs/seqs and their causal links; explain internal `hh.submit`, fixture model, permission decision and terminal verification. A readable excerpt should retain a route to the complete trace.  
Outline delta: CONFIRMS OL-06; CORRECTS OL-03's presumed event vocabulary/order.  
Limitations: Internal scripted effects are not proof of arbitrary filesystem/network tools, approval pauses, containment strength or budget exhaustion.

### F5 — Replay is useful, but whole-stream byte equality is the wrong tutorial assertion

Claim: This fixture replays deterministically without additional model/effect rows, while replay appends lifecycle records and is distinct from a fresh execution.

Status: VERIFIED  
Evidence kind: recorded execution; source inspection  
Evidence: `replay` and before/after event receipts; [replay_ops.rs `replay`, lines 38–170](../../crates/hh-embed/src/replay_ops.rs) reconstructs views or re-drives recorded inputs, checks variant declarations and appends replay markers. The [L4.6b test, lines 418–459](../../crates/hh-embed/tests/l4_6b.rs) checks replay and provider/effect row counts, not byte equality of entire augmented ledgers. [CLI dispatch](../../crates/hh-cli/src/cli.rs) has `run fork` but no direct `run replay` arm; this audit used the protocol.  
Retrieved: 2026-10-29T18:21:05Z; execution above.  
Implication for the spec: Teach replay through an explicit supported entry point and its validity report; compare the contract's decision/assembly sequence and unchanged source prefix, not a blanket diff of two full streams.  
Outline delta: CONFIRMS OL-07; CORRECTS OL-03.  
Limitations: One fixture variant, no live-provider replay rerun, no monetary measurement; “free” ignores setup/storage/compute and cannot imply universal deterministic support.

### F6 — The fixture's R0 bundle round trip exposes a lifecycle-status mismatch

Claim: The newly exported fixture bundle validates at R0 but clean-store reproduction reports lifecycle drift because export and reproduction compare different status vocabularies.

Status: VERIFIED  
Evidence kind: recorded execution; source inspection  
Evidence: `bundle-create`, `bundle-validate`, `bundle-reproduce` receipts. [assemble.rs, lines 174–179](../../crates/hh-bundle/src/assemble.rs) emits `finished/open`; [manifest.rs `SubjectSection`, lines 570–585](../../crates/hh-bundle/src/manifest.rs) documents that vocabulary. [bundle_ops.rs, lines 1566–1590](../../crates/hh-embed/src/bundle_ops.rs) instead takes the terminal event's `status` (`succeeded`) and compares it directly. The separately visible compile refusal is expected for the fixture's unresolved profile: bundle_ops lines 398–415 explicitly allow an R0 export without compiled closure. Neither compile refusal nor an incomplete R1 claim explains away the R0 lifecycle drift.  
Retrieved: 2026-10-29T18:21:05Z; execution above.  
Implication for the spec: Reconcile lifecycle state versus task outcome and add a real export→clean-store reproduction regression for the selected tutorial. Preserve the negative report; document `complete`, supported level, reproduction verdict and independence separately.  
Outline delta: CORRECTS OL-03; EXTENDS OL-07, OL-15, OL-17, OL-20.  
Limitations: No source fix; no assertion that every historical bundle is affected. Exit 0 is a delivered report, not an assurance of reproduction. R1–R3 and fresh reruns were not tested; `independent:false` was returned.

### F7 — Participation guidance exists; a runnable component or replication lane remains to design

Claim: Newcomers already have a bounded bug/docs contribution policy, while the proposed plugin and independent-replication walkthroughs require concrete prerequisites and acceptance steps.

Status: PARTIALLY VERIFIED (policy/package facts inspected; walkthrough usability unverified)  
Evidence kind: source inspection; design proposal  
Evidence: [CONTRIBUTING](../../CONTRIBUTING.md) separates small changes from assigned manifest tickets; [bug form](../../.github/ISSUE_TEMPLATE/bug_report.yml) asks for commands/version/environment, and [PR template](../../.github/pull_request_template.md) asks for scoped verification. [Governance](../../GOVERNANCE.md) retains maintainer merges/ADR authority. [package_variant.rs `materialize`, lines 69–88](../../crates/hh-varhost/tests/package_variant.rs) copies a built variant binary, seals and loads a package: “without recompiling anything” omits actual build prerequisites.  
Retrieved: 2026-10-29T18:21:05Z.  
Implication for the spec: Link a reproducible failure such as F3/F6 to the existing issue route; design a separate build/seal/load/invoke component exercise and replication report template.  
Outline delta: CONFIRMS OL-18, OL-21; EXTENDS OL-13.  
Limitations: No outside contribution, plugin execution or maintainer-response-time evidence. Raw bundles may contain local/private material; a sanitization policy is still F1's work.

## Alternative proposal: disposition of all fourteen suggestions

These are **design proposals (UNVERIFIED)** for C1/C2, grounded in F1–F7,
[brief §4](../brief.md#4-critical-synthesis-of-the-alternative-proposal), and the
source facts below. They do not commit the portfolio or establish effort.

| Original suggestion | Disposition / correction |
|---|---|
| 1. Anatomy of a run | Retain early; use F4's actual sequence and explain a concrete investigative question. |
| 2. Same run again “for free” | Reframe per F5; no whole-stream equality, universal support or zero-total-cost promise. |
| 3. Bundle and reproduce | Retain after F6 repair or a transparently narrower inspection-only demonstration; historical R0 success cannot validate this fresh fixture path. |
| 4. Executable examples | Retain; assert semantic outcomes, failures and schema identities as well as exit codes. [Quickstart checker](../../scripts/check-quickstart.sh) reads committed HEAD; follow that distinction. |
| 5. First experiment | Retain fixture teaching after F3 and E1 seams are resolved; packaged five replicates/10% tolerance are not justified scientific design defaults. |
| 6. Budgets/permissions | Retain bounded trials; the current trace's allow decisions do not establish denial/exhaustion or deployment containment. D3 owns adversarial conditions. |
| 7. Plugin swap without compilation | Reframe as a tested package boundary; F7 shows binary build/seal/load prerequisites. Avoid promising no kernel rebuild until verified for the selected component. |
| 8. Web screenshots | Supporting option; [hh-web startup](../../crates/hh-web/src/main.rs), lines 1–40/114–120, needs a kernel address and token, not simply a store path. Screenshot setup is not zero-cost documentation. |
| 9. Live evidence tour | Retain a claim-to-receipt index; A2 owns current counts/spend and failures. Do not repeat “34 records/$25.26” as an audited current measure here. |
| 10. Foreign replay | Optional bounded conformance branch. [Python guide](../../foreign/e2-python/README.md) explicitly limits eight arms/57 answers and omitted HIR kinds; not a complete Python runtime. Not executed here. |
| 11. README/30-second demo | Retain useful positioning and a short preview; first build duration and outsider comprehension are unmeasured. Reject “Nobody else ships this” as unestablished, with ecosystem comparison owned by B2. |
| 12. Real compaction study as stretch | Revise sequencing: keep central to the first-wave proposal under the approved brief, subject to feasibility and Q-2/Q-3/Q-4; do not silently replace with fixture wins. |
| 13. Approvals | Retain as a scoped operational candidate with a real pause/respond/resume trace; not demonstrated by F4's policy allow. |
| 14. Fleet later | Keep a provisional later disposition with trigger/user job; C1 covers the family instead of silently omitting it. |

The original “weekend” and low-cost characterization is unsupported: F3/F6
require implementation decisions, F1 needs task selection, and F5/F7 require
precise comparison/packaging instructions. Outside adoption value is a hypothesis.

## Proposed compact workflow and remediation priorities

1. **Orient and install:** one primary job, prerequisites and source/release fork; doctor and fixture remain a setup checkpoint.
2. **Do one useful task:** selected input → artifact → independent check; annotate the same run's key decisions and one honest failure. E1/C2 decide task feasibility.
3. **Inspect and explain:** durable run ID, store, summary, context, selected trace; distinguish read success, run outcome, task grade and scientific claim.
4. **Verify/share:** replay contract and bundle inspection, then tested R0 reproduction after F6; separate fresh rerun and redaction instructions.
5. **Compare or contribute:** branch into an offline Lab lesson with synthetic scope or a minimal bug/component exercise; link the separately controlled real study.

Priority P0: resolve selected-path F3/F6 behavior with regression tests or an
explicitly narrower supported route before publishing promises. P1: improve
help and provide the useful-task/annotated-trace/schema-specific editing loop.
P2: expand optional web, foreign-client, approvals and plugin lessons after
coverage/effort selection. These are proposed sequencing priorities, not existing
DF/BL obligations or authorized implementation tickets.

## Answer and completion assessment

A3 is complete for coordinator review: the actual first encounter and follow-on
inspection/replay/bundle path were exercised, a Lab wrapper defect was reproduced,
and all alternative suggestions received a disposition. Missing explanation
(trace narrative, next task, input-type editing loop), interface defects
(help/comparison operands), and behavior defects/gaps (bundle lifecycle mismatch;
selected task/grader runner still E1-owned) are distinguished. No new efficacy,
live, release or usability claim is earned. The note does not authorize fixes.

## Open questions

| Question | Owner / affected units | Operator decision? |
|---|---|---|
| Which useful task and supported CLI versus SDK route should lead? | C1/C2, E1, G1; F3/F6 constrain selection | Q-2, then Q-3/Q-4 for live execution |
| What lifecycle vocabulary should R0 compare, and which selected commands need repair? | E1/E3, F2/F3; propose regression and ADR scope if required | Portfolio/implementation scope, not immediate code authorization |
| Which raw traces/bundles may be redistributed, and what sanitization preserves honest integrity claims? | F1/F3/G2 | Q-5 |
| Do outsiders complete and correctly explain the chosen task without source reading? | B1/G1/G3 | Q-6 before recruitment/collection |

## Spec requirements emitted

All are candidates; reconcile overlaps with REQ-A-002/004/005/006 and REQ-B-002/003/004.

| ID | Candidate requirement / rationale | Findings; outline | Proposed acceptance |
|---|---|---|---|
| REQ-A-201 | Selected onboarding path distinguishes setup from useful task success and names its input schema, IDs, store and independent artifact check. | F1–F2; OL-01, OL-05, OL-17 | Execute chosen task from scratch; inspect artifact/check and one failed outcome without source consultation. |
| REQ-A-202 | Selected commands are discoverable and forward all required boundary fields; tutorial steps never rely on unsupported wrapper shapes. | F2–F3; OL-08, OL-17, OL-20 | Help/recipe review plus real CLI-to-kernel regression failing on baseline comparison/selected experiment mismatch. |
| REQ-A-203 | Annotated trace uses actual classes/order and causal coordinates, with full receipt access and explicit fixture/internal-effect scope. | F4; OL-06, OL-15 | Every annotation resolves to retained events; no invented phase or inferred real-tool/security claim. |
| REQ-A-204 | Replay and sharing instructions assert semantic report verdicts, limits and permitted comparisons; R0 export/reproduce reconciles lifecycle state with terminal outcome. | F5–F6; OL-07, OL-15–OL-17 | Fresh fixture export→clean-store R0 report succeeds after repair; altered status fails; replay markers do not invalidate the intended comparison. |
| REQ-A-205 | Contribution walkthrough separates bug/docs participation, component packaging and scientific replication, using actual prerequisites and privacy handling. | F7; OL-13, OL-18 | Reproduce one bug report and selected package/replication exercise under their distinct acceptance rules. |

## Handoff

A1 dependency is resolved. C1/C2 should budget F3/F6 remediation and preserve the
useful-task plus real-study objectives. F2/G1 can use the recorded trace and
failure cases; E1/E3 own executable integration choices; F1/F3 own shareability
and reproduction guarantees. F6 is a new finding beyond A1, not a contradiction
of its inspection of an older R0 receipt. No central state, source, accepted
note, historical record or gate was changed; no commit or further agent was made.
