//! R5.52 — `hh-outside-withheld/1`: the withheld outside
//! clean-recipient verify/recompute disposition (GATE-G15 PASSED —
//! **out-of-scope disposition**). The participation stage is voided
//! this round — NO outside reviewer exists (HUMAN-H9 withheld,
//! DF-H9-1 open) and no outside verify/recompute runs (reviewer/
//! clearance/cleanup/procedure/card/installation/signer/relationship/
//! assistance/verdict/loss counts all absent); the record is a typed
//! withheld/reviewer-absent packet over supplied facts. Machinery is
//! exercised where meaningful without participation: the ten pinned
//! inputs byte-verify, the §6.3 procedure vocabulary is asserted
//! verbatim (noncumulative `ReproLevel` sum, `FreshLevel` spellings,
//! the closed `cards::code` refusal set, the eight prior-engineering
//! record schemas, `REPLAY_MARKER_PREFIX`, the frozen
//! `within_tolerance` rule), and the reviewer-absent/independence
//! gates produce their verbatim typed outcomes through the real
//! `admit_fresh`/`admit_replay`/`compute_independence` paths — the
//! refusals fire before any card is minted or execution admitted.
//! Fails-without: the `outside` module and this battery do not exist
//! at base `0fb24696` (compile-red evidence).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_analysis::recompute::{within_tolerance, RECOMPUTE_SCHEMA};
use hharness_bundle::assemble::{assemble, AssembleInputs, CompileOutcome};
use hharness_bundle::cards::{
    admit_fresh, admit_replay, code as card_code, FreshClaim, FreshLevel, Isolation, ReplayAttempt,
    FRESH_CARD_SCHEMA, REPLAY_CARD_SCHEMA, REPLAY_MARKER_PREFIX,
};
use hharness_bundle::codec::Decoded;
use hharness_bundle::error::BundleError;
use hharness_bundle::manifest::{BundlePolicy, ReproLevel};
use hharness_bundle::repro::{compute_independence, REPRO_SCHEMA};
use hharness_bundle::rights::{
    HANDOFF_REPORT_SCHEMA, RIGHTS_RECORD_SCHEMA, SUPERSESSION_SCHEMA, TOMBSTONE_SCHEMA,
};
use hharness_env::outside::{
    assemble as assemble_packet, evaluate_claims, evaluate_pins, evaluate_procedure,
    evaluate_refusals, evaluate_vocabulary, ClaimsFacts, Disposition, PinsFacts, ProcedureFacts,
    VocabularyFacts, EXERCISES, EXPECTED_CARD_CODES, EXPECTED_FRESH_LEVELS, EXPECTED_MARKER_PREFIX,
    EXPECTED_REPRO_LEVELS, EXPECTED_SCHEMAS, GATE, LEGS, PROCEDURE_MEMBERS, VERSION,
};
use hharness_env::plugin::{evaluate_gate, GateFacts};
use hharness_env::preflight::{check_pins, Leg, LegVerdict, PinnedArtifact};
use hharness_ledger::event::{Event, Producer, Scope};
use hharness_ledger::ids::ROOT_EVENT;
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

