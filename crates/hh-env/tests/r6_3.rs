//! R6.3 — P1-D dated study recompute (REQ-XA-117): a new dated
//! `hh-study-report/1` that folds the executed DF-CAP5.1-1
//! `hh-s1-useful/1` packet (`7bbd15a2…`, `executed`,
//! `useful_acceptance: marked`) beside the R5.43 `failed` record it
//! supersedes as the stage's current state — the failed packet stands
//! verbatim as history. R5.59's packet and `r5_49.rs` stay
//! byte-unchanged. The committed artifact is produced by the exact
//! emit path below — never hand-written.
//!
//! Honest bindings (P1-D): the fold's `s1_unmet` member records the
//! S1 *objective*, which stays unmet — the executed packet's
//! `bounded_s1_execution` admission discharges only its bounded slice
//! (full-target predicates stay owed per the coverage register; the
//! useful-task card promotion is a gated operator decision, Q-A2-1).
//! `study` therefore spells `unmet{s1_objective}` and `task_truth`
//! records `executed{s1_useful}` (the bounded slice's task truth was
//! established by the independent Decided artifacts). The
//! CI readers scan forbids reading docs/ at test time — every input is
//! a byte-copy fixture under `tests/fixtures/p1/` (RC-5 byte-equality).

use std::path::{Path, PathBuf};

