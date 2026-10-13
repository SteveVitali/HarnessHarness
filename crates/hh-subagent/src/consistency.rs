//! The `ConsistencyLevel` closed set + `ConsistencyDeclaration` +
//! `CoordinationPolicy` (§5e.5 data model; ADR-0193 D5; K-1…K-4).
//!
//! K-1 is the *declaration consistency* check — a level inconsistent with
//! the `derive` mode is a definition error (`share` ⇏ `snapshot_at_fork`;
//! `fork_snapshot` ⇏ `per_key_serializable`; a ledger is always
//! `linearizable_single_writer`; keyed memory always
//! `conflict_set_eventual`). K-4 is the tightening-only rule —
//! [`CoordinationPolicy::delta`] classifies a policy diff as
//! `tightening | loosening | none`; evolution admits tightening only.

use hh_wire::json::Json;

/// `ConsistencyLevel` — the closed six-member set (§5e.5; ADR-0193 D5).
/// The names are the classic hierarchy's; `linearizable_single_writer` is
/// the ledger's own level (always admitted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConsistencyLevel {
    /// One writer; reads see the latest committed (AC-B1-11's level).
    LinearizableSingleWriter,
    /// The child sees the fork point exactly (isolated `fork_snapshot`).
    SnapshotAtFork,
    /// Per-key total order — no interleaved `observed` between another
    /// holder's `committed` and terminal on one key.
    PerKeySerializable,
    /// Causal order across runs (child `created` follows `spawned`; a
    /// merge follows every input head).
    CausalCrossRun,
    /// Union merge — permuting completion order leaves `merged[]`
    /// unchanged.
    StrongEventualUnion,
    /// Keyed-memory level — conflict-set union, no recency tiebreak.
    ConflictSetEventual,
}

impl ConsistencyLevel {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ConsistencyLevel::LinearizableSingleWriter => "linearizable_single_writer",
            ConsistencyLevel::SnapshotAtFork => "snapshot_at_fork",
            ConsistencyLevel::PerKeySerializable => "per_key_serializable",
            ConsistencyLevel::CausalCrossRun => "causal_cross_run",
            ConsistencyLevel::StrongEventualUnion => "strong_eventual_union",
            ConsistencyLevel::ConflictSetEventual => "conflict_set_eventual",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<ConsistencyLevel> {
        Some(match s {
            "linearizable_single_writer" => ConsistencyLevel::LinearizableSingleWriter,
            "snapshot_at_fork" => ConsistencyLevel::SnapshotAtFork,
            "per_key_serializable" => ConsistencyLevel::PerKeySerializable,
            "causal_cross_run" => ConsistencyLevel::CausalCrossRun,
            "strong_eventual_union" => ConsistencyLevel::StrongEventualUnion,
            "conflict_set_eventual" => ConsistencyLevel::ConflictSetEventual,
            _ => return None,
        })
    }

    /// The ordering rank used by K-4 — a *relaxation* moves toward the
    /// weaker end (`conflict_set_eventual`). `strong_eventual_union` and
    /// `causal_cross_run` rank equal (incomparable in the classic
    /// hierarchy; a swap between them is neither tightening nor
    /// loosening — the caller sees `none`, never a guessed direction).
    pub fn strength(self) -> u32 {
        match self {
            ConsistencyLevel::LinearizableSingleWriter => 5,
            ConsistencyLevel::SnapshotAtFork => 4,
            ConsistencyLevel::PerKeySerializable => 3,
            ConsistencyLevel::CausalCrossRun => 2,
            ConsistencyLevel::StrongEventualUnion => 2,
            ConsistencyLevel::ConflictSetEventual => 1,
        }
    }
}

/// `ConsistencyDeclaration{object | kind-wildcard, level, staleness_bound?,
/// readers ∈ {owner_only, grantor, family}}` — the `consistency_declarations`
/// member's typed form.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsistencyDeclaration {
    /// `object` (an `OwnedObject` JSON) or `kind_wildcard` (one of
    /// `fs | memory | artifact | resource`) — exactly one is set.
    pub object: Option<crate::types::OwnedObject>,
    /// The wildcard kind (`fs | memory | artifact | resource`).
    pub kind_wildcard: Option<String>,
    /// The declared level.
    pub level: ConsistencyLevel,
    /// The declared staleness bound (ms) when the level admits one.
    pub staleness_bound_ms: Option<u64>,
    /// The admitted reader set.
    pub readers: String,
}

