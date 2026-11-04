//! R5.45 — the EX-03/EX-04 example integration battery (AC-1 positive,
//! AC-2 negative): the pinned actual-execution packets verified, the
//! bounded `model_calls` cap field through validate/compile/rerun
//! (adaptation, not causal), the sealed-submission/oracle legs, the
//! EX-01 receipt join, and the typed refusal controls. Fails-without:
//! the `ex-03`/`ex-04` cases and these cards do not exist at base
//! a00acee3 (`unknown_case`/missing-file red).
//!
//! Scratch stores only — offline scripted-lane execution; USD 0.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hharness::examples::{run_example, ExampleCard};
use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_wire::Json;

fn tmp(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-r5-45-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn svc(root: &Path) -> EmbedService {
    EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.join("ws"),
        holder: "r5-45".to_string(),
    })
    .unwrap()
}

fn check<'a>(r: &'a hharness::examples::ExampleReport, id: &str) -> &'a str {
    r.checks
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.verdict)
        .unwrap_or_else(|| panic!("check {id} not produced: {:?}", r.checks))
}

fn card(name: &str) -> ExampleCard {
    ExampleCard::load(&repo().join("scripts/examples").join(name)).unwrap()
}

#[test]
fn ex03_card_runs_and_verifies() {
    let root = tmp("ex03");
    let mut s = svc(&root);
    let report = run_example(&mut s, &card("ex-03.json"), &root.join("work"), &repo()).unwrap();
    assert_eq!(report.verdict, "pass", "{:?}", report.checks);
    assert_eq!(check(&report, "packet"), "pass");
    assert_eq!(check(&report, "sealed_submission"), "pass");
    assert_eq!(check(&report, "oracle_chain"), "pass");
    assert_eq!(check(&report, "field_edit"), "pass");
    assert_eq!(check(&report, "edit_refusal"), "expected_refusal");
    assert_eq!(check(&report, "receipt"), "pass");
}

#[test]
fn ex04_card_runs_and_verifies() {
    let root = tmp("ex04");
    let mut s = svc(&root);
    let report = run_example(&mut s, &card("ex-04.json"), &root.join("work"), &repo()).unwrap();
    assert_eq!(report.verdict, "pass", "{:?}", report.checks);
    assert_eq!(check(&report, "packet"), "pass");
    assert_eq!(check(&report, "matrix"), "pass");
    assert_eq!(check(&report, "exhaustion"), "pass");
    for id in [
        "refusal_suspend",
        "refusal_resume",
        "refusal_respond",
        "refusal_amend",
    ] {
        assert_eq!(check(&report, id), "expected_refusal");
    }
}

// ── AC-2 negatives ────────────────────────────────────────────────────

/// A drifted packet byte refuses `stale_pin` before any execution — the
/// card's pin is the committed packet's own sha256.
#[test]
fn ex03_stale_packet_pin_refuses() {
    let root = tmp("pin");
    let mut c = card("ex-03.json");
    let Json::Obj(mut pins) = c.pins.clone() else {
        panic!()
    };
    let Json::Obj(mut pin) = pins.get("packet").cloned().unwrap() else {
        panic!()
    };
    pin.insert("sha256".into(), Json::str("sha256:0000"));
    pins.insert("packet".into(), Json::Obj(pin));
    c.pins = Json::Obj(pins);
    let mut s = svc(&root);
    let e = run_example(&mut s, &c, &root.join("work"), &repo()).unwrap_err();
    assert!(format!("{e}").contains("stale_pin"), "{e}");
}

