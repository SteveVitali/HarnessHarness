//! R6.23 acceptance battery (manifest 276; BINARY-SETS (SR-4) + ARCHIVE (b);
//! AR-20/ADR-0437): AC-1 load-and-verify + rebuild + slot digests +
//! digest_only (REQ-XC-330 / AC-5.2-5); AC-2 stub-shim export/load argv +
//! sha256 + elapsed + artifacts entries (AC-2); AC-3 upload receipt +
//! refusals (REQ-XH-102 / AC-4.10-3); AC-4 pre-upload scan tiers
//! (REQ-XH-103 / AC-4.10-4); AC-5 read-back + supersession (REQ-XH-104 /
//! AC-4.10-5); AC-6 ordered remediation (REQ-XH-107 / AC-4.10-8); AC-7
//! secrets sweep (REQ-XH-023). Offline — no podman/gcloud/hf, no network,
//! USD $0.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::carried::carried_rebuild_guard;
use hharness_runner::carried::{
    artifact_upload_decision, check_slot_identity, identity_h_set, image_id_normalize, run_export,
    run_load, set_digest, set_manifest, slot_identity, verify_image_id, ArmLane, BinarySetRecipe,
    CarriedSets, ExportPlan, ExportShape, LoadKind, LoadPlan, H_SET_NAME, INFRA_SET_MEMBERS,
    INFRA_SET_NAME,
};
use hharness_runner::disposition::{AdmissionReason, Disposition, WithheldReason};
use hharness_runner::facts::{map_slot_facts, SlotFacts};
use hharness_runner::json_ext::JsonExt;
use hharness_runner::receipt_index::validate_index;
use hharness_runner::slot_record::ExperimentSlot;
use hharness_wire::json::Json;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-runner-r623-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn sha64(c: char) -> String {
    std::iter::repeat(c).take(64).collect()
}

fn infra_members() -> BTreeMap<String, String> {
    INFRA_SET_MEMBERS
        .iter()
        .enumerate()
        .map(|(i, (n, _))| (n.to_string(), sha64(char::from(b'a' + i as u8))))
        .collect()
}

fn h_members() -> BTreeMap<String, String> {
    [
        ("hh_kernel", sha64('f')),
        ("hir", sha64('0')),
        ("compiled_artifacts", sha64('9')),
    ]
    .into_iter()
    .map(|(n, s)| (n.to_string(), s))
    .collect()
}

fn sets() -> CarriedSets {
    CarriedSets {
        infra_members: infra_members(),
        h_members: Some(h_members()),
        h_frozen_at: Some("G-L1".into()),
    }
}

// ── AC-1 (AC-5.2-5) — load-and-verify + rebuild + slot digests + digest_only ──

#[test]
fn ac1_mismatched_image_id_withholds_the_slot() {
    let want = format!("sha256:{}", sha64('1'));
    // a match verifies
    verify_image_id(&want, &want).unwrap();
    verify_image_id(&want, &sha64('1')).unwrap(); // bare .Id form normalizes
                                                  // a mismatch refuses the slot as withheld{admission:harness_layer_mismatch}
    let bad = format!("sha256:{}", sha64('2'));
    let reason = verify_image_id(&want, &bad).unwrap_err();
    assert_eq!(reason, AdmissionReason::HarnessLayerMismatch);
    let m = map_slot_facts(&SlotFacts {
        admission_refusal: Some(reason),
        ..SlotFacts::default()
    });
    assert_eq!(
        m.disposition,
        Disposition::Withheld {
            reason: WithheldReason::Admission(AdmissionReason::HarnessLayerMismatch)
        }
    );
    assert_eq!(
        m.disposition.as_str(),
        "withheld{admission:harness_layer_mismatch}"
    );
    // a RepoDigest is never an identity — refused both ways.
    assert!(image_id_normalize("docker.io/x/y@sha256:aaa").is_err());
    assert!(verify_image_id("docker.io/x/y@sha256:aaa", &want).is_err());
}

