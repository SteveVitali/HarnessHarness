//! `EnvelopePolicy` — the sealed MUST-data record the envelope interprets
//! (§5e.2 data model; ADR-0106 D1/D5, ADR-0107, ADR-0108). One typed record on
//! `AgentProcess.native.control_boundary.guards`, referenced from the run
//! manifest as `envelope_policy_ref`, `policy_id` content-addressed
//! (`idp_id("envelope_policy.1", canonical bytes)` — CC1).
//!
//! Sub-policies: `RetryPolicy` (map over `(scope_kind, error_class)` —
//! `model_call` keys `ModelErrorClass`, every other kind `ErrorClass`,
//! CF-314), `TimeoutPolicy` (kind-fixed `ScopeTerminal`s), `LoopPolicy`
//! (deterministic detector thresholds + the `nudge → deny → stop` ladder),
//! `OutputValidationPolicy` (`strict` default; `repair_then_strict` is the
//! declared C1 member), `ExhaustionPolicy` (per-dimension `stop|escalate` +
//! declared grace), `InvariantSet` (INV-1…9 mandatory; `ext.*` additive only),
//! and definition `StopRule`s over the closed trigger grammar (never `Text`,
//! never a model call — kernel rules are implicit and always present).
//!
//! The canonical form is integers-only (the `Json` model carries no floats):
//! backoff `factor`/`jitter` spell as ppm (`1_000_000` ≡ 1.0). Admissible
//! ranges are checked at `from_json`/`validate` — a malformed policy fails
//! `PolicyUnsealed`-class errors, never silently clamps.

use std::collections::{BTreeMap, BTreeSet};

use hh_identity::idp::idp_id;
use hh_ontology::control::{InvariantId, LoopDetectorKind, StopReason};
use hh_ontology::dimensions::DimensionId;
use hh_wire::json::Json;

use crate::vocab::{DeltaKind, ScopeKind};

/// The idp domain for `EnvelopePolicy.policy_id` (ADR-0106 D5 — the sealed
/// policy is content-addressed with the definition; one canonicalizer, CC1).
pub const ENVELOPE_POLICY_IDP: &str = "envelope_policy.1";

/// `jitter`/`factor` are ppm-scaled integers (`1_000_000` ≡ 1.0) — canonical
/// form is integers-only.
pub const PPM: u64 = 1_000_000;

/// `EnvelopePolicy` — the sealed record (§5e.2).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvelopePolicy {
    /// `budget_ref` — the root budget the envelope checks against.
    pub budget_ref: String,
    /// Definition stop rules (evaluated by priority at G-DECIDE; kernel rules
    /// are implicit and always present — they are not listed here).
    pub stop_rules: Vec<StopRule>,
    /// The retry table.
    pub retry: RetryPolicy,
    /// The timeout table.
    pub timeouts: TimeoutPolicy,
    /// The loop-detection policy.
    pub loop_policy: LoopPolicy,
    /// The output-validation policy.
    pub output_validation: OutputValidationPolicy,
    /// The invariant set (INV-1…9 mandatory; `ext.*` additive only).
    pub invariants: InvariantSet,
    /// Per-dimension exhaustion dispositions.
    pub exhaustion: ExhaustionPolicy,
    /// Conditioned `HarnessRule` refs (I9 — model-conditioned behaviour only
    /// through conditioned rules with debt records).
    pub conditioned_rules: Vec<String>,
    /// `policy_id` — the content address (set by `seal`; verified at `arm`).
    pub policy_id: String,
}

impl EnvelopePolicy {
    /// Compute `policy_id` over the canonical form minus `policy_id`
    /// (self-address like `idp` records — the id is over the sealed body).
    pub fn seal(mut self) -> Result<EnvelopePolicy, PolicyError> {
        self.validate()?;
        self.policy_id = String::new();
        self.policy_id = idp_id(
            ENVELOPE_POLICY_IDP,
            self.to_json().to_canonical_string().as_bytes(),
        );
        Ok(self)
    }

    /// Verify `policy_id` equals the content address of the sealed body
    /// (`arm` refuses `PolicyUnsealed` when it does not).
    pub fn verify_seal(&self) -> bool {
        let mut p = self.clone();
        p.policy_id = String::new();
        idp_id(
            ENVELOPE_POLICY_IDP,
            p.to_json().to_canonical_string().as_bytes(),
        ) == self.policy_id
    }

    /// Validate the record (called by `seal` and `from_json`): INV-1…9 present,
    /// every timeout spec's `on_expiry` is the kind-fixed terminal, ladder
    /// order is `nudge → deny → stop`, every `ext.*` invariant is additive,
    /// admissible ranges hold.
    pub fn validate(&self) -> Result<(), PolicyError> {
        for inv in [
            InvariantId::Inv1,
            InvariantId::Inv2,
            InvariantId::Inv3,
            InvariantId::Inv4,
            InvariantId::Inv5,
            InvariantId::Inv6,
            InvariantId::Inv7,
            InvariantId::Inv8,
            InvariantId::Inv9,
        ] {
            if !self.invariants.set.contains(&inv) {
                return Err(PolicyError::MissingInvariant { id: inv.as_str() });
            }
        }
        self.timeouts.validate()?;
        self.retry.validate()?;
        self.loop_policy.validate()?;
        self.output_validation.validate()?;
        self.exhaustion.validate()?;
        for r in &self.stop_rules {
            if r.priority == 0 {
                return Err(PolicyError::BadRange {
                    member: "stop_rules.priority".into(),
                    detail: "priority ≥ 1 (kernel rules occupy 0-reserved space)".into(),
                });
            }
        }
        Ok(())
    }

    /// The default C0/Stage-1 policy (the audited values of §5e.2:
    /// `window_events: 25`, `exact_repeat{cycle_max: 5, threshold: 5}`,
    /// `no_progress{threshold: 3}`, `error_streak{threshold: 3}`,
    /// `monologue{threshold: 3}`, `response_ladder: [nudge, deny, stop]`,
    /// `max_format_failures: 3`, `mode: strict`).
    pub fn stage1_default(budget_ref: &str) -> EnvelopePolicy {
        let mut retry = RetryPolicy::default();
        retry.insert_model_defaults();
        let mut timeouts = TimeoutPolicy::default();
        timeouts.insert_kind_defaults();
        EnvelopePolicy {
            budget_ref: budget_ref.to_string(),
            stop_rules: vec![],
            retry,
            timeouts,
            loop_policy: LoopPolicy::default(),
            output_validation: OutputValidationPolicy::default(),
            invariants: InvariantSet::mandatory(),
            exhaustion: ExhaustionPolicy::default(),
            conditioned_rules: vec![],
            policy_id: String::new(),
        }
    }

    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("budget_ref", Json::str(&self.budget_ref)),
            ("retry", self.retry.to_json()),
            ("timeouts", self.timeouts.to_json()),
            ("loop", self.loop_policy.to_json()),
            ("output_validation", self.output_validation.to_json()),
            ("invariants", self.invariants.to_json()),
            ("exhaustion", self.exhaustion.to_json()),
            (
                "stop_rules",
                Json::Arr(self.stop_rules.iter().map(StopRule::to_json).collect()),
            ),
            (
                "conditioned_rules",
                Json::Arr(self.conditioned_rules.iter().map(Json::str).collect()),
            ),
        ];
        if !self.policy_id.is_empty() {
            m.push(("policy_id", Json::str(&self.policy_id)));
        }
        Json::obj(m)
    }

    /// Parse + validate the canonical form (`PolicyError` on any violation —
    /// never a silent default).
    pub fn from_json(j: &Json) -> Result<EnvelopePolicy, PolicyError> {
        let p = EnvelopePolicy {
            budget_ref: str_at(j, "budget_ref")?,
            retry: RetryPolicy::from_json(req(j, "retry")?)?,
            timeouts: TimeoutPolicy::from_json(req(j, "timeouts")?)?,
            loop_policy: LoopPolicy::from_json(req(j, "loop")?)?,
            output_validation: OutputValidationPolicy::from_json(req(j, "output_validation")?)?,
            invariants: InvariantSet::from_json(req(j, "invariants")?)?,
            exhaustion: ExhaustionPolicy::from_json(req(j, "exhaustion")?)?,
            stop_rules: opt_arr(j, "stop_rules")?
                .iter()
                .map(StopRule::from_json)
                .collect::<Result<_, _>>()?,
            conditioned_rules: opt_arr(j, "conditioned_rules")?
                .iter()
                .map(|r| {
                    r.as_str()
                        .map(String::from)
                        .ok_or_else(|| PolicyError::BadField {
                            member: "conditioned_rules".into(),
                        })
                })
                .collect::<Result<_, _>>()?,
            policy_id: j
                .get("policy_id")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string(),
        };
        p.validate()?;
        Ok(p)
    }
}

