//! The `SurfacePreset` table — MUST-data (§7.1 §2.4/§3; ADR-0168 D3).
//!
//! A preset is a *surface lowering*, never a kernel semantic: each row
//! maps one precedent-style mode spelling onto the one `approval_mode`
//! IR parameter plus Π narrowing leaves. The kernel keeps the identity —
//! `approval_mode` is what `open_session` carries, the preset name is a
//! surface fact (T-LCD-10: it never enters `configuration_id`).
//!
//! Rules the table encodes (ADR-0168 D3; §5g.7 I-P1):
//! - `plan`-class ⇒ `pre_approved_only` + `fs_write`/`exec` deny leaves —
//!   the run may only do what a sealed pre-authorization already granted.
//! - `auto-edit`-class ⇒ `tiered` + an `allow` lease pattern over
//!   `fs_write ∧ workspace_local` — workspace-local writes lease after the
//!   first human approval; everything else still tiers to the principal.
//! - `bypass` ⇒ `bypass` + `requires_containment` — admitted only when
//!   the environment binding supplies `enforcement_evidence` the kernel
//!   can record (`BypassWithoutContainment` otherwise); the constitutional
//!   never-auto set is excluded from every preset, this one included —
//!   its prompts cannot be suppressed.
//!
//! Leaves are *narrowing-only* declarations (`authority_cap`-bounded,
//! §7.1 §4 labels): they may lower a Π verdict, never raise one. They
//! enter the lease key's `policy_fingerprint` leg kernel-side, so a
//! preset change revokes every minted lease by construction (ADR-0071).

use hh_wire::json::Json;

/// A leaf's Π disposition — the closed sum the table admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafDisposition {
    /// The matched domain resolves `deny` outright.
    Deny,
    /// The matched domain resolves `ask` (the human stage).
    Ask,
    /// The matched domain is leaseable — a human `allow_lease` mints an
    /// `ActionPattern` lease covering it.
    AllowLease,
}

impl LeafDisposition {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LeafDisposition::Deny => "deny",
            LeafDisposition::Ask => "ask",
            LeafDisposition::AllowLease => "allow_lease",
        }
    }
}

/// One Π narrowing leaf a preset declares — `{domain, disposition,
/// scope?}`. `domain` is an `EffectDomain` spelling (`fs_write`, `exec`,
/// `net_egress`, `spawn_process`, `message_human`, …); `scope` qualifies
/// it (`workspace_local` — the writable-roots meet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NarrowingLeaf {
    /// The effect domain the leaf narrows.
    pub domain: &'static str,
    /// The disposition the leaf assigns.
    pub disposition: LeafDisposition,
    /// The scope qualifier (`workspace_local`), when the leaf binds one.
    pub scope: Option<&'static str>,
}

impl NarrowingLeaf {
    /// The wire/canonical form — `{domain, disposition, scope?}`.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("domain".into(), Json::str(self.domain));
        m.insert("disposition".into(), Json::str(self.disposition.as_str()));
        if let Some(s) = self.scope {
            m.insert("scope".into(), Json::str(s));
        }
        Json::Obj(m)
    }

    /// `leaf_id` — `H("narrowing_leaf" ∥ canonical(leaf))`; the member the
    /// kernel folds into `policy_fingerprint`'s narrowing-leaf leg.
    pub fn leaf_id(&self) -> String {
        hh_identity::idp::idp_id(
            "narrowing_leaf",
            self.to_json().to_canonical_string().as_bytes(),
        )
    }
}

/// `SurfacePreset{name, approval_mode, narrowing_leaves[],
/// requires_containment}` — the MUST-data row (§7.1 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfacePreset {
    /// The surface spelling (`--preset <name>`; `bypass` also binds the
    /// long `--bypass` switch — long on purpose, hidden from completion).
    pub name: &'static str,
    /// The `approval_mode` value the preset lowers to.
    pub approval_mode: &'static str,
    /// The Π narrowing leaves the preset declares.
    pub narrowing_leaves: &'static [NarrowingLeaf],
    /// `true` ⇒ the preset is admitted only when the environment binding
    /// supplies kernel-minted `enforcement_evidence` (ADR-0136; the
    /// `BypassWithoutContainment` gate).
    pub requires_containment: bool,
}

