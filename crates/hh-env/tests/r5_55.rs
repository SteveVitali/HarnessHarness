//! R5.55 — `hh-value-withheld/1`: the withheld bounded day-30/+90
//! manual value-review and adoption/contribution follow-up
//! disposition (GATE-G16 PASSED — **out-of-scope disposition** +
//! HUMAN-H11 recorded WITHHELD). The publication stage is voided
//! this round — NOTHING published so NO day-30/+90 review dates
//! exist, NO observation is compiled, and NO contacts or raw
//! observations were ever created (nothing to delete; no
//! earlier-withdrawal evidence exists); the record is a typed
//! withheld/no-observation packet over supplied facts, with
//! absence of adoption recorded `observed_not_achieved` — never
//! hidden success. Machinery is exercised where meaningful
//! without a review act: the eleven pinned inputs byte-verify,
//! the §8.3/§8.4 review/rubric vocabulary is asserted verbatim,
//! the pinned `hh-study-report/1` packet's adverse + unmet +
//! `adoption:unavailable{not_started}` members are carried
//! verbatim through `check_echo` (never inflated), the pinned
//! `hh-publication-withheld/1` packet's members are asserted
//! verbatim and its gate/publication/claims legs re-fold
//! `verified` through the real predecessor evaluators, and the
//! inflation/tamper/spend/nonzero-member/never-guess cases
//! produce their verbatim typed outcomes through the real
//! `evaluate_families`/`check_echo`/`evaluate_costs`/
//! `evaluate_publication`/`AuthorityClass::parse` machinery — the
//! refusals fire before anything is admitted. Fails-without:
//! the `value` module and this battery do not exist at base
//! `b73aab11` (compile-red evidence).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_env::plugin::{evaluate_gate, GateFacts};
use hharness_env::preflight::{check_pins, Leg, LegVerdict, PinnedArtifact};
use hharness_env::publication::{
    evaluate_claims as evaluate_pub_claims, evaluate_humans,
    evaluate_publication as evaluate_pub_absent, ClaimsFacts as PubClaimsFacts, HumanFacts,
    PublicationFacts, EXPECTED_STAGE_ORDER, PUBLICATION_MEMBERS,
};
use hharness_env::study::{
    check_echo, evaluate_costs, evaluate_families, CostFacts, FamilyFacts, PacketEcho,
    LEGS as STUDY_LEGS, REQUIRED_FAMILIES,
};
use hharness_env::value::{
    assemble as assemble_packet, evaluate_carried, evaluate_claims, evaluate_observation,
    evaluate_pins, evaluate_refusals, evaluate_review, CarriedFacts, ClaimsFacts, Disposition,
    ObservationFacts, PinsFacts, ReviewFacts, ADOPTION_ABSENCE, ADOPTION_STATUS, EXERCISES,
    EXPECTED_CEILING, EXPECTED_CHECKPOINTS, EXPECTED_DELETION_RULE, EXPECTED_FOLLOWUP_RULES,
    EXPECTED_HONESTY_RULES, EXPECTED_MEASURES, EXPECTED_REVIEW_OUTCOMES, GATE, LEGS,
    OBSERVATION_MEMBERS, PINS_TOTAL, STUDY_STATUS, VERSION,
};
use hharness_provenance::authority::AuthorityClass;
use hharness_wire::json::{parse as json_parse, Json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ── the eleven pinned inputs (nine H8 artifacts + the committed
// `hh-study-report/1` and `hh-publication-withheld/1` packets —
// frozen dated records: the internal claim→receipt spine whose
// `adoption:unavailable{not_started}` is the observed outcome of
// record, and the dated proof that no publication occurred, the
// causal premise that no day-30/+90 dates derive; the GATE-G16/
// H10/H11 readouts are echoed verbatim as leg facts, never pinned
// — living append-only records, BM-TEST-01). Pins 10–11 target
// the byte-copy fixtures under tests/fixtures/ — the CI readers
// gate bars gated test sources from reading committed
// documentation content; each fixture is byte-identical to the
// canonical report under the build reports dir (same sha256 —
// append-only protected records; corrections land as new dated
// records, never in-place edits) ─────────────────────────────────
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
study-report crates/hh-env/tests/fixtures/r5_54-study-report.json 6e83dcf8ddd6c7fa47f5f4859ed53ed15c372f04eef7e9cbb88b0cc55b09f9f3
publication-withheld crates/hh-env/tests/fixtures/r5_54-publication-withheld.json e7f554141ca6b7c23a7d7e6591aa32b1a4e5fee244901d5f679a5b0b9e205ff7";

const STUDY_PATH: &str = "crates/hh-env/tests/fixtures/r5_54-study-report.json";
const PUB_PATH: &str = "crates/hh-env/tests/fixtures/r5_54-publication-withheld.json";

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
// contract consumed unchanged — plus the H10/H11 withheld records
// through the R5.54 humans contract consumed unchanged) ───────────
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
    let digest = |id: &str| {
        artifact_pins()
            .iter()
            .find(|a| a.artifact_id == id)
            .unwrap()
            .sha256
            .clone()
    };
    PinsFacts {
        pins: artifact_pins().len() as u64,
        pins_verdict: verdict.name().to_string(),
        study_sha256: digest("study-report"),
        publication_sha256: digest("publication-withheld"),
    }
}

// ── leg 3 — review_vocabulary: the §8.3/§8.4 frozen decls (the
// stage order is read off the landed publication decl; the §8.4
// rubric decls are spec text supplied verbatim, never exercised) ──
fn review_facts() -> ReviewFacts {
    ReviewFacts {
        stage_order: EXPECTED_STAGE_ORDER.to_string(),
        measures: "Visibility,Access,Useful task,Understanding/procedure,Independent own-task use,Repeat use,Contribution".into(),
        review_outcomes: "continue,repair,narrow,stop".into(),
        checkpoints: "day30,day90".into(),
        maintainer_ceiling: "total_ceiling_hours:24".into(),
        followup_rules: "at_most_one_optional_request_per_checkpoint,no_chasing".into(),
        deletion_rule: "contacts_raw_observations_deleted:30_days_after_day90_or_earlier_withdrawal_where_possible,receipt_retention_separate".into(),
        honesty_rules: "missing_followup_unknown_not_no_use,no_traffic_fixture_assisted_only_inference_of_adoption,convenience_volunteers_cannot_estimate_population_adoption,suppress_identifying_tiny_strata,consented_nonidentifying_summaries_only".into(),
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

fn pub_json() -> Json {
    json_parse(&std::fs::read_to_string(repo_root().join(PUB_PATH)).unwrap()).unwrap()
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

fn family_facts(efficacy: &str, adoption: &str, claim_present: bool) -> FamilyFacts {
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
        adoption: adoption.to_string(),
        claim_present,
    }
}

fn exercise_refusals() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let required = |n: &str| REQUIRED_FAMILIES.iter().find(|(k, _)| *k == n).unwrap().1;
    // The no-hidden-success gate: an adoption promoted past
    // `unavailable{not_started}` refuses — the recorded outcome
    // stays observed-not-achieved.
    let ff = family_facts(required("efficacy"), "demonstrated", false);
    out.insert(
        "family_adoption_inflated".into(),
        outcome(&evaluate_families(&ff)),
    );
    let ff = family_facts(required("efficacy"), required("adoption"), true);
    out.insert(
        "family_claim_present".into(),
        outcome(&evaluate_families(&ff)),
    );
    // An inflated disposition on the real spine refuses; a
    // tampered leg name refuses.
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
    // A review spend refuses — USD $0.00 authority.
    out.insert(
        "cost_review_spend".into(),
        outcome(&evaluate_costs(&CostFacts {
            per_packet: vec![("value_review".to_string(), 1)],
            subjects_first: 12,
            gated_subjects: 0,
        })),
    );
    // The closed order never guesses a class.
    out.insert(
        "authority_followup_parse".into(),
        if AuthorityClass::parse("followup").is_none() {
            "none".into()
        } else {
            "some".into()
        },
    );
    out.insert(
        "authority_scheduled_parse".into(),
        if AuthorityClass::parse("scheduled").is_none() {
            "none".into()
        } else {
            "some".into()
        },
    );
    // The predecessor's own absence guard stands — a fabricated
    // day-30 review member refuses through the real
    // `publication::evaluate_publication` machinery.
    let mut nonzero = PublicationFacts {
        counts: PUBLICATION_MEMBERS
            .iter()
            .map(|m| (m.to_string(), 0))
            .collect(),
        publication_authorized: false,
        public_act_performed: false,
        telemetry: false,
    };
    nonzero.counts.insert("day30_reviews".into(), 1);
    out.insert(
        "publication_nonzero_member".into(),
        outcome(&evaluate_pub_absent(&nonzero)),
    );
    out
}

// ── leg 5 — predecessors_carried: the pinned packets' adverse +
// unmet + absent members asserted verbatim (the publication
// packet's legs re-folded through the real evaluators) ────────────
fn leg_facts<'a>(j: &'a Json, leg: &str) -> &'a Json {
    arr(member(j, "legs"))
        .iter()
        .find(|l| member(l, "name").as_str() == Some(leg))
        .map(|l| member(l, "facts"))
        .unwrap_or_else(|| panic!("leg {leg} absent"))
}

