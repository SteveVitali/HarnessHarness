//! R2.6 — the `control_strategy` out-of-process conformance lane:
//! `hh-plugin-fixture --mode control` hosts the registered `hh-control`
//! variant impls for real through `hh-helper` (`direct` plumbing — no
//! sandbox claims) and serves the contract's method set over
//! `plugin_abi/1` `invoke` ops (`open`/`observe`/`decide`/`terminate`/
//! `checkpoint`/`restore`), each taking and returning canonical
//! `hh_control::wire` documents. The suite asserts on the canonical
//! records themselves — AC-1's document leg, driven over a real process
//! boundary:
//!
//! - the durable-steer interpretation (`woken{delivery_mode: steer}` →
//!   `propose{steer_ref}` — the same answer the in-process driver gets
//!   when a queued `steer{mode: next_turn}` fires, DF-S2.11-1);
//! - the declaration boundary (`hh/react-minimal` never invents a steer
//!   interpretation — a `woken{steer}` falls through to plain `propose`);
//! - the stop protocol (`human_input{interrupt}` → `stop{cancelled}`);
//! - the purity leg a conformance harness is for — the same `decide`
//!   input answers the same canonical document twice;
//! - the checkpoint/restore pair round-trips the state document;
//! - typed refusals, never guesses — malformed input is
//!   `schema_violation`, an unknown op is `unhandled_operation`, an
//!   unregistered variant name is `not_installed` at `bind` (the
//!   fallback-to-`react/minimal` defect class stays dead on both sides
//!   of the process line).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hh_control::react;
use hh_control::strategy::{ConcurrentInput, ControlContext, SteerMode, StrategyParams};
use hh_control::vocab::{Cue, DeliveryMode, HumanInput, WokenTrigger};
use hh_control::wire;
use hh_embed_schema::plugin_abi::BindParams;
use hh_ontology::control::{CancelledBy, StopReason};
use hh_registry::extension::plugin::{PluginIdentity, PluginManifest, Requests, Requires};
use hh_varhost::{
    spawn, HostError, InvokeOutcome, RecordingPorts, SpawnSpec, VariantHost, VariantPackage,
    VecEvents,
};
use hh_wire::json::Json;

const TIMEOUT: Duration = Duration::from_secs(20);

/// `target/{profile}/hh-plugin-fixture` beside the test binary.
fn fixture_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let p = dir.join("hh-plugin-fixture");
    p.exists().then_some(p)
}

fn helper_present() -> bool {
    hh_env::helper::helper_binary().is_some()
}

fn manifest() -> PluginManifest {
    PluginManifest {
        identity: PluginIdentity {
            namespace: "local".into(),
            name: "fixture".into(),
            version_label: None,
        },
        tier: "C0".into(),
        depends_on: vec![],
        requires: Requires {
            hir_dialect: "1.0".into(),
            registry_dialect: "1.0".into(),
            plugin_abi: "1.0".into(),
            contracts: vec![],
            records: vec![],
        },
        contributions: vec![],
        requests: Requests::default(),
        claims: Json::obj([]),
        conformance_claims: vec![],
        parameters: None,
        summary: Json::Null,
        ext: BTreeMap::new(),
    }
}

fn package(bin: &Path, manifest: PluginManifest) -> VariantPackage {
    VariantPackage {
        root: bin.parent().unwrap().to_path_buf(),
        manifest,
        content: "content:fixture".into(),
        version_id: "pin:fixture".into(),
        execs: vec![bin.to_path_buf()],
    }
}

fn spec(pkg: VariantPackage, exec_args: Vec<String>, tag: &str) -> SpawnSpec {
    let mut pinned_args = vec![
        "--plugin-id".into(),
        "local/fixture".into(),
        "--version-id".into(),
        pkg.version_id().to_string(),
        "--content".into(),
        pkg.content().to_string(),
        "--mode".into(),
        "control".into(),
    ];
    pinned_args.extend(exec_args);
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "vh-control-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let cap = pkg.manifest.requests.clone();
    SpawnSpec {
        package: pkg,
        session_id: format!("control-{tag}-{}", std::process::id()),
        backend: "direct".into(),
        placement: hh_registry::kinds::Placement::SubprocessConfined,
        helper_extra_args: vec![],
        socket_dir: dir,
        exec_args: pinned_args,
        cap,
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hh_hir::refs::Ref::pinned("plugin:local/fixture", "pin:fixture"),
        registry_snapshot_id: "snap-1".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            "class_contract:control_strategy".to_string(),
            "1.0".to_string(),
        )]),
        timeout: TIMEOUT,
        at: 0,
    }
}