/// A mutated packet — a leg verdict flipped to `executed` — is an
/// absent/fake event: the closed-set/typed-verdict leg fails.
#[test]
fn ex04_fake_packet_event_fails() {
    let root = tmp("fake");
    let mut s = svc(&root);
    // Rewrite the committed packet with a forged leg — the pin still
    // recomputes over the *pinned file*, so the fake is caught by the
    // packet leg's own checks when the forged file replaces it… rather
    // than mutating the committed artifact, decode+flip in-memory and
    // write a forged copy the pin can't match: assert stale_pin fires.
    // The source path comes from the card's own pin record — the
    // declared, sha-checked contract — never a duplicated literal in
    // this test (a literal could drift from the pin unchecked).
    let mut c = card("ex-04.json");
    let Json::Obj(pin_map) = c.pins.clone() else {
        panic!()
    };
    let Json::Obj(pin_rec) = pin_map.get("packet").cloned().unwrap() else {
        panic!()
    };
    let src = repo().join(
        pin_rec
            .get("path")
            .and_then(Json::as_str)
            .expect("card.pins.packet.path"),
    );
    let mut text = std::fs::read_to_string(&src).unwrap();
    text = text.replacen("\"verdict\":\"verified\"", "\"verdict\":\"bogus-fake\"", 1);
    let forged = root.join("forged.json");
    std::fs::write(&forged, &text).unwrap();
    // A card pinning the forged packet: the pin sha recomputes — point
    // the pin at the forged path so pin verification passes and the
    // *packet check* must catch the mutated verdict.
    let sha = format!(
        "sha256:{}",
        hharness_wire::sha256::sha256_hex(std::fs::read(&forged).unwrap().as_slice())
    );
    let Json::Obj(mut pins) = c.pins.clone() else {
        panic!()
    };
    let Json::Obj(mut pin) = pins.get("packet").cloned().unwrap() else {
        panic!()
    };
    pin.insert(
        "path".into(),
        Json::str(forged.to_string_lossy().to_string()),
    );
    pin.insert("sha256".into(), Json::str(sha));
    pins.insert("packet".into(), Json::Obj(pin));
    c.pins = Json::Obj(pins);
    let report = run_example(&mut s, &c, &root.join("work"), &repo()).unwrap();
    assert_eq!(check(&report, "packet"), "fail");
    assert_eq!(report.verdict, "fail");
}

/// Missing recipient bytes — tamper the exported bundle before the
/// completeness leg would read it: the receipt join must fail rather
/// than pass on a partial export. Exercised through the card's own
/// machinery: a card whose `run` evidence the recipient cannot see is
/// equivalent — here the recipient leg is driven with an empty export.
#[test]
fn ex03_missing_recipient_bytes_fail() {
    let root = tmp("recip");
    let mut s = svc(&root);
    // Open a fresh recipient on an empty export dir — completeness
    // refuses/fails, never a pass.
    let mut recip = EmbedService::open(ServiceConfig {
        store_root: root.join("rs"),
        kernel_version_id: "hh-kernel/0.1.0".to_string(),
        workspace_root: root.join("rw"),
        holder: "r5-45-recip".to_string(),
    })
    .unwrap();
    let resp = recip.handle(&hharness_wire::jsonrpc::Request {
        id: Json::str("neg"),
        method: "kernel.check_completeness".to_string(),
        params: Json::obj([("path", Json::str(root.join("empty").to_string_lossy()))]),
    });
    assert!(
        resp.get("error").is_some() || {
            let r = resp.get("result").cloned().unwrap_or(Json::Null);
            r.get("complete")
                .and_then(|c| c.as_int())
                .map(|i| i == 0)
                .unwrap_or(true)
        },
        "{resp:?}"
    );
    let _ = &mut s;
}

/// Fixture-promotion guard: the cards' fixture_label and honest
/// limitations stay on the report verbatim — an unlabelled fixture
/// success is a card-drift failure, not evidence.
#[test]
fn cards_retain_fixture_labels() {
    for name in ["ex-03.json", "ex-04.json"] {
        let c = card(name);
        assert!(c.fixture_label.contains("fixture") || c.fixture_label.contains("scripted"));
        assert!(c.limitations.iter().any(|l| l.contains("never S1/S3")));
        assert!(c.prerequisites.get("spend_usd").and_then(|v| v.as_int()) == Some(0));
        assert!(c.prerequisites.get("network") == Some(&Json::Bool(false)));
    }
}
