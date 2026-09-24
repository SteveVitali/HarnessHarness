//! `ExecutorDeclaration` + `ToolExecutor` — the operation-shaped executor
//! component (§5d.5 §2 executor row; ADR-0100 I-3: an executor *reports*,
//! never decides — no authorization or lifecycle outcome originates from it).
//!
//! `bind(declaration, capability_version)` refuses when the executor's
//! declared `dedup_support`/`isolation_support` cannot cover the capability's
//! `execution_requirement` — an insufficient declaration is a bind-time
//! refusal, never a silent degrade.

use std::collections::BTreeSet;

use hh_containment::policy::IsolationClass;
use hh_hir::kinds::EffectClass;
use hh_wire::json::Json;

use crate::deadline::DeadlineLadder;
use crate::errors::EnvError;
use crate::observe::ErrorClass;

/// `DedupSupport` — whether the executor holds an idempotency/dedup store.
/// `bind` refuses `none` for a capability whose `execution_requirement`
/// declares idempotency (AC-R-2.5.5-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DedupSupport {
    /// No dedup store — the kernel's dedup is the only guard.
    None,
    /// A best-effort in-memory store.
    BestEffort,
    /// A durable executor-side dedup store (Stage-2+).
    Durable,
}

impl DedupSupport {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DedupSupport::None => "none",
            DedupSupport::BestEffort => "best_effort",
            DedupSupport::Durable => "durable",
        }
    }
}

/// `ProbeSupport` — whether the executor can answer a `probe` (a lapsed-window
/// effect is probed, never redispatched — AC-R-2.5.5-9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProbeSupport {
    /// No probe — a lapsed window is `unknown` forever (never redispatched).
    None,
    /// The executor can re-check whether the effect applied.
    Check,
}

impl ProbeSupport {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeSupport::None => "none",
            ProbeSupport::Check => "check",
        }
    }
}

/// `IsolationRequirement` — the isolation floor a capability's
/// `execution_requirement` declares (the executor must be able to host at
/// least this; `bind` refuses a weaker `isolation_support`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsolationRequirement {
    /// The minimum isolation the workload needs.
    pub minimum: IsolationClass,
}

/// `InterruptSupport` — whether the executor honours the two-phase
/// interruption (`term` → `kill`). `Unknown` is treated as `Unsupported`
/// (AC-R-2.5.5-12 — the catalog reports `unknown`, the behaviour is the
/// safe one: an `irreversible` effect under an `unknown`-interrupt executor
/// is never killed before `deadline` and never redispatched).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptSupport {
    /// The executor cannot interrupt a running effect.
    Unsupported,
    /// The executor honours `term`/`kill` with a grace window.
    Supported,
    /// Declared `unknown` — behaves as `Unsupported`, reports `unknown`.
    Unknown,
}

impl InterruptSupport {
    /// The canonical spelling (the catalog member — `unknown` is reported
    /// honestly, not coerced).
    pub fn as_str(self) -> &'static str {
        match self {
            InterruptSupport::Unsupported => "unsupported",
            InterruptSupport::Supported => "supported",
            InterruptSupport::Unknown => "unknown",
        }
    }

    /// The *effective* support — `unknown` collapses to `unsupported` (the
    /// behaviour the dispatch may rely on), while the declared member is what
    /// the catalog reports.
    pub fn effective(self) -> InterruptSupport {
        match self {
            InterruptSupport::Unknown => InterruptSupport::Unsupported,
            other => other,
        }
    }
}

/// `ExecutorDeclaration` — the executor's honest capability record (T-LCD-07 —
/// every member declared, `none`/`unknown` honest). `bind` reads it against
/// the capability's `execution_requirement`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutorDeclaration {
    /// The executor's name (the `tool_executor` component's variant label).
    pub executor_id: String,
    /// The isolation the executor can host (`local_host`'s in-process
    /// executor declares `none`; the sandboxed helper declares
    /// `process_sandbox`).
    pub isolation_support: IsolationClass,
    /// The dedup store the executor holds.
    pub dedup_support: DedupSupport,
    /// The probe support.
    pub probe_support: ProbeSupport,
    /// The two-phase interruption support (`unknown` ⇒ `unsupported`
    /// behaviourally — AC-R-2.5.5-12).
    pub interrupt: InterruptSupport,
    /// The error classes the executor can emit at `tool` origin (the
    /// `error_classes ⊆ ErrorClass` declaration — a tool-reported class outside
    /// this set is a `protocol_error`).
    pub error_classes: BTreeSet<String>,
    /// Whether the executor streams ephemeral output (`output_chunk`/
    /// `progress`).
    pub streams: bool,
    /// The `EffectDomain`s the executor can run (the bound capability's
    /// domain must be a member).
    pub domains: BTreeSet<hh_hir::kinds::EffectDomain>,
}

