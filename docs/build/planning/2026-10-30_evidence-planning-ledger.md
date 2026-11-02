# Evidence/adoption decomposition working ledger

**Scope:** Authorized by the operator's current-chat message “Ok. Now run the decompose-spec step and continue”, recorded 2026-10-30T02:25:27Z. Follow the frozen [handoff](../../decomposition-prompt.md). This is the ticket/build-artifact authoring phase; runtime/tooling implementation and orchestration do not start here. Root is sole repository writer; three read-only analysis agents return scratch memos.

## CURRENT STATE

```text
projectStatus: DONE
nextUnit: DONE
lastCompleted: P5
blockedOn: (none)
pauseRequested: false
round: 5
updatedAt: 2026-10-30T03:17:48Z
```

## Work units

| id | item | owner | status | evidence |
|---|---|---|---|---|
| P1 | Confirm frozen inputs, build/remote/CI baseline and workflow contracts | A | done | [Baseline digest](../reports/digests/2026-10-30.md); exact supplemental/handoff hashes verified; zero owed deferrals/backlog |
| P2 | Read spec/code seams and assemble dependency/coupling, ownership and sizing map | A | done | Three bounded source maps; exact unique ownership, shared seams and context estimates |
| P3 | Author ticket/gate/human/full-tail contracts, exact coverage and compatible manifest/ledger extension | S | done | 79 indexed rows; 180 candidates/167 primary targets; guarded bootstrap and gated preflight |
| P4 | Fresh-context adversarial partition review, repairs and targeted rereview | S | done | Both independent focused rereviews clean; all IP/COV findings closed at planning scope |
| P5 | Validate artifacts, document remaining launch prerequisites and open results in Zed | S | done | Checks green within documented scope; review guide/partition/readiness opened in Zed (exit 0) |

## Findings and boundaries

- Source/head/default main all resolve to `001490c14a6568972d16a663bb04c07a4255d30d`; `git ls-remote --symref origin HEAD` confirms main. Explicit ancestry checks `origin/main`→HEAD/main exit 0. The generated digest's ancestry field is unknown because the historical chainTip value contains prose; it is not evidence of a divergent chain.
- GitHub open PR list is empty; CI boundary reports not-applicable (no open chain PR), not a passing CI run. Fresh full build-memory validator: zero violations/eight scoped historical warnings, matching H1. No remote writes or CI dispatch performed.
- Existing committed layout already exists; idempotent init preserves every existing file. The next round can start after sequence 167b, provided these facts remain current at handoff/SETUP. No numbers allocated before the partition.
- Preserve frozen specification `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f` and handoff `bbfb72b22c1dee80924ec40562605c9d57238b1ea4a61c6dc6a53692143725ca`; no upstream ledger restart or spec rewrite.
- A retained coverage/routing guard plus actual native dispatch integration is separately authorized implementation work. Its contract belongs here, but no readiness claim may substitute document checks for executable proof. Pending future execution/design gates yield honest skeletons where exact parameters are not derivable.

## Change log

- 2026-10-30T02:25:27Z — P1 baseline complete. Assigned read-only source/contract maps to /root/decompose_execution, /root/decompose_methods and /root/decompose_adoption at Extra High, each with isolated /tmp memo output and no repository writes. No new model/runtime/service permissions are inherited.
- 2026-10-30T02:25:27Z — State anchor: lastCompleted=P1; nextUnit=P2.

- 2026-10-30T03:10:27Z — P2 done: source/DAG/ownership/size synthesis. P3 done: 79 contracts/markers, exact source predicate trace and append-only Round-5 manifest/state extension. Added bounded backend preflight after independent review exposed the proof/permission ordering gap.
- 2026-10-30T03:10:27Z — State anchor: lastCompleted=P3; nextUnit=P4. No implementation or execution started.

- 2026-10-30T03:15:54Z — P4 done: independent partition and exact-predicate/layer rereviews clean after repairs; source/contract identities preserved. State anchor: lastCompleted=P4; nextUnit=P5.

- 2026-10-30T03:17:48Z — P5 done: external tree/history/backlog and repo documentation/row/catalogue checks pass; temporary exact trace/history/link audit has zero diagnostics, both independent rereviews clean. Opened review guide/partition/readiness in Zed, exit 0. No product tests or execution; retained guard/dispatch proof is future work. State anchor: lastCompleted=P5; nextUnit=DONE. Build ledger remains NOT_STARTED/GATE-G8 for user review.
