//! [`AuthorityClass`] — the closed seven-class total order — plus [`TaintTag`],
//! [`ReaderSet`] and [`PersistenceScope`] (§8.1 #3; ADR-0033 D1/D2/D8).
//!
//! `AuthorityClass` answers "how much may this item command the harness". It is **conferred** —
//! derived by the runtime minting table ([`crate::origin::default_authority`]) or a closed-list
//! endorsement — never read from a payload's own claim and never widened outside
//! `endorse`/`declassify` (CC2 anchors here). Command authority is never evidence weight
//! (the §05f/§02 *evidence class* is orthogonal — CF-080).

use std::collections::BTreeSet;

/// The closed total order **`kernel > definition > principal > delegate > environment >
/// external > unverified`** (§8.1 #3; ADR-0033 D2). `Ord` follows that order, so `Kernel` is the
/// maximum and `Unverified` the minimum — `a > b` reads "a may command more than b".
///
/// The order is closed: adding or collapsing a class is an **HIR dialect bump with migration**
/// (ADR-0033 D7), never an ad-hoc value. `imported` is **never a class value** (CF-082) —
/// [`AuthorityClass::parse`] refuses it.
///
/// Minting (§8.1 #3, the minting table — exhaustive; nothing else confers a class, D2/D6):
/// `kernel` only by the reference runtime (ledger facts, monitor decisions, deterministic
/// validator verdicts); `definition` only at `seal` or by a `pin` endorsement; `principal` only
/// for the run's human principal; `delegate` for outputs of any AgentProcess; `environment` only
/// for closed-schema structured values from closed-world tools declared in the sealed
/// definition; `external` for open-world content and **all free text** not from
/// kernel/definition/principal (R-TEXT); `unverified` for imported, lifted, migrated or
/// attestation-failed content and hosted content the Hosting ABI cannot vouch for — a distinct
/// bottom, never coerced into `external` (T-LCD-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AuthorityClass {
    /// The bottom: imported, lifted, migrated or attestation-failed content; hosted content the
    /// Hosting ABI cannot vouch for. Never coerced into `external` (T-LCD-07).
    Unverified = 0,
    /// Open-world content and all free text not from kernel/definition/principal (R-TEXT).
    External = 1,
    /// Closed-schema structured values from closed-world tools declared in the sealed
    /// definition. (May collapse into `external` only by dialect bump — ADR-0033 D7; OQ-102.)
    Environment = 2,
    /// Outputs of any AgentProcess: model claims, plans, subagent results, evolution candidates,
    /// hosted-participant claims. `delegate` may *propose* but never *confer*.
    Delegate = 3,
    /// The run's human principal: the task, replies, approvals, promoted memories.
    Principal = 4,
    /// Minted only at `seal` or by a `pin` endorsement: the sealed Harness Definition.
    Definition = 5,
    /// The top: minted only by the reference runtime — ledger facts, monitor decisions,
    /// deterministic validator verdicts.
    Kernel = 6,
}

impl AuthorityClass {
    /// All seven classes, lowest to highest.
    pub const ALL: [AuthorityClass; 7] = [
        AuthorityClass::Unverified,
        AuthorityClass::External,
        AuthorityClass::Environment,
        AuthorityClass::Delegate,
        AuthorityClass::Principal,
        AuthorityClass::Definition,
        AuthorityClass::Kernel,
    ];

    /// The canonical spelling used in canonical records and the `hir/provenance` payload.
    pub fn as_str(self) -> &'static str {
        match self {
            AuthorityClass::Unverified => "unverified",
            AuthorityClass::External => "external",
            AuthorityClass::Environment => "environment",
            AuthorityClass::Delegate => "delegate",
            AuthorityClass::Principal => "principal",
            AuthorityClass::Definition => "definition",
            AuthorityClass::Kernel => "kernel",
        }
    }

    /// Parse the canonical spelling. `imported` is **refused** — it is an `Origin`, never a
    /// class value (CF-082); unknown spellings are refused too (the order is closed).
    pub fn parse(s: &str) -> Option<AuthorityClass> {
        AuthorityClass::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == s)
    }
}

