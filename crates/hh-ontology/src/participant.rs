//! The two participant classes and admissible granularities (spec §2.7; ADR-0001, ADR-0013).
//!
//! The class criterion is **transparency of H** (§2.7.1): `native` ⇔ θ is a Harness Definition
//! known to the Lab (H = compile(h, p) is transparent); `hosted` ⇔ H is opaque. Rules honored
//! here:
//! - **class is declared, never inferred** from mechanism or observability — a raw descriptor
//!   without a class is [`DescribeError::ClassUndeclared`] (§2.7.1 rule 1; ADR-0013 D1; CF-059);
//! - **`ledger ∈ observability_level ⇔ class = native`** is entailed, never declarable (§2.7.2;
//!   CF-059) — a hosted descriptor claiming `ledger` is refused;
//! - [`admissible_granularities`] is **derived** from class and capability vector, never
//!   declared (§2.7.4; ADR-0013 D4); a factor outside the set is [`InadmissibleFactor`] and the
//!   report renders `n/a{class}`, never 0 and never a proxy (T-LCD-15);
//! - **`unknown` is never coerced** in either direction (§2.7.3; T-LCD-07): an `unknown`
//!   capability coordinate does not become a SUPPORTED configuration-level factor.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// The two participant classes (§2.7.1). A third "grey-box" class is rejected — it differs from
/// hosted only in observability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParticipantClass {
    /// θ is a Harness Definition known to the Lab; H is transparent.
    Native,
    /// H is opaque.
    Hosted,
}

impl ParticipantClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ParticipantClass::Native => "native",
            ParticipantClass::Hosted => "hosted",
        }
    }

    /// Parse a canonical spelling; `None` on any other input.
    pub fn parse(s: &str) -> Option<ParticipantClass> {
        match s {
            "native" => Some(ParticipantClass::Native),
            "hosted" => Some(ParticipantClass::Hosted),
            _ => None,
        }
    }
}

/// Comparison granularity — the unit at which a factor varies (§2.7.4; CF-021).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Granularity {
    /// An IR component variant or parameter (native only).
    ComponentLevel,
    /// A non-component coordinate the Hosting ABI lets the Lab vary for any participant.
    ConfigurationLevel,
    /// A participant version as a whole.
    ProductLevel,
}

impl Granularity {
    /// The canonical spelling (the spec's hyphenated form — §2.7.4).
    pub fn as_str(self) -> &'static str {
        match self {
            Granularity::ComponentLevel => "component-level",
            Granularity::ConfigurationLevel => "configuration-level",
            Granularity::ProductLevel => "product-level",
        }
    }

    /// Parse a canonical spelling; `None` on any other input.
    pub fn parse(s: &str) -> Option<Granularity> {
        match s {
            "component-level" => Some(Granularity::ComponentLevel),
            "configuration-level" => Some(Granularity::ConfigurationLevel),
            "product-level" => Some(Granularity::ProductLevel),
            _ => None,
        }
    }
}

/// The observability levels (§2.7.2). `ledger` is entailed by `native`, never declarable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Observability {
    /// Ledger events.
    Events,
    /// Model I/O.
    ModelIo,
    /// End state.
    EndState,
    /// The full run ledger (⇔ native).
    Ledger,
}

impl Observability {
    /// The canonical spelling (spec §2.7.2 — `requires_observability ⊆
    /// {events, model_io, end_state, ledger}`).
    pub fn as_str(self) -> &'static str {
        match self {
            Observability::Events => "events",
            Observability::ModelIo => "model_io",
            Observability::EndState => "end_state",
            Observability::Ledger => "ledger",
        }
    }

    /// Parse a canonical spelling; `None` on any other input.
    pub fn parse(s: &str) -> Option<Observability> {
        match s {
            "events" => Some(Observability::Events),
            "model_io" => Some(Observability::ModelIo),
            "end_state" => Some(Observability::EndState),
            "ledger" => Some(Observability::Ledger),
            _ => None,
        }
    }
}

/// The hosting mechanism (§2.7.2). `none` only on a native descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostingMechanism {
    /// Native — no hosting mechanism.
    None,
    /// Session ABI.
    SessionAbi,
    /// Model-boundary interception.
    ModelBoundaryIntercept,
    /// Container-installed.
    ContainerInstalled,
}

/// A per-dimension capability-vector verdict (§2.7.3). `Unknown` is never coerced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityVerdict {
    /// The coordinate is supported.
    Supported,
    /// Not supported.
    Unsupported,
    /// Partially supported.
    Partial,
    /// Not applicable.
    NotApplicable,
    /// Unknown — never coerced in either direction (T-LCD-07).
    Unknown,
    /// Skipped.
    Skipped,
    /// Declaration/probe disagree (both concrete).
    Drift,
}