fn bind_params(variant_ref: &str) -> BindParams {
    BindParams {
        slot: "slot-1".into(),
        class_id: "control_strategy".into(),
        contract_version: "1.0".into(),
        variant: Json::obj([("semantic_id", Json::str(variant_ref))]),
        params: Json::Null,
        profile: Json::Null,
        account: Json::Null,
        budget: Json::Null,
        placement: "subprocess_confined".into(),
    }
}

fn steering_ctx(steer_mode: SteerMode) -> ControlContext {
    ControlContext {
        process_ref: "proc-1".into(),
        plan: vec![],
        boundary: react::react_preset(),
        profile: Json::Null,
        account_ref: "acct".into(),
        budget_ref: "b-1".into(),
        envelope_ref: "env-1".into(),
        parameters: StrategyParams::default(),
        capabilities_available: vec!["hh.submit".into()],
        steering: (steer_mode, ConcurrentInput::Steer),
    }
}

fn woken_steer(payload_ref: &str) -> Cue {
    Cue::Woken {
        trigger: WokenTrigger::Manual {
            principal: "principal:p".into(),
        },
        payload_ref: payload_ref.into(),
        delivery_mode: DeliveryMode::Steer,
    }
}

/// `invoke` → the single output document (fails the test on any other
/// shape — a `Failed` is a typed answer, not an output).
fn one_output(
    host: &mut VariantHost<RecordingPorts, VecEvents>,
    s: &mut hh_varhost::VariantSession,
    binding: &str,
    op: &str,
    input: Json,
    n: u64,
) -> Json {
    let out = host
        .invoke(s, binding, op, vec![input], &format!("r{n}"), TIMEOUT)
        .unwrap_or_else(|e| panic!("{op}: host error {e:?}"));
    let InvokeOutcome::Outputs(o) = out else {
        panic!("{op}: {out:?}")
    };
    assert_eq!(o.len(), 1, "{op}: one output doc, got {o:?}");
    o.into_iter().next().unwrap()
}

