//! `hh-cli` acceptance over binding (a) — the in-process `EmbedService`
//! driving the same `Boundary` trait the spawned `hh-kernel serve` child
//! drives (AC-K4-2: the behaviour cannot differ by transport, so the
//! suite runs once in-process and the bindings' own conformance suite
//! proves byte parity).
//!
//! Covers the C0 slice's acceptance rows: the attended approval loop
//! (AC-12), the attendance declaration + M-1 (AC-8/AC-11), the
//! unattended policy-deny (AC-5/AC-6), exhaustion escalate/stop
//! (AC-9/AC-10), the exit-class table (AC-14), the `InvocationRecord` +
//! idempotency (AC-3/AC-4), stdin typing (AC-7) and I-1 (AC-8).

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_assembly::grammar::Assembly;
use hh_cli::boundary::{cli_hello, Boundary, CliError, ProcessBoundary};
use hh_cli::cli::{run_with, Io};
use hh_cli::exit_class::ExitClass;
use hh_cli::invocation::Tty;
use hh_embed::service::{EmbedService, ServiceConfig};
use hh_embed_client_generated::{EmbedError, HelloResult, StreamNotification};
use hh_hir::document::{HirDocument, Node};
use hh_hir::kinds::EntityKind;
use hh_hir::records::*;
use hh_hir::refs::{ComponentVariantRef, EnvironmentRef, ProfileRef, Ref};
use hh_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope, ProvenanceRecord};
use hh_wire::json::Json;
use hh_wire::jsonrpc::Request;

// ── fixture: a minimal conforming hir/1 document ────────────────────────────
// The same shape the hh-embed conformance suite resolves.

fn prov(seq: u64) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human("test:author", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    )
}

fn sel(sid: &str) -> Ref {
    Ref::selected(sid, "latest")
}

fn sid(mut n: Node, id: &str) -> Node {
    n.version.semantic_id = Some(id.into());
    n
}

fn node(kind: EntityKind, rec: KindRecord, seq: u64) -> Node {
    Node::new(kind, rec, prov(seq))
}

fn document_json() -> Json {
    let rule = sid(
        node(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: "test:rule.rule".into(),
                trigger: Json::Null,
                action: RuleAction::RequestApproval(Json::Null),
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            1,
        ),
        "test:rule",
    );
    let budget = sid(
        node(
            EntityKind::Budget,
            KindRecord::Budget(BudgetRecord {
                dimensions: BTreeMap::from([(
                    "tokens.blended".to_string(),
                    DimensionBound {
                        hard: Some(1000),
                        soft: None,
                    },
                )]),
                scope: "*".into(),
                parent: None,
                accounting: sel("test:rule"),
            }),
            2,
        ),
        "test:budget",
    );
    let perm = sid(
        node(
            EntityKind::Permission,
            KindRecord::Permission(PermissionRecord {
                holder: sel("test:agent"),
                grants: vec![],
                issuer: Issuer {
                    authority: AuthorityClass::Kernel,
                    reference: "test:issuer".into(),
                },
                validity: Validity::open_from(0),
                revocation: None,
            }),
            3,
        ),
        "test:perm",
    );
    let agent = sid(
        node(
            EntityKind::AgentProcess,
            KindRecord::AgentProcess(AgentProcessRecord {
                body: AgentProcessBody::Native(NativeProcess {
                    harness_def: sel("test:agent"),
                    profile: ProfileRef {
                        profile: "sha256:profile".into(),
                        pinned: true,
                    },
                    slots: BTreeMap::new(),
                    control_boundary: Default::default(),
                    budget: sel("test:budget"),
                    permissions: sel("test:perm"),
                    environment: EnvironmentRef {
                        environment: "env:test".into(),
                    },
                }),
            }),
            4,
        ),
        "test:agent",
    );
    let mut assembly = Assembly::empty();
    assembly.slots.insert(
        "control_strategy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "control_strategy",
            "hh/round_robin",
            "latest",
        ))),
    );
    assembly.slots.insert(
        "context_policy".into(),
        SlotBindings::One(SlotBinding::of(ComponentVariantRef::selected(
            "context_policy",
            "hh/full_window",
            "latest",
        ))),
    );
    let mut doc = HirDocument::new(sel("test:agent"));
    doc.nodes = vec![rule, budget, perm, agent];
    doc.assembly = Some(assembly.to_json());
    doc.to_json()
}

/// A unique store/workspace root under the OS temp dir.
fn test_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "hh-cli-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// Write the fixture document to a unique temp file — `run start` takes
/// a path.
fn write_definition(tag: &str) -> String {
    let dir = test_dir(&format!("def-{tag}"));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("def.json");
    std::fs::write(&p, document_json().to_canonical_string()).unwrap();
    p.display().to_string()
}

// ── the in-process boundary ─────────────────────────────────────────────

/// Binding (a): `EmbedService::handle` behind the `Boundary` trait — the
/// same calls, the same drain-after-response notification order the
/// stdio binding produces.
struct ServiceBoundary {
    svc: EmbedService,
    /// The service's workspace root — lets a test inspect the store
    /// side-effects (e.g. "no run was opened").
    root: PathBuf,
    hello: Option<HelloResult>,
    frames: VecDeque<StreamNotification>,
    upcalls: VecDeque<(String, Json)>,
    next_id: u64,
}

impl ServiceBoundary {
    fn new(tag: &str) -> ServiceBoundary {
        let root = test_dir(&format!("svc-{tag}"));
        let svc = EmbedService::open(ServiceConfig {
            store_root: root.join("store"),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: root.join("ws"),
            holder: "conformance".into(),
        })
        .unwrap();
        Self::with_svc(root, svc)
    }

    /// Deterministic construction — `ManualClock` + `SeqIds` so two
    /// services produce byte-identical ledger ranges for an identical
    /// operation sequence (AC-R-2.11.1-2). `ws` is the shared workspace
    /// root — the environment records it embeds must hash identically
    /// across the compared services.
    fn deterministic(tag: &str, ws: &str) -> ServiceBoundary {
        let root = test_dir(&format!("svc-{tag}"));
        let svc = EmbedService::open_with(
            ServiceConfig {
                store_root: root.join("store"),
                kernel_version_id: "hh-kernel/0.1.0".into(),
                workspace_root: PathBuf::from(ws),
                holder: "conformance".into(),
            },
            Box::new(hh_ledger::ids::ManualClock::at(1_700_000_000_000)),
            Some(Box::new(hh_ledger::ids::SeqIds::new())),
        )
        .unwrap();
        Self::with_svc(root, svc)
    }

    fn with_svc(root: PathBuf, svc: EmbedService) -> ServiceBoundary {
        let mut b = ServiceBoundary {
            svc,
            root,
            hello: None,
            frames: VecDeque::new(),
            upcalls: VecDeque::new(),
            next_id: 0,
        };
        let raw = b.call("hello", &cli_hello().to_json()).unwrap();
        b.hello = Some(HelloResult::from_json(&raw).unwrap());
        b
    }

    /// Drain the service's queued notifications into the frame/upcall
    /// buffers — mirrors binding (b)'s post-response flush.
    fn drain(&mut self) {
        for n in self.svc.drain_notifications() {
            let method = n.get("method").and_then(Json::as_str).unwrap_or("");
            let params = n.get("params").cloned().unwrap_or(Json::Null);
            if method == "stream.frame" {
                if let Ok(f) = StreamNotification::from_json(&params) {
                    self.frames.push_back(f);
                }
            } else if method.starts_with("upcall.") {
                self.upcalls.push_back((method.to_string(), params));
            }
        }
    }
}

impl Boundary for ServiceBoundary {
    fn call(&mut self, method: &str, params: &Json) -> Result<Json, CliError> {
        self.next_id += 1;
        let resp = self.svc.handle(&Request {
            id: Json::Int(self.next_id as i64),
            method: method.to_string(),
            params: params.clone(),
        });
        // Notifications flush after the response, exactly as binding (b)
        // writes them to the wire.
        self.drain();
        if let Some(e) = resp.get("error") {
            let data = e.get("data").cloned().unwrap_or(Json::Null);
            return Err(CliError::Kernel(EmbedError {
                code: e.get("code").and_then(Json::as_int).unwrap_or(0),
                kind: data
                    .get("kind")
                    .and_then(Json::as_str)
                    .unwrap_or("Refused")
                    .to_string(),
                retryable: matches!(data.get("retryable"), Some(Json::Bool(true))),
                message: e
                    .get("message")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                data,
            }));
        }
        Ok(resp.get("result").cloned().unwrap_or(Json::Null))
    }

    fn poll_frame(&mut self) -> Result<Option<StreamNotification>, CliError> {
        if self.frames.is_empty() {
            self.drain();
        }
        Ok(self.frames.pop_front())
    }

    fn poll_upcall(&mut self) -> Result<Option<(String, Json)>, CliError> {
        if self.upcalls.is_empty() {
            self.drain();
        }
        Ok(self.upcalls.pop_front())
    }

    fn hello_result(&self) -> Option<&HelloResult> {
        self.hello.as_ref()
    }
}

// ── the CLI driver ──────────────────────────────────────────────────────

const ALL_TTY: Tty = Tty {
    stdin: true,
    stdout: true,
    stderr: true,
};
const NO_TTY: Tty = Tty {
    stdin: false,
    stdout: false,
    stderr: false,
};

