//! R6.45k acceptance battery (manifest 309k) — the conformance
//! machinery legs the conformance harness + D16 scan stand on:
//! `hh-w1 render-policy` emits the sealed phase layer through the same
//! `admit_allowlist_mediated` + `phase_schedule` path `h-run` uses
//! (AC: byte-identity vs the in-test render — never a re-derivation),
//! and `hh-w1 scan-hosts` runs the committed `scan_hosts_in_text` over
//! the members' `tests/*` (the D16 derivation input — the
//! pytorch-index host the epoch-3 freeze misses). Both fail on
//! pre-fix code: neither subcommand existed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use hharness_env::egress_allowlist::{
    admit_allowlist_mediated, scan_hosts_in_text, Allowlist, ImageFamily, MediatedCell,
    MediatedDecl, RelayPair, EGRESS_RELAY_PORT, EGRESS_SOCK_PATH, MODEL_SOCK_PATH,
};
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::{self, Json};

const W1: &str = env!("CARGO_BIN_EXE_hh_w1");
/// The epoch-3 freeze as a byte-copy fixture under `tests/fixtures/` —
/// the CI readers gate bars gated test sources from reading committed
/// documentation content; the copy is byte-identical to the canonical
/// build-reports record (same sha256 as its committed `.sha256`,
/// pinned below — append-only record, never edited in place).
fn epoch3() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/r6_45k/2026-11-14-fp-egress-allowlist-freeze-epoch-3.json")
}
const EPOCH3_SHA256: &str = "028a0dd620f7837f373cb9810ca5c9f420b92eda465d2d3d370dfb7dc46a344f";

#[test]
fn epoch3_fixture_is_the_committed_freeze_bytes() {
    let bytes = std::fs::read(epoch3()).unwrap();
    assert_eq!(hharness_wire::sha256::sha256_hex(&bytes), EPOCH3_SHA256);
}

fn scratch(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("r645k-{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn freeze_list() -> Allowlist {
    let j = json::parse(&std::fs::read_to_string(epoch3()).unwrap()).unwrap();
    let l = j.get("list").unwrap();
    let hosts = |k: &str| -> BTreeSet<String> {
        l.get(k)
            .and_then(Json::as_arr)
            .unwrap()
            .iter()
            .filter_map(|h| h.as_str().map(str::to_string))
            .collect()
    };
    Allowlist {
        agent: hosts("agent"),
        verify: hosts("verify"),
        permanent_denies: hosts("permanent_denies"),
        withheld: vec![],
    }
}

/// The in-test render — mirrors `h_run_once`'s admit + extraction so the
/// assertion is byte-identity between two spellings of one code path.
fn in_test_layer(list: &Allowlist, phase: &str) -> String {
    let mut rec = EnvironmentRecord {
        class: EnvironmentClass::LocalContainer,
        image: ImageRef::Tag("hh-w1-render-policy".to_string()),
        build_context: None,
        platform: "linux/amd64".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("w1".to_string(), "v0".to_string()),
        limits: hharness_containment::policy::ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    };
    let mut pol = hharness_containment::policy::kernel_default(0);
    let decl = MediatedDecl {
        family: ImageFamily::Debian,
        allowlist: list.clone(),
        protocol_id: "fp-w1-q-x55".to_string(),
        epoch: 3,
        cell: MediatedCell {
            socket_mounts: BTreeMap::from([
                ("model".to_string(), MODEL_SOCK_PATH.to_string()),
                ("egress".to_string(), EGRESS_SOCK_PATH.to_string()),
            ]),
            relay_bin: "hh-cell-relay".to_string(),
            pairs: vec![
                RelayPair {
                    name: "model".to_string(),
                    port: 8080,
                },
                RelayPair {
                    name: "egress".to_string(),
                    port: EGRESS_RELAY_PORT,
                },
            ],
        },
        model_port: 8080,
        slot_token: "hh-w1-slot-token".to_string(),
    };
    admit_allowlist_mediated(&mut rec, &mut pol, &decl, 0).unwrap();
    let sched = pol
        .ext
        .get("phase_schedule")
        .cloned()
        .expect("phase_schedule");
    let l = sched
        .get(phase)
        .and_then(|e| e.get("layer"))
        .cloned()
        .unwrap();
    let cp = hharness_containment::policy::ContainmentPolicy::from_json(&l).unwrap();
    cp.to_json().to_canonical_string()
}

#[test]
fn render_policy_verify_bytes_match_the_admit_path() {
    let dir = scratch("policy");
    let out = dir.join("verify.json");
    let status = Command::new(W1)
        .args([
            "render-policy",
            "--freeze",
            epoch3().to_str().unwrap(),
            "--phase",
            "verify",
            "--out",
            out.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success(), "render-policy exited {status}");
    let rendered = std::fs::read_to_string(&out).unwrap();
    // Byte-identity with the admit path's canonical layer — the
    // mediator's policy bytes are never re-derived.
    assert_eq!(rendered, in_test_layer(&freeze_list(), "verify"));
}

#[test]
fn render_policy_agent_layer_never_grants_github() {
    let dir = scratch("agent");
    let out = dir.join("agent.json");
    assert!(Command::new(W1)
        .args([
            "render-policy",
            "--freeze",
            epoch3().to_str().unwrap(),
            "--phase",
            "agent",
            "--out",
            out.to_str().unwrap(),
        ])
        .status()
        .unwrap()
        .success());
    let layer = json::parse(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let grants: Vec<&str> = layer
        .get("net")
        .and_then(|n| n.get("rules"))
        .and_then(Json::as_arr)
        .unwrap()
        .iter()
        .filter_map(|r| r.get("host").and_then(Json::as_str))
        .collect();
    assert!(
        !grants.iter().any(|h| h.contains("github.com")),
        "agent phase must never grant github.com (Q-X55): {grants:?}"
    );
}

#[test]
fn scan_hosts_names_the_pytorch_index() {
    let dir = scratch("scan");
    let f = dir.join("t.sh");
    std::fs::write(
        &f,
        "uvx \\\n  --index https://download.pytorch.org/whl/cpu \\\n  -w torch==2.7.1\n",
    )
    .unwrap();
    let out = Command::new(W1)
        .args(["scan-hosts", "--files", f.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.lines().any(|l| l == "download.pytorch.org"),
        "scan-hosts must name the pytorch index: {text}"
    );
    // the same fn the derivation cites — the scan works; D16's gap is
    // the epoch-3 derivation's input set, never the scanner.
    let direct = scan_hosts_in_text("--index https://download.pytorch.org/whl/cpu \\");
    assert!(direct.contains("download.pytorch.org"));
}
