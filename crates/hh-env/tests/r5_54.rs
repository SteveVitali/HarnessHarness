//! R5.54 — `hh-publication-withheld/1`: the withheld public
//! availability/claims/corrections disposition (GATE-G16 PASSED —
//! **out-of-scope disposition**). The publication stage is voided
//! this round — NO public acts/contacts/content were authorized
//! (HUMAN-H10/H11 WITHHELD) and nothing published; the record is a
//! typed withheld/no-publication packet over supplied facts.
//! Machinery is exercised where meaningful without a public act:
//! the ten pinned inputs byte-verify, the §8.3 spine/claim/
//! correction vocabulary is asserted verbatim (study decls, rights
//! schemas, `AuthorityClass`, frozen §8.3 decls), the committed
//! `hh-study-report/1` packet's adverse + unmet members are carried
//! verbatim through `check_echo` (never inflated), and the
//! inflation/tamper/relabel/spend/never-guess cases produce their
//! verbatim typed outcomes through the real `evaluate_*`/`check_echo`/
//! `AuthorityClass::parse` machinery — the refusals fire before
//! anything is admitted. Fails-without: the `publication` module and
//! this battery do not exist at base `676595f3` (compile-red
//! evidence).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_bundle::rights::{
    HANDOFF_REPORT_SCHEMA, RIGHTS_RECORD_SCHEMA, SUPERSESSION_SCHEMA, TOMBSTONE_SCHEMA,
};
use hharness_env::plugin::{evaluate_gate, GateFacts};
use hharness_env::preflight::{check_pins, Leg, LegVerdict, PinnedArtifact};
use hharness_env::publication::{
    assemble as assemble_packet, evaluate_carried, evaluate_claims, evaluate_humans, evaluate_pins,
    evaluate_publication, evaluate_refusals, evaluate_spine, CarriedFacts, ClaimsFacts,
    Disposition, HumanFacts, PinsFacts, PublicationFacts, SpineFacts, EXERCISES,
    EXPECTED_AUTHORITY_CLASSES, EXPECTED_CHANNEL_RULES, EXPECTED_CORRECTION_SCHEMAS,
    EXPECTED_RELEASE_NOTE_FIELDS, EXPECTED_REPORT_SPINE_FIELDS, EXPECTED_STAGE_ORDER,
    EXPECTED_STUDY_DISPOSITIONS, EXPECTED_STUDY_FAMILIES, EXPECTED_STUDY_LEGS,
    EXPECTED_STUDY_VERSION, GATE, LEGS, PINS_TOTAL, PUBLICATION_MEMBERS, STUDY_STATUS, VERSION,
};
use hharness_env::study::{
    check_echo, evaluate_costs, evaluate_families, evaluate_parity_stage, evaluate_useful,
    CostFacts, Disposition as StudyDisposition, FamilyFacts, PacketEcho, ParityStageFacts,
    UsefulFacts, LEGS as STUDY_LEGS, REQUIRED_FAMILIES, VERSION as STUDY_VERSION,
};
use hharness_env::{parity, useful};
use hharness_provenance::authority::AuthorityClass;
use hharness_wire::json::{parse as json_parse, Json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ── the ten pinned inputs (nine H8 artifacts + the committed
// `hh-study-report/1` packet — a frozen dated report, the
// claim→receipt spine that stays internal, carried verbatim; the
// GATE-G16/H10/H11 readouts are echoed verbatim as leg facts,
// never pinned — living append-only records, BM-TEST-01). Pin 10
// targets the byte-copy fixture under tests/fixtures/ — the CI
// readers gate bars gated test sources from reading committed
// documentation/research content; the fixture is byte-identical
// to the canonical report under the build reports dir (same
// sha256 `6e83dcf8…`, an append-only protected record —
// corrections land as new dated records, never in-place edits) ──
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
study-report crates/hh-env/tests/fixtures/r5_54-study-report.json 6e83dcf8ddd6c7fa47f5f4859ed53ed15c372f04eef7e9cbb88b0cc55b09f9f3";

const STUDY_PATH: &str = "crates/hh-env/tests/fixtures/r5_54-study-report.json";

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

// ── leg 1 — gate_readout (GATE-G16 verbatim — the R5.50 gate
// contract consumed unchanged — plus the H10/H11 withheld
// records) ──────────────────────────────────────────────────────
fn gate_facts() -> GateFacts {
    GateFacts {
        decision: "PASSED — out-of-scope disposition".into(),
        operator_words: "confirm and proceed".into(),
        decided_at: "2026-11-04T18:53:32Z".into(),
        block_sha256_prefix: "1b88c0ef16ba".into(),
        df_h9_1: "open".into(),
    }
}

fn human_facts() -> HumanFacts {
    HumanFacts {
        h10_status: "withheld".into(),
        h11_status: "withheld".into(),
        acts_approved: "none".into(),
    }
}

fn gate_verdict() -> LegVerdict {
    match evaluate_gate(&gate_facts()) {
        LegVerdict::Verified => evaluate_humans(&human_facts()),
        v => v,
    }
}

// ── leg 2 — pins_frozen ──────────────────────────────────────────
fn pins_facts() -> PinsFacts {
    let verdict = check_pins(&artifact_pins());
    PinsFacts {
        pins: artifact_pins().len() as u64,
        pins_verdict: verdict.name().to_string(),
        study_sha256: artifact_pins()
            .iter()
            .find(|a| a.artifact_id == "study-report")
            .unwrap()
            .sha256
            .clone(),
    }
}

// ── leg 3 — spine_vocabulary: the §8.3 decls read from the landed
// machinery (facts supplied verbatim, never re-declared) ──────────
fn spine_facts() -> SpineFacts {
    let study_legs = STUDY_LEGS.join(",");
    let study_dispositions = [
        StudyDisposition::Reported,
        StudyDisposition::Failed,
        StudyDisposition::Inconclusive,
    ]
    .iter()
    .map(|d| d.name())
    .collect::<Vec<_>>()
    .join(",");
    let study_families = REQUIRED_FAMILIES
        .iter()
        .map(|(n, s)| format!("{n}:{s}"))
        .collect::<Vec<_>>()
        .join(",");
    let correction_schemas = [
        RIGHTS_RECORD_SCHEMA,
        HANDOFF_REPORT_SCHEMA,
        SUPERSESSION_SCHEMA,
        TOMBSTONE_SCHEMA,
    ]
    .join(",");
    let authority_classes = AuthorityClass::ALL
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(",");
    // The closed order never guesses — a non-class spelling parses
    // to `None` (`imported` is an Origin, never a class — CF-082).
    let authority_parse_never_guesses = [
        "promotional",
        "imported",
        "public",
        "staging-verified",
        "live-executed",
        "verified",
        "",
    ]
    .iter()
    .all(|s| AuthorityClass::parse(s).is_none());
    SpineFacts {
        study_version: STUDY_VERSION.to_string(),
        study_legs,
        study_dispositions,
        study_families,
        correction_schemas,
        authority_classes,
        authority_parse_never_guesses,
        stage_order: "internal_readiness,outside_clean_verify_recompute,formative_reviews_and_repair_recheck,operator_capability_publication_decision,separately_gated_scientific_report,manual_day30_day90_value_review".into(),
        release_note_fields: "audience,job,change_revision,try_inspect_prerequisites,earned_unavailable_claims,evidence,report_contribute_route,correction_owner,correction_date".into(),
        report_spine_fields: "question_decision_scope_class_date,claim_to_receipt,protocol_deviations_population_arms_assignment_oracle_pins_caps_admission,planned_attempt_grade_analysis_counts_and_adverse_states,effects_uncertainty_margins_families_cost_or_operational_verdicts,procedure_cards_outside_reports_assistance_access_losses,limitations_unsupported_conclusions_corrections_rights_retention".into(),
        channel_rules: "github_first,at_most_one_wider_channel_after_current_rule_review_and_operator_approval,no_report_only_launch,no_scraped_lists_or_bulk_mail,no_vote_solicitation,no_promotional_issues_or_implied_endorsement,upstream_engagement_needs_concrete_failing_fixture_and_separate_authorization,no_outreach_or_automation_authorized".into(),
    }
}

// ── leg 4 — refusal_vocabulary: exercised cases through the real
// machinery producing verbatim typed outcomes ─────────────────────
fn outcome(v: &LegVerdict) -> String {
    match v {
        LegVerdict::Verified => "verified".into(),
        _ => v.detail().unwrap_or_else(|| v.name()).to_string(),
    }
}

fn study_json() -> Json {
    json_parse(&std::fs::read_to_string(repo_root().join(STUDY_PATH)).unwrap()).unwrap()
}

fn member<'a>(j: &'a Json, k: &str) -> &'a Json {
    match j {
        Json::Obj(m) => &m[k],
        _ => panic!("not an object"),
    }
}