fn fact<'a>(facts: &'a Json, k: &str) -> &'a str {
    member(facts, k).as_str().unwrap()
}

fn fact_bool(facts: &Json, k: &str) -> bool {
    fact(facts, k) == "true"
}

/// Re-fold the pinned `hh-publication-withheld/1` packet's
/// gate/publication/claims legs through the real predecessor
/// evaluators — the committed record re-computes `verified`,
/// never re-picked.
fn refold_publication() -> String {
    let j = pub_json();
    let g = leg_facts(&j, "gate_readout");
    let gate = GateFacts {
        decision: fact(g, "decision").into(),
        operator_words: fact(g, "operator_words").into(),
        decided_at: fact(g, "decided_at").into(),
        block_sha256_prefix: fact(g, "block_sha256_prefix").into(),
        df_h9_1: fact(g, "df_h9_1").into(),
    };
    let humans = HumanFacts {
        h10_status: fact(g, "h10_status").into(),
        h11_status: fact(g, "h11_status").into(),
        acts_approved: fact(g, "acts_approved").into(),
    };
    let p = leg_facts(&j, "publication_absent");
    let publication = PublicationFacts {
        counts: PUBLICATION_MEMBERS
            .iter()
            .map(|m| (m.to_string(), fact(p, m).parse::<u64>().unwrap()))
            .collect(),
        publication_authorized: fact_bool(p, "publication_authorized"),
        public_act_performed: fact_bool(p, "public_act_performed"),
        telemetry: fact_bool(p, "telemetry"),
    };
    let c = leg_facts(&j, "claims_withheld");
    let claims = PubClaimsFacts {
        df_h9_1: fact(c, "df_h9_1").into(),
        publication_predicates: fact(c, "publication_predicates").into(),
        participant_predicates: fact(c, "participant_predicates").into(),
        spine: fact(c, "spine").into(),
        study_unmet_carried: fact_bool(c, "study_unmet_carried"),
        claim_awarded: fact_bool(c, "claim_awarded"),
        capability_claim: fact_bool(c, "capability_claim"),
        publication_claim: fact_bool(c, "publication_claim"),
        scientific_claim: fact_bool(c, "scientific_claim"),
        adoption_claim: fact_bool(c, "adoption_claim"),
        portability_claim: fact_bool(c, "portability_claim"),
        independence_claim: fact_bool(c, "independence_claim"),
        efficacy_claim: fact_bool(c, "efficacy_claim"),
        promotional_inflation: fact_bool(c, "promotional_inflation"),
        correction_claim: fact_bool(c, "correction_claim"),
        day_review_claim: fact_bool(c, "day_review_claim"),
    };
    for v in [
        evaluate_gate(&gate),
        evaluate_humans(&humans),
        evaluate_pub_absent(&publication),
        evaluate_pub_claims(&claims),
    ] {
        if !v.is_verified() {
            return outcome(&v);
        }
    }
    "verified".into()
}

