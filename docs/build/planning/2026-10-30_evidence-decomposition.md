# Round 5 — evidence and adoption decomposition

**Status:** draft for independent review; authored 2026-10-30T03:09:28Z. Invocation: `decompose-spec spec=docs/evidence-and-adoption-spec.md build_name=evidence-and-adoption mode=extend tickets_dir=docs/tickets tail=full dispatch_target=subagent granularity=balanced sequence_prefix=true ledger=true`.

## Boundary and source identity

[Supplement](../../evidence-and-adoption-spec.md) SHA-256 `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f`; [frozen handoff](../../decomposition-prompt.md). Root authored the contracts after three read-only source maps; the original research ledger stays DONE. Verified local/default/remote main baseline `001490c14a6568972d16a663bb04c07a4255d30d`, completed R4.18; no open PR, zero owed historical deferrals/backlog, stock checker zero violations/eight scoped warnings. [Digest](../reports/digests/2026-10-30.md) is not a CI pass. No code, task/container/provider run, branch, commit, outreach or publication occurred in decomposition.

## Partition and rationale

79 sequential rows, sequences 168–246: {'gate': 11, 'ticket': 43, 'human': 4, 'skeleton': 13, 'capstone': 3, 'reconcile': 3, 'docs': 2}. The eight P packages are split at real owner/decision seams; no one-ticket-per-requirement expansion. [Exact coverage](2026-10-30_evidence-coverage.md) gives 167 primary targets and 13 retained aliases, all 24 OL mappings.

The first gate is a human review plus narrow tooling bootstrap, not an impossible requirement that the guard already exist before its own implementation. R5.1/R5.2 then establish retained checks and actual native routing. Three bounded dossiers prepare seven §9.3 amendment rows; GATE-G9 admits exact semantics before implementation. GATE-G10 is a separate earned narrow offline-method checkpoint, not full study acceptance.

Native execution slices keep launch funding, runtime adoption, effect durability, grade selection, seal/finish, oracle, budget, consumer admission and population distinct. Common interfaces have one earlier owner. Retained representation precedes prepared-call reservation; final wire proof consumes both. Receipt schema, owner references, population assembly and recipient procedures are separate because one broad receipt ticket would touch almost every producer. EX-01/02 complete before authentic-parity numerical/collection gates.

A separately quoted GATE-G18/R5.42a backend preflight earns the previously unproved backend evidence before GATE-G11 can authorize S1/S4. Actual task/observer runs, pilot/parity/confirmation, outside/formative work and publication/time-based follow-up remain skeletons where actual pins/authority are not known. Full fixture engineering contracts can be authored now; no unrecorded global design gap or invented backend is introduced. Method precision/encoding and validation vectors are binding dossier decisions; new inference requires amendment, not an implementer's silent choice.

## Shared decisions and integration owners

| Shared decision / seam | Sole contract owner | Consumers / integration closure |
|---|---|---|
| Seven exact durable/wire/authority amendments | R5.3 coordinates R5.4/R5.5; GATE-G9 admits | All affected owners; REC5.2 audits actual landing |
| Recoverable subject/root binding | R5.6 | R5.7 adoption, R5.14 pending, R5.23 population |
| Single Dispatcher lifecycle/durable reference | R5.8 | R5.10 seal, R5.32–34 operations, R5.43/44 actual proof |
| Pure grade selection / revision proof | R5.9 | R5.11 producer, R5.14 all consumers, R5.15 population, R5.25 recompute |
| Created-origin finish/deadline | R5.10 | R5.19 enforcement; no recipe timer authority |
| Whole-vector reserve / physical-use fold | R5.12/R5.13, distinct operations of same Account owner | R5.14/R5.19/R5.23; never duplicated totals |
| Planned population / method metadata interface | R5.15 under R5.5 admission | R5.16, R5.36–41, receipt and study reports |
| Durable retained view / sealed wire | R5.18 data owner; R5.19 call ordering; R5.20 integration | R5.36 detectors and live admission |
| Native profile / producer-typed closure | R5.21 data owner; R5.22 traversal | R5.23 assembly, R5.24–27 procedures/rights |
| One supported end-to-end recipe and CLI fields | R5.28 | R5.29 offline, R5.43/45 task/trace; no second runner |
| Event joins / task clusters / KL / pair applicability | R5.36 / R5.37 / R5.38 / R5.39 respectively | R5.40 review, R5.41 protocol, fixed live stages |
| One participant cohort and shared observations | R5.53; HUMAN-H9/GATE-G15 authority | Per-predicate records; no mandatory second cohort |
| Full-target/domain closure and residual routing | CAP5.1/CAP5.3 | One primary owner remains in coverage; no engineering-to-research promotion |

