//! model-port guard — the render lane's leg (R6.45n
//! `machinery{model-port-collision}`). `slot.model_port` is the
//! in-cell model-relay bind; a registered-range port is the silent
//! EADDRINUSE class the hh-env projection guard refuses, and the
//! renderer must refuse it before any adapter emits a colliding cell
//! env — typed, never rendered.

use std::path::PathBuf;

use hharness_env::egress_allowlist::MODEL_RELAY_PORT_MIN;
use hharness_runner::render::{render, RenderUse, SlotInputs};
use hharness_wire::json::{parse, Json};

fn admission() -> Json {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("r6_28")
        .join("mini_admission.json");
    parse(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn slot(model_port: u16) -> SlotInputs {
    SlotInputs {
        slot_token: "slot-token-t".to_string(),
        model_port,
        instruction: b"do the task".to_vec(),
        home: "/root".to_string(),
        user: "root".to_string(),
        agent_wall_timeout_s: 3600,
        proxy_budget_ceiling_usd_micros: 1,
        slot_id: "slot-fixture".to_string(),
    }
}

#[test]
fn render_refuses_a_registered_range_model_port() {
    let rec = admission();
    for p in [0, 80, 8080, 3128, 49151] {
        let e = render(&rec, &slot(p), RenderUse::HostedArm).unwrap_err();
        assert_eq!(e.rule, "model_port", "{p}: {e:?}");
    }
}

#[test]
fn render_accepts_the_dynamic_band() {
    let rec = admission();
    let rc = render(&rec, &slot(MODEL_RELAY_PORT_MIN), RenderUse::HostedArm).unwrap();
    assert!(rc
        .env
        .get("OPENAI_BASE_URL")
        .unwrap()
        .contains(&format!("127.0.0.1:{MODEL_RELAY_PORT_MIN}")));
}
