//! R2.15 — the out-of-process `validator` conformance suite (spec §5f.1;
//! ADR-0110/0111; DF-S1.21-2): the `hh-plugin-fixture --mode validator`
//! stub spawned *for real* through `hh-helper`, driven over
//! `plugin_abi/1` `invoke` ops through the generic
//! `declare`/`bind`/`collect`/`check` contract battery. The fixture runs
//! the canonical `hh-verification` semantics — `validators::declare`/
//! `bind`, `build_bundle`, the `Verdict` contract and the
//! `verification.validator.*` codecs — so what the suite asserts is the
//! contract itself crossing the process boundary, never a re-spelling
//! (CC1/CC7).
//!
//! Positive legs: `declare` returns a complete declaration; a bound
//! validator answers `collect` with a content-addressed bundle and
//! `check` with a `decided` deterministic verdict; a second `check` over
//! the same bundle is identical (determinism/purity).
//!
//! Negative legs (the contract's refusals, typed — never silent):
//! `collect`/`check` before `bind` → `UnboundFixture`; a bound
//! declaration pinning a different `validator_ref` →
//! `PayloadHashMismatch`; a declaration with non-read-only
//! `side_effects` → `ValidatorHasEffects`; `evidence_inputs = ∅` →
//! `IncompleteDeclaration`; `model_io` evidence → `ClaimOnlyEvidence`;
//! below-`environment` authority → `EvidenceUnauthoritative`; an
//! undeclared op → `UnhandledOperation`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use hh_embed_schema::plugin_abi::{AbiError, BindParams};
use hh_registry::extension::plugin::{PluginIdentity, PluginManifest, Requests, Requires};
use hh_varhost::{
    spawn, InvokeOutcome, RecordingPorts, SpawnSpec, VariantHost, VariantPackage, VariantSession,
    VecEvents,
};
use hh_wire::json::Json;

const TIMEOUT: Duration = Duration::from_secs(20);

fn fixture_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?;
    let p = dir.join("hh-plugin-fixture");
    p.exists().then_some(p)
}