#[test]
fn ac1_rebuild_without_gate_record_refuses() {
    let entry = Json::obj([
        ("role", Json::str("layer_export")),
        ("name", Json::str("mini-2.4.6-task-a")),
        ("window", Json::str("W1")),
        ("image_id", Json::str(&format!("sha256:{}", sha64('7')))),
        ("path", Json::str("artifacts/mini-2.4.6-task-a.oci")),
        ("sha256", Json::str(&sha64('8'))),
        ("bytes", Json::Int(3)),
        ("upload", Json::str("archived")),
    ]);
    let index = Json::obj([("artifacts", Json::Arr(vec![entry]))]);
    // already carried + no gate record → refuse
    let e = carried_rebuild_guard(&index, "layer_export", "mini-2.4.6-task-a", None).unwrap_err();
    assert_eq!(e.rule, "carried_rebuild");
    // a gate record with a new participant/runner version admits
    let gate = Json::obj([
        ("gate", Json::str("PROV-Wc")),
        ("new_version", Json::str("mini-swe-agent@2.4.7")),
    ]);
    carried_rebuild_guard(&index, "layer_export", "mini-2.4.6-task-a", Some(&gate)).unwrap();
    // a gate record with no new_version refuses
    let thin = Json::obj([("gate", Json::str("PROV-Wc"))]);
    carried_rebuild_guard(&index, "layer_export", "mini-2.4.6-task-a", Some(&thin)).unwrap_err();
    // an uncarried artifact needs no gate
    carried_rebuild_guard(&index, "bundle", "mini-2.4.6", None).unwrap();
}

#[test]
fn ac1_w4_slot_set_digests_and_lanes() {
    let s = sets();
    // HH-native + V5 carry h_set frozen at G-L1; hosted does not.
    for lane in [ArmLane::HhNative, ArmLane::V5] {
        let m = slot_identity(&s, lane).unwrap();
        let infra = m.get("infrastructure").unwrap().as_obj().unwrap();
        assert_eq!(
            infra.get("set_digest").and_then(Json::as_str),
            Some(
                set_digest(INFRA_SET_NAME, &s.infra_members)
                    .unwrap()
                    .as_str()
            )
        );
        let h = m.get("h_set").unwrap().as_obj().unwrap();
        assert_eq!(h.get("frozen_at").and_then(Json::as_str), Some("G-L1"));
        assert_eq!(
            h.get("digest").and_then(Json::as_str),
            Some(
                set_digest(H_SET_NAME, s.h_members.as_ref().unwrap())
                    .unwrap()
                    .as_str()
            )
        );
        // the slot record codec admits the members.
        check_slot_identity(&Json::Obj(m.clone()), &s, lane).unwrap();
    }
    let hosted = slot_identity(&s, ArmLane::Hosted).unwrap();
    assert!(hosted.get("h_set").is_none());
    check_slot_identity(&Json::Obj(hosted), &s, ArmLane::Hosted).unwrap();
    // an H lane with h_set absent refuses; a hosted lane carrying one refuses.
    let mut e = check_slot_identity(
        &Json::obj([("infrastructure", identity_infra(&s))]),
        &s,
        ArmLane::HhNative,
    )
    .unwrap_err();
    assert_eq!(e.rule, "slot_identity");
    let mut hosted_with_h = slot_identity(&s, ArmLane::Hosted).unwrap();
    hosted_with_h.insert("h_set".to_string(), identity_h_set(&s).unwrap());
    e = check_slot_identity(&Json::Obj(hosted_with_h), &s, ArmLane::Hosted).unwrap_err();
    assert_eq!(e.rule, "slot_identity");
    // the full record validates through the strict codec.
    let mut id = base_identity();
    for (k, v) in slot_identity(&s, ArmLane::HhNative).unwrap() {
        id.insert(k, v);
    }
    let slot = experiment_slot(Json::Obj(id));
    ExperimentSlot::from_json(&slot).unwrap();
    // a wrong h_set member name refuses the codec.
    let mut bad = slot.clone();
    let hm = obj_mut(&mut bad);
    let hs = obj_mut(hm.get_mut("identity").unwrap());
    obj_mut(hs.get_mut("h_set").unwrap()).insert("bogus".into(), Json::str("x"));
    assert!(ExperimentSlot::from_json(&bad).is_err());
}