## Dependency and size ledger

All listed dependencies point backward. Physical branch order is the immediately preceding **implementation** branch; markers inherit its tip. Semantic dependency edges are not permission for parallel writers or alternate bases. Context estimates are bounded source/spec/implementation working sets plus **10k reserved for verification/review**; at SETUP require total ≤40% of actual worker window or return a split. No estimate grants execution funding.

| Seq | Contract | Kind | Semantic predecessors | Context + reserve / line split trigger |
|---|---|---|---|---|
| 168 | [GATE-G8 — Review partition and authorize the bounded tooling bootstrap](../../tickets/168_GATE-G8__bootstrap.md) | gate | nothing | human record; bounded readout |
| 169 | [R5.1 — Retained supplemental coverage and routing guard](../../tickets/169_R5.1__guard.md) | ticket | GATE-G8 | 26k + 10k / 900 |
| 170 | [R5.2 — Prove the actual guarded native dispatch and resume path](../../tickets/170_R5.2__dispatch.md) | ticket | R5.1 | 24k + 10k / 700 |
| 171 | [R5.3 — Prepare execution, effect and Account amendment dossier](../../tickets/171_R5.3__adr-exec.md) | ticket | R5.2 | 26k + 10k / 600 |
| 172 | [R5.4 — Prepare native profile and producer-reference admission dossier](../../tickets/172_R5.4__adr-receipt.md) | ticket | R5.3 | 22k + 10k / 550 |
| 173 | [R5.5 — Prepare method, encoding and numerical-validation admission dossier](../../tickets/173_R5.5__adr-methods.md) | ticket | R5.4 | 28k + 10k / 650 |
| 174 | [GATE-G9 — Admit exact architecture and method contracts](../../tickets/174_GATE-G9__admit.md) | gate | R5.3, R5.4, R5.5 | human record; bounded readout |
| 175 | [R5.6 — Recoverable launch and subject funding binding](../../tickets/175_R5.6__launch.md) | ticket | GATE-G9 | 22k + 10k / 850 |
| 176 | [R5.7 — Adopt a launched subject through the existing runtime](../../tickets/176_R5.7__adopt.md) | ticket | R5.6 | 22k + 10k / 750 |
| 177 | [R5.8 — Bridge Dispatcher durability to the task runtime](../../tickets/177_R5.8__effect.md) | ticket | R5.7 | 28k + 10k / 1000 |
| 178 | [R5.9 — Shared first-valid grade selection and immutable revisions](../../tickets/178_R5.9__selection.md) | ticket | GATE-G9, R5.8 | 24k + 10k / 800 |
| 179 | [R5.10 — Seal submission and persist actual created-to-finished time](../../tickets/179_R5.10__seal.md) | ticket | R5.8, R5.9 | 24k + 10k / 850 |
| 180 | [R5.11 — Execute and retain isolated instrument oracle attempts](../../tickets/180_R5.11__oracle.md) | ticket | R5.10, R5.9 | 22k + 10k / 750 |
| 181 | [R5.12 — Atomic whole-vector Account cap admission](../../tickets/181_R5.12__reserve.md) | ticket | R5.6 | 18k + 10k / 600 |
| 182 | [R5.13 — Deduplicated incurred-use and unknown-exposure views](../../tickets/182_R5.13__physical.md) | ticket | R5.12, R5.11 | 22k + 10k / 750 |
| 183 | [R5.14 — Required-grade pending across every scored consumer](../../tickets/183_R5.14__pending.md) | ticket | R5.6, R5.7, R5.10, R5.11, R5.9, R5.13 | 28k + 10k / 1000 |
| 184 | [R5.15 — Planned-slot population and historical analysis coordinates](../../tickets/184_R5.15__population.md) | ticket | R5.14 | 28k + 10k / 1000 |
| 185 | [R5.16 — Early routed finite-manifest Bayesian efficacy](../../tickets/185_R5.16__beta.md) | ticket | R5.15 | 26k + 10k / 850 |
| 186 | [R5.17 — Verify the early offline efficacy route](../../tickets/186_R5.17__early-numeric.md) | ticket | R5.16 | 20k + 10k / 600 |
| 187 | [GATE-G10 — Accept the narrow offline analysis evidence](../../tickets/187_GATE-G10__offline-gate.md) | gate | R5.17 | human record; bounded readout |
| 188 | [R5.18 — Persist retained context and replay its selection](../../tickets/188_R5.18__retained.md) | ticket | R5.14 | 22k + 10k / 700 |
| 189 | [R5.19 — Prepared gateway reservations and persistent deadline](../../tickets/189_R5.19__gateway.md) | ticket | R5.18, R5.13, R5.10 | 28k + 10k / 1000 |
| 190 | [R5.20 — Prove retained treatment reaches exact provider bytes](../../tickets/190_R5.20__wire.md) | ticket | R5.18, R5.19 | 22k + 10k / 700 |
| 191 | [R5.21 — Native evidence profile and immutable snapshot coordinates](../../tickets/191_R5.21__profile.md) | ticket | R5.14, R5.15, R5.4 | 22k + 10k / 750 |
| 192 | [R5.22 — Owner-typed reference closure and materialization](../../tickets/192_R5.22__closure.md) | ticket | R5.21, R5.11, R5.19, R5.20 | 28k + 10k / 1000 |
| 193 | [R5.23 — Assemble all-planned evidence from owning projections](../../tickets/193_R5.23__population-receipt.md) | ticket | R5.22, R5.15, R5.13, R5.20 | 28k + 10k / 950 |
| 194 | [R5.24 — Repair R0 lifecycle and prove clean-recipient verifyfixed](../../tickets/194_R5.24__r0.md) | ticket | R5.23 | 22k + 10k / 700 |
| 195 | [R5.25 — Recompute frozen analysis through the native owner](../../tickets/195_R5.25__recompute.md) | ticket | R5.24, R5.16 | 22k + 10k / 700 |
| 196 | [R5.26 — Isolated replay and fresh-execution admission cards](../../tickets/196_R5.26__replay.md) | ticket | R5.24, R5.25, R5.19, R5.20 | 24k + 10k / 750 |
| 197 | [R5.27 — Rights, losses, quotas and recipient handoff validation](../../tickets/197_R5.27__rights.md) | ticket | R5.22 | 22k + 10k / 650 |
| 198 | [R5.28 — One supported native recipe and exact CLI forwarding](../../tickets/198_R5.28__recipe.md) | ticket | R5.14, R5.23, R5.25, R5.26 | 26k + 10k / 850 |
| 199 | [R5.29 — Early offline examples and informative Lab lesson](../../tickets/199_R5.29__offline.md) | ticket | GATE-G10, R5.28, R5.27 | 24k + 10k / 850 |
| 200 | [R5.30 — Example CI modes and bounded refresh workflow](../../tickets/200_R5.30__ci.md) | ticket | R5.29 | 20k + 10k / 650 |
| 201 | [R5.31 — Ordinary contribution and failure-report routes](../../tickets/201_R5.31__contrib.md) | ticket | R5.29, R5.24 | 18k + 10k / 550 |
| 202 | [R5.32 — Confined observer apparatus and Kp4 controls](../../tickets/202_R5.32__observer.md) | ticket | R5.8, R5.19, R5.23 | 26k + 10k / 900 |
| 203 | [R5.33 — Real permission wait, answer and resume matrix](../../tickets/203_R5.33__permission.md) | ticket | R5.32 | 20k + 10k / 600 |
| 204 | [R5.34 — Account refusal, exhaustion and operational reporting machinery](../../tickets/204_R5.34__operations.md) | ticket | R5.33, R5.19, R5.13 | 24k + 10k / 800 |
| 205 | [R5.35 — Reader spine, evidence cards and portfolio positioning](../../tickets/205_R5.35__position.md) | ticket | R5.29, R5.31 | 20k + 10k / 600 |
| 206 | [R5.36 — Mechanism joins and versioned denominator definitions](../../tickets/206_R5.36__metrics.md) | ticket | R5.20, R5.15 | 26k + 10k / 850 |
| 207 | [R5.37 — Task-mixture sensitivity, sharp-null families and tails](../../tickets/207_R5.37__taskstats.md) | ticket | R5.15, R5.16, R5.36 | 28k + 10k / 950 |
| 208 | [R5.38 — Directional KL and correctly labelled bounded equivalence](../../tickets/208_R5.38__kl.md) | ticket | R5.15, R5.37 | 28k + 10k / 1000 |
| 209 | [R5.39 — Authentic paired records and policy applicability](../../tickets/209_R5.39__parity-admission.md) | ticket | R5.38, R5.28, R5.20, R5.36 | 26k + 10k / 850 |
| 210 | [R5.40 — Composed numerical validation and independent method evidence](../../tickets/210_R5.40__numerics.md) | ticket | R5.37, R5.38, R5.39, R5.17 | 25k + 10k / 750 |
| 211 | [R5.41 — Deterministic central protocol materialization](../../tickets/211_R5.41__protocol.md) | ticket | R5.15, R5.39 | 25k + 10k / 800 |
| 212 | [R5.42 — Per-stage readiness and full exposure quote](../../tickets/212_R5.42__quote.md) | ticket | R5.40, R5.41, R5.34, R5.27 | 22k + 10k / 600 |
| 213 | [HUMAN-H8 — Bind actual backend, inputs, rights and stage resources](../../tickets/213_HUMAN-H8__inputs.md) | human | R5.42 | human record; bounded readout |
| 214 | [GATE-G18 — Authorize narrowly quoted backend feasibility preflight](../../tickets/214_GATE-G18__backend-gate.md) | gate | HUMAN-H8, R5.42, R5.32 | human record; bounded readout |
| 215 | [R5.42a — Earn bounded native backend feasibility evidence](../../tickets/215_R5.42a__backend-preflight.md) | skeleton | GATE-G18 | 22k + 10k / 650 |
| 216 | [GATE-G11 — Authorize exact S1 and S4 executions](../../tickets/216_GATE-G11__execute-gate.md) | gate | HUMAN-H8, R5.42, R5.42a | human record; bounded readout |
| 217 | [R5.43 — Execute two fixed useful-task subjects](../../tickets/217_R5.43__useful.md) | skeleton | GATE-G11, R5.28 | 24k + 10k / 750 |
| 218 | [R5.44 — Execute the confined operational proof matrix](../../tickets/218_R5.44__ops-live.md) | skeleton | GATE-G11, R5.34, R5.43 | 24k + 10k / 750 |
| 219 | [R5.45 — Actual task trace, bounded edit and example integration](../../tickets/219_R5.45__trace.md) | ticket | R5.43, R5.44, R5.35, R5.30 | 26k + 10k / 850 |
| 220 | [GATE-G12 — Authorize activation then fixed pilot](../../tickets/220_GATE-G12__activation-gate.md) | gate | R5.45, R5.42, HUMAN-H8, R5.40 | human record; bounded readout |
| 221 | [R5.46 — Collect bounded activation and pilot evidence](../../tickets/221_R5.46__pilot.md) | skeleton | GATE-G12, R5.41 | 24k + 10k / 750 |
| 222 | [GATE-G13 — Freeze and fund authentic original-runner parity](../../tickets/222_GATE-G13__parity-gate.md) | gate | R5.46, R5.42, R5.39, HUMAN-H8 | human record; bounded readout |
| 223 | [R5.47 — Collect one authentic fixed-N parity tranche](../../tickets/223_R5.47__parity-live.md) | skeleton | GATE-G13 | 24k + 10k / 750 |
| 224 | [GATE-G14 — Preregister and authorize held-out confirmation](../../tickets/224_GATE-G14__confirm-gate.md) | gate | R5.47, R5.42, R5.40, R5.41 | human record; bounded readout |
| 225 | [R5.48 — Execute the frozen held-out confirmation](../../tickets/225_R5.48__confirm.md) | skeleton | GATE-G14 | 24k + 10k / 750 |
| 226 | [R5.49 — Report and recompute admitted study results](../../tickets/226_R5.49__study-report.md) | skeleton | R5.48 | 24k + 10k / 750 |
| 227 | [HUMAN-H9 — Authorize recipient, optional exercises and formative participation](../../tickets/227_HUMAN-H9__participants.md) | human | R5.49, R5.45 | human record; bounded readout |
| 228 | [GATE-G15 — Admit cleared recipient and participant procedures](../../tickets/228_GATE-G15__participation-gate.md) | gate | HUMAN-H9, R5.27, R5.45 | human record; bounded readout |
| 229 | [R5.50 — Selected existing component walkthrough and controls](../../tickets/229_R5.50__plugin.md) | skeleton | GATE-G15 | 24k + 10k / 750 |
| 230 | [R5.51 — Selected native-to-external consumer exercise](../../tickets/230_R5.51__interchange.md) | skeleton | GATE-G15, R5.50 | 24k + 10k / 750 |
| 231 | [R5.52 — Outside clean-recipient verification and recomputation](../../tickets/231_R5.52__outside.md) | skeleton | GATE-G15, R5.25, R5.26, R5.27 | 24k + 10k / 750 |
| 232 | [R5.53 — Consented evaluator and contributor observations](../../tickets/232_R5.53__formative.md) | skeleton | R5.52, R5.51, R5.45, R5.31 | 24k + 10k / 750 |
| 233 | [GATE-G16 — Disposition capability and scientific publication scope](../../tickets/233_GATE-G16__publication-gate.md) | gate | R5.53, R5.49, R5.52 | human record; bounded readout |
| 234 | [HUMAN-H10 — Perform explicitly approved publication and outreach](../../tickets/234_HUMAN-H10__publish.md) | human | GATE-G16 | human record; bounded readout |
| 235 | [R5.54 — Record public availability, claims and corrections](../../tickets/235_R5.54__publication-record.md) | skeleton | HUMAN-H10 | 24k + 10k / 750 |
| 236 | [HUMAN-H11 — Perform bounded manual day-30 and day-90 review](../../tickets/236_HUMAN-H11__value-dates.md) | human | R5.54 | human record; bounded readout |
| 237 | [R5.55 — Record bounded adoption and contribution follow-up](../../tickets/237_R5.55__value.md) | skeleton | HUMAN-H11 | 24k + 10k / 750 |
| 238 | [CAP5.1 — Independent whole-scope gap hunt](../../tickets/238_CAP5.1__cap1.md) | capstone | R5.55 | 28k + 10k / 650 |
| 239 | [CAP5.2 — Composed execution, methods and recipient verification](../../tickets/239_CAP5.2__cap2.md) | capstone | CAP5.1 | 28k + 10k / 700 |
| 240 | [CAP5.3 — Close layers and prepare fresh acceptance](../../tickets/240_CAP5.3__cap3.md) | capstone | CAP5.1, CAP5.2 | 24k + 10k / 600 |
| 241 | [GATE-G17 — Fresh round-5 acceptance (GATE-ACCEPT role)](../../tickets/241_GATE-G17__accept.md) | gate | CAP5.3 | human record; bounded readout |
| 242 | [REC5.1 — Reconcile backlog and current readiness](../../tickets/242_REC5.1__rec1.md) | reconcile | GATE-G17 | 22k + 10k / 500 |
| 243 | [REC5.2 — Reconcile landed code, ADRs and supplemental coverage](../../tickets/243_REC5.2__rec2.md) | reconcile | REC5.1 | 24k + 10k / 550 |
| 244 | [REC5.3 — Prepare integration recommendation from actual branch and CI state](../../tickets/244_REC5.3__rec3.md) | reconcile | REC5.2 | 20k + 10k / 450 |
| 245 | [DOC5.1 — Refresh reader documentation from accepted evidence](../../tickets/245_DOC5.1__doc1.md) | docs | REC5.3 | 20k + 10k / 650 |
| 246 | [DOC5.2 — Refresh agent ownership and verification guidance](../../tickets/246_DOC5.2__doc2.md) | docs | DOC5.1 | 18k + 10k / 500 |

