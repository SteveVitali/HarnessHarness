//! Validity vs compliance as a measured pair (spec §2.6; ADR-0014).
//!
//! Harness **validity** (the artifact encodes a correct policy/fact) and **compliance** (the
//! beneficiary model notices, activates and follows it) are measured separately and never
//! reported as one number (§2.6). This module lands the C0/Stage-1 *shapes*:
//! - [`validity`] — three-valued, decided by a protocol-layer check that **never consults the
//!   beneficiary model**; structurally model-free (no `M`/configuration input) so the result is
//!   identical across configurations (the S-077 property; §2.6.1);
//! - the three [`ChainEvent`]s (`delivered` → `activated` → `followed`) with `detector`
//!   provenance (§2.6.2) — the *schema* only; emission is Stage 2/3;
//! - [`MetricDeclaration`] carrying `applies_to_classes`, `requires_observability`,
//!   `detector_classes_allowed` and the typed [`NaReason`] never coerced to 0 (§2.6.3; T-LCD-15).

use crate::eval::{
    ChargedTo, Dimension, Direction, IntervalMethod, MediationChannel, MediationRequirement,
    MetricLevel, MetricValueType, OracleClass, OutcomeClassPolicy, ReplicateReducer,
};
use crate::participant::CapabilityVerdict;
use crate::participant::{Granularity, Observability, ParticipantClass, ParticipantDescriptor};
use hh_wire::Json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// The three-valued validity state (§2.6.1). `unknown` when no check exists — never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    /// A check decided valid.
    Valid,
    /// A check decided invalid.
    Invalid,
    /// No check exists (or it could not decide).
    Unknown,
}

/// The protocol-layer check `validity` consults (§2.6.1). None of these consults the beneficiary
/// model — the crux of the S-077 property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidityCheck {
    /// A version stamp comparison — valid iff the stamp is the expected version.
    VersionStamp { stamp: String, expected: String },
    /// An executable validator (`validates` edge) — its precomputed, model-free verdict.
    ExecutableValidator { passed: bool, check_ref: String },
    /// An external oracle verdict.
    ExternalOracle { valid: bool, check_ref: String },
    /// A declared validity interval — whether `at` falls inside it.
    DeclaredInterval { active_at: bool, check_ref: String },
    /// No check declared.
    None,
}

/// The outcome of [`validity`]: `{valid, invalid, unknown, check_ref}` (§2.6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidityOutcome {
    /// The three-valued state.
    pub state: Validity,
    /// The check reference, when one existed.
    pub check_ref: Option<String>,
}

/// `validity(α, at) → {valid, invalid, unknown, check_ref}` (§2.9.1). Decided *only* by the
/// protocol-layer check; there is deliberately **no model or configuration parameter**, so the
/// result is identical across configurations (§2.6.1 invariant; ADR-0014). `at` is the event id
/// the interval check is evaluated at.
pub fn validity(check: &ValidityCheck, _at: &str) -> ValidityOutcome {
    match check {
        ValidityCheck::VersionStamp { stamp, expected } => ValidityOutcome {
            state: if stamp == expected {
                Validity::Valid
            } else {
                Validity::Invalid
            },
            check_ref: Some(format!("version:{expected}")),
        },
        ValidityCheck::ExecutableValidator { passed, check_ref } => ValidityOutcome {
            state: if *passed {
                Validity::Valid
            } else {
                Validity::Invalid
            },
            check_ref: Some(check_ref.clone()),
        },
        ValidityCheck::ExternalOracle { valid, check_ref } => ValidityOutcome {
            state: if *valid {
                Validity::Valid
            } else {
                Validity::Invalid
            },
            check_ref: Some(check_ref.clone()),
        },
        ValidityCheck::DeclaredInterval {
            active_at,
            check_ref,
        } => ValidityOutcome {
            state: if *active_at {
                Validity::Valid
            } else {
                Validity::Invalid
            },
            check_ref: Some(check_ref.clone()),
        },
        ValidityCheck::None => ValidityOutcome {
            state: Validity::Unknown,
            check_ref: None,
        },
    }
}