/// `PolicyError` — the typed refusal (`PolicyUnsealed` covers a bad
/// `policy_id`; the rest are record violations).
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyError {
    /// `policy_id` does not match the content address of the sealed body.
    PolicyUnsealed,
    /// An INV-1…9 member is absent from `invariants`.
    MissingInvariant {
        /// The missing invariant spelling.
        id: String,
    },
    /// A required member is missing or mistyped.
    BadField {
        /// The member.
        member: String,
    },
    /// An admissible-range violation.
    BadRange {
        /// The member.
        member: String,
        /// Why.
        detail: String,
    },
    /// A `TimeoutPolicy` row names a terminal that is not the kind-fixed one.
    NonFixedTerminal {
        /// The scope kind.
        scope_kind: String,
        /// The terminal given.
        got: String,
        /// The mandated terminal.
        want: String,
    },
    /// An unknown member spelling in a closed sum.
    UnknownMember {
        /// The member.
        member: String,
        /// The spelling.
        spelling: String,
    },
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::PolicyUnsealed => write!(f, "policy_unsealed"),
            PolicyError::MissingInvariant { id } => write!(f, "missing_invariant{{{id}}}"),
            PolicyError::BadField { member } => write!(f, "bad_field{{{member}}}"),
            PolicyError::BadRange { member, detail } => {
                write!(f, "bad_range{{{member}}}: {detail}")
            }
            PolicyError::NonFixedTerminal {
                scope_kind,
                got,
                want,
            } => write!(f, "non_fixed_terminal{{{scope_kind}}}: {got} ≠ {want}"),
            PolicyError::UnknownMember { member, spelling } => {
                write!(f, "unknown_member{{{member}}}: {spelling}")
            }
        }
    }
}

impl std::error::Error for PolicyError {}

// ─────────────────────────────────────────────────────────────────────────────
// StopRule
// ─────────────────────────────────────────────────────────────────────────────

/// `StopRule{trigger, action, reason, priority}` — a definition stop rule
/// (§5e.2; kernel rules are implicit and always present — they evaluate at
/// the fixed kernel priorities `invariant > cancel > exhaustion > loop/format
/// > definition rules > β`, so a definition `priority` orders within the
/// > definition band only).
#[derive(Debug, Clone, PartialEq)]
pub struct StopRule {
    /// The trigger — the closed grammar (never `Text`, never a model call).
    pub trigger: StopTrigger,
    /// `stop` | `escalate` | `deny_next` | `nudge`.
    pub action: RuleAction,
    /// The `StopReason` the rule fires (`action = stop` only — carried so the
    /// recorded `control.decision{stop}` names it).
    pub reason: StopReason,
    /// Priority within the definition band (≥ 1; lower fires first).
    pub priority: u32,
}

/// The closed trigger grammar (§5e.2: "counters, gauges, scope-kind terminal
/// events, deterministic detector verdicts, invariant ids — never `Text`,
/// never a model call").
#[derive(Debug, Clone, PartialEq)]
pub enum StopTrigger {
    /// A counter reached a bound: `counter{dimension} ≥ value`.
    CounterGte {
        /// The counter dimension.
        dimension: DimensionId,
        /// The threshold.
        value: i64,
    },
    /// A gauge reached a bound: `gauge{dimension} ≥ ppm`.
    GaugeGte {
        /// The gauge dimension (`context.occupancy`, `delegation_depth`,
        /// `fan_out`).
        dimension: DimensionId,
        /// The level in ppm of the cap (1_000_000 = cap).
        ppm: i64,
    },
    /// A scope kind reached a terminal class (`scope_terminal{kind, class}`).
    ScopeTerminal {
        /// The scope kind.
        kind: ScopeKind,
        /// The terminal class spelling (e.g. `failed`, `unknown`).
        terminal: String,
    },
    /// A deterministic detector reached a ladder rung.
    DetectorRung {
        /// The detector variant.
        detector: LoopDetectorKind,
        /// The ladder rung (`nudge` | `deny` | `stop`).
        rung: LadderAction,
    },
    /// An invariant fired.
    InvariantFired {
        /// The invariant.
        invariant_id: InvariantId,
    },
}

/// The `response_ladder`'s rung vocabulary — `control.loop.detected.action ∈
/// {nudge, deny, stop}` (§5e.2 ledger row; `escalate` is never a ladder rung).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LadderAction {
    /// Deliver a nudge (a `HarnessRule` artefact — T-LCD-13).
    Nudge,
    /// Deny the next step of the same loop_key.
    Deny,
    /// Stop the run.
    Stop,
}

impl LadderAction {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LadderAction::Nudge => "nudge",
            LadderAction::Deny => "deny",
            LadderAction::Stop => "stop",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<LadderAction> {
        Some(match s {
            "nudge" => LadderAction::Nudge,
            "deny" => LadderAction::Deny,
            "stop" => LadderAction::Stop,
            _ => return None,
        })
    }
}

/// `StopRule.action ∈ {stop, escalate, deny_next, nudge}` — a distinct sum
/// from the ladder's `{nudge, deny, stop}` (the rule's deny spells
/// `deny_next` and the rule admits `escalate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleAction {
    /// `stop` — fire the stop protocol with `reason`.
    Stop,
    /// `escalate` — raise to the principal.
    Escalate,
    /// `deny_next` — refuse the next step (a `GuardVerdict::respond`
    /// denial).
    DenyNext,
    /// `nudge` — deliver the nudge artefact.
    Nudge,
}

impl RuleAction {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RuleAction::Stop => "stop",
            RuleAction::Escalate => "escalate",
            RuleAction::DenyNext => "deny_next",
            RuleAction::Nudge => "nudge",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<RuleAction> {
        Some(match s {
            "stop" => RuleAction::Stop,
            "escalate" => RuleAction::Escalate,
            "deny_next" => RuleAction::DenyNext,
            "nudge" => RuleAction::Nudge,
            _ => return None,
        })
    }
}

