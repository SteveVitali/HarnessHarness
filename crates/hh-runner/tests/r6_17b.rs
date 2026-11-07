//! R6.17 Stage-2 acceptance battery (manifest 270; REPROOF machinery):
//! the driver's outcome classes + append-only register (AC-6), the
//! pinned label + committed `parity_admission` refusal + live-evidence
//! gate (AC-7) and the REQ-XB-110 fault-injection leg. The `reproof`
//! leg's registry half is exercised by `scripts/ci/test-live-verify.sh`.

use std::collections::BTreeMap;

use hharness_runner::harbor::{HarborPin, HARBOR_PIN};
use hharness_runner::reproof::{
    classify_task, committed_parity_refusal, prove, DiscordClass, EvidenceLabel, FaultInjection,
    ReproofPacket, ReproofPlan, SurfaceDrive, SurfaceOutcome, TaskVerdict, REPROOF_LABEL,
};
use hharness_runner::verdict::{MediatorRow, RewardFileSet};
use hharness_wire::json::Json;

fn pin() -> HarborPin {
    HarborPin::admitted(
        "2026-11-07T22:52Z",
        "2bc1a23bd961de0bec8742940f35ac9070c67815",
    )
}

struct FixtureDrive {
    outcomes: BTreeMap<(String, String), SurfaceOutcome>,
}

impl SurfaceDrive for FixtureDrive {
    fn run_surface(&mut self, surface: &str, task: &str, _r: u32) -> SurfaceOutcome {
        self.outcomes
            .get(&(task.to_string(), surface.to_string()))
            .cloned()
            .unwrap_or_default()
    }
}

fn ok(n: u32) -> SurfaceOutcome {
    SurfaceOutcome {
        rewards: vec![true; n as usize],
        digests: BTreeMap::new(),
        denied_hosts: vec![],
        harbor_exception: None,
        wall_ms: 0,
    }
}

fn freeze() -> hharness_env::egress_allowlist::AllowlistFreeze {
    use hharness_env::egress_allowlist::{Allowlist, ImageFamily};
    let list = Allowlist::first_pass(
        ImageFamily::Debian,
        ["astral.sh".to_string()].into_iter().collect(),
        &[],
        vec![],
    );
    hharness_env::egress_allowlist::AllowlistFreeze::freeze("p:reproof", 0, list)
}

#[test]
fn ac6_classify_outcome_classes() {
    // admit: R/R on every surface.
    assert_eq!(
        classify_task(
            &[true, true, true],
            &[true, true, true],
            &[true, true, true],
            "x",
            &[]
        ),
        TaskVerdict::Admit
    );
    // o_fail — hard (0/3) and flaky (2/3) — excluded, outcome-blind.
    for (o, flaky) in [
        (&[false, false, false][..], false),
        (&[true, true, false][..], true),
    ] {
        match classify_task(o, &[true; 3], &[true; 3], "sandbox_failed", &[]) {
            TaskVerdict::Discord {
                class,
                basis,
                flaky: f,
                ..
            } => {
                assert_eq!(class, DiscordClass::OFail);
                assert_eq!(
                    basis.as_deref(),
                    Some("runner_infeasible:oracle_fails_on_O{sandbox_failed}")
                );
                assert_eq!(f, flaky);
            }
            v => panic!("{v:?}"),
        }
    }
    // machinery — repaired, never excluded (no basis).
    match classify_task(&[true; 3], &[true, false, true], &[true; 3], "x", &[]) {
        TaskVerdict::Discord { class, basis, .. } => {
            assert_eq!(class, DiscordClass::Machinery);
            assert_eq!(basis, None);
        }
        v => panic!("{v:?}"),
    }
    // allowlist_gap — excluded, names the denied host.
    match classify_task(
        &[true; 3],
        &[true; 3],
        &[true, false, true],
        "x",
        &["pypi.org".to_string()],
    ) {
        TaskVerdict::Discord {
            class, basis, host, ..
        } => {
            assert_eq!(class, DiscordClass::AllowlistGap);
            assert_eq!(
                basis.as_deref(),
                Some("runner_infeasible:allowlist_gap{pypi.org}")
            );
            assert_eq!(host.as_deref(), Some("pypi.org"));
        }
        v => panic!("{v:?}"),
    }
}