/// The detector provenance of a compliance signal (§2.6.2; CF-483; ADR-0110). One sum shared
/// with `Verdict.detector`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Detector {
    /// A deterministic detector (schema/predicate, handle-only expansion, retrieval injection …).
    Deterministic,
    /// A judged detector (enters at C2).
    Judged,
    /// A human detector.
    Human,
}

impl Detector {
    /// The full closed set (three detector classes — CF-483).
    pub const ALL: [Detector; 3] = [Detector::Deterministic, Detector::Judged, Detector::Human];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Detector::Deterministic => "deterministic",
            Detector::Judged => "judged",
            Detector::Human => "human",
        }
    }

    /// Parse a canonical spelling; `None` on any other input.
    pub fn parse(s: &str) -> Option<Detector> {
        Detector::ALL.into_iter().find(|d| d.as_str() == s)
    }
}

/// The kinds a harness artifact may take (§2.6.2). `activation_observable` is per-kind
/// (§2.9.3 Stage-1 scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    /// An instruction (prose instructions collapse the chain to delivered → followed).
    Instruction,
    /// A rule.
    Rule,
    /// A memory.
    Memory,
    /// A tool surface.
    ToolSurface,
    /// A procedure.
    Procedure,
    /// An observation.
    Observation,
}

impl ArtifactKind {
    /// Whether this kind carries an activation signal (§2.6.2). Kinds without one collapse the
    /// chain to delivered → followed.
    pub fn activation_observable(self) -> bool {
        matches!(
            self,
            ArtifactKind::Rule
                | ArtifactKind::Memory
                | ArtifactKind::ToolSurface
                | ArtifactKind::Procedure
        )
    }
}

/// The three chain events, schema-only (§2.6.2). Home planes: delivered/activated on P1,
/// followed on P4. `delivered` carries no detector (it is a P1 emission); `activated` and
/// `followed` carry detector provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainEvent {
    /// `context.artefact.delivered` (P1) — one per artifact per model call; needs a compiled id.
    Delivered {
        /// The artifact's content-addressed semantic id.
        artefact_id: String,
        /// The per-delivery id.
        delivery_id: String,
    },
    /// `context.artefact.activated` (P1) — never emitted when `activation_observable = false`.
    Activated {
        /// The artifact id.
        artefact_id: String,
        /// The delivery id.
        delivery_id: String,
        /// The detector provenance.
        detector: Detector,
    },
    /// `verification.artefact.followed` (P4) — deterministic only via a referenced validator.
    Followed {
        /// The artifact id.
        artefact_id: String,
        /// The delivery id.
        delivery_id: String,
        /// The detector provenance.
        detector: Detector,
    },
}

impl ChainEvent {
    /// The event identifier (`plane.noun.verb`), whose prefix is the home plane.
    pub fn identifier(&self) -> &'static str {
        match self {
            ChainEvent::Delivered { .. } => "context.artefact.delivered",
            ChainEvent::Activated { .. } => "context.artefact.activated",
            ChainEvent::Followed { .. } => "verification.artefact.followed",
        }
    }

    /// The detector, when the event carries one (`delivered` does not).
    pub fn detector(&self) -> Option<Detector> {
        match self {
            ChainEvent::Delivered { .. } => None,
            ChainEvent::Activated { detector, .. } | ChainEvent::Followed { detector, .. } => {
                Some(*detector)
            }
        }
    }
}

/// The typed not-applicable reason (§2.6.3; T-LCD-15). Never 0, never a proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaReason {
    /// The metric does not apply to the participant's class.
    Class,
    /// The required observability is unavailable.
    Observability,
    /// A required capability is absent.
    Capability,
    /// The required mediation is unavailable.
    Mediation,
    /// The estimator is undefined for this cell.
    EstimatorUndefined,
    /// The stage was not run.
    NotRun,
    /// No detector exists.
    NoDetector,
}