const TS: &str = "2026-01-01T00:00:00.000Z";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ── the ten pinned inputs (nine H8 artifacts + the committed
// cross-impl `bundle.json` native bundle manifest — the cleared-
// artifact substrate the verify/recompute handoff would carry;
// pin 10, id path sha256, verbatim) ─────────────────────────────
const ARTIFACTS: &str = "\
a-count crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-count.json 9f6d4f11695efff5671745015f7f7c87d9e4e95f049364e027e6a60fc4124ce6
a-echo crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-echo.json 9fdc327157877c3fdbb124a0a5f98b5a2df0d22a7889c4da9e11319559977b40
a-hello crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-hello.json 5bf206ca07f13659bc7bc69a71f7d7dd4b7348a1f635bfdb9a4dccc0912e0344
a-sum crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-sum.json e30c65b99203d604f9a843e72c31fd1a053bf31c8ab901bb295c2773b0f8e349
a-upper crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-upper.json 3b489ba6cf9d225f926f34c027899583b00d0e6607342c987c593fe7f1fc87a4
ex-01 scripts/examples/ex-01.json 0d4cec8dea92168636d2224ecb392628c69b28bc73353674312183db472178e3
ex-02 scripts/examples/ex-02.json 8e35aac7c68aba68d3d2ff1c9abf23bf5ff7f6bff372f948eede02f1eea46285
ex-02-fixture scripts/examples/ex-02-fixture.json f21a340f966d2d21f72f424635461906dbff16e09cfb2f6d9844ad8e203300c8
fixture-definition clients/ts/fixture-definition.json 732af9f584a9f321d347f6f027ebd82a3376a33bb9580f63b2fcd6602865d0f8
cleared-substrate crates/hh-xcheck/fixtures/cross-impl/bundle.json e9d9ef074445ea6952c9c953a6a190bfa13d53e6f3a26b543dd9e196f762fdd0";

fn artifact_pins() -> Vec<PinnedArtifact> {
    ARTIFACTS
        .lines()
        .map(|l| {
            let mut c = l.split_whitespace();
            PinnedArtifact {
                artifact_id: c.next().unwrap().to_string(),
                path: repo_root().join(c.next().unwrap()),
                sha256: c.next().unwrap().to_string(),
            }
        })
        .collect()
}

// ── leg 1 — gate_readout (GATE-G15 verbatim — the R5.50 gate
// contract consumed unchanged) ──────────────────────────────────
fn gate_facts() -> GateFacts {
    GateFacts {
        decision: "PASSED — out-of-scope disposition".into(),
        operator_words: "accept, continue".into(),
        decided_at: "2026-11-04T14:32:29Z".into(),
        block_sha256_prefix: "0e5bd136858c".into(),
        df_h9_1: "open".into(),
    }
}

// ── leg 2 — pins_frozen ──────────────────────────────────────────
fn pins_facts() -> PinsFacts {
    let verdict = check_pins(&artifact_pins());
    PinsFacts {
        pins: artifact_pins().len() as u64,
        pins_verdict: verdict.name().to_string(),
        bundle_sha256: artifact_pins()
            .iter()
            .find(|a| a.artifact_id == "cleared-substrate")
            .unwrap()
            .sha256
            .clone(),
    }
}

// ── leg 3 — procedure_vocabulary: the §6.3 decls read from the
// landed machinery (facts supplied verbatim, never re-declared) ──
fn vocabulary_facts() -> VocabularyFacts {
    let repro_levels = ReproLevel::all()
        .iter()
        .map(|l| l.name())
        .collect::<Vec<_>>()
        .join(",");
    let repro_parse_roundtrip = ReproLevel::all()
        .iter()
        .all(|l| ReproLevel::parse(l.name()) == Some(*l));
    // `transfer` is a `FreshLevel`, never an R level — the grammar
    // never guesses.
    let repro_parse_never_guesses = ["r0", "R4", "transfer", "replay", "", "R0 "]
        .iter()
        .all(|s| ReproLevel::parse(s).is_none());
    let fresh_levels = [FreshLevel::R2, FreshLevel::R3, FreshLevel::Transfer]
        .iter()
        .map(|l| l.name())
        .collect::<Vec<_>>()
        .join(",");
    let card_codes = [
        card_code::MISSING_INPUT,
        card_code::UNSUPPORTED,
        card_code::ISOLATION_DISABLED,
        card_code::UNFUNDED,
        card_code::NOT_FRESH,
        card_code::CHARGING_ATTRIBUTION,
        card_code::UNAVAILABLE,
        card_code::NOT_INDEPENDENT,
        card_code::OUTSIDE_RELATIONSHIP,
        card_code::NO_NEW_PHASE,
        card_code::NEW_QUESTION,
    ]
    .join(",");
    let record_schemas = [
        REPRO_SCHEMA,
        RECOMPUTE_SCHEMA,
        REPLAY_CARD_SCHEMA,
        FRESH_CARD_SCHEMA,
        RIGHTS_RECORD_SCHEMA,
        HANDOFF_REPORT_SCHEMA,
        SUPERSESSION_SCHEMA,
        TOMBSTONE_SCHEMA,
    ]
    .join(",");
    // The frozen scalar rule demonstrated: equality holds at ppm
    // magnitudes; a unit drift at 1e6 fails (abs ≤ 1e-10 ×
    // max(1,abs(ref)) is never widened); the zero-reference bound
    // holds; a unit against a zero reference fails.
    let tolerance_holds = within_tolerance(1_000_000, 1_000_000)
        && !within_tolerance(1_000_001, 1_000_000)
        && within_tolerance(0, 0)
        && !within_tolerance(1, 0);
    VocabularyFacts {
        repro_levels,
        repro_parse_roundtrip,
        repro_parse_never_guesses,
        fresh_levels,
        card_codes,
        record_schemas,
        replay_marker_prefix: REPLAY_MARKER_PREFIX.to_string(),
        tolerance_holds,
    }
}

// ── leg 4 — refusal_vocabulary: exercised cases through the real
// card/independence machinery (refusals fire before any card is
// minted or any execution admitted — pure in-process) ────────────

/// A store with a subject run carrying the caller's events and a
/// decoded bundle depositing the `definition` + `compiled_bundle`
/// members — the r5_26 rig shape: a temp-dir `Store`, never an
/// outside recipient.
struct Rig {
    run: String,
    decoded: Decoded,
}

fn ev(id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: id.to_string(),
        class: class.to_string(),
        ts: TS.to_string(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: ROOT_EVENT.to_string(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", 0)),
        content_kind: None,
        payload,
    }
}

fn rig(tag: &str, events: Vec<Event>) -> Rig {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-env-r5_52-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let mut store = Store::open(&d).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("definition"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hharness_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    let (run, lease) = store.open_run(m, "r52.test").unwrap();
    store.append(&run, &lease, events).unwrap();
    let db = def_bytes.clone();
    let da = def_addr.clone();
    let get = move |a: &str| (a == da).then(|| db.clone());
    let ab: &dyn Fn(&str) -> Option<Vec<u8>> = Box::leak(Box::new(get));
    let out = assemble(&AssembleInputs {
        store: &store,
        run_id: &run,
        policy: &BundlePolicy::default(),
        created_at: TS.into(),
        producer: ProvenanceRecord::kernel("r52.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: ab,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: Some(CompileOutcome {
            bundle_id: "sha256:c".into(),
            derivation_key: "sha256:dk".into(),
            member_bytes: b"{}".to_vec(),
            lcd_report: Json::obj([]),
            opacity_report: Json::obj([]),
            validators: vec![Json::obj([("v", Json::Int(1))])],
            profile_refs: vec![],
        }),
        model_snapshots: vec![],
        registry_snapshot_id: None,
        variants: vec![],
        extensions: vec![],
        budget: Json::Null,
        profile: Json::str("none"),
        profile_refs: vec![],
        images: vec![],
        nondeterminism: vec![],
        participant_class: None,
        evidence_profile: None,
    })
    .unwrap();
    Rig {
        run,
        decoded: Decoded {
            manifest: out.manifest,
            members: out.members,
        },
    }
}

/// The observed spelling of a card outcome — the `CardRefused`
/// code verbatim on a refusal (never a paraphrase), `admitted`
/// when the machinery mints an admission card.
fn card_observed(r: Result<hharness_bundle::cards::AdmissionCard, BundleError>) -> String {
    match r {
        Err(BundleError::CardRefused { code, .. }) => code.to_string(),
        Err(e) => format!("{e:?}")
            .split([' ', '{', '('])
            .next()
            .unwrap_or_default()
            .to_string(),
        Ok(_) => "admitted".to_string(),
    }
}

/// A `FreshClaim` scaffold — every gate satisfied except the one
/// each exercise probes (the refusal is the probed gate's own).
fn fresh_claim() -> FreshClaim {
    FreshClaim {
        level: FreshLevel::R2,
        new_subject: true,
        funded: true,
        model_retired: false,
        seeds: vec![1],
        seed_distribution: None,
        planned_arms: 0,
        min_arms: 0,
        matchspec: None,
        margins: None,
        charging_retains_attribution: true,
        new_phase: false,
        comparison_criterion: None,
        comparison_target: None,
        outside_authority: None,
        outside_attestation: false,
        reproducer: Json::Null,
        reproducer_instrument: Json::Null,
        new_task_or_model: false,
    }
}

/// A disabled-isolation `ReplayAttempt` scaffold — the observed
/// attempt the exercises flip (the isolation gate fires before the
/// prefix/decision legs).
fn replay_attempt(isolated: bool) -> ReplayAttempt {
    ReplayAttempt {
        copy: vec![],
        replayed_decisions: vec![],
        replayed_assembled: vec![],
        rederived: vec![],
        isolation: Isolation {
            provider_transport_disabled: isolated,
            effect_execution_disabled: isolated,
            attempted_provider_dispatches: 0,
            attempted_effect_executions: 0,
        },
        claim_r1: false,
    }
}

fn signer_provenance(signer: &str) -> Json {
    Json::obj([(
        "attestation",
        Json::obj([("anchor", Json::obj([("signer", Json::str(signer))]))]),
    )])
}

fn instrument(installation: &str) -> Json {
    Json::obj([("installation_id", Json::str(installation))])
}

/// `exercise → observed verbatim outcome` — every case is driven
/// through the real machinery on a fresh temp-dir store; each
/// refusal fires before any card is minted or any execution is
/// admitted (nothing runs for an outside reviewer).
fn exercise_refusals() -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let r = rig(
        "cards",
        vec![
            ev("evt-1", "lifecycle.component.bound", Json::obj([])),
            ev(
                "evt-2",
                "control.decision",
                Json::obj([
                    ("kind", Json::str("act")),
                    ("decider", Json::str("strategy")),
                    ("call_no", Json::Int(1)),
                ]),
            ),
            ev(
                "evt-3",
                "model.call.attempt.completed",
                Json::obj([
                    ("model_call_id", Json::str("mc-1")),
                    ("response_ref", Json::str("sha256:resp1")),
                ]),
            ),
            ev("evt-4", "lifecycle.run.finished", Json::obj([])),
        ],
    );
    // `admit_fresh` — no funded replication: $0 stays, zero
    // dispatch.
    out.insert(
        "fresh_unfunded".into(),
        card_observed(admit_fresh(
            &r.decoded,
            &r.run,
            &FreshClaim {
                funded: false,
                ..fresh_claim()
            },
        )),
    );
    // `admit_fresh` — no new subject/calls/cost/grade: not a fresh
    // execution (no invented fresh success).
    out.insert(
        "fresh_not_fresh".into(),
        card_observed(admit_fresh(
            &r.decoded,
            &r.run,
            &FreshClaim {
                new_subject: false,
                ..fresh_claim()
            },
        )),
    );
    // `admit_fresh` transfer — outside authority/assistance/
    // attestation undeclared: the reviewer-absent gate itself.
    out.insert(
        "transfer_outside_undeclared".into(),
        card_observed(admit_fresh(
            &r.decoded,
            &r.run,
            &FreshClaim {
                level: FreshLevel::Transfer,
                ..fresh_claim()
            },
        )),
    );
    // `admit_fresh` transfer — own-task/new-model: a new question,
    // never a replication.
    out.insert(
        "transfer_new_question".into(),
        card_observed(admit_fresh(
            &r.decoded,
            &r.run,
            &FreshClaim {
                level: FreshLevel::Transfer,
                new_task_or_model: true,
                ..fresh_claim()
            },
        )),
    );
    // `admit_replay` — provider transport left enabled: any
    // attempted live dispatch fails (the isolation gate fires
    // before the prefix/decision legs).
    out.insert(
        "replay_isolation_enabled".into(),
        card_observed(admit_replay(&r.decoded, &r.run, &replay_attempt(false))),
    );
    // `admit_replay` on a run with no recorded provider responses
    // — a required recorded input is absent.
    let quiet = rig(
        "quiet",
        vec![
            ev("evt-1", "lifecycle.component.bound", Json::obj([])),
            ev(
                "evt-2",
                "control.decision",
                Json::obj([("kind", Json::str("act"))]),
            ),
            ev("evt-3", "lifecycle.run.finished", Json::obj([])),
        ],
    );
    out.insert(
        "replay_responses_absent".into(),
        card_observed(admit_replay(
            &quiet.decoded,
            &quiet.run,
            &replay_attempt(true),
        )),
    );
    // `compute_independence` — the same-author directory negative:
    // equal signer proves no independence.
    out.insert(
        "independence_same_signer".into(),
        compute_independence(
            &signer_provenance("prod"),
            &instrument("inst-a"),
            &signer_provenance("prod"),
            &instrument("inst-b"),
        )
        .1
        .to_string(),
    );
    // `compute_independence` — undeclared reproducer material
    // fails closed (the reviewer-absent fact pattern).
    out.insert(
        "independence_undeclared".into(),
        compute_independence(
            &signer_provenance("prod"),
            &instrument("inst-a"),
            &Json::obj([]),
            &instrument("inst-b"),
        )
        .1
        .to_string(),
    );
    // `compute_independence` — distinct signer+installation: the
    // declared-material comparison stands (machinery output,
    // never a relationship claim).
    out.insert(
        "independence_distinct".into(),
        compute_independence(
            &signer_provenance("prod"),
            &instrument("inst-a"),
            &signer_provenance("outside"),
            &instrument("inst-b"),
        )
        .1
        .to_string(),
    );
    out
}

// ── legs 5+6 — procedure_absent / claims_withheld ────────────────
fn procedure_facts() -> ProcedureFacts {
    ProcedureFacts {
        counts: PROCEDURE_MEMBERS
            .iter()
            .map(|m| (m.to_string(), 0))
            .collect(),
        reviewer_contacted: false,
        handoff_attempted: false,
        cleanup_attempted: false,
    }
}

fn claims_facts() -> ClaimsFacts {
    ClaimsFacts {
        df_h9_1: "open".into(),
        participant_predicates: "withheld".into(),
        outreach: "none".into(),
        claim_awarded: false,
        outside_review_claim: false,
        independence_claim: false,
        relationship_claim: false,
        r_level_awarded: false,
        r1_replay_claim: false,
        fresh_claim: false,
        transfer_claim: false,
        portability_claim: false,
        publication_claim: false,
        capability_claim: false,
        adoption_claim: false,
    }
}

fn leg(name: &str, verdict: LegVerdict, facts: &[(&str, String)]) -> Leg {
    Leg {
        name: name.into(),
        verdict,
        facts: facts
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    }
}

fn s(v: impl ToString) -> String {
    v.to_string()
}

/// The six legs driven on real inputs (pins recomputed, the
/// procedure vocabulary read from the landed machinery, the
/// refusal/independence cases exercised, the absent counts and
/// withheld claims folded) — the packet's honest record.
fn run_packet() -> Vec<Leg> {
    let g = gate_facts();
    let pf = pins_facts();
    let v = vocabulary_facts();
    let refusals = exercise_refusals();
    let p = procedure_facts();
    let c = claims_facts();
    vec![
        leg(
            "gate_readout",
            evaluate_gate(&g),
            &[
                ("decision", s(&g.decision)),
                ("operator_words", s(&g.operator_words)),
                ("decided_at", s(&g.decided_at)),
                ("block_sha256_prefix", s(&g.block_sha256_prefix)),
                ("df_h9_1", s(&g.df_h9_1)),
            ],
        ),
        leg(
            "pins_frozen",
            evaluate_pins(&pf),
            &[
                ("pins", s(pf.pins)),
                ("pins_verdict", s(&pf.pins_verdict)),
                ("bundle_sha256", s(&pf.bundle_sha256)),
            ],
        ),
        leg(
            "procedure_vocabulary",
            evaluate_vocabulary(&v),
            &[
                ("repro_levels", s(&v.repro_levels)),
                ("repro_parse_roundtrip", s(v.repro_parse_roundtrip)),
                ("repro_parse_never_guesses", s(v.repro_parse_never_guesses)),
                ("fresh_levels", s(&v.fresh_levels)),
                ("card_codes", s(&v.card_codes)),
                ("record_schemas", s(&v.record_schemas)),
                ("replay_marker_prefix", s(&v.replay_marker_prefix)),
                ("tolerance_holds", s(v.tolerance_holds)),
            ],
        ),
        leg(
            "refusal_vocabulary",
            evaluate_refusals(&refusals),
            &refusals
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect::<Vec<_>>(),
        ),
        leg(
            "procedure_absent",
            evaluate_procedure(&p),
            &p.counts
                .iter()
                .map(|(k, v)| (k.as_str(), s(v)))
                .chain([
                    ("reviewer_contacted", s(p.reviewer_contacted)),
                    ("handoff_attempted", s(p.handoff_attempted)),
                    ("cleanup_attempted", s(p.cleanup_attempted)),
                ])
                .collect::<Vec<_>>(),
        ),
        leg(
            "claims_withheld",
            evaluate_claims(&c),
            &[
                ("df_h9_1", s(&c.df_h9_1)),
                ("participant_predicates", s(&c.participant_predicates)),
                ("outreach", s(&c.outreach)),
                ("claim_awarded", s(c.claim_awarded)),
                ("outside_review_claim", s(c.outside_review_claim)),
                ("independence_claim", s(c.independence_claim)),
                ("relationship_claim", s(c.relationship_claim)),
                ("r_level_awarded", s(c.r_level_awarded)),
                ("r1_replay_claim", s(c.r1_replay_claim)),
                ("fresh_claim", s(c.fresh_claim)),
                ("transfer_claim", s(c.transfer_claim)),
                ("portability_claim", s(c.portability_claim)),
                ("publication_claim", s(c.publication_claim)),
                ("capability_claim", s(c.capability_claim)),
                ("adoption_claim", s(c.adoption_claim)),
            ],
        ),
    ]
}

// ── the battery ──────────────────────────────────────────────────

#[test]
fn p52_gate_and_pins_verified() {
    assert_eq!(evaluate_gate(&gate_facts()), LegVerdict::Verified);
    // A procedures-admitted decision, a missing verbatim member or
    // a closed DF-H9-1 refuses — never relabelled.
    for f in [
        GateFacts {
            decision: "PASSED — procedures admitted".into(),
            ..gate_facts()
        },
        GateFacts {
            df_h9_1: "closed".into(),
            ..gate_facts()
        },
        GateFacts {
            block_sha256_prefix: "not-hex!!".into(),
            ..gate_facts()
        },
    ] {
        assert!(matches!(evaluate_gate(&f), LegVerdict::Refused(_)));
    }
    // The ten pinned inputs re-verify byte-for-byte; drift refuses,
    // unreadable is inconclusive.
    assert_eq!(check_pins(&artifact_pins()), LegVerdict::Verified);
    assert_eq!(evaluate_pins(&pins_facts()), LegVerdict::Verified);
    let mut drifted = artifact_pins();
    drifted[0].sha256 = "0".repeat(64);
    assert!(matches!(check_pins(&drifted), LegVerdict::Refused(_)));
    let mut unreadable = artifact_pins();
    unreadable[0].path = repo_root().join("does-not-exist.json");
    assert!(matches!(
        check_pins(&unreadable),
        LegVerdict::Inconclusive(_)
    ));
    for f in [
        PinsFacts {
            pins: 9,
            ..pins_facts()
        },
        PinsFacts {
            pins_verdict: "refused".into(),
            ..pins_facts()
        },
    ] {
        assert!(matches!(evaluate_pins(&f), LegVerdict::Refused(_)));
    }
    let mut f = pins_facts();
    f.bundle_sha256.clear();
    assert!(matches!(evaluate_pins(&f), LegVerdict::Inconclusive(_)));
}

#[test]
fn p52_procedure_vocabulary_and_refusals() {
    // The §6.3 procedure vocabulary is the landed decl verbatim:
    // the noncumulative `ReproLevel` sum (`R0|R1|R2|R3`,
    // `parse` never guesses — `transfer` is not an R level); the
    // `FreshLevel` spellings (`r2|r3|transfer`); the closed
    // eleven-code `cards::code` set; the eight prior-engineering
    // record schemas; `REPLAY_MARKER_PREFIX` (`lifecycle.replay.`);
    // the frozen `within_tolerance` rule demonstrated.
    let v = vocabulary_facts();
    assert_eq!(v.repro_levels, EXPECTED_REPRO_LEVELS);
    assert!(v.repro_parse_roundtrip && v.repro_parse_never_guesses);
    assert_eq!(v.fresh_levels, EXPECTED_FRESH_LEVELS);
    assert_eq!(v.card_codes, EXPECTED_CARD_CODES);
    assert_eq!(v.record_schemas, EXPECTED_SCHEMAS);
    assert_eq!(v.replay_marker_prefix, EXPECTED_MARKER_PREFIX);
    assert!(v.tolerance_holds);
    assert_eq!(evaluate_vocabulary(&v), LegVerdict::Verified);
    // Drifted vocabulary refuses; an absent sum is inconclusive,
    // never verified.
    for f in [
        VocabularyFacts {
            repro_levels: "R0,R1,R2".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            repro_parse_never_guesses: false,
            ..vocabulary_facts()
        },
        VocabularyFacts {
            fresh_levels: "r2,r3".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            card_codes: "missing_input".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            record_schemas: "hh-repro/1".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            replay_marker_prefix: "lifecycle.".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            tolerance_holds: false,
            ..vocabulary_facts()
        },
    ] {
        assert!(matches!(evaluate_vocabulary(&f), LegVerdict::Refused(_)));
    }
    let mut f = vocabulary_facts();
    f.repro_levels.clear();
    assert!(matches!(
        evaluate_vocabulary(&f),
        LegVerdict::Inconclusive(_)
    ));
    // The landed machinery stands — every exercised case produces
    // its verbatim typed outcome through the real
    // `admit_fresh`/`admit_replay`/`compute_independence` paths;
    // nothing admits (`admitted`/`card_minted` would be the guard
    // broken).
    let observed = exercise_refusals();
    for (ex, expected) in EXERCISES {
        assert_eq!(observed.get(ex).map(String::as_str), Some(expected), "{ex}");
    }
    assert_eq!(evaluate_refusals(&observed), LegVerdict::Verified);
    // `admitted`/`card_minted` is the machinery broken;
    // wrong/missing → refused/inconclusive, never verified.
    for (ex, o) in [
        ("transfer_outside_undeclared", "admitted"),
        ("replay_isolation_enabled", "exported"),
        (
            "independence_same_signer",
            "distinct_signer_and_installation",
        ),
    ] {
        let mut bad = observed.clone();
        bad.insert(ex.into(), o.into());
        assert!(matches!(evaluate_refusals(&bad), LegVerdict::Refused(_)));
    }
    let mut missing = observed;
    missing.remove("independence_distinct");
    assert!(matches!(
        evaluate_refusals(&missing),
        LegVerdict::Inconclusive(_)
    ));
}

#[test]
fn p52_procedure_absent_and_claims() {
    assert_eq!(evaluate_procedure(&procedure_facts()), LegVerdict::Verified);
    assert_eq!(evaluate_claims(&claims_facts()), LegVerdict::Verified);
    // Any nonzero procedure fact is a real procedure's evidence —
    // refused here, never a fabricated absence.
    for member in PROCEDURE_MEMBERS {
        let mut p = procedure_facts();
        p.counts.insert(member.into(), 1);
        assert!(matches!(evaluate_procedure(&p), LegVerdict::Refused(_)));
    }
    let mut missing = procedure_facts();
    missing.counts.remove("outside_reviewers_engaged");
    assert!(matches!(
        evaluate_procedure(&missing),
        LegVerdict::Inconclusive(_)
    ));
    for p in [
        ProcedureFacts {
            reviewer_contacted: true,
            ..procedure_facts()
        },
        ProcedureFacts {
            handoff_attempted: true,
            ..procedure_facts()
        },
        ProcedureFacts {
            cleanup_attempted: true,
            ..procedure_facts()
        },
    ] {
        assert!(matches!(evaluate_procedure(&p), LegVerdict::Refused(_)));
    }
    // A claim of any sign — or a discharged deferral — refuses,
    // never relabelled.
    for f in [
        ClaimsFacts {
            df_h9_1: "closed".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            participant_predicates: "admitted".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            outreach: "issue_comment".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            claim_awarded: true,
            ..claims_facts()
        },
        ClaimsFacts {
            outside_review_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            independence_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            relationship_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            r_level_awarded: true,
            ..claims_facts()
        },
        ClaimsFacts {
            r1_replay_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            fresh_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            transfer_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            portability_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            publication_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            capability_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            adoption_claim: true,
            ..claims_facts()
        },
    ] {
        assert!(matches!(evaluate_claims(&f), LegVerdict::Refused(_)));
    }
}

#[test]
fn p52_packet_withheld_honest() {
    let legs = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble_packet(legs).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.gate, GATE);
    assert!(report.withheld && report.stage_voided && report.reviewer_absent);
    assert!(report.df_h9_1_open);
    assert_eq!(report.procedures_completed, 0);
    assert_eq!(report.admission, "withheld_stage_voided");
    assert_eq!(report.disposition, Disposition::Withheld);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\":\"withheld\""));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Emit the withheld record when asked (the committed artifact
    // is produced by this exact path — never hand-written).
    if let Ok(out) = std::env::var("HH_OUTSIDE_WITHHELD_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, format!("{j}\n")).unwrap();
        eprintln!("emitted {} disposition=withheld", out.display());
    }
}