## Gate map

| Marker | Required decision/inputs | Pending behavior |
|---|---|---|
| [GATE-G8](../../tickets/168_GATE-G8__bootstrap.md) | Review the partition, exact coverage, independent dispositions and readiness packet in Zed. A fresh scoped decision may authorize ONLY the two tooling contracts and admission-dossier preparation; no product semantics, collection, spending or publication. Reconfirm base, source digests and stock baseline. Guard absence is the named bootstrap exception, never a product exception. | RETURN PASS; no authority inferred |
| [GATE-G9](../../tickets/174_GATE-G9__admit.md) | Operator/authorized maintainer dispositions all seven §9.3 rows and exact proposed ADRs, including encoding, correction authority, compatibility and numerical protocol. Guard and native launch proofs must pass. Record allowed implementation scope, separate test resources and expiry/void conditions. This admits design and implementation only; numerical validation remains evidence to be earned and study funding stays $0. | RETURN PASS; no authority inferred |
| [GATE-G10](../../tickets/187_GATE-G10__offline-gate.md) | Accept only the admitted, tested EX-02 method/metadata/round-trip path and its fixture label. Require exact selected numerical protocol, independent routed evidence and the normal example resource envelope. No live task, parity, confirmation or public scientific claim is authorized. | RETURN PASS; no authority inferred |
| [HUMAN-H8](../../tickets/213_HUMAN-H8__inputs.md) | Study/operator and environment/artifact owners supply unchanged upstream task/oracle/runner/image/model/roles, backend confinement/observer/probe feasibility, original-runner access, exact rights and full resource/retention quotes. Give separate S1/S4, activation, pilot, parity and confirmation choices; credentials provided yes/no only. No task/container/provider execution follows from merely supplying inputs. | RETURN PASS; no authority inferred |
| [GATE-G18](../../tickets/214_GATE-G18__backend-gate.md) | Approve only an exact bounded backend feasibility preflight with unchanged upstream lifecycle, supported HelperExecutor/Podman candidate, real mediation/surviving observer/probe checks, acquisition/network policy and complete numeric resources/rights/retention quote. Unset fields mean zero dispatch. No S1/S4 trial, study subject, policy search, substitute backend or new dependency is admitted; result is reviewed at GATE-G11. | RETURN PASS; no authority inferred |
| [GATE-G11](../../tickets/216_GATE-G11__execute-gate.md) | Require exact unchanged task/oracle and supported HelperExecutor/Podman boundary, confinement/observer/probe proof, all native cap/wire/grade/receipt preconditions and separate §5 quotes. Bind two S1 subjects and 8×3 S4 cases, ≤1 diagnostic setup retry per cell and stops. Missing backend requires explicit scope decision, never substitute backend. | RETURN PASS; no authority inferred |
| [GATE-G12](../../tickets/220_GATE-G12__activation-gate.md) | Record separate one-task activation quote and, only after its result, explicit fixed pilot authorization ≤60 first subjects under §9.2 per-subject and tranche ceilings. Freeze tasks/splits/policies/caps/oracle/detectors/analysis/stops before data; no implied permission for next stage. One gate can have separate numbered readings, neither pre-signed. | RETURN PASS; no authority inferred |
| [GATE-G13](../../tickets/222_GATE-G13__parity-gate.md) | Require independently admitted exact method/routing, actual endpoints/ranges/margins/applicability/independence and both-policy scope; quote both runners and fix final N before dispatch. No tranche is preallocated. Infeasible precision stops/requotes with S3 unmet; no favorable top-up or CP fallback. | RETURN PASS; no authority inferred |
| [GATE-G14](../../tickets/224_GATE-G14__confirm-gate.md) | Accept full actual parity/treatment/caps/clock/oracle/detectors/methods and exact 30-task held-out manifest, family/margins/stops plus complete numeric quote. ≤300 first subjects, same individual caps, ≤900 core-hours/200GiB/14days; unapproved larger design cannot launch. No automatic transfer/default/publication claim. | RETURN PASS; no authority inferred |
| [HUMAN-H9](../../tickets/227_HUMAN-H9__participants.md) | Maintainer/operator names one outside reviewer, 4–6 formative participants/branches and optional component/interchange selection within existing ceilings; supplies consent, scope/date/assistance/data deletion plan, cleared artifact access and numeric resources before any outreach. State actual external consumer/version/format/rights if selected. No mandatory second cohort; no telemetry. | RETURN PASS; no authority inferred |
| [GATE-G15](../../tickets/228_GATE-G15__participation-gate.md) | Confirm ready accessible artifacts and exact rights/consent/retention/effort/resource decisions. Outside reviewer must execute verify and recompute after producer cleanup. Other procedure cards can be evaluated by comprehension/branch observations as their predicates require; no extra outside fresh/replay quota. Optional plugin ≤one evict_oldest, interchange ≤one named format/reader. | RETURN PASS; no authority inferred |
| [GATE-G16](../../tickets/233_GATE-G16__publication-gate.md) | Review exact ready claim/version/evidence/loss/correction spine, outside results, formative limits and channel rules. Capability release and scientific report have distinct acceptance. Retain null/adverse results and unmet obligations; authorize specific public acts/contacts/content only, with exact expiry/void conditions. An early useful release needs explicit chain/scope amendment, not silent skip of S3. | RETURN PASS; no authority inferred |
| [HUMAN-H10](../../tickets/234_HUMAN-H10__publish.md) | Operator/maintainer executes only named channel/content/release/outreach acts approved at GATE-G16; verifies URLs/access/version and actual timestamps, retains corrections/contact data under consent. No model-inferred sharing permission; public visibility alone never upgrades capability evidence. | RETURN PASS; no authority inferred |
| [HUMAN-H11](../../tickets/236_HUMAN-H11__value-dates.md) | Maintainer manually reviews at actual publication+30/+90 dates, within total 24h effort; no automation. Record actual task/contribution funnel denominators, assistance/missingness and scope decisions, then delete contacts AND raw observations 30 days after day90, or earlier withdrawal where possible; receipt-retention rights remain separate. Calendar not yet set: dates derive from actual publication, not planning. | RETURN PASS; no authority inferred |
| [GATE-G17](../../tickets/241_GATE-G17__accept.md) | Read CAP5.1/2/3 and every carried item, exact scope/digests/expiry/void conditions. Operator words and UTC/channel/items plus agent-draft hash confirmation must be new; old signature cannot pass. Unmet objective requires explicit owned scope disposition/ADR, not inferred completion. No merge/publication permission beyond explicit words. | RETURN PASS; no authority inferred |