impl NaReason {
    /// The full closed set (spec §5h.2 `n/a{reason}`).
    pub const ALL: [NaReason; 7] = [
        NaReason::Class,
        NaReason::Observability,
        NaReason::Capability,
        NaReason::Mediation,
        NaReason::EstimatorUndefined,
        NaReason::NotRun,
        NaReason::NoDetector,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            NaReason::Class => "class",
            NaReason::Observability => "observability",
            NaReason::Capability => "capability",
            NaReason::Mediation => "mediation",
            NaReason::EstimatorUndefined => "estimator_undefined",
            NaReason::NotRun => "not_run",
            NaReason::NoDetector => "no_detector",
        }
    }

    /// Parse a canonical spelling; `None` on any other input.
    pub fn parse(s: &str) -> Option<NaReason> {
        NaReason::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

/// `MetricDeclaration` — the **single complete form** (§5h.2 §3; ADR-0045 D1
/// as amended — the partial forms are superseded, CF-094):
/// `{name, dimension, level, value_type, direction, unit,
/// requires_observability, applies_to_classes, applies_to_families,
/// requires_capabilities, requires_mediation, admissible_granularities,
/// detector_classes_allowed, oracle_classes_allowed, replicate_reducer,
/// interval_method, outcome_class_policy, headline, veto, charged_to}`.
/// Adding a metric is a registry entry; adding a dimension is an ADR.
/// Applicability = class ∧ observability ∧ capabilities ∧ mediation ∧ family;
/// an inapplicable cell renders the typed `n/a{reason}` — never 0, never a
/// proxy (T-LCD-15).
#[derive(Debug, Clone, PartialEq)]
pub struct MetricDeclaration {
    /// The metric name.
    pub name: String,
    /// The scorecard dimension the metric rolls up to.
    pub dimension: Dimension,
    /// The aggregation-ladder level the metric reads at.
    pub level: MetricLevel,
    /// The declared value type.
    pub value_type: MetricValueType,
    /// Whether a higher or lower reading is better.
    pub direction: Direction,
    /// The unit the value is expressed in.
    pub unit: String,
    /// The observability this metric requires.
    pub requires_observability: BTreeSet<Observability>,
    /// The classes this metric applies to.
    pub applies_to_classes: BTreeSet<ParticipantClass>,
    /// The `environment_family` registry refs the metric applies to
    /// (registry-level scoping — OQ-344/CF-326; `∅` = all families).
    pub applies_to_families: BTreeSet<String>,
    /// The capability names this metric requires (default `∅`; ADR-0165 D6).
    pub requires_capabilities: BTreeSet<String>,
    /// The mediation this metric requires (default `any`; ADR-0165 D6).
    pub requires_mediation: MediationRequirement,
    /// The comparison granularities the metric is admissible at.
    pub admissible_granularities: BTreeSet<Granularity>,
    /// The detector classes allowed (CF-483 — one sum with the chain events
    /// and `Verdict.detector`).
    pub detector_classes_allowed: BTreeSet<Detector>,
    /// The oracle classes allowed to produce this metric's values
    /// (ADR-0047 D1 — bounds the metric's inputs).
    pub oracle_classes_allowed: BTreeSet<OracleClass>,
    /// The replicate-axis reducer.
    pub replicate_reducer: ReplicateReducer,
    /// The interval method (ADR-0158's selection rule governs `clt`).
    pub interval_method: IntervalMethod,
    /// The per-outcome-class denominator policy.
    pub outcome_class_policy: OutcomeClassPolicy,
    /// Whether the metric is a headline metric (C0 headline admits only
    /// deterministic oracle classes — AC-R-2.9.2-13).
    pub headline: bool,
    /// Whether the metric is a veto invariant (a tripped veto yields
    /// success-with-veto — excluded from headline success, counted beside;
    /// ADR-0045 D6).
    pub veto: bool,
    /// Who the metric's spend is charged to (`subject | instrument`, CF-109 —
    /// the renamed `cost_attribution`).
    pub charged_to: ChargedTo,
}

/// `MetricDeclaration` validation failures (typed — never a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum MetricError {
    /// `name` is empty.
    EmptyName,
    /// `applies_to_classes` is empty — a metric applying to no class is malformed.
    NoClasses,
    /// `detector_classes_allowed` is empty — no detector may produce the metric.
    NoDetectors,
    /// `oracle_classes_allowed` is empty — the declaration bounds its inputs;
    /// admitting no oracle admits no values (ADR-0047 D1).
    NoOracleClasses,
    /// A `headline` metric admits a non-deterministic oracle class (a judged
    /// value never enters the headline — AC-R-2.9.2-13; ADR-0047 D2).
    HeadlineAdmitsNonDeterministic {
        /// The offending oracle class.
        class: OracleClass,
    },
    /// `interval_method = clt` on `unit ∈ {tokens, money, ms}` (ADR-0158's
    /// rule — heavy-tailed units never take a CLT interval).
    CltForbiddenUnit,
}

