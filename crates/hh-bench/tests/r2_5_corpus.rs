//! R2.5 / DF-S1.19-2 — the §5c §9 corpus legs executed over the committed
//! `benchset.stage3.v1` recorded-`model_io` corpus, honestly labelled.
//!
//! Each leg iterates the corpus members (suite × task) and runs the real
//! `hh-context` machinery — never a test-double memory plane:
//!
//! - **labelled corpora** (`AC-R-2.4.3-*`): every member is written as a
//!   `session`-scoped memory row carrying its own authority label; the
//!   same labelled store is then read under two `slot_min_authority`
//!   floors and the delivered/withheld split follows the label — the
//!   corpus's label partition is load-bearing, asserted per member.
//! - **`resume_set` runs** (`AC-R-2.4.3-11`): every member's head is bound
//!   `resume:<task>` at session scope and resolved through the same
//!   `by_name` path `KernelMemory::resume_set_read` uses — a member whose
//!   head doesn't resolve `Live` fails the battery.
//! - **`environment_epoch` providers** (`AC-R-2.4.4-*`): every member
//!   carries a `DependencyStamp{kind: environment_epoch}` into the suite's
//!   epoch ref; the provider (`MemoryStore::set_stamp`) republishing the
//!   epoch expires every member (`dependency_changed`) — the environment
//!   mutation is load-bearing, asserted per member.
//! - **judged-conflict fixtures** (`AC-R-2.4.4-4` + DF-S2.8-1(e)): every
//!   member writes a conflicting pair under one `subject_key`; the minted
//!   `ConflictSet` carries `detector: "deterministic"`. The `judged`
//!   detector member is staged C2 — the battery emits a typed
//!   `n/a{judged_conflict_detector_staged_c2}` row per member (never a
//!   fabricated judged resolution).
//!
//! Plus the DF-S2.8-1(e) detector legs' corpus coverage: `benchset.
//! stage3.v1` carries no recorded judge verdicts and no recorded human
//! artefact marks, so the `judged`/`human` `activated`/`followed` legs
//! cannot be corpus-driven hermetically — each member emits a typed `n/a`
//! row; the legs themselves execute at driver level in
//! `crates/hh-control/tests/r2_5.rs` (`judged_verify_mints_*`,
//! `human_artefact_mark_mints_activated`).

use hh_bench::benchset::Benchset;
use hh_context::events::CollectSink;
use hh_context::lifecycle;
use hh_context::memory::{
    DependencyStamp, InvalidationContract, MemoryDraft, MemoryStore, WriteContext,
};
use hh_context::plan::ValidityPolicy;
use hh_context::retrieve::{self, RetrievalBudget, RetrievalRequest, SlotConstraints};
use hh_context::vocab::{
    CacheHint, ConflictResolution, DependencyKind, Granularity, Layer, LifecycleState,
    LifecycleStateKind, MemoryContent, MemoryKind, RetrievalQuery, Revalidation, SubjectKey,
};
use hh_context::vocab::{ConflictPolicy, InvalidationCondition};
use hh_identity::names::ResolveMode;
use hh_provenance::authority::{AuthorityClass, PersistenceScope};
use hh_provenance::origin::Origin;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::json::Json;

/// Per-suite provenance: strata a/b read as `delegate` (model-derived
/// rows), the rest `external` (tool-recorded) — a member's label follows
/// its recorded provenance, so the authority floor partitions the corpus
/// without a second spelling of "label".
fn member_prov(suite_key: &str, task: &str) -> ProvenanceRecord {
    let origin = if matches!(suite_key, "a" | "b") {
        Origin::model("corpus-model", "corpus", task)
    } else {
        Origin::tool("cap:corpus", task)
    };
    ProvenanceRecord::minted(origin, PersistenceScope::Session, 0)
}

fn wctx(store: &MemoryStore, scope: PersistenceScope, seq: u64) -> WriteContext {
    WriteContext {
        context_label: hh_provenance::label::Label::top(),
        lease_generation: store.lease(scope),
        at_seq: seq,
        run_id: "corpus".into(),
    }
}

/// A session-scoped contract (C-CONTRACT-1's floor: `scope_ended` on the
/// write's own scope — the same contract `KernelContext::put_artifact`
/// declares for session writes).
fn session_contract() -> Option<InvalidationContract> {
    Some(InvalidationContract {
        dependencies: Vec::new(),
        cache_hint: CacheHint::Cacheable,
        validator_ref: None,
        freshness: None,
        invalidation_condition: Some(InvalidationCondition::ScopeEnded(PersistenceScope::Session)),
        revalidation: Revalidation::Never,
    })
}