use hharness_env::preflight::{Leg, LegVerdict};
use hharness_env::study::{
    assemble_with_repairs, check_echo, evaluate_costs_table, evaluate_families_table,
    evaluate_useful_reexec, CostFacts, Disposition, FamilyFacts, PacketEcho, StudyFold,
    UsefulReexecFacts, LEGS, REQUIRED_FAMILIES_P1D, VERSION,
};
use hharness_wire::json::{self, Json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_packet(name: &str) -> Json {
    let text = std::fs::read_to_string(
        repo_root()
            .join("crates/hh-env/tests/fixtures/p1")
            .join(name),
    )
    .expect("packet fixture readable");
    json::parse_provider(&text).expect("packet fixture parses")
}

fn jstr(j: &Json, k: &str) -> String {
    match j.get(k) {
        Some(Json::Str(s)) => s.clone(),
        other => panic!("member {k} not a string: {other:?}"),
    }
}

/// A committed packet parsed to a `PacketEcho` — verbatim members.
fn packet_echo(j: &Json) -> PacketEcho {
    let legs = match j.get("legs") {
        Some(Json::Arr(a)) => a
            .iter()
            .map(|l| {
                let name = l.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let v = l
                    .get("verdict")
                    .and_then(|x| x.get("verdict"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                (name.to_string(), v.to_string())
            })
            .collect(),
        _ => panic!("packet legs absent"),
    };
    PacketEcho {
        version: jstr(j, "version"),
        disposition: jstr(j, "disposition"),
        admission: j
            .get("admission")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .into(),
        packet_id: jstr(j, "packet_id"),
        legs,
        details: vec![],
    }
}

fn s(v: impl ToString) -> String {
    v.to_string()
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

const DF_PACKET_ID: &str = "7bbd15a2a53101dfef1a5ea3524bf670dc446a2b90a05c49e7818c28aec13a82";
const DEVIATIONS: &str = "2026-11-07-s4-protocol-deviations.json";

/// The repair register the emitted records carry (REQ-XA-008) — the
/// same register the deviation packet carries, schema-checked below.
fn repair_register() -> Vec<Json> {
    let j = load_packet(DEVIATIONS);
    match j.get("repairs") {
        Some(Json::Arr(a)) => a.clone(),
        _ => panic!("deviation packet repairs absent"),
    }
}

/// The new fold's `useful_s1` leg — the failed record refolds `failed`
/// as history; the executed DF packet refolds `executed` with marked
/// acceptance; the objective flag stays unmet.
fn useful_leg() -> (Leg, UsefulReexecFacts) {
    let df = load_packet("2026-11-05-df-cap5.1-s1-useful.json");
    let old = load_packet("2026-11-04-r5.43-s1-useful.json");
    let df_echo = packet_echo(&df);
    assert_eq!(df_echo.packet_id, DF_PACKET_ID);
    let f = UsefulReexecFacts {
        failed: packet_echo(&old),
        executed: df_echo,
        acceptance: jstr(&df, "useful_acceptance"),
        carried: match df.get("carried_predicates") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|v| v.as_str())
                .map(String::from)
                .collect(),
            _ => panic!("carried_predicates absent"),
        },
        s1_unmet: true,
    };
    (
        leg(
            "useful_s1",
            evaluate_useful_reexec(&f),
            &[
                ("failed_packet", s(f.failed.packet_id.clone())),
                ("failed_disposition", s(&f.failed.disposition)),
                ("packet_id", s(f.executed.packet_id.clone())),
                ("disposition", s(&f.executed.disposition)),
                ("acceptance", s(&f.acceptance)),
                ("carried", s(f.carried.join(","))),
                ("s1_unmet", s(f.s1_unmet)),
                ("status", s("executed{bounded;marked}")),
            ],
        ),
        f,
    )
}

/// The unchanged legs echo the committed R5.59 fold — records-in: the
/// supplied inputs are byte-identical, so the refold is the committed
/// `verified` verdicts verbatim (`check_echo` re-validates the packet
/// refold itself).
fn echoed_legs() -> Vec<Leg> {
    let report = load_packet("2026-11-06-r5.59-study-report.json");
    let echo = packet_echo(&report);
    assert_eq!(
        check_echo(&echo, &LEGS, "reported", "failed"),
        LegVerdict::Verified
    );
    match report.get("legs") {
        Some(Json::Arr(a)) => a
            .iter()
            .map(|l| {
                let name = l
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut facts: Vec<(String, String)> = Vec::new();
                if let Some(Json::Obj(m)) = l.get("facts") {
                    for (k, v) in m {
                        if let Json::Str(v) = v {
                            facts.push((k.clone(), v.clone()));
                        }
                    }
                }
                facts.push((
                    "refold".to_string(),
                    "2026-11-06-r5.59-study-report.json".to_string(),
                ));
                Leg {
                    name,
                    verdict: LegVerdict::Verified,
                    facts: facts.into_iter().collect(),
                }
            })
            .collect(),
        _ => panic!("study report legs absent"),
    }
}

fn eval_name(v: &LegVerdict) -> &'static str {
    match v {
        LegVerdict::Verified => "verified",
        LegVerdict::Inconclusive(_) => "inconclusive",
        LegVerdict::Refused(_) => "refused",
        LegVerdict::Unsupported(_) => "unsupported",
    }
}

/// AC-6 — the schema check over the emitted records: the deviation
/// packet's `hh-protocol-deviations/1` register validates the closed
/// class set, a code location naming a commit, and affected slots on
/// every entry.
fn check_deviation_register() {
    let j = load_packet(DEVIATIONS);
    assert_eq!(jstr(&j, "schema"), "hh-protocol-deviations/1");
    let classes = [
        "machinery-committed",
        "machinery-scratch",
        "environment",
        "admission",
    ];
    let repairs = repair_register();
    assert!(!repairs.is_empty());
    for r in &repairs {
        let cls = jstr(r, "class");
        assert!(classes.contains(&cls.as_str()), "class {cls}");
        let commit = r
            .get("code")
            .and_then(|c| c.get("commit"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            commit.len() >= 7 && commit.chars().all(|c| c.is_ascii_hexdigit()),
            "commit {commit}"
        );
        assert!(
            r.get("code")
                .and_then(|c| c.get("path"))
                .and_then(|v| v.as_str())
                .map(|p| !p.is_empty())
                == Some(true),
            "code.path absent"
        );
        match r.get("affected_slots") {
            Some(Json::Arr(a)) => assert!(!a.is_empty(), "affected_slots empty"),
            _ => panic!("affected_slots absent"),
        }
    }
    // The recomputed counts the record enumerates equal RC-4's fold.
    let deps = match j.get("departures") {
        Some(Json::Arr(a)) => a,
        _ => panic!("departures absent"),
    };
    let get = |id: &str| -> &Json {
        deps.iter()
            .find(|d| jstr(d, "id") == id)
            .unwrap_or_else(|| panic!("departure {id} absent"))
    };
    let n = |j: &Json, k: &str| j.get(k).and_then(|v| v.as_int()).unwrap_or(-1);
    let mult = get("reattempts_beyond_frozen_bound")
        .get("counts")
        .unwrap()
        .get("multiplicity")
        .unwrap()
        .clone();
    assert_eq!(n(&mult, "1"), 160);
    assert_eq!(n(&mult, "2"), 90);
    assert_eq!(n(&mult, "3"), 20);
    assert_eq!(n(&mult, "4"), 30);
    assert_eq!(
        n(get("round0_later_attempts").get("counts").unwrap(), "count"),
        28
    );
    assert_eq!(
        n(get("backend_mix").get("counts").unwrap(), "arm64_applehv"),
        250
    );
    assert_eq!(
        n(get("backend_mix").get("counts").unwrap(), "amd64_gce"),
        50
    );
    assert_eq!(n(get("network_mix").get("counts").unwrap(), "none"), 160);
    assert_eq!(n(get("network_mix").get("counts").unwrap(), "bridge"), 140);
    // The record is never an amendment (its own kind line says so).
    assert!(matches!(
        j.get("kind").and_then(|v| v.as_str()),
        Some(k) if k.contains("never an amendment")
    ));
}

#[test]
fn p1d_study_recompute_folds_the_executed_packet() {
    check_deviation_register();

    let (uleg, uf) = useful_leg();
    assert_eq!(uleg.verdict, LegVerdict::Verified);
    assert_eq!(uf.executed.packet_id, DF_PACKET_ID);
    assert_eq!(uf.acceptance, "marked");

    // The unchanged legs echo the committed fold; the useful_s1 leg is
    // replaced by the new recompute — every other leg's verdict stays
    // the committed `verified`.
    let mut legs: Vec<Leg> = echoed_legs()
        .into_iter()
        .filter(|l| l.name != "useful_s1" && l.name != "families" && l.name != "costs")
        .collect();
    legs.push(uleg);

    // families — task_truth records the executed bounded slice.
    let fam = FamilyFacts {
        efficacy: "computed{strata:easy,hard,medium}".into(),
        required_efficacy: "computed{strata:easy,hard,medium}".into(),
        task_mixture: "descriptive{pilot_recomputed}".into(),
        sharp_null: "unavailable{not_computed}".into(),
        parity: "executed{verified_30_30}".into(),
        mechanism: "executed{activation,pilot}".into(),
        engineering: "executed{ops_live;backend_verified}".into(),
        task_truth: "executed{s1_useful}".into(),
        operations: "executed{ops_live}".into(),
        adoption: "unavailable{not_started}".into(),
        claim_present: false,
    };
    assert_eq!(
        evaluate_families_table(&fam, &REQUIRED_FAMILIES_P1D),
        LegVerdict::Verified
    );
    legs.push(leg(
        "families",
        evaluate_families_table(&fam, &REQUIRED_FAMILIES_P1D),
        &[
            ("efficacy", s(&fam.efficacy)),
            ("task_truth", s(&fam.task_truth)),
            ("adoption", s(&fam.adoption)),
        ],
    ));

    // costs — the DF packet joins the rollup (15 packets, 0 micros,
    // USD $0.00; 14 admitted first subjects incl. the two S1 subjects).
    let co = CostFacts {
        per_packet: [
            "hh-backend-preflight/1#first",
            "hh-backend-preflight/1#reprobe",
            "hh-s1-useful/1",
            "hh-s1-useful/1#df-cap5.1",
            "hh-ops-live/1#first",
            "hh-ops-live/1#second",
            "hh-activation/1",
            "hh-pilot/1",
            "hh-parity-n0/1",
            "hh-confirm-withheld/1",
            "hh-parity-tranche-report/1",
            "hh-s4-confirmation/1",
            "hh-s4-egress-supplement/1",
            "hh-s4-completion/1",
            "hh-s4-30slot-repairs/1",
        ]
        .iter()
        .map(|p| (p.to_string(), 0))
        .collect(),
        subjects_first: 14,
        gated_subjects: 90,
    };
    assert_eq!(evaluate_costs_table(&co, 15, 14, 90), LegVerdict::Verified);
    legs.push(leg(
        "costs",
        evaluate_costs_table(&co, 15, 14, 90),
        &[
            ("packets", s(co.per_packet.len())),
            ("total_micros", s(0)),
            ("subjects_first", s(co.subjects_first)),
            ("gated_subjects", s(co.gated_subjects)),
        ],
    ));

    // Legs must be in LEGS order.
    legs.sort_by_key(|l| LEGS.iter().position(|n| *n == l.name).unwrap());
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let fold = StudyFold {
        subjects_first: 14,
        exposure_micros: 0,
        s1_unmet: true,
        s3_unmet: false,
        s4_unmet: false,
        study: "unmet{s1_objective}".into(),
    };
    let report = assemble_with_repairs(legs, &fold, repair_register()).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.disposition, Disposition::Reported);
    assert!(report.s1_unmet && !report.s3_unmet && !report.s4_unmet);
    assert_eq!(report.study, "unmet{s1_objective}");
    assert_eq!(report.subjects_first, 14);
    assert_eq!(report.admission, "recompute_only");
    assert_eq!(report.repairs.len(), 7);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("7bbd15a2a53101dfef1a5ea3524bf670dc446a2b90a05c49e7818c28aec13a82"));

    if let Ok(out) = std::env::var("HH_STUDY_REPORT_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, format!("{j}\n")).unwrap();
        eprintln!("emitted {} disposition=reported", out.display());
    }
}

