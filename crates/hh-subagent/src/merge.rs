//! `merge` — the parent's own baselined effects under the one
//! [`MergePolicy`] sum (§5e.3; ADR-0192 D1/D3; M-3; G-1/G-2/G-5).
//!
//! - `single_writer` (`refuse`) — any conflict refuses the merge typed;
//!   only objects the child owned (or newly wrote) merge.
//! - `parent_decides` (`ask`) — conflicts surface as [`MergeConflictRecord`]
//!   rows (`control.merge.resolved` is the resolution row; `choose{side}`/
//!   `supersede{new_ref}` are the parent's `delegate`-class resolutions —
//!   never model-owned; `escalate` is `IllegitimateResolution` here).
//! - `three_way_text{line}` / `validator_selected` — the R-2.6.5 arm,
//!   `MergePolicyUnsupported` (declared, never silent).
//!
//! G-1 (no lost writes): `merged[] ∪ conflicts[] ∪ absent[]` covers every
//! child output; a merge completing with `lost_write_count > 0` vetoes.
//! G-2 (no silent overwrite): every overwrite is either `single_writer`'s
//! typed refusal or a `parent_decides` recorded resolution.
//! G-5: `outcome ≠ result` children land in `absent[]` and contribute
//! nothing to `merged[]`.

use std::collections::BTreeMap;

use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_wire::json::Json;

use crate::ownership::OwnershipTable;
use crate::spawn::{fold_children, kernel_ev_pub};
use crate::types::SpawnError;
use crate::types::*;

/// A merge input — one child's workspace delta as the `control.subagent.
/// result` row's `fs_changes` member gave it: `path → {before, after}`
/// content addresses (`before = None` for a create; `after = None` for a
/// delete). The parent's current versions come from
/// `parent_versions: path → Option<ref>`.
#[derive(Debug, Clone, Default)]
pub struct MergeInput {
    /// The child.
    pub child_run_id: String,
    /// `path → (before, after)`.
    pub changes: BTreeMap<String, (Option<String>, Option<String>)>,
}

/// What `merge` needs — the parent's seam set (everything commits under
/// `parent_lease` on the parent's ledger).
pub struct MergeCtx<'a> {
    /// The store.
    pub store: &'a mut Store,
    /// The parent run.
    pub parent_run_id: &'a str,
    /// The parent's writer lease.
    pub parent_lease: &'a Lease,
    /// The causing decision (`control.decision{kind: delegate}` or the
    /// merge decision row — `causes[]`).
    pub decision: &'a EventRef,
    /// The ownership projection (the `NotOwner` check).
    pub ownerships: &'a OwnershipTable,
    /// The merge policy (from the child's `merge_policy_ref` resolution —
    /// one policy per merge call; mixed policies are C3's orchestration
    /// problem, the kernel takes the resolved one).
    pub policy: MergePolicy,
    /// The children's merge inputs (`fs_changes` per child).
    pub inputs: Vec<MergeInput>,
    /// The parent's current object versions (`path → Option<ref>`; a path
    /// absent from the map = the parent never wrote it).
    pub parent_versions: &'a BTreeMap<String, Option<String>>,
    /// The parent's baseline at fork time (`path → ref` — the
    /// `parent_baseline` the conflict record names).
    pub baseline: &'a BTreeMap<String, String>,
    /// The child's ownership at merge time (`child → objects` — a child
    /// only merges objects it owns or that are unowned-and-new).
    pub child_ownerships: &'a BTreeMap<String, Vec<OwnedObject>>,
}

/// The typed merge outcome — `control.merge.{started, completed}` rows
/// land on the parent's ledger; `resolved` rows follow when the parent
/// decides a `parent_decides` conflict.
#[derive(Debug)]
pub struct MergeOutcome {
    /// The merge id (`control.merge.started`'s event id).
    pub merge_id: String,
    /// The report (completed merges only — `None` on a veto).
    pub report: Option<MergeReport>,
    /// The veto (G-1 lost-write, G-2 unrecorded overwrite, single-writer
    /// conflict, `MergePolicyUnsupported`, `NotOwner`).
    pub veto: Option<MergeVeto>,
}

/// The closed merge-veto sum.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeVeto {
    /// `single_writer` hit a conflict on `path`.
    Conflict { path: String },
    /// A child wrote an object it does not own (the `prepare`-time
    /// `NotOwner` check catches this too — the merge-time recheck is the
    /// parent's final gate).
    NotOwner {
        child_run_id: String,
        object: String,
    },
    /// G-1 — the accounting walked: `merged ∪ conflicts ∪ absent` left
    /// writes unaccounted.
    LostWrite { count: u64 },
    /// G-2 — an overwrite the policy did not admit silently applied.
    SilentOverwrite { path: String },
    /// The declared policy is the R-2.6.5 arm.
    PolicyUnsupported { policy: String },
    /// `escalate` requires the H7 channel (absent at this slice).
    IllegitimateResolution { resolution: String },
}

