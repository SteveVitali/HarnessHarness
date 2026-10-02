//! `run_refs` — the `resolved_dependencies` lock-manifest index of
//! every reference a subject run's `RunManifest` carries (§5h.3
//! "Complete: … every run-manifest reference of every subject run
//! resolves inside `resolved_dependencies`"; AC-R-2.12.1-8's first
//! clause; S3.12).
//!
//! The projection is a flat `{dotted.field → ref}` map — one entry per
//! reference the manifest names (`harness_def_ref`, the policy refs,
//! `task_ref.*`, `experiment.*` refs, the lineage coordinates, …).
//! Non-reference members (`seed`, `lease_ttl`, `run_kind`, …) are not
//! indexed: they name no external content. `extra` members are
//! uninterpreted — they ride the manifest verbatim and are not claimed
//! here (ADR-0290 D2).
//!
//! One function serves both directions: `assemble` writes the index
//! into `resolved_dependencies.run_refs{run_id}`, and `validate`
//! recomputes the index from the exported `lifecycle.run.created`
//! payload and compares — a reference the manifest carries but the
//! index lacks is `manifest_reference_unresolved` (S1), and an index
//! entry the manifest does not carry is a stale claim (the same check,
//! symmetric — CC3).

use std::collections::BTreeMap;

use hh_ledger::manifest::RunManifest;
use hh_wire::json::Json;

/// The flat `{dotted.field → ref}` projection of `rm`'s reference
/// members. Nested members index under dotted keys
/// (`experiment.design_ref`, `task_ref.task_id`, `forked_from.head_hash`,
/// `spawn_event.event_id`); `signer_key_ids` indexes as one array member.
pub fn manifest_refs(rm: &RunManifest) -> Json {
    let mut m: BTreeMap<String, Json> = BTreeMap::new();
    let mut put = |k: &str, v: &Option<String>| {
        if let Some(v) = v {
            m.insert(k.to_string(), Json::str(v.clone()));
        }
    };
    put("configuration_id", &rm.configuration_id);
    put("configuration_version_id", &rm.configuration_version_id);
    put("harness_def_ref", &rm.harness_def_ref);
    put("model_profile_ref", &rm.model_profile_ref);
    put("environment_ref", &rm.environment_ref);
    put("environment_version_id", &rm.environment_version_id);
    put("budget", &rm.budget);
    put("hosting_mechanism", &rm.hosting_mechanism);
    put("capability_declaration_ref", &rm.capability_declaration_ref);
    put("overrides_layer_id", &rm.overrides_layer_id);
    put("envelope_policy_ref", &rm.envelope_policy_ref);
    put("healing_policy_ref", &rm.healing_policy_ref);
    put("audit_policy_ref", &rm.audit_policy_ref);
    put("registry_snapshot_id", &rm.registry_snapshot_id);
    put("parent_run_id", &rm.parent_run_id);
    if !rm.signer_key_ids.is_empty() {
        m.insert(
            "signer_key_ids".to_string(),
            Json::Arr(rm.signer_key_ids.iter().map(Json::str).collect()),
        );
    }
    if let Some(ev) = &rm.spawn_event {
        m.insert(
            "spawn_event.run_id".to_string(),
            Json::str(ev.run_id.clone()),
        );
        m.insert(
            "spawn_event.event_id".to_string(),
            Json::str(ev.event_id.clone()),
        );
    }
    for (key, link) in [
        ("forked_from", &rm.forked_from),
        ("continued_from", &rm.continued_from),
    ] {
        if let Some(l) = link {
            m.insert(format!("{key}.run_id"), Json::str(l.run_id.clone()));
            m.insert(format!("{key}.at_seq"), Json::Int(l.at_seq as i64));
            m.insert(format!("{key}.head_hash"), Json::str(l.head_hash.clone()));
        }
    }
    if let Some(t) = &rm.task_ref {
        m.insert("task_ref.task_id".to_string(), Json::str(t.task_id.clone()));
        m.insert(
            "task_ref.suite_id".to_string(),
            Json::str(t.suite_id.clone()),
        );
    }
    if let Some(e) = &rm.experiment {
        for (k, v) in [
            ("experiment_id", &e.experiment_id),
            ("plan_id", &e.plan_id),
            ("scheduling_policy", &e.scheduling_policy),
            ("reattempt_policy", &e.reattempt_policy),
            ("design_ref", &e.design_ref),
            ("pre_registration_ref", &e.pre_registration_ref),
            ("match_spec_ref", &e.match_spec_ref),
            ("suite_manifest_ref", &e.suite_manifest_ref),
            ("registry_snapshot_id", &e.registry_snapshot_id),
            ("experiment_run_id", &e.experiment_run_id),
            ("arm_id", &e.arm_id),
            ("cell_id", &e.cell_id),
        ] {
            if let Some(v) = v {
                m.insert(format!("experiment.{k}"), Json::str(v.clone()));
            }
        }
    }
    Json::Obj(m)
}

/// `run_refs` — the per-subject-run index object
/// (`{run_id → manifest_refs}`) `assemble` writes into
/// `resolved_dependencies`.
pub fn run_refs_index(run_manifests: &[(&str, &RunManifest)]) -> Json {
    Json::Obj(
        run_manifests
            .iter()
            .map(|(run, rm)| (run.to_string(), manifest_refs(rm)))
            .collect(),
    )
}

/// The `run_refs` member of a decoded `resolved_dependencies` section —
/// `{run_id → {field → ref}}`, empty object when absent.
pub fn declared_run_refs(deps_doc: &Json, run_id: &str) -> Json {
    deps_doc
        .get("run_refs")
        .and_then(|r| r.get(run_id))
        .cloned()
        .unwrap_or_else(|| Json::obj([]))
}
