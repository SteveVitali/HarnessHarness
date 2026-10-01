//! Label branches — the R-2.8.2 Stage-4 slice (§5g.2 `spawn_branch` /
//! `return_from_branch`; ADR-0054 D1/D4; ADR-0034 P2; AC-R-2.8.2-6).
//!
//! `spawn_branch` is the `spawn` entry under
//! `spec.isolation.context = label_branch`: the child's `ctx₀` is the
//! parent's context label at spawn *stamped by the kernel* (never carried
//! in the spec — the spec is model-visible data), and its clearance is the
//! parent's effective authority — a `ceiling` request above it refuses
//! `ClearanceExceeded`. The `control.subagent.spawned{isolation =
//! label_branch, ctx₀, clearance}` row is the durable record.
//!
//! `return_from_branch` is the only way branch content re-enters the
//! parent:
//!
//! - `abandon` — nothing re-enters; the exit is recorded
//!   (`control.subagent.branch_exited{exit: abandon}`).
//! - `labeled_return` — the returned record re-enters at `external` with
//!   the union taint (`ctx₀.taint ∪ subject.taint`); a `declared` label
//!   more permissive than that floor is `BranchTaintEscaped`. The re-entry
//!   stamps `security.label.applied` with `derived_from` = the branch.
//! - `attested_return(validator_ref)` — the `validator` endorsement: a
//!   capacity-bounded schema raises the record `external → environment`,
//!   `taint → ∅` (`hh_provenance::endorse::shape_endorse`); a free string
//!   is `BasisNotAllowed` and an over-capacity schema is
//!   `ShapeCapacityExceeded`.
//!
//! The parent's `ctx` never changes while a branch is open — the exits are
//! the only re-entry — and child approvals/rulings never transfer (no
//! path exists here; a delegate-class endorser is `IllegitimateEndorsement`
//! at `endorse`, ADR-0034 P2).

use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_provenance::endorse::{self, EndorsementError};
use hh_provenance::flow::Remedy;
use hh_provenance::{AuthorityClass, Label, ProvenanceRecord};
use hh_wire::json::Json;

use crate::spawn::{fold_children, kernel_ev_pub, BranchCtx, SpawnCtx};
use crate::types::{ContextIsolation, SpawnError, SpawnRefused, Spawned, SubagentSpec};

/// The `spawn_branch` kernel entry (§5g.2 `spawn_process{isolation =
/// label_branch}`): forces the context isolation, binds the kernel-stamped
/// `ctx₀`/clearance, and runs the ordinary `spawn` fold — the durable
/// `spawned` row carries `isolation = label_branch` + `ctx0` + `clearance`.
///
/// `ClearanceExceeded` when the spec's `ceiling` request exceeds the
/// parent's effective authority (`clearance`); `BranchContextMissing`
/// never fires here (the inputs are bound before `spawn` runs) — it guards
/// callers who declare `label_branch` on a spec passed to bare `spawn`.
pub fn spawn_branch(
    ctx: &mut SpawnCtx,
    spec: &SubagentSpec,
    ctx0: &Label,
    clearance: AuthorityClass,
) -> Result<Spawned, SpawnError> {
    let mut spec = spec.clone();
    spec.context = ContextIsolation::LabelBranch;
    ctx.branch_ctx = Some(BranchCtx {
        ctx0: ctx0.clone(),
        clearance,
    });
    crate::spawn::spawn(ctx, &spec)
}

/// Consume a `Remedy::Branch{spec}` (§5g.2 §3 `Remedy` closed sum): the
/// remedy's spec string is the canonical `SubagentSpec` JSON; consumption
/// lands the `control.subagent.spawned{isolation = label_branch}` row —
/// every consumed remedy is its own ledger event (§5g.2 §6).
pub fn consume_branch_remedy(
    ctx: &mut SpawnCtx,
    remedy: &Remedy,
    ctx0: &Label,
    clearance: AuthorityClass,
) -> Result<Spawned, SpawnError> {
    let Remedy::Branch { spec } = remedy else {
        return Err(SpawnError::Refused(SpawnRefused::ModeUnsupported {
            detail: format!("remedy.kind = {} is not branch", remedy.kind()),
        }));
    };
    let j = hh_wire::json::parse(spec).map_err(|_| {
        SpawnError::Refused(SpawnRefused::DefinitionUnresolvable {
            detail: "branch remedy spec is not canonical JSON".into(),
        })
    })?;
    let spec = SubagentSpec::from_json(&j).ok_or_else(|| {
        SpawnError::Refused(SpawnRefused::DefinitionUnresolvable {
            detail: "branch remedy spec does not decode as SubagentSpec".into(),
        })
    })?;
    spawn_branch(ctx, &spec, ctx0, clearance)
}

