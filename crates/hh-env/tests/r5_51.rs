//! R5.51 — `hh-interchange-unselected/1`: the unselected
//! native-to-external consumer disposition (GATE-G15 PASSED —
//! **out-of-scope disposition**). The participation stage is
//! voided this round — no consumer/direction is selected and no
//! artifact→format→reader exercise runs (export/foreign-file/
//! external-reader/field-loss/direction-version-control/return-
//! import counts all absent); the record is a typed unselected
//! packet over supplied facts. Machinery is exercised where
//! meaningful without participation: the ten pinned inputs
//! byte-verify, the `ForeignFormat` closed-sum/direction-table
//! decls are asserted verbatim, and the direction/unknown-format
//! refusals produce their typed spellings through the real
//! `lift_foreign`/`export_target`/`lift` paths — the refusals fire
//! before any foreign artifact is produced or consumed. Fails-
//! without: the `interchange` module and this battery do not
//! exist at base `0b61d2be` (compile-red evidence).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_bundle::assemble::{assemble, AssembleInputs};
use hharness_bundle::codec::Decoded;
use hharness_bundle::error::BundleError;
use hharness_bundle::export::{export_target, PublicationPolicy};
use hharness_bundle::foreign::ForeignFormat;
use hharness_bundle::import::{lift, lift_foreign, FOREIGN_FORMATS};
use hharness_bundle::manifest::BundlePolicy;
use hharness_env::interchange::{
    assemble as assemble_packet, evaluate_claims, evaluate_exercise, evaluate_pins,
    evaluate_refusals, evaluate_vocabulary, ClaimsFacts, Disposition, ExerciseFacts, PinsFacts,
    VocabularyFacts, EXERCISES, EXERCISE_MEMBERS, EXPECTED_LIFTABLE, EXPECTED_SUM, GATE, LEGS,
    PREFERRED_CANDIDATE, VERSION,
};
use hharness_env::plugin::{evaluate_gate, GateFacts};
use hharness_env::preflight::{check_pins, Leg, LegVerdict, PinnedArtifact};
use hharness_ledger::manifest::{RunKind, RunManifest};
use hharness_ledger::store::Store;
use hharness_provenance::ProvenanceRecord;
use hharness_wire::json::Json;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ── the ten pinned inputs (nine H8 artifacts + the committed
// `inspect_eval_log` foreign fixture — S6's preferred consumer
// format, pin 10; id path sha256, verbatim) ─────────────────────
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
inspect-fixture crates/hh-bundle/fixtures/foreign/inspect_eval_log/eval_log.json 4bae98032d76d42b1c8fcead473cd32b95ad30f33fab5ee43b9c158dae0c4b21";

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
        fixture_sha256: artifact_pins()
            .iter()
            .find(|a| a.artifact_id == "inspect-fixture")
            .unwrap()
            .sha256
            .clone(),
    }
}

