//! R6.17 acceptance battery (manifest 270; HARBOR-O + REPROOF): the
//! pin record + Q-X41 branch + AR-04 carriage (AC-1), the venv recipe +
//! lock-hash refusal (AC-2), the committed job config + read-only
//! ingest (AC-3), image identity + tree digests (AC-4), `skip_install`
//! / the not-image-identical label (AC-5), the re-proof driver's
//! outcome classes + append-only register (AC-6), the pinned label +
//! committed parity refusal + live-evidence gate (AC-7) and the REQ-
//! XB-110 fault-injection leg (deliverable 6). The `reproof` leg's
//! registry half is exercised by `scripts/ci/test-live-verify.sh`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::harbor::{
    check_task_digests, image_identical_label, ingest_job, task_tree_digest, BlobSink,
    HarborIngest, HarborPin, ImagePin, JobConfig, SkipInstall, VenvRecipe, HARBOR_PIN,
    PENDING_FIRST_BUILD, V0_25_0_COMMIT,
};
use hharness_runner::packet::SlotPacket;
use hharness_runner::slot_record::ExperimentSlot;
use hharness_wire::json::{self, Json};

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-runner-r617-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn pin() -> HarborPin {
    HarborPin::admitted(
        "2026-11-07T22:52Z",
        "2bc1a23bd961de0bec8742940f35ac9070c67815",
    )
}

struct MapSink {
    blobs: BTreeMap<String, Vec<u8>>,
}

impl BlobSink for MapSink {
    fn put(
        &mut self,
        bytes: &[u8],
        _mime: &str,
    ) -> Result<String, hharness_runner::harbor::HarborError> {
        let r = format!("sha256:{}", hharness_wire::sha256::sha256_hex(bytes));
        self.blobs.insert(r.clone(), bytes.to_vec());
        Ok(r)
    }
}

fn hash_tree(dir: &Path) -> String {
    task_tree_digest(dir).unwrap()
}

// ── AC-1 — the pin record + its carriers ─────────────────────────────────────

#[test]
fn ac1_pin_record_names_branch_and_live_read() {
    let committed =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/r6_17/harbor-pin.json");
    let text = std::fs::read_to_string(&committed).unwrap();
    let j = json::parse(&text).unwrap();
    let p = HarborPin::from_json(&j).unwrap();
    assert_eq!(p.pin, HARBOR_PIN);
    assert_eq!(p.branch.as_str(), "commit_fallback");
    assert_eq!(p.live_read.result, "no_stable_tag_contains_skip_install");
    assert_eq!(p.live_read.newest_stable_tag.as_deref(), Some("v0.25.0"));
    assert_eq!(
        p.live_read.newest_stable_tag_commit.as_deref(),
        Some(V0_25_0_COMMIT)
    );
    assert!(!p.diff_required());
    // The emitted record round-trips (canonical — compare decoded, the
    // committed file may carry a trailing newline).
    assert_eq!(HarborPin::from_json(&p.to_json()).unwrap(), p);
    // A tag later than v0.25.0 would require the E1 F13 diff.
    let mut later = p.clone();
    later.kind = "tag".into();
    later.pin = "v0.26.0".into();
    assert!(later.diff_required());
}

#[test]
fn ac1_slot_and_trial_records_carry_the_pin() {
    // Slot record: `identity.harbor.pin` strictly checked.
    let mut slot: ExperimentSlot = bare_slot();
    if let Json::Obj(ref mut id) = slot.identity {
        id.insert("harbor".into(), Json::obj([("pin", Json::str(HARBOR_PIN))]));
    }
    let rt = ExperimentSlot::from_json(&slot.to_json()).unwrap();
    assert_eq!(
        rt.identity
            .get("harbor")
            .unwrap()
            .get("pin")
            .unwrap()
            .as_str(),
        Some(HARBOR_PIN)
    );
    // An unknown member under identity.harbor refuses.
    let mut bad = bare_slot();
    if let Json::Obj(ref mut id) = bad.identity {
        id.insert(
            "harbor".into(),
            Json::obj([("pin", Json::str(HARBOR_PIN)), ("bogus", Json::str("x"))]),
        );
    }
    assert!(ExperimentSlot::from_json(&bad.to_json()).is_err());
    // trial records carry `harbor_pin` — proven inside the ingest leg.
    let (ingest, _) = fixture_job(&pin());
    assert!(ingest.trials.iter().all(|t| t.harbor_pin == HARBOR_PIN));
    let tj = ingest.trials[0].to_json();
    assert_eq!(tj.get("harbor_pin").unwrap().as_str(), Some(HARBOR_PIN));
}

