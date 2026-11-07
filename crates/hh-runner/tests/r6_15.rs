//! R6.15 (AR-05; REQ-XB-129) — the relay-death classification fixtures
//! and the relay digest carried into the slot identity.

use hharness_ontology::control::OutcomeClass;
use hharness_runner::disposition::{Disposition, InfraCause, SlotFlag};
use hharness_runner::facts::{map_slot_facts, Resolution, RewardFact, SlotFacts};
use hharness_runner::slot_record::ExperimentSlot;
use hharness_wire::json::{self, Json};

#[test]
fn relay_death_classifications() {
    // Agent-issued signal to PID 1 → scored executed{crashed} + flag.
    let f = SlotFacts {
        relay_killed_by_agent: true,
        reward: Some(RewardFact {
            resolution: Resolution::Unresolved,
            end_cause: hharness_runner::disposition::EndCause::Crashed,
            reward_ppm: Some(0),
            flags: vec![SlotFlag::RelayKilledByAgent],
        }),
        ..Default::default()
    };
    let m = map_slot_facts(&f);
    assert_eq!(m.outcome_class, OutcomeClass::Scored);
    assert_eq!(m.disposition.as_str(), "executed{unresolved}");
    assert!(matches!(
        m.disposition,
        Disposition::Executed {
            end_cause: hharness_runner::disposition::EndCause::Crashed,
            ..
        }
    ));
    assert!(f
        .reward
        .unwrap()
        .flags
        .contains(&SlotFlag::RelayKilledByAgent));
    // Injected relay crash (no agent signal) → infrastructure_failure.
    let f = SlotFacts {
        infra: Some(InfraCause::RelayDied),
        ..Default::default()
    };
    let m = map_slot_facts(&f);
    assert_eq!(m.outcome_class, OutcomeClass::InfrastructureFailure);
    assert!(m.replans);
    assert_eq!(m.disposition.as_str(), "inconclusive{infra:relay_died}");
}

#[test]
fn relay_digest_lands_in_identity_infrastructure() {
    // Build a minimal valid record JSON with the infrastructure member,
    // then decode — the relay digest carries verbatim.
    let mut slot: ExperimentSlot = {
        let base = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/r6_8_slot.json"
        ))
        .ok()
        .and_then(|t| json::parse(&t).ok())
        .unwrap_or_else(|| minimal_slot_json());
        ExperimentSlot::from_json(&base).unwrap()
    };
    let infra = Json::obj([
        (
            "runner",
            Json::obj([("binary_sha256", Json::str("sha256:aa"))]),
        ),
        (
            "relay",
            Json::obj([("binary_sha256", Json::str("sha256:bb"))]),
        ),
    ]);
    let mut id = match &slot.identity {
        Json::Obj(m) => m.clone(),
        _ => panic!("identity not obj"),
    };
    id.insert("infrastructure".to_string(), infra);
    slot.identity = Json::Obj(id);
    let j = slot.to_json();
    assert_eq!(
        j.get("identity")
            .unwrap()
            .get("infrastructure")
            .unwrap()
            .get("relay")
            .unwrap()
            .get("binary_sha256")
            .and_then(Json::as_str),
        Some("sha256:bb")
    );
    ExperimentSlot::from_json(&j).unwrap();
    // A malformed member refuses (strict decode).
    let mut bad = j.clone();
    let bm = match &mut bad {
        Json::Obj(m) => match m.get_mut("identity").unwrap() {
            Json::Obj(im) => im.get_mut("infrastructure").unwrap(),
            _ => panic!("identity not obj"),
        },
        _ => panic!("record not obj"),
    };
    *bm = Json::obj([("bogus", Json::obj([("binary_sha256", Json::str("x"))]))]);
    assert!(ExperimentSlot::from_json(&bad).is_err());
}

fn minimal_slot_json() -> Json {
    Json::obj([
        ("schema", Json::str("hh-experiment-slot/1")),
        (
            "identity",
            Json::obj([
                ("experiment_id", Json::str("e")),
                ("protocol_id", Json::str("p")),
                ("run_plan_id", Json::str("rp")),
                ("cell_id", Json::str("c")),
                ("arm_id", Json::str("a")),
                ("task_id", Json::str("t")),
                ("replicate_index", Json::Int(0)),
                ("first", Json::Bool(true)),
                ("attempt_no", Json::Int(1)),
                ("subject_run_id", Json::str("s")),
                (
                    "slot_store",
                    Json::obj([
                        ("root_digest", Json::str("d")),
                        ("run_id", Json::str("r")),
                        ("head_hash", Json::str("h")),
                    ]),
                ),
                (
                    "runner",
                    Json::obj([
                        ("driver_commit", Json::str("c")),
                        ("binary_sha256", Json::str("sha256:x")),
                    ]),
                ),
            ]),
        ),
        ("task", Json::obj([])),
        ("participant", Json::obj([])),
        ("cell", Json::obj([])),
        ("phases", Json::Arr(vec![])),
        (
            "disposition",
            Json::obj([("value", Json::str("inconclusive{infra:runner_crash}"))]),
        ),
        ("integrity", Json::obj([("seal", Json::str("sealed"))])),
        ("trajectory", Json::obj([])),
        ("costs", Json::obj([])),
    ])
}
