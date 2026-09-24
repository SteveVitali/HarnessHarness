//! **P7** — the lowering carrier (§8.1 #6): when a canonical `ProvenanceRecord` crosses into a
//! representation that cannot hold the whole record (a wire projection, a CEL/Datalog policy
//! atom, an evidence-ledger row, a host-side artifact stamp), the lowered form **must not
//! silently drop** what the target cannot express. [`lower`] returns the projection *plus* an
//! explicit `lost` field list; [`lift`] reconstitutes a record whose lost fields are defaulted
//! **and whose `taint` is stamped with the lift**, so a round-trip is never invisible and a
//! lift **never widens** authority (CC2 — the lifted record's `origin` is `import`, so
//! `default_authority` mints it `unverified`; the stamped record is capped at `≤ external` and
//! fails [`ProvenanceRecord::validate`] if it claims more).
//!
//! **RP** — role placement ([`render_role`], §8.1 #6): a definition's role text
//! (`system`/`developer`/`user`/`tool`) is placed by **authority**, never by the author's
//! wish: a `definition`-authority record may occupy `system`/`developer`; `principal` may
//! occupy `user`; `external` may occupy `tool`. A request to place a low-authority record in a
//! high slot is a typed refusal — never a silent re-slot, never a warning.

use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::authority::{AuthorityClass, PersistenceScope, ReaderSet, TaintTag};
use crate::origin::Origin;
use crate::record::{Derivation, DerivationKind, ProvenanceRecord};

/// A lowering target — how much of the record survives the projection (§8.1 P7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowerTarget {
    /// A wire projection that carries the full record (nothing lost).
    Wire,
    /// A CEL/Datalog policy atom: keeps `authority` + `taint`, loses `readers`/`derived_from`/
    /// `attestation`/`origin` detail.
    PolicyAtom,
    /// An evidence-ledger row: keeps the `origin` tag + `authority` + `created_at`, loses the
    /// rest.
    EvidenceRow,
    /// A host-side artifact stamp: keeps `authority` + `created_at` only.
    ArtifactStamp,
}

/// The lowered projection — the fields the target carries, plus the explicit `lost` list.
/// `lost` names every field the target could not express (§8.1 P6 "no silent loss"): a
/// consumer can never *believe* it holds the whole record when it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lowered {
    /// The target.
    pub target: LowerTarget,
    /// The carried `authority` (always survives — the minimum the carrier guarantees).
    pub authority: AuthorityClass,
    /// The carried `taint` (as canonical tag strings), if the target expresses it.
    pub taint: Option<Vec<String>>,
    /// The carried `readers`, if the target expresses it.
    pub readers: Option<ReaderSet>,
    /// The carried `origin` tag (the discriminant only), if the target expresses it.
    pub origin_tag: Option<String>,
    /// The carried `created_at`, if the target expresses it.
    pub created_at: Option<u64>,
    /// The canonical record fields the target **could not carry** — the explicit loss list.
    pub lost: Vec<&'static str>,
    /// The full record — carried only by [`LowerTarget::Wire`] (the one target that expresses
    /// everything). For every other target this is `None` and `lost` names what was dropped.
    pub record: Option<Box<ProvenanceRecord>>,
}

/// P7 `lower` — project a `ProvenanceRecord` onto a carrier, naming every field that does not
/// survive.
pub fn lower(record: &ProvenanceRecord, target: LowerTarget) -> Lowered {
    let origin_tag = record.origin.tag().to_string();
    let taint: Vec<String> = record.taint.iter().map(|t| t.as_string()).collect();
    match target {
        LowerTarget::Wire => Lowered {
            target,
            authority: record.authority,
            taint: Some(taint),
            readers: Some(record.readers.clone()),
            origin_tag: Some(origin_tag),
            created_at: Some(record.created_at),
            lost: vec![],
            record: Some(Box::new(record.clone())),
        },
        LowerTarget::PolicyAtom => Lowered {
            target,
            authority: record.authority,
            taint: Some(taint),
            readers: None,
            origin_tag: None,
            created_at: None,
            lost: vec!["readers", "derived_from", "attestation", "origin", "scope"],
            record: None,
        },
        LowerTarget::EvidenceRow => Lowered {
            target,
            authority: record.authority,
            taint: None,
            readers: None,
            origin_tag: Some(origin_tag),
            created_at: Some(record.created_at),
            lost: vec![
                "taint",
                "readers",
                "derived_from",
                "attestation",
                "origin.detail",
                "scope",
            ],
            record: None,
        },
        LowerTarget::ArtifactStamp => Lowered {
            target,
            authority: record.authority,
            taint: None,
            readers: None,
            origin_tag: None,
            created_at: Some(record.created_at),
            lost: vec![
                "taint",
                "readers",
                "derived_from",
                "attestation",
                "origin",
                "scope",
            ],
            record: None,
        },
    }
}

