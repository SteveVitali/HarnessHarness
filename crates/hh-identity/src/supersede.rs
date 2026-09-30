//! Supersession & revocation: the `supersedes` edge, [`RevocationRecord`], the derived
//! [`StaleIndex`], and `supersede`/`revoke` (§8.3 #2/#3; ADR-0037 D2 as amended by ADR-0080/0082).
//!
//! Revocation is **never an in-place flag** (S1): a `RevocationRecord` is a new version of the
//! same kind with an empty body, so "revoked" is itself hash-chained. The version graph is
//! **acyclic** and per-name a DAG (S2). Dependants of a revoked member are *stale-by-dependency*
//! in the derived [`StaleIndex`] and their results are annotated `depends_on_revoked = true`,
//! never silently excluded (S4; CC3). Full resolver enforcement over the index is Stage 2; this
//! Stage-1 slice lands the records, edges and the index.

use std::collections::BTreeMap;

use crate::kinds::RecordKind;
use hh_provenance::{Origin, PersistenceScope, ProvenanceRecord};

/// The closed set of supersession reasons (§8.3 #3; ADR-0037 D2 as amended — `fork` per ADR-0082,
/// `consolidation` per ADR-0080). A new reason is a dialect bump, never an ad-hoc string (CC8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupersedeReason {
    Edit,
    Revocation,
    Expiry,
    Migration,
    Consolidation,
    Fork,
}

/// A `supersedes` edge carried on the newer record and mirrored in the name history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupersedesEdge {
    pub newer: String,
    pub older: String,
    pub reason: SupersedeReason,
}

/// A `RevocationRecord` — a new version of the same kind with an **empty body** (§8.3 #3). "Revoked"
/// is hash-chained: `revokes` names the version_id being revoked, and the record itself gets its
/// own version id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevocationRecord {
    pub kind: RecordKind,
    pub revokes: String,
    pub reason: SupersedeReason,
    /// The claimed valid-time end (`validity.until`); `None` = revoked immediately.
    pub validity_until: Option<u64>,
    pub replacement: Option<String>,
    /// Who authorized the revocation — the canonical §8.1 `Origin`.
    pub authority: Origin,
    /// The revocation's canonical provenance record (minted from `authority` at `seq`).
    pub provenance: ProvenanceRecord,
}

/// One stale-by-dependency entry (`StaleIndex[version_id] → [revoked member, reason, since]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleEntry {
    /// The revoked member this dependant transitively depends on.
    pub revoked_member: String,
    pub reason: SupersedeReason,
    /// The transaction-time `seq` the revocation took effect.
    pub since_seq: u64,
}

/// `supersede`/`revoke` failure modes (§8.3 #2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupersedeError {
    /// The edge would introduce a cycle in the version graph (§8.3 #2, S2).
    CycleDetected { newer: String, older: String },
    /// The successor is not an ancestor or sibling on the lineage — allowed only with
    /// `reason = fork` (§8.3 #2).
    NotAncestorOrSibling,
}

/// The lineage graph plus the derived stale index. A registry/results consumer reads the stale
/// index; it is derived, never authored (§8.3 #3, S4).
#[derive(Debug, Clone, Default)]
pub struct Lineage {
    edges: Vec<SupersedesEdge>,
    revocations: Vec<RevocationRecord>,
    /// version_id → [members it (transitively) depends on].
    dependencies: BTreeMap<String, Vec<String>>,
    stale: BTreeMap<String, Vec<StaleEntry>>,
    next_seq: u64,
}

impl Lineage {
    pub fn new() -> Lineage {
        Lineage::default()
    }

    /// Record that `dependant` depends on `member` (used to compute stale-by-dependency).
    /// When `member` is already revoked — or itself stale on revoked members — the
    /// dependant joins the derived index immediately (the index is derived, never
    /// authored; registering onto a revoked pin is never silently clean — S4/CC3).
    pub fn declare_dependency(&mut self, dependant: &str, member: &str) {
        self.dependencies
            .entry(dependant.to_string())
            .or_default()
            .push(member.to_string());
        for cause in self.revoked_causes(member) {
            self.mark_stale(dependant, &cause.0, cause.1, cause.2);
        }
    }