/// `merge(ctx) → MergeOutcome` — appends `control.merge.started`, folds
/// the children's inputs, and either completes (`control.merge.completed{
/// merge_report_ref}`) or vetoes (the veto is typed, retained; the
/// `started` row's `outcome: refused` marks it).
pub fn merge(ctx: &mut MergeCtx) -> Result<MergeOutcome, SpawnError> {
    if !ctx.policy.implemented() {
        return Ok(MergeOutcome {
            merge_id: String::new(),
            report: None,
            veto: Some(MergeVeto::PolicyUnsupported {
                policy: ctx.policy.as_str().to_string(),
            }),
        });
    }
    // `control.merge.started{merge inputs}` — durable before the fold.
    let children: Vec<String> = ctx.inputs.iter().map(|i| i.child_run_id.clone()).collect();
    let started = kernel_ev_pub(
        ctx.store,
        ctx.parent_run_id,
        "control.merge.started",
        Json::obj([
            (
                "children",
                Json::Arr(children.iter().map(|c| Json::str(c.clone())).collect()),
            ),
            ("policy", Json::str(ctx.policy.as_str())),
            ("parent_run_id", Json::str(ctx.parent_run_id)),
        ]),
        vec![ctx.decision.clone()],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let merge_id = started.event_id.clone();
    ctx.store
        .append(ctx.parent_run_id, ctx.parent_lease, vec![started])
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;

    // ── the fold ─────────────────────────────────────────────────────────
    // Terminal-but-non-result children contribute to `absent[]` (G-5):
    // their `fs_changes` never merge. `result`-armed inputs merge.
    let terminal: BTreeMap<String, String> = fold_children(ctx.store, ctx.parent_run_id)?
        .into_iter()
        .filter_map(|(c, r)| r.terminal_class.map(|t| (c, t)))
        .collect();
    let mut report = MergeReport {
        merge_id: merge_id.clone(),
        parent_run_id: ctx.parent_run_id.to_string(),
        children: children.clone(),
        ..Default::default()
    };
    let mut veto: Option<MergeVeto> = None;
    // First pass — absent bookkeeping (G-5) + NotOwner.
    for input in &ctx.inputs {
        match terminal.get(&input.child_run_id).map(String::as_str) {
            Some("control.subagent.result") | None => {}
            Some(_) => {
                report.absent.push(input.child_run_id.clone());
                continue;
            }
        }
        let owned: &[OwnedObject] = ctx
            .child_ownerships
            .get(&input.child_run_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for path in input.changes.keys() {
            let obj = OwnedObject::FsPathPrefix(path.clone());
            let parent_holds = ctx
                .ownerships
                .owner_of(&obj)
                .map(|o| o == ctx.parent_run_id)
                .unwrap_or(false);
            let child_holds = owned.iter().any(|o| o.covers(&obj));
            if !child_holds && parent_holds {
                veto = Some(MergeVeto::NotOwner {
                    child_run_id: input.child_run_id.clone(),
                    object: path.clone(),
                });
            }
        }
    }
    // Second pass — conflict detection + merge accounting (G-1/G-2).
    let mut applied: BTreeMap<String, Json> = BTreeMap::new();
    if veto.is_none() {
        for input in &ctx.inputs {
            if report.absent.contains(&input.child_run_id) {
                continue;
            }
            for (path, (before, after)) in &input.changes {
                let baseline = ctx.baseline.get(path).cloned();
                let parent_now = ctx.parent_versions.get(path).cloned().flatten();
                // Conflict iff the parent moved since the child's
                // baseline (`before` = the snapshot the child wrote from;
                // `parent_baseline` = the fork point). A parent move the
                // child did not see is a real overlap.
                let parent_moved = parent_now != baseline;
                let child_moved = after.is_some() && after != &baseline;
                if parent_moved && child_moved {
                    match ctx.policy {
                        MergePolicy::SingleWriter => {
                            // The veto is typed — nothing applies.
                            veto = Some(MergeVeto::Conflict { path: path.clone() });
                            report.conflicts.push(MergeConflictRecord {
                                conflict_id: hh_identity::idp::idp_id(
                                    "hh.subagent.conflict",
                                    Json::obj([
                                        ("path", Json::str(path.clone())),
                                        ("child", Json::str(input.child_run_id.clone())),
                                    ])
                                    .to_canonical_string()
                                    .as_bytes(),
                                ),
                                kind: "workspace".into(),
                                path: path.clone(),
                                parent_baseline: baseline.clone(),
                                child_before: before.clone(),
                                child_after: after.clone(),
                                parent_current: parent_now.clone(),
                                child_run_id: input.child_run_id.clone(),
                            });
                        }
                        MergePolicy::ParentDecides => {
                            report.conflicts.push(MergeConflictRecord {
                                conflict_id: hh_identity::idp::idp_id(
                                    "hh.subagent.conflict",
                                    Json::obj([
                                        ("path", Json::str(path.clone())),
                                        ("child", Json::str(input.child_run_id.clone())),
                                    ])
                                    .to_canonical_string()
                                    .as_bytes(),
                                ),
                                kind: "workspace".into(),
                                path: path.clone(),
                                parent_baseline: baseline.clone(),
                                child_before: before.clone(),
                                child_after: after.clone(),
                                parent_current: parent_now.clone(),
                                child_run_id: input.child_run_id.clone(),
                            });
                        }
                        _ => unreachable!("implemented() gate"),
                    }
                } else if child_moved || (after.is_some() && baseline.is_none()) {
                    // Clean merge — the child wrote where the parent did
                    // not move (a create or an untouched object). The
                    // merged row records the application (G-1's coverage).
                    let entry = Json::obj([
                        ("child_run_id", Json::str(input.child_run_id.clone())),
                        ("path", Json::str(path.clone())),
                        (
                            "after_ref",
                            after
                                .as_ref()
                                .map(|a| Json::str(a.clone()))
                                .unwrap_or(Json::Null),
                        ),
                        ("merged_by", Json::str(ctx.policy.as_str())),
                    ]);
                    report.merged.push(entry.clone());
                    applied.insert(path.clone(), entry);
                }
            }
        }
    }
    // G-1 accounting — every child write must be in merged ∪ conflicts ∪
    // absent; a hole means a lost write and the merge vetoes.
    let mut accounted = 0u64;
    for input in &ctx.inputs {
        if report.absent.contains(&input.child_run_id) {
            continue;
        }
        for (path, (_, after)) in &input.changes {
            let in_merged = applied.contains_key(path)
                || report
                    .conflicts
                    .iter()
                    .any(|c| &c.path == path && c.child_run_id == input.child_run_id);
            if after.is_some() && !in_merged {
                accounted += 1;
            }
        }
    }
    report.lost_write_count = accounted;
    if accounted > 0 && veto.is_none() {
        veto = Some(MergeVeto::LostWrite { count: accounted });
    }
    report.provenance = Json::obj([
        ("fold", Json::str("hh-subagent::merge")),
        ("merge_id", Json::str(merge_id.clone())),
    ]);
    report.derived_from = Json::obj([
        ("kind", Json::str("subagent_result")),
        (
            "children",
            Json::Arr(children.iter().map(|c| Json::str(c.clone())).collect()),
        ),
    ]);

    // ── commit ───────────────────────────────────────────────────────────
    // Reports are unbounded evidence rows: persist the canonical record in
    // the blob pool and put only its `idp/1` address in the audit payload
    // (`merge_report_ref` is a real resolvable citation, not a synthetic
    // label). `put_blob` precedes the completed row so the ref never
    // dangles.
    let report_bytes = report.to_json().to_canonical_string().into_bytes();
    let report_addr = ctx
        .store
        .put_blob(&report_bytes, "application/json")
        .map_err(|e| SpawnError::Kernel(format!("merge report blob: {e}")))?;
    let completed = kernel_ev_pub(
        ctx.store,
        ctx.parent_run_id,
        "control.merge.completed",
        Json::obj([
            ("merge_id", Json::str(merge_id.clone())),
            ("merge_report_ref", Json::str(report_addr.id())),
            (
                "outcome",
                Json::str(if veto.is_none() { "merged" } else { "refused" }),
            ),
            (
                "veto",
                match &veto {
                    None => Json::Null,
                    Some(v) => Json::str(merge_veto_str(v)),
                },
            ),
            // `merge_report_ref` resolves through `project_merge_report`
            // to the blob above (ADR-0192 D3 rebuild-equality).
        ]),
        vec![ctx.decision.clone()],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    ctx.store
        .append(ctx.parent_run_id, ctx.parent_lease, vec![completed])
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;

    Ok(MergeOutcome {
        merge_id,
        report: if veto.is_none() { Some(report) } else { None },
        veto,
    })
}

/// The `MergeVeto` spelling (the `completed{outcome: refused}` member).
fn merge_veto_str(v: &MergeVeto) -> &'static str {
    match v {
        MergeVeto::Conflict { .. } => "conflict",
        MergeVeto::NotOwner { .. } => "not_owner",
        MergeVeto::LostWrite { .. } => "lost_write",
        MergeVeto::SilentOverwrite { .. } => "silent_overwrite",
        MergeVeto::PolicyUnsupported { .. } => "policy_unsupported",
        MergeVeto::IllegitimateResolution { .. } => "illegitimate_resolution",
    }
}

/// `resolve_conflict(merge_id, conflict_id, resolution)` — the
/// `control.merge.resolved` row (G-2's recorded resolution). `choose{side}` /
/// `supersede{new_ref}` are the parent's own `delegate`-class effects;
/// `escalate` vetoes `IllegitimateResolution` at this slice.
pub fn resolve_conflict(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    merge_id: &str,
    conflict_id: &str,
    resolution: &MergeResolution,
) -> Result<hh_ledger::event::SeqRange, SpawnError> {
    let res_json = match resolution {
        MergeResolution::Choose { side } => Json::obj([
            ("kind", Json::str("choose")),
            (
                "side",
                Json::str(match side {
                    ChooseSide::Parent => "parent",
                    ChooseSide::Child => "child",
                }),
            ),
        ]),
        MergeResolution::Supersede { new_ref } => Json::obj([
            ("kind", Json::str("supersede")),
            ("new_ref", Json::str(new_ref.clone())),
        ]),
        MergeResolution::Escalate => {
            return Err(SpawnError::Refused(SpawnRefused::ModeUnsupported {
                detail: "resolve{escalate} requires the WS-H7 channel".into(),
            }))
        }
    };
    let ev = kernel_ev_pub(
        store,
        parent_run_id,
        "control.merge.resolved",
        Json::obj([
            ("merge_id", Json::str(merge_id)),
            ("conflict_id", Json::str(conflict_id)),
            ("resolution", res_json),
        ]),
        vec![],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    store
        .append(parent_run_id, parent_lease, vec![ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))
}

/// `project_merge_report(store, parent_run_id, merge_id)` — the ADR-0192
/// D3 `project()` view: rebuilds the `MergeReport` from the durable rows
/// (rebuild-equality is the merge invariant's test seam).
pub fn project_merge_report(
    store: &Store,
    parent_run_id: &str,
    merge_id: &str,
) -> Result<Option<MergeReport>, SpawnError> {
    let events = store
        .events(parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    for e in events.iter().rev() {
        if e.class == "control.merge.completed"
            && e.payload.get("merge_id").and_then(Json::as_str) == Some(merge_id)
        {
            let Some(report_ref) = e.payload.get("merge_report_ref").and_then(Json::as_str) else {
                continue;
            };
            let parsed = hh_identity::idp::parse_id(report_ref)
                .map_err(|e| SpawnError::Kernel(format!("merge_report_ref parse: {e:?}")))?;
            let bytes = store
                .get_blob(&hh_identity::idp::ContentAddress {
                    idp: "idp/1",
                    algorithm: "sha256",
                    digest: parsed.digest_hex,
                    media_type: "application/json".to_string(),
                    size: 0,
                })
                .map_err(|e| SpawnError::Kernel(format!("merge report blob: {e}")))?;
            let text = String::from_utf8(bytes)
                .map_err(|e| SpawnError::Kernel(format!("merge report utf8: {e}")))?;
            let json = hh_wire::json::parse(&text)
                .map_err(|e| SpawnError::Kernel(format!("merge report json: {e}")))?;
            return Ok(report_from_json(&json));
        }
    }
    Ok(None)
}

/// `MergeReport` from its canonical JSON (the `completed.report` member).
pub fn report_from_json(j: &Json) -> Option<MergeReport> {
    let mut r = MergeReport {
        merge_id: j.get("merge_id")?.as_str()?.to_string(),
        parent_run_id: j
            .get("parent_run_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        ..Default::default()
    };
    if let Some(Json::Arr(a)) = j.get("children") {
        r.children = a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect();
    }
    if let Some(Json::Arr(a)) = j.get("merged") {
        r.merged = a.clone();
    }
    if let Some(Json::Arr(a)) = j.get("absent") {
        r.absent = a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect();
    }
    if let Some(Json::Arr(a)) = j.get("conflicts") {
        for c in a {
            let s = |k: &str| c.get(k).and_then(Json::as_str).map(str::to_string);
            r.conflicts.push(MergeConflictRecord {
                conflict_id: s("conflict_id")?,
                kind: s("kind").unwrap_or_else(|| "workspace".into()),
                path: s("path")?,
                parent_baseline: s("parent_baseline"),
                child_before: s("child_before"),
                child_after: s("child_after"),
                parent_current: s("parent_current"),
                child_run_id: s("child_run_id").unwrap_or_default(),
            });
        }
    }
    r.lost_write_count = j
        .get("lost_write_count")
        .and_then(Json::as_int)
        .map(|v| v.max(0) as u64)
        .unwrap_or(0);
    r.provenance = j.get("provenance").cloned().unwrap_or(Json::Null);
    r.derived_from = j.get("derived_from").cloned().unwrap_or(Json::Null);
    Some(r)
}