/// The lift result — the reconstituted record plus the fields that were **not** recoverable
/// (the same list `lower` produced, kept so the consumer knows exactly what is reconstruction,
/// not fact).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lifted {
    /// The reconstituted record.
    pub record: ProvenanceRecord,
    /// The fields defaulted on lift.
    pub defaulted: Vec<&'static str>,
}

/// P7 `lift` — reconstitute a `ProvenanceRecord` from a lowered projection.
///
/// The lift is **lossy and explicit**:
/// - a `Wire` projection carried the whole record — the lift returns it unchanged;
/// - otherwise `origin` is `import(lifted:<source-tag>)` — a lifted "kernel" is never mistaken
///   for a live kernel fact, and `default_authority` mints the record `unverified` (lifted
///   content is `unverified` — the minting table's bottom);
/// - every carried taint string is re-stamped as an `import` tag, plus a `lift:<target>`
///   marker so the loss is stamped, not erased;
/// - a lift **never widens**: the record's `authority` is what `default_authority` mints for
///   the import origin (`unverified`) regardless of the carried class — a bare projection
///   carries no verifiable higher authority (CC2);
/// - `derived_from` records the lift as a deterministic `projection`.
pub fn lift(lowered: &Lowered, scope: PersistenceScope) -> Lifted {
    if let Some(record) = &lowered.record {
        // A full-fidelity carrier: nothing to reconstitute, nothing defaulted.
        return Lifted {
            record: (**record).clone(),
            defaulted: vec![],
        };
    }
    let origin = Origin::import(
        format!(
            "lifted:{}",
            lowered
                .origin_tag
                .clone()
                .unwrap_or_else(|| target_tag(lowered.target).to_string())
        ),
        format!("lowered:{}", target_tag(lowered.target)),
    );
    let mut taint = std::collections::BTreeSet::new();
    for t in lowered.taint.clone().unwrap_or_default() {
        taint.insert(TaintTag::Import {
            source_system: format!("carried:{t}"),
        });
    }
    taint.insert(TaintTag::Import {
        source_system: format!("lift:{}", target_tag(lowered.target)),
    });
    let readers = lowered.readers.clone().unwrap_or(ReaderSet::Public);
    let created_at = lowered.created_at.unwrap_or(0);
    let mut record = ProvenanceRecord::minted(origin.clone(), scope, created_at);
    record.taint = taint;
    record.readers = readers;
    record.derived_from = vec![Derivation {
        kind: DerivationKind::Projection,
        inputs: vec![format!("lowered:{}", target_tag(lowered.target))],
        deriver: Origin::kernel("kernel:lift"),
        deterministic: true,
    }];
    Lifted {
        defaulted: lowered.lost.clone(),
        record,
    }
}

fn target_tag(t: LowerTarget) -> &'static str {
    match t {
        LowerTarget::Wire => "wire",
        LowerTarget::PolicyAtom => "policy-atom",
        LowerTarget::EvidenceRow => "evidence-row",
        LowerTarget::ArtifactStamp => "artifact-stamp",
    }
}

// ── RP — role placement ──────────────────────────────────────────────────────

/// The message-role slots a definition's text may occupy (§8.1 RP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RoleSlot {
    /// `system` — harness-author instructions (requires `definition`).
    System,
    /// `developer` — platform/developer instructions (requires `definition`).
    Developer,
    /// `user` — the principal's turn (requires `principal`).
    User,
    /// `tool` — a tool/environment result (requires `external`).
    Tool,
}

impl RoleSlot {
    /// The minimum authority a record must carry to occupy this slot.
    pub fn required_authority(self) -> AuthorityClass {
        match self {
            RoleSlot::System | RoleSlot::Developer => AuthorityClass::Definition,
            RoleSlot::User => AuthorityClass::Principal,
            RoleSlot::Tool => AuthorityClass::External,
        }
    }
}

/// The role-placement refusal (RP): a record whose `authority` is below the requested slot's
/// requirement may not be placed there — a typed refusal, never a silent re-slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RolePlacementError {
    /// `record.authority < slot.required_authority` — a low-authority record may not occupy a
    /// high slot (an `external` tool result may not be re-slotted as `system`).
    InsufficientAuthority {
        /// The record's authority.
        record: AuthorityClass,
        /// The requested slot.
        slot: RoleSlot,
        /// The slot's required authority.
        required: AuthorityClass,
    },
}