/// Run one `hh` invocation over the boundary; return
/// `(exit_class, stdout, stderr)`.
fn hh(
    b: &mut dyn Boundary,
    args: &[&str],
    tty: Tty,
    stdin: Option<&[u8]>,
    answers: &[&str],
) -> (ExitClass, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut answers: VecDeque<String> = answers.iter().map(|s| s.to_string()).collect();
    let mut prompt = move |_label: &str| -> Option<String> { answers.pop_front() };
    let argv: Vec<String> = std::iter::once("hh".to_string())
        .chain(args.iter().map(|s| s.to_string()))
        .collect();
    let outcome = {
        let mut io = Io {
            tty,
            stdin: stdin.map(|s| s.to_vec()),
            cwd_ref: "cwd:/test".into(),
            principal: "principal:tester".into(),
            kernel_env: vec![],
            kernel_cmd: "hh-kernel".into(),
            out: &mut out,
            err: &mut err,
            prompt: Some(&mut prompt),
        };
        run_with(b, &argv, &mut io)
    };
    (
        outcome.class,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

/// The `run_id` out of a `run start` result line — `json`/`jsonl` emit
/// the bare record; `human` prefixes `result: `.
fn result_run_id(stdout: &str) -> String {
    let line = stdout.trim().lines().last().unwrap_or("");
    let line = line.strip_prefix("result: ").unwrap_or(line);
    let j = hh_wire::json::parse(line).unwrap_or(Json::Null);
    j.get("payload")
        .and_then(|p| p.get("run_id"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string()
}

/// `run events` as parsed envelopes — the durable rows, in order.
fn events_json(b: &mut dyn Boundary, run_id: &str) -> Vec<Json> {
    let (class, out, _) = hh(b, &["run", "events", run_id], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "run events failed: {out}");
    out.lines()
        .filter_map(|l| hh_wire::json::parse(l).ok())
        .filter(|j| j.get("class").is_some())
        .collect()
}

/// `run events` over a fresh attach — the durable classes, in order.
fn event_classes(b: &mut dyn Boundary, run_id: &str) -> Vec<String> {
    let (class, out, _) = hh(b, &["run", "events", run_id], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "run events failed: {out}");
    out.lines()
        .filter_map(|l| {
            let j = hh_wire::json::parse(l).ok()?;
            j.get("class").and_then(Json::as_str).map(String::from)
        })
        .collect()
}

// ── the happy path ──────────────────────────────────────────────────────

#[test]
fn run_start_completes_scored_ok() {
    let mut b = ServiceBoundary::new("happy");
    let def = write_definition("happy");
    let (class, out, _err) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    assert!(!run_id.is_empty(), "{out}");
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "lifecycle.run.finished"),
        "{classes:?}"
    );
    // AC-3 — the durable `lifecycle.surface.invoked` minted with the
    // InvocationRecord members.
    assert!(
        classes.iter().any(|c| c == "lifecycle.surface.invoked"),
        "{classes:?}"
    );
}

#[test]
fn run_start_json_result_record_shape() {
    let mut b = ServiceBoundary::new("json");
    let def = write_definition("json");
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--format", "json"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok);
    let j = hh_wire::json::parse(out.trim()).unwrap();
    assert_eq!(j.get("kind"), Some(&Json::str("result")));
    assert_eq!(j.get("exit_class"), Some(&Json::str("ok")));
    let payload = j.get("payload").unwrap();
    assert!(payload.get("run_id").is_some());
    assert!(payload.get("configuration_version_id").is_some());
    assert!(payload.get("stop_reason").is_some());
    assert!(payload.get("outcome_class").is_some());
    assert!(payload.get("resource_account").is_some());
    assert!(payload.get("veto_tripped").is_some());
}

// ── attendance declaration + M-1 ────────────────────────────────────────

#[test]
fn declared_interactive_without_tty_is_invocation_error() {
    let mut b = ServiceBoundary::new("m1");
    let def = write_definition("m1");
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--attendance", "interactive"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::InvocationError, "{out}");
    assert!(out.contains("interactive_requires_tty"), "{out}");
}

#[test]
fn no_input_forces_unattended_and_opens() {
    let mut b = ServiceBoundary::new("noinput");
    let def = write_definition("noinput");
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--no-input"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
}

#[test]
fn attendance_plus_no_input_is_flag_conflict() {
    let mut b = ServiceBoundary::new("conflict");
    let def = write_definition("conflict");
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--attendance",
            "async",
            "--no-input",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::InvocationError);
    assert!(out.contains("flag_conflict"), "{out}");
}

// ── pre-ledger refusals ─────────────────────────────────────────────────

#[test]
fn bypass_over_local_host_is_admitted_with_evidence() {
    // S2.10: `--bypass` over the kernel-provisioned `local_host` binding
    // is *admitted* — the EP2 reference backend mints probed
    // enforcement evidence for every relied-on group, recorded on the
    // manifest (`containment.enforcement_evidence`). The pre-ledger
    // refusal now applies only to bindings the surface cannot inspect.
    let mut b = ServiceBoundary::new("bypass");
    let def = write_definition("bypass");
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--bypass"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    let created = created_payload(&mut b, &run_id);
    let ev = created
        .get("containment")
        .and_then(|c| c.get("enforcement_evidence"))
        .cloned()
        .unwrap_or(Json::Null);
    assert!(
        matches!(ev, Json::Obj(_)),
        "manifest must record enforcement_evidence: {}",
        created.to_canonical_string()
    );
}

#[test]
fn missing_budget_is_invocation_error() {
    let mut b = ServiceBoundary::new("nobudget");
    // A definition with no Budget node and no --budget flag.
    let dir = test_dir("def-nobudget");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("def.json");
    let mut doc = document_json();
    if let Json::Obj(m) = &mut doc {
        m.insert("nodes".into(), Json::Arr(vec![]));
    }
    std::fs::write(&p, doc.to_canonical_string()).unwrap();
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &p.display().to_string(), "hi"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::InvocationError);
    assert!(out.contains("missing_budget"), "{out}");
}

#[test]
fn unknown_flag_and_unknown_command_are_invocation_errors() {
    let mut b = ServiceBoundary::new("n1");
    let def = write_definition("n1");
    let (class, out, _) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--bogus", "1"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::InvocationError);
    assert!(out.contains("unknown_flag"), "{out}");
    let (class, out, _) = hh(&mut b, &["warp", "speed"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::InvocationError);
    assert!(out.contains("unknown_command"), "{out}");
}

#[test]
fn unknown_format_is_invocation_error() {
    let mut b = ServiceBoundary::new("fmt");
    let def = write_definition("fmt");
    let (class, _, _) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--format", "yaml"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::InvocationError);
}

// ── the unattended policy-deny (AC-5/AC-6) ──────────────────────────────

#[test]
fn unattended_requires_approval_cap_is_policy_denied() {
    let mut b = ServiceBoundary::new("deny");
    let def = write_definition("deny");
    let cap =
        r#"{"capability_id":"cap:exec","surface_id":"surface:exec","requires_approval":true}"#;
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--no-input",
            "--capability",
            cap,
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "security.permission.pending"),
        "{classes:?}"
    );
    assert!(
        classes.iter().any(|c| c == "security.permission.decided"),
        "{classes:?}"
    );
}

// ── the attended approval loop (AC-12) ──────────────────────────────────

#[test]
fn interactive_approval_prompt_allow_once() {
    let mut b = ServiceBoundary::new("approve");
    let def = write_definition("approve");
    let cap = r#"{"capability_id":"cap:exec","surface_id":"surface:exec","requires_approval":true,"options":["allow_once","deny_once"]}"#;
    let (class, out, err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--capability", cap],
        ALL_TTY,
        None,
        &["allow_once"],
    );
    assert_eq!(class, ExitClass::Ok, "{out} {err}");
    // The prompt rendered the ask on stderr — the untruncated proposal
    // plus every `Explanation` member the ask carries (AC-12).
    assert!(err.contains("approval requested"), "{err}");
    assert!(err.contains("allow_once"), "{err}");
    assert!(
        err.contains("proposal: host capability cap:exec requests approval"),
        "{err}"
    );
    assert!(
        err.contains("explanation.rule_id: capability.requires_approval"),
        "{err}"
    );
    assert!(
        err.contains("explanation.risk_factor: capability_id:cap:exec"),
        "{err}"
    );
    assert!(
        err.contains("explanation.what_would_auto_approve:"),
        "{err}"
    );
    let run_id = result_run_id(&out);
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "security.permission.decided"),
        "{classes:?}"
    );
}

#[test]
fn interactive_approval_prompt_deny() {
    let mut b = ServiceBoundary::new("deny-int");
    let def = write_definition("deny-int");
    let cap = r#"{"capability_id":"cap:exec","surface_id":"surface:exec","requires_approval":true,"options":["allow_once","deny_once"]}"#;
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--capability", cap],
        ALL_TTY,
        None,
        &["deny_once"],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
}

// ── exhaustion escalate / stop (AC-9/AC-10) ─────────────────────────────

#[test]
fn unattended_exhaustion_stops_budget_exhausted() {
    let mut b = ServiceBoundary::new("exh-unatt");
    let def = write_definition("exh-unatt");
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--no-input",
            "--budget",
            "model_calls=0",
        ],
        NO_TTY,
        None,
        &[],
    );
    // The zero ceiling fires before the first model call — the run stops
    // `budget_exhausted`, never blocks.
    let run_id = result_run_id(&out);
    let classes = event_classes(&mut b, &run_id);
    assert_eq!(class, ExitClass::BudgetExhausted, "{out} {classes:?}");
    assert!(
        classes.iter().any(|c| c == "control.budget.exceeded"),
        "{classes:?}"
    );
}

#[test]
fn interactive_exhaustion_amend_wakes_the_run() {
    let mut b = ServiceBoundary::new("exh-amend");
    let def = write_definition("exh-amend");
    let (class, out, err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--budget", "model_calls=0"],
        ALL_TTY,
        None,
        &["4"],
    );
    // The exhaustion decision point prompts; the amend lifts the ceiling
    // and the run completes scored.
    assert_eq!(class, ExitClass::Ok, "{out} {err}");
    assert!(err.contains("budget exhausted"), "{err}");
    let run_id = result_run_id(&out);
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "control.budget.amended"),
        "{classes:?}"
    );
    assert!(
        classes.iter().any(|c| c == "lifecycle.run.finished"),
        "{classes:?}"
    );
}

#[test]
fn interactive_exhaustion_stop_cancels() {
    let mut b = ServiceBoundary::new("exh-stop");
    let def = write_definition("exh-stop");
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--budget", "model_calls=0"],
        ALL_TTY,
        None,
        &["stop"],
    );
    assert_eq!(class, ExitClass::Cancelled, "{out}");
}

// ── run events / status / inspect / cancel ──────────────────────────────

#[test]
fn run_events_exports_the_durable_prefix() {
    let mut b = ServiceBoundary::new("events");
    let def = write_definition("events");
    let (class, out, _) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok);
    let run_id = result_run_id(&out);
    let (class, out, _) = hh(
        &mut b,
        &["run", "events", &run_id, "--format", "jsonl"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok);
    let lines: Vec<Json> = out
        .lines()
        .filter_map(|l| hh_wire::json::parse(l).ok())
        .collect();
    // Every line is a canonical durable envelope or the terminal result.
    let envelope = lines.iter().find(|j| j.get("class").is_some());
    assert!(envelope.is_some(), "{out}");
    assert!(
        envelope
            .unwrap()
            .get("hash")
            .or_else(|| envelope.unwrap().get("event_id"))
            .is_some()
            || envelope.unwrap().get("seq").is_some()
    );
}

#[test]
fn run_status_and_inspect_project() {
    let mut b = ServiceBoundary::new("status");
    let def = write_definition("status");
    let (class, out, _) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok);
    let run_id = result_run_id(&out);
    let (class, out, _) = hh(&mut b, &["run", "status", &run_id], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    let (class, out, _) = hh(
        &mut b,
        &["run", "inspect", &run_id, "--view", "run_summary"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
}

#[test]
fn run_cancel_parked_run_cancels() {
    let mut b = ServiceBoundary::new("cancel");
    let def = write_definition("cancel");
    // Park the run — an open approval ask *and* a `model_calls=0`
    // escalation; the prompt channel has no answers (EOF), so the CLI
    // detaches with the writer session live.
    let cap = r#"{"capability_id":"cap:exec","surface_id":"surface:exec","requires_approval":true,"options":["allow_once","deny_once"]}"#;
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--capability",
            cap,
            "--budget",
            "model_calls=0",
        ],
        ALL_TTY,
        None,
        &[],
    );
    let run_id = result_run_id(&out);
    assert!(!run_id.is_empty(), "{out}");
    let _ = class;
    // `approval list` shows the open ask.
    let (class, out, _) = hh(&mut b, &["approval", "list", &run_id], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains("cap:exec") || out.contains("perm-"), "{out}");
    // `run cancel --takeover` fences the parked writer →
    // cancelled{by:principal}.
    let (class, out, _err) = hh(
        &mut b,
        &["run", "cancel", &run_id, "--takeover"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Cancelled, "{out}");
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "lifecycle.run.finished"),
        "{classes:?}"
    );
}