fn identity_infra(s: &CarriedSets) -> Json {
    slot_identity(s, ArmLane::Hosted)
        .unwrap()
        .get("infrastructure")
        .unwrap()
        .clone()
}

fn base_identity() -> BTreeMap<String, Json> {
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
    ])
    .as_obj()
    .unwrap()
    .clone()
}

fn experiment_slot(identity: Json) -> Json {
    Json::obj([
        ("schema", Json::str("hh-experiment-slot/1")),
        ("identity", identity),
        ("task", Json::obj([])),
        ("participant", Json::obj([])),
        ("cell", Json::obj([])),
        ("phases", Json::Arr(vec![])),
        (
            "disposition",
            Json::obj([("value", Json::str("executed{resolved}"))]),
        ),
        ("integrity", Json::obj([("seal", Json::str("sealed"))])),
        ("trajectory", Json::obj([])),
        ("costs", Json::obj([])),
    ])
}

#[test]
fn ac1_binary_set_manifest_and_recipe() {
    let m = set_manifest(INFRA_SET_NAME, &infra_members()).unwrap();
    assert_eq!(
        m.get("set_digest").and_then(Json::as_str),
        Some(
            set_digest(INFRA_SET_NAME, &infra_members())
                .unwrap()
                .as_str()
        )
    );
    for (n, _) in INFRA_SET_MEMBERS {
        assert!(m
            .get("members")
            .and_then(|x| x.get(n))
            .and_then(|x| x.get("binary_sha256"))
            .is_some());
    }
    // a member outside the closed set / a missing member refuses.
    let mut bad = infra_members();
    bad.insert("mystery".into(), sha64('z'));
    assert!(set_manifest(INFRA_SET_NAME, &bad).is_err());
    let mut short = infra_members();
    short.remove("relay");
    assert!(set_manifest(INFRA_SET_NAME, &short).is_err());
    // the recipe: FROM digest only, locked cargo build, member copies, manifest.
    let recipe = BinarySetRecipe {
        set: INFRA_SET_NAME.into(),
        toolchain_base: "docker.io/library/rust@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        workspace_commit: "0123456789abcdef0123456789abcdef01234567".into(),
        packages: vec![
            "hharness-runner".into(),
            "hharness-helper".into(),
            "hharness-env".into(),
            "hharness-hosting".into(),
        ],
        extra_outputs: vec![],
    };
    let cf = recipe.render().unwrap();
    assert!(cf.contains("FROM docker.io/library/rust@sha256:aaaa"));
    assert!(cf.contains("cargo build --release --locked -p hharness-runner -p hharness-helper -p hharness-env -p hharness-hosting"));
    assert!(cf.contains(
        "cp -r 'target/release/hh-model-proxy' '/opt/hh-set-infrastructure/model_proxy'"
    ));
    assert!(
        cf.contains("sha256sum runner helper relay model_proxy egress_mediator > MANIFEST.sha256")
    );
    // tag-pinned toolchain refuses.
    let mut bad = recipe.clone();
    bad.toolchain_base = "rust:1.94".into();
    assert!(bad.render().is_err());
    // the H recipe stages the extra outputs.
    let h = BinarySetRecipe {
        set: H_SET_NAME.into(),
        toolchain_base: recipe.toolchain_base.clone(),
        workspace_commit: recipe.workspace_commit.clone(),
        packages: vec!["hharness-kernel".into(), "hharness-compiler".into()],
        extra_outputs: vec!["hh-compile emit --level phase-4 -o artifacts/compiled".into()],
    }
    .render()
    .unwrap();
    assert!(h.contains("cp -r 'target/release/hh-kernel' '/opt/hh-set-h/hh_kernel'"));
    assert!(h.contains("cp -r 'artifacts/hir' '/opt/hh-set-h/hir'"));
}

