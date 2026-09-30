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

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<DedupSupport> {
        match s {
            "none" => Some(DedupSupport::None),
            "best_effort" => Some(DedupSupport::BestEffort),
            "durable" => Some(DedupSupport::Durable),
            _ => None,
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
    /// An `execution_requirement` member outside the closed spelling set.
    MalformedRequirement {
        /// The offending member.
        member: String,
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
            BindError::MalformedRequirement { member } => {
                write!(f, "MalformedRequirement: {member}")
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

/// `EnvironmentRequirement` — one member of a capability's
/// `execution_requirement.env_requires[]` (the spec's
/// `env_requires(EnvironmentRequirement)` — §5d.1's `PreconditionDomain` arm
/// checked at `bind`, AC-R-2.5.1-8). The member spellings are closed:
/// `{kind: "binary", name}` (an executable resolvable in the environment),
/// `{kind: "env", key}` (an environment variable the environment declares),
/// `{kind: "path", path}` (a path that must exist inside the environment).
/// Unknown kinds/members are refused, never coerced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentRequirement {
    /// `binary` — an executable by name resolvable in the environment.
    Binary {
        /// The binary name.
        name: String,
    },
    /// `env` — an environment variable key the environment declares.
    Env {
        /// The variable key.
        key: String,
    },
    /// `path` — a path that must exist in the environment.
    Path {
        /// The path.
        path: String,
    },
}

impl EnvironmentRequirement {
    /// The canonical spelling of one member (`kind:arg`).
    pub fn display(&self) -> String {
        match self {
            EnvironmentRequirement::Binary { name } => format!("binary:{name}"),
            EnvironmentRequirement::Env { key } => format!("env:{key}"),
            EnvironmentRequirement::Path { path } => format!("path:{path}"),
        }
    }

    /// Parse one `env_requires[]` member (`{kind, name|key|path}`).
    pub fn from_json(j: &Json) -> Result<EnvironmentRequirement, BindError> {
        let kind = j.get("kind").and_then(Json::as_str).ok_or_else(|| {
            BindError::MalformedRequirement {
                member: "env_requires member needs kind".into(),
            }
        })?;
        let str_member = |k: &str| -> Result<String, BindError> {
            j.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| BindError::MalformedRequirement {
                    member: format!("env_requires.{kind}.{k} missing"),
                })
        };
        match kind {
            "binary" => Ok(EnvironmentRequirement::Binary {
                name: str_member("name")?,
            }),
            "env" => Ok(EnvironmentRequirement::Env {
                key: str_member("key")?,
            }),
            "path" => Ok(EnvironmentRequirement::Path {
                path: str_member("path")?,
            }),
            other => Err(BindError::MalformedRequirement {
                member: format!("env_requires kind {other}"),
            }),
        }
    }
}

/// `EnvironmentProbe` — the usability-probe surface a bound environment
/// exposes to `bind` (§5d.1: `bind(version_id, environment, executors[])`;
/// AC-R-2.5.1-8). Each answer is tri-state: `Some(bool)` decided,
/// `None` the probe cannot decide (the fact class is undeclared for this
/// environment — `bind` treats undecidable as unmet, never silently passes).
pub trait EnvironmentProbe {
    /// Whether `name` resolves to an executable in the environment.
    fn binary_present(&self, name: &str) -> Option<bool>;
    /// Whether `key` is declared in the environment's variable set.
    fn env_key_present(&self, key: &str) -> Option<bool>;
    /// Whether `path` exists inside the environment.
    fn path_present(&self, path: &str) -> Option<bool>;

    /// Evaluate one requirement — `Some(bool)` decided, `None` undecidable.
    fn satisfied(&self, req: &EnvironmentRequirement) -> Option<bool> {
        match req {
            EnvironmentRequirement::Binary { name } => self.binary_present(name),
            EnvironmentRequirement::Env { key } => self.env_key_present(key),
            EnvironmentRequirement::Path { path } => self.path_present(path),
        }
    }
}

/// `LocalEnvProbe` — the probe for `local_host`/`local_sandboxed`
/// environments: `binary` resolves against the process `PATH` (existence,
/// never execution); `env` reads the process environment's declared keys;
/// `path` is an existence check. Offline — no subprocess, no network.
#[derive(Debug, Default)]
pub struct LocalEnvProbe;

impl EnvironmentProbe for LocalEnvProbe {
    fn binary_present(&self, name: &str) -> Option<bool> {
        let path_var = std::env::var_os("PATH")?;
        Some(std::env::split_paths(&path_var).any(|dir| {
            let candidate = dir.join(name);
            candidate.is_file()
        }))
    }

    fn env_key_present(&self, key: &str) -> Option<bool> {
        Some(std::env::var_os(key).is_some())
    }

    fn path_present(&self, path: &str) -> Option<bool> {
        Some(std::path::Path::new(path).exists())
    }
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
    /// `env_requires[]` — the environment facts `bind` must satisfy
    /// (S2.11 — the Stage-2 row; AC-R-2.5.1-8).
    pub env_requires: Vec<EnvironmentRequirement>,
    /// `capability_requires[]` — `Ref<ToolCapability>` members the link must
    /// resolve in the same registry snapshot (E1 `capability_requires` —
    /// §5d.1 check points: `capability_requires` at `link`).
    pub capability_requires: Vec<hh_hir::refs::Ref>,
}

impl Default for ExecutionRequirement {
    fn default() -> Self {
        ExecutionRequirement {
            minimum_isolation: IsolationClass::None,
            minimum_dedup: DedupSupport::None,
            env_requires: Vec::new(),
            capability_requires: Vec::new(),
        }
    }
}

impl ExecutionRequirement {
    /// Parse the members of a capability's `execution_requirement` the bind
    /// check reads — `minimum_isolation`/`isolation`, `minimum_dedup`/`dedup`,
    /// `env_requires[]`, `capability_requires[]`; absent members default to
    /// the honest minimum (no floor ⇒ nothing to check). Unknown spellings
    /// are refused, never coerced.
    pub fn from_json(j: &Json) -> Result<ExecutionRequirement, BindError> {
        let iso = j
            .get("minimum_isolation")
            .or_else(|| j.get("isolation"))
            .and_then(Json::as_str)
            .map(|s| {
                IsolationClass::parse(s).map_err(|_| BindError::MalformedRequirement {
                    member: format!("minimum_isolation={s}"),
                })
            })
            .transpose()?
            .unwrap_or(IsolationClass::None);
        let dedup = j
            .get("minimum_dedup")
            .or_else(|| j.get("dedup"))
            .and_then(Json::as_str)
            .map(|s| {
                DedupSupport::parse(s).ok_or_else(|| BindError::MalformedRequirement {
                    member: format!("minimum_dedup={s}"),
                })
            })
            .transpose()?
            .unwrap_or(DedupSupport::None);
        let env_requires = match j.get("env_requires") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(EnvironmentRequirement::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => {
                return Err(BindError::MalformedRequirement {
                    member: "env_requires must be an array".into(),
                })
            }
            None => Vec::new(),
        };
        let capability_requires = match j.get("capability_requires") {
            Some(Json::Arr(items)) => items
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    hh_hir::refs::Ref::from_json(m, &format!("capability_requires[{i}]")).map_err(
                        |e| BindError::MalformedRequirement {
                            member: format!("capability_requires[{i}]: {e}"),
                        },
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => {
                return Err(BindError::MalformedRequirement {
                    member: "capability_requires must be an array".into(),
                })
            }
            None => Vec::new(),
        };
        Ok(ExecutionRequirement {
            minimum_isolation: iso,
            minimum_dedup: dedup,
            env_requires,
            capability_requires,
        })
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
    /// The `idempotency_key` the prepare stage derived — the executor-side
    /// dedup store keys on it (R-2.5.5¹; the kernel's dedup remains the
    /// authoritative guard — an executor hit is a *recorded-verdict
    /// replay*, never a fresh decision).
    pub idempotency_key: String,
    /// The `commit` stage's write-ahead evidence — the helper recomputes
    /// the `commit_proof` over its session nonce and refuses `NotCommitted`
    /// when the members don't match (S2.1; §5d.5 §4 commit_token).
    pub commit_evidence: crate::helper::CommitEvidence,
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

// ── The registry `bind` verb (§5d.1 §2; ADR-0088 D5; S1.17) ──────────────────

/// `ExecutorBinding{capability_version, executor_ref,
/// environment_handle_class, dedup_support, interrupt, isolation, usability}`
/// — the record `bind` produces (ledgered `lifecycle.component.bound{class_id
/// = tool_executor}` by the caller).
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutorBinding {
    /// The capability's `version_id` (the registered record).
    pub capability_version: String,
    /// The executor's id.
    pub executor_ref: String,
    /// The environment handle class the binding targets (`kernel_internal` or
    /// `sandbox_helper` at C0 — the `environment_class` member of the
    /// capability's `execution_requirement`).
    pub environment_handle_class: String,
    /// The executor's dedup support.
    pub dedup_support: DedupSupport,
    /// The executor's interrupt support.
    pub interrupt: InterruptSupport,
    /// The executor's isolation support.
    pub isolation: IsolationClass,
    /// `usable` | `unusable` (the catalogue's per-view usability — AC-E1-8:
    /// an unbindable capability is `unusable`, never silently dropped).
    pub usability: &'static str,
}

/// The spec-spelled `bind` failure sum — `env_requires_unmet |
/// isolation_insufficient | dedup_required_unsupported | not_installed`
/// (§5d.1 §2; `env_requires` matching itself is the Stage-2 row — the variant
/// is declared now so the spelling is stable).
#[derive(Debug, Clone, PartialEq)]
pub enum BindFailure {
    /// An `env_requires` member the environment cannot satisfy (Stage-2 check).
    EnvRequiresUnmet {
        /// The unmet requirement.
        requirement: String,
    },
    /// `isolation_support` weaker than the declared floor.
    IsolationInsufficient {
        /// The required floor.
        required: IsolationClass,
        /// What the executor declared.
        declared: IsolationClass,
    },
    /// `dedup_support` cannot cover the idempotency requirement.
    DedupRequiredUnsupported {
        /// What the capability needs.
        required: DedupSupport,
        /// What the executor declared.
        declared: DedupSupport,
    },
    /// No declared executor covers the capability (its domain or its
    /// `environment_class`) — the request `not_installed` spelling.
    NotInstalled {
        /// Why no executor matched.
        detail: String,
    },
}

impl std::fmt::Display for BindFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindFailure::EnvRequiresUnmet { requirement } => {
                write!(f, "env_requires_unmet: {requirement}")
            }
            BindFailure::IsolationInsufficient { required, declared } => write!(
                f,
                "isolation_insufficient: need {} got {}",
                required.as_str(),
                declared.as_str()
            ),
            BindFailure::DedupRequiredUnsupported { required, declared } => write!(
                f,
                "dedup_required_unsupported: need {} got {}",
                required.as_str(),
                declared.as_str()
            ),
            BindFailure::NotInstalled { detail } => write!(f, "not_installed: {detail}"),
        }
    }
}

impl std::error::Error for BindFailure {}

/// `bind(version_id, environment, executors[])` — the §5d.1 registry verb
/// (ADR-0088 D5). Reads the registered capability's `execution_requirement`
/// and declared effect domains, probes `env_requires`/`capability_requires`
/// against `environment`/`store` (S2.11 — AC-R-2.5.1-8: an unmet
/// `env_requires` is `env_requires_unmet`, never silently honoured; an
/// undecidable probe answer is unmet — fail-closed), then runs [`bind`]
/// against each [`ExecutorDeclaration`] and returns the first admissible
/// [`ExecutorBinding`] (deterministic: executors are tried in declaration
/// order).
pub fn bind_capability(
    store: &hh_registry::store::RegistryStore,
    version_id: &str,
    environment: &dyn EnvironmentProbe,
    executors: &[ExecutorDeclaration],
) -> Result<ExecutorBinding, BindFailure> {
    let (cap, _registrar) =
        hh_registry::capability::capability_at(store, version_id).map_err(|e| {
            BindFailure::NotInstalled {
                detail: format!("capability lookup failed: {e:?}"),
            }
        })?;
    let hh_hir::records::KindRecord::ToolCapability(t) = &cap.node.semantic else {
        return Err(BindFailure::NotInstalled {
            detail: "record is not a capability".into(),
        });
    };
    let requirement = ExecutionRequirement::from_json(&t.execution_requirement).map_err(|e| {
        BindFailure::NotInstalled {
            detail: format!("{e}"),
        }
    })?;
    // `env_requires` at `bind` (E1 check point; AC-R-2.5.1-8) — an absent or
    // undecidable fact is `env_requires_unmet`, never a silent pass.
    for req in &requirement.env_requires {
        match environment.satisfied(req) {
            Some(true) => {}
            Some(false) => {
                return Err(BindFailure::EnvRequiresUnmet {
                    requirement: req.display(),
                })
            }
            None => {
                return Err(BindFailure::EnvRequiresUnmet {
                    requirement: format!("{} (probe: undecidable)", req.display()),
                })
            }
        }
    }
    // `capability_requires` at `link` — each `Ref<ToolCapability>` must
    // resolve to a capability record in the same registry snapshot (a
    // `Selector` version is not bindable — the link names pinned records).
    for dep in &requirement.capability_requires {
        let vid = match &dep.version {
            hh_hir::refs::RefVersion::Pinned(v) => v.clone(),
            hh_hir::refs::RefVersion::Selector(s) => {
                return Err(BindFailure::NotInstalled {
                    detail: format!(
                        "capability_requires unpinned selector {}@{s}",
                        dep.semantic_id
                    ),
                })
            }
        };
        match hh_registry::capability::capability_at(store, &vid) {
            Ok((dep_cap, _)) => {
                if dep_cap.node.semantic_id() != dep.semantic_id {
                    return Err(BindFailure::NotInstalled {
                        detail: format!(
                            "capability_requires {vid} resolves {} not {}",
                            dep_cap.node.semantic_id(),
                            dep.semantic_id
                        ),
                    });
                }
            }
            Err(e) => {
                return Err(BindFailure::NotInstalled {
                    detail: format!("capability_requires {} unmet: {e:?}", dep.semantic_id),
                })
            }
        }
    }
    let env_class = t
        .execution_requirement
        .get("environment_class")
        .and_then(Json::as_str)
        .unwrap_or("kernel_internal")
        .to_string();
    let domains: Vec<hh_hir::kinds::EffectDomain> = match &t.effects {
        hh_hir::kinds::ToolEffects::Pure => Vec::new(),
        hh_hir::kinds::ToolEffects::Declared(set) => set.iter().map(|e| e.domain).collect(),
    };
    let mut first_failure: Option<BindFailure> = None;
    for decl in executors {
        // A `pure` capability names no domain — only the floors are checked.
        let verdict = if domains.is_empty() {
            if decl.isolation_support.strength() < requirement.minimum_isolation.strength() {
                Err(BindError::InsufficientIsolation {
                    required: requirement.minimum_isolation,
                    declared: decl.isolation_support,
                })
            } else if decl.dedup_support < requirement.minimum_dedup {
                Err(BindError::InsufficientDedup {
                    required: requirement.minimum_dedup,
                    declared: decl.dedup_support,
                })
            } else {
                Ok(())
            }
        } else {
            let mut res = Ok(());
            for d in &domains {
                res = bind(decl, *d, &requirement);
                if res.is_err() {
                    break;
                }
            }
            res
        };
        match verdict {
            Ok(()) => {
                return Ok(ExecutorBinding {
                    capability_version: version_id.to_string(),
                    executor_ref: decl.executor_id.clone(),
                    environment_handle_class: env_class,
                    dedup_support: decl.dedup_support,
                    interrupt: decl.interrupt,
                    isolation: decl.isolation_support,
                    usability: "usable",
                });
            }
            Err(BindError::InsufficientIsolation { required, declared }) => {
                first_failure
                    .get_or_insert(BindFailure::IsolationInsufficient { required, declared });
            }
            Err(BindError::InsufficientDedup { required, declared }) => {
                first_failure
                    .get_or_insert(BindFailure::DedupRequiredUnsupported { required, declared });
            }
            Err(BindError::UnsupportedDomain { domain }) => {
                first_failure.get_or_insert(BindFailure::NotInstalled {
                    detail: format!("no executor covers domain {domain}"),
                });
            }
            Err(BindError::MalformedRequirement { member }) => {
                first_failure.get_or_insert(BindFailure::NotInstalled {
                    detail: format!("malformed requirement {member}"),
                });
            }
        }
    }
    Err(first_failure.unwrap_or(BindFailure::NotInstalled {
        detail: "no executors declared".into(),
    }))
}

/// `UsabilityReport` — the catalog view's per-environment usability probe
/// result (AC-R-2.5.1-8/AC-E1-8): `usable` when `bind` succeeds;
/// `unusable` carrying the typed [`BindFailure`] otherwise — an unbindable
/// capability is reported, never silently dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct UsabilityReport {
    /// `usable` | `unusable`.
    pub usability: &'static str,
    /// The bind failure that makes the view `unusable`.
    pub failure: Option<BindFailure>,
    /// The admissible binding when `usable`.
    pub binding: Option<ExecutorBinding>,
}

/// `usability(version_id, environment, executors[])` — the per-environment
/// probe the catalog view renders (§5d.1: "a record with no successful
/// binding in an environment is `usability = unusable` in that environment's
/// catalog view"). Deterministic: the same `(store snapshot, version_id,
/// probe answers, executors)` yields the same report.
pub fn usability(
    store: &hh_registry::store::RegistryStore,
    version_id: &str,
    environment: &dyn EnvironmentProbe,
    executors: &[ExecutorDeclaration],
) -> UsabilityReport {
    match bind_capability(store, version_id, environment, executors) {
        Ok(b) => UsabilityReport {
            usability: "usable",
            failure: None,
            binding: Some(b),
        },
        Err(f) => UsabilityReport {
            usability: "unusable",
            failure: Some(f),
            binding: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_containment::policy::IsolationClass;
    use hh_hir::document::Node;
    use hh_hir::kinds::{EntityKind, ToolEffects};
    use hh_hir::leaves::Text;
    use hh_hir::records::{KindRecord, Resources, ScopeBindings, ToolCapabilityRecord};
    use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};
    use hh_registry::records::{CapabilityRecord, RegistryRecord};
    use hh_registry::store::RegistryStore;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hh-exec-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn prov() -> ProvenanceRecord {
        ProvenanceRecord::kernel("executor.test", 0)
    }

    fn text_prov(seq: u64) -> ProvenanceRecord {
        ProvenanceRecord::minted(
            Origin::human("tester", HumanRole::Author),
            PersistenceScope::Definition,
            seq,
        )
    }

    /// A V-E1-clean capability with the given `execution_requirement`.
    fn cap_record(exec_req: Json) -> ToolCapabilityRecord {
        ToolCapabilityRecord {
            purpose: Text::new("a test capability", "test", text_prov(1)),
            input_schema: Json::obj([(
                "properties",
                Json::obj([("path", Json::obj([("type", Json::str("string"))]))]),
            )]),
            output_schema: None,
            effects: ToolEffects::Pure,
            preconditions: vec![],
            scope_bindings: ScopeBindings::Unknown,
            resources: Resources::NoneDeclared,
            observation_contract: Json::obj([(
                "error_classes",
                Json::Arr(vec![Json::str("io_error")]),
            )]),
            cost_model: None,
            execution_requirement: exec_req,
            source: Json::obj([("kind", Json::str("native_variant"))]),
            exposure_hint: Json::obj([("default", Json::str("direct"))]),
            postconditions: vec![],
            flow_contract: None,
            action_patterns: Vec::new(),
        }
    }

    fn register_cap(store: &mut RegistryStore, exec_req: Json) -> String {
        let node = Node::new(
            EntityKind::ToolCapability,
            KindRecord::ToolCapability(cap_record(exec_req)),
            text_prov(1),
        );
        store
            .register(
                RegistryRecord::Capability(CapabilityRecord { node }),
                &prov(),
                None,
            )
            .expect("register")
            .version_id
    }

    fn executor() -> ExecutorDeclaration {
        ExecutorDeclaration {
            executor_id: "mock/1".to_string(),
            isolation_support: IsolationClass::None,
            dedup_support: DedupSupport::BestEffort,
            probe_support: ProbeSupport::Check,
            interrupt: InterruptSupport::Supported,
            error_classes: BTreeSet::new(),
            streams: true,
            domains: BTreeSet::new(),
        }
    }

    /// A scripted probe — each fact class answers from the named sets;
    /// `None` members make that class undecidable.
    struct StubProbe {
        binaries: Option<BTreeSet<String>>,
        env_keys: Option<BTreeSet<String>>,
        paths: Option<BTreeSet<String>>,
    }

    impl StubProbe {
        fn all() -> Self {
            StubProbe {
                binaries: Some(BTreeSet::new()),
                env_keys: Some(BTreeSet::new()),
                paths: Some(BTreeSet::new()),
            }
        }
    }

    impl EnvironmentProbe for StubProbe {
        fn binary_present(&self, name: &str) -> Option<bool> {
            self.binaries.as_ref().map(|s| s.contains(name))
        }
        fn env_key_present(&self, key: &str) -> Option<bool> {
            self.env_keys.as_ref().map(|s| s.contains(key))
        }
        fn path_present(&self, path: &str) -> Option<bool> {
            self.paths.as_ref().map(|s| s.contains(path))
        }
    }

    // ── AC-R-2.5.1-8 — `env_requires`/`capability_requires` at `bind` ────────

    #[test]
    fn ac_r_2_5_1_8_env_requires_satisfied_binds_usable() {
        let mut store = RegistryStore::open(dir("e251-8-sat"), &prov()).unwrap();
        let vid = register_cap(
            &mut store,
            Json::obj([
                ("environment_class", Json::str("local_host")),
                (
                    "env_requires",
                    Json::Arr(vec![
                        Json::obj([("kind", Json::str("binary")), ("name", Json::str("jq"))]),
                        Json::obj([("kind", Json::str("env")), ("key", Json::str("HOME"))]),
                    ]),
                ),
            ]),
        );
        let mut env = StubProbe::all();
        env.binaries.as_mut().unwrap().insert("jq".into());
        env.env_keys.as_mut().unwrap().insert("HOME".into());
        let binding = bind_capability(&store, &vid, &env, &[executor()]).expect("bind");
        assert_eq!(binding.usability, "usable");
        assert_eq!(binding.environment_handle_class, "local_host");
    }

    #[test]
    fn ac_r_2_5_1_8_env_requires_unmet_refuses_typed() {
        let mut store = RegistryStore::open(dir("e251-8-unmet"), &prov()).unwrap();
        let vid = register_cap(
            &mut store,
            Json::obj([(
                "env_requires",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("binary")),
                    ("name", Json::str("missing-tool")),
                ])]),
            )]),
        );
        let env = StubProbe::all(); // decides, but the binary is absent
        let err = bind_capability(&store, &vid, &env, &[executor()]).expect_err("unmet");
        assert_eq!(
            err,
            BindFailure::EnvRequiresUnmet {
                requirement: "binary:missing-tool".into()
            }
        );
        // The catalog view reports `unusable` carrying the typed refusal.
        let report = usability(&store, &vid, &env, &[executor()]);
        assert_eq!(report.usability, "unusable");
        assert!(matches!(
            report.failure,
            Some(BindFailure::EnvRequiresUnmet { .. })
        ));
    }

    #[test]
    fn ac_r_2_5_1_8_undecidable_probe_is_unmet_never_silent() {
        let mut store = RegistryStore::open(dir("e251-8-undec"), &prov()).unwrap();
        let vid = register_cap(
            &mut store,
            Json::obj([(
                "env_requires",
                Json::Arr(vec![Json::obj([
                    ("kind", Json::str("path")),
                    ("path", Json::str("/x")),
                ])]),
            )]),
        );
        let env = StubProbe {
            binaries: None,
            env_keys: None,
            paths: None, // the probe cannot decide
        };
        let err = bind_capability(&store, &vid, &env, &[executor()]).expect_err("undecidable");
        assert!(
            matches!(err, BindFailure::EnvRequiresUnmet { ref requirement }
                if requirement.contains("undecidable")),
            "undecidable must be env_requires_unmet, got {err:?}"
        );
    }

    #[test]
    fn ac_r_2_5_1_8_capability_requires_checked_at_link() {
        let mut store = RegistryStore::open(dir("e251-8-capreq"), &prov()).unwrap();
        let dep_vid = register_cap(&mut store, Json::obj([]));
        let dep_semantic = hh_registry::capability::capability_at(&store, &dep_vid)
            .unwrap()
            .0
            .node
            .semantic_id();
        // Satisfied: the dependency resolves to the pinned record.
        let vid = register_cap(
            &mut store,
            Json::obj([(
                "capability_requires",
                Json::Arr(vec![Json::obj([
                    ("semantic_id", Json::str(dep_semantic.as_str())),
                    ("version_id", Json::str(dep_vid.as_str())),
                ])]),
            )]),
        );
        let env = StubProbe::all();
        bind_capability(&store, &vid, &env, &[executor()]).expect("dep met");
        // Unmet: a pinned ref to a version not in the snapshot.
        let vid2 = register_cap(
            &mut store,
            Json::obj([(
                "capability_requires",
                Json::Arr(vec![Json::obj([
                    ("semantic_id", Json::str("dep/absent")),
                    ("version_id", Json::str("cap/9")),
                ])]),
            )]),
        );
        let err = bind_capability(&store, &vid2, &env, &[executor()]).expect_err("dep unmet");
        assert!(matches!(err, BindFailure::NotInstalled { .. }));
    }

    #[test]
    fn ac_r_2_5_1_8_executor_floors_still_typed() {
        let mut store = RegistryStore::open(dir("e251-8-floors"), &prov()).unwrap();
        let vid = register_cap(
            &mut store,
            Json::obj([("minimum_isolation", Json::str("process_sandbox"))]),
        );
        let env = StubProbe::all();
        let err = bind_capability(&store, &vid, &env, &[executor()]).expect_err("weak iso");
        assert!(matches!(
            err,
            BindFailure::IsolationInsufficient {
                required: IsolationClass::ProcessSandbox,
                declared: IsolationClass::None
            }
        ));
    }

    #[test]
    fn ac_r_2_5_1_8_malformed_env_requires_refuses() {
        let mut store = RegistryStore::open(dir("e251-8-bad"), &prov()).unwrap();
        let vid = register_cap(
            &mut store,
            Json::obj([(
                "env_requires",
                Json::Arr(vec![Json::obj([("kind", Json::str("widget"))])]),
            )]),
        );
        let err =
            bind_capability(&store, &vid, &StubProbe::all(), &[executor()]).expect_err("malformed");
        assert!(matches!(err, BindFailure::NotInstalled { .. }));
    }
}