## Acceptance dependencies and conscious tradeoffs

- The retained guard is deliberately a separately authorized bootstrap, followed by actual native stub-worker proof. This plan and its temporary checks are not dispatch enforcement. No unguarded product loop is ready.
- Primary ownership is unique but full-predicate completion can follow later evidence. The coverage table names contributing tickets; earlier workers must create/update owed DF rows at closure. Forward evidence references never become forward dispatch dependencies.
- Full fixture contracts are conditional on unchanged admitted semantics. Exact wire identifiers, correction authority and numerical representation are frozen by admission dossiers; substantive changes return for scoped decomposition. Gated actual-execution skeletons have no Run line.
- The default chain retains the complete scientific and adoption program; it can span the day-90 observation window. It is not one uninterrupted unattended run. Earlier capability publication is possible only through an explicit scoped plan/gate amendment that retains S3 and the later obligations, never by dropping them. Human waits do not count as failed experiments.
- Optional component/interchange are bounded conditional skeletons. A documented non-selection with withheld claims preserves their source disposition; selecting them requires the actual named package/consumer controls and appropriate participant proof.
- Overflow splits use the weak seam: population contract vs consumer wiring; environment reference enumerators after common closure; prepared-call path vs shared-origin deadline; numerical math vs already-fixed report routing. Do not split a shared decision into conflicting owners. These limits are reviewed, not promises of elapsed implementation time.