impl StopRule {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut t = vec![("kind", Json::str(self.trigger.kind()))];
        match &self.trigger {
            StopTrigger::CounterGte { dimension, value } => {
                t.push(("dimension", Json::str(dimension.as_str())));
                t.push(("value", Json::Int(*value)));
            }
            StopTrigger::GaugeGte { dimension, ppm } => {
                t.push(("dimension", Json::str(dimension.as_str())));
                t.push(("ppm", Json::Int(*ppm)));
            }
            StopTrigger::ScopeTerminal { kind, terminal } => {
                t.push(("scope_kind", Json::str(kind.as_str())));
                t.push(("terminal", Json::str(terminal)));
            }
            StopTrigger::DetectorRung { detector, rung } => {
                t.push(("detector", Json::str(detector.as_str())));
                t.push(("rung", Json::str(rung.as_str())));
            }
            StopTrigger::InvariantFired { invariant_id } => {
                t.push(("invariant_id", Json::str(invariant_id.as_str())));
            }
        }
        let trigger_obj: std::collections::BTreeMap<String, Json> =
            t.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        Json::obj([
            ("trigger", Json::Obj(trigger_obj)),
            ("action", Json::str(self.action.as_str())),
            ("reason", self.reason.to_json()),
            ("priority", Json::Int(self.priority as i64)),
        ])
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json) -> Result<StopRule, PolicyError> {
        let t = req(j, "trigger")?;
        let trigger = match str_at(t, "kind")?.as_str() {
            "counter_gte" => StopTrigger::CounterGte {
                dimension: DimensionId::parse(&str_at(t, "dimension")?).ok_or_else(|| {
                    PolicyError::UnknownMember {
                        member: "trigger.dimension".into(),
                        spelling: str_at(t, "dimension").unwrap_or_default(),
                    }
                })?,
                value: int_at(t, "value")?,
            },
            "gauge_gte" => StopTrigger::GaugeGte {
                dimension: DimensionId::parse(&str_at(t, "dimension")?).ok_or_else(|| {
                    PolicyError::UnknownMember {
                        member: "trigger.dimension".into(),
                        spelling: str_at(t, "dimension").unwrap_or_default(),
                    }
                })?,
                ppm: int_at(t, "ppm")?,
            },
            "scope_terminal" => StopTrigger::ScopeTerminal {
                kind: ScopeKind::parse(&str_at(t, "scope_kind")?).ok_or_else(|| {
                    PolicyError::UnknownMember {
                        member: "trigger.scope_kind".into(),
                        spelling: str_at(t, "scope_kind").unwrap_or_default(),
                    }
                })?,
                terminal: str_at(t, "terminal")?,
            },
            "detector_rung" => StopTrigger::DetectorRung {
                detector: LoopDetectorKind::parse(&str_at(t, "detector")?).ok_or_else(|| {
                    PolicyError::UnknownMember {
                        member: "trigger.detector".into(),
                        spelling: str_at(t, "detector").unwrap_or_default(),
                    }
                })?,
                rung: LadderAction::parse(&str_at(t, "rung")?).ok_or_else(|| {
                    PolicyError::UnknownMember {
                        member: "trigger.rung".into(),
                        spelling: str_at(t, "rung").unwrap_or_default(),
                    }
                })?,
            },
            "invariant_fired" => StopTrigger::InvariantFired {
                invariant_id: InvariantId::parse(&str_at(t, "invariant_id")?).ok_or_else(|| {
                    PolicyError::UnknownMember {
                        member: "trigger.invariant_id".into(),
                        spelling: str_at(t, "invariant_id").unwrap_or_default(),
                    }
                })?,
            },
            other => {
                return Err(PolicyError::UnknownMember {
                    member: "trigger.kind".into(),
                    spelling: other.into(),
                })
            }
        };
        Ok(StopRule {
            trigger,
            action: RuleAction::parse(&str_at(j, "action")?).ok_or_else(|| {
                PolicyError::UnknownMember {
                    member: "action".into(),
                    spelling: str_at(j, "action").unwrap_or_default(),
                }
            })?,
            reason: StopReason::from_json(req(j, "reason")?).ok_or_else(|| {
                PolicyError::BadField {
                    member: "reason".into(),
                }
            })?,
            priority: int_at(j, "priority")? as u32,
        })
    }
}

impl StopTrigger {
    /// The trigger-kind tag.
    pub fn kind(&self) -> &'static str {
        match self {
            StopTrigger::CounterGte { .. } => "counter_gte",
            StopTrigger::GaugeGte { .. } => "gauge_gte",
            StopTrigger::ScopeTerminal { .. } => "scope_terminal",
            StopTrigger::DetectorRung { .. } => "detector_rung",
            StopTrigger::InvariantFired { .. } => "invariant_fired",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RetryPolicy
// ─────────────────────────────────────────────────────────────────────────────

/// `RetryPolicy` — the map `(scope_kind, error_class) → RetrySpec` (§5e.2;
/// ADR-0107 D1–D3). `model_call` keys on `ModelErrorClass` spellings; every
/// other kind keys on `ErrorClass` spellings (CF-314) — the map key carries
/// the canonical spelling string and `RetryPolicy::spec` resolves it typed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RetryPolicy {
    /// The table.
    pub table: BTreeMap<(ScopeKind, String), RetrySpec>,
}

impl RetryPolicy {
    /// The spec for `(scope_kind, error_class spelling)`, or `None` (the
    /// error is not retryable under this policy — `give_up`).
    pub fn spec(&self, kind: ScopeKind, error_class: &str) -> Option<&RetrySpec> {
        self.table.get(&(kind, error_class.to_string()))
    }

    /// Insert a spec (builder).
    pub fn set(&mut self, kind: ScopeKind, error_class: &str, spec: RetrySpec) {
        self.table.insert((kind, error_class.to_string()), spec);
    }

    /// Validate every spec's admissible ranges.
    pub fn validate(&self) -> Result<(), PolicyError> {
        for ((kind, class), spec) in &self.table {
            if class.is_empty() {
                return Err(PolicyError::BadField {
                    member: format!("retry[{kind:?}].error_class"),
                });
            }
            spec.validate(&format!("retry[{},{}]", kind.as_str(), class))?;
        }
        Ok(())
    }

    /// The audited Stage-1 model-call defaults (ADR-0107 D1–D7 keyed by the
    /// CF-470 mapped names): transient classes retry with bounded backoff;
    /// `context_length_exceeded`/`invalid_request`/`content_policy`/
    /// `cancelled`/`unknown` are absent — never retryable.
    pub fn insert_model_defaults(&mut self) {
        let transient = RetrySpec {
            max_attempts: MaxAttempts::Bounded(3),
            backoff: Backoff {
                initial_ms: 250,
                factor_ppm: 2 * PPM,
                max_ms: 5_000,
                jitter_ppm: 200_000,
            },
            honour_retry_after: true,
            attempt_delta_allowed: vec![DeltaKind::Transport],
        };
        for class in [
            "network",
            "timeout{connect}",
            "timeout{attempt}",
            "timeout{stream_idle}",
            "stream_decode",
            "server_error",
            "overloaded",
            "rate_limited",
            "auth_expired",
        ] {
            self.set(ScopeKind::ModelCall, class, transient.clone());
        }
        // Tool attempts: `timeout` and `executor_error` retry once by default
        // (idempotent classes only — INV-8 is enforced at pre_dispatch).
        let tool = RetrySpec {
            max_attempts: MaxAttempts::Bounded(2),
            backoff: Backoff {
                initial_ms: 100,
                factor_ppm: 2 * PPM,
                max_ms: 1_000,
                jitter_ppm: 100_000,
            },
            honour_retry_after: false,
            attempt_delta_allowed: vec![],
        };
        for class in ["timeout", "executor_error", "environment_unavailable"] {
            self.set(ScopeKind::ToolAttempt, class, tool.clone());
        }
    }

