//! The **effect risk class** vocabulary (§5a.2 R-2.2.2; ADR-0031 §1–3).
//!
//! A `RiskClass` is the runtime record `{reversibility, repeat_safety, scope}` — a
//! **projection** of a capability's declared `EffectAttributes`, never a second
//! declaration (ADR-0031 §1; CF-104):
//!
//! - `reversibility' = read_only if mutability = read_only else reversibility` — the
//!   HIR `reversible(Ref<Procedure>)` constructor projects to the bare `reversible`
//!   value (the procedure ref stays on the HIR declaration);
//! - `scope = workspace_local if world = closed else external`;
//! - `repeat_safety` verbatim — which is why [`RepeatSafety`] lives here (one sum,
//!   one spelling — CC1/CC7): `hh-hir`'s `EffectAttributes` field is this type.
//!
//! `effective_risk_class = max_by_danger(kernel_assessed, declared)` — componentwise
//! maximum over the danger orders below (ADR-0031 §2). An absent/unparseable declared
//! class projects to [`RiskClass::UNKNOWN`] — the most dangerous point —
//! `unknown ⇒ {irreversible, non_idempotent, external}` (ADR-0031 §2; the
//! ADR-0212/OQ-223 interim rule: both spellings of "no domain declared" project to the
//! most dangerous class).
//!
//! The class table consumed by the lifecycle and by recovery (ADR-0031 §3):
//!
//! | reversibility | dispatch | on `unknown` | undo |
//! |---|---|---|---|
//! | `read_only` | at-least-once; free retry; no write-ahead | redispatch | none |
//! | `reversible` | at-least-once; effectively-once by revert-and-redo | `diff(baseline)` probe; else revert + redispatch | mechanical revert |
//! | `compensable` | at-most-once + mandatory idempotency key | probe first; `not_applied` ⇒ same-key retry; `undeterminable` ⇒ escalate | compensator (saga) |
//! | `irreversible` | at-most-once; never auto-retried | never redispatched; escalated | none — `abandoned` or human |
//!
//! The types are pure vocabulary — `hh-ledger` validates payloads against them,
//! `hh-budget` reads `reversibility` for the E1 `read_only` exemption, `hh-hir`
//! projects declarations into them.

use std::fmt;

use hh_wire::json::Json;

/// `repeat_safety ∈ {idempotent, non_idempotent}` — the one sum shared by the HIR
/// `EffectAttributes` declaration and the risk-class projection (ADR-0031 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RepeatSafety {
    /// Safe to repeat — redispatch from `unknown` is legal without a probe.
    Idempotent,
    /// Not safe to repeat — redispatch only after `probe = not_applied`.
    NonIdempotent,
}

impl RepeatSafety {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            RepeatSafety::Idempotent => "idempotent",
            RepeatSafety::NonIdempotent => "non_idempotent",
        }
    }

    /// Parse the canonical name.
    pub fn parse(s: &str) -> Option<RepeatSafety> {
        match s {
            "idempotent" => Some(RepeatSafety::Idempotent),
            "non_idempotent" => Some(RepeatSafety::NonIdempotent),
            _ => None,
        }
    }
}

/// The risk-class reversibility axis `∈ {read_only, reversible, compensable,
/// irreversible}` — **danger-ordered** (declaration order is the order): a model
/// self-report or hint may raise, never lower (ADR-0031 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskReversibility {
    /// Reads only — at-least-once, free retry, no write-ahead.
    ReadOnly,
    /// Workspace-local writes revertible from a baseline.
    Reversible,
    /// Externally visible but compensable by a registered plan.
    Compensable,
    /// Cannot be undone — at-most-once, never auto-retried.
    Irreversible,
}

impl RiskReversibility {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            RiskReversibility::ReadOnly => "read_only",
            RiskReversibility::Reversible => "reversible",
            RiskReversibility::Compensable => "compensable",
            RiskReversibility::Irreversible => "irreversible",
        }
    }

    /// Parse the canonical name.
    pub fn parse(s: &str) -> Option<RiskReversibility> {
        match s {
            "read_only" => Some(RiskReversibility::ReadOnly),
            "reversible" => Some(RiskReversibility::Reversible),
            "compensable" => Some(RiskReversibility::Compensable),
            "irreversible" => Some(RiskReversibility::Irreversible),
            _ => None,
        }
    }
}

/// The risk-class scope axis `∈ {workspace_local, external}` — `external` raises the
/// approval default one step (ADR-0031 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskScope {
    /// Contained inside the declared writable roots.
    WorkspaceLocal,
    /// Reaches outside the workspace (raises approval default).
    External,
}

impl RiskScope {
    /// Canonical name.
    pub fn name(self) -> &'static str {
        match self {
            RiskScope::WorkspaceLocal => "workspace_local",
            RiskScope::External => "external",
        }
    }

    /// Parse the canonical name.
    pub fn parse(s: &str) -> Option<RiskScope> {
        match s {
            "workspace_local" => Some(RiskScope::WorkspaceLocal),
            "external" => Some(RiskScope::External),
            _ => None,
        }
    }
}