fn carried_facts() -> CarriedFacts {
    let j = study_json();
    let echo = study_echo();
    let adoption = REQUIRED_FAMILIES
        .iter()
        .find(|(k, _)| *k == "adoption")
        .unwrap()
        .1
        .to_string();
    let flag = |k: &str| matches!(member(&j, k), Json::Bool(true));
    let p = pub_json();
    CarriedFacts {
        study_echo_verdict: check_echo(&echo, &STUDY_LEGS, "reported", "failed")
            .name()
            .to_string(),
        study_status: member(&j, "study").as_str().unwrap().to_string(),
        study_adoption: adoption,
        s1_unmet: flag("s1_unmet"),
        s3_unmet: flag("s3_unmet"),
        s4_unmet: flag("s4_unmet"),
        publication_disposition: member(&p, "disposition").as_str().unwrap().to_string(),
        publication_admission: member(&p, "admission").as_str().unwrap().to_string(),
        publication_gate: member(&p, "gate").as_str().unwrap().to_string(),
        publication_packet_id: member(&p, "packet_id").as_str().unwrap().to_string(),
        publication_withheld: matches!(member(&p, "withheld"), Json::Bool(true)),
        publication_stage_voided: matches!(member(&p, "stage_voided"), Json::Bool(true)),
        publication_no_publication: matches!(member(&p, "no_publication"), Json::Bool(true)),
        publication_df_h9_1_open: matches!(member(&p, "df_h9_1_open"), Json::Bool(true)),
        publication_public_acts: member(&p, "public_acts").as_int().unwrap() as u64,
        publication_refold: refold_publication(),
    }
}

