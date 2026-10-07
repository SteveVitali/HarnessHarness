//! `merge` — the parent's own baselined effects under the one
//! [`MergePolicy`] sum (§5e.3; ADR-0192 D1/D3; M-3; G-1/G-2/G-5).
//!
//! - `single_writer` (`refuse`) — any conflict refuses the merge typed;
//!   only objects the child owned (or newly wrote) merge.
//! - `parent_decides` (`ask`) — conflicts surface as [`MergeConflictRecord`]
//!   rows (`control.merge.resolved` is the resolution row; `choose{side}`/
//!   `supersede{new_ref}` are the parent's `delegate`-class resolutions —
//!   never model-owned; `escalate` is `IllegitimateResolution` here).
//! - `three_way_text{line}` (S4.8/R-2.6.5) — line-level diff3 over the
//!   fork-point snapshot: disjoint hunks merge (a new blob the parent
//!   writes), overlapping hunks are `MergeConflict` records (`detector:
//!   deterministic`), never a picked side.
//! - `validator_selected` (S4.8/R-2.6.5) — conflicts go to a bound
//!   [`MergeValidator`] port; a `choose`/`supersede` verdict lands as a
//!   ledgered `control.merge.resolved{resolver: validator}` (G-2 — no
//!   side without a row); an abstention leaves the conflict open; no
//!   bound port is the typed `ValidatorUnavailable` veto, never a
//!   silent pick.
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

use crate::consistency::OnAbsentChild;
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
    /// The S4.8 merge options (`on_absent_child`, the `validator_selected`
    /// port) — `MergeOpts::default()` is the S4.6 behaviour.
    pub opts: MergeOpts<'a>,
}