#[test]
fn p1d_reexec_refusals() {
    let df = load_packet("2026-11-05-df-cap5.1-s1-useful.json");
    let old = load_packet("2026-11-04-r5.43-s1-useful.json");
    let base = UsefulReexecFacts {
        failed: packet_echo(&old),
        executed: packet_echo(&df),
        acceptance: "marked".into(),
        carried: vec![
            "probe_verb_enoent".into(),
            "exec_backend_hello_clobber".into(),
        ],
        s1_unmet: true,
    };
    // A mutated disposition refuses — the fold never relabels.
    let mut bad = base.clone();
    bad.executed.disposition = "failed".into();
    assert!(matches!(
        evaluate_useful_reexec(&bad),
        LegVerdict::Refused(_)
    ));
    // A withheld acceptance refuses — the executed packet is `marked`.
    let mut bad = base.clone();
    bad.acceptance = "withheld{no_independent_correct_artifact}".into();
    assert!(matches!(
        evaluate_useful_reexec(&bad),
        LegVerdict::Refused(_)
    ));
    // A met-objective claim refuses — the bounded slice is not the
    // objective.
    let mut bad = base.clone();
    bad.s1_unmet = false;
    assert!(matches!(
        evaluate_useful_reexec(&bad),
        LegVerdict::Refused(_)
    ));
    // The failed record's refold still holds as history.
    assert_eq!(
        check_echo(
            &base.failed,
            &hharness_env::useful::LEGS,
            "executed",
            "failed"
        ),
        LegVerdict::Verified
    );
    let _ = eval_name(&LegVerdict::Verified);
}