/// A branch exit (§5g.2 contract row — the closed set `abandon |
/// labeled_return | attested_return(validator_ref)`).
#[allow(clippy::large_enum_variant)] // the variants are the spec record shapes — exits are rare, not hot.
#[derive(Debug, Clone)]
pub enum BranchExit {
    /// Nothing re-enters; the exit is recorded.
    Abandon,
    /// `labeled_return` — re-enter `subject` at `external` with union
    /// taint. `declared` is the child's claimed re-entry label when the
    /// channel carries one; a claim more permissive than the computed
    /// floor is `BranchTaintEscaped` (a claim may never drop a required
    /// taint tag, claim authority above `external`, or widen `readers`).
    LabeledReturn {
        /// The returned record's provenance (its label).
        subject: ProvenanceRecord,
        /// The returned record's identity coordinate.
        subject_ref: String,
        /// The claimed re-entry label, when declared.
        declared: Option<Label>,
    },
    /// `attested_return(validator_ref)` — the shape endorsement: a
    /// capacity-bounded schema raises the record `external → environment`
    /// with `taint → ∅`; free text is `BasisNotAllowed` (AC-R-2.8.2-6c).
    AttestedReturn {
        /// The returned record's provenance (its label).
        subject: ProvenanceRecord,
        /// The returned record's identity coordinate.
        subject_ref: String,
        /// The validator's declared output schema (capacity measured by
        /// the kernel — OQ-145's ratified default).
        schema: Json,
        /// The validator's provenance record (the endorser).
        validator: ProvenanceRecord,
        /// The validator ref the exit names.
        validator_ref: String,
        /// The definition's `cap_max` (default 8 — §5g.2 §3).
        cap_max: u64,
    },
}

/// The `return_from_branch` refusal/error sum.
#[derive(Debug)]
pub enum BranchError {
    /// The child is not a label branch (or does not exist under the
    /// parent).
    NotALabelBranch {
        /// The child named.
        child_run_id: String,
    },
    /// The declared re-entry label was more permissive than the exit's
    /// floor — a laundering attempt through the branch (§5g.2 §5).
    BranchTaintEscaped {
        /// Why the declared label escapes.
        detail: String,
    },
    /// The `attested_return` endorsement refused — `BasisNotAllowed`
    /// (free text / unbounded schema), `ShapeCapacityExceeded`, or any
    /// other `EndorsementError` passes through typed.
    Endorsement(EndorsementError),
    /// A kernel-side failure (append/scan).
    Kernel(String),
}

impl std::fmt::Display for BranchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BranchError::NotALabelBranch { child_run_id } => {
                write!(f, "{child_run_id} is not a label_branch child")
            }
            BranchError::BranchTaintEscaped { detail } => {
                write!(f, "BranchTaintEscaped: {detail}")
            }
            BranchError::Endorsement(e) => write!(f, "{e:?}"),
            BranchError::Kernel(d) => write!(f, "kernel: {d}"),
        }
    }
}

impl std::error::Error for BranchError {}

/// The `labeled_return` re-entry floor: `external` authority, `ctx₀.taint
/// ∪ subject.taint`, the subject's `readers` (never widened).
pub fn reentry_label(ctx0: &Label, subject: &ProvenanceRecord) -> Label {
    let s = subject.label();
    Label {
        authority: AuthorityClass::External,
        taint: ctx0.taint.union(&s.taint).cloned().collect(),
        readers: s.readers.clone(),
    }
}

/// `declared ⊑ floor` — the declared label may only be *more* restrictive
/// than the floor: authority ≤ `external`, taint ⊇ the required union,
/// readers ⊆ the subject's. Anything else is `BranchTaintEscaped`.
fn declared_within_floor(declared: &Label, floor: &Label) -> Result<(), String> {
    if declared.authority > AuthorityClass::External {
        return Err(format!(
            "declared authority {} exceeds the external floor",
            declared.authority.as_str()
        ));
    }
    if let Some(missing) = floor.taint.iter().find(|t| !declared.taint.contains(t)) {
        return Err(format!("declared taint drops {}", missing.as_string()));
    }
    if !floor.readers.is_superset_of(&declared.readers) {
        return Err("declared readers widen past the subject's".to_string());
    }
    Ok(())
}