// ── leg 3 — format_vocabulary: the `ForeignFormat` closed-sum
// decls read from the landed machinery (the one grammar — facts
// supplied verbatim, never re-declared) ─────────────────────────
fn vocabulary_facts() -> VocabularyFacts {
    let closed_sum = ForeignFormat::ALL
        .iter()
        .map(|f| f.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let parse_roundtrip = ForeignFormat::ALL
        .iter()
        .all(|f| ForeignFormat::parse(f.as_str()) == Some(*f));
    let parse_never_guesses = [
        "swebench",
        "inspect_eval",
        "intoto",
        "otlp",
        "",
        "HARBOR_JOB_DIR",
        "bogus_foreign",
    ]
    .iter()
    .all(|s| ForeignFormat::parse(s).is_none());
    let import_only = ForeignFormat::ALL
        .iter()
        .filter(|f| f.lifts() && !f.exports())
        .map(|f| f.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let export_only = ForeignFormat::ALL
        .iter()
        .filter(|f| f.exports() && !f.lifts())
        .map(|f| f.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let bidirectional = ForeignFormat::ALL
        .iter()
        .filter(|f| f.lifts() && f.exports())
        .count();
    let liftable = ForeignFormat::liftable_spellings();
    let liftable_is_import_list = liftable == FOREIGN_FORMATS.to_vec();
    VocabularyFacts {
        formats: ForeignFormat::ALL.len() as u64,
        closed_sum,
        parse_roundtrip,
        parse_never_guesses,
        import_only,
        export_only,
        bidirectional: bidirectional as u64,
        liftable: liftable.join(","),
        liftable_is_import_list,
        preferred_candidate: PREFERRED_CANDIDATE.to_string(),
    }
}

// ── leg 4 — refusal_vocabulary: exercised cases through the real
// import/export machinery (refusals fire before any artifact is
// produced — pure in-process, nothing external) ─────────────────

/// A store with a subject run (the decoded bundle's source) and a
/// hosted carrier run (the lift's subject) — the r4_9 rig shape:
/// a temp-dir `Store`, never an external consumer.
struct Rig {
    store: Store,
    run: String,
    carrier: String,
    def_addr: String,
    def_bytes: Vec<u8>,
}

fn rig(tag: &str) -> Rig {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-env-r5_51-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let mut store = Store::open(&d).unwrap();
    let def_bytes = Json::obj([("sealed", Json::str("r5.51"))])
        .to_canonical_string()
        .into_bytes();
    let def_addr = hharness_identity::address(&def_bytes, "application/vnd.hh.sealed+json").id();
    let mut m = RunManifest::minimal(RunKind::Agent);
    m.harness_def_ref = Some(def_addr.clone());
    let (run, _l) = store.open_run(m, "r51.test").unwrap();
    let (carrier, _l2) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "r51.test")
        .unwrap();
    Rig {
        store,
        run,
        carrier,
        def_addr,
        def_bytes,
    }
}

fn decoded(rig: &Rig) -> Decoded {
    let get = |a: &str| (a == rig.def_addr).then(|| rig.def_bytes.clone());
    let a = assemble(&AssembleInputs {
        store: &rig.store,
        run_id: &rig.run,
        policy: &BundlePolicy::default(),
        created_at: "2026-01-01T00:00:00.000Z".into(),
        producer: ProvenanceRecord::kernel("r51.test", 0).to_json(),
        contract_identity: Json::Null,
        kernel_version_id: "sha256:kernel".into(),
        instrument_dirty: false,
        artifact_bytes: &get,
        environment: Json::obj([("platform", Json::str("test"))]),
        compiled: None,
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
    Decoded {
        manifest: a.manifest,
        members: a.members,
    }
}

/// The `BundleError` variant name verbatim — the observed refusal
/// the leg echoes (the Debug discriminant, never a paraphrase).
fn error_kind(e: &BundleError) -> String {
    let dbg = format!("{e:?}");
    dbg.split([' ', '{', '('])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// `exercise → observed refusal variant` — every case is driven
/// through the real machinery on a fresh temp-dir store; each
/// refusal fires before any foreign artifact is produced or
/// consumed (nothing exports, nothing lifts).
fn exercise_refusals() -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let r = rig("vocab");
    let d = decoded(&r);
    // `interchange_trajectory` at `import` — the return-import
    // guard (OQ-030/ADR-0213): the direction check fires first,
    // before a single file is read (empty file map supplied).
    out.insert(
        "interchange_at_import".into(),
        match lift_foreign(
            &r.store,
            &r.carrier,
            &BTreeMap::new(),
            ForeignFormat::InterchangeTrajectory,
            ProvenanceRecord::kernel("r51.test", 0).to_json(),
            "2026-01-01T00:00:00.000Z".into(),
            "sha256:kernel",
            0,
        ) {
            Err(e) => error_kind(&e),
            Ok(_) => "lifted".to_string(),
        },
    );
    // `harbor_trial_dir` at `export` — known-but-import-only.
    out.insert(
        "harbor_trial_at_export".into(),
        match export_target(&d, "harbor_trial_dir", &PublicationPolicy::default()) {
            Err(e) => error_kind(&e),
            Ok(_) => "exported".to_string(),
        },
    );
    // An outside spelling at `export` — the grammar never guesses.
    out.insert(
        "unknown_spelling_export".into(),
        match export_target(&d, "bogus_foreign", &PublicationPolicy::default()) {
            Err(e) => error_kind(&e),
            Ok(_) => "exported".to_string(),
        },
    );
    // A foreign spelling at the native `lift` — only
    // `ledger_native` lifts at this stage (the foreign lift path
    // is `lift_foreign`, never `lift`).
    out.insert(
        "foreign_at_native_lift".into(),
        match lift(&d, "inspect_eval_log", 0) {
            Err(e) => error_kind(&e),
            Ok(_) => "lifted".to_string(),
        },
    );
    out
}

// ── legs 5+6 — exercise_absent / claims_withheld ─────────────────
fn exercise_facts() -> ExerciseFacts {
    ExerciseFacts {
        counts: EXERCISE_MEMBERS
            .iter()
            .map(|m| (m.to_string(), 0))
            .collect(),
        selection_attempted: false,
        consumer_contacted: false,
        return_import_attempted: false,
    }
}

fn claims_facts() -> ClaimsFacts {
    ClaimsFacts {
        df_h9_1: "open".into(),
        participant_predicates: "withheld".into(),
        outreach: "none".into(),
        claim_awarded: false,
        round_trip_claim: false,
        reverse_direction_claim: false,
        interop_claim: false,
        compat_claim: false,
        native_authority_claim: false,
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
/// `ForeignFormat` decls read from the landed machinery, the
/// refusal vocabulary exercised, the absent counts and withheld
/// claims folded) — the packet's honest record.
fn run_packet() -> Vec<Leg> {
    let g = gate_facts();
    let pf = pins_facts();
    let v = vocabulary_facts();
    let refusals = exercise_refusals();
    let e = exercise_facts();
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
                ("fixture_sha256", s(&pf.fixture_sha256)),
            ],
        ),
        leg(
            "format_vocabulary",
            evaluate_vocabulary(&v),
            &[
                ("formats", s(v.formats)),
                ("closed_sum", s(&v.closed_sum)),
                ("parse_roundtrip", s(v.parse_roundtrip)),
                ("parse_never_guesses", s(v.parse_never_guesses)),
                ("import_only", s(&v.import_only)),
                ("export_only", s(&v.export_only)),
                ("bidirectional", s(v.bidirectional)),
                ("liftable", s(&v.liftable)),
                ("liftable_is_import_list", s(v.liftable_is_import_list)),
                ("preferred_candidate", s(&v.preferred_candidate)),
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
            "exercise_absent",
            evaluate_exercise(&e),
            &e.counts
                .iter()
                .map(|(k, v)| (k.as_str(), s(v)))
                .chain([
                    ("selection_attempted", s(e.selection_attempted)),
                    ("consumer_contacted", s(e.consumer_contacted)),
                    ("return_import_attempted", s(e.return_import_attempted)),
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
                ("round_trip_claim", s(c.round_trip_claim)),
                ("reverse_direction_claim", s(c.reverse_direction_claim)),
                ("interop_claim", s(c.interop_claim)),
                ("compat_claim", s(c.compat_claim)),
                ("native_authority_claim", s(c.native_authority_claim)),
                ("publication_claim", s(c.publication_claim)),
                ("capability_claim", s(c.capability_claim)),
                ("adoption_claim", s(c.adoption_claim)),
            ],
        ),
    ]
}

// ── the battery ──────────────────────────────────────────────────

#[test]
fn p51_gate_and_pins_verified() {
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
    f.fixture_sha256.clear();
    assert!(matches!(evaluate_pins(&f), LegVerdict::Inconclusive(_)));
}

#[test]
fn p51_format_vocabulary_and_refusals() {
    // The `ForeignFormat` closed sum is the landed decl verbatim:
    // seven canonical spellings each round-tripping `parse`;
    // `harbor_trial_dir` import-only / `interchange_trajectory`
    // export-only / five bidirectional; the six-member liftable
    // list equal to `import::FOREIGN_FORMATS`; `parse` never
    // guesses; the preferred-candidate decl `inspect_eval_log`
    // (S6's "preferably existing Inspect JSON" — declared, never
    // selected).
    let v = vocabulary_facts();
    assert_eq!(v.formats, 7);
    assert_eq!(v.closed_sum, EXPECTED_SUM);
    assert!(v.parse_roundtrip && v.parse_never_guesses);
    assert_eq!(v.import_only, "harbor_trial_dir");
    assert_eq!(v.export_only, "interchange_trajectory");
    assert_eq!(v.bidirectional, 5);
    assert_eq!(v.liftable, EXPECTED_LIFTABLE);
    assert!(v.liftable_is_import_list);
    assert_eq!(v.preferred_candidate, "inspect_eval_log");
    assert_eq!(evaluate_vocabulary(&v), LegVerdict::Verified);
    // Drifted vocabulary refuses; an absent sum is inconclusive,
    // never verified.
    for f in [
        VocabularyFacts {
            formats: 6,
            ..vocabulary_facts()
        },
        VocabularyFacts {
            closed_sum: "harbor_trial_dir".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            parse_never_guesses: false,
            ..vocabulary_facts()
        },
        VocabularyFacts {
            export_only: "swebench_submission".into(),
            ..vocabulary_facts()
        },
        VocabularyFacts {
            bidirectional: 6,
            ..vocabulary_facts()
        },
        VocabularyFacts {
            liftable_is_import_list: false,
            ..vocabulary_facts()
        },
        VocabularyFacts {
            preferred_candidate: "atif".into(),
            ..vocabulary_facts()
        },
    ] {
        assert!(matches!(evaluate_vocabulary(&f), LegVerdict::Refused(_)));
    }
    let mut f = vocabulary_facts();
    f.closed_sum.clear();
    assert!(matches!(
        evaluate_vocabulary(&f),
        LegVerdict::Inconclusive(_)
    ));
    // The landed machinery stands — every exercised case produces
    // its typed refusal verbatim through the real paths; nothing
    // produces (`exported`/`lifted` would be the guard broken).
    let observed = exercise_refusals();
    for (ex, expected) in EXERCISES {
        assert_eq!(observed.get(ex).map(String::as_str), Some(expected), "{ex}");
    }
    assert_eq!(evaluate_refusals(&observed), LegVerdict::Verified);
    // `exported`/`lifted` is the machinery broken; wrong/missing →
    // refused/inconclusive, never verified.
    for (ex, o) in [
        ("interchange_at_import", "lifted"),
        ("harbor_trial_at_export", "exported"),
        ("unknown_spelling_export", "ForeignDirectionRefused"),
    ] {
        let mut bad = observed.clone();
        bad.insert(ex.into(), o.into());
        assert!(matches!(evaluate_refusals(&bad), LegVerdict::Refused(_)));
    }
    let mut missing = observed;
    missing.remove("foreign_at_native_lift");
    assert!(matches!(
        evaluate_refusals(&missing),
        LegVerdict::Inconclusive(_)
    ));
}

#[test]
fn p51_exercise_absent_and_claims() {
    assert_eq!(evaluate_exercise(&exercise_facts()), LegVerdict::Verified);
    assert_eq!(evaluate_claims(&claims_facts()), LegVerdict::Verified);
    // Any nonzero exercise fact is a real exercise's evidence —
    // refused here, never a fabricated absence.
    for member in EXERCISE_MEMBERS {
        let mut e = exercise_facts();
        e.counts.insert(member.into(), 1);
        assert!(matches!(evaluate_exercise(&e), LegVerdict::Refused(_)));
    }
    let mut missing = exercise_facts();
    missing.counts.remove("native_artifacts_exported");
    assert!(matches!(
        evaluate_exercise(&missing),
        LegVerdict::Inconclusive(_)
    ));
    for e in [
        ExerciseFacts {
            selection_attempted: true,
            ..exercise_facts()
        },
        ExerciseFacts {
            consumer_contacted: true,
            ..exercise_facts()
        },
        ExerciseFacts {
            return_import_attempted: true,
            ..exercise_facts()
        },
    ] {
        assert!(matches!(evaluate_exercise(&e), LegVerdict::Refused(_)));
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
            round_trip_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            reverse_direction_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            interop_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            compat_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            native_authority_claim: true,
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
fn p51_packet_unselected_honest() {
    let legs = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble_packet(legs).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.gate, GATE);
    assert!(report.unselected && report.stage_voided && report.df_h9_1_open);
    assert_eq!(report.exercises_completed, 0);
    assert_eq!(report.admission, "unselected_stage_voided");
    assert_eq!(report.disposition, Disposition::Unselected);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\":\"unselected\""));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Emit the unselected record when asked (the committed
    // artifact is produced by this exact path — never
    // hand-written).
    if let Ok(out) = std::env::var("HH_INTERCHANGE_UNSELECTED_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, format!("{j}\n")).unwrap();
        eprintln!("emitted {} disposition=unselected", out.display());
    }
}

#[test]
fn p51_fold_honest_and_legs_closed() {
    let mk = |v: LegVerdict| -> Vec<Leg> { LEGS.iter().map(|n| leg(n, v.clone(), &[])).collect() };
    assert_eq!(
        assemble_packet(mk(LegVerdict::Verified))
            .unwrap()
            .disposition,
        Disposition::Unselected
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
    // A refused leg never folds to unselected.
    let mut legs = run_packet();
    legs[4].verdict = LegVerdict::Refused("exercise_present".into());
    assert_eq!(
        assemble_packet(legs).unwrap().disposition,
        Disposition::Failed
    );
}