fn manifest() -> PluginManifest {
    PluginManifest {
        identity: PluginIdentity {
            namespace: "local".into(),
            name: "fixture-validator".into(),
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

fn spec(pkg: VariantPackage, exec_args: Vec<String>) -> SpawnSpec {
    let mut pinned_args = vec![
        "--plugin-id".into(),
        "local/fixture-validator".into(),
        "--version-id".into(),
        pkg.version_id().to_string(),
        "--content".into(),
        pkg.content().to_string(),
        "--mode".into(),
        "validator".into(),
    ];
    pinned_args.extend(exec_args);
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "vh-val-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let cap = pkg.manifest.requests.clone();
    SpawnSpec {
        package: pkg,
        session_id: format!("val-{}", std::process::id()),
        backend: "direct".into(),
        placement: hh_registry::kinds::Placement::SubprocessConfined,
        helper_extra_args: vec![],
        socket_dir: dir,
        exec_args: pinned_args,
        cap,
        ambient_env: vec![],
        issuer_ref: "principal:test".into(),
        holder: hh_hir::refs::Ref::pinned("plugin:local/fixture-validator", "pin:fixture"),
        registry_snapshot_id: "snap-1".into(),
        kernel_capabilities: BTreeMap::new(),
        contract_versions_offered: BTreeMap::from([(
            "class_contract:validator".to_string(),
            "1.0".to_string(),
        )]),
        timeout: TIMEOUT,
        at: 0,
    }
}

fn bind_params() -> BindParams {
    BindParams {
        slot: "slot-1".into(),
        class_id: "validator".into(),
        contract_version: "1.0".into(),
        variant: Json::Null,
        params: Json::Null,
        profile: Json::Null,
        account: Json::Null,
        budget: Json::Null,
        placement: "subprocess_confined".into(),
    }
}

struct Rig {
    session: VariantSession,
    host: VariantHost<RecordingPorts, VecEvents>,
    binding: String,
}

/// Spawn + ABI-bind a `--mode validator` fixture; `None` when the helper
/// or the fixture binary is unavailable (the suite is a real-process
/// lane — absence is a loud skip, never a fabricated pass).
fn rig(exec_args: Vec<String>) -> Option<Rig> {
    let bin = fixture_bin()?;
    if hh_env::helper::helper_binary().is_none() {
        eprintln!("validator conformance: hh-helper binary not found — skipped");
        return None;
    }
    let pkg = VariantPackage {
        root: bin.parent().unwrap().to_path_buf(),
        manifest: manifest(),
        content: "content:fixture".into(),
        version_id: "pin:fixture".into(),
        execs: vec![bin.to_path_buf()],
    };
    let s = spec(pkg, exec_args);
    let (mut session, _lowered) = spawn(&s).expect("spawn");
    let mut host: VariantHost<RecordingPorts, VecEvents> =
        VariantHost::new(RecordingPorts::default(), VecEvents::default());
    let binding = host.bind(&mut session, bind_params()).expect("bind");
    Some(Rig {
        session,
        host,
        binding,
    })
}

impl Rig {
    fn invoke(&mut self, op: &str, inputs: Vec<Json>) -> Vec<Json> {
        match self
            .host
            .invoke(
                &mut self.session,
                &self.binding,
                op,
                inputs,
                "res-1",
                TIMEOUT,
            )
            .expect("invoke")
        {
            InvokeOutcome::Outputs(o) => o,
            other => panic!("invoke {op}: {other:?}"),
        }
    }

    /// The declared doc (`declare` must succeed — this is the positive
    /// lane's own precondition).
    fn declare(&mut self) -> Json {
        let mut out = self.invoke("declare", vec![]);
        let doc = out.remove(0);
        assert!(
            doc.get("error").is_none(),
            "declare answered a contract error: {doc:?}"
        );
        doc
    }

    fn bind_validator(&mut self, decl: &Json) -> Json {
        let mut out = self.invoke("bind", vec![decl.clone()]);
        out.remove(0)
    }

    fn close(mut self) {
        self.host.close(&mut self.session).unwrap();
        assert!(self.session.detached);
    }
}

fn err_kind(doc: &Json) -> Option<String> {
    doc.get("error")
        .and_then(|e| e.get("kind"))
        .and_then(Json::as_str)
        .map(str::to_string)
}

fn env_handle(kind: &str, auth: &str, r: &str) -> Json {
    Json::obj([
        ("kind", Json::str(kind.to_string())),
        ("ref", Json::str(r.to_string())),
        ("authority", Json::str(auth.to_string())),
        ("produced_at_seq", Json::Int(1)),
    ])
}

// ── positive legs ────────────────────────────────────────────────────────────

#[test]
fn declare_returns_the_complete_declaration() {
    let Some(mut r) = rig(vec![]) else {
        return;
    };
    let decl = r.declare();
    assert_eq!(
        decl.get("validator_ref")
            .and_then(|v| v.get("version_id"))
            .and_then(Json::as_str),
        Some("validator/hh-plugin-fixture@1")
    );
    assert_eq!(decl.get("kind").and_then(Json::as_str), Some("predicate"));
    assert_eq!(
        decl.get("oracle_class").and_then(Json::as_str),
        Some("executable")
    );
    assert_eq!(
        decl.get("deterministic"),
        Some(&Json::Bool(true)),
        "deterministic must be declared, not defaulted"
    );
    let inputs = match decl.get("evidence_inputs") {
        Some(Json::Arr(a)) => a,
        other => panic!("evidence_inputs: {other:?}"),
    };
    assert!(!inputs.is_empty(), "I-V1: evidence_inputs ≠ ∅");
    assert_eq!(
        inputs[0].get("min_authority").and_then(Json::as_str),
        Some("environment")
    );
    assert_eq!(
        decl.get("verdict_type").and_then(Json::as_str),
        Some("bool")
    );
    assert_eq!(
        decl.get("charged_to").and_then(Json::as_str),
        Some("instrument")
    );
    assert_eq!(
        decl.get("isolation").and_then(Json::as_str),
        Some("external")
    );
    r.close();
}

#[test]
fn bind_collect_check_runs_the_contract_end_to_end() {
    let Some(mut r) = rig(vec![]) else {
        return;
    };
    let decl = r.declare();
    let bound = r.bind_validator(&decl);
    assert_eq!(
        bound.get("bound"),
        Some(&Json::Bool(true)),
        "bind: {bound:?}"
    );

    let bundle = {
        let mut out = r.invoke(
            "collect",
            vec![Json::obj([
                (
                    "handles",
                    Json::Arr(vec![env_handle(
                        "effect_record",
                        "environment",
                        "effect:e1",
                    )]),
                ),
                ("target", Json::str("run:r1".to_string())),
            ])],
        );
        out.remove(0)
    };
    assert!(
        bundle.get("error").is_none(),
        "collect answered an error: {bundle:?}"
    );
    assert!(
        bundle
            .get("inputs_digest")
            .and_then(Json::as_str)
            .is_some_and(|d| !d.is_empty()),
        "a bundle carries its content address: {bundle:?}"
    );
    let bundle_id = bundle
        .get("bundle_id")
        .and_then(Json::as_str)
        .expect("bundle_id")
        .to_string();

    let verdict = {
        let mut out = r.invoke("check", vec![bundle.clone()]);
        out.remove(0)
    };
    assert!(
        verdict.get("error").is_none(),
        "check answered an error: {verdict:?}"
    );
    assert_eq!(
        verdict.get("status").and_then(Json::as_str),
        Some("decided")
    );
    assert_eq!(verdict.get("value"), Some(&Json::Bool(true)));
    assert_eq!(
        verdict.get("detector").and_then(Json::as_str),
        Some("deterministic")
    );
    assert_eq!(
        verdict.get("inputs_digest").and_then(Json::as_str),
        bundle.get("inputs_digest").and_then(Json::as_str),
        "the verdict's R1 replay anchor is the bundle's inputs_digest"
    );
    assert_eq!(
        verdict.get("bundle_id").and_then(Json::as_str),
        Some(bundle_id.as_str())
    );
    assert_eq!(
        verdict.get("charged_to").and_then(Json::as_str),
        Some("instrument")
    );
    r.close();
}

#[test]
fn check_is_deterministic_over_the_same_bundle() {
    let Some(mut r) = rig(vec![]) else {
        return;
    };
    let decl = r.declare();
    r.bind_validator(&decl);
    let bundle = {
        let mut out = r.invoke(
            "collect",
            vec![Json::obj([
                (
                    "handles",
                    Json::Arr(vec![env_handle(
                        "effect_record",
                        "environment",
                        "effect:e1",
                    )]),
                ),
                ("target", Json::str("run:r1".to_string())),
            ])],
        );
        out.remove(0)
    };
    let a = r.invoke("check", vec![bundle.clone()]).remove(0);
    let b = r.invoke("check", vec![bundle]).remove(0);
    assert_eq!(
        a.get("verdict_id"),
        b.get("verdict_id"),
        "deterministic ⇒ pure in (inputs_digest, validator version_id)"
    );
    assert_eq!(a.get("inputs_digest"), b.get("inputs_digest"));
    r.close();
}

// ── negative legs (typed refusals, never silent degrade) ─────────────────────

#[test]
fn collect_and_check_before_bind_answer_unbound_fixture() {
    let Some(mut r) = rig(vec![]) else {
        return;
    };
    let collect = r
        .invoke(
            "collect",
            vec![Json::obj([
                ("handles", Json::Arr(vec![])),
                ("target", Json::str("run:r1".to_string())),
            ])],
        )
        .remove(0);
    assert_eq!(err_kind(&collect).as_deref(), Some("UnboundFixture"));
    let check = r.invoke("check", vec![Json::obj([])]).remove(0);
    assert_eq!(err_kind(&check).as_deref(), Some("UnboundFixture"));
    r.close();
}

#[test]
fn bind_with_a_foreign_validator_ref_is_payload_mismatch() {
    let Some(mut r) = rig(vec![]) else {
        return;
    };
    let mut decl = r.declare();
    if let Json::Obj(m) = &mut decl {
        m.insert(
            "validator_ref".to_string(),
            Json::obj([("version_id", Json::str("validator/impostor@9".to_string()))]),
        );
    }
    let bound = r.bind_validator(&decl);
    assert_eq!(
        err_kind(&bound).as_deref(),
        Some("PayloadHashMismatch"),
        "binding a declaration that is not the fixture's pin must refuse: {bound:?}"
    );
    r.close();
}

#[test]
fn effect_bearing_declaration_is_refused_at_bind() {
    let Some(mut r) = rig(vec!["--validator-side-effects".into(), "fs_write".into()]) else {
        return;
    };
    let decl = r.declare();
    let bound = r.bind_validator(&decl);
    assert_eq!(
        err_kind(&bound).as_deref(),
        Some("ValidatorHasEffects"),
        "non-read-only side_effects must fail `bind`: {bound:?}"
    );
    r.close();
}

#[test]
fn empty_evidence_inputs_is_refused_at_declare() {
    let Some(mut r) = rig(vec!["--validator-empty-evidence".into(), "yes".into()]) else {
        return;
    };
    let doc = r.invoke("declare", vec![]).remove(0);
    assert_eq!(
        err_kind(&doc).as_deref(),
        Some("IncompleteDeclaration"),
        "I-V1: evidence_inputs = ∅ must fail `declare`: {doc:?}"
    );
    r.close();
}

#[test]
fn claim_only_and_unauthoritative_evidence_are_refused_at_collect() {
    let Some(mut r) = rig(vec![]) else {
        return;
    };
    let decl = r.declare();
    r.bind_validator(&decl);

    // `model_io` is judges-only — a validator bundle carrying it is
    // ClaimOnlyEvidence (I-V4's evidence-side leg).
    let claim_only = r
        .invoke(
            "collect",
            vec![Json::obj([
                (
                    "handles",
                    Json::Arr(vec![env_handle("model_io", "environment", "ev:m1")]),
                ),
                ("target", Json::str("run:r1".to_string())),
            ])],
        )
        .remove(0);
    assert_eq!(err_kind(&claim_only).as_deref(), Some("ClaimOnlyEvidence"));

    // `external` authority is below the `environment` floor — rejected,
    // never silently weakened.
    let weak = r
        .invoke(
            "collect",
            vec![Json::obj([
                (
                    "handles",
                    Json::Arr(vec![env_handle("effect_record", "external", "effect:e1")]),
                ),
                ("target", Json::str("run:r1".to_string())),
            ])],
        )
        .remove(0);
    assert_eq!(err_kind(&weak).as_deref(), Some("EvidenceUnauthoritative"));
    r.close();
}

#[test]
fn an_undeclared_op_is_the_typed_unhandled_answer() {
    let Some(mut r) = rig(vec![]) else {
        return;
    };
    let out = r.host.invoke(
        &mut r.session,
        &r.binding,
        "frobnicate",
        vec![],
        "res-1",
        TIMEOUT,
    );
    assert_eq!(
        out.unwrap(),
        InvokeOutcome::Failed(AbiError::UnhandledOperation)
    );
    r.close();
}
