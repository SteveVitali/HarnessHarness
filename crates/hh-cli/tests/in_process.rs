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
fn bypass_without_containment_is_invocation_error() {
    let mut b = ServiceBoundary::new("bypass");
    let def = write_definition("bypass");
    let (class, out, _err) = hh(
        &mut b,
        &["run", "start", &def, "hi", "--bypass"],
        NO_TTY,
        None,
        &[],
    );
    assert_eq!(class, ExitClass::InvocationError);
    assert!(out.contains("bypass_without_containment"), "{out}");
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