fn arr(j: &Json) -> &[Json] {
    match j {
        Json::Arr(v) => v,
        _ => panic!("not an array"),
    }
}

/// The pinned `hh-study-report/1` packet parsed to a `PacketEcho`
/// (records-in — the committed record is read, never re-picked).
fn study_echo() -> PacketEcho {
    let j = study_json();
    let mut legs = Vec::new();
    let mut details = Vec::new();
    for l in arr(member(&j, "legs")) {
        let name = member(l, "name").as_str().unwrap().to_string();
        let v = member(l, "verdict");
        let verdict = member(v, "verdict").as_str().unwrap().to_string();
        if let Json::Obj(o) = v {
            if let Some(d) = o.get("detail").and_then(Json::as_str) {
                details.push((name.clone(), d.to_string()));
            }
        }
        legs.push((name, verdict));
    }
    PacketEcho {
        version: member(&j, "version").as_str().unwrap().to_string(),
        disposition: member(&j, "disposition").as_str().unwrap().to_string(),
        admission: member(&j, "admission").as_str().unwrap().to_string(),
        packet_id: member(&j, "packet_id").as_str().unwrap().to_string(),
        legs,
        details,
    }
}

fn family_facts(efficacy: &str, claim_present: bool) -> FamilyFacts {
    let get = |n: &str| {
        REQUIRED_FAMILIES
            .iter()
            .find(|(k, _)| *k == n)
            .unwrap()
            .1
            .to_string()
    };
    FamilyFacts {
        efficacy: efficacy.to_string(),
        task_mixture: get("task_mixture"),
        sharp_null: get("sharp_null"),
        parity: get("parity"),
        mechanism: get("mechanism"),
        engineering: get("engineering"),
        task_truth: get("task_truth"),
        operations: get("operations"),
        adoption: get("adoption"),
        claim_present,
    }
}