#[test]
fn ac6_prove_register_and_freeze() {
    let mut drive = FixtureDrive {
        outcomes: BTreeMap::new(),
    };
    // admit task.
    for s in ["O", "H-pub", "H-allow"] {
        drive.outcomes.insert(("t-admit".into(), s.into()), ok(3));
    }
    // o_fail task — O fails twice with an exception + digests.
    let mut of = SurfaceOutcome {
        rewards: vec![false; 3],
        digests: [("reward".to_string(), "sha256:r".to_string())]
            .into_iter()
            .collect(),
        denied_hosts: vec![],
        harbor_exception: Some("sandbox_failed".to_string()),
        wall_ms: 7,
    };
    drive
        .outcomes
        .insert(("t-ofail".into(), "O".into()), of.clone());
    drive
        .outcomes
        .insert(("t-ofail".into(), "H-pub".into()), ok(3));
    drive
        .outcomes
        .insert(("t-ofail".into(), "H-allow".into()), ok(3));
    // machinery task — H-pub flakes.
    drive.outcomes.insert(("t-mach".into(), "O".into()), ok(3));
    drive.outcomes.insert(
        ("t-mach".into(), "H-pub".into()),
        SurfaceOutcome {
            rewards: vec![true, false, true],
            digests: BTreeMap::new(),
            denied_hosts: vec![],
            harbor_exception: None,
            wall_ms: 2,
        },
    );
    drive
        .outcomes
        .insert(("t-mach".into(), "H-allow".into()), ok(3));
    // allowlist_gap task — H-allow denied pypi.org.
    drive.outcomes.insert(("t-gap".into(), "O".into()), ok(3));
    drive
        .outcomes
        .insert(("t-gap".into(), "H-pub".into()), ok(3));
    drive.outcomes.insert(
        ("t-gap".into(), "H-allow".into()),
        SurfaceOutcome {
            rewards: vec![false; 3],
            digests: BTreeMap::new(),
            denied_hosts: vec!["pypi.org".to_string()],
            harbor_exception: None,
            wall_ms: 1,
        },
    );
    of.rewards = vec![false; 3];
    let plan = ReproofPlan {
        pin: pin(),
        r: 3,
        observe: hharness_runner::reproof::ObservePass::MediatorRecordOnly,
        freeze: freeze(),
        tasks: vec![
            "t-admit".into(),
            "t-ofail".into(),
            "t-mach".into(),
            "t-gap".into(),
        ],
        fault_injection: Some(FaultInjection {
            task: "t-gap".into(),
            removed_host: "astral.sh".into(),
            expected: "oracle_failure{verifier_unavailable:bootstrap:egress_denied}".into(),
            observed: None,
        }),
    };
    let packet = prove(&plan, &mut drive).unwrap();
    assert_eq!(packet.tasks["t-admit"].0, "admit");
    assert_eq!(
        packet.tasks["t-ofail"].0,
        "exclude:runner_infeasible:oracle_fails_on_O{sandbox_failed}"
    );
    assert_eq!(packet.tasks["t-mach"].0, "repair:machinery");
    assert_eq!(
        packet.tasks["t-gap"].0,
        "exclude:runner_infeasible:allowlist_gap{pypi.org}"
    );
    // The register: one append-only row per discordant task with the
    // §3.8 fields; seq monotone.
    assert_eq!(packet.register.rows.len(), 3);
    for (i, row) in packet.register.rows.iter().enumerate() {
        assert_eq!(row.seq as usize, i + 1);
        assert!(!row.surfaces.is_empty());
        assert_eq!(row.repeats, 3);
    }
    let gap = &packet.register.rows[2];
    assert_eq!(gap.classification, "allowlist_gap");
    assert_eq!(gap.denied_hosts, vec!["pypi.org".to_string()]);
    assert_eq!(gap.walls.get("H-allow"), Some(&1));
    let ofail = &packet.register.rows[0];
    assert_eq!(ofail.harbor_exception.as_deref(), Some("sandbox_failed"));
    // The freeze step uses R6.15's record + digest.
    assert_eq!(packet.freeze.digest, freeze().digest);
    // Round-trip: the register's append-only decode.
    let rt = hharness_runner::reproof::DiscordanceRegister::from_json(&packet.register.to_json())
        .unwrap();
    assert_eq!(rt.rows.len(), 3);
    // A re-ordered/shadowed seq refuses.
    let mut bad = match packet.register.to_json() {
        Json::Obj(m) => m,
        _ => panic!(),
    };
    let mut rows = match bad.get("rows") {
        Some(Json::Arr(r)) => r.clone(),
        _ => panic!(),
    };
    if let Json::Obj(ref mut r0) = rows[0] {
        r0.insert("seq".into(), Json::Int(9));
    }
    bad.insert("rows".into(), Json::Arr(rows));
    assert!(hharness_runner::reproof::DiscordanceRegister::from_json(&Json::Obj(bad)).is_err());
}

