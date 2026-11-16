//! model-port guard regression battery (R6.45n diagnosis —
//! `machinery{model-port-collision}`). The in-cell model listener is
//! the harness's own loopback bind: a registered-range port is a
//! silent EADDRINUSE deadlock against any task-owned service — the
//! diagnosed instance was `pypi-server -p 8080` meeting the relay's
//! own :8080, after which the task's upload POST hung on the
//! never-answering model stub until the agent deadline. The guard
//! refuses the declared port at every projection seam — typed and
//! immediate, never the silent 900 s hang.

use std::collections::{BTreeMap, BTreeSet};

use hharness_containment::policy::kernel_default;
use hharness_env::egress_allowlist::*;
use hharness_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};

fn cell_record() -> EnvironmentRecord {
    EnvironmentRecord {
        class: EnvironmentClass::LocalContainer,
        image: ImageRef::Tag("docker.io/library/alpine:3.20".to_string()),
        build_context: None,
        platform: "linux/amd64".to_string(),
        provisioning: ProvisioningRecipe::default(),
        containment_policy_ref: ("test:policy".to_string(), "v-pol".to_string()),
        limits: hharness_containment::policy::ResourceLimits::default(),
        nondeterminism: vec![],
        unpinned: BTreeSet::new(),
        image_attestation: None,
        ext: BTreeMap::new(),
        canary_channels: vec![],
    }
}

fn cell() -> MediatedCell {
    MediatedCell {
        socket_mounts: BTreeMap::from([
            ("model".to_string(), "/host/slot-1/model.sock".to_string()),
            ("egress".to_string(), "/host/slot-1/egress.sock".to_string()),
        ]),
        relay_bin: "/host/bin/hh-cell-relay".to_string(),
        pairs: vec![
            RelayPair {
                name: "model".to_string(),
                port: MODEL_RELAY_PORT_MIN,
            },
            RelayPair {
                name: "egress".to_string(),
                port: EGRESS_RELAY_PORT,
            },
        ],
    }
}

fn list() -> Allowlist {
    Allowlist::first_pass(
        ImageFamily::Debian,
        BTreeSet::from(["astral.sh".to_string()]),
        &["mirror.tb-tasks.example".to_string()],
        vec![],
    )
}

fn decl() -> MediatedDecl {
    MediatedDecl {
        family: ImageFamily::Debian,
        allowlist: list(),
        protocol_id: "protocol:t".to_string(),
        epoch: 1,
        cell: cell(),
        model_port: MODEL_RELAY_PORT_MIN,
        slot_token: "slot-token-t".to_string(),
    }
}

// ── the guard itself ─────────────────────────────────────────────────────────

#[test]
fn guard_refuses_the_registered_range() {
    for p in [0, 80, 8080, 3128, 49151] {
        let e = check_model_relay_port(p).unwrap_err().to_string();
        assert!(e.contains("registered-service"), "{p}: {e}");
    }
    for p in [MODEL_RELAY_PORT_MIN, 58_080, 65_535] {
        check_model_relay_port(p).unwrap();
    }
}

// ── the projection seam ─────────────────────────────────────────────────────

#[test]
fn projection_refuses_registered_range_and_renders_the_band() {
    for p in [8080, 4242, 9001] {
        assert!(
            cell_env_projection(p, "slot-token-t").is_err(),
            "{p} must refuse"
        );
    }
    let env = cell_env_projection(MODEL_RELAY_PORT_MIN, "slot-token-t").unwrap();
    assert_eq!(
        env.get("OPENAI_BASE_URL"),
        Some(&format!("http://127.0.0.1:{MODEL_RELAY_PORT_MIN}/v1"))
    );
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL"),
        Some(&format!("http://127.0.0.1:{MODEL_RELAY_PORT_MIN}"))
    );
}

// ── the admission seam ──────────────────────────────────────────────────────

#[test]
fn admit_refuses_a_registered_range_decl_port_and_leaves_policy_byte_identical() {
    let mut rec = cell_record();
    let mut pol = kernel_default(0);
    let before_pol = pol.clone();
    let mut d = decl();
    d.model_port = 8080;
    assert!(admit_allowlist_mediated(&mut rec, &mut pol, &d, 1).is_err());
    assert!(rec.ext.is_empty(), "record ext stamped on a refusal");
    assert_eq!(pol, before_pol, "policy mutated on a refusal");
}

#[test]
fn admit_refuses_a_registered_range_pair_port_even_when_the_decl_port_is_in_band() {
    // The pair is the port the relay actually binds — a sub-band pair
    // with an in-band decl port is the same silent collision.
    let mut rec = cell_record();
    let mut pol = kernel_default(0);
    let before_pol = pol.clone();
    let mut d = decl();
    d.cell
        .pairs
        .iter_mut()
        .find(|p| p.name == "model")
        .unwrap()
        .port = 8080;
    assert!(admit_allowlist_mediated(&mut rec, &mut pol, &d, 1).is_err());
    assert_eq!(pol, before_pol, "policy mutated on a refusal");
}

#[test]
fn admit_with_a_band_port_admits_and_renders_the_port() {
    let mut rec = cell_record();
    let mut pol = kernel_default(0);
    admit_allowlist_mediated(&mut rec, &mut pol, &decl(), 1).unwrap();
    assert_eq!(
        pol.proc.env.set.get("OPENAI_BASE_URL"),
        Some(&format!("http://127.0.0.1:{MODEL_RELAY_PORT_MIN}/v1"))
    );
}