    /// Canonical JSON: `{<scope_kind>: {<error_class>: spec}}`.
    pub fn to_json(&self) -> Json {
        let mut by_kind: BTreeMap<&str, Vec<(&String, &RetrySpec)>> = BTreeMap::new();
        for ((kind, class), spec) in &self.table {
            by_kind
                .entry(kind.as_str())
                .or_default()
                .push((class, spec));
        }
        let outer: Vec<(String, Json)> = by_kind
            .into_iter()
            .map(|(k, specs)| {
                let inner: Vec<(String, Json)> = specs
                    .into_iter()
                    .map(|(c, s)| (c.clone(), s.to_json()))
                    .collect();
                (k.to_string(), Json::Obj(inner.into_iter().collect()))
            })
            .collect();
        Json::Obj(outer.into_iter().collect())
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json) -> Result<RetryPolicy, PolicyError> {
        let mut table = BTreeMap::new();
        let obj = match j {
            Json::Obj(m) => m,
            _ => {
                return Err(PolicyError::BadField {
                    member: "retry".into(),
                })
            }
        };
        for (kind_name, specs) in obj {
            let kind = ScopeKind::parse(kind_name).ok_or_else(|| PolicyError::UnknownMember {
                member: "retry.scope_kind".into(),
                spelling: kind_name.clone(),
            })?;
            let inner = match specs {
                Json::Obj(m) => m,
                _ => {
                    return Err(PolicyError::BadField {
                        member: format!("retry.{kind_name}"),
                    })
                }
            };
            for (class, spec) in inner {
                table.insert((kind, class.clone()), RetrySpec::from_json(spec)?);
            }
        }
        Ok(RetryPolicy { table })
    }
}

/// `max_attempts: int | unbounded` — `unbounded` is still INV-6-bounded by the
/// single `retries` counter's hard ceiling (the spec's "bounded counter"
/// rule: an unbounded per-class spec is admitted only while `retries` has
/// headroom — the envelope folds both into `schedule_retry`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxAttempts {
    /// A bounded attempt count (total attempts incl. the first).
    Bounded(u32),
    /// Unbounded per class — bounded globally by the `retries` dimension.
    Unbounded,
}

/// `RetrySpec{max_attempts, backoff, honour_retry_after,
/// attempt_delta_allowed}` (§5e.2).
#[derive(Debug, Clone, PartialEq)]
pub struct RetrySpec {
    /// The attempt bound (inclusive of attempt 1).
    pub max_attempts: MaxAttempts,
    /// The backoff shape.
    pub backoff: Backoff,
    /// Whether a provider `retry-after` hint is honoured (it only ever
    /// *extends* `not_before` — never shortens it).
    pub honour_retry_after: bool,
    /// The `DeltaKind`s a retry may record (`[]` = the request is byte-stable
    /// — any change is `attempt_delta` not permitted).
    pub attempt_delta_allowed: Vec<DeltaKind>,
}

impl RetrySpec {
    /// `delay_ms(attempt_no, jitter_key)` — `min(initial · factor^(n-1),
    /// max)` plus deterministic jitter in `[0, delay · jitter]` derived from
    /// a hash of the key (pure — the same `(attempt, key)` always yields the
    /// same delay, AC-F2-06's "jitter within bounds" without a randomness
    /// source the replay could diverge on).
    pub fn delay_ms(&self, attempt_no: u64, jitter_key: &str) -> u64 {
        let base = {
            let mut d = self.backoff.initial_ms;
            for _ in 1..attempt_no {
                d = d.saturating_mul(self.backoff.factor_ppm) / PPM;
            }
            d.min(self.backoff.max_ms)
        };
        if self.backoff.jitter_ppm == 0 || base == 0 {
            return base;
        }
        let h = hh_wire::sha256::sha256_hex(jitter_key.as_bytes());
        let jitter_unit = u64::from_str_radix(&h[..8], 16).unwrap_or(0) % PPM;
        base.saturating_add(base.saturating_mul(self.backoff.jitter_ppm) / PPM * jitter_unit / PPM)
    }

    /// Validate admissible ranges.
    pub fn validate(&self, member: &str) -> Result<(), PolicyError> {
        if let MaxAttempts::Bounded(n) = self.max_attempts {
            if n == 0 {
                return Err(PolicyError::BadRange {
                    member: member.into(),
                    detail: "max_attempts ≥ 1".into(),
                });
            }
        }
        if self.backoff.initial_ms > self.backoff.max_ms {
            return Err(PolicyError::BadRange {
                member: member.into(),
                detail: "backoff.initial_ms ≤ backoff.max_ms".into(),
            });
        }
        if self.backoff.jitter_ppm > PPM {
            return Err(PolicyError::BadRange {
                member: member.into(),
                detail: "jitter ∈ [0,1] (jitter_ppm ≤ 1_000_000)".into(),
            });
        }
        if self.backoff.factor_ppm < PPM {
            return Err(PolicyError::BadRange {
                member: member.into(),
                detail: "factor ≥ 1.0 (factor_ppm ≥ 1_000_000)".into(),
            });
        }
        Ok(())
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let max = match self.max_attempts {
            MaxAttempts::Bounded(n) => Json::Int(n as i64),
            MaxAttempts::Unbounded => Json::str("unbounded"),
        };
        Json::obj([
            ("max_attempts", max),
            (
                "backoff",
                Json::obj([
                    ("initial_ms", Json::Int(self.backoff.initial_ms as i64)),
                    ("factor_ppm", Json::Int(self.backoff.factor_ppm as i64)),
                    ("max_ms", Json::Int(self.backoff.max_ms as i64)),
                    ("jitter_ppm", Json::Int(self.backoff.jitter_ppm as i64)),
                ]),
            ),
            ("honour_retry_after", Json::Bool(self.honour_retry_after)),
            (
                "attempt_delta_allowed",
                Json::Arr(
                    self.attempt_delta_allowed
                        .iter()
                        .map(|d| Json::str(d.as_str()))
                        .collect(),
                ),
            ),
        ])
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json) -> Result<RetrySpec, PolicyError> {
        let max_attempts = match req(j, "max_attempts")? {
            Json::Int(n) => MaxAttempts::Bounded(*n as u32),
            Json::Str(s) if s == "unbounded" => MaxAttempts::Unbounded,
            _ => {
                return Err(PolicyError::BadField {
                    member: "max_attempts".into(),
                })
            }
        };
        let b = req(j, "backoff")?;
        let spec = RetrySpec {
            max_attempts,
            backoff: Backoff {
                initial_ms: int_at(b, "initial_ms")? as u64,
                factor_ppm: int_at(b, "factor_ppm")? as u64,
                max_ms: int_at(b, "max_ms")? as u64,
                jitter_ppm: int_at(b, "jitter_ppm")? as u64,
            },
            honour_retry_after: bool_at(j, "honour_retry_after")?,
            attempt_delta_allowed: opt_arr(j, "attempt_delta_allowed")?
                .iter()
                .map(|d| {
                    let s = d.as_str().unwrap_or("");
                    Some(match s {
                        "transport" => DeltaKind::Transport,
                        "sampling_temperature" => DeltaKind::SamplingTemperature,
                        _ if s.starts_with("model_fallback{") && s.ends_with('}') => {
                            DeltaKind::ModelFallback {
                                profile_ref: s["model_fallback{".len()..s.len() - 1].to_string(),
                            }
                        }
                        "model_fallback" => DeltaKind::ModelFallback {
                            profile_ref: String::new(),
                        },
                        _ => return None,
                    })
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| PolicyError::BadField {
                    member: "attempt_delta_allowed".into(),
                })?,
        };
        spec.validate("retry_spec")?;
        Ok(spec)
    }
}

/// `backoff{initial_ms, factor, max_ms, jitter}` — factor/jitter as ppm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    /// The first delay.
    pub initial_ms: u64,
    /// The multiplier per attempt, ppm (`2_000_000` = ×2.0).
    pub factor_ppm: u64,
    /// The delay cap.
    pub max_ms: u64,
    /// The jitter fraction, ppm of the delay (`[0, 1_000_000]`).
    pub jitter_ppm: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// TimeoutPolicy
// ─────────────────────────────────────────────────────────────────────────────

/// `TimeoutPolicy` — the map scope-kind → `TimeoutSpec` (§5e.2; ADR-0107
/// D4–D5). `on_expiry` is the kind-fixed terminal — the table records it so
/// the sealed policy is self-describing, but `validate` refuses any spelling
/// other than the mandated one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimeoutPolicy {
    /// The table.
    pub table: BTreeMap<ScopeKind, TimeoutSpec>,
}

