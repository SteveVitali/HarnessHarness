//! `CaptureItem` + `EffectCaptureManifest` — the closed capture-item sum sealed
//! into a content-addressed manifest with explicit completeness (§5d.5 §2
//! capture row; ADR-0101 D3/D4). The manifest is the capture's identity —
//! `idp/1` under `capture_manifest` over the non-ephemeral items; ephemeral
//! chunks (`output_chunk`, `progress`) never enter it (they stream and are
//! dropped).
//!
//! Redaction is on the **capture path**, not the executor — the kernel masks
//! every captured payload through `hh-secrets` before it enters the manifest
//! or the raw-output blob (ADR-0101 D5; a hostile executor's output is masked
//! before any visibility). See [`crate::dispatch::Dispatcher`].

use std::collections::BTreeSet;

use hh_hir::kinds::EffectDomain;
use hh_identity::idp::idp_id;
use hh_wire::json::Json;

use crate::errors::EnvError;

/// `CaptureSource` — who produced the item (the attribution axis — the kernel
/// distinguishes what the executor *reported* from what it *observed*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CaptureSource {
    /// The executor reported it (a claim — trusted only as far as the
    /// attribution token + the mediator's corroboration carry it).
    ExecutorReported,
    /// The out-of-process helper observed it.
    HelperObserved,
    /// The mediator observed it (egress, containment violation).
    MediatorObserved,
    /// The kernel derived it (terminal status, fs diff, unattributed marker).
    KernelDerived,
}

impl CaptureSource {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CaptureSource::ExecutorReported => "executor_reported",
            CaptureSource::HelperObserved => "helper_observed",
            CaptureSource::MediatorObserved => "mediator_observed",
            CaptureSource::KernelDerived => "kernel_derived",
        }
    }
}

/// `ProcessTransition` — a `process` capture kind's sub-tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessTransition {
    /// A child spawned.
    Spawned,
    /// A child exited.
    Exited,
    /// A child was signalled.
    Signalled,
    /// A child detached (the detached-child effect's parentage).
    Detached,
}

impl ProcessTransition {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProcessTransition::Spawned => "spawned",
            ProcessTransition::Exited => "exited",
            ProcessTransition::Signalled => "signalled",
            ProcessTransition::Detached => "detached",
        }
    }
}

/// `WorldScope` — the `world_changed` kind's scope tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldScope {
    /// Workspace-local.
    Workspace,
    /// External (an out-of-environment mutation the mediators observed).
    External,
}

impl WorldScope {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            WorldScope::Workspace => "workspace",
            WorldScope::External => "external",
        }
    }
}

/// `CaptureKind` — the closed item sum (ADR-0101 D3). The two *ephemeral*
/// kinds stream and never enter the manifest.
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureKind {
    /// `output_chunk{stream, data}` — **ephemeral** (streams; the manifest
    /// holds `raw_output_ref` instead).
    OutputChunk {
        /// `stdout|stderr`.
        stream: String,
        /// The chunk (masked before capture).
        data: String,
    },
    /// `progress{note}` — **ephemeral**.
    Progress {
        /// The progress note.
        note: String,
    },
    /// `fs_change{path, change, before_ref, after_ref, inside_writable_roots}`.
    FsChange {
        /// The canonical path.
        path_canonical: String,
        /// `added|modified|removed`.
        change: String,
        /// The before content id.
        before_ref: Option<String>,
        /// The after content id.
        after_ref: Option<String>,
        /// Whether the path is inside a writable root (the kernel's check —
        /// a `false` here is a containment violation, recorded).
        inside_writable_roots: bool,
    },
    /// `process{transition}` — spawn/exit/signal/detach.
    Process {
        /// The transition.
        transition: ProcessTransition,
        /// The child pid / process token.
        process_ref: String,
    },
    /// `egress{decision_ref}` — a `net_egress` the mediator decided.
    Egress {
        /// The `security.permission.decided` ref the egress ran under.
        decision_ref: String,
    },
    /// `credential_use{used_ref}` — a `security.credential.used` row.
    CredentialUse {
        /// The credential-use event ref.
        used_ref: String,
    },
    /// `containment_violation{violated_ref}` — a `security.containment.
    /// violated` row (a denied attempt, observed).
    ContainmentViolation {
        /// The violation event ref.
        violated_ref: String,
    },
    /// `resource_usage{dimension, amount, measured_at}`.
    ResourceUsage {
        /// The dimension (`cpu_ms`, `mem_bytes`, …).
        dimension: String,
        /// The amount.
        amount: u64,
        /// When measured (mono ms).
        measured_at: u64,
    },
    /// `world_changed{scope, evidence_ref}` — an out-of-band mutation.
    WorldChanged {
        /// The scope.
        scope: WorldScope,
        /// The evidence ref.
        evidence_ref: String,
    },
    /// `structured_result{schema_ref, value_ref}` — the tool's typed result.
    StructuredResult {
        /// The output schema ref.
        schema_ref: String,
        /// The result value's content ref.
        value_ref: String,
    },
    /// `terminal{status, exit_status?, executor_outcome_hint, retryable_hint,
    /// detail_ref?}` — the executor's terminal report (the hint is
    /// *lowered-only* — never raises `retryable`).
    Terminal {
        /// `ok|error`.
        status: String,
        /// The process exit status, when a process ran.
        exit_status: Option<i64>,
        /// The executor's outcome claim (`applied|not_applied|unknown`) —
        /// advisory; the kernel's own outcome rules decide.
        executor_outcome_hint: String,
        /// The executor's retryable hint — may only lower, never raise.
        retryable_hint: Option<bool>,
        /// A detail ref (masked content).
        detail_ref: Option<String>,
    },
}