fn task_draft(
    suite_key: &str,
    name: &str,
    body: &str,
    subject: Option<SubjectKey>,
    contract: Option<InvalidationContract>,
) -> MemoryDraft {
    MemoryDraft {
        kind: MemoryKind::Fact,
        subject_key: subject,
        content: MemoryContent::Structured(Json::obj([
            ("task", Json::str(name)),
            ("body", Json::str(body)),
        ])),
        contract,
        scope: PersistenceScope::Session,
        declared_inputs: vec![],
        justifications: vec![],
        supersedes: None,
        validity: None,
        provenance: Some(member_prov(suite_key, name)),
        semantic_id: None,
        validator_endorsed: false,
    }
}

/// One retrieval request over the store — the same request shape
/// `KernelMemory::run_retrieve` mints (the corpus leg reads the same
/// pipeline the driver's memory port does).
fn corpus_retrieve(
    store: &mut MemoryStore,
    query: RetrievalQuery,
    min_authority: AuthorityClass,
) -> (usize, usize) {
    let req = RetrievalRequest {
        model_call_id: "mc:corpus".into(),
        at: ("corpus".into(), store.applied_seq()),
        layers: [
            Layer::Artifact,
            Layer::Episodic,
            Layer::Procedural,
            Layer::Session,
        ]
        .into_iter()
        .collect(),
        query,
        constraints: SlotConstraints {
            slot_min_authority: min_authority,
            validity_policy: ValidityPolicy {
                admitted_states: [
                    LifecycleStateKind::Valid,
                    LifecycleStateKind::StaleByDependency,
                    LifecycleStateKind::Unknown,
                ]
                .into_iter()
                .collect(),
                conflict_policy: ConflictPolicy::DeliverAllAnnotated,
                max_stale: None,
            },
            readers_required: None,
        },
        reader: "corpus/r2_5".into(),
        budget: RetrievalBudget {
            tokens: 65536,
            k: 64,
        },
        ranker: retrieve::DETERMINISTIC_DEFAULT.into(),
        mode: ResolveMode::Execute,
    };
    let mut sink = CollectSink::default();
    let (items, _report) =
        retrieve::retrieve(store, &req, &mut sink, None, || 0).expect("corpus retrieve");
    let read = sink
        .events
        .iter()
        .find(|(c, _)| c == "context.memory.read")
        .map(|(_, p)| p.clone())
        .unwrap_or(Json::Null);
    let withheld = read
        .get("withheld")
        .and_then(|w| match w {
            Json::Arr(a) => Some(a.len()),
            _ => None,
        })
        .unwrap_or(0);
    (items.len(), withheld)
}

/// Leg 1 — labelled corpora: every member resolves under a floor at-or-
/// below its own authority and withholds above it; the label is the
/// corpus's own (provenance-derived), never test-injected.
#[test]
fn labelled_corpora_retrieval_over_benchset() {
    let bs = Benchset::load_default().expect("benchset loads");
    let mut members = 0u32;
    let mut floor_served = 0u32;
    let mut floor_withheld = 0u32;
    for (key, suite) in &bs.suites {
        let mut store = MemoryStore::new("corpus");
        let _g = store.take_lease(PersistenceScope::Session, "corpus");
        for (i, task) in suite.tasks.iter().enumerate() {
            let body = String::from_utf8_lossy(task.expected()).into_owned();
            let draft = task_draft(
                key,
                &task.name,
                &body,
                Some(SubjectKey {
                    schema_ref: "benchset.stage3.v1".into(),
                    key: task.name.clone(),
                }),
                session_contract(),
            );
            let v = store
                .put(
                    draft,
                    &wctx(&store, PersistenceScope::Session, (i + 1) as u64),
                )
                .expect("corpus write")
                .version;
            store
                .bind(
                    PersistenceScope::Session,
                    &task.name,
                    &v.version_id,
                    None,
                    "corpus",
                    (i + 1) as u64,
                )
                .expect("corpus bind");
            // The member resolves `by_name` under its own scope — the
            // resume_set leg's path (Audit reports the honest state).
            match store.resolve(
                PersistenceScope::Session,
                Some(&task.name),
                None,
                ResolveMode::Audit,
            ) {
                Ok(hh_context::memory::ResolveOutcome::Annotated {
                    state: LifecycleStateKind::Valid,
                    ..
                }) => {}
                other => panic!("{}/{} did not resolve valid: {other:?}", key, task.name),
            }
            members += 1;
        }
        // Retrieve the suite's corpus under the member-derived floors:
        // `Unverified` serves every member; `Principal` withholds the
        // delegate/external-labelled rows (the label is load-bearing).
        let (all, _) = corpus_retrieve(
            &mut store,
            RetrievalQuery::ByRun {
                run_id: "corpus".into(),
                seq_range: None,
                classes: vec![],
            },
            AuthorityClass::Unverified,
        );
        assert_eq!(
            all,
            suite.tasks.len(),
            "{key}: unverified floor must serve every member"
        );
        let (hi, hi_withheld) = corpus_retrieve(
            &mut store,
            RetrievalQuery::ByRun {
                run_id: "corpus".into(),
                seq_range: None,
                classes: vec![],
            },
            AuthorityClass::Principal,
        );
        assert_eq!(hi, 0, "{key}: no member is principal-labelled");
        assert!(hi_withheld >= 1, "{key}: principal floor must withhold");
        floor_served += all as u32;
        floor_withheld += hi_withheld as u32;
    }
    assert!(members >= 20, "corpus membership: {members}");
    assert!(floor_served >= members, "{floor_served} < {members}");
    eprintln!(
        "labelled corpora: {members} members, served {floor_served}, withheld {floor_withheld}"
    );
}