impl TimeoutPolicy {
    /// The spec for `kind`.
    pub fn spec(&self, kind: ScopeKind) -> Option<&TimeoutSpec> {
        self.table.get(&kind)
    }

    /// The audited Stage-1 defaults per kind (the kind-fixed terminals are
    /// `ScopeTerminal::for_kind`).
    pub fn insert_kind_defaults(&mut self) {
        for (kind, default_ms, hard_max_ms) in [
            (ScopeKind::ModelCall, 60_000u64, 120_000u64),
            (ScopeKind::ToolAttempt, 30_000, 60_000),
            (ScopeKind::Compaction, 60_000, 120_000),
            (ScopeKind::Validator, 30_000, 60_000),
            (ScopeKind::Subagent, 300_000, 600_000),
            (ScopeKind::Permission, 0, 300_000), // `attended` ⇒ deadline none
            (ScopeKind::Resource, 10_000, 30_000),
        ] {
            self.table.insert(
                kind,
                TimeoutSpec {
                    default_ms,
                    extend_on_progress: true,
                    hard_max_ms,
                    on_expiry: ScopeTerminal::for_kind(kind),
                },
            );
        }
    }

    /// Validate: `default ≤ hard_max`, `extend_on_progress` implies
    /// `default ≤ hard_max`, and `on_expiry` is the kind-fixed terminal.
    pub fn validate(&self) -> Result<(), PolicyError> {
        for (kind, spec) in &self.table {
            if spec.default_ms > spec.hard_max_ms {
                return Err(PolicyError::BadRange {
                    member: format!("timeouts.{}", kind.as_str()),
                    detail: "default_ms ≤ hard_max_ms".into(),
                });
            }
            let want = ScopeTerminal::for_kind(*kind);
            if spec.on_expiry != want {
                return Err(PolicyError::NonFixedTerminal {
                    scope_kind: kind.as_str().into(),
                    got: spec.on_expiry.as_str().into(),
                    want: want.as_str().into(),
                });
            }
        }
        Ok(())
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let m: Vec<(String, Json)> = self
            .table
            .iter()
            .map(|(k, s)| (k.as_str().to_string(), s.to_json()))
            .collect();
        Json::Obj(m.into_iter().collect())
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json) -> Result<TimeoutPolicy, PolicyError> {
        let mut table = BTreeMap::new();
        let obj = match j {
            Json::Obj(m) => m,
            _ => {
                return Err(PolicyError::BadField {
                    member: "timeouts".into(),
                })
            }
        };
        for (kind_name, spec) in obj {
            let kind = ScopeKind::parse(kind_name).ok_or_else(|| PolicyError::UnknownMember {
                member: "timeouts.scope_kind".into(),
                spelling: kind_name.clone(),
            })?;
            table.insert(kind, TimeoutSpec::from_json(spec, kind)?);
        }
        let p = TimeoutPolicy { table };
        p.validate()?;
        Ok(p)
    }
}

/// `{default_ms, extend_on_progress, hard_max_ms, on_expiry}` per scope kind.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeoutSpec {
    /// The nominal deadline (0 = none — `attended` permission waits).
    pub default_ms: u64,
    /// Progress events extend the deadline up to `hard_max_ms`, never beyond.
    pub extend_on_progress: bool,
    /// The hard cap on any extension.
    pub hard_max_ms: u64,
    /// The kind-fixed terminal on expiry.
    pub on_expiry: ScopeTerminal,
}

impl TimeoutSpec {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("default_ms", Json::Int(self.default_ms as i64)),
            ("extend_on_progress", Json::Bool(self.extend_on_progress)),
            ("hard_max_ms", Json::Int(self.hard_max_ms as i64)),
            ("on_expiry", Json::str(self.on_expiry.as_str())),
        ])
    }

    /// Parse the canonical form (`kind` is supplied for the fixed-terminal
    /// check — `on_expiry` must equal `ScopeTerminal::for_kind(kind)`).
    pub fn from_json(j: &Json, kind: ScopeKind) -> Result<TimeoutSpec, PolicyError> {
        let expiry = str_at(j, "on_expiry")?;
        TimeoutSpec {
            default_ms: int_at(j, "default_ms")? as u64,
            extend_on_progress: bool_at(j, "extend_on_progress")?,
            hard_max_ms: int_at(j, "hard_max_ms")? as u64,
            on_expiry: ScopeTerminal::parse(&expiry).ok_or_else(|| PolicyError::UnknownMember {
                member: "on_expiry".into(),
                spelling: expiry,
            })?,
        }
        .check_kind(kind)
    }

    fn check_kind(self, kind: ScopeKind) -> Result<TimeoutSpec, PolicyError> {
        let want = ScopeTerminal::for_kind(kind);
        if self.on_expiry != want {
            return Err(PolicyError::NonFixedTerminal {
                scope_kind: kind.as_str().into(),
                got: self.on_expiry.as_str().into(),
                want: want.as_str().into(),
            });
        }
        Ok(self)
    }
}

/// `ScopeTerminal` — the kind-fixed terminal a fired deadline writes (§5e.2
/// `TimeoutPolicy` row; "a fired timeout is a typed terminal for its scope,
/// never an exception").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeTerminal {
    /// `model_call → model.call.failed{error_class: timeout}`.
    ModelCallFailedTimeout,
    /// `tool_attempt/effect → action.effect.unknown{cause: timeout}`
    /// (non-`read_only`).
    EffectUnknownTimeout,
    /// `tool_attempt/effect → observed(not_applied)` (`read_only`).
    EffectNotApplied,
    /// `permission → security.permission.decided{decision: timed_out,
    /// decider: policy}` → `action.effect.refused`.
    PermissionTimedOut,
    /// `subagent → control.subagent.cancelled{reason: deadline}` then the
    /// child's own drain.
    SubagentCancelledDeadline,
    /// `compaction → context.compaction.completed{status: failed}`.
    CompactionFailed,
    /// `validation → verification.validator.verdict{oracle_failure}`.
    ValidatorOracleFailure,
    /// `resource → ResourceLockTimeout` (R-2.6.5).
    ResourceLockTimeout,
}

impl ScopeTerminal {
    /// The kind-fixed terminal (the only admissible `on_expiry` for `kind`).
    pub fn for_kind(kind: ScopeKind) -> ScopeTerminal {
        match kind {
            ScopeKind::ModelCall => ScopeTerminal::ModelCallFailedTimeout,
            ScopeKind::ToolAttempt => ScopeTerminal::EffectUnknownTimeout,
            ScopeKind::Compaction => ScopeTerminal::CompactionFailed,
            ScopeKind::Validator => ScopeTerminal::ValidatorOracleFailure,
            ScopeKind::Subagent => ScopeTerminal::SubagentCancelledDeadline,
            ScopeKind::Permission => ScopeTerminal::PermissionTimedOut,
            ScopeKind::Resource => ScopeTerminal::ResourceLockTimeout,
        }
    }