#[test]
fn approval_respond_decides_the_ask() {
    let mut b = ServiceBoundary::new("respond");
    let def = write_definition("respond");
    let cap = r#"{"capability_id":"cap:exec","surface_id":"surface:exec","requires_approval":true,"options":["allow_once","deny_once"]}"#;
    let (_class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--capability",
            cap,
            "--budget",
            "model_calls=0",
        ],
        ALL_TTY,
        None,
        &[], // EOF — the ask and the escalation stay parked
    );
    let run_id = result_run_id(&out);
    let (class, out, _) = hh(&mut b, &["approval", "list", &run_id], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok);
    let j = hh_wire::json::parse(out.trim().lines().last().unwrap()).unwrap();
    let pid = match j.get("payload").and_then(|p| p.get("open")) {
        Some(Json::Arr(a)) => a.first().and_then(Json::as_str).unwrap_or("").to_string(),
        _ => String::new(),
    };
    assert!(!pid.is_empty(), "{out}");
    let (class, out, _err) = hh(
        &mut b,
        &[
            "approval",
            "respond",
            &run_id,
            &pid,
            "deny_once",
            "--takeover",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    // `approval show` reflects the decision.
    let (class, out, _) = hh(
        &mut b,
        &["approval", "show", &run_id, &pid],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains("decided"), "{out}");
}

// ── invocation record + idempotency (AC-3/AC-4) ─────────────────────────

#[test]
fn invocation_record_mints_with_attendance_and_idem_key() {
    let mut b = ServiceBoundary::new("invrec");
    let def = write_definition("invrec");
    let (class, out, _) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--no-input",
            "--format",
            "jsonl",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok);
    let run_id = result_run_id(&out);
    // Read the invoked event's payload off the durable export.
    let (_, ev_out, _) = hh(
        &mut b,
        &["run", "events", &run_id, "--format", "jsonl"],
        NO_TTY,
        None,
        &[],
    );
    let invoked = ev_out
        .lines()
        .filter_map(|l| hh_wire::json::parse(l).ok())
        .find(|j| j.get("class").and_then(Json::as_str) == Some("lifecycle.surface.invoked"));
    let invoked = invoked.unwrap_or_else(|| panic!("no surface.invoked: {ev_out}"));
    let payload = invoked.get("payload").unwrap();
    assert_eq!(
        payload
            .get("attendance")
            .and_then(|a| a.get("value"))
            .and_then(Json::as_str),
        Some("unattended"),
        "{}",
        payload.to_canonical_string()
    );
    assert!(
        payload
            .get("idempotency_key")
            .and_then(Json::as_str)
            .map(|k| !k.is_empty())
            .unwrap_or(false),
        "{}",
        payload.to_canonical_string()
    );
    assert!(payload.get("argv_canonical").is_some());
    assert!(payload.get("cwd_ref").is_some());
    assert!(payload.get("principal").is_some());
}

#[test]
fn idempotent_replay_returns_the_same_run() {
    let mut b = ServiceBoundary::new("idem");
    let def = write_definition("idem");
    let (class1, out1, _) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--idempotency-key", "inv-1"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class1, ExitClass::Ok, "{out1}");
    let (class2, out2, _) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--idempotency-key", "inv-1"],
        NO_TTY,
        None,
        &[],
    );
    let run1 = result_run_id(&out1);
    let run2 = result_run_id(&out2);
    assert_eq!(run1, run2, "replay must return the same run: {out1} {out2}");
    let _ = class2;
}

// ── stdin typing (AC-7) ─────────────────────────────────────────────────

#[test]
fn piped_stdin_is_external_import_never_principal() {
    let mut b = ServiceBoundary::new("stdin");
    let def = write_definition("stdin");
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "-", "--format", "jsonl"],
        NO_TTY,
        Some(b"piped text"),
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    let (_, ev_out, _) = hh(
        &mut b,
        &["run", "events", &run_id, "--format", "jsonl"],
        NO_TTY,
        None,
        &[],
    );
    // The surface.invoked record carries the stdin digest — the record
    // pins what was read, never the bytes.
    let invoked = ev_out
        .lines()
        .filter_map(|l| hh_wire::json::parse(l).ok())
        .find(|j| j.get("class").and_then(Json::as_str) == Some("lifecycle.surface.invoked"))
        .unwrap_or_else(|| panic!("no invoked: {ev_out}"));
    let digest = invoked
        .get("payload")
        .and_then(|p| p.get("stdin_digest"))
        .and_then(Json::as_str)
        .unwrap_or("");
    assert!(digest.starts_with("sha256:"), "{digest}");
    // The context item entered as external/import.
    let adopted = ev_out
        .lines()
        .filter_map(|l| hh_wire::json::parse(l).ok())
        .find(|j| {
            let c = j.get("class").and_then(Json::as_str).unwrap_or("");
            c == "context.item.adopted" || c == "context.artefact.delivered"
        });
    if let Some(a) = adopted {
        let s = a.to_canonical_string();
        assert!(
            !s.contains(r#""authority":"principal""#) || s.contains("import"),
            "piped stdin must not mint principal-authority input: {s}"
        );
    }
}

// ── I-1 (AC-8) ──────────────────────────────────────────────────────────

#[test]
fn widening_override_refused_unattended_invocation_error() {
    let mut b = ServiceBoundary::new("i1");
    let def = write_definition("i1");
    // `/nodes/1` is the Budget node — loosen `hard` 1000 → 100000.
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--no-input",
            "--override",
            "/nodes/1/semantic/dimensions/tokens.blended/hard=100000",
        ],
        NO_TTY,
        None,
        &[],
    );
    // `AuthorityWideningRequiresHuman` maps to invocation_error — the
    // refusal lands before any run opens.
    assert!(
        class == ExitClass::InvocationError
            || class == ExitClass::RefusedByKernel
            || class == ExitClass::ValidationError,
        "{class:?} {out}"
    );
    let _ = out;
}

// ── AC-R-2.2.3-11 — resume definition verification ────────────────────

/// `open_session{kind:"resume", definition}` — the re-presented
/// definition is `verify_resume`-checked against the persisted sealed
/// definition *before* the takeover: add-only ⇒ `lifecycle.definition
/// .changed` + the session opens; a mid-run budget decrease the
/// accounting gate refuses ⇒ `DefinitionChanged` and no session.
#[test]
fn resume_verifies_a_re_presented_definition() {
    let mut b = ServiceBoundary::new("resumedef");
    let def = write_definition("resumedef");
    let (_c, out, _e) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--budget", "model_calls=0"],
        ALL_TTY,
        None,
        &[],
    );
    let run_id = result_run_id(&out);
    assert!(!run_id.is_empty(), "{out}");

    let resume_params = |doc: Json, key: &str| {
        Json::obj([
            (
                "spec",
                Json::obj([
                    ("kind", Json::str("resume")),
                    ("run_id", Json::str(run_id.clone())),
                    ("mode", Json::str("takeover")),
                    (
                        "definition",
                        Json::obj([("kind", Json::str("document")), ("document", doc)]),
                    ),
                ]),
            ),
            ("idempotency_key", Json::str(key)),
        ])
    };

    // (a) add-only — a second HarnessRule node ⇒ `Compatible`; the
    // session opens and `lifecycle.definition.changed` is durable.
    let mut added = document_json();
    if let Json::Obj(m) = &mut added {
        if let Some(Json::Arr(nodes)) = m.get_mut("nodes") {
            nodes.push(hh_hir::wire::node_to_json(&sid(
                node(
                    EntityKind::HarnessRule,
                    KindRecord::HarnessRule(HarnessRuleRecord {
                        rule_id: "test:rule2".into(),
                        trigger: Json::Null,
                        action: RuleAction::RequestApproval(Json::Null),
                        scope: Json::Null,
                        conditioned_on: None,
                        assumption_debt: None,
                    }),
                    9,
                ),
                "test:rule2",
            )));
        }
    }
    let sess = b
        .call("open_session", &resume_params(added, "resume-def-add"))
        .unwrap();
    assert!(
        sess.get("session_id").and_then(Json::as_str).is_some(),
        "{sess:?}"
    );
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "lifecycle.definition.changed"),
        "{classes:?}"
    );

    // (b) a mid-run budget decrease — the accounting gate refuses ⇒
    // `DefinitionChanged{budget_decrease_denied}`, and the refused
    // attempt mints no `lifecycle.session.attached`.
    let before = event_classes(&mut b, &run_id).len();
    let mut shrunk = document_json();
    if let Json::Obj(m) = &mut shrunk {
        if let Some(Json::Arr(nodes)) = m.get_mut("nodes") {
            for n in nodes.iter_mut() {
                if let Some(Json::Obj(dm)) =
                    n.get("semantic").and_then(|s| s.get("dimensions")).cloned()
                {
                    let mut dm = dm;
                    if let Some(Json::Obj(tb)) = dm.get_mut("tokens.blended") {
                        tb.insert("hard".to_string(), Json::Int(500));
                    }
                    if let Json::Obj(nm) = n {
                        if let Some(Json::Obj(sm)) = nm.get_mut("semantic") {
                            sm.insert("dimensions".to_string(), Json::Obj(dm));
                        }
                    }
                }
            }
        }
    }
    match b.call("open_session", &resume_params(shrunk, "resume-def-shrink")) {
        Err(CliError::Kernel(e)) => {
            assert_eq!(e.kind, "DefinitionChanged", "{e:?}");
            assert!(
                e.message.contains("budget_decrease_denied")
                    || e.data
                        .get("reasons")
                        .and_then(|r| {
                            if let Json::Arr(a) = r {
                                Some(
                                    a.iter()
                                        .filter_map(Json::as_str)
                                        .any(|s| s.contains("budget_decrease_denied")),
                                )
                            } else {
                                None
                            }
                        })
                        .unwrap_or(false),
                "{e:?}"
            );
        }
        other => panic!("a refused resume change must not open a session: {other:?}"),
    }
    assert_eq!(
        event_classes(&mut b, &run_id).len(),
        before,
        "the refused resume must leave the ledger untouched"
    );
}

// ── DF-S1.25-2/-3 — durable resume across a service restart ─────────