/// `return_from_branch(child, exit)` — the branch's only re-entry
/// (§5g.2; ADR-0054 D4). Returns the `EventRef` of the exit record
/// (`control.subagent.branch_exited`); the re-entry row —
/// `security.label.applied` or `security.label.endorsed` — lands in the
/// same append with `derived_from` naming the branch's `spawned` row.
pub fn return_from_branch(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    child_run_id: &str,
    ctx0: &Label,
    exit: &BranchExit,
) -> Result<EventRef, BranchError> {
    // The child must be a label branch under this parent (the `isolation`
    // member on its `spawned` row is the durable record).
    let children =
        fold_children(store, parent_run_id).map_err(|e| BranchError::Kernel(format!("{e:?}")))?;
    let Some((_, rec)) = children.iter().find(|(c, _)| c == child_run_id) else {
        return Err(BranchError::NotALabelBranch {
            child_run_id: child_run_id.to_string(),
        });
    };
    if rec.spawned.get("isolation").and_then(Json::as_str) != Some("label_branch") {
        return Err(BranchError::NotALabelBranch {
            child_run_id: child_run_id.to_string(),
        });
    }
    // The re-entry's `derived_from` edge — the branch's `spawned` row.
    let branch_ref = EventRef {
        run_id: parent_run_id.to_string(),
        event_id: rec.spawn_event_id.clone(),
    };
    let kernel_prov = ProvenanceRecord::kernel(crate::spawn::KERNEL_SUBAGENT, store.now_ms());

    let mut batch = Vec::new();
    let mut exit_members = vec![
        ("child_run_id", Json::str(child_run_id)),
        ("exit", Json::str(exit_kind(exit))),
    ];
    match exit {
        BranchExit::Abandon => {}
        BranchExit::LabeledReturn {
            subject,
            subject_ref,
            declared,
        } => {
            let floor = reentry_label(ctx0, subject);
            if let Some(d) = declared {
                declared_within_floor(d, &floor)
                    .map_err(|detail| BranchError::BranchTaintEscaped { detail })?;
            }
            // The re-entry lands at the floor — never the claim.
            let applied = endorse::apply_label(subject_ref.clone(), floor, &kernel_prov);
            let ev = kernel_ev_pub(
                store,
                parent_run_id,
                "security.label.applied",
                Json::obj([
                    ("subject_ref", Json::str(applied.subject_ref.clone())),
                    ("label", hh_provenance::label_json_full(&applied.label)),
                ]),
                vec![branch_ref.clone()],
                None,
            )
            .map_err(|e| BranchError::Kernel(e.to_string()))?;
            batch.push(ev);
            exit_members.push(("subject_ref", Json::str(subject_ref.clone())));
            exit_members.push((
                "derived_from",
                Json::str(format!("{child_run_id}:labeled_return")),
            ));
        }
        BranchExit::AttestedReturn {
            subject,
            subject_ref,
            schema,
            validator,
            validator_ref,
            cap_max,
        } => {
            let endorsed = endorse::shape_endorse(
                subject,
                subject_ref.clone(),
                schema,
                *cap_max,
                validator,
                Some(validator_ref.clone()),
                vec![],
            )
            .map_err(BranchError::Endorsement)?;
            let mut payload = endorsed.to_json();
            if let Json::Obj(m) = &mut payload {
                // The documented `{endorser, basis_ref}` members the
                // append-time `check_endorsement` re-verifies (§8.1 #3).
                m.insert("endorser".to_string(), endorsed.endorser.to_json());
                if let Some(b) = &endorsed.basis_ref {
                    m.insert("basis_ref".to_string(), Json::str(b.clone()));
                }
            }
            let mut ev = kernel_ev_pub(
                store,
                parent_run_id,
                "security.label.endorsed",
                payload,
                vec![branch_ref.clone()],
                None,
            )
            .map_err(|e| BranchError::Kernel(e.to_string()))?;
            // `validator` applies only to closed-schema values — the stamp
            // the append-time check re-reads (`content_kind` on the event,
            // never a payload claim).
            ev.content_kind = Some(hh_provenance::ContentKind::ClosedSchemaValue);
            batch.push(ev);
            exit_members.push(("subject_ref", Json::str(subject_ref.clone())));
            exit_members.push(("validator_ref", Json::str(validator_ref.clone())));
            exit_members.push((
                "derived_from",
                Json::str(format!("{child_run_id}:attested_return")),
            ));
        }
    }
    let exit_ev = kernel_ev_pub(
        store,
        parent_run_id,
        "control.subagent.branch_exited",
        Json::Obj(
            exit_members
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        ),
        vec![branch_ref],
        None,
    )
    .map_err(|e| BranchError::Kernel(e.to_string()))?;
    batch.push(exit_ev);
    store
        .append(parent_run_id, parent_lease, batch)
        .map_err(|e| BranchError::Kernel(e.to_string()))?;
    // The branch_exited row is the batch tail.
    let events = store
        .events(parent_run_id)
        .map_err(|e| BranchError::Kernel(e.to_string()))?;
    let ev = events
        .iter()
        .rev()
        .find(|e| {
            e.class == "control.subagent.branch_exited"
                && e.payload.get("child_run_id").and_then(Json::as_str) == Some(child_run_id)
        })
        .ok_or_else(|| BranchError::Kernel("branch_exited row missing after append".into()))?;
    Ok(EventRef {
        run_id: parent_run_id.to_string(),
        event_id: ev.event_id.clone(),
    })
}

fn exit_kind(exit: &BranchExit) -> &'static str {
    match exit {
        BranchExit::Abandon => "abandon",
        BranchExit::LabeledReturn { .. } => "labeled_return",
        BranchExit::AttestedReturn { .. } => "attested_return",
    }
}