/// `bind` failure — an insufficient declaration is a typed refusal.
#[derive(Debug, Clone, PartialEq)]
pub enum BindError {
    /// `dedup_support` cannot cover the capability's idempotency requirement.
    InsufficientDedup {
        /// What the capability needs.
        required: DedupSupport,
        /// What the executor declared.
        declared: DedupSupport,
    },
    /// `isolation_support` is weaker than the capability's floor.
    InsufficientIsolation {
        /// The required floor.
        required: IsolationClass,
        /// What the executor declared.
        declared: IsolationClass,
    },
    /// The executor cannot run the capability's domain.
    UnsupportedDomain {
        /// The domain.
        domain: String,
    },
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindError::InsufficientDedup { required, declared } => write!(
                f,
                "InsufficientDedup: need {} got {}",
                required.as_str(),
                declared.as_str()
            ),
            BindError::InsufficientIsolation { required, declared } => write!(
                f,
                "InsufficientIsolation: need {} got {}",
                required.as_str(),
                declared.as_str()
            ),
            BindError::UnsupportedDomain { domain } => {
                write!(f, "UnsupportedDomain: {domain}")
            }
        }
    }
}

impl std::error::Error for BindError {}

/// `bind(declaration, capability_domain, requirement)` — refuse when the
/// executor's declaration cannot cover the capability's `execution_requirement`
/// (dedup, isolation, domain). `requirement` carries the capability's parsed
/// `execution_requirement` members the bind check reads.
pub fn bind(
    decl: &ExecutorDeclaration,
    domain: hh_hir::kinds::EffectDomain,
    requirement: &ExecutionRequirement,
) -> Result<(), BindError> {
    // Domain coverage.
    if !decl.domains.contains(&domain) {
        return Err(BindError::UnsupportedDomain {
            domain: domain.name().to_string(),
        });
    }
    // Isolation floor — `strength()` ranks mechanisms; `external` is weakest
    // (a participant claim). The executor must host ≥ the requirement.
    if decl.isolation_support.strength() < requirement.minimum_isolation.strength() {
        return Err(BindError::InsufficientIsolation {
            required: requirement.minimum_isolation,
            declared: decl.isolation_support,
        });
    }
    // Dedup floor.
    if decl.dedup_support < requirement.minimum_dedup {
        return Err(BindError::InsufficientDedup {
            required: requirement.minimum_dedup,
            declared: decl.dedup_support,
        });
    }
    Ok(())
}

/// `ExecutionRequirement` — the parsed members of a capability's
/// `execution_requirement` the bind check reads (the capability's declared
/// floor). Defaults are the honest minimum (no floor ⇒ nothing to check).
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionRequirement {
    /// The minimum isolation the workload needs (default `none` — honest:
    /// a capability that does not declare a floor runs anywhere).
    pub minimum_isolation: IsolationClass,
    /// The minimum dedup the capability's idempotency needs (default `none` —
    /// a capability that does not declare idempotency needs no executor store;
    /// the kernel's dedup still guards it).
    pub minimum_dedup: DedupSupport,
}

impl Default for ExecutionRequirement {
    fn default() -> Self {
        ExecutionRequirement {
            minimum_isolation: IsolationClass::None,
            minimum_dedup: DedupSupport::None,
        }
    }
}