#[test]
fn ac1_proprietary_bundle_is_digest_only() {
    // a Claude Code bundle — proprietary — is digest_only, no uploaded bytes.
    assert_eq!(artifact_upload_decision("proprietary", None), "digest_only");
    assert_eq!(
        artifact_upload_decision("source_available", None),
        "digest_only"
    );
    assert_eq!(artifact_upload_decision("open", None), "archived");
    // an operator redistribution record admits the upload.
    assert_eq!(
        artifact_upload_decision("proprietary", Some("op-redist-record/claude-code-2.1.50")),
        "archived"
    );
    // the archive fixture: digest_only entry, no path.
    let plan = export_plan(ExportShape::PodmanSave, "bundle", "claude-code-2.1.50");
    let e = plan.index_entry(b"bundle", "digest_only", None).unwrap();
    assert_eq!(e.get("upload").and_then(Json::as_str), Some("digest_only"));
    assert_eq!(e.get("path"), Some(&Json::Null));
    let index = Json::obj([
        ("schema", Json::str("hh-receipt-index/1")),
        (
            "dataset",
            Json::obj([
                ("repo", Json::str("harnessharness/receipts")),
                ("revision", Json::str("r1")),
                ("visibility", Json::str("private")),
            ]),
        ),
        ("attempts", Json::Arr(vec![])),
        ("artifacts", Json::Arr(vec![e])),
        ("scans", Json::obj([])),
    ]);
    validate_index(&index).unwrap();
}

fn export_plan(shape: ExportShape, role: &str, name: &str) -> ExportPlan {
    let id = match role {
        "layer_export" => ("image_id", format!("sha256:{}", sha64('9'))),
        _ => ("digest", format!("sha256:{}", sha64('9'))),
    };
    ExportPlan {
        shape,
        role: role.into(),
        name: name.into(),
        window: "W1".into(),
        image: Some(
            "docker.io/hh/layer@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .into(),
        ),
        base_blobs: vec![],
        inputs: vec![],
        context_dir: String::new(),
        out_path: String::new(),
        identity: (id.0.to_string(), id.1),
    }
}

// ── AC-2 — stub podman/gcloud argv + sha256 + elapsed + artifacts entries ──