fn all_verified_echo(version: &str, disposition: &str, vocab: &[&str]) -> PacketEcho {
    PacketEcho {
        version: version.to_string(),
        disposition: disposition.to_string(),
        admission: "x".to_string(),
        packet_id: "0".repeat(64),
        legs: vocab
            .iter()
            .map(|n| (n.to_string(), "verified".to_string()))
            .collect(),
        details: vec![],
    }
}

fn exercise_refusals() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    // The no-promotional-inflation gate: an inflated family refuses.
    let ff = family_facts("demonstrated", false);
    out.insert(
        "family_efficacy_inflated".into(),
        outcome(&evaluate_families(&ff)),
    );
    let required_efficacy = REQUIRED_FAMILIES
        .iter()
        .find(|(k, _)| *k == "efficacy")
        .unwrap()
        .1;
    let ff = family_facts(required_efficacy, true);
    out.insert(
        "family_claim_present".into(),
        outcome(&evaluate_families(&ff)),
    );
    // An inflated disposition on the real spine refuses; a tampered
    // leg name refuses.
    let mut inflated = study_echo();
    inflated.disposition = "satisfied".into();
    out.insert(
        "echo_disposition_inflated".into(),
        outcome(&check_echo(&inflated, &STUDY_LEGS, "reported", "failed")),
    );
    let mut tampered = study_echo();
    for (n, _) in tampered.legs.iter_mut() {
        if n == "families" {
            *n = "families_inflated".into();
        }
    }
    out.insert(
        "echo_leg_tampered".into(),
        outcome(&check_echo(&tampered, &STUDY_LEGS, "reported", "failed")),
    );
    // A relabeled or re-signed stage record refuses — the adverse
    // records carry their committed spellings only.
    out.insert(
        "useful_version_relabel".into(),
        outcome(&evaluate_useful(&UsefulFacts {
            echo: PacketEcho {
                version: "hh-study-report/1".into(),
                ..all_verified_echo(useful::VERSION, "failed", &useful::LEGS)
            },
            acceptance: "withheld{no_independent_correct_artifact}".into(),
            carried: vec![
                useful::PROBE_CARRIED.to_string(),
                useful::EXEC_BACKEND_CLOBBER.to_string(),
            ],
            s1_unmet: true,
        })),
    );
    out.insert(
        "useful_executed_relabel".into(),
        outcome(&evaluate_useful(&UsefulFacts {
            echo: all_verified_echo(useful::VERSION, "executed", &useful::LEGS),
            acceptance: "withheld{no_independent_correct_artifact}".into(),
            carried: vec![
                useful::PROBE_CARRIED.to_string(),
                useful::EXEC_BACKEND_CLOBBER.to_string(),
            ],
            s1_unmet: true,
        })),
    );
    out.insert(
        "parity_gate_resign".into(),
        outcome(&evaluate_parity_stage(&ParityStageFacts {
            echo: all_verified_echo(parity::VERSION, "withheld", &parity::LEGS),
            n_declared: 0,
            s3_unmet: true,
            confirmation_blocked: true,
            gate: "GATE-G16".into(),
        })),
    );
    // A publication spend refuses — USD $0.00 authority.
    out.insert(
        "cost_publication_spend".into(),
        outcome(&evaluate_costs(&CostFacts {
            per_packet: vec![("publication".to_string(), 1)],
            subjects_first: 12,
            gated_subjects: 0,
        })),
    );
    // The closed order never guesses a class.
    out.insert(
        "authority_promotional_parse".into(),
        if AuthorityClass::parse("promotional").is_none() {
            "none".into()
        } else {
            "some".into()
        },
    );
    out.insert(
        "authority_imported_parse".into(),
        if AuthorityClass::parse("imported").is_none() {
            "none".into()
        } else {
            "some".into()
        },
    );
    out
}