/// A raw, as-supplied participant record — `class` may be absent, which `describe` rejects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDescriptor {
    /// The declared class, or `None` (⇒ `ClassUndeclared`).
    pub class: Option<ParticipantClass>,
    /// The hosting mechanism.
    pub hosting_mechanism: HostingMechanism,
    /// The declared observability level.
    pub observability_level: BTreeSet<Observability>,
    /// The reconciled capability vector, per configuration-level coordinate.
    pub capability_vector: BTreeMap<String, CapabilityVerdict>,
}

/// The validated, per-run-immutable participant descriptor (§2.7.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantDescriptor {
    /// The declared class.
    pub class: ParticipantClass,
    /// The hosting mechanism.
    pub hosting_mechanism: HostingMechanism,
    /// The observability level.
    pub observability_level: BTreeSet<Observability>,
    /// The capability vector.
    pub capability_vector: BTreeMap<String, CapabilityVerdict>,
}

/// Errors from [`describe`] (§2.9.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescribeError {
    /// Class was not declared — never inferred from mechanism (ADR-0013 D1; CF-059).
    ClassUndeclared,
    /// `ledger ∈ observability_level` but class is not native — the CF-059 entailment breach.
    LedgerRequiresNative,
    /// `hosting_mechanism = none` on a hosted descriptor, or a mechanism on a native one.
    MechanismClassMismatch,
}

/// Validate a raw descriptor into a [`ParticipantDescriptor`], enforcing the §2.7 rules.
/// `describe(participant) → ParticipantDescriptor`, failing `ClassUndeclared` when class is
/// absent (§2.9.1/§2.9.6).
pub fn describe(raw: RawDescriptor) -> Result<ParticipantDescriptor, DescribeError> {
    let class = raw.class.ok_or(DescribeError::ClassUndeclared)?;

    // ledger ∈ observability_level ⇔ class = native (entailed, never declarable).
    let claims_ledger = raw.observability_level.contains(&Observability::Ledger);
    if claims_ledger && class != ParticipantClass::Native {
        return Err(DescribeError::LedgerRequiresNative);
    }

    // `none` only on a native descriptor; a native descriptor carries `none`.
    let mechanism_is_none = raw.hosting_mechanism == HostingMechanism::None;
    if mechanism_is_none != (class == ParticipantClass::Native) {
        return Err(DescribeError::MechanismClassMismatch);
    }

    Ok(ParticipantDescriptor {
        class,
        hosting_mechanism: raw.hosting_mechanism,
        observability_level: raw.observability_level,
        capability_vector: raw.capability_vector,
    })
}

/// Raised when a factor is registered at a granularity the participant's class does not admit
/// (§2.9.6; ADR-0013 D4). The engine renders `n/a{class}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InadmissibleFactor {
    /// The granularity that was refused.
    pub granularity: Granularity,
    /// The participant class it was refused for.
    pub class: ParticipantClass,
}

/// `admissible_granularities(participant)` — **derived** from class and capability vector, never
/// declared (§2.7.4; ADR-0013 D4). Native admits all three; hosted admits `product-level` plus
/// `configuration-level` **only** where a capability coordinate is `SUPPORTED` — an `unknown`
/// coordinate is never coerced into admitting configuration-level variation (T-LCD-07).
pub fn admissible_granularities(desc: &ParticipantDescriptor) -> BTreeSet<Granularity> {
    let mut out = BTreeSet::new();
    match desc.class {
        ParticipantClass::Native => {
            out.insert(Granularity::ComponentLevel);
            out.insert(Granularity::ConfigurationLevel);
            out.insert(Granularity::ProductLevel);
        }
        ParticipantClass::Hosted => {
            out.insert(Granularity::ProductLevel);
            // configuration-level only if at least one coordinate is concretely SUPPORTED;
            // `unknown` never grants it.
            let any_supported = desc
                .capability_vector
                .values()
                .any(|v| *v == CapabilityVerdict::Supported);
            if any_supported {
                out.insert(Granularity::ConfigurationLevel);
            }
        }
    }
    out
}