/// The `woken{delivery_mode: steer}` cell, driven over a real process:
/// the declared-steering variant answers `propose{steer_ref}` — the
/// same interpretation the in-process driver's `ReactSteerable` gives
/// the drained wakeup row (DF-S2.11-1's interpreter leg, verified on
/// the ABI the packaged variant would ride).
#[test]
fn oop_control_steerable_queue_serves_the_contract() {
    let Some(bin) = fixture_bin() else { return };
    if !helper_present() {
        eprintln!("control lane: hh-helper binary not found — skipped");
        return;
    }
    let pkg = package(&bin, manifest());
    let s = spec(pkg, vec![], "steer");
    let (mut session, _lowered) = spawn(&s).expect("spawn");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host
        .bind(&mut session, bind_params("hh/react-steerable-queue"))
        .expect("bind");

    // open(ctx) → state₀ — the context rides the canonical codec.
    let ctx = steering_ctx(SteerMode::QueueNextTurn);
    let ctx_doc = wire::context_to_json(&ctx);
    let state0 = one_output(&mut host, &mut session, &b, "open", ctx_doc.clone(), 1);
    let st0 = wire::state_from_json(&state0).expect("open: state doc decodes");
    // The `-queue` registry alias binds the same `ReactSteerable` impl —
    // the state's `variant_ref` is the impl's own ref (`hh/react-steerable@1`),
    // while `steer_mode = queue_next_turn` is declaration data, not impl state.
    assert_eq!(st0.variant_ref, react::REACT_STEERABLE_REF);

    // decide({state, cue: woken{steer}}) → {state', propose{steer_ref}}.
    let decide_in = Json::obj([
        ("state", state0.clone()),
        ("cue", wire::cue_to_json(&woken_steer("input-9"))),
    ]);
    let out = one_output(&mut host, &mut session, &b, "decide", decide_in.clone(), 2);
    let decision = out.get("decision").cloned().unwrap_or(Json::Null);
    let d = wire::decision_from_json(&decision).expect("decide: decision doc decodes");
    match &d.kind {
        hh_control::vocab::DecisionKind::Propose {
            context_request, ..
        } => {
            assert_eq!(
                context_request.get("steer_ref").and_then(Json::as_str),
                Some("input-9"),
                "the steer cue's payload ref is the proposed context request: {decision:?}"
            );
        }
        other => panic!("woken{{steer}} decided {other:?}, expected propose"),
    }
    // The returned state is a real state — decision count advanced.
    let st1 = wire::state_from_json(out.get("state").unwrap_or(&Json::Null))
        .expect("decide: state doc decodes");
    assert_eq!(st1.decision_count, st0.decision_count + 1);

    // Purity — the same `decide` input answers the same canonical
    // document (the conformance harness's determinism leg holds by
    // construction of the caller-carried state).
    let again = one_output(&mut host, &mut session, &b, "decide", decide_in, 3);
    assert_eq!(again, out, "decide is deterministic over its inputs");

    // observe({state, events: []}) → the same state (the fold is a pure
    // function of the durable prefix — ∅ prefix ↦ identity).
    let observed = one_output(
        &mut host,
        &mut session,
        &b,
        "observe",
        Json::obj([("state", state0.clone()), ("events", Json::Arr(vec![]))]),
        4,
    );
    assert_eq!(observed, state0, "observe over no events is the identity");

    // checkpoint → restore round-trips the state document through the
    // canonical-bytes dialect the driver writes to `leaf.checkpoint`.
    let cp = one_output(
        &mut host,
        &mut session,
        &b,
        "checkpoint",
        Json::obj([("state", state0.clone())]),
        5,
    );
    let cp_text = cp
        .get("checkpoint")
        .and_then(Json::as_str)
        .expect("checkpoint doc")
        .to_string();
    let restored = one_output(
        &mut host,
        &mut session,
        &b,
        "restore",
        Json::obj([("checkpoint", Json::str(cp_text)), ("ctx", ctx_doc)]),
        6,
    );
    assert_eq!(restored, state0, "restore(checkpoint(s)) = s");

    // terminate({state, stop_reason}) → the final report document.
    let report = one_output(
        &mut host,
        &mut session,
        &b,
        "terminate",
        Json::obj([
            ("state", state0),
            (
                "stop_reason",
                wire::stop_reason_to_json(&StopReason::Cancelled {
                    by: CancelledBy::Principal,
                }),
            ),
        ]),
        7,
    );
    let _ = wire::report_from_json(&report).expect("terminate: report doc decodes");

    host.close(&mut session).unwrap();
    assert!(session.detached);
}

/// The declaration boundary holds over the wire: `hh/react-minimal`
/// declares no steering, so a `woken{delivery_mode: steer}` falls
/// through to the shared table (plain `propose` — never a `steer_ref`
/// the declaration did not admit).
#[test]
fn oop_control_minimal_never_invents_a_steer_interpretation() {
    let Some(bin) = fixture_bin() else { return };
    if !helper_present() {
        eprintln!("control lane: hh-helper binary not found — skipped");
        return;
    }
    let pkg = package(&bin, manifest());
    let s = spec(pkg, vec![], "minimal");
    let (mut session, _lowered) = spawn(&s).expect("spawn");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host
        .bind(&mut session, bind_params("hh/react-minimal"))
        .expect("bind");

    let state0 = one_output(
        &mut host,
        &mut session,
        &b,
        "open",
        wire::context_to_json(&steering_ctx(SteerMode::Unsupported)),
        1,
    );
    let out = one_output(
        &mut host,
        &mut session,
        &b,
        "decide",
        Json::obj([
            ("state", state0),
            ("cue", wire::cue_to_json(&woken_steer("input-9"))),
        ]),
        2,
    );
    let decision = out.get("decision").cloned().unwrap_or(Json::Null);
    let d = wire::decision_from_json(&decision).expect("decision decodes");
    match &d.kind {
        hh_control::vocab::DecisionKind::Propose {
            context_request, ..
        } => {
            assert!(
                context_request.get("steer_ref").is_none(),
                "an undeclared steer never mints steer_ref: {decision:?}"
            );
        }
        other => panic!("woken{{steer}} decided {other:?}, expected propose"),
    }

    host.close(&mut session).unwrap();
    assert!(session.detached);
}