impl CaptureKind {
    /// The kind tag (`capture.kind` member).
    pub fn as_str(&self) -> &'static str {
        match self {
            CaptureKind::OutputChunk { .. } => "output_chunk",
            CaptureKind::Progress { .. } => "progress",
            CaptureKind::FsChange { .. } => "fs_change",
            CaptureKind::Process { .. } => "process",
            CaptureKind::Egress { .. } => "egress",
            CaptureKind::CredentialUse { .. } => "credential_use",
            CaptureKind::ContainmentViolation { .. } => "containment_violation",
            CaptureKind::ResourceUsage { .. } => "resource_usage",
            CaptureKind::WorldChanged { .. } => "world_changed",
            CaptureKind::StructuredResult { .. } => "structured_result",
            CaptureKind::Terminal { .. } => "terminal",
        }
    }

    /// Whether the kind is ephemeral (streams; never in the manifest).
    pub fn is_ephemeral(&self) -> bool {
        matches!(
            self,
            CaptureKind::OutputChunk { .. } | CaptureKind::Progress { .. }
        )
    }

    /// The `payload` member (the kind's own members, minus the envelope).
    pub fn payload_json(&self) -> Json {
        match self {
            CaptureKind::OutputChunk { stream, data } => Json::obj([
                ("stream", Json::str(stream.clone())),
                ("data", Json::str(data.clone())),
            ]),
            CaptureKind::Progress { note } => Json::obj([("note", Json::str(note.clone()))]),
            CaptureKind::FsChange {
                path_canonical,
                change,
                before_ref,
                after_ref,
                inside_writable_roots,
            } => Json::obj([
                ("path_canonical", Json::str(path_canonical.clone())),
                ("change", Json::str(change.clone())),
                (
                    "before_ref",
                    before_ref
                        .as_ref()
                        .map_or(Json::Null, |r| Json::str(r.clone())),
                ),
                (
                    "after_ref",
                    after_ref
                        .as_ref()
                        .map_or(Json::Null, |r| Json::str(r.clone())),
                ),
                ("inside_writable_roots", Json::Bool(*inside_writable_roots)),
            ]),
            CaptureKind::Process {
                transition,
                process_ref,
            } => Json::obj([
                ("transition", Json::str(transition.as_str())),
                ("process_ref", Json::str(process_ref.clone())),
            ]),
            CaptureKind::Egress { decision_ref } => {
                Json::obj([("decision_ref", Json::str(decision_ref.clone()))])
            }
            CaptureKind::CredentialUse { used_ref } => {
                Json::obj([("used_ref", Json::str(used_ref.clone()))])
            }
            CaptureKind::ContainmentViolation { violated_ref } => {
                Json::obj([("violated_ref", Json::str(violated_ref.clone()))])
            }
            CaptureKind::ResourceUsage {
                dimension,
                amount,
                measured_at,
            } => Json::obj([
                ("dimension", Json::str(dimension.clone())),
                ("amount", Json::Int(*amount as i64)),
                ("measured_at", Json::Int(*measured_at as i64)),
            ]),
            CaptureKind::WorldChanged {
                scope,
                evidence_ref,
            } => Json::obj([
                ("scope", Json::str(scope.as_str())),
                ("evidence_ref", Json::str(evidence_ref.clone())),
            ]),
            CaptureKind::StructuredResult {
                schema_ref,
                value_ref,
            } => Json::obj([
                ("schema_ref", Json::str(schema_ref.clone())),
                ("value_ref", Json::str(value_ref.clone())),
            ]),
            CaptureKind::Terminal {
                status,
                exit_status,
                executor_outcome_hint,
                retryable_hint,
                detail_ref,
            } => Json::obj([
                ("status", Json::str(status.clone())),
                ("exit_status", exit_status.map_or(Json::Null, Json::Int)),
                (
                    "executor_outcome_hint",
                    Json::str(executor_outcome_hint.clone()),
                ),
                (
                    "retryable_hint",
                    retryable_hint.map_or(Json::Null, Json::Bool),
                ),
                (
                    "detail_ref",
                    detail_ref
                        .as_ref()
                        .map_or(Json::Null, |r| Json::str(r.clone())),
                ),
            ]),
        }
    }
}