/// The `readers` closed set.
pub const CONSISTENCY_READERS: &[&str] = &["owner_only", "grantor", "family"];

/// The `kind_wildcard` closed set.
pub const CONSISTENCY_KINDS: &[&str] = &["fs", "memory", "artifact", "resource"];

impl ConsistencyDeclaration {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = vec![];
        if let Some(o) = &self.object {
            m.push(("object", o.to_json()));
        }
        if let Some(k) = &self.kind_wildcard {
            m.push(("kind_wildcard", Json::str(k)));
        }
        m.push(("level", Json::str(self.level.as_str())));
        if let Some(b) = self.staleness_bound_ms {
            m.push((
                "staleness_bound_ms",
                Json::Int(b.min(i64::MAX as u64) as i64),
            ));
        }
        m.push(("readers", Json::str(&self.readers)));
        Json::obj(m)
    }

    /// From canonical JSON — `None` on any closed-set violation.
    pub fn from_json(j: &Json) -> Option<ConsistencyDeclaration> {
        let object = j
            .get("object")
            .and_then(crate::types::OwnedObject::from_json);
        let kind_wildcard = j
            .get("kind_wildcard")
            .and_then(Json::as_str)
            .map(str::to_string);
        if object.is_none() && kind_wildcard.is_none() {
            return None; // the declaration names an object or a wildcard — never neither
        }
        if object.is_some() && kind_wildcard.is_some() {
            return None;
        }
        if let Some(k) = &kind_wildcard {
            if !CONSISTENCY_KINDS.contains(&k.as_str()) {
                return None;
            }
        }
        let readers = j
            .get("readers")
            .and_then(Json::as_str)
            .unwrap_or("owner_only")
            .to_string();
        if !CONSISTENCY_READERS.contains(&readers.as_str()) {
            return None;
        }
        Some(ConsistencyDeclaration {
            object,
            kind_wildcard,
            level: ConsistencyLevel::parse(j.get("level")?.as_str()?)?,
            staleness_bound_ms: j
                .get("staleness_bound_ms")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64),
            readers,
        })
    }
}

/// K-1 — the declaration-vs-mode consistency check (§5e.5). Returns the
/// violation list (empty = consistent): `share` admits no
/// `snapshot_at_fork` (a shared mutable view has no fork point);
/// `fork_snapshot` admits no `per_key_serializable` (an isolated copy
/// orders nothing per key); `scoped_subtree` admits no
/// `snapshot_at_fork`-only declarations that assume full isolation of the
/// parent (a scoped share is still shared — `snapshot_at_fork` means the
/// whole-tree fork invariant the scoped view cannot honour).
pub fn k1_check(mode: hh_env::handle::DeriveMode, decls: &[ConsistencyDeclaration]) -> Vec<String> {
    use hh_env::handle::DeriveMode;
    let mut out = Vec::new();
    for (i, d) in decls.iter().enumerate() {
        let bad = match (mode, d.level) {
            (DeriveMode::Share, ConsistencyLevel::SnapshotAtFork) => {
                "share is inconsistent with snapshot_at_fork (no fork point exists on a shared view)"
            }
            (DeriveMode::ForkSnapshot, ConsistencyLevel::PerKeySerializable) => {
                "fork_snapshot is inconsistent with per_key_serializable (an isolated copy orders nothing per key)"
            }
            (DeriveMode::ScopedSubtree, ConsistencyLevel::SnapshotAtFork) => {
                "scoped_subtree is inconsistent with snapshot_at_fork (a scoped share has no whole-tree fork point)"
            }
            _ => "",
        };
        if !bad.is_empty() {
            out.push(format!("consistency_declarations[{i}]: {bad}"));
        }
    }
    out
}