/// The S4.8 merge options — the `CoordinationPolicy` members `merge`
/// consults (K-4's `on_absent_child` gating and the `validator_selected`
/// invocation port).
#[derive(Default)]
pub struct MergeOpts<'a> {
    /// `block_completion` (default) — `MergeReport::blocks_completion`
    /// reports the gate condition; `annotate` completes with the absence
    /// annotated (the gate's own call — the merge records `absent[]`
    /// faithfully either way, G-5).
    pub on_absent_child: OnAbsentChild,
    /// The `validator_selected` invocation port (ADR-0110's accountable
    /// validator seam — `None` with a declared `validator_selected`
    /// policy is the `ValidatorUnavailable` veto).
    pub validator: Option<&'a dyn MergeValidator>,
    /// The `validator_ref` the `validator_selected{validator_ref}` arm
    /// names (the `verification.validator.*` rows and the
    /// `control.merge.resolved{resolver}` member).
    pub validator_ref: Option<String>,
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
    /// `three_way_text` could not read a side's blob (the fork-point
    /// snapshot is a `ForkPointMissing` refusal — the merge does not
    /// guess at bytes it cannot hash, G-6's pure-function contract).
    ForkPointMissing { path: String },
    /// `validator_selected` declared with no bound port — a veto, never
    /// a silent side-pick (AC-R-2.6.5-2).
    ValidatorUnavailable { validator_ref: Option<String> },
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
                policy: ctx.policy.spelling(),
            }),
        });
    }
    // G-6 determinism — the fold walks children in canonical
    // `child_run_id` order, never in completion/arrival order
    // (`merge_hash` is then order-independent by construction).
    let mut inputs_sorted: Vec<&MergeInput> = ctx.inputs.iter().collect();
    inputs_sorted.sort_by(|a, b| a.child_run_id.cmp(&b.child_run_id));
    // `control.merge.started{merge inputs}` — durable before the fold.
    let children: Vec<String> = inputs_sorted
        .iter()
        .map(|i| i.child_run_id.clone())
        .collect();
    let started = kernel_ev_pub(
        ctx.store,
        ctx.parent_run_id,
        "control.merge.started",
        Json::obj([
            (
                "children",
                Json::Arr(children.iter().map(|c| Json::str(c.clone())).collect()),
            ),
            ("policy", Json::str(ctx.policy.spelling())),
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
    for input in &inputs_sorted {
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
    // `current` is the running parent-state overlay: a `three_way_text`
    // or validator-resolved write updates it so the *next* child (in
    // canonical order) three-ways against the merged head, not the stale
    // `parent_versions` snapshot — sequential pairwise diff3, still a
    // pure function of (child heads, fork-point, policy) (G-6).
    let mut applied: BTreeMap<String, Json> = BTreeMap::new();
    let mut current: BTreeMap<String, Option<String>> = ctx.parent_versions.clone();
    if veto.is_none() {
        for input in &inputs_sorted {
            if report.absent.contains(&input.child_run_id) {
                continue;
            }
            for (path, (before, after)) in &input.changes {
                let baseline = ctx.baseline.get(path).cloned();
                let parent_now = current.get(path).cloned().flatten();
                // Conflict iff the parent moved since the child's
                // baseline (`before` = the snapshot the child wrote from;
                // `parent_baseline` = the fork point). A parent move the
                // child did not see is a real overlap.
                let parent_moved = parent_now != baseline;
                let child_moved = after.is_some() && after != &baseline;
                if parent_moved && child_moved {
                    let conflict = MergeConflictRecord {
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
                        detector: None,
                    };
                    match ctx.policy {
                        MergePolicy::SingleWriter => {
                            // The veto is typed — nothing applies.
                            veto = Some(MergeVeto::Conflict { path: path.clone() });
                            report.conflicts.push(conflict);
                        }
                        MergePolicy::ParentDecides => {
                            report.conflicts.push(conflict);
                        }
                        MergePolicy::ThreeWayText | MergePolicy::ThreeWayTextAst { .. } => {
                            let tok = match &ctx.policy {
                                MergePolicy::ThreeWayTextAst { grammar_ref } => {
                                    TextTokenizer::TaggedCst(grammar_ref.clone())
                                }
                                _ => TextTokenizer::Line,
                            };
                            match three_way_merge_path(
                                ctx.store,
                                path,
                                baseline.as_deref(),
                                parent_now.as_deref(),
                                after.as_deref(),
                                &tok,
                            )? {
                                TextMerge::Merged(new_ref) => {
                                    let entry = Json::obj([
                                        ("child_run_id", Json::str(input.child_run_id.clone())),
                                        ("path", Json::str(path.clone())),
                                        ("after_ref", Json::str(new_ref.clone())),
                                        ("merged_by", Json::str(tok.merged_by())),
                                    ]);
                                    report.merged.push(entry.clone());
                                    applied.insert(path.clone(), entry);
                                    current.insert(path.clone(), Some(new_ref));
                                }
                                TextMerge::Conflicts => {
                                    // Disjoint hunks merged, overlaps are
                                    // the recorded conflict — never a
                                    // picked side (G-2).
                                    let mut c = conflict;
                                    c.detector = Some(tok.detector_label());
                                    report.conflicts.push(c);
                                }
                                TextMerge::MissingBlob => {
                                    veto = Some(MergeVeto::ForkPointMissing { path: path.clone() });
                                }
                            }
                        }
                        MergePolicy::ValidatorSelected => {
                            let mut c = conflict;
                            c.detector = Some(format!(
                                "judged{{{}}}",
                                ctx.opts.validator_ref.as_deref().unwrap_or("?")
                            ));
                            report.conflicts.push(c.clone());
                            let Some(port) = ctx.opts.validator else {
                                veto = Some(MergeVeto::ValidatorUnavailable {
                                    validator_ref: ctx.opts.validator_ref.clone(),
                                });
                                continue;
                            };
                            let verdict = port.select(&c);
                            emit_validator_rows(
                                ctx.store,
                                ctx.parent_run_id,
                                ctx.parent_lease,
                                ctx.opts.validator_ref.as_deref(),
                                &c,
                                &verdict,
                                &merge_id,
                            )?;
                            match verdict {
                                ValidatorMergeVerdict::Abstain => {
                                    // The conflict stays open — an
                                    // abstaining validator resolves
                                    // nothing (no silent pick, AC-2).
                                }
                                v => {
                                    let resolution = match v {
                                        ValidatorMergeVerdict::Choose { side } => {
                                            MergeResolution::Choose { side }
                                        }
                                        ValidatorMergeVerdict::Supersede { new_ref } => {
                                            MergeResolution::Supersede { new_ref }
                                        }
                                        ValidatorMergeVerdict::Coexist => MergeResolution::Coexist,
                                        ValidatorMergeVerdict::Abandon { side } => {
                                            MergeResolution::Abandon { side }
                                        }
                                        ValidatorMergeVerdict::Abstain => unreachable!(),
                                    };
                                    resolve_conflict_as(
                                        ctx.store,
                                        ctx.parent_run_id,
                                        ctx.parent_lease,
                                        &merge_id,
                                        &c.conflict_id,
                                        &resolution,
                                        ctx.opts.validator_ref.as_deref().unwrap_or("validator"),
                                    )?;
                                    let applied_ref = match &resolution {
                                        MergeResolution::Choose {
                                            side: ChooseSide::Child,
                                        } => after.clone(),
                                        _ => parent_now.clone(),
                                    };
                                    if let Some(r) = applied_ref {
                                        let entry = Json::obj([
                                            ("child_run_id", Json::str(input.child_run_id.clone())),
                                            ("path", Json::str(path.clone())),
                                            ("after_ref", Json::str(r.clone())),
                                            ("merged_by", Json::str("validator_selected")),
                                        ]);
                                        report.merged.push(entry.clone());
                                        applied.insert(path.clone(), entry);
                                        current.insert(path.clone(), Some(r));
                                    }
                                }
                            }
                        }
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
                        ("merged_by", Json::str(ctx.policy.spelling())),
                    ]);
                    report.merged.push(entry.clone());
                    applied.insert(path.clone(), entry);
                    if let Some(a) = after {
                        current.insert(path.clone(), Some(a.clone()));
                    }
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
    // G-6 `merge_hash` — the order-independent outcome address: a crash
    // matrix row proves "merge after restore = merge without crash" by
    // comparing this id (AC-R-2.6.5-4; computed over the *outcome*
    // projection — merged refs, conflict ids, absent set — so two
    // different outcomes can never collide).
    report.merge_hash = hh_identity::idp::idp_id(
        "hh.subagent.merge_hash",
        Json::obj([
            ("policy", Json::str(ctx.policy.spelling())),
            (
                "merged",
                Json::Arr(
                    {
                        let mut m: Vec<(String, String)> = report
                            .merged
                            .iter()
                            .map(|e| {
                                (
                                    e.get("path")
                                        .and_then(Json::as_str)
                                        .unwrap_or("")
                                        .to_string(),
                                    e.get("after_ref")
                                        .and_then(Json::as_str)
                                        .unwrap_or("")
                                        .to_string(),
                                )
                            })
                            .collect();
                        m.sort();
                        m
                    }
                    .into_iter()
                    .map(|(p, r)| Json::obj([("path", Json::str(p)), ("after_ref", Json::str(r))]))
                    .collect(),
                ),
            ),
            (
                "conflicts",
                Json::Arr(
                    {
                        let mut c: Vec<String> = report
                            .conflicts
                            .iter()
                            .map(|c| c.conflict_id.clone())
                            .collect();
                        c.sort();
                        c
                    }
                    .into_iter()
                    .map(Json::str)
                    .collect(),
                ),
            ),
            (
                "absent",
                Json::Arr(
                    {
                        let mut a = report.absent.clone();
                        a.sort();
                        a
                    }
                    .into_iter()
                    .map(Json::str)
                    .collect(),
                ),
            ),
        ])
        .to_canonical_string()
        .as_bytes(),
    );
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
            ("merge_hash", Json::str(report.merge_hash.clone())),
            (
                "on_absent_child",
                Json::str(ctx.opts.on_absent_child.as_str()),
            ),
            (
                "absent_children",
                Json::Arr(report.absent.iter().map(|a| Json::str(a.clone())).collect()),
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
        MergeVeto::ForkPointMissing { .. } => "fork_point_missing",
        MergeVeto::ValidatorUnavailable { .. } => "validator_unavailable",
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
    resolve_conflict_as(
        store,
        parent_run_id,
        parent_lease,
        merge_id,
        conflict_id,
        resolution,
        "parent",
    )
}

/// `resolve_conflict_as(…, resolver)` — the `control.merge.resolved` row
/// with the explicit `resolver` member (G-2: the parent for
/// `parent_decides`, the `validator_ref` for a `validator_selected`
/// verdict — every chosen side names who chose it).
pub fn resolve_conflict_as(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    merge_id: &str,
    conflict_id: &str,
    resolution: &MergeResolution,
    resolver: &str,
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
        MergeResolution::Coexist => Json::obj([("kind", Json::str("coexist"))]),
        MergeResolution::Abandon { side } => Json::obj([
            ("kind", Json::str("abandon")),
            (
                "side",
                Json::str(match side {
                    ChooseSide::Parent => "parent",
                    ChooseSide::Child => "child",
                }),
            ),
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
            ("resolver", Json::str(resolver)),
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
                detector: s("detector"),
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
    r.merge_hash = j
        .get("merge_hash")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    Some(r)
}

// ─────────────────────────────────────────────────────────────────────────────
// `three_way_text{line}` / `three_way_text{ast(grammar)}` — the
// tokenizer-parameterized diff3 (§5e.5; ADR-0192 D2; S5.5)
// ─────────────────────────────────────────────────────────────────────────────

/// The textual-merge outcome for one path.
enum TextMerge {
    /// Disjoint hunks merged — `put_blob` minted the new head's address.
    Merged(String),
    /// Overlapping hunks — a `MergeConflict` record, never a pick (G-2).
    Conflicts,
    /// A side's ref is not a readable text blob (fork-point or head
    /// missing → `ForkPointMissing` veto).
    MissingBlob,
}

/// The `three_way_text` tokenizer member — `line` (S4.8) or the
/// registered tagged-CST stream (S5.5, §5e.5's `ast(grammar_ref)`).
/// Carries the grammar ref so the `merged_by`/`detector` row members
/// spell the declared arm verbatim.
enum TextTokenizer {
    /// `three_way_text{line}` — byte lines.
    Line,
    /// `three_way_text{ast(grammar_ref)}` — the tagged-CST stream.
    TaggedCst(String),
}

impl TextTokenizer {
    /// The `merged_by` report member.
    fn merged_by(&self) -> String {
        match self {
            TextTokenizer::Line => "three_way_text{line}".to_string(),
            TextTokenizer::TaggedCst(g) => format!("three_way_text{{ast({g})}}"),
        }
    }

    /// The `MergeConflict.detector` member.
    fn detector_label(&self) -> String {
        match self {
            TextTokenizer::Line => "deterministic{three_way_text}".to_string(),
            TextTokenizer::TaggedCst(_) => "deterministic{three_way_text{ast}}".to_string(),
        }
    }
}

/// `three_way_merge_path(store, path, base_ref, parent_ref, child_ref,
/// tok)` — fetch the three text heads from the blob pool and run the
/// tokenizer's diff3; a clean merge commits the merged text as a parent
/// blob (the merge's own effect, `merged_by` spelling the arm).
fn three_way_merge_path(
    store: &mut Store,
    path: &str,
    base_ref: Option<&str>,
    parent_ref: Option<&str>,
    child_ref: Option<&str>,
    tok: &TextTokenizer,
) -> Result<TextMerge, SpawnError> {
    let fetch = |id: Option<&str>| -> Result<Option<String>, SpawnError> {
        let Some(id) = id else { return Ok(None) };
        let Ok(parsed) = hh_identity::idp::parse_id(id) else {
            return Ok(None); // a non-idp ref cannot name blob bytes
        };
        let addr = hh_identity::idp::ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: parsed.digest_hex,
            media_type: String::new(),
            size: 0,
        };
        match store.get_blob(&addr) {
            Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
            Err(hh_ledger::errors::LedgerError::Missing { .. }) => Ok(None),
            Err(e) => Err(SpawnError::Kernel(format!("blob {id}: {e}"))),
        }
    };
    let (Some(base), Some(parent), Some(child)) =
        (fetch(base_ref)?, fetch(parent_ref)?, fetch(child_ref)?)
    else {
        return Ok(TextMerge::MissingBlob);
    };
    let merged = match tok {
        TextTokenizer::Line => merge_lines(&base, &parent, &child),
        TextTokenizer::TaggedCst(_) => merge_cst(&base, &parent, &child),
    };
    match merged {
        Ok(text) => {
            let addr = store
                .put_blob(text.as_bytes(), "text/plain")
                .map_err(|e| SpawnError::Kernel(format!("merge blob {path}: {e}")))?;
            Ok(TextMerge::Merged(addr.id()))
        }
        Err(_hunks) => Ok(TextMerge::Conflicts),
    }
}

/// `merge_lines(base, ours, theirs) → merged text | conflicted` — the
/// classic diff3 chunked over lines (the `tokenizer ∈ {line}` arm;
/// `ast(grammar_ref)` is a declared policy member, never silently run as
/// `line`). Deterministic: LCS matches are canonically tie-broken, hunks
/// are maximal, and a both-changed-differently hunk is a conflict —
/// `seq`/arrival order never selects a side (G-6).
fn merge_lines(base: &str, ours: &str, theirs: &str) -> Result<String, Vec<String>> {
    let b: Vec<&str> = base.split('\n').collect();
    let o: Vec<&str> = ours.split('\n').collect();
    let t: Vec<&str> = theirs.split('\n').collect();
    merge_seq(&b, &o, &t).map(|v| v.join("\n"))
}

/// `merge_seq(base, ours, theirs) → merged | conflicted` — the classic
/// diff3 chunked over `T` items (the merge algorithm is
/// tokenizer-agnostic — `line` and `ast(grammar_ref)` differ only in the
/// item stream). Deterministic: LCS matches are canonically tie-broken,
/// hunks are maximal, and a both-changed-differently hunk is a conflict
/// — `seq`/arrival order never selects a side (G-6).
fn merge_seq<T: PartialEq + Clone>(b: &[T], o: &[T], t: &[T]) -> Result<Vec<T>, Vec<String>> {
    let mo = lcs_match(b, o);
    let mt = lcs_match(b, t);
    let mut out: Vec<T> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    let (mut ib, mut io, mut it) = (0usize, 0usize, 0usize);
    while ib < b.len() {
        // The next base line matched on *both* sides ends the changed
        // region; everything before it is one hunk.
        let mut j = ib;
        while j < b.len() && !(mo[j].is_some() && mt[j].is_some()) {
            j += 1;
        }
        if j == b.len() {
            // Tail hunk — the rest of all three files.
            let (bs, os, ts) = (&b[ib..], &o[io..], &t[it..]);
            if os == ts {
                out.extend_from_slice(os);
            } else if os == bs {
                out.extend_from_slice(ts);
            } else if ts == bs {
                out.extend_from_slice(os);
            } else {
                conflicts.push(format!("hunk@{}", ib));
            }
            break;
        }
        let oj = mo[j].expect("matched");
        let tj = mt[j].expect("matched");
        let (bs, os, ts) = (&b[ib..j], &o[io..oj], &t[it..tj]);
        if os == ts {
            out.extend_from_slice(os);
        } else if os == bs {
            out.extend_from_slice(ts);
        } else if ts == bs {
            out.extend_from_slice(os);
        } else {
            conflicts.push(format!("hunk@{}", ib));
        }
        out.push(b[j].clone());
        ib = j + 1;
        io = oj + 1;
        it = tj + 1;
    }
    // Base exhausted — trailing insertions from a side that ran ahead of
    // the other's last match surface in the final hunk above (io/it).
    if !conflicts.is_empty() {
        return Err(conflicts);
    }
    Ok(out)
}

/// `lcs_match(a, b) → for each `a` index the matched `b` index` — the
/// longest-common-subsequence monotone matching (classic DP, canonical
/// backtrack: on ties prefer consuming `b` first — deterministic under
/// any input order).
fn lcs_match<T: PartialEq>(a: &[T], b: &[T]) -> Vec<Option<usize>> {
    let (n, m) = (a.len(), b.len());
    // dp[i][j] = lcs length of a[i..], b[j..].
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut out = vec![None; n];
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            out[i] = Some(j);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// `three_way_text{ast(grammar/tagged-cst)}` — the tagged-CST tokenizer (S5.5)
// ─────────────────────────────────────────────────────────────────────────────

/// `CstToken{class, depth, text}` — the `grammar/tagged-cst` stream
/// member: a deterministic partition of the byte stream into tagged
/// spans (no parse, no tree — `depth` is `()[]{}` balance at the
/// token's start). Two regions compare equal only when their tagged
/// streams match exactly, so a comment-only write never collides with a
/// code write sharing the line — the `ast` arm's semantic over `line`.
/// It is still a *token* merge: it does not parse, so two safe-ordering
/// insertions can interleave into text a later parser would reject; the
/// merged bytes land as the merge's own effect (a semantic merge that
/// parses and rewrites is a later slice).
#[derive(Debug, Clone, PartialEq)]
struct CstToken {
    /// The class — `open|close|comment|str|ident|num|punct|ws`.
    class: &'static str,
    /// The `()[]{}` balance at the token's start (open → emitted at the
    /// pre-depth then incremented; close → decremented first,
    /// saturating at 0 — matched pairs compare at equal depth).
    depth: u32,
    /// The source bytes.
    text: String,
}

/// `tokenize_tagged_cst(src) → Vec<CstToken>` — the registered
/// `grammar/tagged-cst` tokenizer (canonical, byte-deterministic, total):
///
/// - `//` … up to (excluding) the next `\n` is a `comment` span;
/// - `"` … `"` with `\` escapes is a `str` span (an unterminated quote
///   runs to EOF — tokenization never fails);
/// - `[0-9]+` is `num`; `[A-Za-z_][A-Za-z0-9_]*` is `ident`;
/// - `(` `[` `{` are `open`, `)` `]` `}` are `close` (depth saturates);
/// - whitespace runs are `ws`; every other byte is a single-byte `punct`.
fn tokenize_tagged_cst(src: &str) -> Vec<CstToken> {
    let b = src.as_bytes();
    let mut out: Vec<CstToken> = Vec::new();
    let mut depth: u32 = 0;
    let mut i = 0usize;
    let push = |out: &mut Vec<CstToken>, class: &'static str, depth: u32, lo: usize, hi: usize| {
        out.push(CstToken {
            class,
            depth,
            text: String::from_utf8_lossy(&src.as_bytes()[lo..hi]).into_owned(),
        });
    };
    while i < b.len() {
        let c = b[i];
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            // `//` line comment — the newline stays a `ws` token.
            let start = i;
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            push(&mut out, "comment", depth, start, i);
        } else if c == b'"' {
            // `"` … `"` with `\` escapes; unterminated runs to EOF.
            let start = i;
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i += 2;
                } else if b[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            push(&mut out, "str", depth, start, i.min(b.len()));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            push(&mut out, "num", depth, start, i);
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            push(&mut out, "ident", depth, start, i);
        } else if c == b'(' || c == b'[' || c == b'{' {
            push(&mut out, "open", depth, i, i + 1);
            depth = depth.saturating_add(1);
            i += 1;
        } else if c == b')' || c == b']' || c == b'}' {
            depth = depth.saturating_sub(1);
            push(&mut out, "close", depth, i, i + 1);
            i += 1;
        } else if c.is_ascii_whitespace() {
            let start = i;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            push(&mut out, "ws", depth, start, i);
        } else {
            push(&mut out, "punct", depth, i, i + 1);
            i += 1;
        }
    }
    out
}

/// `merge_cst(base, ours, theirs)` — the `ast` arm's diff3 over the
/// tagged-CST stream; a clean merge re-concatenates token bytes (the
/// token partition is a cover — the merged text is always well-formed
/// bytes, though not necessarily well-formed syntax).
fn merge_cst(base: &str, ours: &str, theirs: &str) -> Result<String, Vec<String>> {
    let b = tokenize_tagged_cst(base);
    let o = tokenize_tagged_cst(ours);
    let t = tokenize_tagged_cst(theirs);
    merge_seq(&b, &o, &t).map(|v| v.iter().map(|x| x.text.as_str()).collect::<String>())
}

/// `tokenize_tagged_cst` exposed for the test seam.
#[cfg(test)]
pub(crate) fn tokenize_tagged_cst_pub(src: &str) -> Vec<(String, u32, String)> {
    tokenize_tagged_cst(src)
        .into_iter()
        .map(|t| (t.class.to_string(), t.depth, t.text))
        .collect()
}

/// `merge_cst` exposed for the test seam.
#[cfg(test)]
pub(crate) fn merge_cst_pub(base: &str, ours: &str, theirs: &str) -> Result<String, Vec<String>> {
    merge_cst(base, ours, theirs)
}

// ─────────────────────────────────────────────────────────────────────────────
// `validator_selected` — the accountable-validator port (§5e.5; ADR-0110/0192)
// ─────────────────────────────────────────────────────────────────────────────

/// The `validator_selected` invocation port — the caller binds an
/// accountable validator (the same `verification.validator.*` evidence
/// class ADR-0110 owns). The port *selects*, never writes: a verdict is
/// authoritative only through the ledgered `control.merge.resolved` row
/// the merge emits from it (ADR-0192 G-2 — a judged detector may create
/// a conflict and never resolve one without that row).
pub trait MergeValidator {
    /// `select(conflict) → verdict` — called once per open conflict under
    /// `validator_selected`; `Abstain` leaves the conflict open.
    fn select(&self, conflict: &MergeConflictRecord) -> ValidatorMergeVerdict;
}

/// The port's verdict vocabulary (a subset of `MergeResolution` plus the
/// honest no-answer — an abstention is not a pick).
#[derive(Debug, Clone, PartialEq)]
pub enum ValidatorMergeVerdict {
    /// `choose{side}` — pick the parent or the child head.
    Choose { side: ChooseSide },
    /// `supersede{new_ref}` — a new value supersedes both sides.
    Supersede { new_ref: String },
    /// `coexist` — both versions stand (no merge output; conflict
    /// closes resolved).
    Coexist,
    /// `abandon{side}` — retire one side.
    Abandon { side: ChooseSide },
    /// No verdict — the conflict stays open (never a silent pick).
    Abstain,
}

/// `emit_validator_rows(…)` — the `verification.validator.invoked` +
/// `verification.validator.verdict` pair for one `validator_selected`
/// conflict (the evidence rows that make the port's answer auditable;
/// `charged_to = subject` — merge-side validator cost is the parent's).
fn emit_validator_rows(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    validator_ref: Option<&str>,
    conflict: &MergeConflictRecord,
    verdict: &ValidatorMergeVerdict,
    merge_id: &str,
) -> Result<(), SpawnError> {
    let vref = validator_ref.unwrap_or("validator").to_string();
    let invoked = kernel_ev_pub(
        store,
        parent_run_id,
        "verification.validator.invoked",
        Json::obj([
            ("validator_ref", Json::str(vref.clone())),
            ("merge_id", Json::str(merge_id)),
            ("conflict_id", Json::str(conflict.conflict_id.clone())),
            ("object", Json::str(conflict.path.clone())),
            ("phase", Json::str("merge")),
            ("detector", Json::str("judged")),
            ("charged_to", Json::str("subject")),
        ]),
        vec![],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let verdict_str = match verdict {
        ValidatorMergeVerdict::Choose { side } => match side {
            ChooseSide::Parent => "choose{parent}",
            ChooseSide::Child => "choose{child}",
        },
        ValidatorMergeVerdict::Supersede { .. } => "supersede",
        ValidatorMergeVerdict::Coexist => "coexist",
        ValidatorMergeVerdict::Abandon { .. } => "abandon",
        ValidatorMergeVerdict::Abstain => "abstain",
    };
    let verdict_ev = kernel_ev_pub(
        store,
        parent_run_id,
        "verification.validator.verdict",
        Json::obj([
            ("validator_ref", Json::str(vref)),
            ("merge_id", Json::str(merge_id)),
            ("conflict_id", Json::str(conflict.conflict_id.clone())),
            ("verdict", Json::str(verdict_str)),
            ("charged_to", Json::str("subject")),
        ]),
        vec![],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    store
        .append(parent_run_id, parent_lease, vec![invoked, verdict_ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    Ok(())
}

/// `merge_lines` / `three_way_merge_path` exposed for the test seam (the
/// merge engine is pure — the crash-matrix rebuild asserts the same
/// `merge_hash` on re-fold, and the line engine itself is unit-testable).
#[cfg(test)]
pub(crate) fn merge_lines_pub(base: &str, ours: &str, theirs: &str) -> Result<String, Vec<String>> {
    merge_lines(base, ours, theirs)
}

#[cfg(test)]
mod ast_tests {
    use super::{merge_cst_pub, tokenize_tagged_cst_pub};

    /// The tokenizer partitions into the documented classes at the
    /// documented depths — canonical, byte-deterministic.
    #[test]
    fn tagged_cst_tokenizer_classes_and_depth() {
        let toks = tokenize_tagged_cst_pub("fn f() { // hi\n  let x = 42;\n}");
        let classes: Vec<&str> = toks.iter().map(|t| t.0.as_str()).collect();
        assert!(classes.contains(&"comment"));
        assert!(classes.contains(&"num"));
        assert!(classes.contains(&"open"));
        assert!(classes.contains(&"close"));
        assert!(classes.contains(&"ident"));
        // `fn`/`f`/`()` sit at depth 0; `let` inside the braces at 1.
        let let_tok = toks.iter().find(|t| t.2 == "let").expect("let token");
        assert_eq!(let_tok.1, 1);
        // Re-concatenating the token bytes is a lossless cover.
        let joined: String = toks.iter().map(|t| t.2.clone()).collect();
        assert_eq!(joined, "fn f() { // hi\n  let x = 42;\n}");
    }

    /// The AC-R-2.6.5-3 semantic: a comment-only write on the parent
    /// side does not collide with the child's code write on the same
    /// line — `ast` merges where `line` would conflict.
    #[test]
    fn ast_comment_vs_code_same_line_merges() {
        let base = "fn f() { work(); }\n";
        let parent = "fn f() { /* note */ work(); }\n";
        let child = "fn f() { work(); log(); }\n";
        let merged = merge_cst_pub(base, parent, child).expect("ast merge");
        assert_eq!(merged, "fn f() { /* note */ work(); log(); }\n");
        // Under `line` this is a conflict (both sides rewrote the line).
        assert!(super::merge_lines_pub(base, parent, child).is_err());
    }

    /// Both sides rewriting the *same* token is still a typed conflict —
    /// the ast tokenizer narrows collision granularity, never picks a
    /// side.
    #[test]
    fn ast_same_token_conflict_records() {
        let base = "let x = 1;\n";
        let parent = "let x = 2;\n";
        let child = "let x = 3;\n";
        assert!(merge_cst_pub(base, parent, child).is_err());
    }

    /// Disjoint code edits at the same depth merge — the stream carries
    /// nesting so inserts inside different scopes do not collide.
    #[test]
    fn ast_disjoint_scope_inserts_merge() {
        let base = "fn a() { }\nfn b() { }\n";
        let parent = "fn a() { pa(); }\nfn b() { }\n";
        let child = "fn a() { }\nfn b() { pb(); }\n";
        let merged = merge_cst_pub(base, parent, child).expect("ast merge");
        assert_eq!(merged, "fn a() { pa(); }\nfn b() { pb(); }\n");
    }
}

#[cfg(test)]
mod line_engine_tests {
    use super::merge_lines_pub;

    #[test]
    fn disjoint_hunks_merge_both_sides() {
        let base = "l1\nl2\nl3\nl4\nl5\n";
        let ours = "L1\nl2\nl3\nl4\nl5\n";
        let theirs = "l1\nl2\nl3\nl4\nL5\n";
        let out = merge_lines_pub(base, ours, theirs).expect("disjoint merges");
        assert!(out.contains("L1") && out.contains("L5"), "{out}");
        assert!(out.contains("l2") && out.contains("l4"));
    }

    #[test]
    fn overlapping_hunks_conflict_never_pick() {
        let base = "a\nb\nc\n";
        let ours = "a\nOURS\nc\n";
        let theirs = "a\nTHEIRS\nc\n";
        let conflicts = merge_lines_pub(base, ours, theirs).unwrap_err();
        assert_eq!(conflicts.len(), 1, "one overlapping hunk, no picked side");
    }

    #[test]
    fn identical_edits_merge_cleanly() {
        let base = "a\nb\n";
        let same = "a\nSAME\n";
        let out = merge_lines_pub(base, same, same).expect("same edit merges");
        assert_eq!(out, "a\nSAME\n");
    }

    #[test]
    fn one_sided_edits_take_the_edited_side() {
        let base = "a\nb\n";
        let edited = "a\nEDITED\n";
        let out = merge_lines_pub(base, edited, base).expect("their edit wins untouched side");
        assert_eq!(out, "a\nEDITED\n");
        let out = merge_lines_pub(base, base, edited).expect("our edit wins untouched side");
        assert_eq!(out, "a\nEDITED\n");
    }
}