    /// The `read_only`-effect variant (`EffectUnknownTimeout` degrades to
    /// `observed(not_applied)` — the effect never wrote ahead).
    pub fn for_read_only(self) -> ScopeTerminal {
        match self {
            ScopeTerminal::EffectUnknownTimeout => ScopeTerminal::EffectNotApplied,
            other => other,
        }
    }

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeTerminal::ModelCallFailedTimeout => "model.call.failed{timeout}",
            ScopeTerminal::EffectUnknownTimeout => "action.effect.unknown{timeout}",
            ScopeTerminal::EffectNotApplied => "action.effect.observed{not_applied}",
            ScopeTerminal::PermissionTimedOut => "security.permission.decided{timed_out}",
            ScopeTerminal::SubagentCancelledDeadline => "control.subagent.cancelled{deadline}",
            ScopeTerminal::CompactionFailed => "context.compaction.completed{failed}",
            ScopeTerminal::ValidatorOracleFailure => {
                "verification.validator.verdict{oracle_failure}"
            }
            ScopeTerminal::ResourceLockTimeout => "resource.lock.timeout",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ScopeTerminal> {
        Some(match s {
            "model.call.failed{timeout}" => ScopeTerminal::ModelCallFailedTimeout,
            "action.effect.unknown{timeout}" => ScopeTerminal::EffectUnknownTimeout,
            "action.effect.observed{not_applied}" => ScopeTerminal::EffectNotApplied,
            "security.permission.decided{timed_out}" => ScopeTerminal::PermissionTimedOut,
            "control.subagent.cancelled{deadline}" => ScopeTerminal::SubagentCancelledDeadline,
            "context.compaction.completed{failed}" => ScopeTerminal::CompactionFailed,
            "verification.validator.verdict{oracle_failure}" => {
                ScopeTerminal::ValidatorOracleFailure
            }
            "resource.lock.timeout" => ScopeTerminal::ResourceLockTimeout,
            _ => return None,
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LoopPolicy
// ─────────────────────────────────────────────────────────────────────────────

/// `LoopPolicy` (§5e.2 — the audited defaults are the struct defaults and are
/// declared Lab factors, never Core constants).
#[derive(Debug, Clone, PartialEq)]
pub struct LoopPolicy {
    /// The detector window in events (default 25).
    pub window_events: u32,
    /// `exact_repeat{cycle_max, threshold}` — a cycle of `cycle_len ≤
    /// cycle_max` repeated `threshold` times.
    pub exact_repeat: ExactRepeatSpec,
    /// `no_progress{threshold}` — same `loop_key` ∧ same
    /// `Observation.version_id` `threshold` times.
    pub no_progress: ThresholdSpec,
    /// `error_streak{threshold}` — consecutive error-class terminals.
    pub error_streak: ThresholdSpec,
    /// `monologue{threshold}` — consecutive model turns with no `act`.
    pub monologue: ThresholdSpec,
    /// `response_ladder` — always `[nudge, deny, stop]`; each rung fires once
    /// per run.
    pub response_ladder: Vec<LadderAction>,
    /// `judged` (C1) — declared member, `None` at Stage 1 (admitted only with
    /// a `Validator{kind: judge}` + `charged_to`; never sole grounds for a
    /// stop).
    pub judged: Option<JudgedSpec>,
}

/// `exact_repeat{cycle_max, threshold}` — `cycle_max` caps the detectable
/// cycle length (`k = 1…5` under the audited default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactRepeatSpec {
    /// The longest cycle the detector tracks (default 5).
    pub cycle_max: u32,
    /// The repeats needed to fire (default 5).
    pub threshold: u32,
}

/// A `{threshold}`-only detector spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThresholdSpec {
    /// The streak needed to fire.
    pub threshold: u32,
}

/// `judged{validator_ref, after_turns, interval, confidence_threshold,
/// charged_to}` — the declared C1 member (OQ-264's sibling; admitted only at
/// C1, never sole grounds for a stop).
#[derive(Debug, Clone, PartialEq)]
pub struct JudgedSpec {
    /// The `Validator{kind: judge}` ref.
    pub validator_ref: String,
    /// Turns before the judge may fire.
    pub after_turns: u32,
    /// The judge's cadence.
    pub interval: u32,
    /// The confidence floor (ppm).
    pub confidence_threshold_ppm: u64,
    /// The budget the judge charges to (ADR-0047 governance).
    pub charged_to: String,
}

impl Default for LoopPolicy {
    /// The audited values (§5e.2 `LoopPolicy` row).
    fn default() -> Self {
        LoopPolicy {
            window_events: 25,
            exact_repeat: ExactRepeatSpec {
                cycle_max: 5,
                threshold: 5,
            },
            no_progress: ThresholdSpec { threshold: 3 },
            error_streak: ThresholdSpec { threshold: 3 },
            monologue: ThresholdSpec { threshold: 3 },
            response_ladder: vec![LadderAction::Nudge, LadderAction::Deny, LadderAction::Stop],
            judged: None,
        }
    }
}

impl LoopPolicy {
    /// Validate admissible ranges (thresholds ≥ 1; the ladder is exactly
    /// `[nudge, deny, stop]` at C0 — `escalate` is not a ladder rung).
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.window_events == 0
            || self.exact_repeat.cycle_max == 0
            || self.exact_repeat.threshold == 0
            || self.no_progress.threshold == 0
            || self.error_streak.threshold == 0
            || self.monologue.threshold == 0
        {
            return Err(PolicyError::BadRange {
                member: "loop".into(),
                detail: "every window/threshold ≥ 1".into(),
            });
        }
        if self.response_ladder != [LadderAction::Nudge, LadderAction::Deny, LadderAction::Stop] {
            return Err(PolicyError::BadRange {
                member: "loop.response_ladder".into(),
                detail: "the C0 ladder is exactly [nudge, deny, stop]".into(),
            });
        }
        Ok(())
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("window_events", Json::Int(self.window_events as i64)),
            (
                "exact_repeat",
                Json::obj([
                    ("cycle_max", Json::Int(self.exact_repeat.cycle_max as i64)),
                    ("threshold", Json::Int(self.exact_repeat.threshold as i64)),
                ]),
            ),
            (
                "no_progress",
                Json::obj([("threshold", Json::Int(self.no_progress.threshold as i64))]),
            ),
            (
                "error_streak",
                Json::obj([("threshold", Json::Int(self.error_streak.threshold as i64))]),
            ),
            (
                "monologue",
                Json::obj([("threshold", Json::Int(self.monologue.threshold as i64))]),
            ),
            (
                "response_ladder",
                Json::Arr(
                    self.response_ladder
                        .iter()
                        .map(|a| Json::str(a.as_str()))
                        .collect(),
                ),
            ),
        ];
        if let Some(j) = &self.judged {
            m.push((
                "judged",
                Json::obj([
                    ("validator_ref", Json::str(&j.validator_ref)),
                    ("after_turns", Json::Int(j.after_turns as i64)),
                    ("interval", Json::Int(j.interval as i64)),
                    (
                        "confidence_threshold_ppm",
                        Json::Int(j.confidence_threshold_ppm as i64),
                    ),
                    ("charged_to", Json::str(&j.charged_to)),
                ]),
            ));
        }
        Json::obj(m)
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json) -> Result<LoopPolicy, PolicyError> {
        let thr = |name: &str| -> Result<u32, PolicyError> {
            Ok(int_at(req(j, name)?, "threshold")? as u32)
        };
        let ladder: Vec<LadderAction> = opt_arr(j, "response_ladder")?
            .iter()
            .map(|a| LadderAction::parse(a.as_str().unwrap_or("")))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| PolicyError::BadField {
                member: "response_ladder".into(),
            })?;
        let judged = match j.get("judged") {
            None | Some(Json::Null) => None,
            Some(jd) => Some(JudgedSpec {
                validator_ref: str_at(jd, "validator_ref")?,
                after_turns: int_at(jd, "after_turns")? as u32,
                interval: int_at(jd, "interval")? as u32,
                confidence_threshold_ppm: int_at(jd, "confidence_threshold_ppm")? as u64,
                charged_to: str_at(jd, "charged_to")?,
            }),
        };
        let p = LoopPolicy {
            window_events: int_at(j, "window_events")? as u32,
            exact_repeat: ExactRepeatSpec {
                cycle_max: int_at(req(j, "exact_repeat")?, "cycle_max")? as u32,
                threshold: thr("exact_repeat")?,
            },
            no_progress: ThresholdSpec {
                threshold: thr("no_progress")?,
            },
            error_streak: ThresholdSpec {
                threshold: thr("error_streak")?,
            },
            monologue: ThresholdSpec {
                threshold: thr("monologue")?,
            },
            response_ladder: ladder,
            judged,
        };
        p.validate()?;
        Ok(p)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OutputValidationPolicy
// ─────────────────────────────────────────────────────────────────────────────

/// `OutputValidationPolicy` (§5e.2): `mode` (`strict` default;
/// `repair_then_strict` is the declared C1 member), `repairs ⊆ {json_repair,
/// arg_coercion, trailing_text_strip}`, `max_format_failures` (default 3),
/// `on_failure: respond`, `on_exhaustion: stop{format_failure}`.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputValidationPolicy {
    /// The mode.
    pub mode: ValidationMode,
    /// The admitted repair set (C1; empty under `strict`).
    pub repairs: Vec<RepairKind>,
    /// `max_format_failures` — the running count that trips
    /// `stop{format_failure}`.
    pub max_format_failures: u32,
}