impl Default for MetricDeclaration {
    /// The Stage-1 *partial* form's implied values (CF-094 — the superseded
    /// partial declarations become the full form with these defaults):
    /// `dimension = capability`, `level = run`, `value_type = decimal`,
    /// `direction = higher`, `unit = count`, all requirement sets empty,
    /// `requires_mediation = any`, every granularity admissible, the
    /// deterministic detector + C0-headline oracle classes admitted,
    /// `replicate_reducer = mean`, `interval_method = clustered_clt(task)`,
    /// the capability outcome-class policy, `headline = veto = false`,
    /// `charged_to = subject`.
    fn default() -> MetricDeclaration {
        MetricDeclaration {
            name: String::new(),
            dimension: Dimension::Capability,
            level: MetricLevel::Run,
            value_type: MetricValueType::Decimal,
            direction: Direction::Higher,
            unit: "count".into(),
            requires_observability: BTreeSet::new(),
            applies_to_classes: [ParticipantClass::Native, ParticipantClass::Hosted]
                .into_iter()
                .collect(),
            applies_to_families: BTreeSet::new(),
            requires_capabilities: BTreeSet::new(),
            requires_mediation: MediationRequirement::Any,
            admissible_granularities: [
                Granularity::ComponentLevel,
                Granularity::ConfigurationLevel,
                Granularity::ProductLevel,
            ]
            .into_iter()
            .collect(),
            detector_classes_allowed: [Detector::Deterministic].into_iter().collect(),
            oracle_classes_allowed: [
                OracleClass::Executable,
                OracleClass::EndState,
                OracleClass::OutputCheck,
                OracleClass::TracePredicate,
                OracleClass::ProtocolCheck,
            ]
            .into_iter()
            .collect(),
            replicate_reducer: ReplicateReducer::Mean,
            interval_method: IntervalMethod::ClusteredClt,
            outcome_class_policy: OutcomeClassPolicy::for_capability(),
            headline: false,
            veto: false,
            charged_to: ChargedTo::Subject,
        }
    }
}

impl MetricDeclaration {
    /// The Stage-1 schema checks: nonempty name/classes/detectors/oracles;
    /// `headline ⇒ oracle_classes_allowed ⊆` the C0 deterministic set;
    /// `clt` never on a heavy-tailed unit (ADR-0158).
    pub fn validate(&self) -> Result<(), MetricError> {
        if self.name.is_empty() {
            return Err(MetricError::EmptyName);
        }
        if self.applies_to_classes.is_empty() {
            return Err(MetricError::NoClasses);
        }
        if self.detector_classes_allowed.is_empty() {
            return Err(MetricError::NoDetectors);
        }
        if self.oracle_classes_allowed.is_empty() {
            return Err(MetricError::NoOracleClasses);
        }
        if self.headline {
            for c in &self.oracle_classes_allowed {
                if !c.is_c0_headline() {
                    return Err(MetricError::HeadlineAdmitsNonDeterministic { class: *c });
                }
            }
        }
        if self.interval_method == IntervalMethod::Clt
            && matches!(self.unit.as_str(), "tokens" | "money" | "ms")
        {
            return Err(MetricError::CltForbiddenUnit);
        }
        Ok(())
    }

    /// Whether the metric is admissible at `g` (`admissible_granularities`
    /// bounds the comparisons it may appear in — §2.7.4).
    pub fn admits_granularity(&self, g: Granularity) -> bool {
        self.admissible_granularities.contains(&g)
    }

