# Independent Round 5 partition review

Verdict: **changes-required**, scoped P2 repairs. No established P0/P1 defect and no wholesale repartition recommended.

Reviewed frozen supplement SHA-256 `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f`, the decomposition/invariants, all 77 new contract scopes and unique ACs, the exact 180-candidate coverage companions, and bounded native source anchors. The completed manifest extension was available before conclusion. Root remains sole repository writer. This review ran no implementation, task/container/provider or code-test work and changed no repository file. Read-only structural inspection found 77 unique new contracts, 180 unique candidate rows, 167 canonical targets, and no forward declared contract dependency. This is review evidence, not the future retained executable guard.

## Required repairs

### IP-01 — P2: exact live quote precedes the inputs that make it possible

Paths: `docs/tickets/212_R5.42__quote.md:35`; `docs/tickets/213_HUMAN-H8__inputs.md:17`; decomposition rows 212–214.

R5.42 AC-1 requires an exact-input packet and quotes for every independent stage. Its successor HUMAN-H8 supplies the actual task/oracle/runner/image/model, backend feasibility, rights and resource/retention inputs. Thus the contract is not independently closable as written without either guessing future binding choices or returning for inputs scheduled after it. AC-2 honestly refuses unset fields, but refusal does not satisfy AC-1's positive exact-packet requirement.

Minimal repair: make R5.42 explicitly own reusable quote/readiness construction, validation and informative fixture controls. Actual stage packets remain unresolved until HUMAN-H8 and each applicable stage gate bind them; those gates own completed numeric quotes. Preserve the primary requirement's later actual-evidence contributors and unmet R status. No extra ticket is needed.

Tradeoff: engineering may land while every execution-stage quote is still unavailable; the coverage record must retain that difference.

### IP-02 — P2: mandatory selected-client evidence is controlled by optional component selection

Paths: `docs/tickets/227_R5.50__plugin.md:15`; coverage MD REQ-A-106 row (line 30 at review); coverage JSON REQ-A-106; `docs/tickets/198_R5.28__recipe.md`.

REQ-A-106 is assigned solely to optional R5.50, with no contributor. R5.50 does mention client versions/callbacks/notifications, but the entire scope is under “If explicitly selected” for the evict_oldest package. The supplement §1.4 independently retains selected-client request and notification/callback failures, and R5.28 selects the supported client/recipe. Declining the optional package cannot remove the evidence obligation on the mandatory selected client path.

Minimal repair: give R5.28 the required selected-client slice/primary ownership (or make it an explicit required contributor to a nonconditional A-106 owner), including exact applicability and typed inapplicability for channels the chosen path lacks. R5.50 contributes only selected plugin phases/provenance. Keep optional corpus replay scoped and withhold unsupported compatibility claims. Update both coverage forms and contract ID lists consistently.

Tradeoff: one shared requirement can consume required client evidence plus conditional package/corpus evidence; unique primary ownership remains unchanged in principle.

### IP-03 — P2: newly earned backend proof lacks a pre-execution admission path

Paths: `docs/tickets/202_R5.32__observer.md:38`; `docs/tickets/213_HUMAN-H8__inputs.md:17`; `docs/tickets/214_GATE-G11__execute-gate.md:18`; supplement §§2.1, 9.2.

GATE-G11 demands supported HelperExecutor/Podman confinement/observer/probe proof before admitting S1/S4. R5.32 is fixture-testable and explicitly leaves actual backend trials gated; HUMAN-H8 grants no task/container/provider execution merely by supplying inputs. The source explicitly calls backend suitability unproved. If admissible proof must be newly earned, the plan currently supplies no named bounded preflight owner/authorization before the gate demands the proof. Human provision of an already existing exact proof would avoid a cycle, but no such evidence is asserted.

Minimal repair: state how this prerequisite is earned. A narrow first G11 reading may authorize only a separately quoted backend/preflight validation, followed by proof review and a second S1/S4 authorization; alternatively name an admissible preexisting proof with exact limits, or explicitly admit a named bounded preflight in the preceding engineering-test scope. No S1 subject/24-trial matrix may be launched on the preflight authorization. Preserve zero dispatch and scope review on unsupported backend.