/// Leg 2 — `resume_set` runs: every member's carried head resolves
/// through the `by_name` path `KernelMemory::resume_set_read` runs —
/// `resolve(scope, Some(head), Audit)` lands `Annotated{Valid}`.
#[test]
fn resume_set_members_resolve_over_benchset() {
    let bs = Benchset::load_default().expect("benchset loads");
    let mut members = 0u32;
    for (key, suite) in &bs.suites {
        let mut store = MemoryStore::new("corpus");
        let _g = store.take_lease(PersistenceScope::Session, "corpus");
        for (i, task) in suite.tasks.iter().enumerate() {
            let head = format!("resume:{}", task.name);
            let mut draft = task_draft(key, &task.name, "resume-state", None, session_contract());
            draft.semantic_id = Some(head.clone());
            let v = store
                .put(
                    draft,
                    &wctx(&store, PersistenceScope::Session, (i + 1) as u64),
                )
                .expect("resume head write")
                .version;
            store
                .bind(
                    PersistenceScope::Session,
                    &head,
                    &v.version_id,
                    None,
                    "resume",
                    (i + 1) as u64,
                )
                .expect("resume head bind");
            // The resume path — session scope first, then the wider
            // floors (KernelMemory::resume_set_read's order).
            let resolved = [
                PersistenceScope::Session,
                PersistenceScope::Run,
                PersistenceScope::Project,
                PersistenceScope::User,
                PersistenceScope::Definition,
            ]
            .into_iter()
            .find_map(|scope| {
                store
                    .resolve(scope, Some(&head), None, ResolveMode::Audit)
                    .ok()
            });
            match resolved {
                Some(hh_context::memory::ResolveOutcome::Annotated {
                    state: LifecycleStateKind::Valid,
                    ..
                }) => members += 1,
                other => panic!("{key}/{} resume head unserved: {other:?}", task.name),
            }
        }
    }
    assert!(members >= 20, "resume members: {members}");
    eprintln!("resume_set runs: {members} heads resolved");
}

/// Leg 3 — `environment_epoch` providers: a member's contract carries an
/// `environment_epoch` stamp; the provider republishing the epoch expires
/// the member (`dependency_changed`) — per member, never a blanket flag.
#[test]
fn environment_epoch_providers_expire_over_benchset() {
    let bs = Benchset::load_default().expect("benchset loads");
    let mut expired_members = 0u32;
    for (key, suite) in &bs.suites {
        let mut store = MemoryStore::new("corpus");
        let _g = store.take_lease(PersistenceScope::Session, "corpus");
        let env_ref = format!("env:{key}");
        for (i, task) in suite.tasks.iter().enumerate() {
            let mut contract = session_contract().unwrap();
            contract.dependencies = vec![DependencyStamp {
                kind: DependencyKind::EnvironmentEpoch,
                ref_: env_ref.clone(),
                stamp: "epoch-0".into(),
                granularity: Granularity::Row,
                validator_ref: None,
            }];
            let draft = task_draft(key, &task.name, "epoch-stamped", None, Some(contract));
            store
                .put(
                    draft,
                    &wctx(&store, PersistenceScope::Session, (i + 1) as u64),
                )
                .expect("epoch write");
        }
        // The epoch provider: publish the recorded epoch, observe all
        // members valid; republish the mutation, observe all expired.
        store.set_stamp(&env_ref, "epoch-0");
        let at = store.applied_seq() + 10;
        let n = suite.tasks.len();
        let ids: Vec<String> = store.versions().keys().cloned().collect();
        let still_valid = ids
            .iter()
            .filter(|id| {
                lifecycle::lifecycle_state(&store, id, at).kind() == LifecycleStateKind::Valid
            })
            .count();
        assert_eq!(still_valid, n, "{key}: epoch-0 members must be valid");
        store.set_stamp(&env_ref, "epoch-1");
        for id in &ids {
            match lifecycle::lifecycle_state(&store, id, at) {
                LifecycleState::Expired { reason } => {
                    assert!(reason.starts_with("dependency_changed"), "{key}: {reason}");
                    expired_members += 1;
                }
                other => panic!("{key}: member not expired on epoch bump: {other:?}"),
            }
        }
    }
    assert!(expired_members >= 20, "expired members: {expired_members}");
    eprintln!("environment_epoch providers: {expired_members} members expired on republish");
}

