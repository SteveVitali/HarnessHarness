//! R6.42 — the `hh-fp-partition/1` acceptance battery (manifest 303,
//! Stage 1 FP-RECORD-PARTITION; REQ-XH-114, REQ-XD-106, REQ-XH-011
//! primary; REQ-XH-121 contributing). Fixture-verified, offline, $0.

use hharness_runner::continuation::check_cost_only;
use hharness_runner::fp_partition::*;
use hharness_runner::fp_seal::{check_dev_projection, dev_projection, DEV_PROJECTION_SCHEMA};
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::Json;
use hharness_wire::sha256::sha256_hex;

fn repo(rel: &str) -> Json {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    hharness_wire::json::parse(
        &std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}")),
    )
    .unwrap_or_else(|e| panic!("parse {rel}: {e}"))
}

/// The committed-record relpaths — the same inputs manifest the
/// emitter resolves (`scripts/fp-partition-inputs.txt`; the reader
/// rule keeps the docs/ relpaths at script level, never in the crate).
fn inputs() -> PartitionInputs {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("scripts")
        .join("fp-partition-inputs.txt");
    let body = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest.display()));
    PartitionInputs::from_manifest(&body).expect("fp-partition inputs manifest parses")
}

fn committed() -> (Json, Json, Json) {
    let i = inputs();
    (repo(&i.packet), repo(&i.partition), repo(&i.dev_projection))
}

fn partition<'a>(record: &'a Json, name: &str) -> &'a Json {
    record
        .get("partitions")
        .and_then(JsonExt::as_arr)
        .and_then(|ps| {
            ps.iter()
                .find(|p| p.get("name").and_then(Json::as_str) == Some(name))
        })
        .unwrap_or_else(|| panic!("partition {name} absent"))
}