/// `ExecutionRequest` — what the kernel hands the executor at `execute`
/// (§5d.5 §2 request row). The executor sees the canonical args + the
/// capability + the token it must echo — never the proposal or the decision.
#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    /// The execution id (the dispatch's).
    pub execution_id: String,
    /// The effect.
    pub effect_id: String,
    /// The attempt.
    pub attempt_no: u64,
    /// The capability (pinned).
    pub capability_ref: (String, String),
    /// The declared effect class.
    pub effect: EffectClass,
    /// The canonical args (`CanonicalArgs.params` — post-arg-map).
    pub args: Json,
    /// The environment the effect runs in.
    pub env_handle_id: String,
    /// The attribution token the executor must echo on every captured item.
    pub attribution_token: String,
    /// The effective deadline (the ladder's min).
    pub deadline_ms: Option<u64>,
    /// The deadline ladder (for the executor's own timeout checks).
    pub ladder: DeadlineLadder,
    /// The retained-bytes cap (the output policy's).
    pub retain_bytes_cap: u64,
}

/// `TerminalReport` — the executor's terminal report (the `terminal` capture
/// item's payload source). The executor *reports* outcome/retryable *hints* —
/// the kernel's own rules settle the lifecycle (I-3; a hint may only lower).
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalReport {
    /// `ok | error{class}`.
    pub status: TerminalStatus,
    /// The process exit status, when a process ran.
    pub exit_status: Option<i64>,
    /// The executor's outcome claim (`applied|not_applied|unknown`).
    pub outcome_hint: String,
    /// The executor's retryable hint (may only lower).
    pub retryable_hint: Option<bool>,
    /// A detail ref (masked).
    pub detail_ref: Option<String>,
    /// Whether the executor truncated output under `retain_bytes_cap`
    /// (the manifest's `truncation` flag — ADR-0101 D6).
    pub truncated: bool,
    /// The bytes dropped under the cap (`truncation.omitted_bytes`).
    pub omitted_bytes: u64,
    /// The output's pre-cap size (`truncation.original_size`).
    pub original_size: u64,
}

/// `TerminalStatus` — `ok` or `error{class}` (the tool's own result).
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalStatus {
    /// Clean.
    Ok,
    /// The tool reported a failure — `class` is the tool-reported member
    /// (drawn from the capability's declared `error_classes`; the kernel
    /// validates membership — an undeclared class is a `protocol_error`).
    ToolError {
        /// The reported class.
        class: ErrorClass,
    },
}

/// `ExecutorSignal` — one item the executor emits during `execute`: the
/// echoed `attribution_token` + the capture kind. The dispatcher resolves the
/// token per signal — an unresolvable echo is `unattributed` (the marker is
/// appended; the item is dropped, never silently attributed).
#[derive(Debug, Clone)]
pub struct ExecutorSignal {
    /// The token the executor echoes (must equal `request.attribution_token`).
    pub token: String,
    /// The capture kind.
    pub kind: crate::capture::CaptureKind,
}

/// `ToolExecutor` — the operation-shaped executor component (I-3: reports,
/// never decides). The kernel calls `execute` at the `execute` stage; the
/// executor returns a stream of capture items + a terminal report. `probe`
/// answers whether a lapsed-window effect applied.
pub trait ToolExecutor {
    /// The executor's declaration.
    fn declaration(&self) -> &ExecutorDeclaration;

    /// `execute(request, sink)` — run the capability; emit `ExecutorSignal`s
    /// to `sink` (ephemeral `output_chunk`/`progress` stream; the rest are the
    /// manifest's items); return the terminal report. Every signal echoes
    /// `request.attribution_token` — the dispatcher resolves it.
    fn execute(
        &mut self,
        request: &ExecutionRequest,
        sink: &mut dyn FnMut(ExecutorSignal),
    ) -> Result<TerminalReport, EnvError>;

    /// `probe(effect_id, attempt_no)` — did a lapsed-window effect apply?
    /// `ProbeSupport::None` ⇒ `Err(Unsupported)` (the kernel's `unknown` →
    /// probe path handles it; a `none` executor is honest that it cannot
    /// check).
    fn probe(&self, effect_id: &str, attempt_no: u64) -> Result<ProbeVerdict, EnvError>;
}

/// `ProbeVerdict` — the probe's answer (`applied|not_applied|undeterminable`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeVerdict {
    /// The effect applied.
    Applied,
    /// The effect did not apply.
    NotApplied,
    /// Cannot be determined.
    Undeterminable,
}

impl ProbeVerdict {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeVerdict::Applied => "applied",
            ProbeVerdict::NotApplied => "not_applied",
            ProbeVerdict::Undeterminable => "undeterminable",
        }
    }
}