/// Leg 4 — judged-conflict fixtures: every member's conflicting pair
/// mints a `ConflictSet` under the `deterministic` detector; the `judged`
/// member is C2-staged — the battery emits a typed `n/a` per member,
/// never a fabricated judged resolution (DF-S2.8-1(e) corpus coverage).
#[test]
fn conflict_fixtures_and_detector_na_rows_over_benchset() {
    let bs = Benchset::load_default().expect("benchset loads");
    let mut conflicted = 0u32;
    let mut na_rows: Vec<Json> = Vec::new();
    for (key, suite) in &bs.suites {
        let mut store = MemoryStore::new("corpus");
        let _g = store.take_lease(PersistenceScope::Session, "corpus");
        for (i, task) in suite.tasks.iter().enumerate() {
            let subject = SubjectKey {
                schema_ref: "benchset.stage3.v1".into(),
                key: task.name.clone(),
            };
            let d1 = task_draft(
                key,
                &task.name,
                "v1",
                Some(subject.clone()),
                session_contract(),
            );
            let mut d2 = task_draft(
                key,
                &task.name,
                "v2-conflicting",
                Some(subject),
                session_contract(),
            );
            d2.semantic_id = Some(format!("conflict:{}", task.name));
            store
                .put(
                    d1,
                    &wctx(&store, PersistenceScope::Session, (i * 2 + 1) as u64),
                )
                .expect("v1");
            let out = store
                .put(
                    d2,
                    &wctx(&store, PersistenceScope::Session, (i * 2 + 2) as u64),
                )
                .expect("v2");
            let set = out.conflict.unwrap_or_else(|| {
                panic!(
                    "{key}/{}: conflicting writes must mint a ConflictSet",
                    task.name
                )
            });
            assert_eq!(set.detector, "deterministic");
            assert!(matches!(
                set.resolution,
                ConflictResolution::Escalated { .. } | ConflictResolution::Coexist
            ));
            conflicted += 1;
            na_rows.push(Json::obj([
                ("member", Json::str(format!("{key}/{}", task.name))),
                ("leg", Json::str("conflict_detector.judged")),
                ("n/a", Json::str("judged_conflict_detector_staged_c2")),
            ]));
        }
    }
    assert!(conflicted >= 20, "conflict members: {conflicted}");
    assert_eq!(na_rows.len() as u32, conflicted);
    eprintln!(
        "judged-conflict fixtures: {conflicted} sets, {} typed n/a rows",
        na_rows.len()
    );
}

/// DF-S2.8-1(e) corpus coverage — the `judged`/`human` artefact detector
/// legs cannot run corpus-driven hermetically (the corpus records
/// `model_io` surface calls, never judge verdicts or human artefact
/// marks). Every member emits a typed `n/a` row naming the driver-level
/// battery that does exercise the leg — counted, never skipped silently.
#[test]
fn judged_and_human_detector_legs_emit_typed_na_over_benchset() {
    let bs = Benchset::load_default().expect("benchset loads");
    let mut rows = Vec::new();
    for (key, suite) in &bs.suites {
        for task in &suite.tasks {
            let member = format!("{key}/{}", task.name);
            rows.push(Json::obj([
                ("member", Json::str(member.clone())),
                ("leg", Json::str("artefact_detector.judged")),
                (
                    "n/a",
                    Json::str("no_recorded_judge_verdict_in_benchset; driver-level leg: hh-control/tests/r2_5.rs"),
                ),
            ]));
            rows.push(Json::obj([
                ("member", Json::str(member)),
                ("leg", Json::str("artefact_detector.human")),
                (
                    "n/a",
                    Json::str("no_recorded_human_mark_in_benchset; driver-level leg: hh-control/tests/r2_5.rs"),
                ),
            ]));
        }
    }
    let total: usize = bs.suites.values().map(|s| s.tasks.len()).sum();
    assert_eq!(rows.len(), total * 2);
    for r in &rows {
        assert!(r.get("n/a").and_then(Json::as_str).is_some());
    }
    eprintln!(
        "detector corpus rows: {} typed n/a across {total} members",
        rows.len()
    );
}