    /// Applicability = class ∧ observability ∧ capabilities (§2.6.3 +
    /// ADR-0165 D6). Returns the typed [`NaReason`] on an inadmissible cell —
    /// never 0, never a proxy (T-LCD-15). A required capability is satisfied
    /// only by a `Supported` verdict — `Unknown`/absent is `n/a{capability}`,
    /// never coerced (T-LCD-07).
    pub fn applicability(&self, desc: &ParticipantDescriptor) -> Result<(), NaReason> {
        if !self.applies_to_classes.contains(&desc.class) {
            return Err(NaReason::Class);
        }
        if !self
            .requires_observability
            .is_subset(&desc.observability_level)
        {
            return Err(NaReason::Observability);
        }
        for cap in &self.requires_capabilities {
            match desc.capability_vector.get(cap) {
                Some(CapabilityVerdict::Supported) => {}
                _ => return Err(NaReason::Capability),
            }
        }
        Ok(())
    }

    /// The full applicability — class ∧ observability ∧ capabilities ∧
    /// mediation ∧ family (ADR-0165 D6; `applies_to_families` empty = all
    /// families).
    pub fn applicability_at(
        &self,
        desc: &ParticipantDescriptor,
        mediated: &BTreeSet<MediationChannel>,
        environment_family: Option<&str>,
    ) -> Result<(), NaReason> {
        self.applicability(desc)?;
        if !self.requires_mediation.satisfied_by(mediated) {
            return Err(NaReason::Mediation);
        }
        if let Some(f) = environment_family {
            if !self.applies_to_families.is_empty() && !self.applies_to_families.contains(f) {
                return Err(NaReason::Class);
            }
        }
        Ok(())
    }