    /// `supersede(new, old, reason)` — emit the edge, never touch `old`. Refuses a cycle; a
    /// non-ancestor/sibling edge is allowed only with `reason = Fork`.
    pub fn supersede(
        &mut self,
        newer: &str,
        older: &str,
        reason: SupersedeReason,
        ancestor_or_sibling: bool,
    ) -> Result<&SupersedesEdge, SupersedeError> {
        if self.would_cycle(newer, older) {
            return Err(SupersedeError::CycleDetected {
                newer: newer.to_string(),
                older: older.to_string(),
            });
        }
        if !ancestor_or_sibling && reason != SupersedeReason::Fork {
            return Err(SupersedeError::NotAncestorOrSibling);
        }
        self.edges.push(SupersedesEdge {
            newer: newer.to_string(),
            older: older.to_string(),
            reason,
        });
        Ok(self.edges.last().unwrap())
    }

    /// `revoke(old, reason, authority, replacement?) → RevocationRecord`. Emits the record,
    /// extends the lineage, and updates the derived stale index for every dependant of `old`.
    pub fn revoke(
        &mut self,
        old: &str,
        kind: RecordKind,
        reason: SupersedeReason,
        authority: Origin,
        replacement: Option<String>,
    ) -> RevocationRecord {
        let seq = self.next_seq;
        self.next_seq += 1;
        let rec = RevocationRecord {
            kind,
            revokes: old.to_string(),
            reason,
            validity_until: None,
            replacement: replacement.clone(),
            authority: authority.clone(),
            provenance: ProvenanceRecord::minted(authority, PersistenceScope::Run, seq),
        };
        self.revocations.push(rec.clone());
        // Every *transitive* dependant of `old` becomes stale-by-dependency (never
        // silently excluded — CC3). A dependant of `old` also inherits every revoked
        // member `old` was itself stale on: `d → old → … → m` means `d` transitively
        // depends on `m` too.
        let mut causes: Vec<(String, SupersedeReason, u64)> = vec![(old.to_string(), reason, seq)];
        causes.extend(
            self.stale_for(old)
                .iter()
                .map(|s| (s.revoked_member.clone(), s.reason, s.since_seq)),
        );
        let mut queue: Vec<String> = vec![old.to_string()];
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        seen.insert(old.to_string());
        while let Some(node) = queue.pop() {
            let direct: Vec<String> = self
                .dependencies
                .iter()
                .filter(|(_, members)| members.contains(&node))
                .map(|(d, _)| d.clone())
                .collect();
            for d in direct {
                if !seen.insert(d.clone()) {
                    continue;
                }
                queue.push(d.clone());
                for (member, cause_reason, cause_seq) in &causes {
                    self.mark_stale(&d, member, *cause_reason, *cause_seq);
                }
            }
        }
        rec
    }

    /// Whether `version_id` is revoked (a `RevocationRecord` names it).
    pub fn is_revoked(&self, version_id: &str) -> bool {
        self.revocations.iter().any(|r| r.revokes == version_id)
    }

    /// The revoked members `member` transitively depends on, as
    /// `(revoked_member, reason, since_seq)` — `{member}` itself when revoked,
    /// plus every cause already in `StaleIndex[member]` (dependants inherit them).
    fn revoked_causes(&self, member: &str) -> Vec<(String, SupersedeReason, u64)> {
        let mut out: Vec<(String, SupersedeReason, u64)> = Vec::new();
        if let Some(r) = self.revocations.iter().find(|r| r.revokes == member) {
            out.push((
                member.to_string(),
                r.reason,
                self.revocations
                    .iter()
                    .position(|x| x.revokes == member)
                    .unwrap_or(0) as u64,
            ));
        }
        for s in self.stale_for(member) {
            out.push((s.revoked_member.clone(), s.reason, s.since_seq));
        }
        out
    }

    /// Record one stale-by-dependency entry (deduplicated on `revoked_member`).
    fn mark_stale(
        &mut self,
        dependant: &str,
        revoked_member: &str,
        reason: SupersedeReason,
        since_seq: u64,
    ) {
        let entries = self.stale.entry(dependant.to_string()).or_default();
        if !entries.iter().any(|e| e.revoked_member == revoked_member) {
            entries.push(StaleEntry {
                revoked_member: revoked_member.to_string(),
                reason,
                since_seq,
            });
        }
    }