/// The `SurfacePreset` table — MUST-data; growing it is a spec change
/// (OQ-246's close grows the lease rows, ADR-0168 (d)).
pub const SURFACE_PRESETS: &[SurfacePreset] = &[
    SurfacePreset {
        name: "plan",
        approval_mode: "pre_approved_only",
        narrowing_leaves: &[
            NarrowingLeaf {
                domain: "fs_write",
                disposition: LeafDisposition::Deny,
                scope: None,
            },
            NarrowingLeaf {
                domain: "exec",
                disposition: LeafDisposition::Deny,
                scope: None,
            },
        ],
        requires_containment: false,
    },
    SurfacePreset {
        name: "auto-edit",
        approval_mode: "tiered",
        narrowing_leaves: &[NarrowingLeaf {
            domain: "fs_write",
            disposition: LeafDisposition::AllowLease,
            scope: Some("workspace_local"),
        }],
        requires_containment: false,
    },
    SurfacePreset {
        name: "bypass",
        approval_mode: "bypass",
        // No leaves: `bypass` is a mode, not a narrowing — the never-auto
        // set is excluded inside the kernel chain, not declared here.
        narrowing_leaves: &[],
        requires_containment: true,
    },
];

/// `preset(name) → Option<&SurfacePreset>` — the table lookup; an unknown
/// spelling is the caller's `invocation_error` (never coerced).
pub fn preset(name: &str) -> Option<&'static SurfacePreset> {
    SURFACE_PRESETS.iter().find(|p| p.name == name)
}

/// The preset names, table order (`config explain` / help render it).
pub fn preset_names() -> Vec<&'static str> {
    SURFACE_PRESETS.iter().map(|p| p.name).collect()
}

/// `environment_supplies_containment_evidence(connection_info) → bool` —
/// the surface's declared answer to "does this environment binding carry
/// kernel-minted `enforcement_evidence`". Only the kernel-provisioned
/// `local_host`/`local` class does at this stage (the reference backend's
/// attach report mints probed/reported evidence for the relied-on field
/// groups); every other class — and a `ref` the surface cannot inspect —
/// answers `false`, so the bypass gate fails closed *before* a run opens.
/// The kernel re-checks at `open_session` — this row is the pre-ledger
/// UX gate, never the authority (§7.1 §2.4 fault row).
pub fn environment_supplies_containment_evidence(environment: &Json) -> bool {
    match environment.get("kind").and_then(Json::as_str) {
        Some("connection_info") => matches!(
            environment
                .get("connection_info")
                .and_then(|c| c.get("class"))
                .and_then(Json::as_str),
            Some("local_host") | Some("local")
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_the_spec_shape() {
        let plan = preset("plan").unwrap();
        assert_eq!(plan.approval_mode, "pre_approved_only");
        assert!(!plan.requires_containment);
        assert_eq!(
            plan.narrowing_leaves
                .iter()
                .map(|l| (l.domain, l.disposition))
                .collect::<Vec<_>>(),
            vec![
                ("fs_write", LeafDisposition::Deny),
                ("exec", LeafDisposition::Deny)
            ]
        );
        let edit = preset("auto-edit").unwrap();
        assert_eq!(edit.approval_mode, "tiered");
        assert_eq!(edit.narrowing_leaves.len(), 1);
        let bypass = preset("bypass").unwrap();
        assert_eq!(bypass.approval_mode, "bypass");
        assert!(bypass.requires_containment);
        assert!(preset("yolo").is_none());
    }

    #[test]
    fn leaf_ids_are_canonical_and_distinct() {
        let a = NarrowingLeaf {
            domain: "fs_write",
            disposition: LeafDisposition::Deny,
            scope: None,
        };
        let b = NarrowingLeaf {
            domain: "exec",
            disposition: LeafDisposition::Deny,
            scope: None,
        };
        assert_ne!(a.leaf_id(), b.leaf_id());
        assert_eq!(a.leaf_id(), a.leaf_id());
        assert!(a.leaf_id().starts_with("b3:") || !a.leaf_id().is_empty());
    }

    #[test]
    fn containment_evidence_gate_fails_closed() {
        let local = Json::obj([
            ("kind", Json::str("connection_info")),
            (
                "connection_info",
                Json::obj([("class", Json::str("local_host"))]),
            ),
        ]);
        assert!(environment_supplies_containment_evidence(&local));
        let foreign = Json::obj([
            ("kind", Json::str("connection_info")),
            (
                "connection_info",
                Json::obj([("class", Json::str("container_x"))]),
            ),
        ]);
        assert!(!environment_supplies_containment_evidence(&foreign));
        let refr = Json::obj([("kind", Json::str("ref")), ("ref", Json::str("env:x"))]);
        assert!(!environment_supplies_containment_evidence(&refr));
    }
}