/// `mode ∈ {strict, repair_then_strict}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationMode {
    /// Strict — every failure is a rejection (the C0 default).
    Strict,
    /// `repair_then_strict` — declared repairs run first (C1).
    RepairThenStrict,
}

/// `repairs ⊆ {json_repair, arg_coercion, trailing_text_strip}` (C1 members).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairKind {
    /// Re-parse with a JSON repair pass.
    JsonRepair,
    /// Coerce argument types.
    ArgCoercion,
    /// Strip trailing non-JSON text.
    TrailingTextStrip,
}

impl RepairKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RepairKind::JsonRepair => "json_repair",
            RepairKind::ArgCoercion => "arg_coercion",
            RepairKind::TrailingTextStrip => "trailing_text_strip",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<RepairKind> {
        Some(match s {
            "json_repair" => RepairKind::JsonRepair,
            "arg_coercion" => RepairKind::ArgCoercion,
            "trailing_text_strip" => RepairKind::TrailingTextStrip,
            _ => return None,
        })
    }
}

impl Default for OutputValidationPolicy {
    /// `strict` + `max_format_failures: 3` (the audited values).
    fn default() -> Self {
        OutputValidationPolicy {
            mode: ValidationMode::Strict,
            repairs: vec![],
            max_format_failures: 3,
        }
    }
}

impl OutputValidationPolicy {
    /// Validate (repairs may be declared only under `repair_then_strict`;
    /// `max_format_failures ≥ 1`).
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.max_format_failures == 0 {
            return Err(PolicyError::BadRange {
                member: "output_validation.max_format_failures".into(),
                detail: "≥ 1".into(),
            });
        }
        if self.mode == ValidationMode::Strict && !self.repairs.is_empty() {
            return Err(PolicyError::BadRange {
                member: "output_validation.repairs".into(),
                detail: "repairs require mode = repair_then_strict (C1)".into(),
            });
        }
        Ok(())
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "mode",
                Json::str(match self.mode {
                    ValidationMode::Strict => "strict",
                    ValidationMode::RepairThenStrict => "repair_then_strict",
                }),
            ),
            (
                "repairs",
                Json::Arr(self.repairs.iter().map(|r| Json::str(r.as_str())).collect()),
            ),
            (
                "max_format_failures",
                Json::Int(self.max_format_failures as i64),
            ),
            ("on_failure", Json::str("respond")),
            ("on_exhaustion", Json::str("stop{format_failure}")),
        ])
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json) -> Result<OutputValidationPolicy, PolicyError> {
        let mode = match str_at(j, "mode")?.as_str() {
            "strict" => ValidationMode::Strict,
            "repair_then_strict" => ValidationMode::RepairThenStrict,
            other => {
                return Err(PolicyError::UnknownMember {
                    member: "mode".into(),
                    spelling: other.into(),
                })
            }
        };
        let p = OutputValidationPolicy {
            mode,
            repairs: opt_arr(j, "repairs")?
                .iter()
                .map(|r| RepairKind::parse(r.as_str().unwrap_or("")))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| PolicyError::BadField {
                    member: "repairs".into(),
                })?,
            max_format_failures: int_at(j, "max_format_failures")? as u32,
        };
        p.validate()?;
        Ok(p)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ExhaustionPolicy + InvariantSet
// ─────────────────────────────────────────────────────────────────────────────

/// `ExhaustionPolicy{per dimension → on_exhaustion ∈ {stop, escalate},
/// grace}` (§5e.2; ADR-0168 D6 — `interactive` attendance ⇒ `escalate` for
/// every ceiling; the Stage-1 default is `stop`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExhaustionPolicy {
    /// `dimension spelling → rule`.
    pub rules: BTreeMap<String, ExhaustionRule>,
}

/// One exhaustion rule: `on_exhaustion` plus the declared grace — the number
/// of `grace` calls admitted past the barrier for drain purposes (INV-3's
/// "except declared `grace` calls"; `0` = none, the Stage-1 default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExhaustionRule {
    /// `stop` (default) | `escalate` (`interactive` attendance only).
    pub on_exhaustion: ExhaustionAction,
    /// Declared grace calls admitted during drain.
    pub grace_calls: u32,
}

/// `on_exhaustion ∈ {stop, escalate}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExhaustionAction {
    /// `stop` (the Stage-1 default).
    Stop,
    /// `escalate` (`interactive` attendance only — ADR-0168 D6).
    Escalate,
}

impl ExhaustionPolicy {
    /// The rule for `dimension` (default: `stop`, no grace).
    pub fn rule(&self, dimension: DimensionId) -> ExhaustionRule {
        self.rules
            .get(dimension.as_str())
            .copied()
            .unwrap_or(ExhaustionRule {
                on_exhaustion: ExhaustionAction::Stop,
                grace_calls: 0,
            })
    }