    /// The stale-by-dependency entries for a dependant (`StaleIndex[dependant]`).
    pub fn stale_for(&self, dependant: &str) -> &[StaleEntry] {
        self.stale
            .get(dependant)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Whether a result over `dependant` must be annotated `depends_on_revoked = true` (never
    /// hidden — §8.3 #3 S4).
    pub fn depends_on_revoked(&self, dependant: &str) -> bool {
        !self.stale_for(dependant).is_empty()
    }

    /// All `supersedes` edges, in append order (read-only — the registry store persists and
    /// replays them; additive accessor, CC8).
    pub fn edges(&self) -> &[SupersedesEdge] {
        &self.edges
    }

    /// All `RevocationRecord`s, in append order (read-only — same consumer as `edges`).
    pub fn revocations(&self) -> &[RevocationRecord] {
        &self.revocations
    }

    fn would_cycle(&self, newer: &str, older: &str) -> bool {
        // Edges point `newer → older` (toward ancestors). Adding `newer → older` closes a cycle
        // iff `older` can already reach `newer` following those edges (older → … → newer). Walk
        // from `older` in the edge direction (from a node, take edges whose `newer` == node to its
        // `older`) and see whether we arrive at `newer`.
        if newer == older {
            return true;
        }
        let mut stack = vec![older.to_string()];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur.clone()) {
                continue;
            }
            for e in &self.edges {
                if e.newer == cur {
                    if e.older == newer {
                        return true;
                    }
                    stack.push(e.older.clone());
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revocation_is_a_record_not_a_flag() {
        let mut lin = Lineage::new();
        let rec = lin.revoke(
            "sha256:v1",
            RecordKind::Validator,
            SupersedeReason::Revocation,
            Origin::human("revoker", hh_provenance::HumanRole::Principal),
            Some("sha256:v2".into()),
        );
        assert_eq!(rec.revokes, "sha256:v1");
        assert_eq!(rec.replacement.as_deref(), Some("sha256:v2"));
        assert!(lin.is_revoked("sha256:v1"));
    }

    #[test]
    fn dependants_of_a_revoked_member_are_stale_by_dependency() {
        // CC3 / S4: annotated, never silently excluded.
        let mut lin = Lineage::new();
        lin.declare_dependency("sha256:run1", "sha256:member");
        lin.declare_dependency("sha256:run2", "sha256:other");
        lin.revoke(
            "sha256:member",
            RecordKind::VariantRecord,
            SupersedeReason::Revocation,
            Origin::human("revoker", hh_provenance::HumanRole::Principal),
            None,
        );
        assert!(lin.depends_on_revoked("sha256:run1"));
        assert_eq!(
            lin.stale_for("sha256:run1")[0].revoked_member,
            "sha256:member"
        );
        assert!(!lin.depends_on_revoked("sha256:run2"));
    }

    #[test]
    fn supersede_refuses_a_cycle() {
        let mut lin = Lineage::new();
        lin.supersede("sha256:v2", "sha256:v1", SupersedeReason::Edit, true)
            .unwrap();
        lin.supersede("sha256:v3", "sha256:v2", SupersedeReason::Edit, true)
            .unwrap();
        // v1 → v3 would close a cycle (v3 already supersedes v2 supersedes v1).
        assert!(matches!(
            lin.supersede("sha256:v1", "sha256:v3", SupersedeReason::Edit, true),
            Err(SupersedeError::CycleDetected { .. })
        ));
    }

    #[test]
    fn non_ancestor_edge_needs_fork_reason() {
        let mut lin = Lineage::new();
        assert!(matches!(
            lin.supersede("sha256:b", "sha256:a", SupersedeReason::Edit, false),
            Err(SupersedeError::NotAncestorOrSibling)
        ));
        assert!(lin
            .supersede("sha256:b", "sha256:a", SupersedeReason::Fork, false)
            .is_ok());
    }
}