fn member_tasks(record: &Json, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for m in partition(record, name)
        .get("members")
        .and_then(JsonExt::as_arr)
        .expect("members")
    {
        for k in ["member", "drawn"] {
            if let Some(t) = m.get(k).and_then(Json::as_str) {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// AC-1 — the partition recomputes from the committed seed: 3/2/2
/// tasks, 12/8/8 trajectories, disjoint by task, digests recomputed,
/// members byte-verbatim against the packet's fp_dev7 slots.
#[test]
fn ac1_partition_recomputes_from_committed_seed() {
    let (packet, record, _proj) = committed();
    check_partition(&record, &packet).expect("committed partition passes its schema check");

    for (name, tasks, traj) in [("dev", 3i64, 12i64), ("gate_a", 2, 8), ("gate_b", 2, 8)] {
        let p = partition(&record, name);
        assert_eq!(p.get("tasks"), Some(&Json::Int(tasks)), "{name} tasks");
        assert_eq!(
            p.get("trajectories"),
            Some(&Json::Int(traj)),
            "{name} trajectories"
        );
        let members = p.get("members").and_then(JsonExt::as_arr).expect("members");
        assert_eq!(members.len() as i64, tasks, "{name} member count");
    }
    // disjoint by task — no task appears in two partitions
    let dev: std::collections::BTreeSet<String> =
        member_tasks(&record, "dev").into_iter().collect();
    for g in ["gate_a", "gate_b"] {
        for t in member_tasks(&record, g) {
            assert!(!dev.contains(&t), "{t} in dev and {g}");
        }
    }
    let ga: std::collections::BTreeSet<String> =
        member_tasks(&record, "gate_a").into_iter().collect();
    for t in member_tasks(&record, "gate_b") {
        assert!(!ga.contains(&t), "{t} in gate_a and gate_b");
    }
    // a wrong seed recomputes a different member order — the check
    // catches it (the committed seed is what freezes the draw)
    let mut wrong = record.clone();
    if let Json::Obj(ref mut m) = wrong {
        m.insert("seed".to_string(), Json::str("fp-partition-wrong-seed"));
    }
    assert!(
        check_partition(&wrong, &packet).is_err(),
        "a different seed must fail the recompute check"
    );
}

/// AC-1 — no development task is in the held-out manifest; the
/// blocked freeze state is recorded verbatim on every member.
#[test]
fn ac1_no_dev_task_in_held_out_and_states_verbatim() {
    let (packet, record, _proj) = committed();
    let i = inputs();
    check_dev_not_held(&record, &packet, &repo(&i.select_commit))
        .expect("no partition member in the held-out manifest");

    // the freeze is blocked — every member carries its freeze state
    // verbatim; none was silently admitted
    let mut states: std::collections::BTreeMap<String, usize> = Default::default();
    for name in PARTITION_NAMES {
        for m in partition(&record, name)
            .get("members")
            .and_then(JsonExt::as_arr)
            .expect("members")
        {
            let s = m
                .get("state")
                .and_then(Json::as_str)
                .expect("member state verbatim");
            assert_ne!(s, "admitted", "a blocked slot never reads admitted");
            *states.entry(s.to_string()).or_default() += 1;
        }
    }
    assert_eq!(
        states.get("blocked{un_reproofed_successor}"),
        Some(&4),
        "4 slots blocked on un-re-proofed successors"
    );
    assert_eq!(
        states.get("blocked{no_adjacent_donor}"),
        Some(&3),
        "3 medium slots blocked with no adjacent donor"
    );

    // a planted held-out member refuses
    let mut planted = record.clone();
    if let Json::Obj(ref mut top) = planted {
        if let Some(Json::Arr(parts)) = top.get_mut("partitions") {
            if let Json::Obj(ref mut p) = parts[0] {
                if let Some(Json::Arr(ms)) = p.get_mut("members") {
                    ms.push(Json::obj([
                        ("cell", Json::str("easy")),
                        ("drawn", Json::str("overfull-hbox")),
                        ("state", Json::str("blocked{no_adjacent_donor}")),
                    ]));
                }
            }
        }
    }
    assert!(matches!(
        check_dev_not_held(&planted, &packet, &repo(&i.select_commit)),
        Err(PartitionError::HeldOut { .. })
    ));
}

/// AC-1 — a schema test rejects a dev projection containing a verdict
/// member; the committed projection carries the dev half only (no
/// gate-half task content — REQ-XH-121/X-07). The R6.41 runner
/// refusal is cited verbatim.
#[test]
fn ac1_dev_projection_boundary_and_verdict_refusal() {
    let (packet, record, proj) = committed();
    assert_eq!(
        proj.get("schema").and_then(Json::as_str),
        Some(DEV_PROJECTION_SCHEMA)
    );
    check_dev_projection(&proj).expect("committed dev projection passes the schema check");

    // the projection's record is the dev partition, stripped
    let dev = partition(&record, "dev");
    let stripped = dev_projection(dev);
    assert_eq!(proj.get("record"), stripped.get("record"));

    // no gate-half member content: no gate member spelling appears in
    // the projected record — only the binding digests
    let proj_bytes = proj.get("record").expect("record").to_canonical_string();
    for g in ["gate_a", "gate_b"] {
        for t in member_tasks(&record, g) {
            assert!(
                !proj_bytes.contains(&t),
                "gate-half task {t} leaked into the dev projection"
            );
        }
    }
    let gd = proj
        .get("gate_digests")
        .and_then(Json::as_obj)
        .expect("gate_digests");
    for g in ["gate_a", "gate_b"] {
        let want = partition(&record, g)
            .get("sha256")
            .and_then(Json::as_str)
            .expect("gate sha");
        assert_eq!(
            gd.get(g).and_then(Json::as_str),
            Some(want),
            "gate digest {g} binds the sealed half"
        );
    }

    // a dev projection containing a verdict member refuses
    let mut bad = proj.clone();
    if let Json::Obj(ref mut m) = bad {
        if let Some(Json::Obj(ref mut r)) = m.get_mut("record") {
            r.insert("verdict".to_string(), Json::str("executed{resolved}"));
        }
    }
    assert!(
        check_dev_projection(&bad).is_err(),
        "a verdict member in the dev projection refuses"
    );

    // the R6.41 runner refusal is cited (AC-1)
    let gc = record
        .get("gate_consumption")
        .and_then(Json::as_str)
        .expect("gate_consumption");
    assert!(
        gc.contains("withheld{l1_not_passed}"),
        "the runner's native-lane refusal is cited verbatim"
    );

    // pin's pending digest state is carried verbatim — never invented
    let pin = record
        .get("source")
        .and_then(|s| s.get("pin"))
        .expect("source.pin");
    let digests = pin
        .get("dataset_toml_digests")
        .and_then(Json::as_str)
        .expect("dataset_toml_digests");
    assert!(
        digests.starts_with("pending{w1_observe_ingest}"),
        "pin digest state verbatim"
    );
    let _ = packet;
}

/// AC-2 — the partition record is a cost-field input: `check_cost_only`
/// passes, and an outcome field anywhere in it refuses (the
/// REQ-XH-011 firewall). The committed declaration's closed member
/// set is the quote-input half — `scripts/check-fp-partition.sh`'s
/// outcome-field fixture leg.
#[test]
fn ac2_partition_is_cost_field_input() {
    let (_packet, record, proj) = committed();
    check_cost_only(&record).expect("the partition record carries cost fields only");
    check_cost_only(&proj).expect("the dev projection carries cost fields only");
    let mut bad = record.clone();
    if let Json::Obj(ref mut m) = bad {
        m.insert("outcome".to_string(), Json::str("executed{resolved}"));
    }
    assert!(
        check_cost_only(&bad).is_err(),
        "an outcome field in the partition input refuses"
    );
}

/// AC-3 — partitions are by task and the REQ-XD-106 version-bump
/// predicate refuses a rule change after a recorded gate run without
/// a version bump, and a rerun on a used gate partition.
#[test]
fn ac3_by_task_and_gate_run_version_bump() {
    // by task: every member is one (cell, slot) of the fp_dev7 draw —
    // the seeded draw's own partition_key carries cell+slot verbatim
    let (_packet, record, _proj) = committed();
    let mut cells = std::collections::BTreeSet::new();
    for name in PARTITION_NAMES {
        for m in partition(&record, name)
            .get("members")
            .and_then(JsonExt::as_arr)
            .expect("members")
        {
            let cell = m.get("cell").and_then(Json::as_str).expect("cell");
            let slot = m
                .get("slot")
                .and_then(|s| match s {
                    Json::Int(i) => Some(*i),
                    _ => None,
                })
                .expect("slot index");
            assert!(m.get("partition_key").and_then(Json::as_str).is_some());
            assert!(
                cells.insert((cell.to_string(), slot)),
                "one (cell, slot) per task"
            );
        }
    }
    assert_eq!(cells.len(), 7, "the 7 fp_dev7 slots partition exactly once");

    // the version-bump predicate (REQ-XD-106)
    let run = |p: &str, v: &str, r: &str| GateRun {
        partition: p.to_string(),
        canon_version: v.to_string(),
        rules_digest: r.to_string(),
    };
    let recorded = vec![run("gate_a", "hh-l1-canon/1", "sha256:rules-v1")];

    // a rerun on the used partition refuses — any version, any rules
    assert!(matches!(
        check_gate_run(
            &recorded,
            &run("gate_a", "hh-l1-canon/1", "sha256:rules-v1")
        ),
        Err(PartitionError::PartitionUsed { .. })
    ));
    assert!(matches!(
        check_gate_run(
            &recorded,
            &run("gate_a", "hh-l1-canon/2", "sha256:rules-v2")
        ),
        Err(PartitionError::PartitionUsed { .. })
    ));
    // a rule change under the unchanged version refuses
    assert!(matches!(
        check_gate_run(
            &recorded,
            &run("gate_b", "hh-l1-canon/1", "sha256:rules-v2")
        ),
        Err(PartitionError::CanonUnbumped { .. })
    ));
    // a rule change admits on a bumped version over the unused partition
    check_gate_run(
        &recorded,
        &run("gate_b", "hh-l1-canon/2", "sha256:rules-v2"),
    )
    .expect("bumped version + unused partition admits");
    // unchanged rules + unchanged version on the unused partition admits
    check_gate_run(
        &recorded,
        &run("gate_b", "hh-l1-canon/1", "sha256:rules-v1"),
    )
    .expect("a same-rules rerun on the unused partition admits");
    // dev never serves as a gate partition
    assert!(matches!(
        check_gate_run(&[], &run("dev", "hh-l1-canon/1", "sha256:rules-v1")),
        Err(PartitionError::GatePartition { .. })
    ));
}

/// The committed records' sidecars match the canonical bytes — the
/// digests the PR pushes are the file hashes (REQ-XD-106's "committed
/// by digest").
#[test]
fn sidecars_match_committed_bytes() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let i = inputs();
    for (record_path, sidecar_path) in [
        (i.partition.clone(), i.partition_sha256.clone()),
        (i.dev_projection.clone(), i.dev_projection_sha256.clone()),
    ] {
        let bytes = std::fs::read(root.join(&record_path)).expect("record bytes");
        let want = std::fs::read_to_string(root.join(&sidecar_path))
            .expect("sidecar")
            .split_whitespace()
            .next()
            .expect("sidecar hash")
            .to_string();
        assert_eq!(sha256_hex(&bytes), want, "{record_path} hash sidecar");
        // canonical bytes: parse + reserialize is byte-identical
        let j = hharness_wire::json::parse(std::str::from_utf8(&bytes).expect("utf8"))
            .expect("record parses");
        assert_eq!(
            j.to_canonical_string() + "\n",
            String::from_utf8(bytes).expect("utf8"),
            "{record_path} is canonical JSON"
        );
    }
}