/// `CaptureItem` — one captured fact (§5d.5 §2 item row). The kernel stamps
/// the five identity ids + `seq`/`ts_mono`; the executor/helper supplies only
/// `source`/`kind` (+ the masked payload).
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureItem {
    /// The run.
    pub run_id: String,
    /// The turn.
    pub turn_id: String,
    /// The model call.
    pub model_call_id: String,
    /// The tool call.
    pub tool_call_id: String,
    /// The effect.
    pub effect_id: String,
    /// The attempt.
    pub attempt_no: u64,
    /// The execution (the executor's `execution_id` — the dispatch's).
    pub execution_id: String,
    /// The item's sequence within the execution.
    pub seq: u64,
    /// The monotonic timestamp (ms — a helper/kernel clock, never wall).
    pub ts_mono: u64,
    /// Who produced it.
    pub source: CaptureSource,
    /// What it is.
    pub kind: CaptureKind,
    /// The item's provenance (`kernel_derived` items carry the kernel's
    /// record; `helper_observed` the helper's claim — recorded, never trusted
    /// as authority).
    pub provenance: String,
}

impl CaptureItem {
    /// The canonical member form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("run_id", Json::str(self.run_id.clone())),
            ("turn_id", Json::str(self.turn_id.clone())),
            ("model_call_id", Json::str(self.model_call_id.clone())),
            ("tool_call_id", Json::str(self.tool_call_id.clone())),
            ("effect_id", Json::str(self.effect_id.clone())),
            ("attempt_no", Json::Int(self.attempt_no as i64)),
            ("execution_id", Json::str(self.execution_id.clone())),
            ("seq", Json::Int(self.seq as i64)),
            ("ts_mono", Json::Int(self.ts_mono as i64)),
            ("source", Json::str(self.source.as_str())),
            ("kind", Json::str(self.kind.as_str())),
            ("payload", self.kind.payload_json()),
            ("provenance", Json::str(self.provenance.clone())),
        ])
    }
}

/// `OutputPolicy` — the declared output contract (`retain_bytes_cap`,
/// `model_view`, `offload_above`, `max_deltas`). The manifest's `truncation`
/// names the policy *ref* that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputPolicy {
    /// The retained-bytes cap (default 1 MiB — `output_cap_exceeded` above).
    pub retain_bytes_cap: u64,
    /// The model-view mode (`full|tail|summary`).
    pub model_view: String,
    /// Offload to a blob above this size.
    pub offload_above: u64,
    /// Max retained deltas.
    pub max_deltas: u64,
}

impl Default for OutputPolicy {
    fn default() -> Self {
        OutputPolicy {
            retain_bytes_cap: 1 << 20,
            model_view: "full".to_string(),
            offload_above: 1 << 20,
            max_deltas: 64,
        }
    }
}

impl OutputPolicy {
    /// The canonical member form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("retain_bytes_cap", Json::Int(self.retain_bytes_cap as i64)),
            ("model_view", Json::str(self.model_view.clone())),
            ("offload_above", Json::Int(self.offload_above as i64)),
            ("max_deltas", Json::Int(self.max_deltas as i64)),
        ])
    }

    /// The `policy_ref` — `idp/1` under `output_policy` over the canonical
    /// form.
    pub fn policy_ref(&self) -> String {
        idp_id(
            "output_policy",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }
}