// ── legs 6–7 — observation_absent / claims_withheld ───────────────
fn observation_facts() -> ObservationFacts {
    ObservationFacts {
        counts: OBSERVATION_MEMBERS
            .iter()
            .map(|m| (m.to_string(), 0))
            .collect(),
        review_authorized: false,
        observation_performed: false,
        contact_or_observation_created: false,
        telemetry: false,
    }
}

fn claims_facts() -> ClaimsFacts {
    ClaimsFacts {
        df_h9_1: "open".into(),
        review_predicates: "withheld".into(),
        adoption_absence: ADOPTION_ABSENCE.into(),
        deletion_obligation: "none_arose".into(),
        study_unmet_carried: true,
        claim_awarded: false,
        adoption_claim: false,
        usefulness_claim: false,
        contribution_claim: false,
        capability_claim: false,
        publication_claim: false,
        scientific_claim: false,
        causal_claim: false,
        population_estimate: false,
        promotional_inflation: false,
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
/// review/rubric vocabulary asserted verbatim, the
/// refusal/inflation cases exercised, the pinned predecessor
/// packets carried verbatim — the publication packet's legs
/// re-folded — the absent counts and withheld claims folded) —
/// the packet's honest record.
fn run_packet() -> Vec<Leg> {
    let g = gate_facts();
    let h = human_facts();
    let pf = pins_facts();
    let rv = review_facts();
    let refusals = exercise_refusals();
    let c = carried_facts();
    let o = observation_facts();
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
                ("publication_sha256", s(&pf.publication_sha256)),
            ],
        ),
        leg(
            "review_vocabulary",
            evaluate_review(&rv),
            &[
                ("stage_order", s(&rv.stage_order)),
                ("measures", s(&rv.measures)),
                ("review_outcomes", s(&rv.review_outcomes)),
                ("checkpoints", s(&rv.checkpoints)),
                ("maintainer_ceiling", s(&rv.maintainer_ceiling)),
                ("followup_rules", s(&rv.followup_rules)),
                ("deletion_rule", s(&rv.deletion_rule)),
                ("honesty_rules", s(&rv.honesty_rules)),
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
            "predecessors_carried",
            evaluate_carried(&c),
            &[
                ("study_echo_verdict", s(&c.study_echo_verdict)),
                ("study_status", s(&c.study_status)),
                ("study_adoption", s(&c.study_adoption)),
                ("s1_unmet", s(c.s1_unmet)),
                ("s3_unmet", s(c.s3_unmet)),
                ("s4_unmet", s(c.s4_unmet)),
                ("publication_disposition", s(&c.publication_disposition)),
                ("publication_admission", s(&c.publication_admission)),
                ("publication_gate", s(&c.publication_gate)),
                ("publication_packet_id", s(&c.publication_packet_id)),
                ("publication_withheld", s(c.publication_withheld)),
                ("publication_stage_voided", s(c.publication_stage_voided)),
                (
                    "publication_no_publication",
                    s(c.publication_no_publication),
                ),
                ("publication_df_h9_1_open", s(c.publication_df_h9_1_open)),
                ("publication_public_acts", s(c.publication_public_acts)),
                ("publication_refold", s(&c.publication_refold)),
            ],
        ),
        leg(
            "observation_absent",
            evaluate_observation(&o),
            &o.counts
                .iter()
                .map(|(k, v)| (k.as_str(), s(v)))
                .chain([
                    ("review_authorized", s(o.review_authorized)),
                    ("observation_performed", s(o.observation_performed)),
                    (
                        "contact_or_observation_created",
                        s(o.contact_or_observation_created),
                    ),
                    ("telemetry", s(o.telemetry)),
                ])
                .collect::<Vec<_>>(),
        ),
        leg(
            "claims_withheld",
            evaluate_claims(&cl),
            &[
                ("df_h9_1", s(&cl.df_h9_1)),
                ("review_predicates", s(&cl.review_predicates)),
                ("adoption_absence", s(&cl.adoption_absence)),
                ("deletion_obligation", s(&cl.deletion_obligation)),
                ("study_unmet_carried", s(cl.study_unmet_carried)),
                ("claim_awarded", s(cl.claim_awarded)),
                ("adoption_claim", s(cl.adoption_claim)),
                ("usefulness_claim", s(cl.usefulness_claim)),
                ("contribution_claim", s(cl.contribution_claim)),
                ("capability_claim", s(cl.capability_claim)),
                ("publication_claim", s(cl.publication_claim)),
                ("scientific_claim", s(cl.scientific_claim)),
                ("causal_claim", s(cl.causal_claim)),
                ("population_estimate", s(cl.population_estimate)),
                ("promotional_inflation", s(cl.promotional_inflation)),
                ("day_review_claim", s(cl.day_review_claim)),
            ],
        ),
    ]
}