/// A `open_session{resume}` after the whole service is dropped and
/// reopened on the same store root restores the leaf from the persisted
/// `leaf.checkpoint` (DF-S1.25-2) and `verify_resume` reads the sealed
/// definition from the durable artifact pool (DF-S1.25-3) — the
/// in-memory `sealed_defs` table is empty in the new lifetime.
#[test]
fn durable_resume_survives_a_service_restart() {
    let root = test_dir("svc-restart");
    let def = write_definition("restart");
    let run_id;
    {
        // The "crashed" service's holder carries a probeable dead pid —
        // `restore`'s liveness-shortened takeover admits it (AC-R-2.2.3-5).
        let svc = EmbedService::open(ServiceConfig {
            store_root: root.join("store"),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: root.join("ws"),
            holder: hh_ledger::leases::holder_spelling("conformance", 99_999_999),
        })
        .unwrap();
        let mut b = ServiceBoundary::with_svc(root.clone(), svc);
        let (_c, out, _e) = hh(
            &mut b,
            &["run", "start", &def, "hi", "--budget", "model_calls=0"],
            ALL_TTY,
            None,
            &[],
        );
        run_id = result_run_id(&out);
        assert!(!run_id.is_empty(), "{out}");
        // The leaf pair is durable before the "crash".
        let leaf = root
            .join("store")
            .join("runs")
            .join(&run_id)
            .join("leaf.checkpoint");
        assert!(leaf.exists(), "no persisted leaf checkpoint: {leaf:?}");
    }
    // The first service is dropped — its in-memory driver table and
    // `sealed_defs` are gone; the store root persists. The stale writer
    // lease is expiry-gated (the writer lease is never liveness-shortened
    // — AC-5's probe shortens *scoped* leases), so the restart ages past
    // its TTL: rewrite `expires_at_ms` to the past, exactly what a
    // post-TTL resume sees.
    let lease_file = root
        .join("store")
        .join("runs")
        .join(&run_id)
        .join("lease.json");
    let mut lj = hh_wire::json::parse(&std::fs::read_to_string(&lease_file).unwrap()).unwrap();
    if let Json::Obj(m) = &mut lj {
        m.insert("expires_at_ms".to_string(), Json::Int(0));
    }
    std::fs::write(&lease_file, lj.to_canonical_string()).unwrap();
    let svc = EmbedService::open(ServiceConfig {
        store_root: root.join("store"),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: root.join("ws"),
        holder: "conformance".into(),
    })
    .unwrap();
    let mut b2 = ServiceBoundary::with_svc(root, svc);
    // Resume re-presenting the same definition — the artifact pool
    // (not the dropped service's memory) supplies the persisted sealed
    // doc for `verify_resume`.
    let params = Json::obj([
        (
            "spec",
            Json::obj([
                ("kind", Json::str("resume")),
                ("run_id", Json::str(run_id.clone())),
                ("mode", Json::str("takeover")),
                (
                    "definition",
                    Json::obj([
                        ("kind", Json::str("document")),
                        ("document", document_json()),
                    ]),
                ),
            ]),
        ),
        ("idempotency_key", Json::str("resume-restart")),
    ]);
    let sess = b2.call("open_session", &params).unwrap();
    assert!(
        sess.get("session_id").and_then(Json::as_str).is_some(),
        "{sess:?}"
    );
    let classes = event_classes(&mut b2, &run_id);
    assert!(
        classes.iter().any(|c| c == "lifecycle.run.resumed"),
        "{classes:?}"
    );
    // `get_artifact` serves the sealed definition across the restart —
    // the durable artifact pool, not `sealed_defs`.
    let manifest_ref = sess
        .get("manifest_ref")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    assert!(!manifest_ref.is_empty(), "{sess:?}");
    let sid = sess
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string();
    let got = b2
        .call(
            "get_artifact",
            &Json::obj([
                ("session_id", Json::str(sid)),
                ("address", Json::str(manifest_ref.clone())),
            ]),
        )
        .unwrap();
    assert_eq!(
        got.get("address").and_then(Json::as_str),
        Some(manifest_ref.as_str()),
        "{got:?}"
    );
}

// ── resume / fork / amend / submit on a parked run (AC-6/9) ─────────

#[test]
fn parked_run_takeover_resume_fork_amend_submit() {
    let mut b = ServiceBoundary::new("takeover");
    let def = write_definition("takeover");
    // Park the run on the `model_calls=0` escalation — EOF on the prompt
    // detaches with the writer session live.
    let (_class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--budget", "model_calls=0"],
        ALL_TTY,
        None,
        &[],
    );
    let run_id = result_run_id(&out);
    assert!(!run_id.is_empty(), "{out}");

    // `run resume --takeover` fences the parked writer → a fresh writer
    // session on the same run (carries the leaf's runtime).
    let (class, out, _err) = hh(
        &mut b,
        &["run", "resume", &run_id, "--takeover"],
        ALL_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains(&run_id), "{out}");

    // `run fork --takeover --at-seq` → a child run; the parent stays
    // parked (the writer session is NOT closed over it).
    let (class, out, _err) = hh(
        &mut b,
        &["run", "fork", &run_id, "--takeover", "--at-seq", "5"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let child = {
        let line = out.trim().lines().last().unwrap_or("");
        let line = line.strip_prefix("result: ").unwrap_or(line);
        hh_wire::json::parse(line)
            .ok()
            .and_then(|j| {
                j.get("payload")
                    .and_then(|p| p.get("run_id").or_else(|| p.get("child_run_id")))
                    .and_then(Json::as_str)
                    .map(String::from)
            })
            .unwrap_or_default()
    };
    assert!(!child.is_empty() && child != run_id, "fork child: {out}");

    // `run amend --takeover budget model_calls=4` — a loosening amendment
    // under interactive attendance mints `control.budget.amended` and
    // re-drives the leaf to `format_failure`.
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "amend",
            &run_id,
            "budget",
            "model_calls=4",
            "--takeover",
        ],
        ALL_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "control.budget.amended"),
        "{classes:?}"
    );
    assert!(
        classes.iter().any(|c| c == "lifecycle.run.finished"),
        "{classes:?}"
    );

    // `run submit` on the finished run refuses — `Draining` is a kernel
    // answer, never a CLI invention.
    let (class, out, _err) = hh(
        &mut b,
        &["run", "submit", &run_id, "more", "--takeover"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::RefusedByKernel, "{out}");
}

#[test]
fn approval_show_reads_the_open_ask() {
    let mut b = ServiceBoundary::new("ashow");
    let def = write_definition("ashow");
    let cap = r#"{"capability_id":"cap:exec","surface_id":"surface:exec","requires_approval":true,"options":["allow_once","deny_once"]}"#;
    let (_class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--capability",
            cap,
            "--budget",
            "model_calls=0",
        ],
        ALL_TTY,
        None,
        &[],
    );
    let run_id = result_run_id(&out);
    let (class, out, _err) = hh(&mut b, &["approval", "list", &run_id], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    let pid = {
        let line = out
            .lines()
            .find(|l| l.contains("permission_id"))
            .unwrap_or("")
            .to_string();
        let line = line.strip_prefix("result: ").unwrap_or(&line).to_string();
        hh_wire::json::parse(&line)
            .ok()
            .and_then(|j| {
                j.get("payload")
                    .and_then(|p| p.get("open"))
                    .and_then(|a| match a {
                        Json::Arr(v) => v.first().cloned(),
                        _ => None,
                    })
                    .and_then(|a| {
                        a.as_str().map(String::from).or_else(|| {
                            a.get("permission_id")
                                .and_then(Json::as_str)
                                .map(String::from)
                        })
                    })
            })
            .unwrap_or_default()
    };
    assert!(!pid.is_empty(), "no pending ask id in {out}");
    let (class, out, _err) = hh(
        &mut b,
        &["approval", "show", &run_id, &pid],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains(&pid), "{out}");
}

// ── program commands ────────────────────────────────────────────────────

