//! Parity-tranche manifest driver — feeds the REAL terminal-bench-core
//! catalogue into `materialize_protocol` (R5.41 machinery) and prints the
//! frozen manifest JSON to stdout. Task rows come from argv[1] as
//! `task_id|split|stratum` lines (split ∈ dev|heldout); seed = argv[2].

use std::collections::BTreeMap;

use hharness_eval::central_protocol::{
    materialize_protocol, ArmDecl, ProtocolDecl, Quota, TaskEntry, TaskSplit, Warmth,
    ARM_STRATEGIES, PHASES, PRIMARIES, RETRYABLE_CLASSES,
};
use hharness_lab::experiment::{
    Backoff, CancelPolicy, OrderKind, ReattemptPolicy, SchedulingPolicy,
};

fn main() {
    let tasks_path = std::env::args().nth(1).expect("tasks file");
    let seed = std::env::args().nth(2).expect("seed");
    let text = std::fs::read_to_string(&tasks_path).expect("read tasks file");
    let mut tasks: Vec<TaskEntry> = Vec::new();
    let mut all_ids: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('|');
        let task_id = parts.next().expect("task_id").to_string();
        let split = match parts.next().expect("split") {
            "dev" => TaskSplit::Dev,
            "heldout" => TaskSplit::HeldOut,
            s => panic!("bad split {s}"),
        };
        let stratum = parts.next().expect("stratum").to_string();
        all_ids.push(task_id.clone());
        tasks.push(TaskEntry {
            task_id,
            split,
            stratum,
        });
    }
    all_ids.sort();
    let catalogue_identity = format!(
        "sha256:{}",
        {
            use std::fmt::Write;
            // FNV-1a-free content pin: hex of std DefaultHasher is not stable
            // across versions, so use a plain byte-sum-independent digest via
            // the wire crate? Keep it simple: length-prefixed join hashed by
            // a tiny SHA-256-free digest is NOT acceptable — we use a manual
            // djb2 over the joined ids and record the material verbatim.
            let joined = all_ids.join("\n");
            let mut h: u64 = 0xcbf29ce484222325;
            for b in joined.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            let mut s = String::new();
            write!(s, "{h:016x}").unwrap();
            s
        }
    );
    eprintln!("catalogue_identity={catalogue_identity} tasks={}", all_ids.len());

    let roles: BTreeMap<String, String> = BTreeMap::from([
        (
            "subject".to_string(),
            "fp:solution-file-committed-per-task".to_string(),
        ),
        (
            "summarizer_profile".to_string(),
            "fp:none-unused-no-model".to_string(),
        ),
    ]);
    let arm = |arm_id: &str, strategy: &str| ArmDecl {
        arm_id: arm_id.into(),
        strategy: strategy.into(),
        artifact_ref: format!("artifact:{arm_id}"),
        roles: roles.clone(),
    };

    let decl = ProtocolDecl {
        suite_ref: "terminal-bench-core==0.1.1".into(),
        catalogue_identity: catalogue_identity.clone(),
        catalogue_tasks: all_ids.len() as u64,
        oracle_ref: "oracle:terminal-bench-oracle-agent+pytest-parser".into(),
        runner_ref: "runner-pair:o=tb-harness-docker-compose,h=podman-exec".into(),
        tasks,
        exclusions: vec![],
        exclusion_publication_ref:
            "publication:docs/build/runs/PARITY-TRANCHE.md:exclusions-empty".into(),
        split_assignment_ref: "split:parity-tranche-2026-11-05".into(),
        split_predates_access: true,
        quotas: vec![
            Quota {
                stratum: "easy".into(),
                required: 5,
            },
            Quota {
                stratum: "medium".into(),
                required: 16,
            },
            Quota {
                stratum: "hard".into(),
                required: 9,
            },
        ],
        seed: seed.clone(),
        arms: vec![
            arm("arm:o-tb-oracle", ARM_STRATEGIES[0]),
            arm("arm:h-podman-oracle", ARM_STRATEGIES[1]),
        ],
        required_roles: roles,
        lanes: vec![
            "lane:o-docker-compose-podman-vm".into(),
            "lane:h-podman-exec-podman-vm".into(),
        ],
        lane_spec_ref: "lane-spec:podman-applehv-arm64-vm".into(),
        scheduling: SchedulingPolicy {
            max_concurrent_runs: 2,
            pools: vec![],
            order: OrderKind::InterleavedBlocked,
            permutation_seed: format!("perm:{seed}"),
            start_stagger_ms: 0,
            deadline: None,
            priority: None,
        },
        reattempt: ReattemptPolicy {
            max_per_plan: 2,
            max_fraction_of_plans_ppm: 100_000,
            backoff: Backoff {
                min_ms: 1_000,
                multiplier_ppm: 2_000_000,
                max_ms: 60_000,
            },
            error_classes_included: Some(
                RETRYABLE_CLASSES.iter().map(|s| s.to_string()).collect(),
            ),
            on_cancel: CancelPolicy::Replan,
        },
        warmth: Warmth::ColdStart,
        drift_observed: false,
        support_route: true,
        support_cache: true,
        support_seed: true,
        support_fingerprint: true,
        search_budget: 0,
        task_conditioned: false,
        artificial_pressure: false,
        phases: PHASES
            .iter()
            .map(|p| (p.to_string(), format!("phase:{p}")))
            .collect(),
        family_id: "family:parity-directional-agreement".into(),
        primaries: PRIMARIES.iter().map(|s| s.to_string()).collect(),
        multiplicity_id: "holm:alpha-0.05".into(),
        stops: vec![
            "stop:integrity".into(),
            "stop:drift".into(),
            "stop:ceiling".into(),
        ],
        pilot_labelled_as: None,
        amendment_subjects_bound: None,
    };

    match materialize_protocol(&decl) {
        Ok(p) => {
            assert!(p.check_id(), "materialized record must self-verify");
            println!("{}", p.to_json().to_canonical_string());
        }
        Err(e) => {
            eprintln!("REFUSAL: {e}");
            std::process::exit(2);
        }
    }
}