/// A `TaintTag` (§8.1 #3): an origin reference **at class ≤ `external`** — the four taint
/// sources. Taint is *carried* at C0 and *enforced* at C2 (ADR-0033 D4; §05g H2). The
/// `taint ≠ ∅ ⇒ authority ≤ external` rule is checked by
/// [`crate::record::ProvenanceRecord::validate`] (`TaintedAboveExternal`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaintTag {
    /// A tool plus its inner source (`tool(Ref<ToolCapability>, invocation_ref, inner_source)`).
    Tool {
        /// Identity coordinate of the `ToolCapability` (a `version_id` / content address).
        capability: String,
        /// The tool's inner source, when the capability declares one.
        inner_source: Option<String>,
    },
    /// A hosted participant (`participant(participant_ref, …)`).
    Participant {
        /// Identity coordinate of the participant.
        participant: String,
    },
    /// An import source (`import(source_system, …)`).
    Import {
        /// The source system the content was imported from.
        source_system: String,
    },
    /// An extension, by `extension_id` (§8.5/08.4 supply-chain surface).
    Extension {
        /// The extension's id.
        extension_id: String,
    },
}

impl TaintTag {
    /// A stable canonical label for the tag (used in the lowered `hir/provenance` payload).
    pub fn as_string(&self) -> String {
        match self {
            TaintTag::Tool {
                capability,
                inner_source,
            } => match inner_source {
                Some(inner) => format!("tool:{capability}+{inner}"),
                None => format!("tool:{capability}"),
            },
            TaintTag::Participant { participant } => format!("participant:{participant}"),
            TaintTag::Import { source_system } => format!("import:{source_system}"),
            TaintTag::Extension { extension_id } => format!("extension:{extension_id}"),
        }
    }
}

/// `ReaderSet = Public | set<PrincipalRef>` (§8.1 #3). `Public` is the identity of `∩` (join)
/// and the absorbing element of `∪` (meet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReaderSet {
    /// Every principal may read — the lattice identity for intersection.
    Public,
    /// A closed set of principal refs.
    Restricted(BTreeSet<String>),
}

impl ReaderSet {
    /// `readers₁ ⊇ readers₂` — the `leq` conjunct. `Public` is a superset of everything.
    pub fn is_superset_of(&self, other: &ReaderSet) -> bool {
        match (self, other) {
            (ReaderSet::Public, _) => true,
            (ReaderSet::Restricted(a), ReaderSet::Restricted(b)) => a.is_superset(b),
            (ReaderSet::Restricted(_), ReaderSet::Public) => false,
        }
    }

    /// `∩` for join: `Public ∩ x = x`.
    pub fn intersect(&self, other: &ReaderSet) -> ReaderSet {
        match (self, other) {
            (ReaderSet::Public, r) | (r, ReaderSet::Public) => r.clone(),
            (ReaderSet::Restricted(a), ReaderSet::Restricted(b)) => {
                ReaderSet::Restricted(a.intersection(b).cloned().collect())
            }
        }
    }

    /// `∪` for meet: `Public ∪ x = Public`.
    pub fn union(&self, other: &ReaderSet) -> ReaderSet {
        match (self, other) {
            (ReaderSet::Public, _) | (_, ReaderSet::Public) => ReaderSet::Public,
            (ReaderSet::Restricted(a), ReaderSet::Restricted(b)) => {
                ReaderSet::Restricted(a.union(b).cloned().collect())
            }
        }
    }

    /// Whether `reader` is in the set (P5 filter iii: `reader ∈ readers(item)`).
    pub fn admits(&self, reader: &str) -> bool {
        match self {
            ReaderSet::Public => true,
            ReaderSet::Restricted(s) => s.contains(reader),
        }
    }
}

/// `PersistenceScope` (§8.1 #3): **where an item lives**, distinct from its authority.
/// `{definition, user, project, session, run, turn}`. The write ceilings
/// (`user` ≥ `principal` + `approval`; `project` ≥ `principal`; `session`/`run` ≥ `delegate`;
/// `definition` only via `seal`; the `store = memory` caps) are enforced by `check_persist` —
/// a **C0/Stage-2** surface (§8.1 #9), not this ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PersistenceScope {
    /// The sealed definition — reachable only via `seal`.
    Definition,
    /// User scope — write ceiling `principal` + `approval`.
    User,
    /// Project scope — write ceiling `principal`.
    Project,
    /// Session scope — write ceiling `delegate`.
    Session,
    /// Run scope — write ceiling `delegate`.
    Run,
    /// Turn scope — the most ephemeral; lives for one turn.
    Turn,
}