// ── AC-7 — the pinned label + committed refusal + live-evidence gate ─────────

#[test]
fn ac7_label_refusal_and_live_gate() {
    // The committed refusal — the real parity_admission call.
    let pr = committed_parity_refusal("reproof:test:ref");
    assert_eq!(pr.submitted, "oracle_transport_equality");
    assert_eq!(
        pr.refusal,
        "inauthentic_evidence{oracle_transport_equality}"
    );
    // The packet carries the pinned phrase verbatim.
    let packet = ReproofPacket {
        harbor_pin: HARBOR_PIN.into(),
        observe: hharness_runner::reproof::ObservePass::AskToLog,
        freeze: freeze(),
        r: 3,
        tasks: BTreeMap::new(),
        register: Default::default(),
        parity_refusal: pr,
        fault_injection: None,
        evidence: EvidenceLabel::RecordedExecution,
        image_identity: "image-identical".into(),
    };
    let j = packet.to_json();
    assert_eq!(j.get("label").unwrap().as_str(), Some(REPROOF_LABEL));
    assert_eq!(j.get("parity").unwrap().as_str(), Some("none"));
    // recorded_execution round-trips; live-executed without the
    // hh-live-evidence/1 ref refuses (REQ-XH-027).
    let rt = ReproofPacket::from_json(&j).unwrap();
    assert_eq!(rt.evidence, EvidenceLabel::RecordedExecution);
    let jobj = || match j.clone() {
        Json::Obj(m) => m,
        _ => panic!(),
    };
    let mut m = jobj();
    m.insert("evidence_label".into(), Json::str("live-executed"));
    assert!(ReproofPacket::from_json(&Json::Obj(m)).is_err());
    let mut m2 = jobj();
    m2.insert("evidence_label".into(), Json::str("live-executed"));
    m2.insert(
        "live_evidence_ref".into(),
        Json::str("docs/build/reports/live/2026-11-07-reproof.json"),
    );
    assert!(ReproofPacket::from_json(&Json::Obj(m2)).is_ok());
    // A drifted label refuses (the phrase pin).
    let mut m3 = jobj();
    m3.insert("label".into(), Json::str("other"));
    assert!(ReproofPacket::from_json(&Json::Obj(m3)).is_err());
}

// ── REQ-XB-110 — the fault-injection leg ──────────────────────────────────────

#[test]
fn fault_injection_classifies_egress_denied() {
    // One task's verify allowlist without astral.sh: a zero reward
    // with no pytest report under a verify-phase deny classifies
    // verifier_unavailable:bootstrap:egress_denied (R6.9's classifier).
    let files = RewardFileSet {
        reward_json: Some(br#"{"reward":0}"#.to_vec()),
        reward_txt: None,
        ctrf_json: None,
        test_stdout: None,
    };
    let rows = vec![MediatorRow {
        phase: "verify".into(),
        decision: "deny".into(),
    }];
    let observed = hharness_runner::reproof::run_fault_injection(&files, &rows);
    assert_eq!(
        observed,
        "oracle_failure{verifier_unavailable:bootstrap:egress_denied}"
    );
    // The declared leg names astral.sh and the expected spelling.
    let fi = FaultInjection {
        task: "t".into(),
        removed_host: "astral.sh".into(),
        expected: "oracle_failure{verifier_unavailable:bootstrap:egress_denied}".into(),
        observed: Some(observed),
    };
    assert_eq!(fi.observed.as_deref(), Some(fi.expected.as_str()));
}