/// The stop protocol crosses the boundary: `human_input{interrupt}`
/// decides `stop{cancelled{principal}}` — a parked run stays
/// cancellable and the answer is code-owned, not improvised.
#[test]
fn oop_control_interrupt_is_stop_cancelled() {
    let Some(bin) = fixture_bin() else { return };
    if !helper_present() {
        eprintln!("control lane: hh-helper binary not found — skipped");
        return;
    }
    let pkg = package(&bin, manifest());
    let s = spec(pkg, vec![], "interrupt");
    let (mut session, _lowered) = spawn(&s).expect("spawn");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host
        .bind(&mut session, bind_params("hh/react-steerable"))
        .expect("bind");

    let state0 = one_output(
        &mut host,
        &mut session,
        &b,
        "open",
        wire::context_to_json(&steering_ctx(SteerMode::InterruptAtDecisionPoint)),
        1,
    );
    let out = one_output(
        &mut host,
        &mut session,
        &b,
        "decide",
        Json::obj([
            ("state", state0),
            (
                "cue",
                wire::cue_to_json(&Cue::HumanInput(HumanInput::Interrupt)),
            ),
        ]),
        2,
    );
    let decision = out.get("decision").cloned().unwrap_or(Json::Null);
    let d = wire::decision_from_json(&decision).expect("decision decodes");
    assert!(
        matches!(
            &d.kind,
            hh_control::vocab::DecisionKind::Stop {
                proposed_reason: StopReason::Cancelled {
                    by: CancelledBy::Principal
                },
                ..
            }
        ),
        "interrupt → stop{{cancelled{{principal}}}}: {decision:?}"
    );

    host.close(&mut session).unwrap();
    assert!(session.detached);
}

/// Honest refusals over the wire — malformed inputs are
/// `schema_violation`, an unknown op is `unhandled_operation`, and an
/// unregistered variant name is `not_installed` at `bind` (the
/// silent-`react/minimal` defect class stays dead on the plugin side
/// too — the fixture resolves through the same `variant_for` table).
#[test]
fn oop_control_typed_refusals() {
    let Some(bin) = fixture_bin() else { return };
    if !helper_present() {
        eprintln!("control lane: hh-helper binary not found — skipped");
        return;
    }
    let pkg = package(&bin, manifest());
    let s = spec(pkg, vec![], "refusals");
    let (mut session, _lowered) = spawn(&s).expect("spawn");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let b = host
        .bind(&mut session, bind_params("hh/react-steerable"))
        .expect("bind");

    // Malformed decide input — `{state: "not-a-doc"}` is a typed
    // `schema_violation`, never a guess.
    let out = host
        .invoke(
            &mut session,
            &b,
            "decide",
            vec![Json::obj([("state", Json::str("not-a-doc"))])],
            "r1",
            TIMEOUT,
        )
        .expect("decide invoke");
    let InvokeOutcome::Failed(e) = out else {
        panic!("malformed decide: {out:?}")
    };
    assert_eq!(e, hh_embed_schema::plugin_abi::AbiError::SchemaViolation);

    // An op outside the contract's method set — `unhandled_operation`.
    let out = host
        .invoke(
            &mut session,
            &b,
            "not_an_op",
            vec![Json::Null],
            "r2",
            TIMEOUT,
        )
        .expect("unknown op invoke");
    let InvokeOutcome::Failed(e) = out else {
        panic!("unknown op: {out:?}")
    };
    assert_eq!(e, hh_embed_schema::plugin_abi::AbiError::UnhandledOperation);

    // An unregistered variant name — `not_installed` at bind, never a
    // silent substitute.
    let err = host
        .bind(&mut session, bind_params("hh/does-not-exist"))
        .expect_err("unknown variant refuses at bind");
    assert!(
        matches!(
            err,
            HostError::BindFailed(hh_embed_schema::plugin_abi::BindFailure::NotInstalled)
        ),
        "unknown variant: {err:?}"
    );

    host.close(&mut session).unwrap();
    assert!(session.detached);
}