impl PersistenceScope {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PersistenceScope::Definition => "definition",
            PersistenceScope::User => "user",
            PersistenceScope::Project => "project",
            PersistenceScope::Session => "session",
            PersistenceScope::Run => "run",
            PersistenceScope::Turn => "turn",
        }
    }
}

/// The **opacity report** (T-LCD-02; AC-R-2.1.5-1): over a set of `Text` leaves (or any
/// records), the count per `AuthorityClass` — the breakdown a run's transparency report uses
/// to say how much of its context the harness could not fully trust. Every text leaf has a
/// class ([`crate::origin::default_text_authority`] is total — R-TEXT), so the report is a
/// total partition: `by_class` sums to the input length.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OpacityReport {
    /// `class → count` for every class that occurred (absent key = zero).
    pub by_class: std::collections::BTreeMap<AuthorityClass, usize>,
}

impl OpacityReport {
    /// Build the report over the authority classes of `leaves`.
    pub fn over(classes: impl IntoIterator<Item = AuthorityClass>) -> OpacityReport {
        let mut by_class = std::collections::BTreeMap::new();
        for c in classes {
            *by_class.entry(c).or_insert(0) += 1;
        }
        OpacityReport { by_class }
    }

    /// The count at `class` (0 if absent).
    pub fn count(&self, class: AuthorityClass) -> usize {
        self.by_class.get(&class).copied().unwrap_or(0)
    }

    /// The total number of leaves reported on.
    pub fn total(&self) -> usize {
        self.by_class.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seven_classes_form_the_closed_total_order() {
        // kernel > definition > principal > delegate > environment > external > unverified.
        let sorted = [
            AuthorityClass::Kernel,
            AuthorityClass::Definition,
            AuthorityClass::Principal,
            AuthorityClass::Delegate,
            AuthorityClass::Environment,
            AuthorityClass::External,
            AuthorityClass::Unverified,
        ];
        for w in sorted.windows(2) {
            assert!(w[0] > w[1], "{:?} must exceed {:?}", w[0], w[1]);
        }
        assert_eq!(AuthorityClass::ALL.len(), 7);
    }

    #[test]
    fn parse_round_trips_and_refuses_imported() {
        for c in AuthorityClass::ALL {
            assert_eq!(AuthorityClass::parse(c.as_str()), Some(c));
        }
        // CF-082: `imported` is an Origin, never a class value.
        assert_eq!(AuthorityClass::parse("imported"), None);
        assert_eq!(AuthorityClass::parse("trusted"), None);
        assert_eq!(AuthorityClass::parse(""), None);
    }

    #[test]
    fn opacity_report_partitions_by_class() {
        // T-LCD-02: the report breaks down by class and sums to the input length.
        let report = OpacityReport::over([
            AuthorityClass::Kernel,
            AuthorityClass::External,
            AuthorityClass::External,
            AuthorityClass::Unverified,
        ]);
        assert_eq!(report.count(AuthorityClass::External), 2);
        assert_eq!(report.count(AuthorityClass::Kernel), 1);
        assert_eq!(report.count(AuthorityClass::Principal), 0);
        assert_eq!(report.total(), 4);
    }

    #[test]
    fn reader_set_algebra() {
        let a = ReaderSet::Restricted(BTreeSet::from(["alice".to_string()]));
        let ab = ReaderSet::Restricted(BTreeSet::from(["alice".to_string(), "bob".to_string()]));
        // Public is the identity of ∩ and absorbing for ∪.
        assert_eq!(ReaderSet::Public.intersect(&a), a);
        assert_eq!(ReaderSet::Public.union(&a), ReaderSet::Public);
        assert_eq!(a.intersect(&ab), a);
        assert_eq!(a.union(&ab), ab);
        // Superset: Public ⊇ everything; Restricted compares by inclusion.
        assert!(ReaderSet::Public.is_superset_of(&ab));
        assert!(ab.is_superset_of(&a));
        assert!(!a.is_superset_of(&ReaderSet::Public));
        // Membership.
        assert!(ReaderSet::Public.admits("anyone"));
        assert!(ab.admits("bob"));
        assert!(!a.admits("bob"));
    }
}