/// RP `render_role(record, slot)` — place a definition's text into a message-role slot,
/// guarded by authority (§8.1 RP). Returns the slot on success.
pub fn render_role(
    record: &ProvenanceRecord,
    slot: RoleSlot,
) -> Result<RoleSlot, RolePlacementError> {
    let required = slot.required_authority();
    if record.authority < required {
        return Err(RolePlacementError::InsufficientAuthority {
            record: record.authority,
            slot,
            required,
        });
    }
    Ok(slot)
}

// ── `hir/provenance` in `_meta` (§8.1 P7 — MCP/A2A/ACP/provider tool APIs) ────

/// The typed-extension-slot key carrying the provenance projection —
/// `_meta["hir/provenance"]` on MCP (§8.1 P7: `{origin_ref, authority, scope,
/// taint_tags}`; `readers` rides the same member at C2 — AC-R-2.8.2-13).
pub const META_PROVENANCE_KEY: &str = "hir/provenance";

/// `lower_provenance(record) → _meta member` — the compact projection a
/// foreign target carries in its typed extension slot: `{origin_ref,
/// authority, scope, taint_tags[], readers[]}` (readers `[]` = `Public` —
/// the same spelling `label_json_full` uses). This is the *carrier* form —
/// the record itself never crosses; a target that cannot carry the member
/// records the loss through [`lift_provenance_meta`]'s report.
pub fn lower_provenance_meta(record: &ProvenanceRecord) -> Json {
    Json::obj([
        ("origin_ref", Json::str(record.origin.tag().to_string())),
        ("authority", Json::str(record.authority.as_str())),
        ("scope", Json::str(record.scope.as_str())),
        (
            "taint_tags",
            Json::Arr(
                record
                    .taint
                    .iter()
                    .map(|t| Json::str(t.as_string()))
                    .collect(),
            ),
        ),
        (
            "readers",
            match &record.readers {
                ReaderSet::Public => Json::Arr(vec![]),
                ReaderSet::Restricted(rs) => {
                    Json::Arr(rs.iter().map(|r| Json::str(r.clone())).collect())
                }
            },
        ),
    ])
}

/// Where a lifted payload came from — fixes the minted authority (the
/// lift's *cap*; a class is never read from the payload itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaLiftSource {
    /// A server outside the sealed definition — `origin = import`,
    /// `authority = unverified`.
    ExternalServer,
    /// An open-world tool present in the sealed definition — `origin =
    /// tool(capability)`, `authority = external` (the open-world row of the
    /// minting table).
    DeclaredOpenWorldTool {
        /// The capability's identity coordinate.
        capability: String,
        /// The invocation coordinate the result answers.
        invocation_ref: String,
    },
    /// A hosted participant's content the Hosting ABI cannot vouch for —
    /// `origin = participant`, `authority = unverified` (T-LCD-07).
    Participant {
        /// The participant's identity coordinate.
        participant_ref: String,
        /// The hosting mechanism (`hh.hosting/1` binding id).
        hosting_mechanism: String,
    },
}

/// The lift result — the reconstituted record plus the explicit
/// carried/lost lists (§8.1 P6/P7: the loss report the lowering owes; a
/// target that could not carry `hir/provenance` is *named*, not silent).
#[derive(Debug, Clone, PartialEq)]
pub struct MetaLift {
    /// The lifted record — authority fixed by the source class
    /// (`unverified`/`external`), taint stamped with the lift, `derived_from`
    /// recording the projection.
    pub record: ProvenanceRecord,
    /// The members the slot carried and the lift honoured (`taint_tags`,
    /// `readers` — restrictions are safe to carry: they never raise
    /// authority).
    pub carried: Vec<&'static str>,
    /// The explicit loss list — members the payload claimed that could not
    /// survive (`authority`/`scope`/`origin_ref` mismatches against the
    /// minted cap), or the whole member when the target could not carry it.
    pub lost: Vec<String>,
}