/// `CoordinationPolicy{messaging, merge_policies: map<kind, MergePolicy>,
/// declarations, default_isolation, depth_cap, fan_out_cap,
/// on_absent_child}` — MUST-data of the sealed definition (§5e.5; K-4 makes
/// the policy `authority_cap`-layered and tightening-only).
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinationPolicy {
    /// The message-permission data (the `MessagingPolicy` record owns its
    /// own caps).
    pub messaging: crate::types::MessagingPolicy,
    /// `kind → MergePolicy` over the object kinds (`fs | memory | artifact
    /// | resource` keys; unmapped kinds take the parent's
    /// `merge_policy_ref` resolution).
    pub merge_policies: std::collections::BTreeMap<String, crate::types::MergePolicy>,
    /// The declared consistency levels.
    pub declarations: Vec<ConsistencyDeclaration>,
    /// `fork_snapshot (default) | scoped_subtree | share`.
    pub default_isolation: hh_env::handle::DeriveMode,
    /// The delegation-depth cap (`depth_cap`; `delegation_depth + 1 ≤`).
    pub depth_cap: u64,
    /// The live-children cap (`fan_out + 1 ≤`).
    pub fan_out_cap: u64,
    /// `block_completion (default) | annotate`.
    pub on_absent_child: OnAbsentChild,
}

/// `on_absent_child` — what the completion gate does with `absent[]`
/// children (K-4: `annotate` is a loosening).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnAbsentChild {
    /// The gate refuses completion while children are absent (default).
    BlockCompletion,
    /// The gate completes with the absence annotated.
    Annotate,
}

impl Default for OnAbsentChild {
    /// `block_completion` — the spec default (K-4: `annotate` is a
    /// loosening, so the default must be the strict arm).
    fn default() -> Self {
        OnAbsentChild::BlockCompletion
    }
}

impl OnAbsentChild {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OnAbsentChild::BlockCompletion => "block_completion",
            OnAbsentChild::Annotate => "annotate",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<OnAbsentChild> {
        Some(match s {
            "block_completion" => OnAbsentChild::BlockCompletion,
            "annotate" => OnAbsentChild::Annotate,
            _ => return None,
        })
    }
}

/// The K-4 delta classification (the `coordination_delta` direction — kept
/// as a string so `hh-hir` can compare policy payloads without a dep cycle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDelta {
    /// No semantic direction change.
    None,
    /// The policy moved stricter (admissible in evolution).
    Tightening,
    /// The policy relaxed (refused in evolution contexts).
    Loosening,
}

impl PolicyDelta {
    /// The canonical spelling (`hh-hir::diff::Delta`'s, one vocabulary).
    pub fn as_str(self) -> &'static str {
        match self {
            PolicyDelta::None => "none",
            PolicyDelta::Tightening => "tightening",
            PolicyDelta::Loosening => "loosening",
        }
    }
}