fn bare_slot() -> ExperimentSlot {
    ExperimentSlot {
        identity: Json::obj([
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
                    ("root_digest", Json::str("r")),
                    ("run_id", Json::str("s")),
                    ("head_hash", Json::str("h")),
                ]),
            ),
            (
                "runner",
                Json::obj([
                    ("driver_commit", Json::str("x")),
                    ("binary_sha256", Json::str("y")),
                ]),
            ),
        ]),
        task: Json::obj([]),
        participant: Json::obj([]),
        cell: Json::obj([]),
        phases: vec![],
        verdict: None,
        disposition: hharness_runner::disposition::Disposition::Executed {
            resolved: true,
            end_cause: hharness_runner::disposition::EndCause::Exited(0),
        },
        prior_disposition: None,
        repairs: vec![],
        integrity: hharness_runner::slot_record::SlotIntegrity {
            seal: "sealed".into(),
            seal_ref: None,
            sentinel_intact: Some(true),
            observer_survived: Some(true),
            probe: None,
        },
        trajectory: BTreeMap::new(),
        costs: hharness_runner::slot_record::SlotCosts::default(),
    }
}

// ── AC-2 — the venv recipe + lock-hash refusal ────────────────────────────────

#[test]
fn ac2_venv_record_lock_hash_and_litellm() {
    let lock = b"[fixture uv.lock bytes]\n";
    let v = VenvRecipe::built(HARBOR_PIN, lock, "litellm==1.2.3");
    let j = v.to_json();
    assert!(j
        .get("uv_lock_sha256")
        .unwrap()
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert_eq!(
        j.get("resolved_litellm").unwrap().as_str(),
        Some("litellm==1.2.3")
    );
    assert_eq!(j.get("command").unwrap().as_str(), Some("uv sync --frozen"));
    v.check_lock(lock).unwrap();
    // A fixture lock whose hash differs refuses.
    assert!(v.check_lock(b"other bytes").is_err());
    // The declared recipe is honest pending.
    let d = VenvRecipe::declared(HARBOR_PIN);
    assert_eq!(d.uv_lock_sha256, PENDING_FIRST_BUILD);
    assert!(d.check_lock(lock).is_err());
}

// ── AC-3 — the committed job config + read-only ingest ────────────────────────

#[test]
fn ac3_job_config_golden_and_upload_refusal() {
    let cfg = JobConfig::new(
        &pin(),
        "w1-reproof-e0",
        "tasks",
        vec!["hello-world".into()],
        "oracle",
        3,
        2,
        "jobs",
        Some(SkipInstall {
            layer_digest: "sha256:layer".into(),
        }),
    )
    .unwrap();
    let j = cfg.to_json();
    assert_eq!(j.get("max_retries").unwrap().as_int(), Some(0));
    let argv = cfg.to_argv("harbor").unwrap();
    assert!(argv.contains(&"-r".to_string()) && argv.contains(&"0".to_string()));
    assert!(argv.contains(&"--no-force-build".to_string()));
    assert!(argv.contains(&"skip_install=true".to_string()));
    assert!(!argv.iter().any(|a| a == "--upload" || a == "--public"));
    // An argv carrying --upload refuses.
    let mut bad = argv.clone();
    bad.push("--upload".into());
    assert!(JobConfig::check_argv(&bad).is_err());
    // A decoded config with max_retries != 0 refuses (golden enforced).
    let mut m = match cfg.to_json() {
        Json::Obj(m) => m,
        _ => panic!(),
    };
    m.insert("max_retries".into(), Json::Int(1));
    assert!(JobConfig::from_json(&Json::Obj(m)).is_err());
    // skip_install at a pin lacking the knob refuses.
    let mut nopin = pin();
    nopin.contains_skip_install = false;
    assert!(JobConfig::new(
        &nopin,
        "j",
        "tasks",
        vec![],
        "oracle",
        1,
        1,
        "jobs",
        Some(SkipInstall {
            layer_digest: "sha256:x".into()
        })
    )
    .is_err());
}

fn fixture_job(pin: &HarborPin) -> (HarborIngest, PathBuf) {
    let root = tmp("job-fixture");
    let job = root.join("w1-e0");
    let trial = job.join("hello-world__abc1234");
    std::fs::create_dir_all(trial.join("verifier")).unwrap();
    std::fs::create_dir_all(trial.join("agent")).unwrap();
    std::fs::write(job.join("config.json"), b"{}").unwrap();
    std::fs::write(job.join("lock.json"), b"{}").unwrap();
    std::fs::write(job.join("result.json"), b"{}").unwrap();
    std::fs::write(
        trial.join("result.json"),
        br#"{"cause":"sandbox_failed","attempt_index":2}"#,
    )
    .unwrap();
    std::fs::write(trial.join("verifier").join("reward.txt"), b"0").unwrap();
    std::fs::write(
        trial.join("verifier").join("ctrf.json"),
        br#"{"results":{"summary":{"tests":3,"failed":3}}}"#,
    )
    .unwrap();
    std::fs::write(trial.join("verifier").join("test-stdout.txt"), b"out").unwrap();
    std::fs::write(trial.join("exception.txt"), b"boom").unwrap();
    std::fs::write(
        trial.join("agent").join("trajectory.json"),
        br#"{"atif_version":"ATIF-v1.8","steps":[]}"#,
    )
    .unwrap();
    let cfg = JobConfig::new(
        pin,
        "w1-e0",
        "tasks",
        vec!["hello-world".into()],
        "oracle",
        3,
        2,
        root.to_str().unwrap(),
        None,
    )
    .unwrap();
    let mut sink = MapSink {
        blobs: BTreeMap::new(),
    };
    let ingest = ingest_job(&root, "w1-e0", &cfg, pin, &mut sink).unwrap();
    (ingest, root)
}

#[test]
fn ac3_readonly_ingest_trial_members() {
    let p = pin();
    let root = tmp("job-ac3");
    let job = root.join("w1-e0");
    let trial = job.join("hello-world__abc1234");
    std::fs::create_dir_all(trial.join("verifier")).unwrap();
    std::fs::create_dir_all(trial.join("agent")).unwrap();
    std::fs::write(job.join("config.json"), b"{}").unwrap();
    std::fs::write(job.join("lock.json"), b"{}").unwrap();
    std::fs::write(job.join("result.json"), b"{}").unwrap();
    std::fs::write(
        trial.join("result.json"),
        br#"{"cause":"sandbox_failed","attempt_index":2}"#,
    )
    .unwrap();
    std::fs::write(trial.join("verifier").join("reward.txt"), b"0").unwrap();
    std::fs::write(
        trial.join("verifier").join("ctrf.json"),
        br#"{"results":{"summary":{"tests":3,"failed":3}}}"#,
    )
    .unwrap();
    std::fs::write(trial.join("verifier").join("test-stdout.txt"), b"out").unwrap();
    std::fs::write(trial.join("exception.txt"), b"boom").unwrap();
    std::fs::write(
        trial.join("agent").join("trajectory.json"),
        br#"{"atif_version":"ATIF-v1.8","steps":[]}"#,
    )
    .unwrap();
    let before = hash_tree(&job);
    let cfg = JobConfig::new(
        &p,
        "w1-e0",
        "tasks",
        vec!["hello-world".into()],
        "oracle",
        3,
        2,
        root.to_str().unwrap(),
        None,
    )
    .unwrap();
    let mut sink = MapSink {
        blobs: BTreeMap::new(),
    };
    let ingest = ingest_job(&root, "w1-e0", &cfg, &p, &mut sink).unwrap();
    // Byte-unchanged job tree (read-only walk).
    assert_eq!(hash_tree(&job), before);
    // Per-file sha256 + blob-pool copies: every file's bytes landed.
    for (rel, fd) in &ingest.files {
        let bytes = std::fs::read(job.join(rel)).unwrap();
        assert_eq!(
            fd.sha256,
            format!("sha256:{}", hharness_wire::sha256::sha256_hex(&bytes))
        );
        assert!(sink.blobs.contains_key(&fd.blob_ref));
    }
    assert_eq!(ingest.job_files.len(), 3);
    // The committed runner-written config landed in the record.
    assert_eq!(
        ingest
            .job_config
            .to_json()
            .get("max_retries")
            .unwrap()
            .as_int(),
        Some(0)
    );
    assert!(sink.blobs.contains_key(&ingest.job_config_ref));
    // One hh-harbor-trial/1 with every named member.
    let t = &ingest.trials[0];
    assert_eq!(t.trial, "hello-world__abc1234");
    assert_eq!(t.task, "hello-world");
    assert_eq!(t.attempt_index, Some(2));
    assert!(t.reward_files.contains_key("reward.txt"));
    assert!(t.ctrf.is_some());
    assert!(t.test_stdout.is_some());
    assert!(t.exception.is_some());
    let (_, atif) = t.trajectory.as_ref().unwrap();
    assert_eq!(atif.as_deref(), Some("ATIF-v1.8"));
    // The pinned cause map: sandbox_failed → inconclusive{infra:…}
    // (the failed trial is preserved; a re-run is a new HH slot).
    assert_eq!(t.cause.as_deref(), Some("sandbox_failed"));
    assert!(t.cause_mapped);
    assert_eq!(t.disposition, "inconclusive{infra:sandbox_failed}");
}

// ── AC-4 — image identity + tree digests ─────────────────────────────────────

#[test]
fn ac4_drift_withholds_and_tree_digest_refuses() {
    let img = ImagePin::new(
        "hello-world",
        "registry.test/task",
        "tb21-hello",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "linux/amd64",
    )
    .unwrap();
    let argv = img.prepull_argv("podman");
    assert_eq!(argv[0][2], "registry.test/task@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert_eq!(argv[1][3], "registry.test/task:tb21-hello");
    // Post-job: the tag still resolves to the pin → ok; drift → withheld.
    img.post_job_assert("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .unwrap();
    let drift = img
        .post_job_assert("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        .unwrap_err();
    assert_eq!(drift.disposition().as_str(), "withheld{image_drift}");
    // A tag-only image reference refuses at admission.
    assert!(ImagePin::new("t", "r", "tag", "tb21-hello", "linux/amd64").is_err());
    // Tree digests vs dataset.toml before any slot.
    let dir = tmp("task");
    std::fs::write(dir.join("task.toml"), b"x").unwrap();
    std::fs::write(dir.join("instruction.md"), b"do it").unwrap();
    let digest = task_tree_digest(&dir).unwrap();
    let mut declared = BTreeMap::new();
    declared.insert("hello-world".to_string(), digest.clone());
    let mut computed = BTreeMap::new();
    computed.insert("hello-world".to_string(), digest);
    check_task_digests(&declared, &computed).unwrap();
    let mism = check_task_digests(&declared, &BTreeMap::new()).unwrap_err();
    assert_eq!(
        mism.disposition().as_str(),
        "withheld{admission:task_digest_mismatch}"
    );
}

// ── AC-5 — skip_install / the not-image-identical label ──────────────────────

#[test]
fn ac5_skip_install_and_label() {
    let p = pin();
    assert_eq!(image_identical_label(&p), "image-identical");
    let cfg = JobConfig::new(
        &p,
        "j",
        "tasks",
        vec!["t".into()],
        "oracle",
        1,
        1,
        "jobs",
        Some(SkipInstall {
            layer_digest: "sha256:layer".into(),
        }),
    )
    .unwrap();
    let j = cfg.to_json();
    let si = j.get("skip_install").unwrap();
    assert_eq!(si.get("enabled").unwrap(), &Json::Bool(true));
    assert_eq!(
        si.get("layer_digest").unwrap().as_str(),
        Some("sha256:layer")
    );
    // A pin without skip_install → every runner check labelled.
    let mut nopin = p.clone();
    nopin.contains_skip_install = false;
    assert_eq!(image_identical_label(&nopin), "not-image-identical");
}

// Packet plumbing: `harbor` member carries the pin into slot identity.
#[test]
fn packet_harbor_member_round_trips() {
    let mut sp = SlotPacket {
        run_plan_id: "rp".into(),
        subject_run_id: "s".into(),
        attempt_no: 1,
        experiment_id: "e".into(),
        protocol_id: "p".into(),
        selection_id: "sel".into(),
        cell_id: "c".into(),
        arm_id: "a".into(),
        task_id: "t".into(),
        replicate_index: 0,
        split_label: "split".into(),
        slot_root: "slots/rp-a1".into(),
        slot_run_id: "slot".into(),
        task: BTreeMap::new(),
        participant: BTreeMap::new(),
        cell: BTreeMap::new(),
        harbor: Some(HARBOR_PIN.into()),
        recipe: hharness_runner::packet::SlotRecipe::Cancelled,
    };
    let rt = SlotPacket::from_json(&sp.to_json()).unwrap();
    assert_eq!(rt.harbor.as_deref(), Some(HARBOR_PIN));
    sp.harbor = None;
    let rt2 = SlotPacket::from_json(&sp.to_json()).unwrap();
    assert_eq!(rt2.harbor, None);
}