## Full tail equivalence and read decisions

| Required role | New ID | Read decision |
|---|---|---|
| CAP.1 | CAP5.1 | Independent full-scope verdict before per-ticket run narratives |
| CAP.2 | CAP5.2 | Composed native and actual recipient evidence; no unauthorized extra run |
| CAP.3 | CAP5.3 | Close gaps and recompute distinct engineering/satisfaction/domain totals |
| GATE-ACCEPT | GATE-G17 | Fresh operator scope/signature; old approval rejected |
| REC.1 | REC5.1 | Read current artifact access/claimed service state because access expires |
| REC.2 | REC5.2 | Read actual landed code/ADRs/coverage; only ticked amendments apply |
| REC.3 | REC5.3 | Read current stack CI/branches/release prerequisites; recommend, never merge |
| DOC.1 | DOC5.1 | Reader commands/receipts/limits from accepted evidence |
| DOC.2 | DOC5.2 | Agent owner/check/guard guidance from landed implementation |

New tail outputs use `docs/build/reports/round5/`; old same-named reports remain historical. Reconciliation invokes the external skill in the named mode and updates existing registers only through their append/status protocol. Live-state reads confer no production authority.

## Source review evidence

Read-only bounded-source maps: [execution](2026-10-30_evidence-execution-partition-input.md), [methods](2026-10-30_evidence-methods-partition-input.md), [adoption](2026-10-30_evidence-adoption-partition-input.md). These are analysis inputs, not new product contracts or executed tests. Reconfirm symbols on the actual implementation base.

## Independent partition review

Initial independent reviews found scoped ownership/order gaps; repairs add bounded backend preflight, mandatory client evidence and exact later-predicate contributors. Focused rereview is pending; no implementation readiness asserted.

## Review disposition — 2026-10-30T03:15:54Z

The [partition rereview](2026-10-30_evidence-partition-rereview.md) closes IP-01–04/IP-N1; the [coverage rereview](2026-10-30_evidence-coverage-rereview.md) closes COV-01–04. Both are clean at planning scope after the recorded repairs. No runtime/guard/participant proof or operator acceptance is inferred. The exact reviewed plan bytes before this status-only addendum have SHA-256 `8ade4d28950d976c51c0781b3e259aa7f74251174ed852c0006deb478d4e5664`; the coverage rereview records the contract/coverage aggregate. The 79 contracts, exact coverage and invariants are unchanged by this addendum.

[Review guide](2026-10-30_evidence-review.md), [readiness](2026-10-30_evidence-readiness.md) and [validation](2026-10-30_evidence-validation.md) are the current handoff. Initial draft/pending language above is the reviewed history, superseded only in status by this dated disposition. GATE-G8 remains pending.