impl CoordinationPolicy {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("messaging", self.messaging.to_json()),
            (
                "merge_policies",
                Json::Obj(
                    self.merge_policies
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v.as_str())))
                        .collect(),
                ),
            ),
            (
                "declarations",
                Json::Arr(self.declarations.iter().map(|d| d.to_json()).collect()),
            ),
            (
                "default_isolation",
                Json::str(crate::types::derive_mode_str(self.default_isolation)),
            ),
            (
                "depth_cap",
                Json::Int(self.depth_cap.min(i64::MAX as u64) as i64),
            ),
            (
                "fan_out_cap",
                Json::Int(self.fan_out_cap.min(i64::MAX as u64) as i64),
            ),
            ("on_absent_child", Json::str(self.on_absent_child.as_str())),
        ])
    }

    /// From canonical JSON — `None` on any closed-set violation.
    pub fn from_json(j: &Json) -> Option<CoordinationPolicy> {
        let messaging = j
            .get("messaging")
            .and_then(crate::types::MessagingPolicy::from_json)
            .unwrap_or_default();
        let mut merge_policies = std::collections::BTreeMap::new();
        if let Some(Json::Obj(m)) = j.get("merge_policies") {
            for (k, v) in m {
                let p = crate::types::MergePolicy::parse(v.as_str()?)?;
                merge_policies.insert(k.clone(), p);
            }
        }
        let declarations = j
            .get("declarations")
            .and_then(|d| match d {
                Json::Arr(a) => Some(a.clone()),
                _ => None,
            })
            .unwrap_or_default()
            .iter()
            .map(ConsistencyDeclaration::from_json)
            .collect::<Option<Vec<_>>>()?;
        Some(CoordinationPolicy {
            messaging,
            merge_policies,
            declarations,
            default_isolation: crate::types::parse_derive_mode(
                j.get("default_isolation")
                    .and_then(Json::as_str)
                    .unwrap_or("fork_snapshot"),
            )?,
            depth_cap: j
                .get("depth_cap")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64)
                .unwrap_or(4),
            fan_out_cap: j
                .get("fan_out_cap")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64)
                .unwrap_or(8),
            on_absent_child: OnAbsentChild::parse(
                j.get("on_absent_child")
                    .and_then(Json::as_str)
                    .unwrap_or("block_completion"),
            )?,
        })
    }

    /// K-4 — classify `self → other` (old → new): `Loosening` when the new
    /// policy enables `sibling`/`broadcast` messaging, widens
    /// `default_isolation` toward `share`, raises `depth_cap`/`fan_out_cap`/
    /// `max_pending`, sets `on_absent_child = annotate`, relaxes a
    /// declaration level, or drops a merge policy toward an unordered arm.
    /// Any tightening signal loses to a loosening one (one `loosening`
    /// member classifies the whole diff — the direction is per-diff, never
    /// per-member).
    pub fn delta(&self, other: &CoordinationPolicy) -> PolicyDelta {
        let m = &self.messaging;
        let n = &other.messaging;
        if (!m.sibling && n.sibling) || (!m.broadcast && n.broadcast) {
            return PolicyDelta::Loosening;
        }
        if n.max_pending > m.max_pending {
            return PolicyDelta::Loosening;
        }
        // `default_isolation` ranks shared > scoped > fork (widening toward
        // the shared end loosens).
        let iso_rank = |d: hh_env::handle::DeriveMode| match d {
            hh_env::handle::DeriveMode::Share => 3,
            hh_env::handle::DeriveMode::ScopedSubtree => 2,
            hh_env::handle::DeriveMode::ForkSnapshot => 1,
            hh_env::handle::DeriveMode::FreshFromImage => 0,
        };
        if iso_rank(other.default_isolation) > iso_rank(self.default_isolation) {
            return PolicyDelta::Loosening;
        }
        if other.depth_cap > self.depth_cap || other.fan_out_cap > self.fan_out_cap {
            return PolicyDelta::Loosening;
        }
        if self.on_absent_child == OnAbsentChild::BlockCompletion
            && other.on_absent_child == OnAbsentChild::Annotate
        {
            return PolicyDelta::Loosening;
        }
        // A declaration level that moves *down* the strength ladder loosens;
        // a dropped declaration loosens; an added or strengthened one
        // tightens.
        let mut tightening = false;
        for (i, d) in self.declarations.iter().enumerate() {
            match other.declarations.get(i) {
                None => return PolicyDelta::Loosening,
                Some(o) => {
                    if o.level.strength() < d.level.strength() {
                        return PolicyDelta::Loosening;
                    }
                    if o.level.strength() > d.level.strength() {
                        tightening = true;
                    }
                }
            }
        }
        if other.declarations.len() > self.declarations.len() {
            tightening = true;
        }
        // A merge-policy move toward `single_writer` tightens; toward an
        // unordered/judged arm loosens. Rank: single_writer=4,
        // parent_decides=3, three_way_text=2, validator_selected=1
        // (a judged resolver admits more mergers than a deterministic one).
        let merge_rank = |p: &crate::types::MergePolicy| match p {
            crate::types::MergePolicy::SingleWriter => 4,
            crate::types::MergePolicy::ParentDecides => 3,
            crate::types::MergePolicy::ThreeWayText => 2,
            // `three_way_text{ast}` ranks with `line` — both are diff3
            // conflict-recorders, never side-pickers (the tokenizer
            // member changes collision granularity, not permissiveness).
            crate::types::MergePolicy::ThreeWayTextAst { .. } => 2,
            crate::types::MergePolicy::ValidatorSelected => 1,
        };
        for (k, o) in &self.merge_policies {
            match other.merge_policies.get(k) {
                None => return PolicyDelta::Loosening,
                Some(n) => {
                    if merge_rank(n) < merge_rank(o) {
                        return PolicyDelta::Loosening;
                    }
                    if merge_rank(n) > merge_rank(o) {
                        tightening = true;
                    }
                }
            }
        }
        for k in other.merge_policies.keys() {
            if !self.merge_policies.contains_key(k) {
                tightening = true;
            }
        }
        if tightening {
            PolicyDelta::Tightening
        } else {
            PolicyDelta::None
        }
    }
}