/// `lift_provenance(meta, source, scope, at)` — reconstitute a
/// `ProvenanceRecord` from an `_meta` payload (§8.1 P7). **A class is never
/// read from the lifted payload**: the record's authority is what the
/// minting table assigns the source's origin — `unverified` for
/// `import`/`participant`, `external` for a declared open-world `tool`.
/// Carried `taint_tags` are re-stamped as `import` markers plus a
/// `lift:mcp` tag (the loss is stamped, not erased); carried `readers`
/// restore verbatim (a restriction claim can only narrow where the value
/// flows). Every member that could not survive lands in `lost`.
pub fn lift_provenance_meta(
    meta: Option<&Json>,
    source: &MetaLiftSource,
    scope: PersistenceScope,
    at: u64,
) -> MetaLift {
    let (origin, cap) = match source {
        MetaLiftSource::ExternalServer => (
            Origin::import("lifted:mcp", "lowered:mcp-meta"),
            AuthorityClass::Unverified,
        ),
        MetaLiftSource::DeclaredOpenWorldTool {
            capability,
            invocation_ref,
        } => (
            Origin::tool(capability.clone(), invocation_ref.clone()),
            AuthorityClass::External,
        ),
        MetaLiftSource::Participant {
            participant_ref,
            hosting_mechanism,
        } => (
            Origin::participant(participant_ref.clone(), hosting_mechanism.clone()),
            AuthorityClass::Unverified,
        ),
    };
    let mut record = ProvenanceRecord::minted(origin, scope, at);
    record.authority = cap;
    let mut carried: Vec<&'static str> = Vec::new();
    let mut lost: Vec<String> = Vec::new();
    let mut taint: BTreeSet<TaintTag> = BTreeSet::new();
    match meta {
        None => {
            // A target that cannot carry `hir/provenance` — the loss report
            // names the whole member (T-LCD-11).
            lost.push(format!("{META_PROVENANCE_KEY} (target cannot carry it)"));
        }
        Some(m) => {
            // `authority`/`scope`/`origin_ref` are descriptive only — the
            // minted cap decides. A payload claiming a *different* class
            // than the cap is named in the loss report (never believed).
            if let Some(a) = m.get("authority").and_then(Json::as_str) {
                if a != cap.as_str() {
                    lost.push(format!("authority (claimed {a}, minted {})", cap.as_str()));
                }
            }
            if let Some(s) = m.get("scope").and_then(Json::as_str) {
                if s != scope.as_str() {
                    lost.push(format!("scope (claimed {s}, minted {})", scope.as_str()));
                }
            }
            if let Some(o) = m.get("origin_ref").and_then(Json::as_str) {
                if o != record.origin.tag() {
                    lost.push(format!(
                        "origin_ref (claimed {o}, minted {})",
                        record.origin.tag()
                    ));
                }
            }
            match m.get("taint_tags") {
                Some(Json::Arr(items)) => {
                    for t in items {
                        if let Some(ts) = t.as_str() {
                            taint.insert(TaintTag::Import {
                                source_system: format!("carried:{ts}"),
                            });
                        }
                    }
                    carried.push("taint_tags");
                }
                Some(_) => lost.push("taint_tags (malformed)".to_string()),
                None => {}
            }
            match m.get("readers") {
                Some(Json::Arr(items)) => {
                    let set: BTreeSet<String> = items
                        .iter()
                        .filter_map(|i| i.as_str().map(String::from))
                        .collect();
                    record.readers = if set.is_empty() {
                        ReaderSet::Public
                    } else {
                        ReaderSet::Restricted(set)
                    };
                    carried.push("readers");
                }
                Some(_) => lost.push("readers (malformed)".to_string()),
                None => {}
            }
        }
    }
    taint.insert(TaintTag::Import {
        source_system: "lift:mcp".to_string(),
    });
    record.taint = taint;
    record.derived_from = vec![Derivation {
        kind: DerivationKind::Projection,
        inputs: vec!["lowered:mcp-meta".to_string()],
        deriver: Origin::kernel("kernel:lift"),
        deterministic: true,
    }];
    MetaLift {
        record,
        carried,
        lost,
    }
}

// ── `role_map` collapse (§8.1 RP — targets with fewer roles) ────────────────

/// One collapsed group: the provider role several `AuthorityClass`es share —
/// the classes whose distinction the target's `role_map` cannot express.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleCollapse {
    /// The provider-role spelling the classes collapse into.
    pub role: String,
    /// The classes sharing it (sorted by lattice order).
    pub classes: Vec<AuthorityClass>,
}