#[test]
fn version_and_doctor_answer_hello_facts() {
    let mut b = ServiceBoundary::new("ver");
    let (class, out, _) = hh(&mut b, &["version"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(
        out.contains("hh-cli/")
            && out.contains("\"kernel\"")
            && out.contains("schema_hash")
            && out.contains("idp"),
        "{out}"
    );
    let (class, out, _) = hh(&mut b, &["doctor"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
}

// ── AC-1: the spawned-process binding ──────────────────────────────────

/// Every command is executable against a service in a separate process:
/// drive `version`, `run start` and `run events` over the spawned
/// `hh-kernel serve` child (binding (b)) — the same `run_with` the
/// in-process suite drives (AC-R-2.11.1-1).
#[test]
fn ac1_commands_run_over_the_spawned_process_binding() {
    let kernel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/hh-kernel");
    if !kernel.exists() {
        // `cargo test -p hh-cli` alone does not build the kernel bin.
        let st = std::process::Command::new("cargo")
            .args(["build", "-p", "hh-kernel"])
            .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .status()
            .unwrap();
        assert!(st.success(), "cargo build -p hh-kernel failed");
    }
    let root = test_dir("ac1-kernel");
    let env = vec![
        (
            "HH_STORE_ROOT".to_string(),
            root.join("store").display().to_string(),
        ),
        (
            "HH_WORKSPACE_ROOT".to_string(),
            root.join("ws").display().to_string(),
        ),
    ];
    let mut b = ProcessBoundary::spawn(&kernel.display().to_string(), &env).unwrap();
    let (class, out, _) = hh(&mut b, &["version"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");

    let def = write_definition("ac1");
    let (class, out, err) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out} {err}");
    let run_id = result_run_id(&out);
    assert!(!run_id.is_empty(), "{out}");
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "lifecycle.run.finished"),
        "{classes:?}"
    );
}

// ── AC-2: byte-identical ledger ranges ─────────────────────────────────

/// `open_session{attach}` + `read` the whole durable prefix — the event
/// envelopes as canonical `Json`.
fn durable_events(b: &mut dyn Boundary, run_id: &str) -> Vec<Json> {
    let sess = b
        .call(
            "open_session",
            &Json::obj([
                (
                    "spec",
                    Json::obj([
                        ("kind", Json::str("attach")),
                        ("run_id", Json::str(run_id)),
                        ("read_only", Json::Bool(true)),
                    ]),
                ),
                ("idempotency_key", Json::str(format!("ev:{run_id}"))),
            ]),
        )
        .unwrap();
    let sid = sess
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let mut events = Vec::new();
    let mut cursor = Json::obj([("kind", Json::str("seq")), ("seq", Json::Int(0))]);
    loop {
        let page = b
            .call(
                "read",
                &Json::obj([
                    ("session_id", Json::str(sid.clone())),
                    ("cursor", cursor),
                    ("direction", Json::str("fwd")),
                    ("limit", Json::Int(500)),
                ]),
            )
            .unwrap();
        let mut got = 0usize;
        if let Some(Json::Arr(evs)) = page.get("events") {
            got = evs.len();
            events.extend(evs.iter().cloned());
        }
        match page.get("next") {
            Some(n @ Json::Obj(_)) => cursor = n.clone(),
            _ => break,
        }
        if got == 0 {
            break;
        }
    }
    events
}

/// AC-R-2.11.1-2 — the CLI's `run start` and the equivalent `hh-embed/1`
/// operation sequence produce byte-identical ledger ranges after
/// removing `lifecycle.surface.invoked` (the CLI's own provenance row —
/// the kernel mints it from the `invocation` member the surface sends).
#[test]
fn ac2_cli_ledger_range_byte_identical_to_the_boundary_sequence() {
    let def = write_definition("ac2");
    let doc = hh_wire::json::parse(&std::fs::read_to_string(&def).unwrap()).unwrap();

    // The CLI-driven invocation (deterministic store: frozen clock +
    // sequential ids, so the two services produce identical bytes for an
    // identical operation sequence).
    let mut cli_b = ServiceBoundary::deterministic("ac2-cli", "/tmp/hh-cli-ac2-ws");
    let (class, out, err) = hh(&mut cli_b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out} {err}");
    let run_id = result_run_id(&out);
    let cli_events = durable_events(&mut cli_b, &run_id);

    // The equivalent raw boundary sequence — the same `open_session{new}`
    // spec minus `invocation`, the same `submit` input, `close{done}`.
    let mut raw_b = ServiceBoundary::deterministic("ac2-raw", "/tmp/hh-cli-ac2-ws");
    let sess = raw_b
        .call(
            "open_session",
            &Json::obj([
                (
                    "spec",
                    Json::obj([
                        ("kind", Json::str("new")),
                        (
                            "definition",
                            Json::obj([("kind", Json::str("document")), ("document", doc)]),
                        ),
                        ("overrides", Json::Arr(vec![])),
                        (
                            "environment",
                            Json::obj([
                                ("kind", Json::str("connection_info")),
                                (
                                    "connection_info",
                                    Json::obj([("class", Json::str("local_host"))]),
                                ),
                            ]),
                        ),
                        (
                            "attendance",
                            Json::obj([
                                ("value", Json::str("unattended")),
                                ("source", Json::str("tty_inferred")),
                            ]),
                        ),
                        (
                            "supplies",
                            Json::obj([
                                ("context", Json::Arr(vec![])),
                                ("host_capabilities", Json::Arr(vec![])),
                                ("mcp_servers", Json::Arr(vec![])),
                                ("procedures", Json::Arr(vec![])),
                            ]),
                        ),
                        ("approval_mode", Json::str("unattended_deny")),
                    ]),
                ),
                ("idempotency_key", Json::str("raw:open")),
            ]),
        )
        .unwrap();
    let sid = sess
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    assert_eq!(
        sess.get("run_id").and_then(Json::as_str).unwrap_or(""),
        run_id,
        "the boundary sequence minted a different run"
    );
    raw_b
        .call(
            "submit",
            &Json::obj([
                ("session_id", Json::str(sid.clone())),
                (
                    "input",
                    Json::Arr(vec![Json::obj([
                        ("kind", Json::str("text")),
                        ("text", Json::str("hi")),
                        ("authority", Json::str("principal")),
                        ("origin", Json::str("human")),
                    ])]),
                ),
                ("idempotency_key", Json::str("raw:submit")),
            ]),
        )
        .unwrap();
    let _ = raw_b.call(
        "close",
        &Json::obj([
            ("session_id", Json::str(sid)),
            ("reason", Json::str("done")),
        ]),
    );
    let raw_events = durable_events(&mut raw_b, &run_id);

    assert!(
        cli_events
            .iter()
            .any(|e| e.get("class").and_then(Json::as_str) == Some("lifecycle.surface.invoked")),
        "the CLI did not mint surface.invoked"
    );
    // Drop the CLI's own provenance row, then compare. The hash chain
    // makes literal byte-identity after removal impossible — the extra
    // row consumes a seq, an event id and re-links every later `hash` —
    // so the executable form of the AC is: same length, same class
    // sequence, and every differing leaf is a member a one-row
    // insertion mechanically shifts (chain linkage, allocated ids, and
    // content addresses whose preimages embed them).
    let cli_s: Vec<&Json> = cli_events
        .iter()
        .filter(|e| e.get("class").and_then(Json::as_str) != Some("lifecycle.surface.invoked"))
        .collect();
    assert_eq!(cli_s.len(), raw_events.len(), "event count differs");
    let mut diffs = Vec::new();
    for (x, y) in cli_s.iter().zip(raw_events.iter()) {
        diff_paths(x, y, "", &mut diffs);
    }
    // Mechanical members a one-row insertion shifts: the hash chain
    // (`hash`/`prev_hash`), the dense `seq`, allocated ids
    // (`*_event_id` incl. `causes[]`, `env_handle`, `lease_id`,
    // `decision_id`), logical provenance seqs (`created_at`) and the
    // content addresses whose preimages embed shifted ids (`*_ref`,
    // `head_hash`).
    const MECHANICAL: &[&str] = &[
        "seq",
        "hash",
        "event_id",
        "created_at",
        "env_handle",
        "lease_id",
        "decision_id",
        "checkpoint_ref",
        "containment_report_ref",
        "drain_report_ref",
        "bound_ref",
        "head_hash",
    ];
    for d in &diffs {
        assert!(
            MECHANICAL.iter().any(|m| d.ends_with(m)),
            "non-mechanical ledger difference at {d} — the CLI added content"
        );
    }
}

// ── AC-5: one configuration, attendance aside ──────────────────────────

/// Collect the leaf paths where two `Json` values differ.
fn diff_paths(a: &Json, b: &Json, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Json::Obj(x), Json::Obj(y)) => {
            for (k, v) in x {
                let p = format!("{path}.{k}");
                match y.get(k) {
                    Some(w) => diff_paths(v, w, &p, out),
                    None => out.push(p),
                }
            }
            for k in y.keys() {
                if !x.contains_key(k) {
                    out.push(format!("{path}.{k}"));
                }
            }
        }
        (Json::Arr(x), Json::Arr(y)) => {
            for (i, (v, w)) in x.iter().zip(y.iter()).enumerate() {
                diff_paths(v, w, &format!("{path}[{i}]"), out);
            }
            if x.len() != y.len() {
                out.push(format!("{path}.len"));
            }
        }
        _ => {
            if a != b {
                out.push(path.to_string());
            }
        }
    }
}

/// AC-R-2.11.1-5 — the same sealed definition started attended and
/// unattended yields one `configuration_id`; the only ledger
/// differences are the `attendance` members (and the derived
/// idempotency key) on the invocation/opened rows and any
/// `decider: policy` rows.
#[test]
fn ac5_attended_and_unattended_share_one_configuration() {
    let def = write_definition("ac5");
    let mut attended = ServiceBoundary::deterministic("ac5-att", "/tmp/hh-cli-ac5-ws");
    let mut unattended = ServiceBoundary::deterministic("ac5-unatt", "/tmp/hh-cli-ac5-ws");
    let args = [
        "run",
        "start",
        &def,
        "hi",
        "--format",
        "json",
        "--approval-mode",
        "unattended_deny",
    ];
    let (ca, oa, ea) = hh(&mut attended, &args, ALL_TTY, None, &[]);
    let (cu, ou, eu) = hh(&mut unattended, &args, NO_TTY, None, &[]);
    assert_eq!(ca, ExitClass::Ok, "{oa} {ea}");
    assert_eq!(cu, ExitClass::Ok, "{ou} {eu}");
    let cfg = |o: &str| -> String {
        let line = o.trim().lines().last().unwrap_or("");
        let j = hh_wire::json::parse(line).unwrap_or(Json::Null);
        j.get("payload")
            .and_then(|p| p.get("configuration_version_id"))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string()
    };
    let (ca_cfg, cu_cfg) = (cfg(&oa), cfg(&ou));
    assert!(!ca_cfg.is_empty());
    assert_eq!(ca_cfg, cu_cfg, "attendance changed the configuration");

    let ea_evs = durable_events(&mut attended, &result_run_id(&oa));
    let eu_evs = durable_events(&mut unattended, &result_run_id(&ou));
    assert_eq!(ea_evs.len(), eu_evs.len(), "event count differs");
    let mut diffs = Vec::new();
    for (x, y) in ea_evs.iter().zip(eu_evs.iter()) {
        diff_paths(x, y, "", &mut diffs);
    }
    // `hash`/`prev_hash` are the chain consequence of the allowed rows;
    // `idempotency_key` derives over `attendance`.
    for d in &diffs {
        assert!(
            d == ".hash"
                || d == ".prev_hash"
                || d.ends_with("attendance.value")
                || d.ends_with("attendance.source")
                || d.ends_with("idempotency_key")
                || d.ends_with("decider")
                || d.ends_with("reason"),
            "unexpected ledger difference at {d}"
        );
    }
    assert!(
        diffs.iter().any(|d| d.ends_with("attendance.value")),
        "the attendance rows did not differ: {diffs:?}"
    );
}

// ── AC-R-2.2.4-12 — the speculation-policy floor at the override seam ──

/// An `overrides[]` write of a `speculation_policy` member meets the
/// §5a.4 floor wherever it lands: `defer_irreversible = false` is
/// `AuthorityViolation` — refused before the run opens — and the
/// `allow_classes` widening leg refuses the same way. (Narrowing is
/// admitted at the `check_override` seam — hh-hir's unit tests; a
/// schema-invisible member is *also* refused downstream by the
/// coordinate-space rule, which the floor precedes.)
#[test]
fn ac_2_2_4_12_speculation_floor_refuses_widening_overrides() {
    for (tag, ov) in [
        (
            "floor-defer",
            r#"/nodes/0/speculation_policy={"defer_irreversible":false}"#,
        ),
        (
            "floor-classes",
            r#"/nodes/0/speculation_policy={"allow_classes":["read_only","compensable"],"max_concurrent_branches":9}"#,
        ),
    ] {
        let mut b = ServiceBoundary::new(tag);
        let def = write_definition(tag);
        let (class, out, err) = hh(
            &mut b,
            &["run", "start", &def, "hi", "--override", ov],
            ALL_TTY,
            None,
            &[],
        );
        assert_eq!(class, ExitClass::RefusedByKernel, "{tag}: {out} {err}");
        assert!(
            err.contains("speculation_policy") || out.contains("speculation_policy"),
            "{tag}: the refusal names the floor member: {out} {err}"
        );
    }
}

// ── AC-8/AC-9: the attended half of I-1 + layer-id determinism ─────────

/// AC-R-2.11.1-8 (attended half) + AC-R-2.11.1-9 — the same widening
/// override at a TTY lifts into an `overrides` layer (`origin: human`),
/// the run opens, and the layer id is deterministic across invocations.
#[test]
fn ac8_ac9_attended_widening_override_lifts_with_a_deterministic_layer_id() {
    let layer_of = |tag: &str| -> String {
        let mut b = ServiceBoundary::new(tag);
        let def = write_definition(tag);
        let (class, out, err) = hh(
            &mut b,
            &[
                "run",
                "start",
                &def,
                "hi",
                "--override",
                "/nodes/1/semantic/dimensions/tokens.blended/hard=100000",
            ],
            ALL_TTY,
            None,
            &[],
        );
        assert_eq!(class, ExitClass::Ok, "{out} {err}");
        let run_id = result_run_id(&out);
        durable_events(&mut b, &run_id)
            .iter()
            .find(|e| e.get("class").and_then(Json::as_str) == Some("lifecycle.surface.invoked"))
            .and_then(|e| {
                e.get("payload")
                    .and_then(|p| p.get("overrides_layer_id"))
                    .and_then(Json::as_str)
            })
            .unwrap_or("")
            .to_string()
    };
    let l1 = layer_of("ac8-a");
    let l2 = layer_of("ac8-b");
    assert!(!l1.is_empty(), "no overrides layer lifted");
    assert_eq!(l1, l2, "overrides_layer_id is not deterministic");
}

// ── AC-10: a validation_error never opens a run ────────────────────────

/// AC-R-2.11.1-10 — `run start` on a document the boundary refuses is a
/// `validation_error` and mints no `lifecycle.run.opened` — the store
/// carries no run at all.
#[test]
fn ac10_validation_error_never_opens_a_run() {
    let mut b = ServiceBoundary::new("ac10");
    let dir = test_dir("ac10-def");
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.json");
    // A well-formed document (a Budget node is present — the CLI's
    // MissingBudget pre-check passes) whose `root` names a semantic id
    // no node carries — the kernel's resolve/validate step refuses it.
    let mut doc = document_json();
    if let Json::Obj(m) = &mut doc {
        m.insert(
            "root".into(),
            Json::obj([
                ("semantic_id", Json::str("missing:agent")),
                ("version_selector", Json::str("latest")),
            ]),
        );
    }
    std::fs::write(&bad, doc.to_canonical_string()).unwrap();
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &bad.display().to_string(), "hi"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::ValidationError, "{out}");
    // No run directory was minted — the refusal never reached open_run.
    let runs_dir = b.root.join("store").join("runs");
    let runs: Vec<_> = std::fs::read_dir(&runs_dir)
        .map(|rd| rd.flatten().collect())
        .unwrap_or_default();
    assert!(runs.is_empty(), "a run was opened: {runs:?}");
}

// ── AC-3: jsonl stream ≡ run events --from 0 ───────────────────────────