/// A `podman` stub: logs argv to `$HH_PODMAN_LOG`; `save -o <path>`
/// writes canned bytes (oci-archive) or a canned dir (oci-dir).
fn podman_stub(dir: &Path) -> PathBuf {
    let bin = dir.join("podman");
    std::fs::write(
        &bin,
        "#!/bin/sh\necho \"podman $@\" >> \"$HH_PODMAN_LOG\"\nprev=\"\"\nfor a in \"$@\"; do\n  if [ \"$prev\" = \"-o\" ]; then\n    if [ \"$3\" = \"oci-dir\" ]; then mkdir -p \"$a\"; printf 'x' > \"$a/manifest.json\"; printf 'y' > \"$a/blob1\"; else printf 'canned-oci-archive' > \"$a\"; fi\n  fi\n  prev=\"$a\"\ndone\nexit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

/// A `gcloud` stub: logs argv to `$HH_GCLOUD_LOG`; `compute scp` copies
/// the source arg to the dest's path half; `compute ssh --command` logs
/// and prints `$HH_SHIM_IMAGE_ID` for inspect commands.
fn gcloud_stub(dir: &Path) -> PathBuf {
    let bin = dir.join("gcloud");
    std::fs::write(
        &bin,
        "#!/bin/sh\necho \"gcloud $@\" >> \"$HH_GCLOUD_LOG\"\ncase \"$*\" in\n  *\"compute scp\"*) exit 0 ;;\n  *\"compute ssh\"*)\n    case \"$*\" in *\"image inspect\"*) printf '%s\\n' \"$HH_SHIM_IMAGE_ID\" ;; esac\n    exit 0 ;;\nesac\nexit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

/// A `tar` stub: logs argv, writes canned bytes to the `-cf` target.
fn tar_stub(dir: &Path) -> PathBuf {
    let bin = dir.join("tar");
    std::fs::write(
        &bin,
        "#!/bin/sh\necho \"tar $@\" >> \"$HH_TAR_LOG\"\nprev=\"\"\nfor a in \"$@\"; do if [ \"$prev\" = \"-cf\" ]; then printf 'canned-set-archive' > \"$a\"; fi; prev=\"$a\"; done\nexit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

fn with_stub_path<R>(dir: &Path, body: impl FnOnce() -> R) -> R {
    let key = "PATH";
    let old = std::env::var(key).unwrap_or_default();
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap();
    std::env::set_var(key, format!("{}:{}", dir.display(), old));
    let r = body();
    std::env::set_var(key, old);
    r
}

fn obj_mut(j: &mut Json) -> &mut BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m,
        _ => panic!("expected object"),
    }
}

#[test]
fn ac2_export_pinned_argv_sha256_elapsed_and_index_entries() {
    let d = tmp("export-argv");
    let _p = podman_stub(&d);
    let _t = tar_stub(&d);
    let img = "docker.io/hh/layer@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    // podman save — a layer_export entry.
    let mut plan = export_plan(ExportShape::PodmanSave, "layer_export", "mini-2.4.6-task-a");
    plan.out_path = d.join("layer.oci").display().to_string();
    plan.image = Some(img.into());
    let rec = with_stub_path(&d, || {
        std::env::set_var("HH_PODMAN_LOG", d.join("podman.log"));
        std::env::set_var("HH_TAR_LOG", d.join("tar.log"));
        let r = run_export(&plan, &d.join("export.log")).unwrap();
        std::env::remove_var("HH_PODMAN_LOG");
        r
    });
    let plog = std::fs::read_to_string(d.join("podman.log")).unwrap();
    assert!(
        plog.contains(&format!(
            "podman save --format oci-archive -o {} {}",
            plan.out_path, img
        )),
        "{plog}"
    );
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some("hh-carried-export/1")
    );
    assert!(rec.get("elapsed_ms").and_then(Json::as_int).is_some());
    assert_eq!(
        rec.get("sha256").and_then(Json::as_str),
        Some(hharness_wire::sha256::sha256_hex(b"canned-oci-archive").as_str())
    );
    // the artifacts entry validates — image_id identity, never a RepoDigest.
    let e = plan
        .index_entry(
            b"canned-oci-archive",
            "archived",
            Some("artifacts/layer.oci"),
        )
        .unwrap();
    assert_eq!(e.get("role").and_then(Json::as_str), Some("layer_export"));
    assert!(e.get("image_id").is_some());
    assert!(e
        .get("image_id")
        .and_then(Json::as_str)
        .unwrap()
        .starts_with("sha256:"));
    // tar — a binary_set entry (digest identity).
    let mut bp = export_plan(ExportShape::ArchiveTar, "binary_set", "infrastructure");
    bp.inputs = vec!["runner".into(), "helper".into()];
    bp.context_dir = d.display().to_string();
    bp.out_path = d.join("infra.tar").display().to_string();
    let brec = with_stub_path(&d, || {
        std::env::set_var("HH_TAR_LOG", d.join("tar.log"));
        run_export(&bp, &d.join("tarexport.log")).unwrap()
    });
    let tlog = std::fs::read_to_string(d.join("tar.log")).unwrap();
    assert!(
        tlog.contains(&format!(
            "tar -cf {} -C {} runner helper",
            bp.out_path, bp.context_dir
        )),
        "{tlog}"
    );
    assert_eq!(
        brec.get("sha256").and_then(Json::as_str),
        Some(hharness_wire::sha256::sha256_hex(b"canned-set-archive").as_str())
    );
    let be = bp
        .index_entry(
            b"canned-set-archive",
            "archived",
            Some("artifacts/infra.tar"),
        )
        .unwrap();
    assert_eq!(be.get("role").and_then(Json::as_str), Some("binary_set"));
    assert!(be.get("digest").is_some());
    // every role lands in a validator-clean index.
    let bundle = export_plan(ExportShape::PodmanSave, "bundle", "mini-2.4.6");
    let idx = Json::obj([
        ("schema", Json::str("hh-receipt-index/1")),
        (
            "dataset",
            Json::obj([
                ("repo", Json::str("hh/receipts")),
                ("revision", Json::str("r1")),
                ("visibility", Json::str("private")),
            ]),
        ),
        ("attempts", Json::Arr(vec![])),
        (
            "artifacts",
            Json::Arr(vec![
                e,
                be,
                bundle
                    .index_entry(b"b", "archived", Some("artifacts/bundle.oci"))
                    .unwrap(),
            ]),
        ),
        ("scans", Json::obj([])),
    ]);
    validate_index(&idx).unwrap();
    // the egress line is a labelled REQ-XH-118 input.
    assert_eq!(
        plan.egress_line(123).get("label").and_then(Json::as_str),
        Some("artifact_export:layer_export:mini-2.4.6-task-a")
    );
    // a RepoDigest as the identity member refuses at plan argv.
    let mut rp = export_plan(ExportShape::PodmanSave, "layer_export", "x");
    rp.identity = (
        "image_id".to_string(),
        format!("docker.io/hh/layer@sha256:{}", sha64('9')),
    );
    assert!(rp.argv().is_err());
}

#[test]
fn ac2_load_over_iap_pinned_argv_and_image_id_verify() {
    let d = tmp("load-argv");
    let _g = gcloud_stub(&d);
    let want = format!("sha256:{}", sha64('7'));
    let plan = LoadPlan {
        name: "mini-2.4.6-task-a".into(),
        source: d.join("layer.oci").display().to_string(),
        instance: "hh-p1-w4".into(),
        project: "hh-proj".into(),
        zone: "us-central1-a".into(),
        remote_dir: "/var/lib/hh/artifacts".into(),
        kind: LoadKind::PodmanLoad,
        base_blobs: vec![
            "docker.io/alexgshaw/task-base-a@sha256:1111111111111111111111111111111111111111111111111111111111111111".into(),
        ],
        image_ref: Some("hh-carried/mini:loaded".into()),
        expected_image_id: Some(want.clone()),
        dest_dir: String::new(),
    };
    let scp = plan.scp_argv();
    assert_eq!(
        scp,
        vec![
            "compute",
            "scp",
            "--tunnel-through-iap",
            "--project",
            "hh-proj",
            "--zone",
            "us-central1-a",
            plan.source.as_str(),
            "hh-p1-w4:/var/lib/hh/artifacts/layer.oci"
        ]
    );
    let cmds = plan.vm_commands();
    assert_eq!(cmds[0], "podman load -i '/var/lib/hh/artifacts/layer.oci'");
    assert_eq!(cmds[1], "podman pull 'docker.io/alexgshaw/task-base-a@sha256:1111111111111111111111111111111111111111111111111111111111111111'");
    assert_eq!(
        cmds[2],
        "podman image inspect --format '{{.Id}}' 'hh-carried/mini:loaded'"
    );
    let rec = with_stub_path(&d, || {
        std::env::set_var("HH_GCLOUD_LOG", d.join("gcloud.log"));
        std::env::set_var("HH_SHIM_IMAGE_ID", &want);
        run_load(&plan, &d.join("load.log")).unwrap()
    });
    let glog = std::fs::read_to_string(d.join("gcloud.log")).unwrap();
    assert!(glog.contains("compute scp --tunnel-through-iap"), "{glog}");
    assert!(glog.contains("compute ssh hh-p1-w4 --project hh-proj --zone us-central1-a --tunnel-through-iap --command podman load"), "{glog}");
    assert!(glog.contains("podman pull"), "{glog}");
    assert!(glog.contains("podman image inspect"), "{glog}");
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some("hh-carried-load/1")
    );
    assert_eq!(
        rec.get("image_id").and_then(Json::as_str),
        Some(want.as_str())
    );
    assert_eq!(rec.get("verified").and_then(Json::as_bool), Some(true));
    assert!(rec.get("elapsed_ms").and_then(Json::as_int).is_some());
    // a mismatched image ID refuses the load typed.
    let bad = with_stub_path(&d, || {
        std::env::set_var("HH_GCLOUD_LOG", d.join("g2.log"));
        std::env::set_var("HH_SHIM_IMAGE_ID", format!("sha256:{}", sha64('8')));
        run_load(&plan, &d.join("load2.log")).unwrap_err()
    });
    assert_eq!(bad.rule, "load.image_id_mismatch");
}