// ── leg 5 — study_carried: the pinned report's adverse + unmet
// members asserted verbatim through check_echo ────────────────────
fn carried_facts() -> CarriedFacts {
    let j = study_json();
    let echo = study_echo();
    let leg_fact = |leg: &str, k: &str| -> String {
        arr(member(&j, "legs"))
            .iter()
            .find(|l| member(l, "name").as_str() == Some(leg))
            .map(|l| member(&member(l, "facts"), k).as_str().unwrap().to_string())
            .unwrap()
    };
    let flag = |k: &str| matches!(member(&j, k), Json::Bool(true));
    CarriedFacts {
        echo_verdict: check_echo(&echo, &STUDY_LEGS, "reported", "failed")
            .name()
            .to_string(),
        version: echo.version.clone(),
        disposition: echo.disposition.clone(),
        admission: echo.admission.clone(),
        packet_id: echo.packet_id.clone(),
        study_status: member(&j, "study").as_str().unwrap().to_string(),
        s1_unmet: flag("s1_unmet"),
        s3_unmet: flag("s3_unmet"),
        s4_unmet: flag("s4_unmet"),
        subjects_first: member(&j, "subjects_first").as_int().unwrap() as u64,
        exposure_micros: member(&j, "exposure_micros").as_int().unwrap(),
        s1_disposition: leg_fact("useful_s1", "disposition"),
        s1_acceptance: leg_fact("useful_s1", "acceptance"),
        parity_status: leg_fact("parity", "status"),
        confirmation_status: leg_fact("confirmation", "status"),
    }
}

// ── legs 6–7 — publication_absent / claims_withheld ───────────────
fn publication_facts() -> PublicationFacts {
    PublicationFacts {
        counts: PUBLICATION_MEMBERS
            .iter()
            .map(|m| (m.to_string(), 0))
            .collect(),
        publication_authorized: false,
        public_act_performed: false,
        telemetry: false,
    }
}