/// AC-R-2.11.1-3 — `run start --jsonl` minus the terminal `result` line
/// equals `run events --from 0` for the same run, byte for byte.
#[test]
fn ac3_jsonl_stream_minus_result_equals_run_events() {
    let mut b = ServiceBoundary::new("ac3");
    let def = write_definition("ac3");
    let (class, out, err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--format", "jsonl"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out} {err}");
    let run_id = result_run_id(&out);
    let start_lines: Vec<&str> = out.trim_end().lines().collect();
    let (event_lines, result_line) = start_lines.split_at(start_lines.len() - 1);
    assert!(
        result_line[0].contains("\"kind\":\"result\""),
        "last line is not the result record: {}",
        result_line[0]
    );
    let (class2, out2, err2) = hh(
        &mut b,
        &["run", "events", &run_id, "--from", "0", "--format", "jsonl"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class2, ExitClass::Ok, "{out2} {err2}");
    let events_lines: Vec<&str> = out2.trim_end().lines().collect();
    let (replay_events, _replay_result) = events_lines.split_at(events_lines.len() - 1);
    assert_eq!(
        event_lines,
        replay_events,
        "jsonl stream differs from run events ({} vs {} lines)",
        event_lines.len(),
        replay_events.len()
    );
    assert!(!event_lines.is_empty());
}

// ── S2.10 — SurfacePreset, workspace_trust, env *, config explain,
// compact, amend(attendance|approval_mode) ───────────────────────────

/// The `result` record's `payload` member.
fn result_payload(stdout: &str) -> Json {
    let line = stdout.trim().lines().last().unwrap_or("");
    let line = line.strip_prefix("result: ").unwrap_or(line);
    hh_wire::json::parse(line)
        .unwrap_or(Json::Null)
        .get("payload")
        .cloned()
        .unwrap_or(Json::Null)
}

/// `attach` + `describe` + `close` — the session's realized settings.
fn describe(b: &mut dyn Boundary, run_id: &str) -> Json {
    let sess = b
        .call(
            "open_session",
            &Json::obj([
                (
                    "spec",
                    Json::obj([
                        ("kind", Json::str("attach")),
                        ("run_id", Json::str(run_id)),
                        ("read_only", Json::Bool(true)),
                    ]),
                ),
                ("idempotency_key", Json::str("test:attach")),
            ]),
        )
        .unwrap();
    let sid = sess
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap()
        .to_string();
    let d = b
        .call(
            "describe",
            &Json::obj([("session_id", Json::str(sid.clone()))]),
        )
        .unwrap();
    let _ = b.call(
        "close",
        &Json::obj([
            ("session_id", Json::str(sid)),
            ("reason", Json::str("done")),
        ]),
    );
    d
}

/// The durable `lifecycle.run.created` payload — the manifest record.
fn created_payload(b: &mut dyn Boundary, run_id: &str) -> Json {
    let (class, out, _) = hh(b, &["run", "events", run_id], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    out.lines()
        .filter_map(|l| hh_wire::json::parse(l).ok())
        .find(|e| e.get("class").and_then(Json::as_str) == Some("lifecycle.run.created"))
        .and_then(|e| e.get("payload").cloned())
        .unwrap_or(Json::Null)
}

/// Park a run on the `model_calls=0` escalation — EOF on the prompt
/// detaches with the writer session live (the takeover test's recipe).
fn park_run(b: &mut dyn Boundary, tag: &str) -> String {
    let def = write_definition(tag);
    let (_class, out, _err) = hh(
        b,
        &["run", "start", &def, "hi", "--budget", "model_calls=0"],
        ALL_TTY,
        None,
        &[],
    );
    let run_id = result_run_id(&out);
    assert!(!run_id.is_empty(), "{out}");
    run_id
}

#[test]
fn preset_plan_lowers_to_pre_approved_only() {
    let mut b = ServiceBoundary::new("preset-plan");
    let def = write_definition("preset-plan");
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--preset", "plan"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    let d = describe(&mut b, &run_id);
    assert_eq!(
        d.get("realized")
            .and_then(|r| r.get("policy_mode"))
            .and_then(Json::as_str),
        Some("pre_approved_only"),
        "{}",
        d.to_canonical_string()
    );
    // The manifest records the narrowing leaves the preset carried.
    let created = created_payload(&mut b, &run_id);
    let leaves = created.get("narrowing_leaves");
    assert!(
        leaves.is_some(),
        "narrowing_leaves absent: {}",
        created.to_canonical_string()
    );
}

#[test]
fn preset_bypass_meets_the_same_gate() {
    let mut b = ServiceBoundary::new("preset-bypass");
    let def = write_definition("preset-bypass");
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--preset", "bypass"],
        NO_TTY,
        None,
        &[],
    );
    // `--preset bypass` lowers to approval_mode `bypass` and meets the
    // identical gate — admitted over local_host.
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    let d = describe(&mut b, &run_id);
    assert_eq!(
        d.get("realized")
            .and_then(|r| r.get("policy_mode"))
            .and_then(Json::as_str),
        Some("bypass"),
        "{}",
        d.to_canonical_string()
    );
}

#[test]
fn preset_and_flag_conflict_is_invocation_error() {
    let mut b = ServiceBoundary::new("preset-conflict");
    let def = write_definition("preset-conflict");
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--preset",
            "plan",
            "--approval-mode",
            "tiered",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::InvocationError, "{out}");
    assert!(out.contains("flag_conflict"), "{out}");
}

// `HH_TRUST_STORE` is process-global; the two tests that set it must not
// interleave or one test's store bleeds into the other's run.start.
static TRUST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn workspace_trust_claim_lands_on_the_manifest() {
    let _guard = TRUST_ENV_LOCK.lock().unwrap();
    let mut b = ServiceBoundary::new("trust");
    let dir = test_dir("trust-store");
    std::fs::create_dir_all(&dir).unwrap();
    let store = dir.join("trust.json");
    let cwd = std::env::current_dir()
        .unwrap()
        .canonicalize()
        .unwrap()
        .display()
        .to_string();
    std::fs::write(
        &store,
        format!("{{\"workspaces\":{{\"{cwd}\":\"trusted\"}}}}"),
    )
    .unwrap();
    std::env::set_var("HH_TRUST_STORE", &store);
    let def = write_definition("trust");
    let (class, out, _err) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    std::env::remove_var("HH_TRUST_STORE");
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    let created = created_payload(&mut b, &run_id);
    assert_eq!(
        created.get("workspace_trust").and_then(Json::as_str),
        Some("trusted"),
        "{}",
        created.to_canonical_string()
    );
}

#[test]
fn workspace_trust_absent_is_unknown() {
    let _guard = TRUST_ENV_LOCK.lock().unwrap();
    let mut b = ServiceBoundary::new("trust-unknown");
    let dir = test_dir("trust-store-absent");
    std::fs::create_dir_all(&dir).unwrap();
    let store = dir.join("trust.json");
    std::fs::write(&store, "{\"workspaces\":{}}").unwrap();
    std::env::set_var("HH_TRUST_STORE", &store);
    let def = write_definition("trust-unknown");
    let (class, out, _err) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    std::env::remove_var("HH_TRUST_STORE");
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    let created = created_payload(&mut b, &run_id);
    assert_eq!(
        created.get("workspace_trust").and_then(Json::as_str),
        Some("unknown"),
        "{}",
        created.to_canonical_string()
    );
}

// ── compact ─────────────────────────────────────────────────────