/// `RiskClass{reversibility, repeat_safety, scope}` — the runtime record
/// (§5a.2 §3; ADR-0031 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RiskClass {
    /// Effective reversibility — drives the delivery table.
    pub reversibility: RiskReversibility,
    /// Repeat safety — verbatim from the declaration.
    pub repeat_safety: RepeatSafety,
    /// Workspace-local or external reach.
    pub scope: RiskScope,
}

impl RiskClass {
    /// `unknown ⇒ {irreversible, non_idempotent, external}` — the most dangerous
    /// class, assigned to undeclared/unparseable declarations (ADR-0031 §2).
    pub const UNKNOWN: RiskClass = RiskClass {
        reversibility: RiskReversibility::Irreversible,
        repeat_safety: RepeatSafety::NonIdempotent,
        scope: RiskScope::External,
    };

    /// The least-dangerous class.
    pub const READ_ONLY: RiskClass = RiskClass {
        reversibility: RiskReversibility::ReadOnly,
        repeat_safety: RepeatSafety::Idempotent,
        scope: RiskScope::WorkspaceLocal,
    };

    /// `max_by_danger` — the componentwise maximum (ADR-0031 §2): `reversibility` and
    /// `scope` take the more dangerous variant; `repeat_safety` takes
    /// `non_idempotent` over `idempotent`. Monotone — a lower claim never wins.
    pub fn max_by_danger(a: RiskClass, b: RiskClass) -> RiskClass {
        RiskClass {
            reversibility: a.reversibility.max(b.reversibility),
            repeat_safety: a.repeat_safety.max(b.repeat_safety),
            scope: a.scope.max(b.scope),
        }
    }

    /// `self ≤ other` under the danger order (the `effective ≥ declared` check).
    pub fn leq_danger(&self, other: &RiskClass) -> bool {
        self.reversibility <= other.reversibility
            && self.repeat_safety <= other.repeat_safety
            && self.scope <= other.scope
    }

    /// Is the effective reversibility `read_only` (the E1/DF-S1.6-1 exemption key and
    /// the write-ahead carve-out)?
    pub fn is_read_only(&self) -> bool {
        self.reversibility == RiskReversibility::ReadOnly
    }

    /// The canonical JSON form `{reversibility, repeat_safety, scope}` — the
    /// `intended`/`decided` payload member spelling.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("reversibility", Json::str(self.reversibility.name())),
            ("repeat_safety", Json::str(self.repeat_safety.name())),
            ("scope", Json::str(self.scope.name())),
        ])
    }

    /// Parse the canonical JSON form — `None` on any bad member (never coerced).
    pub fn from_json(j: &Json) -> Option<RiskClass> {
        Some(RiskClass {
            reversibility: RiskReversibility::parse(j.get("reversibility")?.as_str()?)?,
            repeat_safety: RepeatSafety::parse(j.get("repeat_safety")?.as_str()?)?,
            scope: RiskScope::parse(j.get("scope")?.as_str()?)?,
        })
    }
}

impl fmt::Display for RiskClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{{},{},{}}}",
            self.reversibility.name(),
            self.repeat_safety.name(),
            self.scope.name()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_is_the_most_dangerous_point() {
        for rev in [
            RiskReversibility::ReadOnly,
            RiskReversibility::Reversible,
            RiskReversibility::Compensable,
            RiskReversibility::Irreversible,
        ] {
            for rs in [RepeatSafety::Idempotent, RepeatSafety::NonIdempotent] {
                for sc in [RiskScope::WorkspaceLocal, RiskScope::External] {
                    let c = RiskClass {
                        reversibility: rev,
                        repeat_safety: rs,
                        scope: sc,
                    };
                    assert!(c.leq_danger(&RiskClass::UNKNOWN));
                    assert_eq!(
                        RiskClass::max_by_danger(c, RiskClass::UNKNOWN),
                        RiskClass::UNKNOWN
                    );
                }
            }
        }
    }

    #[test]
    fn max_by_danger_is_componentwise_and_monotone() {
        let low = RiskClass {
            reversibility: RiskReversibility::ReadOnly,
            repeat_safety: RepeatSafety::Idempotent,
            scope: RiskScope::WorkspaceLocal,
        };
        let high = RiskClass {
            reversibility: RiskReversibility::Compensable,
            repeat_safety: RepeatSafety::NonIdempotent,
            scope: RiskScope::External,
        };
        assert_eq!(RiskClass::max_by_danger(low, high), high);
        assert_eq!(RiskClass::max_by_danger(high, low), high);
        assert!(!high.leq_danger(&low));
        assert!(low.leq_danger(&high));
    }

    #[test]
    fn json_round_trips_and_refuses_unknown_members() {
        let c = RiskClass::UNKNOWN;
        let back = RiskClass::from_json(&c.to_json()).unwrap();
        assert_eq!(back, c);
        assert_eq!(RiskClass::from_json(&Json::Null), None);
        assert_eq!(
            RiskClass::from_json(&Json::obj([
                ("reversibility", Json::str("safe")),
                ("repeat_safety", Json::str("idempotent")),
                ("scope", Json::str("external")),
            ])),
            None
        );
    }
}