fn claims_facts() -> ClaimsFacts {
    ClaimsFacts {
        df_h9_1: "open".into(),
        publication_predicates: "withheld".into(),
        participant_predicates: "withheld".into(),
        spine: "internal".into(),
        study_unmet_carried: true,
        claim_awarded: false,
        capability_claim: false,
        publication_claim: false,
        scientific_claim: false,
        adoption_claim: false,
        portability_claim: false,
        independence_claim: false,
        efficacy_claim: false,
        promotional_inflation: false,
        correction_claim: false,
        day_review_claim: false,
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

/// The seven legs driven on real inputs (pins recomputed, the
/// spine vocabulary read from the landed machinery, the
/// refusal/inflation cases exercised, the pinned study report
/// carried verbatim, the absent counts and withheld claims
/// folded) — the packet's honest record.
fn run_packet() -> Vec<Leg> {
    let g = gate_facts();
    let h = human_facts();
    let pf = pins_facts();
    let sp = spine_facts();
    let refusals = exercise_refusals();
    let c = carried_facts();
    let p = publication_facts();
    let cl = claims_facts();
    vec![
        leg(
            "gate_readout",
            gate_verdict(),
            &[
                ("decision", s(&g.decision)),
                ("operator_words", s(&g.operator_words)),
                ("decided_at", s(&g.decided_at)),
                ("block_sha256_prefix", s(&g.block_sha256_prefix)),
                ("df_h9_1", s(&g.df_h9_1)),
                ("h10_status", s(&h.h10_status)),
                ("h11_status", s(&h.h11_status)),
                ("acts_approved", s(&h.acts_approved)),
            ],
        ),
        leg(
            "pins_frozen",
            evaluate_pins(&pf),
            &[
                ("pins", s(pf.pins)),
                ("pins_verdict", s(&pf.pins_verdict)),
                ("study_sha256", s(&pf.study_sha256)),
            ],
        ),
        leg(
            "spine_vocabulary",
            evaluate_spine(&sp),
            &[
                ("study_version", s(&sp.study_version)),
                ("study_legs", s(&sp.study_legs)),
                ("study_dispositions", s(&sp.study_dispositions)),
                ("study_families", s(&sp.study_families)),
                ("correction_schemas", s(&sp.correction_schemas)),
                ("authority_classes", s(&sp.authority_classes)),
                (
                    "authority_parse_never_guesses",
                    s(sp.authority_parse_never_guesses),
                ),
                ("stage_order", s(&sp.stage_order)),
                ("release_note_fields", s(&sp.release_note_fields)),
                ("report_spine_fields", s(&sp.report_spine_fields)),
                ("channel_rules", s(&sp.channel_rules)),
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
            "study_carried",
            evaluate_carried(&c),
            &[
                ("echo_verdict", s(&c.echo_verdict)),
                ("version", s(&c.version)),
                ("disposition", s(&c.disposition)),
                ("admission", s(&c.admission)),
                ("packet_id", s(&c.packet_id)),
                ("study_status", s(&c.study_status)),
                ("s1_unmet", s(c.s1_unmet)),
                ("s3_unmet", s(c.s3_unmet)),
                ("s4_unmet", s(c.s4_unmet)),
                ("subjects_first", s(c.subjects_first)),
                ("exposure_micros", s(c.exposure_micros)),
                ("s1_disposition", s(&c.s1_disposition)),
                ("s1_acceptance", s(&c.s1_acceptance)),
                ("parity_status", s(&c.parity_status)),
                ("confirmation_status", s(&c.confirmation_status)),
            ],
        ),
        leg(
            "publication_absent",
            evaluate_publication(&p),
            &p.counts
                .iter()
                .map(|(k, v)| (k.as_str(), s(v)))
                .chain([
                    ("publication_authorized", s(p.publication_authorized)),
                    ("public_act_performed", s(p.public_act_performed)),
                    ("telemetry", s(p.telemetry)),
                ])
                .collect::<Vec<_>>(),
        ),
        leg(
            "claims_withheld",
            evaluate_claims(&cl),
            &[
                ("df_h9_1", s(&cl.df_h9_1)),
                ("publication_predicates", s(&cl.publication_predicates)),
                ("participant_predicates", s(&cl.participant_predicates)),
                ("spine", s(&cl.spine)),
                ("study_unmet_carried", s(cl.study_unmet_carried)),
                ("claim_awarded", s(cl.claim_awarded)),
                ("capability_claim", s(cl.capability_claim)),
                ("publication_claim", s(cl.publication_claim)),
                ("scientific_claim", s(cl.scientific_claim)),
                ("adoption_claim", s(cl.adoption_claim)),
                ("portability_claim", s(cl.portability_claim)),
                ("independence_claim", s(cl.independence_claim)),
                ("efficacy_claim", s(cl.efficacy_claim)),
                ("promotional_inflation", s(cl.promotional_inflation)),
                ("correction_claim", s(cl.correction_claim)),
                ("day_review_claim", s(cl.day_review_claim)),
            ],
        ),
    ]
}

// ── the battery ──────────────────────────────────────────────────

#[test]
fn p54_gate_and_pins_verified() {
    assert_eq!(evaluate_gate(&gate_facts()), LegVerdict::Verified);
    assert_eq!(evaluate_humans(&human_facts()), LegVerdict::Verified);
    assert_eq!(gate_verdict(), LegVerdict::Verified);
    // A procedures-admitted decision, a missing verbatim member, a
    // performed/approved act or a closed DF-H9-1 refuses — never
    // relabelled.
    for f in [
        GateFacts {
            decision: "PASSED — public acts admitted".into(),
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
    for f in [
        HumanFacts {
            h10_status: "performed".into(),
            ..human_facts()
        },
        HumanFacts {
            h11_status: "scheduled".into(),
            ..human_facts()
        },
        HumanFacts {
            acts_approved: "some".into(),
            ..human_facts()
        },
    ] {
        assert!(matches!(evaluate_humans(&f), LegVerdict::Refused(_)));
    }
    // The ten pinned inputs re-verify byte-for-byte; drift
    // refuses, unreadable is inconclusive.
    assert_eq!(check_pins(&artifact_pins()), LegVerdict::Verified);
    assert_eq!(evaluate_pins(&pins_facts()), LegVerdict::Verified);
    assert_eq!(artifact_pins().len() as u64, PINS_TOTAL);
    // The frozen dated report is parsed and re-echoed — the pin
    // binds the carried members to the committed bytes.
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
    f.study_sha256.clear();
    assert!(matches!(evaluate_pins(&f), LegVerdict::Inconclusive(_)));
}

#[test]
fn p54_spine_vocabulary_and_refusals() {
    // The §8.3 spine/claim/correction vocabulary is the landed
    // decl verbatim — the machinery is read, never re-declared.
    let f = spine_facts();
    assert_eq!(f.study_version, EXPECTED_STUDY_VERSION);
    assert_eq!(f.study_legs, EXPECTED_STUDY_LEGS);
    assert_eq!(f.study_dispositions, EXPECTED_STUDY_DISPOSITIONS);
    assert_eq!(f.study_families, EXPECTED_STUDY_FAMILIES);
    assert_eq!(f.correction_schemas, EXPECTED_CORRECTION_SCHEMAS);
    assert_eq!(f.authority_classes, EXPECTED_AUTHORITY_CLASSES);
    assert_eq!(f.stage_order, EXPECTED_STAGE_ORDER);
    assert_eq!(f.release_note_fields, EXPECTED_RELEASE_NOTE_FIELDS);
    assert_eq!(f.report_spine_fields, EXPECTED_REPORT_SPINE_FIELDS);
    assert_eq!(f.channel_rules, EXPECTED_CHANNEL_RULES);
    assert_eq!(evaluate_spine(&f), LegVerdict::Verified);
    // A drifted fact is a different vocabulary — refused.
    for mut bad in [
        SpineFacts {
            study_version: "hh-study-report/2".into(),
            ..f.clone()
        },
        SpineFacts {
            study_families: "efficacy:demonstrated".into(),
            ..f.clone()
        },
        SpineFacts {
            channel_rules: "any_channel".into(),
            ..f.clone()
        },
        SpineFacts {
            authority_parse_never_guesses: false,
            ..f.clone()
        },
    ] {
        assert!(matches!(evaluate_spine(&bad), LegVerdict::Refused(_)));
        bad.study_version.clear();
        assert!(matches!(evaluate_spine(&bad), LegVerdict::Inconclusive(_)));
    }
    // Every declared exercise produced its verbatim typed outcome
    // through the real machinery — the inflation/tamper gates hold.
    let observed = exercise_refusals();
    for (exercise, expected) in EXERCISES {
        assert_eq!(
            observed.get(exercise).map(String::as_str),
            Some(expected),
            "{exercise}"
        );
    }
    assert_eq!(evaluate_refusals(&observed), LegVerdict::Verified);
    // A produced outcome is a broken guard — refused.
    for ex in ["family_efficacy_inflated", "parity_gate_resign"] {
        let mut bad = observed.clone();
        bad.insert(ex.into(), "verified".into());
        assert!(matches!(evaluate_refusals(&bad), LegVerdict::Refused(_)));
    }
    let mut missing = observed.clone();
    missing.remove("cost_publication_spend");
    assert!(matches!(
        evaluate_refusals(&missing),
        LegVerdict::Inconclusive(_)
    ));
}

#[test]
fn p54_study_carried_and_absent_claims() {
    // The pinned `hh-study-report/1` packet re-echoes verified and
    // its adverse + unmet members carry verbatim — never inflated.
    let c = carried_facts();
    assert_eq!(c.echo_verdict, "verified");
    assert_eq!(c.study_status, STUDY_STATUS);
    assert!(c.s1_unmet && c.s3_unmet && c.s4_unmet);
    assert_eq!(c.s1_disposition, "failed");
    assert_eq!(c.s1_acceptance, "withheld{no_independent_correct_artifact}");
    assert_eq!(c.parity_status, "withheld{n_declared:0}");
    assert_eq!(c.confirmation_status, "withheld{tranche_absent}");
    assert_eq!(evaluate_carried(&c), LegVerdict::Verified);
    // A drifted or inflated carried fact refuses — the adverse
    // record never promotes.
    for f in [
        CarriedFacts {
            echo_verdict: "refused".into(),
            ..c.clone()
        },
        CarriedFacts {
            study_status: "met{efficacy}".into(),
            ..c.clone()
        },
        CarriedFacts {
            s1_disposition: "executed".into(),
            ..c.clone()
        },
        CarriedFacts {
            s3_unmet: false,
            ..c.clone()
        },
        CarriedFacts {
            exposure_micros: 1,
            ..c.clone()
        },
    ] {
        assert!(matches!(evaluate_carried(&f), LegVerdict::Refused(_)));
    }
    // Every publication member is zero — any nonzero fact refuses.
    assert_eq!(
        evaluate_publication(&publication_facts()),
        LegVerdict::Verified
    );
    let mut nonzero = publication_facts();
    nonzero.counts.insert("urls_minted".into(), 1);
    assert!(matches!(
        evaluate_publication(&nonzero),
        LegVerdict::Refused(_)
    ));
    for f in [
        PublicationFacts {
            publication_authorized: true,
            ..publication_facts()
        },
        PublicationFacts {
            public_act_performed: true,
            ..publication_facts()
        },
        PublicationFacts {
            telemetry: true,
            ..publication_facts()
        },
    ] {
        assert!(matches!(evaluate_publication(&f), LegVerdict::Refused(_)));
    }
    let mut absent = publication_facts();
    absent.counts.remove("day90_reviews");
    assert!(matches!(
        evaluate_publication(&absent),
        LegVerdict::Inconclusive(_)
    ));
    // DF-H9-1 open, spine internal, unmet carried, no claim.
    assert_eq!(evaluate_claims(&claims_facts()), LegVerdict::Verified);
    for f in [
        ClaimsFacts {
            df_h9_1: "closed".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            spine: "public".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            study_unmet_carried: false,
            ..claims_facts()
        },
        ClaimsFacts {
            publication_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            claim_awarded: true,
            ..claims_facts()
        },
    ] {
        assert!(matches!(evaluate_claims(&f), LegVerdict::Refused(_)));
    }
}

#[test]
fn p54_packet_withheld_honest() {
    let legs = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble_packet(legs).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.gate, GATE);
    assert!(report.withheld && report.stage_voided && report.no_publication);
    assert!(report.df_h9_1_open);
    assert_eq!(report.public_acts, 0);
    assert_eq!(report.study, STUDY_STATUS);
    assert_eq!(report.admission, "withheld_stage_voided");
    assert_eq!(report.disposition, Disposition::Withheld);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\":\"withheld\""));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Emit the withheld record when asked (the committed artifact
    // is produced by this exact path — never hand-written).
    if let Ok(out) = std::env::var("HH_PUBLICATION_WITHHELD_EMIT") {
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
fn p54_fold_honest_and_legs_closed() {
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
    assert!(assemble_packet(mk(LegVerdict::Verified)[..6].to_vec()).is_err());
    let mut bad = mk(LegVerdict::Verified);
    bad[0].name = "not_a_leg".into();
    assert!(assemble_packet(bad).is_err());
    // A refused leg never folds to withheld.
    let mut legs = run_packet();
    legs[5].verdict = LegVerdict::Refused("publication_present".into());
    assert_eq!(
        assemble_packet(legs).unwrap().disposition,
        Disposition::Failed
    );
}