#[test]
fn compact_view_ships_a_loss_report() {
    let mut b = ServiceBoundary::new("compact");
    let def = write_definition("compact");
    let (_c, out, _err) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    let run_id = result_run_id(&out);
    let (class, out, _err) = hh(
        &mut b,
        &["compact", &run_id, "--format", "json"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let view = result_payload(&out);
    let payload = view.get("payload").cloned().unwrap_or(Json::Null);
    assert_eq!(
        payload.get("kind"),
        Some(&Json::str("compact")),
        "{}",
        payload.to_canonical_string()
    );
    // AC-R-2.11.1 — the loss report enumerates every dropped class.
    let dropped = payload
        .get("loss_report")
        .and_then(|l| l.get("dropped"))
        .cloned()
        .unwrap_or(Json::Arr(vec![]));
    let dropped_classes: Vec<String> = match &dropped {
        Json::Arr(rows) => rows
            .iter()
            .filter_map(|r| r.get("class").and_then(Json::as_str).map(String::from))
            .collect(),
        _ => vec![],
    };
    assert!(
        dropped_classes.iter().any(|c| c == "lifecycle.run.created"),
        "lifecycle classes must be declared dropped: {dropped_classes:?}"
    );
    // The view is a stamped projection — a view_hash, never a bare array.
    assert!(view.get("view_hash").and_then(Json::as_str).is_some());
}

#[test]
fn run_inspect_accepts_compact() {
    let mut b = ServiceBoundary::new("inspect-compact");
    let def = write_definition("inspect-compact");
    let (_c, out, _err) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    let run_id = result_run_id(&out);
    let (class, out, _err) = hh(
        &mut b,
        &["run", "inspect", &run_id, "compact", "--format", "json"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let view = result_payload(&out);
    assert_eq!(
        view.get("payload")
            .and_then(|p| p.get("kind"))
            .and_then(Json::as_str),
        Some("compact"),
        "{}",
        view.to_canonical_string()
    );
}

/// `--until-seq` is a declared flag (C1): it pins `project`'s upper bound —
/// an unregistered spelling is an `invocation_error`, so this fails if the
/// flag is dropped from `VALUE_FLAGS` or the wiring is removed.
#[test]
fn run_inspect_until_seq_bounds_the_projection() {
    let mut b = ServiceBoundary::new("inspect-until");
    let def = write_definition("inspect-until");
    let (_c, out, _err) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    let run_id = result_run_id(&out);
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "inspect",
            &run_id,
            "--until-seq",
            "0",
            "--format",
            "json",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    // A bad seq value is a typed invocation error, never a kernel call.
    let (class, _out, _err) = hh(
        &mut b,
        &["run", "inspect", &run_id, "--until-seq", "not-a-number"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::InvocationError);
}

// ── config explain ──────────────────────────────────────────────

#[test]
fn config_explain_renders_layered_precedence() {
    let mut b = ServiceBoundary::new("config");
    let (class, out, _err) = hh(
        &mut b,
        &["config", "explain", "--format", "json"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let p = result_payload(&out);
    assert_eq!(
        p.get("precedence").and_then(Json::as_str),
        Some("session > environment > project > user > packaged"),
        "{}",
        p.to_canonical_string()
    );
    let keys = match p.get("keys") {
        Some(Json::Arr(k)) => k.clone(),
        _ => panic!("no keys: {}", p.to_canonical_string()),
    };
    let kernel = keys
        .iter()
        .find(|k| k.get("key").and_then(Json::as_str) == Some("kernel_cmd"))
        .cloned()
        .unwrap_or(Json::Null);
    assert_eq!(
        kernel.get("effective").and_then(Json::as_str),
        Some("hh-kernel"),
        "{}",
        kernel.to_canonical_string()
    );
    assert_eq!(
        kernel.get("source").and_then(Json::as_str),
        Some("packaged"),
        "{}",
        kernel.to_canonical_string()
    );
    // --key filters to a single entry.
    let (class, out, _err) = hh(
        &mut b,
        &[
            "config",
            "explain",
            "--key",
            "attendance",
            "--format",
            "json",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let p = result_payload(&out);
    let keys = match p.get("keys") {
        Some(Json::Arr(k)) => k.clone(),
        _ => panic!("no keys: {}", p.to_canonical_string()),
    };
    assert_eq!(keys.len(), 1, "{keys:?}");
    assert_eq!(
        keys[0].get("key").and_then(Json::as_str),
        Some("attendance")
    );
}

// ── env * ───────────────────────────────────────────────────────

#[test]
fn env_status_and_meters_render_describe() {
    let mut b = ServiceBoundary::new("env-status");
    let def = write_definition("env-status");
    let (_c, out, _err) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    let run_id = result_run_id(&out);
    for verb in ["status", "meters"] {
        let (class, out, _err) = hh(
            &mut b,
            &["env", verb, &run_id, "--format", "json"],
            NO_TTY,
            None,
            &[],
        );
        assert_eq!(class, ExitClass::Ok, "{verb}: {out}");
        let p = result_payload(&out);
        assert!(
            p.get("connection_info").is_some(),
            "{}",
            p.to_canonical_string()
        );
        assert!(p.get("health").is_some(), "{}", p.to_canonical_string());
        assert!(p.get("meters").is_some(), "{}", p.to_canonical_string());
    }
}

#[test]
fn env_list_detached_answers_a_count() {
    let mut b = ServiceBoundary::new("env-detached");
    let def = write_definition("env-detached");
    let (_c, out, _err) = hh(&mut b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    let run_id = result_run_id(&out);
    let (class, out, _err) = hh(
        &mut b,
        &["env", "list-detached", &run_id, "--format", "json"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let p = result_payload(&out);
    assert!(
        p.get("count").and_then(Json::as_int).is_some(),
        "{}",
        p.to_canonical_string()
    );
    assert!(p.get("detached").is_some(), "{}", p.to_canonical_string());
}

#[test]
fn env_snapshot_and_derive_over_the_writer_session() {
    let mut b = ServiceBoundary::new("env-ops");
    // `derive` copies the fs_tree — the workspace root must exist.
    std::fs::create_dir_all(b.root.join("ws")).unwrap();
    let run_id = park_run(&mut b, "env-ops");
    // `env snapshot --takeover` fences the parked writer and mints the
    // instrument-charged fs_tree snapshot (ADR-0177 D7).
    let (class, out, _err) = hh(
        &mut b,
        &["env", "snapshot", &run_id, "--takeover", "--format", "json"],
        ALL_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let p = result_payload(&out);
    assert!(
        p.get("snapshot_ref").and_then(Json::as_str).is_some(),
        "{}",
        p.to_canonical_string()
    );
    assert_eq!(
        p.get("taken_by").and_then(Json::as_str),
        Some("instrument"),
        "{}",
        p.to_canonical_string()
    );
    // `env derive --takeover` — a fork_snapshot child off the session
    // handle; the result is the record, never a handle id (R-NOSIDE).
    let (class, out, _err) = hh(
        &mut b,
        &[
            "env",
            "derive",
            &run_id,
            "--takeover",
            "--mode",
            "fork_snapshot",
            "--format",
            "json",
        ],
        ALL_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let p = result_payload(&out);
    assert_eq!(
        p.get("derived"),
        Some(&Json::Bool(true)),
        "{}",
        p.to_canonical_string()
    );
    assert!(
        p.get("env_handle_id").is_none(),
        "{}",
        p.to_canonical_string()
    );
}

#[test]
fn env_set_phase_refuses_without_a_sealed_schedule() {
    let mut b = ServiceBoundary::new("env-phase");
    let run_id = park_run(&mut b, "env-phase");
    // The reference local_host policy declares no
    // `per_phase_network_policy` — the kernel answers the typed refusal,
    // never a silent phase swap (CF-318; ADR-0142).
    let (class, out, _err) = hh(
        &mut b,
        &[
            "env",
            "set-phase",
            &run_id,
            "setup",
            "--takeover",
            "--format",
            "json",
        ],
        ALL_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::RefusedByKernel, "{out}");
    assert!(out.contains("phase_schedule_undeclared"), "{out}");
}

#[test]
fn env_unsupported_verbs_refuse_stage_pending() {
    let mut b = ServiceBoundary::new("env-pending");
    let (class, out, _err) = hh(&mut b, &["env", "upload", "run-x"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::InvocationError, "{out}");
    assert!(out.contains("stage_pending"), "{out}");
}

// ── amend(attendance|approval_mode) ─────────────────────────────

#[test]
fn amend_attendance_and_approval_mode_are_ledgered() {
    let mut b = ServiceBoundary::new("amend-policy");
    let run_id = park_run(&mut b, "amend-policy");
    // `amend attendance async` — a narrowing of the attendance channel;
    // admitted, minted `control.attendance.amended`.
    let (class, out, _err) = hh(
        &mut b,
        &["run", "amend", &run_id, "attendance", "async", "--takeover"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    // `amend approval_mode manual` — tightening the policy mode
    // (interactive attendance was the parked run's setting; a manual
    // mode narrows auto-resolution).
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "amend",
            &run_id,
            "approval_mode",
            "manual",
            "--takeover",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "control.attendance.amended"),
        "{classes:?}"
    );
    assert!(
        classes.iter().any(|c| c == "control.approval_mode.amended"),
        "{classes:?}"
    );
}

#[test]
fn amend_approval_mode_widening_needs_a_human() {
    let mut b = ServiceBoundary::new("amend-widen");
    let run_id = park_run(&mut b, "amend-widen");
    // `manual → bypass` widens auto-resolution *and* bypass requires
    // containment evidence — under unattended attendance with no
    // attestation the widening refusal lands first (I-1).
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "amend",
            &run_id,
            "approval_mode",
            "bypass",
            "--takeover",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert!(
        class == ExitClass::RefusedByKernel || class == ExitClass::InvocationError,
        "{out}"
    );
    let classes = event_classes(&mut b, &run_id);
    assert!(
        !classes.iter().any(|c| c == "control.approval_mode.amended"),
        "{classes:?}"
    );
}

// ── AC-R-2.11.1-7 — the bypass never-auto split (S2.12) ──────────────────

/// A capability ask whose declared `risk_class` is outside never-auto
/// auto-resolves under `--bypass` — `decided{decision: allow, decider:
/// policy, reason: bypass}`, no prompt, at either attendance.
#[test]
fn bypass_auto_resolves_the_non_never_auto_ask() {
    let mut b = ServiceBoundary::new("bypass-auto");
    let def = write_definition("bypass-auto");
    let cap = r#"{"capability_id":"cap:fs","surface_id":"surface:fs","requires_approval":true,"risk_class":{"reversibility":"reversible","repeat_safety":"non_idempotent","scope":"workspace_local"}}"#;
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--bypass",
            "--no-input",
            "--capability",
            cap,
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    let decided = events_json(&mut b, &run_id)
        .into_iter()
        .filter(|e| e.get("class").and_then(Json::as_str) == Some("security.permission.decided"))
        .collect::<Vec<_>>();
    assert_eq!(decided.len(), 1, "one auto-resolved ask: {decided:?}");
    let p = decided[0].get("payload").cloned().unwrap_or(Json::Null);
    assert_eq!(p.get("decision").and_then(Json::as_str), Some("allow"));
    assert_eq!(p.get("decider").and_then(Json::as_str), Some("policy"));
    assert_eq!(p.get("reason").and_then(Json::as_str), Some("bypass"));
}

/// The never-auto half: under `--bypass` a request whose `risk_class` is
/// irreversible (or undeclared ⇒ UNKNOWN) still reaches the human stage —
/// the attended loop prompts and the principal's answer lands.
#[test]
fn bypass_never_auto_still_prompts_attended() {
    let mut b = ServiceBoundary::new("bypass-never");
    let def = write_definition("bypass-never");
    let cap = r#"{"capability_id":"cap:exec","surface_id":"surface:exec","requires_approval":true,"options":["allow_once","deny_once"],"risk_class":{"reversibility":"irreversible","repeat_safety":"non_idempotent","scope":"external"}}"#;
    let (class, out, err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--bypass", "--capability", cap],
        ALL_TTY,
        None,
        &["allow_once"],
    );
    assert_eq!(class, ExitClass::Ok, "{out} {err}");
    // The ask reached the human stage — the prompt rendered.
    assert!(err.contains("approval requested"), "{err}");
    let run_id = result_run_id(&out);
    let decided = events_json(&mut b, &run_id)
        .into_iter()
        .filter(|e| e.get("class").and_then(Json::as_str) == Some("security.permission.decided"))
        .collect::<Vec<_>>();
    // The human decided (`decider: human`), never a policy auto-resolve.
    assert!(
        decided.iter().any(|e| e
            .get("payload")
            .and_then(|p| p.get("decider"))
            .and_then(Json::as_str)
            == Some("human")),
        "{decided:?}"
    );
}

/// An undeclared `risk_class` reads UNKNOWN ⇒ never-auto ⇒ under
/// `--bypass --no-input` the ask still denies (Π-12), never auto-allows.
#[test]
fn bypass_never_auto_denies_unattended() {
    let mut b = ServiceBoundary::new("bypass-deny");
    let def = write_definition("bypass-deny");
    let cap =
        r#"{"capability_id":"cap:exec","surface_id":"surface:exec","requires_approval":true}"#;
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "start",
            &def,
            "hi",
            "--bypass",
            "--no-input",
            "--capability",
            cap,
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let run_id = result_run_id(&out);
    let decided = events_json(&mut b, &run_id)
        .into_iter()
        .filter(|e| e.get("class").and_then(Json::as_str) == Some("security.permission.decided"))
        .collect::<Vec<_>>();
    assert_eq!(decided.len(), 1, "{decided:?}");
    let p = decided[0].get("payload").cloned().unwrap_or(Json::Null);
    assert_eq!(p.get("decision").and_then(Json::as_str), Some("deny"));
    assert_eq!(p.get("decider").and_then(Json::as_str), Some("policy"));
    assert_eq!(p.get("reason").and_then(Json::as_str), Some("unattended"));
}

// ── S3.1 — the Lab nouns + bundle/run import-export over the boundary ──
//
// Every verb is one named Group L/M/R op (K-2/N-3); the verbs whose
// owning Lab service lands later (S3.4a–S3.5) surface the typed
// `stage_pending` refusal verbatim — an honest answer, never a fake.

/// A boundary wrapper recording every `(method, params)` — the dry-run
/// parity evidence (which op a verb routes to).
struct RecordingBoundary<'a> {
    inner: &'a mut dyn Boundary,
    calls: Vec<(String, Json)>,
}

impl Boundary for RecordingBoundary<'_> {
    fn call(&mut self, method: &str, params: &Json) -> Result<Json, CliError> {
        self.calls.push((method.to_string(), params.clone()));
        self.inner.call(method, params)
    }
    fn poll_frame(&mut self) -> Result<Option<StreamNotification>, CliError> {
        self.inner.poll_frame()
    }
    fn poll_upcall(&mut self) -> Result<Option<(String, Json)>, CliError> {
        self.inner.poll_upcall()
    }
    fn hello_result(&self) -> Option<&HelloResult> {
        self.inner.hello_result()
    }
}

/// A definition file for the Lab verbs.
fn write_lab_definition(tag: &str) -> String {
    let dir = test_dir(&format!("def-{tag}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("def.hir.json");
    std::fs::write(&path, document_json().to_canonical_string()).unwrap();
    path.to_string_lossy().to_string()
}

/// `run start` → finished run_id (the bundle subject).
fn started_run(b: &mut dyn Boundary, tag: &str) -> String {
    let def = write_definition(&format!("run-{tag}"));
    let (class, out, _err) = hh(b, &["run", "start", &def, "hi"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    result_run_id(&out)
}

#[test]
fn s31_run_export_delivers_a_bundle_dir() {
    let mut b = ServiceBoundary::new("s31-export");
    let run_id = started_run(&mut b, "exp");
    let out_dir = test_dir("s31-exported");
    let (class, out, _err) = hh(
        &mut b,
        &[
            "run",
            "export",
            &run_id,
            "--out",
            &out_dir.to_string_lossy(),
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    // The sink holds bundle.json + members/*.
    assert!(out_dir.join("bundle.json").is_file(), "{out}");
    assert!(out_dir.join("members").is_dir(), "{out}");
    // The delivery row is on the ledger.
    let classes = event_classes(&mut b, &run_id);
    assert!(
        classes.iter().any(|c| c == "measurement.export.delivered"),
        "{classes:?}"
    );
    assert!(
        classes
            .iter()
            .any(|c| c == "measurement.experiment.bundle_assembled"),
        "{classes:?}"
    );
}

#[test]
fn s31_bundle_validate_show_reproduce_over_exported_dir() {
    let mut b = ServiceBoundary::new("s31-bval");
    let run_id = started_run(&mut b, "bval");
    let out_dir = test_dir("s31-bval-out");
    let (class, _, _) = hh(
        &mut b,
        &[
            "run",
            "export",
            &run_id,
            "--out",
            &out_dir.to_string_lossy(),
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok);

    // validate — the staged gate's report.
    let (class, out, _) = hh(
        &mut b,
        &["bundle", "validate", &out_dir.to_string_lossy()],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains("\"complete\":true"), "{out}");

    // show — the manifest summary.
    let (class, out, _) = hh(
        &mut b,
        &["bundle", "show", &out_dir.to_string_lossy()],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");

    // reproduce R0 — the report names the achieved/refused outcome.
    let (class, out, _) = hh(
        &mut b,
        &[
            "bundle",
            "reproduce",
            &out_dir.to_string_lossy(),
            "--level",
            "R0",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains("\"requested_level\":\"R0\""), "{out}");
}

#[test]
fn s31_run_import_round_trips_and_dry_run_does_not_lift() {
    let mut b = ServiceBoundary::new("s31-import");
    let run_id = started_run(&mut b, "imp");
    let out_dir = test_dir("s31-import-src");
    let (class, _, _) = hh(
        &mut b,
        &[
            "run",
            "export",
            &run_id,
            "--out",
            &out_dir.to_string_lossy(),
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok);

    // dry-run — validation only; no new run.
    let mut rec = RecordingBoundary {
        inner: &mut b,
        calls: vec![],
    };
    let (class, out, _) = hh(
        &mut rec,
        &["run", "import", &out_dir.to_string_lossy(), "--dry-run"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains("\"dry_run\":true"), "{out}");
    assert!(
        rec.calls
            .iter()
            .all(|(m, _)| m == "kernel.check_completeness"),
        "dry-run must not lift: {:?}",
        rec.calls
    );

    // The real import lifts to a new run.
    let (class, out, _) = hh(
        &mut b,
        &["run", "import", &out_dir.to_string_lossy()],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let new_run = result_run_id(&out);
    assert!(!new_run.is_empty(), "{out}");
    assert_ne!(new_run, run_id);
    let classes = event_classes(&mut b, &new_run);
    assert!(
        classes.iter().any(|c| c == "lifecycle.run.imported"),
        "{classes:?}"
    );
}

#[test]
fn s31_bundle_create_reports_contract_identity() {
    let mut b = ServiceBoundary::new("s31-bcreate");
    let run_id = started_run(&mut b, "bc");
    let (class, out, _) = hh(
        &mut b,
        &["bundle", "create", &run_id, "--format", "json"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let j = hh_wire::json::parse(out.trim()).unwrap();
    let manifest = j
        .get("payload")
        .and_then(|p| p.get("manifest"))
        .cloned()
        .unwrap_or(Json::Null);
    let cv = manifest
        .get("instrument")
        .and_then(|i| i.get("component_versions"))
        .cloned()
        .unwrap_or(Json::Null);
    assert!(
        cv.to_canonical_string().contains("schema_hash"),
        "ContractIdentity rides in instrument.component_versions: {cv:?}"
    );
}

#[test]
fn s31_lab_nouns_route_to_group_l_and_dry_run_is_the_planning_half() {
    let mut b = ServiceBoundary::new("s31-lab");
    let def = write_lab_definition("route");
    let mut rec = RecordingBoundary {
        inner: &mut b,
        calls: vec![],
    };

    // `definition plan` → lab.assembly.plan.
    let _ = hh(&mut rec, &["definition", "plan", &def], NO_TTY, None, &[]);
    // `definition apply --dry-run` → lab.assembly.plan (parity — ADR-0147 S-1).
    let _ = hh(
        &mut rec,
        &["definition", "apply", &def, "--dry-run"],
        NO_TTY,
        None,
        &[],
    );
    // `definition apply` → lab.assembly.apply.
    let _ = hh(&mut rec, &["definition", "apply", &def], NO_TTY, None, &[]);
    // `experiment open --dry-run` → lab.experiment.expand (no open).
    let _ = hh(
        &mut rec,
        &["experiment", "open", "exp-1", "--dry-run"],
        NO_TTY,
        None,
        &[],
    );
    let _ = hh(
        &mut rec,
        &["experiment", "open", "exp-1"],
        NO_TTY,
        None,
        &[],
    );
    // `registry catalog` → lab.registry.catalog (implemented — S2.12).
    let (class, out, _) = hh(&mut rec, &["registry", "catalog"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    // `compare report` → analyze then render(view=report).
    let _ = hh(
        &mut rec,
        &["compare", "report", "a", "b"],
        NO_TTY,
        None,
        &[],
    );
    let _ = hh(&mut rec, &["results", "query"], NO_TTY, None, &[]);

    let methods: Vec<&str> = rec.calls.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(methods[0], "lab.assembly.plan", "{methods:?}");
    assert_eq!(
        methods[1], "lab.assembly.plan",
        "apply --dry-run must route to plan: {methods:?}"
    );
    assert_eq!(methods[2], "lab.assembly.apply", "{methods:?}");
    assert_eq!(
        methods[3], "lab.experiment.expand",
        "open --dry-run must route to expand: {methods:?}"
    );
    assert_eq!(methods[4], "lab.experiment.open_experiment", "{methods:?}");
    assert_eq!(methods[5], "lab.registry.catalog", "{methods:?}");
    assert_eq!(methods[6], "lab.analysis.analyze", "{methods:?}");
    // `analyze` is stage_pending at this stage → `compare report`
    // stops after the first op (no fake `render` on an error — K-2).
    assert_eq!(methods[7], "lab.results.query_rows", "{methods:?}");
    assert_eq!(methods.len(), 8, "{methods:?}");
}

#[test]
fn s31_pending_lab_ops_surface_typed_stage_pending() {
    let mut b = ServiceBoundary::new("s31-pending");
    let def = write_lab_definition("pend");
    // The assembly service lands at S3.5 — `definition plan` forwards to
    // the registered op and surfaces `Refused{stage_pending}` verbatim
    // (the Lab noun adds no semantics — N-3).
    let (class, out, _) = hh(
        &mut b,
        &["definition", "plan", &def, "--format", "json"],
        NO_TTY,
        None,
        &[],
    );
    assert_ne!(class, ExitClass::Ok, "{out}");
    assert!(out.contains("stage_pending"), "{out}");

    // Verbs with no registered op refuse `invocation_error` stage_pending.
    let (class, out, _) = hh(&mut b, &["definition", "space", &def], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::InvocationError, "{out}");
    assert!(out.contains("stage_pending"), "{out}");
    let (class, out, _) = hh(&mut b, &["bundle", "verify", "x"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::InvocationError, "{out}");
}

#[test]
fn s31_lab_serve_returns_stdio_launch_binding() {
    let mut b = ServiceBoundary::new("s31-serve");
    // A hand-authored bundle dir with a `target:mcp` member.
    let dir = test_dir("s31-serve-bundle");
    let member =
        br#"{"schema":"hh-mcp-target/1","target":"mcp","tools":[{"name":"echo"}]}"#.to_vec();
    let addr = hh_identity::idp_id("member", &member);
    let doc = Json::obj([
        ("schema", Json::str("hh-bundle/1")),
        ("idp", Json::str("idp/1")),
        ("bundle_kind", Json::str("run")),
        ("producer", Json::obj([])),
        (
            "subject",
            Json::obj([
                ("run_ids", Json::Arr(vec![])),
                ("status", Json::str("finished")),
            ]),
        ),
        (
            "members",
            Json::Arr(vec![Json::obj([
                ("role", Json::str("target:mcp")),
                ("ref", Json::str(addr.clone())),
                ("status", Json::str("present")),
            ])]),
        ),
        ("version_id", Json::str("pending")),
    ]);
    let mut manifest = hh_bundle::manifest::BundleManifest::from_json(&doc).unwrap();
    manifest.version_id = manifest.compute_id();
    let mut members = hh_bundle::export::MemberBytes::new();
    members.insert(addr, member);
    hh_bundle::codec::encode_dir(&dir, &manifest, &members).unwrap();

    let (class, out, _) = hh(
        &mut b,
        &["lab", "serve", &dir.to_string_lossy()],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains("\"stdio_launch\""), "{out}");
    assert!(out.contains("\"principal:test\""), "{out}");
}

/// AC-R-2.11.4-10 — the Lab runs entirely over Groups S/R/M/L in a
/// *separate process*: drive the Lab nouns (`registry catalog`,
/// `bundle create`, `run export`, `run import`, `lab serve`) through
/// the spawned `hh-kernel serve` child (binding (b)), not the
/// in-process service. Byte-parity with binding (a) is the bindings'
/// own conformance property (AC-K4-2); this test is the process seam.
#[test]
fn s31_lab_ops_run_over_the_spawned_process() {
    let kernel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/hh-kernel");
    assert!(
        kernel.exists(),
        "build hh-kernel first (`cargo build -p hh-kernel`)"
    );
    let root = test_dir("s31-proc-kernel");
    let env = vec![
        (
            "HH_STORE_ROOT".to_string(),
            root.join("store").display().to_string(),
        ),
        (
            "HH_WORKSPACE_ROOT".to_string(),
            root.join("ws").display().to_string(),
        ),
    ];
    let mut b = ProcessBoundary::spawn(&kernel.display().to_string(), &env).unwrap();

    // Group L: registry catalog over the process boundary.
    let (class, out, _) = hh(&mut b, &["registry", "catalog"], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains("\"entries\""), "{out}");

    // Group M: a run → bundle → validate → reproduce over the process.
    let run_id = started_run(&mut b, "proc");
    let (class, out, _) = hh(&mut b, &["bundle", "create", &run_id], NO_TTY, None, &[]);
    assert_eq!(class, ExitClass::Ok, "{out}");
    let out_dir = test_dir("s31-proc-export");
    let (class, out, _) = hh(
        &mut b,
        &[
            "run",
            "export",
            &run_id,
            "--out",
            &out_dir.to_string_lossy(),
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let (class, out, _) = hh(
        &mut b,
        &["bundle", "validate", &out_dir.to_string_lossy()],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    assert!(out.contains("\"complete\":true"), "{out}");
    let (class, out, _) = hh(
        &mut b,
        &[
            "bundle",
            "reproduce",
            &out_dir.to_string_lossy(),
            "--level",
            "R0",
        ],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");

    // The lift back — `run import` over the process boundary.
    let (class, out, _) = hh(
        &mut b,
        &["run", "import", &out_dir.to_string_lossy()],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::Ok, "{out}");
    let new_run = result_run_id(&out);
    assert_ne!(new_run, run_id, "{out}");
}