/// `Truncation` — the manifest's record that output was cut (`omitted_bytes`,
/// `original_size`, the `policy_ref` that produced it).
#[derive(Debug, Clone, PartialEq)]
pub struct Truncation {
    /// Bytes dropped.
    pub omitted_bytes: u64,
    /// The pre-truncation size.
    pub original_size: u64,
    /// The `OutputPolicy` ref that produced the truncation.
    pub policy_ref: String,
}

/// `IncompleteReason` — why a manifest is `partial` (never silent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncompleteReason {
    /// Output was truncated at the cap.
    Truncated,
    /// A channel the domain implies had no observer.
    UnobservedChannel,
    /// The capture drain timed out.
    DrainTimeout,
    /// The executor never reported a terminal.
    ExecutorUnreported,
}

impl IncompleteReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            IncompleteReason::Truncated => "truncated",
            IncompleteReason::UnobservedChannel => "unobserved_channel",
            IncompleteReason::DrainTimeout => "drain_timeout",
            IncompleteReason::ExecutorUnreported => "executor_unreported",
        }
    }
}

/// `Completeness` — `complete | partial{reasons} | unknown` (T-LCD-07 — never
/// coerced).
#[derive(Debug, Clone, PartialEq)]
pub enum Completeness {
    /// Every domain-implied capture kind has an observer and no residual
    /// channel is open.
    Complete,
    /// Some implied kind is unobserved or a channel is open.
    Partial(BTreeSet<IncompleteReason>),
    /// The manifest's completeness cannot be determined (executor
    /// unreported + no terminal).
    Unknown,
}

impl Completeness {
    /// The canonical member form.
    pub fn to_json(&self) -> Json {
        match self {
            Completeness::Complete => Json::str("complete"),
            Completeness::Unknown => Json::str("unknown"),
            Completeness::Partial(reasons) => Json::obj([
                ("status", Json::str("partial")),
                (
                    "reasons",
                    Json::Arr(reasons.iter().map(|r| Json::str(r.as_str())).collect()),
                ),
            ]),
        }
    }
}

/// `EffectCaptureManifest` — the sealed, content-addressed capture record
/// (§5d.5 §2 manifest row). `content_id` is the capture's identity — over the
/// *non-ephemeral* items + the envelope ids; ephemeral chunks are absent by
/// construction (seal refuses one).
#[derive(Debug, Clone, PartialEq)]
pub struct EffectCaptureManifest {
    /// The effect.
    pub effect_id: String,
    /// The attempt.
    pub attempt_no: u64,
    /// The execution.
    pub execution_id: String,
    /// The non-ephemeral items (seal refuses an ephemeral member).
    pub items: Vec<CaptureItem>,
    /// The raw (pre-model-view, masked) output blob's content address.
    pub raw_output_ref: Option<String>,
    /// The truncation record, when output was cut.
    pub truncation: Option<Truncation>,
    /// The completeness verdict.
    pub completeness: Completeness,
    /// The capture kinds with a wired observer (`observers_present` — the
    /// completeness rule's operand).
    pub observers_present: BTreeSet<String>,
    /// Declared residual channels still open at seal.
    pub residual_channels_ref: Option<String>,
}

/// The capture kinds an `EffectDomain` *implies* must have an observer —
/// `complete` requires each to be wired (the completeness rule's table).
pub fn domain_implied_kinds(domain: EffectDomain) -> &'static [&'static str] {
    match domain {
        EffectDomain::Exec | EffectDomain::SpawnProcess => {
            &["terminal", "process", "resource_usage"]
        }
        EffectDomain::FsWrite => &["terminal", "fs_change"],
        EffectDomain::FsRead => &["terminal"],
        EffectDomain::NetEgress => &["terminal", "egress"],
        EffectDomain::SecretAccess => &["terminal", "credential_use"],
        _ => &["terminal"],
    }
}

impl EffectCaptureManifest {
    /// Compute the completeness verdict — `complete` only when every
    /// domain-implied kind is in `observers_present`, no residual channel is
    /// open, and nothing was truncated; else `partial` with the reasons.
    pub fn compute_completeness(
        domain: EffectDomain,
        observers_present: &BTreeSet<String>,
        residual_open: bool,
        truncated: bool,
        executor_reported_terminal: bool,
    ) -> Completeness {
        if !executor_reported_terminal {
            return Completeness::Unknown;
        }
        let mut reasons = BTreeSet::new();
        for k in domain_implied_kinds(domain) {
            if !observers_present.contains(*k) {
                reasons.insert(IncompleteReason::UnobservedChannel);
            }
        }
        if residual_open {
            reasons.insert(IncompleteReason::UnobservedChannel);
        }
        if truncated {
            reasons.insert(IncompleteReason::Truncated);
        }
        if reasons.is_empty() {
            Completeness::Complete
        } else {
            Completeness::Partial(reasons)
        }
    }