    /// Validate (`escalate` is admitted for attended runs only — the driver's
    /// attendance mode is checked at `arm`; the policy itself only declares).
    pub fn validate(&self) -> Result<(), PolicyError> {
        for dim in self.rules.keys() {
            if DimensionId::parse(dim).is_none() {
                return Err(PolicyError::UnknownMember {
                    member: "exhaustion.dimension".into(),
                    spelling: dim.clone(),
                });
            }
        }
        Ok(())
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let m: Vec<(String, Json)> = self
            .rules
            .iter()
            .map(|(d, r)| {
                (
                    d.clone(),
                    Json::obj([
                        (
                            "on_exhaustion",
                            Json::str(match r.on_exhaustion {
                                ExhaustionAction::Stop => "stop",
                                ExhaustionAction::Escalate => "escalate",
                            }),
                        ),
                        ("grace_calls", Json::Int(r.grace_calls as i64)),
                    ]),
                )
            })
            .collect();
        Json::Obj(m.into_iter().collect())
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json) -> Result<ExhaustionPolicy, PolicyError> {
        let mut rules = BTreeMap::new();
        let obj = match j {
            Json::Obj(m) => m,
            _ => {
                return Err(PolicyError::BadField {
                    member: "exhaustion".into(),
                })
            }
        };
        for (dim, spec) in obj {
            if DimensionId::parse(dim).is_none() {
                return Err(PolicyError::UnknownMember {
                    member: "exhaustion.dimension".into(),
                    spelling: dim.clone(),
                });
            }
            let action = match str_at(spec, "on_exhaustion")?.as_str() {
                "stop" => ExhaustionAction::Stop,
                "escalate" => ExhaustionAction::Escalate,
                other => {
                    return Err(PolicyError::UnknownMember {
                        member: "on_exhaustion".into(),
                        spelling: other.into(),
                    })
                }
            };
            rules.insert(
                dim.clone(),
                ExhaustionRule {
                    on_exhaustion: action,
                    grace_calls: int_at(spec, "grace_calls")? as u32,
                },
            );
        }
        Ok(ExhaustionPolicy { rules })
    }
}

/// `InvariantSet` — INV-1…9 mandatory; `ext.*` predicates may be added,
/// never removed (§5e.2).
#[derive(Debug, Clone, PartialEq)]
pub struct InvariantSet {
    /// The declared set.
    pub set: BTreeSet<InvariantId>,
}

impl InvariantSet {
    /// The mandatory set (INV-1…9).
    pub fn mandatory() -> InvariantSet {
        InvariantSet {
            set: [
                InvariantId::Inv1,
                InvariantId::Inv2,
                InvariantId::Inv3,
                InvariantId::Inv4,
                InvariantId::Inv5,
                InvariantId::Inv6,
                InvariantId::Inv7,
                InvariantId::Inv8,
                InvariantId::Inv9,
            ]
            .into_iter()
            .collect(),
        }
    }

    /// Canonical JSON (`["inv-1",…,"inv-9","ext.…"]`).
    pub fn to_json(&self) -> Json {
        Json::Arr(self.set.iter().map(|i| Json::str(i.as_str())).collect())
    }

    /// Parse the canonical form (INV-1…9 presence is checked by
    /// `EnvelopePolicy::validate`).
    pub fn from_json(j: &Json) -> Result<InvariantSet, PolicyError> {
        let items = match j {
            Json::Arr(items) => items,
            _ => {
                return Err(PolicyError::BadField {
                    member: "invariants".into(),
                })
            }
        };
        let mut set = BTreeSet::new();
        for i in items {
            let id = InvariantId::parse(i.as_str().unwrap_or("")).ok_or_else(|| {
                PolicyError::UnknownMember {
                    member: "invariants".into(),
                    spelling: i.as_str().unwrap_or("?").into(),
                }
            })?;
            set.insert(id);
        }
        Ok(InvariantSet { set })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// codec helpers
// ─────────────────────────────────────────────────────────────────────────────

fn req<'a>(j: &'a Json, key: &str) -> Result<&'a Json, PolicyError> {
    j.get(key)
        .ok_or_else(|| PolicyError::BadField { member: key.into() })
}

fn str_at(j: &Json, key: &str) -> Result<String, PolicyError> {
    req(j, key)?
        .as_str()
        .map(String::from)
        .ok_or_else(|| PolicyError::BadField { member: key.into() })
}

fn int_at(j: &Json, key: &str) -> Result<i64, PolicyError> {
    req(j, key)?
        .as_int()
        .ok_or_else(|| PolicyError::BadField { member: key.into() })
}

fn bool_at(j: &Json, key: &str) -> Result<bool, PolicyError> {
    match req(j, key)? {
        Json::Bool(b) => Ok(*b),
        _ => Err(PolicyError::BadField { member: key.into() }),
    }
}

fn opt_arr<'a>(j: &'a Json, key: &str) -> Result<&'a [Json], PolicyError> {
    match j.get(key) {
        None | Some(Json::Null) => Ok(&[]),
        Some(Json::Arr(items)) => Ok(items),
        _ => Err(PolicyError::BadField { member: key.into() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage1_default_seals_and_verifies() {
        let p = EnvelopePolicy::stage1_default("budget-root")
            .seal()
            .unwrap();
        assert!(!p.policy_id.is_empty());
        assert!(p.verify_seal());
        // Round-trips through the strict codec.
        let back = EnvelopePolicy::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn a_tampered_policy_fails_verify_seal() {
        let mut p = EnvelopePolicy::stage1_default("budget-root")
            .seal()
            .unwrap();
        p.loop_policy.no_progress.threshold += 1;
        assert!(!p.verify_seal());
    }

    #[test]
    fn missing_inv_member_fails_validation() {
        let mut p = EnvelopePolicy::stage1_default("b");
        p.invariants.set.remove(&InvariantId::Inv3);
        assert!(matches!(
            p.validate(),
            Err(PolicyError::MissingInvariant { .. })
        ));
    }

    #[test]
    fn non_fixed_terminal_is_refused() {
        let mut t = TimeoutPolicy::default();
        t.insert_kind_defaults();
        t.table.get_mut(&ScopeKind::ModelCall).unwrap().on_expiry =
            ScopeTerminal::EffectUnknownTimeout;
        assert!(matches!(
            t.validate(),
            Err(PolicyError::NonFixedTerminal { .. })
        ));
    }

    #[test]
    fn retry_delay_is_bounded_deterministic_and_within_jitter() {
        let spec = RetrySpec {
            max_attempts: MaxAttempts::Bounded(3),
            backoff: Backoff {
                initial_ms: 100,
                factor_ppm: 2 * PPM,
                max_ms: 400,
                jitter_ppm: 500_000,
            },
            honour_retry_after: true,
            attempt_delta_allowed: vec![],
        };
        let d1 = spec.delay_ms(1, "scope/1");
        let d1b = spec.delay_ms(1, "scope/1");
        assert_eq!(d1, d1b); // deterministic
        assert!((100..=150).contains(&d1)); // base + ≤50% jitter
        let d3 = spec.delay_ms(3, "scope/1");
        assert!(d3 <= 400 + 200); // capped + jitter
    }

    #[test]
    fn escalations_outside_the_ladder_are_refused() {
        let mut lp = LoopPolicy {
            response_ladder: vec![LadderAction::Nudge, LadderAction::Stop],
            ..Default::default()
        };
        assert!(lp.validate().is_err());
        lp = LoopPolicy {
            response_ladder: vec![LadderAction::Deny, LadderAction::Nudge, LadderAction::Stop],
            ..Default::default()
        };
        assert!(lp.validate().is_err());
    }

    #[test]
    fn repairs_require_repair_then_strict() {
        let ov = OutputValidationPolicy {
            repairs: vec![RepairKind::JsonRepair],
            ..Default::default()
        };
        assert!(ov.validate().is_err());
        let ov = OutputValidationPolicy {
            mode: ValidationMode::RepairThenStrict,
            repairs: vec![RepairKind::JsonRepair],
            ..Default::default()
        };
        assert!(ov.validate().is_ok());
    }
}