Tradeoff: an additional reading/pause may be necessary; it does not require another product backend or expanded task study.

### IP-04 — P2: profile ownership omits known recipient acceptance contributors

Paths: exact coverage REQ-A-103 and REQ-A-101 rows; `docs/tickets/191_R5.21__profile.md`; `docs/tickets/192_R5.22__closure.md`; `docs/tickets/193_R5.23__population-receipt.md`; recipient contracts R5.24–26/R5.52.

REQ-A-103's literal acceptance requires a clean recipient to verify the selected artifacts and complete every advertised procedure. It is primary-owned by profile/identity ticket R5.21 with no contributors; R5.21 AC-1/2 cannot establish closure/procedure execution. A-101 likewise includes failures/skips remaining queryable, which requires the assembly/projection slice, beyond profile identity creation. The generic universal “later owed” clause prevents false completion if obeyed, but the supposedly exact initial mapping does not identify these already-known dependent owners.

Minimal repair: add the necessary closure/assembly/recipient contributors to these rows and R5.21's AC-3 evidence dependency list, with their corresponding contributing-ID declarations. Keep R5.21 a schema/identity concern; full requirements remain unmet until all actual procedure predicates land. Audit similar multi-predicate rows during the consolidated coverage repair. No new ticket is needed.

Tradeoff: explicit contributors add links, not duplicate primary ownership or a forward dispatch dependency.

## Editorial repair, not a partition blocker

IP-N1 — R5.5 AC-1 says “five method/detector §9.3 rows.” The seven-row register allocates two to R5.3 and one to R5.4, leaving four. Correct the count; no scope expansion or redesign follows.

## Conscious tradeoffs / areas not found blocking

- Declared DAG and branch-order separation are sound. Semantic contributor references may point forward as evidence dependencies without becoming alternate branch bases.
- G8's narrow guard bootstrap avoids demanding the guard before its own implementation. R5.2 correctly requires the actual native route/stub proof and refuses a documentation-only result. A route that cannot enforce it must return the scoped integration decision already required by that contract.
- G9 design admission, G10 narrow earned offline-method admission, and later full numerical/collection admission are distinct. EX-02 does not depend on authentic live parity or confirmation.
- R5.16/17 versus R5.37–40 is justified by the required early exact teaching path. Retained view, prepared-call reservation and provider-byte proof have an explicit earlier representation owner. I do not recommend merging these solely to reduce the row count.
- R5.14, R5.19, R5.22 and CAP5.1 have the highest context risk: their native owners include large driver/dispatcher/runtime files, and CAP5.1 spans all requirements. The bounded-symbol-load and ≤40%-of-actual-window SETUP check/split rule are necessary, not optional. The estimates are not measured implementation working sets. This is a documented execution constraint rather than a proven overflow on the unknown future worker window.
- G12's activation-before-pilot two-reading design can work if the future fleshed contract and guard bind the exact authorized stage; the first reading must never admit the pilot automatically. This is an explicit staged-gate tradeoff, not a reason to preauthorize both stages.
- Optional package/interchange nonselection and the day-90 tail are consciously retained. Nonselection must be an explicit disposition with withheld claims and routing advancement; skeletons may never be dispatched as-is. Earlier capability release requires the explicit plan amendment already recorded.
- Late metrics/protocol/operational producers rely on the earlier profile/reference interfaces; R5.45 and CAP5.2 provide composed closure. Keep owner enumerators updated with new admitted payload references rather than creating a bundle-owned schema table.

## Scope limits

No runtime feasibility, backend availability, quote affordability, numerical coverage, real statistical independence, rights clearance, outside participation or future native dispatch enforcement has been established. No old gate/signature or stock checker result substitutes for those proofs. This review does not treat missing gate inputs or absent future execution as defects by themselves; the findings concern their ownership/order and preservation in the plan.

Root has already acknowledged repairs for IP-01, IP-02 and IP-N1 and will consolidate changes. Focused rereview should check the amended exact paths/coverage, IP-03's earned-proof route and IP-04's contributor links; the frozen supplement must remain byte-identical.