// ── the battery ──────────────────────────────────────────────────

#[test]
fn p55_gate_and_pins_verified() {
    assert_eq!(evaluate_gate(&gate_facts()), LegVerdict::Verified);
    assert_eq!(evaluate_humans(&human_facts()), LegVerdict::Verified);
    assert_eq!(gate_verdict(), LegVerdict::Verified);
    // A procedures-admitted decision, a missing verbatim member, a
    // performed/approved act or a closed DF-H9-1 refuses — never
    // relabelled.
    for f in [
        GateFacts {
            decision: "PASSED — review admitted".into(),
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
    // The eleven pinned inputs re-verify byte-for-byte; drift
    // refuses, unreadable is inconclusive.
    assert_eq!(check_pins(&artifact_pins()), LegVerdict::Verified);
    assert_eq!(evaluate_pins(&pins_facts()), LegVerdict::Verified);
    assert_eq!(artifact_pins().len() as u64, PINS_TOTAL);
    // The frozen dated reports are parsed and re-echoed — the pins
    // bind the carried members to the committed bytes.
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
            pins: 10,
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
    f.publication_sha256.clear();
    assert!(matches!(evaluate_pins(&f), LegVerdict::Inconclusive(_)));
}

#[test]
fn p55_review_vocabulary_and_refusals() {
    // The §8.3/§8.4 review/rubric vocabulary is the frozen decl
    // verbatim — the rubric stands declared, never exercised.
    let f = review_facts();
    assert_eq!(f.stage_order, EXPECTED_STAGE_ORDER);
    assert!(f.stage_order.ends_with("manual_day30_day90_value_review"));
    assert_eq!(f.measures, EXPECTED_MEASURES);
    assert_eq!(f.review_outcomes, EXPECTED_REVIEW_OUTCOMES);
    assert_eq!(f.checkpoints, EXPECTED_CHECKPOINTS);
    assert_eq!(f.maintainer_ceiling, EXPECTED_CEILING);
    assert_eq!(f.followup_rules, EXPECTED_FOLLOWUP_RULES);
    assert_eq!(f.deletion_rule, EXPECTED_DELETION_RULE);
    assert_eq!(f.honesty_rules, EXPECTED_HONESTY_RULES);
    assert_eq!(evaluate_review(&f), LegVerdict::Verified);
    // A drifted fact is a different vocabulary — refused.
    for mut bad in [
        ReviewFacts {
            stage_order: "publication_first".into(),
            ..f.clone()
        },
        ReviewFacts {
            measures: "Adoption,Publicity".into(),
            ..f.clone()
        },
        ReviewFacts {
            review_outcomes: "continue,promote".into(),
            ..f.clone()
        },
        ReviewFacts {
            deletion_rule: "retain_indefinitely".into(),
            ..f.clone()
        },
    ] {
        assert!(matches!(evaluate_review(&bad), LegVerdict::Refused(_)));
        bad.stage_order.clear();
        assert!(matches!(evaluate_review(&bad), LegVerdict::Inconclusive(_)));
    }
    // Every declared exercise produced its verbatim typed outcome
    // through the real machinery — the inflation/tamper/$0/absence
    // gates hold.
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
    for ex in ["family_adoption_inflated", "publication_nonzero_member"] {
        let mut bad = observed.clone();
        bad.insert(ex.into(), "verified".into());
        assert!(matches!(evaluate_refusals(&bad), LegVerdict::Refused(_)));
    }
    let mut missing = observed.clone();
    missing.remove("cost_review_spend");
    assert!(matches!(
        evaluate_refusals(&missing),
        LegVerdict::Inconclusive(_)
    ));
}

#[test]
fn p55_predecessors_carried_and_absent_claims() {
    // The pinned `hh-study-report/1` packet re-echoes verified and
    // its adverse + unmet + adoption members carry verbatim — the
    // recorded `adoption:unavailable{not_started}` is the observed
    // outcome, never inflated.
    let c = carried_facts();
    assert_eq!(c.study_echo_verdict, "verified");
    assert_eq!(c.study_status, STUDY_STATUS);
    assert_eq!(c.study_adoption, ADOPTION_STATUS);
    assert!(c.s1_unmet && c.s3_unmet && c.s4_unmet);
    // The pinned `hh-publication-withheld/1` packet's members are
    // verbatim and its legs re-fold verified through the real
    // predecessor evaluators — the dated record that no
    // publication occurred.
    assert_eq!(c.publication_disposition, "withheld");
    assert_eq!(c.publication_admission, "withheld_stage_voided");
    assert_eq!(c.publication_gate, GATE);
    assert!(
        c.publication_withheld
            && c.publication_stage_voided
            && c.publication_no_publication
            && c.publication_df_h9_1_open
    );
    assert_eq!(c.publication_public_acts, 0);
    assert_eq!(c.publication_refold, "verified");
    assert_eq!(evaluate_carried(&c), LegVerdict::Verified);
    // A drifted or inflated carried fact — incl. adoption promoted
    // past `unavailable{not_started}` (hidden success) or a
    // publication re-fold that fails — refuses, never relabels.
    for f in [
        CarriedFacts {
            study_echo_verdict: "refused".into(),
            ..c.clone()
        },
        CarriedFacts {
            study_status: "met{efficacy}".into(),
            ..c.clone()
        },
        CarriedFacts {
            study_adoption: "demonstrated".into(),
            ..c.clone()
        },
        CarriedFacts {
            s3_unmet: false,
            ..c.clone()
        },
        CarriedFacts {
            publication_disposition: "executed".into(),
            ..c.clone()
        },
        CarriedFacts {
            publication_no_publication: false,
            ..c.clone()
        },
        CarriedFacts {
            publication_public_acts: 1,
            ..c.clone()
        },
        CarriedFacts {
            publication_refold: "refused".into(),
            ..c.clone()
        },
    ] {
        assert!(matches!(evaluate_carried(&f), LegVerdict::Refused(_)));
    }
    let mut absent = c.clone();
    absent.publication_packet_id.clear();
    assert!(matches!(
        evaluate_carried(&absent),
        LegVerdict::Inconclusive(_)
    ));
    // Every observation member is zero — any nonzero fact refuses.
    assert_eq!(
        evaluate_observation(&observation_facts()),
        LegVerdict::Verified
    );
    let mut nonzero = observation_facts();
    nonzero.counts.insert("day30_review_dates".into(), 1);
    assert!(matches!(
        evaluate_observation(&nonzero),
        LegVerdict::Refused(_)
    ));
    for f in [
        ObservationFacts {
            review_authorized: true,
            ..observation_facts()
        },
        ObservationFacts {
            observation_performed: true,
            ..observation_facts()
        },
        ObservationFacts {
            contact_or_observation_created: true,
            ..observation_facts()
        },
        ObservationFacts {
            telemetry: true,
            ..observation_facts()
        },
    ] {
        assert!(matches!(evaluate_observation(&f), LegVerdict::Refused(_)));
    }
    let mut dropped = observation_facts();
    dropped.counts.remove("deletion_acts");
    assert!(matches!(
        evaluate_observation(&dropped),
        LegVerdict::Inconclusive(_)
    ));
    // DF-H9-1 open, review predicates withheld, adoption absence
    // observed-not-achieved, deletion obligation none_arose, the
    // unmet record carried — no claim of any sign.
    assert_eq!(evaluate_claims(&claims_facts()), LegVerdict::Verified);
    for f in [
        ClaimsFacts {
            df_h9_1: "closed".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            review_predicates: "met".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            adoption_absence: "achieved".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            deletion_obligation: "performed".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            study_unmet_carried: false,
            ..claims_facts()
        },
        ClaimsFacts {
            adoption_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            day_review_claim: true,
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
fn p55_packet_withheld_honest() {
    let legs = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble_packet(legs).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.gate, GATE);
    assert!(report.withheld && report.stage_voided);
    assert!(report.no_publication && report.no_observation);
    assert!(report.df_h9_1_open);
    assert_eq!(report.reviews, 0);
    assert_eq!(report.observations, 0);
    assert_eq!(report.study, STUDY_STATUS);
    assert_eq!(report.adoption_absence, ADOPTION_ABSENCE);
    assert_eq!(report.admission, "withheld_stage_voided");
    assert_eq!(report.disposition, Disposition::Withheld);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\":\"withheld\""));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Emit the withheld record when asked (the committed artifact
    // is produced by this exact path — never hand-written).
    if let Ok(out) = std::env::var("HH_VALUE_WITHHELD_EMIT") {
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
fn p55_fold_honest_and_legs_closed() {
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
    legs[5].verdict = LegVerdict::Refused("observation_present".into());
    assert_eq!(
        assemble_packet(legs).unwrap().disposition,
        Disposition::Failed
    );
}