    /// `seal()` — validate the manifest: no ephemeral item may be present
    /// (the capture path streams those; they never reach the manifest). The
    /// returned manifest is the closed form.
    pub fn seal(self) -> Result<Self, EnvError> {
        for it in &self.items {
            if it.kind.is_ephemeral() {
                return Err(EnvError::Unsupported {
                    capability: "manifest.seal",
                    detail: format!(
                        "ephemeral kind {} cannot enter a manifest",
                        it.kind.as_str()
                    ),
                });
            }
        }
        Ok(self)
    }

    /// The canonical member form (the `content_id` payload).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("effect_id", Json::str(self.effect_id.clone())),
            ("attempt_no", Json::Int(self.attempt_no as i64)),
            ("execution_id", Json::str(self.execution_id.clone())),
            (
                "items",
                Json::Arr(self.items.iter().map(CaptureItem::to_json).collect()),
            ),
            (
                "raw_output_ref",
                self.raw_output_ref
                    .as_ref()
                    .map_or(Json::Null, |r| Json::str(r.clone())),
            ),
            (
                "truncation",
                self.truncation.as_ref().map_or(Json::Null, |t| {
                    Json::obj([
                        ("omitted_bytes", Json::Int(t.omitted_bytes as i64)),
                        ("original_size", Json::Int(t.original_size as i64)),
                        ("policy_ref", Json::str(t.policy_ref.clone())),
                    ])
                }),
            ),
            ("completeness", self.completeness.to_json()),
            (
                "observers_present",
                Json::Arr(
                    self.observers_present
                        .iter()
                        .map(|o| Json::str(o.clone()))
                        .collect(),
                ),
            ),
            (
                "residual_channels_ref",
                self.residual_channels_ref
                    .as_ref()
                    .map_or(Json::Null, |r| Json::str(r.clone())),
            ),
        ])
    }

    /// The *content* projection the manifest's identity hashes — the capture's
    /// payload sans the transport coordinates (`run_id`, the scope chain,
    /// `effect_id`, `attempt_no`, `execution_id`, `seq`, `ts_mono`). This is
    /// what makes `capture_manifest_ref` chunking-invariant (AC-R-2.5.5-5):
    /// two runs of the same effect with different chunk boundaries produce
    /// the same id (ephemeral chunks never appear here either way — only the
    /// `raw_output_ref` blob, which is the same bytes).
    fn content_json(&self) -> Json {
        let item_json = |i: &CaptureItem| {
            Json::obj([
                ("source", Json::str(i.source.as_str())),
                ("provenance", Json::str(i.provenance.clone())),
                ("kind", Json::str(i.kind.as_str())),
                ("payload", i.kind.payload_json()),
            ])
        };
        Json::obj([
            (
                "items",
                Json::Arr(self.items.iter().map(item_json).collect()),
            ),
            (
                "raw_output_ref",
                self.raw_output_ref
                    .as_ref()
                    .map_or(Json::Null, |r| Json::str(r.clone())),
            ),
            (
                "truncation",
                self.truncation.as_ref().map_or(Json::Null, |t| {
                    Json::obj([
                        ("omitted_bytes", Json::Int(t.omitted_bytes as i64)),
                        ("original_size", Json::Int(t.original_size as i64)),
                        ("policy_ref", Json::str(t.policy_ref.clone())),
                    ])
                }),
            ),
            ("completeness", self.completeness.to_json()),
            (
                "observers_present",
                Json::Arr(
                    self.observers_present
                        .iter()
                        .map(|o| Json::str(o.clone()))
                        .collect(),
                ),
            ),
            (
                "residual_channels_ref",
                self.residual_channels_ref
                    .as_ref()
                    .map_or(Json::Null, |r| Json::str(r.clone())),
            ),
        ])
    }

    /// `content_id` — `idp/1` under `capture_manifest` over the content
    /// projection (the capture's identity — the effect/execution coordinates
    /// are *members*, not identity: identical captures share the ref).
    pub fn content_id(&self) -> String {
        idp_id(
            "capture_manifest",
            self.content_json().to_canonical_string().as_bytes(),
        )
    }
}