#[test]
fn p52_fold_honest_and_legs_closed() {
    let mk = |v: LegVerdict| -> Vec<Leg> { LEGS.iter().map(|n| leg(n, v.clone(), &[])).collect() };
    assert_eq!(
        assemble_packet(mk(LegVerdict::Verified))
            .unwrap()
            .disposition,
        Disposition::Withheld
    );
    assert_eq!(
        assemble_packet(mk(LegVerdict::Inconclusive("x".into())))
            .unwrap()
            .disposition,
        Disposition::Inconclusive
    );
    // A definitive failure dominates an undetermined leg.
    let mut legs = mk(LegVerdict::Inconclusive("x".into()));
    legs[3].verdict = LegVerdict::Refused("z".into());
    assert_eq!(
        assemble_packet(legs).unwrap().disposition,
        Disposition::Failed
    );
    // Malformed leg sets refuse — never a partial packet.
    assert!(assemble_packet(mk(LegVerdict::Verified)[..5].to_vec()).is_err());
    let mut bad = mk(LegVerdict::Verified);
    bad[0].name = "not_a_leg".into();
    assert!(assemble_packet(bad).is_err());
    // A refused leg never folds to withheld.
    let mut legs = run_packet();
    legs[4].verdict = LegVerdict::Refused("procedure_present".into());
    assert_eq!(
        assemble_packet(legs).unwrap().disposition,
        Disposition::Failed
    );
}