/// Admit (or refuse) a factor at `granularity` for this participant (§2.7.4). `Ok(())` when the
/// granularity is admissible; otherwise [`InadmissibleFactor`] (→ `n/a{class}`, never 0).
pub fn admit_factor(
    desc: &ParticipantDescriptor,
    granularity: Granularity,
) -> Result<(), InadmissibleFactor> {
    if admissible_granularities(desc).contains(&granularity) {
        Ok(())
    } else {
        Err(InadmissibleFactor {
            granularity,
            class: desc.class,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(levels: &[Observability]) -> BTreeSet<Observability> {
        levels.iter().copied().collect()
    }

    fn native() -> ParticipantDescriptor {
        describe(RawDescriptor {
            class: Some(ParticipantClass::Native),
            hosting_mechanism: HostingMechanism::None,
            observability_level: obs(&[
                Observability::Events,
                Observability::ModelIo,
                Observability::EndState,
                Observability::Ledger,
            ]),
            capability_vector: BTreeMap::new(),
        })
        .unwrap()
    }

    fn hosted_with(coords: &[(&str, CapabilityVerdict)]) -> ParticipantDescriptor {
        describe(RawDescriptor {
            class: Some(ParticipantClass::Hosted),
            hosting_mechanism: HostingMechanism::SessionAbi,
            observability_level: obs(&[Observability::Events, Observability::ModelIo]),
            capability_vector: coords.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        })
        .unwrap()
    }

    #[test]
    fn class_is_declared_never_inferred() {
        // §2.7.1 rule 1: a descriptor without a class is ClassUndeclared.
        let raw = RawDescriptor {
            class: None,
            hosting_mechanism: HostingMechanism::SessionAbi,
            observability_level: obs(&[Observability::Events]),
            capability_vector: BTreeMap::new(),
        };
        assert_eq!(describe(raw), Err(DescribeError::ClassUndeclared));
    }

    #[test]
    fn ledger_is_entailed_by_native_only() {
        // §2.7.2 / CF-059: a hosted descriptor claiming `ledger` is refused.
        let raw = RawDescriptor {
            class: Some(ParticipantClass::Hosted),
            hosting_mechanism: HostingMechanism::SessionAbi,
            observability_level: obs(&[Observability::Ledger]),
            capability_vector: BTreeMap::new(),
        };
        assert_eq!(describe(raw), Err(DescribeError::LedgerRequiresNative));
    }

    #[test]
    fn hosting_mechanism_none_is_native_only() {
        let raw = RawDescriptor {
            class: Some(ParticipantClass::Hosted),
            hosting_mechanism: HostingMechanism::None,
            observability_level: obs(&[Observability::Events]),
            capability_vector: BTreeMap::new(),
        };
        assert_eq!(describe(raw), Err(DescribeError::MechanismClassMismatch));
    }

    #[test]
    fn native_admits_all_three_granularities() {
        let g = admissible_granularities(&native());
        assert!(g.contains(&Granularity::ComponentLevel));
        assert!(g.contains(&Granularity::ConfigurationLevel));
        assert!(g.contains(&Granularity::ProductLevel));
        assert!(admit_factor(&native(), Granularity::ComponentLevel).is_ok());
    }

    #[test]
    fn hosted_refuses_component_level_factor() {
        // §2.7.4: a component-level factor on a hosted participant is InadmissibleFactor.
        let h = hosted_with(&[("model", CapabilityVerdict::Supported)]);
        let err = admit_factor(&h, Granularity::ComponentLevel).unwrap_err();
        assert_eq!(err.class, ParticipantClass::Hosted);
        assert_eq!(err.granularity, Granularity::ComponentLevel);
    }

    #[test]
    fn unknown_capability_is_never_coerced_to_configuration_level() {
        // §2.7.3 / T-LCD-07: with only `unknown` coordinates, hosted collapses to product-level;
        // `unknown` is not flipped to SUPPORTED to admit configuration-level variation.
        let h = hosted_with(&[("model", CapabilityVerdict::Unknown)]);
        let g = admissible_granularities(&h);
        assert_eq!(g, [Granularity::ProductLevel].into_iter().collect());
        assert!(admit_factor(&h, Granularity::ConfigurationLevel).is_err());
        // And the coordinate itself remains Unknown — not mutated by the derivation.
        assert_eq!(h.capability_vector["model"], CapabilityVerdict::Unknown);
    }

    #[test]
    fn hosted_admits_configuration_level_only_on_supported_coord() {
        let h = hosted_with(&[
            ("model", CapabilityVerdict::Supported),
            ("permissions", CapabilityVerdict::Unknown),
        ]);
        let g = admissible_granularities(&h);
        assert!(g.contains(&Granularity::ConfigurationLevel));
        assert!(g.contains(&Granularity::ProductLevel));
        assert!(!g.contains(&Granularity::ComponentLevel));
    }
}