/// `role_map_collapse(role_map)` — the loss a reduced-role target declares
/// (§8.1 RP: "targets with fewer roles collapse classes and the lowering
/// loss report says so"). Groups the profile's `AuthorityClass →
/// ProviderRole` map by role; every role serving more than one class is a
/// collapse the loss report names — e.g. a two-role target collapses
/// `{kernel, definition, …}` into its privileged role and the rest into its
/// unprivileged one.
pub fn role_map_collapse(
    role_map: &std::collections::BTreeMap<AuthorityClass, String>,
) -> Vec<RoleCollapse> {
    let mut by_role: std::collections::BTreeMap<String, Vec<AuthorityClass>> =
        std::collections::BTreeMap::new();
    for (class, role) in role_map {
        by_role.entry(role.clone()).or_default().push(*class);
    }
    by_role
        .into_iter()
        .filter(|(_, classes)| classes.len() > 1)
        .map(|(role, classes)| RoleCollapse { role, classes })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::origin::HumanRole;

    fn rec(origin: Origin, taint: &[&str]) -> ProvenanceRecord {
        let mut r = ProvenanceRecord::minted(origin, PersistenceScope::Run, 0);
        for t in taint {
            r.taint.insert(TaintTag::Import {
                source_system: (*t).to_string(),
            });
            r.authority = r.authority.min(AuthorityClass::External);
        }
        r
    }

    #[test]
    fn a_wire_projection_loses_nothing() {
        let r = rec(Origin::human("alice", HumanRole::Principal), &[]);
        let l = lower(&r, LowerTarget::Wire);
        assert!(l.lost.is_empty());
        assert_eq!(l.authority, r.authority);
        assert!(l.taint.is_some() && l.readers.is_some() && l.origin_tag.is_some());
        // A full-fidelity carrier round-trips the record unchanged.
        let lifted = lift(&l, PersistenceScope::Run);
        assert_eq!(lifted.record, r);
        assert!(lifted.defaulted.is_empty());
    }

    #[test]
    fn a_policy_atom_names_every_dropped_field() {
        // P6/no-silent-loss: `lost` is the contract — a consumer sees exactly what is gone.
        let r = rec(Origin::human("alice", HumanRole::Principal), &[]);
        let l = lower(&r, LowerTarget::PolicyAtom);
        assert!(l.lost.contains(&"readers"));
        assert!(l.lost.contains(&"derived_from"));
        assert!(l.lost.contains(&"attestation"));
        assert!(l.readers.is_none() && l.origin_tag.is_none());
    }

    #[test]
    fn lift_never_widens_and_stamps_the_loss() {
        // CC2: a lift mints `unverified` regardless of the carried class — a bare projection
        // carries no verifiable higher authority; the `lift:*` taint marker stamps the loss.
        let r = rec(Origin::human("alice", HumanRole::Principal), &[]);
        let l = lower(&r, LowerTarget::ArtifactStamp); // carries authority=principal, drops rest
        let lifted = lift(&l, PersistenceScope::Run);
        assert_eq!(lifted.record.authority, AuthorityClass::Unverified);
        assert!(lifted
            .record
            .taint
            .iter()
            .any(|t| t.as_string().starts_with("import:lift:")));
        assert!(!lifted.defaulted.is_empty());
        // The lifted record validates (its taint is consistent with its ≤ external authority).
        assert!(lifted.record.validate(None).is_ok());
    }

    #[test]
    fn a_tainted_lower_stays_low() {
        let r = rec(Origin::tool("t", "i"), &["env"]);
        let l = lower(&r, LowerTarget::EvidenceRow);
        assert_eq!(l.authority, AuthorityClass::External);
        let lifted = lift(&l, PersistenceScope::Run);
        assert!(lifted.record.authority <= AuthorityClass::External);
    }

    #[test]
    fn render_role_places_by_authority() {
        // A definition-authority record (sealed) may take system/developer.
        let mut def = ProvenanceRecord::minted(
            Origin::human("author", HumanRole::Author),
            PersistenceScope::Definition,
            0,
        );
        def.authority = AuthorityClass::Definition; // as if sealed
        assert!(render_role(&def, RoleSlot::System).is_ok());
        // A principal record may take user, not system.
        let user = ProvenanceRecord::minted(
            Origin::human("alice", HumanRole::Principal),
            PersistenceScope::User,
            0,
        );
        assert!(render_role(&user, RoleSlot::User).is_ok());
        assert!(matches!(
            render_role(&user, RoleSlot::System),
            Err(RolePlacementError::InsufficientAuthority { .. })
        ));
        // A tool result may take tool, not user/system.
        let tool = rec(Origin::tool("t", "i"), &["e"]);
        assert!(render_role(&tool, RoleSlot::Tool).is_ok());
        assert!(render_role(&tool, RoleSlot::User).is_err());
        assert!(render_role(&tool, RoleSlot::Developer).is_err());
    }
}