    /// The canonical JSON form (the `metric_declaration` registry body).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("name".into(), Json::str(&self.name));
        m.insert("dimension".into(), Json::str(self.dimension.as_str()));
        m.insert("level".into(), Json::str(self.level.as_str()));
        m.insert("value_type".into(), Json::str(self.value_type.as_str()));
        m.insert("direction".into(), Json::str(self.direction.as_str()));
        m.insert("unit".into(), Json::str(&self.unit));
        m.insert(
            "requires_observability".into(),
            Json::Arr(
                self.requires_observability
                    .iter()
                    .map(|o| Json::str(o.as_str()))
                    .collect(),
            ),
        );
        m.insert(
            "applies_to_classes".into(),
            Json::Arr(
                self.applies_to_classes
                    .iter()
                    .map(|c| Json::str(c.as_str()))
                    .collect(),
            ),
        );
        m.insert(
            "applies_to_families".into(),
            Json::Arr(self.applies_to_families.iter().map(Json::str).collect()),
        );
        m.insert(
            "requires_capabilities".into(),
            Json::Arr(self.requires_capabilities.iter().map(Json::str).collect()),
        );
        m.insert(
            "requires_mediation".into(),
            self.requires_mediation.to_json(),
        );
        m.insert(
            "admissible_granularities".into(),
            Json::Arr(
                self.admissible_granularities
                    .iter()
                    .map(|g| Json::str(g.as_str()))
                    .collect(),
            ),
        );
        m.insert(
            "detector_classes_allowed".into(),
            Json::Arr(
                self.detector_classes_allowed
                    .iter()
                    .map(|d| Json::str(d.as_str()))
                    .collect(),
            ),
        );
        m.insert(
            "oracle_classes_allowed".into(),
            Json::Arr(
                self.oracle_classes_allowed
                    .iter()
                    .map(|o| Json::str(o.as_str()))
                    .collect(),
            ),
        );
        m.insert("replicate_reducer".into(), self.replicate_reducer.to_json());
        m.insert("interval_method".into(), self.interval_method.to_json());
        m.insert(
            "outcome_class_policy".into(),
            self.outcome_class_policy.to_json(),
        );
        m.insert("headline".into(), Json::Bool(self.headline));
        m.insert("veto".into(), Json::Bool(self.veto));
        m.insert("charged_to".into(), Json::str(self.charged_to.as_str()));
        Json::Obj(m)
    }

    /// Strict decode — `Err` on missing/unknown members or spellings.
    pub fn from_json(j: &Json) -> Result<MetricDeclaration, String> {
        const REC: &str = "MetricDeclaration";
        let m = match j {
            Json::Obj(m) => m,
            _ => return Err(format!("{REC} must be an object")),
        };
        let allowed: BTreeSet<&str> = [
            "name",
            "dimension",
            "level",
            "value_type",
            "direction",
            "unit",
            "requires_observability",
            "applies_to_classes",
            "applies_to_families",
            "requires_capabilities",
            "requires_mediation",
            "admissible_granularities",
            "detector_classes_allowed",
            "oracle_classes_allowed",
            "replicate_reducer",
            "interval_method",
            "outcome_class_policy",
            "headline",
            "veto",
            "charged_to",
        ]
        .into_iter()
        .collect();
        for k in m.keys() {
            if !allowed.contains(k.as_str()) {
                return Err(format!("unknown member {k} of {REC}"));
            }
        }
        let str_at = |k: &str| -> Result<&str, String> {
            m.get(k)
                .and_then(Json::as_str)
                .ok_or_else(|| format!("{REC}.{k} must be a string"))
        };
        let bool_at = |k: &str| -> Result<bool, String> {
            match m.get(k) {
                Some(Json::Bool(b)) => Ok(*b),
                _ => Err(format!("{REC}.{k} must be a bool")),
            }
        };
        fn set_at<T: Ord>(
            m: &BTreeMap<String, Json>,
            k: &str,
            rec: &str,
            parse: impl Fn(&str) -> Option<T>,
        ) -> Result<BTreeSet<T>, String> {
            match m.get(k) {
                Some(Json::Arr(a)) => {
                    let mut out = BTreeSet::new();
                    for v in a {
                        let s = v
                            .as_str()
                            .ok_or_else(|| format!("{rec}.{k} members must be strings"))?;
                        out.insert(
                            parse(s).ok_or_else(|| format!("unknown {rec}.{k} member {s}"))?,
                        );
                    }
                    Ok(out)
                }
                _ => Err(format!("{rec}.{k} must be an array")),
            }
        }
        Ok(MetricDeclaration {
            name: str_at("name")?.to_string(),
            dimension: Dimension::parse(str_at("dimension")?)
                .ok_or_else(|| format!("unknown {REC}.dimension"))?,
            level: MetricLevel::parse(str_at("level")?)
                .ok_or_else(|| format!("unknown {REC}.level"))?,
            value_type: MetricValueType::parse(str_at("value_type")?)
                .ok_or_else(|| format!("unknown {REC}.value_type"))?,
            direction: Direction::parse(str_at("direction")?)
                .ok_or_else(|| format!("unknown {REC}.direction"))?,
            unit: str_at("unit")?.to_string(),
            requires_observability: set_at(m, "requires_observability", REC, Observability::parse)?,
            applies_to_classes: set_at(m, "applies_to_classes", REC, ParticipantClass::parse)?,
            applies_to_families: set_at(m, "applies_to_families", REC, |s| Some(s.to_string()))?,
            requires_capabilities: set_at(m, "requires_capabilities", REC, |s| {
                Some(s.to_string())
            })?,
            requires_mediation: MediationRequirement::from_json(
                m.get("requires_mediation")
                    .ok_or_else(|| format!("missing {REC}.requires_mediation"))?,
            )
            .ok_or_else(|| format!("bad {REC}.requires_mediation"))?,
            admissible_granularities: set_at(
                m,
                "admissible_granularities",
                REC,
                Granularity::parse,
            )?,
            detector_classes_allowed: set_at(m, "detector_classes_allowed", REC, Detector::parse)?,
            oracle_classes_allowed: set_at(m, "oracle_classes_allowed", REC, OracleClass::parse)?,
            replicate_reducer: ReplicateReducer::from_json(
                m.get("replicate_reducer")
                    .ok_or_else(|| format!("missing {REC}.replicate_reducer"))?,
            )
            .ok_or_else(|| format!("bad {REC}.replicate_reducer"))?,
            interval_method: IntervalMethod::from_json(
                m.get("interval_method")
                    .ok_or_else(|| format!("missing {REC}.interval_method"))?,
            )
            .ok_or_else(|| format!("bad {REC}.interval_method"))?,
            outcome_class_policy: OutcomeClassPolicy::from_json(
                m.get("outcome_class_policy")
                    .ok_or_else(|| format!("missing {REC}.outcome_class_policy"))?,
            )
            .map_err(|e| format!("bad {REC}.outcome_class_policy: {e:?}"))?,
            headline: bool_at("headline")?,
            veto: bool_at("veto")?,
            charged_to: ChargedTo::parse(str_at("charged_to")?)
                .ok_or_else(|| format!("unknown {REC}.charged_to"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validity_is_three_valued_and_model_free() {
        // §2.6.1: a version stamp decides valid/invalid; no check ⇒ unknown; no model input.
        let ok = validity(
            &ValidityCheck::VersionStamp {
                stamp: "v3".into(),
                expected: "v3".into(),
            },
            "evt:1",
        );
        assert_eq!(ok.state, Validity::Valid);
        let bad = validity(
            &ValidityCheck::VersionStamp {
                stamp: "v2".into(),
                expected: "v3".into(),
            },
            "evt:1",
        );
        assert_eq!(bad.state, Validity::Invalid);
        let none = validity(&ValidityCheck::None, "evt:1");
        assert_eq!(none.state, Validity::Unknown);
        assert_eq!(none.check_ref, None);
    }

    #[test]
    fn validity_is_identical_across_configurations() {
        // S-077 property: the same artifact version + check yields the same result no matter the
        // configuration/model. Structurally guaranteed (no M/κ param); asserted for two "runs".
        let check = ValidityCheck::ExecutableValidator {
            passed: true,
            check_ref: "chk:schema-1".into(),
        };
        let run_a = validity(&check, "evt:a");
        let run_b = validity(&check, "evt:b");
        assert_eq!(run_a, run_b);
    }

    #[test]
    fn chain_has_three_events_with_detector_provenance() {
        // AC-A2-3 (T-LCD-13): the taxonomy contains the three chain events; activated/followed
        // carry a detector; delivered does not; the home-plane prefixes are P1/P1/P4.
        let d = ChainEvent::Delivered {
            artefact_id: "a".into(),
            delivery_id: "d".into(),
        };
        let a = ChainEvent::Activated {
            artefact_id: "a".into(),
            delivery_id: "d".into(),
            detector: Detector::Deterministic,
        };
        let f = ChainEvent::Followed {
            artefact_id: "a".into(),
            delivery_id: "d".into(),
            detector: Detector::Deterministic,
        };
        assert_eq!(d.identifier(), "context.artefact.delivered");
        assert_eq!(a.identifier(), "context.artefact.activated");
        assert_eq!(f.identifier(), "verification.artefact.followed");
        assert_eq!(d.detector(), None);
        assert_eq!(a.detector(), Some(Detector::Deterministic));
        assert_eq!(f.detector(), Some(Detector::Deterministic));
    }

    #[test]
    fn activation_observable_is_per_kind() {
        assert!(ArtifactKind::ToolSurface.activation_observable());
        assert!(ArtifactKind::Procedure.activation_observable());
        // Prose instructions/observations have no activation signal (collapse the chain).
        assert!(!ArtifactKind::Instruction.activation_observable());
        assert!(!ArtifactKind::Observation.activation_observable());
    }

    #[test]
    fn metric_applicability_returns_typed_na_never_zero() {
        // AC-A2-4 (T-LCD-15): a metric carries applies_to_classes + requires_observability and
        // renders a typed n/a on an inadmissible cell.
        use crate::participant::{describe, HostingMechanism, RawDescriptor};
        let native_only = MetricDeclaration {
            name: "compliance.rate".into(),
            applies_to_classes: [ParticipantClass::Native].into_iter().collect(),
            requires_observability: [Observability::Events].into_iter().collect(),
            detector_classes_allowed: [Detector::Deterministic].into_iter().collect(),
            ..MetricDeclaration::default()
        };
        let hosted = describe(RawDescriptor {
            class: Some(ParticipantClass::Hosted),
            hosting_mechanism: HostingMechanism::SessionAbi,
            observability_level: [Observability::Events].into_iter().collect(),
            capability_vector: Default::default(),
        })
        .unwrap();
        assert_eq!(native_only.applicability(&hosted), Err(NaReason::Class));

        let events_metric = MetricDeclaration {
            requires_observability: [Observability::ModelIo].into_iter().collect(),
            applies_to_classes: [ParticipantClass::Hosted].into_iter().collect(),
            ..native_only.clone()
        };
        assert_eq!(
            events_metric.applicability(&hosted),
            Err(NaReason::Observability)
        );
    }
}
