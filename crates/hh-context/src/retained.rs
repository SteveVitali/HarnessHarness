//! The durable retained selection (`retained_selection/1`) and its
//! deterministic replay — R5.18; the REQ-E-206 contributing slice
//! (§2.3: "Persist retained/omitted item/content identities and legal
//! tool/result, batch and required-group joins; rebuild from this
//! durable selection").
//!
//! [`RetainedSelection`] rides the existing
//! `context.compaction.completed` row as the additive `selection`
//! member — the producer mints, the run's fenced writer lands (I-02,
//! CC3); no new event class, codec or store. Its `input` member,
//! [`SelectionInput`], is the shared input representation the
//! prepared call (the sealed-request leg) consumes: the recorded
//! policy/parameters plus the flattened view rows carrying the
//! pair/batch/required joins the selection was judged against —
//! everything a rebuild needs, no ledger read.
//!
//! [`replay_selection`] rebuilds that input and re-runs the recorded
//! variant's `propose`/`execute` — the same production path, never a
//! second selection implementation or a parallel context
//! representation. [`verify_selection`] reports [`SelectionVerdict`]:
//! `Reproduced` only on exact set/outcome equality, `Diverged` on a
//! mismatch, or a typed [`SelectionRefusal`] — never a silent
//! reconcile (the ADR-0135 §3 replay posture).

use std::collections::{BTreeMap, BTreeSet};

use hharness_hir::records::Validity;
use hharness_provenance::authority::{AuthorityClass, PersistenceScope, ReaderSet};
use hharness_provenance::label::Label;
use hharness_provenance::origin::Origin;
use hharness_provenance::record::ProvenanceRecord;
use hharness_wire::json::Json;

use crate::codec::{arr_at, expect_obj, int_at, reject_unknown, str_at, CodecError};
use crate::compact::{
    assess, evict_oldest_propose, execute, ClearToolResults, CompactInput, CompactedView,
    CompactionStatus, CompactionStrategy, CompactionTrigger, Requirement, CLEAR_TOOL_RESULTS_REF,
    EVICT_OLDEST_REF,
};
use crate::plan::{Candidate, ContextPlan, CutPoint, DerivedFrom, Estimate, PlannedItem, SlotFill};
use crate::vocab::{CandidateKind, CandidateState, PriorityClass, Retention};

/// The `selection` member's schema marker on `context.compaction.completed`.
pub const RETAINED_SELECTION_SCHEMA: &str = "retained_selection/1";

/// The variant refs the retained-selection record admits (REQ-E-206's
/// frozen native bindings — `evict_oldest` and `clear_tool_results`).
const ADMITTED_VARIANTS: &[&str] = &[EVICT_OLDEST_REF, CLEAR_TOOL_RESULTS_REF];

/// The tool join an item carries — `Origin::Tool{capability,
/// invocation_ref}` reduced to the members the completion-age join
/// reads (a `clear_tool_results` selection is only auditable when the
/// join is persisted with the view row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolJoin {
    /// The tool capability that produced the item.
    pub capability: String,
    /// The `action.tool.*` invocation the result answers.
    pub invocation_ref: String,
}

/// One flattened input row the selection joined over — the
/// item/content identity plus the pair/batch/required-group joins
/// (REQ-E-206's "legal tool/result, batch and required-group joins").
/// `candidate_id: None` marks a kernel-authored row (never evictable).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionItem {
    /// The delivered `ContextItem` identity (content address).
    pub context_item_id: String,
    /// The admitted candidate identity — `None` for kernel-authored
    /// items (the omission placeholder itself).
    pub candidate_id: Option<String>,
    /// The slot the item sat in (flat order is slot order).
    pub slot_id: String,
    /// The recorded candidate retention — the required-group join.
    /// `None` on a kernel-authored row.
    pub retention: Option<Retention>,
    /// The candidate's ledger-order `source_seq` (evict_oldest's age).
    pub source_seq: u64,
    /// The plan-counted tokens (the reclaim input).
    pub tokens: u64,
    /// I-PAIR — the `candidate_id` this item is indivisible with
    /// (the tool call/result pair).
    pub paired_with: Option<String>,
    /// I-ATOM — the batch this item moves atomically with.
    pub batch_id: Option<String>,
    /// The tool join, when the item is tool-result sourced.
    pub tool: Option<ToolJoin>,
}

/// The recorded policy — the admitted variant plus its exact bound
/// parameters (`clear_tool_results` records `{capabilities[],
/// older_than, completed_invocations{invocation_ref → seq}}`;
/// `evict_oldest` records `{}` — its order is contract-fixed).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionPolicy {
    /// The variant whose proposal applied (or the last attempted).
    pub variant_ref: String,
    /// The exact parameters the variant consumed (opaque here; the
    /// replay interprets them under `variant_ref` only).
    pub parameters: Json,
}

/// `SelectionInput` — the shared input representation the prepared
/// call consumes: the recorded policy, the assessed envelope
/// (trigger/cap/needed/fraction/occupancy), the legal cut points and
/// the flattened join rows. Rebuild requires no ledger read.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionInput {
    /// The admitted policy + exact parameters.
    pub policy: SelectionPolicy,
    /// The recorded trigger — `None` on a record that omits the member
    /// (a distinct [`SelectionRefusal::TriggerAbsent`], never a
    /// fabricated default).
    pub trigger: Option<CompactionTrigger>,
    /// The context window cap the trigger measured against.
    pub window_cap: u64,
    /// The reactive reclaim floor (`needed`).
    pub needed: u64,
    /// OQ-197's target fraction (ppm — integer-canonical).
    pub target_fraction_ppm: u64,
    /// The plan's occupancy the assessment measured.
    pub occupancy: u64,
    /// `legal_cut_points` — the `before_index` values.
    pub cut_points: Vec<u64>,
    /// The flattened view rows (flat order).
    pub items: Vec<SelectionItem>,
}

/// `RetainedSelection` — the persisted `selection` member: the shared
/// input plus the recorded selection output.
#[derive(Debug, Clone, PartialEq)]
pub struct RetainedSelection {
    /// The shared input representation.
    pub input: SelectionInput,
    /// The retained `context_item_id`s in flat order.
    pub retained: Vec<String>,
    /// The omitted `context_item_id`s (the forgotten set).
    pub omitted: Vec<String>,
    /// The recorded ladder outcome for this rung.
    pub status: CompactionStatus,
    /// The ladder position that applied.
    pub pipeline_index: u64,
    /// The `fallback_variant` the record carries, when it ran.
    pub fallback_variant: Option<String>,
}

/// The replayed selection — the deterministic re-derivation plus the
/// recorded fallback leg (kept distinct, never folded into the kind).
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayedSelection {
    /// The recomputed outcome kind.
    pub outcome: SelectionOutcome,
    /// The recorded `fallback_variant`, carried through.
    pub via_fallback: Option<String>,
    /// The recomputed omitted ids (the executed `forgotten` set).
    pub omitted: Vec<String>,
    /// The recomputed retained ids (flat order).
    pub retained: Vec<String>,
}

/// The distinct selection outcome kinds — trigger absence,
/// ineffectiveness and fallback stay explicit (AC-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionOutcome {
    /// The assessment required nothing — no selection ran (all
    /// retained; the untriggered result stays distinct from `Applied`).
    Untriggered,
    /// The recorded variant's selection applied.
    Applied,
    /// The ops ran but the reclaim fell short of the requirement
    /// (the ladder continues; this rung stayed `ineffective`).
    Ineffective,
    /// The recorded rung could not produce a legal selection.
    Failed,
}

/// The typed refusals the replay keeps distinct (AC-2 — a malformed
/// or illegal record never degrades to a recompute or a pass).
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionRefusal {
    /// `policy.variant_ref` is not an admitted binding.
    UnknownPolicy {
        /// The recorded ref.
        variant_ref: String,
    },
    /// A `context_item_id`/`candidate_id` appears twice in the input.
    DuplicateIds {
        /// The repeated id.
        id: String,
    },
    /// An omitted row is required-retention or kernel-authored
    /// (the protected group cannot be evicted).
    ProtectedGroup {
        /// The protected item.
        item_id: String,
    },
    /// A pair join dangles (names an absent candidate) or the recorded
    /// selection split a `paired_with` group.
    OrphanPair {
        /// The item whose pair join is broken.
        item_id: String,
    },
    /// The recorded selection split a `batch_id` group.
    OrphanBatch {
        /// The item whose batch join is broken.
        item_id: String,
    },
    /// `clear_tool_results`: a tool item inside the declared
    /// capabilities has no `completed_invocations` entry — the
    /// completion-age join is missing, never defaulted.
    MissingCompletionAge {
        /// The item missing the join.
        item_id: String,
    },
    /// The record carries no `trigger` member.
    TriggerAbsent,
    /// An `omitted` id names no input row.
    UnknownItem {
        /// The unknown id.
        item_id: String,
    },
    /// The record is malformed (bad member/type/shape).
    Malformed {
        /// What failed decode.
        detail: String,
    },
}

impl std::fmt::Display for SelectionRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectionRefusal::UnknownPolicy { variant_ref } => {
                write!(f, "UnknownPolicy{{{variant_ref}}}")
            }
            other => write!(f, "{other:?}"),
        }
    }
}
impl std::error::Error for SelectionRefusal {}

/// The replay verdict — `Reproduced` only on exact equality; a typed
/// refusal or a named divergence otherwise.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionVerdict {
    /// The durable selection reproduces exactly.
    Reproduced {
        /// The recomputed outcome kind.
        outcome: SelectionOutcome,
        /// The recorded fallback leg, kept distinct.
        via_fallback: Option<String>,
    },
    /// The record is malformed or illegal — the typed refusal.
    Refused(SelectionRefusal),
    /// The recompute disagrees with the record — never reconciled.
    Diverged {
        /// What diverged.
        detail: String,
    },
}

fn status_str(s: CompactionStatus) -> &'static str {
    match s {
        CompactionStatus::Applied => "applied",
        CompactionStatus::Ineffective => "ineffective",
        CompactionStatus::Failed => "failed",
    }
}

fn status_parse(s: &str) -> Option<CompactionStatus> {
    match s {
        "applied" => Some(CompactionStatus::Applied),
        "ineffective" => Some(CompactionStatus::Ineffective),
        "failed" => Some(CompactionStatus::Failed),
        _ => None,
    }
}

fn retention_json(r: Retention) -> Json {
    match r {
        Retention::Required => Json::str("required"),
        Retention::Optional(p) => Json::obj([("optional", Json::str(p.as_str()))]),
    }
}

fn retention_parse(j: &Json) -> Option<Retention> {
    match j {
        Json::Str(s) if s == "required" => Some(Retention::Required),
        Json::Obj(m) => m
            .get("optional")
            .and_then(Json::as_str)
            .and_then(PriorityClass::parse)
            .map(Retention::Optional),
        _ => None,
    }
}

fn trigger_json(t: &CompactionTrigger) -> Json {
    match t {
        CompactionTrigger::OccupancyHard => Json::obj([("kind", Json::str("occupancy_hard"))]),
        CompactionTrigger::OccupancySoft { rule_id } => Json::obj([
            ("kind", Json::str("occupancy_soft")),
            ("rule_id", Json::str(rule_id.clone())),
        ]),
        CompactionTrigger::OverflowReactive { error_ref } => Json::obj([
            ("kind", Json::str("overflow_reactive")),
            ("error_ref", Json::str(error_ref.clone())),
        ]),
        CompactionTrigger::RequestPrincipal => {
            Json::obj([("kind", Json::str("request_principal"))])
        }
        CompactionTrigger::RequestModel { tool_call_id } => Json::obj([
            ("kind", Json::str("request_model")),
            ("tool_call_id", Json::str(tool_call_id.clone())),
        ]),
        CompactionTrigger::Schedule { rule_id } => Json::obj([
            ("kind", Json::str("schedule")),
            ("rule_id", Json::str(rule_id.clone())),
        ]),
        CompactionTrigger::Relower { dropped_items } => Json::obj([
            ("kind", Json::str("relower")),
            (
                "dropped_items",
                Json::Arr(dropped_items.iter().map(|d| Json::str(d.clone())).collect()),
            ),
        ]),
    }
}

fn trigger_parse(j: &Json) -> Result<CompactionTrigger, CodecError> {
    const REC: &str = "retained_selection/1.trigger";
    let m = expect_obj(j, REC)?;
    let known = [
        "kind",
        "rule_id",
        "error_ref",
        "tool_call_id",
        "dropped_items",
    ];
    reject_unknown(m, &known, REC)?;
    match str_at(m, "kind", REC)? {
        "occupancy_hard" => Ok(CompactionTrigger::OccupancyHard),
        "occupancy_soft" => Ok(CompactionTrigger::OccupancySoft {
            rule_id: str_at(m, "rule_id", REC)?.to_string(),
        }),
        "overflow_reactive" => Ok(CompactionTrigger::OverflowReactive {
            error_ref: str_at(m, "error_ref", REC)?.to_string(),
        }),
        "request_principal" => Ok(CompactionTrigger::RequestPrincipal),
        "request_model" => Ok(CompactionTrigger::RequestModel {
            tool_call_id: str_at(m, "tool_call_id", REC)?.to_string(),
        }),
        "schedule" => Ok(CompactionTrigger::Schedule {
            rule_id: str_at(m, "rule_id", REC)?.to_string(),
        }),
        "relower" => Ok(CompactionTrigger::Relower {
            dropped_items: arr_at(m, "dropped_items", REC)?
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
        }),
        other => Err(CodecError::BadMember {
            member: other.to_string(),
            record: REC,
        }),
    }
}

fn item_json(it: &SelectionItem) -> Json {
    let mut v = vec![
        ("context_item_id", Json::str(it.context_item_id.clone())),
        ("slot_id", Json::str(it.slot_id.clone())),
        ("source_seq", Json::Int(it.source_seq as i64)),
        ("tokens", Json::Int(it.tokens as i64)),
    ];
    if let Some(c) = &it.candidate_id {
        v.push(("candidate_id", Json::str(c.clone())));
    }
    if let Some(r) = it.retention {
        v.push(("retention", retention_json(r)));
    }
    if let Some(p) = &it.paired_with {
        v.push(("paired_with", Json::str(p.clone())));
    }
    if let Some(b) = &it.batch_id {
        v.push(("batch_id", Json::str(b.clone())));
    }
    if let Some(t) = &it.tool {
        v.push((
            "tool",
            Json::obj([
                ("capability", Json::str(t.capability.clone())),
                ("invocation_ref", Json::str(t.invocation_ref.clone())),
            ]),
        ));
    }
    Json::obj(v)
}

fn item_parse(j: &Json) -> Result<SelectionItem, CodecError> {
    const REC: &str = "retained_selection/1.items[]";
    let m = expect_obj(j, REC)?;
    let known = [
        "context_item_id",
        "candidate_id",
        "slot_id",
        "retention",
        "source_seq",
        "tokens",
        "paired_with",
        "batch_id",
        "tool",
    ];
    reject_unknown(m, &known, REC)?;
    let retention = match m.get("retention") {
        None => None,
        Some(j) => Some(retention_parse(j).ok_or_else(|| CodecError::TypeMismatch {
            member: "retention".to_string(),
            expected: "\"required\" | {optional: <priority>}",
        })?),
    };
    let tool = match m.get("tool") {
        None => None,
        Some(j) => {
            let t = expect_obj(j, REC)?;
            reject_unknown(t, &["capability", "invocation_ref"], REC)?;
            Some(ToolJoin {
                capability: str_at(t, "capability", REC)?.to_string(),
                invocation_ref: str_at(t, "invocation_ref", REC)?.to_string(),
            })
        }
    };
    Ok(SelectionItem {
        context_item_id: str_at(m, "context_item_id", REC)?.to_string(),
        candidate_id: m
            .get("candidate_id")
            .and_then(Json::as_str)
            .map(str::to_string),
        slot_id: str_at(m, "slot_id", REC)?.to_string(),
        retention,
        source_seq: int_at(m, "source_seq", REC)?.max(0) as u64,
        tokens: int_at(m, "tokens", REC)?.max(0) as u64,
        paired_with: m
            .get("paired_with")
            .and_then(Json::as_str)
            .map(str::to_string),
        batch_id: m.get("batch_id").and_then(Json::as_str).map(str::to_string),
        tool,
    })
}

fn malformed(e: CodecError) -> SelectionRefusal {
    SelectionRefusal::Malformed {
        detail: e.to_string(),
    }
}

impl RetainedSelection {
    /// Mint the durable selection from a compaction rung — the
    /// flattened input joins plus the executed view's forgotten set.
    /// `parameters` is the applied variant's
    /// [`CompactionStrategy::selection_parameters`] output.
    pub fn mint(
        input: &CompactInput,
        view: &CompactedView,
        status: CompactionStatus,
        variant_ref: &str,
        parameters: Json,
        pipeline_index: u64,
        fallback_variant: Option<String>,
    ) -> RetainedSelection {
        let flat = input.flattened();
        let omitted: BTreeSet<&str> = view.forgotten.iter().map(String::as_str).collect();
        let items: Vec<SelectionItem> = flat
            .iter()
            .map(|f| {
                let tool = f
                    .candidate
                    .as_ref()
                    .and_then(|c| match &c.provenance.origin {
                        Origin::Tool {
                            capability,
                            invocation_ref,
                            ..
                        } => Some(ToolJoin {
                            capability: capability.clone(),
                            invocation_ref: invocation_ref.clone(),
                        }),
                        _ => None,
                    });
                SelectionItem {
                    context_item_id: f.item.context_item_id.clone(),
                    candidate_id: f.candidate.as_ref().map(|c| c.candidate_id.clone()),
                    slot_id: f.slot_id.clone(),
                    retention: f.candidate.as_ref().map(|c| c.retention),
                    source_seq: f.candidate.as_ref().map(|c| c.source_seq).unwrap_or(0),
                    tokens: f.item.tokens,
                    paired_with: f.candidate.as_ref().and_then(|c| c.paired_with.clone()),
                    batch_id: f.candidate.as_ref().and_then(|c| c.batch_id.clone()),
                    tool,
                }
            })
            .collect();
        RetainedSelection {
            input: SelectionInput {
                policy: SelectionPolicy {
                    variant_ref: variant_ref.to_string(),
                    parameters,
                },
                trigger: Some(input.trigger.clone()),
                window_cap: input.window_cap,
                needed: input.needed,
                target_fraction_ppm: input.target_fraction_ppm,
                occupancy: input.plan.occupancy_estimate,
                cut_points: input
                    .plan
                    .legal_cut_points
                    .iter()
                    .map(|c| c.before_index)
                    .collect(),
                items,
            },
            retained: flat
                .iter()
                .filter(|f| !omitted.contains(f.item.context_item_id.as_str()))
                .map(|f| f.item.context_item_id.clone())
                .collect(),
            omitted: view.forgotten.clone(),
            status,
            pipeline_index,
            fallback_variant,
        }
    }

    /// The canonical `selection` member — integer-canonical (no
    /// floats), closed shape.
    pub fn to_json(&self) -> Json {
        let i = &self.input;
        let mut v = vec![
            ("schema", Json::str(RETAINED_SELECTION_SCHEMA)),
            (
                "input",
                Json::obj([
                    (
                        "policy",
                        Json::obj([
                            ("variant_ref", Json::str(i.policy.variant_ref.clone())),
                            ("parameters", i.policy.parameters.clone()),
                        ]),
                    ),
                    (
                        "trigger",
                        i.trigger.as_ref().map(trigger_json).unwrap_or(Json::Null),
                    ),
                    ("window_cap", Json::Int(i.window_cap as i64)),
                    ("needed", Json::Int(i.needed as i64)),
                    (
                        "target_fraction_ppm",
                        Json::Int(i.target_fraction_ppm as i64),
                    ),
                    ("occupancy", Json::Int(i.occupancy as i64)),
                    (
                        "cut_points",
                        Json::Arr(i.cut_points.iter().map(|c| Json::Int(*c as i64)).collect()),
                    ),
                    ("items", Json::Arr(i.items.iter().map(item_json).collect())),
                ]),
            ),
            (
                "retained",
                Json::Arr(self.retained.iter().map(|r| Json::str(r.clone())).collect()),
            ),
            (
                "omitted",
                Json::Arr(self.omitted.iter().map(|o| Json::str(o.clone())).collect()),
            ),
            ("status", Json::str(status_str(self.status))),
            ("pipeline_index", Json::Int(self.pipeline_index as i64)),
        ];
        if let Some(f) = &self.fallback_variant {
            v.push(("fallback_variant", Json::str(f.clone())));
        }
        Json::obj(v)
    }

    /// The strict decode — closed shape; unknown members, bad types
    /// and a `schema` mismatch are `Malformed`. An absent `trigger`
    /// decodes `None` (the replay's distinct `TriggerAbsent`, not a
    /// decode error).
    pub fn from_json(j: &Json) -> Result<RetainedSelection, SelectionRefusal> {
        const REC: &str = "retained_selection/1";
        let m = expect_obj(j, REC).map_err(malformed)?;
        reject_unknown(
            m,
            &[
                "schema",
                "input",
                "retained",
                "omitted",
                "status",
                "pipeline_index",
                "fallback_variant",
            ],
            REC,
        )
        .map_err(malformed)?;
        if str_at(m, "schema", REC).map_err(malformed)? != RETAINED_SELECTION_SCHEMA {
            return Err(SelectionRefusal::Malformed {
                detail: format!("schema: expected {RETAINED_SELECTION_SCHEMA}"),
            });
        }
        let im = expect_obj(
            m.get("input").unwrap_or(&Json::Null),
            "retained_selection/1.input",
        )
        .map_err(malformed)?;
        const IREC: &str = "retained_selection/1.input";
        reject_unknown(
            im,
            &[
                "policy",
                "trigger",
                "window_cap",
                "needed",
                "target_fraction_ppm",
                "occupancy",
                "cut_points",
                "items",
            ],
            IREC,
        )
        .map_err(malformed)?;
        let pm = expect_obj(
            im.get("policy").unwrap_or(&Json::Null),
            "retained_selection/1.input.policy",
        )
        .map_err(malformed)?;
        const PREC: &str = "retained_selection/1.input.policy";
        reject_unknown(pm, &["variant_ref", "parameters"], PREC).map_err(malformed)?;
        let trigger = match im.get("trigger") {
            None | Some(Json::Null) => None,
            Some(j) => Some(trigger_parse(j).map_err(malformed)?),
        };
        let uint = |m: &BTreeMap<String, Json>, k: &'static str| -> Result<u64, SelectionRefusal> {
            int_at(m, k, IREC)
                .map(|v| v.max(0) as u64)
                .map_err(malformed)
        };
        Ok(RetainedSelection {
            input: SelectionInput {
                policy: SelectionPolicy {
                    variant_ref: str_at(pm, "variant_ref", PREC)
                        .map_err(malformed)?
                        .to_string(),
                    parameters: pm
                        .get("parameters")
                        .cloned()
                        .unwrap_or_else(|| Json::obj([])),
                },
                trigger,
                window_cap: uint(im, "window_cap")?,
                needed: uint(im, "needed")?,
                target_fraction_ppm: uint(im, "target_fraction_ppm")?,
                occupancy: uint(im, "occupancy")?,
                cut_points: arr_at(im, "cut_points", IREC)
                    .map_err(malformed)?
                    .iter()
                    .filter_map(Json::as_int)
                    .map(|i| i.max(0) as u64)
                    .collect(),
                items: arr_at(im, "items", IREC)
                    .map_err(malformed)?
                    .iter()
                    .map(item_parse)
                    .collect::<Result<_, _>>()
                    .map_err(malformed)?,
            },
            retained: arr_at(m, "retained", REC)
                .map_err(malformed)?
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            omitted: arr_at(m, "omitted", REC)
                .map_err(malformed)?
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            status: status_parse(str_at(m, "status", REC).map_err(malformed)?).ok_or_else(
                || SelectionRefusal::Malformed {
                    detail: "status: expected applied|ineffective|failed".to_string(),
                },
            )?,
            pipeline_index: int_at(m, "pipeline_index", REC)
                .map(|v| v.max(0) as u64)
                .map_err(malformed)?,
            fallback_variant: m
                .get("fallback_variant")
                .and_then(Json::as_str)
                .map(str::to_string),
        })
    }
}

/// The recorded `parameters` members a `clear_tool_results` policy
/// carries — decoded for the completion-age join check and the
/// variant reconstruction.
fn ctr_parameters(
    params: &Json,
) -> Result<(BTreeSet<String>, u64, BTreeMap<String, u64>), SelectionRefusal> {
    const REC: &str = "retained_selection/1.input.policy.parameters";
    let m = expect_obj(params, REC).map_err(malformed)?;
    reject_unknown(
        m,
        &["capabilities", "older_than", "completed_invocations"],
        REC,
    )
    .map_err(malformed)?;
    let capabilities = arr_at(m, "capabilities", REC)
        .map_err(malformed)?
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_string)
        .collect();
    let older_than = int_at(m, "older_than", REC)
        .map(|v| v.max(0) as u64)
        .map_err(malformed)?;
    let completed_invocations = match m.get("completed_invocations") {
        None | Some(Json::Null) => BTreeMap::new(),
        Some(j) => expect_obj(j, REC)
            .map_err(malformed)?
            .iter()
            .filter_map(|(k, v)| v.as_int().map(|i| (k.clone(), i.max(0) as u64)))
            .collect(),
    };
    Ok((capabilities, older_than, completed_invocations))
}

/// Structural validation — the AC-2 distinct refusals, checked before
/// any recompute so an illegal record never degrades to a verdict.
fn validate(rec: &RetainedSelection) -> Result<(), SelectionRefusal> {
    let input = &rec.input;
    // Duplicate ids — a flattened view never repeats an identity.
    let mut seen_items: BTreeSet<&str> = BTreeSet::new();
    let mut seen_candidates: BTreeSet<&str> = BTreeSet::new();
    for it in &input.items {
        if !seen_items.insert(it.context_item_id.as_str()) {
            return Err(SelectionRefusal::DuplicateIds {
                id: it.context_item_id.clone(),
            });
        }
        if let Some(c) = &it.candidate_id {
            if !seen_candidates.insert(c.as_str()) {
                return Err(SelectionRefusal::DuplicateIds { id: c.clone() });
            }
        }
    }
    // The trigger the assessment ran under must be recorded.
    if input.trigger.is_none() {
        return Err(SelectionRefusal::TriggerAbsent);
    }
    // The policy must be one of the frozen native bindings.
    if !ADMITTED_VARIANTS.contains(&input.policy.variant_ref.as_str()) {
        return Err(SelectionRefusal::UnknownPolicy {
            variant_ref: input.policy.variant_ref.clone(),
        });
    }
    // Every omitted id names an input row; required/kernel rows are
    // the protected group — they can never leave the view.
    for id in &rec.omitted {
        let it = input
            .items
            .iter()
            .find(|i| i.context_item_id == *id)
            .ok_or_else(|| SelectionRefusal::UnknownItem {
                item_id: id.clone(),
            })?;
        if it.candidate_id.is_none() || it.retention == Some(Retention::Required) {
            return Err(SelectionRefusal::ProtectedGroup {
                item_id: id.clone(),
            });
        }
    }
    let omitted: BTreeSet<&str> = rec.omitted.iter().map(String::as_str).collect();
    // The pair/batch joins: `paired_with` names a present candidate;
    // a selection may not split an indivisible group across
    // retained/omitted.
    let side_of = |candidate_id: &str| -> Option<bool> {
        input.items.iter().find_map(|i| {
            if i.candidate_id.as_deref() == Some(candidate_id) {
                Some(omitted.contains(i.context_item_id.as_str()))
            } else {
                None
            }
        })
    };
    let mut batches: BTreeMap<&str, bool> = BTreeMap::new();
    for it in &input.items {
        let my_side = omitted.contains(it.context_item_id.as_str());
        if let Some(p) = &it.paired_with {
            let partner = side_of(p).ok_or_else(|| SelectionRefusal::OrphanPair {
                item_id: it.context_item_id.clone(),
            })?;
            if partner != my_side {
                return Err(SelectionRefusal::OrphanPair {
                    item_id: it.context_item_id.clone(),
                });
            }
        }
        if let Some(b) = &it.batch_id {
            match batches.insert(b.as_str(), my_side) {
                Some(side) if side != my_side => {
                    return Err(SelectionRefusal::OrphanBatch {
                        item_id: it.context_item_id.clone(),
                    });
                }
                _ => {}
            }
        }
    }
    // `clear_tool_results`: every tool-sourced item inside the declared
    // capabilities must carry the completion-age join — a missing join
    // is a distinct refusal, never an untargeted read.
    if input.policy.variant_ref == CLEAR_TOOL_RESULTS_REF {
        let (capabilities, _, completed) = ctr_parameters(&input.policy.parameters)?;
        for it in &input.items {
            if let Some(t) = &it.tool {
                if capabilities.contains(&t.capability)
                    && !completed.contains_key(&t.invocation_ref)
                {
                    return Err(SelectionRefusal::MissingCompletionAge {
                        item_id: it.context_item_id.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Rebuild the plan + candidate set the record names — the shared
/// representation carries only the joins the selection reads
/// (labels/authorities do not participate in the evict-family
/// selection and are minted at the kernel floor). The caller owns
/// both for the reconstructed `CompactInput`'s borrow.
fn rebuild_plan(input: &SelectionInput) -> (ContextPlan, BTreeMap<String, Candidate>) {
    let mut slots: Vec<SlotFill> = Vec::new();
    let mut candidates: BTreeMap<String, Candidate> = BTreeMap::new();
    for it in &input.items {
        if slots.last().map(|s| s.slot_id.as_str()) != Some(it.slot_id.as_str()) {
            slots.push(SlotFill {
                slot_id: it.slot_id.clone(),
                items: Vec::new(),
            });
        }
        let label = Label::top();
        slots.last_mut().unwrap().items.push(PlannedItem {
            candidate_id: it
                .candidate_id
                .clone()
                .unwrap_or_else(|| format!("kernel:{}", it.context_item_id)),
            context_item_id: it.context_item_id.clone(),
            artefact_id: None,
            delivery_id: format!("del:{}", it.context_item_id),
            authority: AuthorityClass::Delegate,
            label: label.clone(),
            tokens: it.tokens,
            state: CandidateState::Expanded,
            delivered_by_reference: false,
            derived_from: None,
        });
        if let Some(c) = &it.candidate_id {
            let provenance = match &it.tool {
                Some(t) => ProvenanceRecord {
                    origin: Origin::tool(t.capability.clone(), t.invocation_ref.clone()),
                    authority: AuthorityClass::Delegate,
                    taint: BTreeSet::new(),
                    readers: ReaderSet::Public,
                    scope: PersistenceScope::Run,
                    derived_from: Vec::new(),
                    created_at: 0,
                    attestation: None,
                },
                None => ProvenanceRecord::kernel("kernel:compact", 0),
            };
            candidates.insert(
                c.clone(),
                Candidate {
                    candidate_id: c.clone(),
                    context_item_id: Some(it.context_item_id.clone()),
                    kind: CandidateKind::Observation,
                    state: CandidateState::Expanded,
                    retention: it.retention.unwrap_or(Retention::Required),
                    estimate: Estimate {
                        tokens: it.tokens,
                        estimator_ref: "est/retained-selection".to_string(),
                    },
                    source_event: None,
                    source_seq: it.source_seq,
                    label,
                    provenance,
                    validity: Validity::open_from(0),
                    readers: None,
                    slot_hint: None,
                    volatile: false,
                    paired_with: it.paired_with.clone(),
                    batch_id: it.batch_id.clone(),
                    artefact_id: None,
                    handle: None,
                },
            );
        }
    }
    let plan = ContextPlan {
        plan_id: "plan:retained-selection".into(),
        model_call_id: String::new(),
        derived_from: DerivedFrom {
            run_id: String::new(),
            seq: 0,
            view_hash: String::new(),
        },
        slots,
        omitted: vec![],
        context_label: Label::top(),
        occupancy_estimate: input.occupancy,
        legal_cut_points: input
            .cut_points
            .iter()
            .map(|b| CutPoint { before_index: *b })
            .collect(),
        reserved: 0,
        estimator_ref: "est/retained-selection".into(),
        static_hash: String::new(),
    };
    (plan, candidates)
}

/// The reconstructed `CompactInput` over an owned plan/candidate set —
/// the borrows the rebuild functions hand back.
fn rebuild_cinput<'a>(
    input: &SelectionInput,
    trigger: &CompactionTrigger,
    plan: &'a ContextPlan,
    candidates: BTreeMap<String, Candidate>,
) -> CompactInput<'a> {
    CompactInput {
        plan,
        candidates,
        window_cap: input.window_cap,
        trigger: trigger.clone(),
        needed: input.needed,
        target_fraction_ppm: input.target_fraction_ppm,
        scope: PersistenceScope::Run,
        at: 0,
        run_id: String::new(),
        summarizer: None,
        slot_min_authority: BTreeMap::new(),
        item_texts: BTreeMap::new(),
        item_kinds: BTreeMap::new(),
        extractor: None,
        provider: None,
        previous_summary_ref: None,
    }
}

/// `replay_selection(record)` — rebuild the recorded input and re-run
/// the recorded variant's deterministic selection (`propose` →
/// `execute`), producing the recomputed omitted/retained sets and the
/// outcome kind. Typed refusals for the AC-2 negatives; a proposal/
/// execute failure folds to `Failed` — the rung, not the record.
pub fn replay_selection(rec: &RetainedSelection) -> Result<ReplayedSelection, SelectionRefusal> {
    validate(rec)?;
    let input = &rec.input;
    let trigger = input.trigger.clone().expect("validated");
    let (plan, candidates) = rebuild_plan(input);
    let cinput = rebuild_cinput(input, &trigger, &plan, candidates);
    let assessment = assess(
        &trigger,
        input.occupancy,
        input.window_cap,
        input.needed,
        input.target_fraction_ppm,
    );
    let retained_of = |omitted: &BTreeSet<String>| -> Vec<String> {
        input
            .items
            .iter()
            .filter(|i| !omitted.contains(&i.context_item_id))
            .map(|i| i.context_item_id.clone())
            .collect()
    };
    if assessment.requirement == Requirement::None
        || (assessment.target_reclaim == 0 && assessment.min_reclaim == 0)
    {
        return Ok(ReplayedSelection {
            outcome: SelectionOutcome::Untriggered,
            via_fallback: rec.fallback_variant.clone(),
            omitted: vec![],
            retained: retained_of(&BTreeSet::new()),
        });
    }
    let proposal = match input.policy.variant_ref.as_str() {
        EVICT_OLDEST_REF => evict_oldest_propose(&cinput, &assessment),
        CLEAR_TOOL_RESULTS_REF => {
            let (capabilities, older_than, completed_invocations) =
                ctr_parameters(&input.policy.parameters)?;
            ClearToolResults {
                capabilities,
                older_than,
                completed_invocations,
            }
            .propose(&cinput, &assessment)
        }
        other => {
            return Err(SelectionRefusal::UnknownPolicy {
                variant_ref: other.to_string(),
            })
        }
    };
    let proposal = match proposal {
        Ok(p) => p,
        Err(_) => {
            return Ok(ReplayedSelection {
                outcome: SelectionOutcome::Failed,
                via_fallback: rec.fallback_variant.clone(),
                omitted: vec![],
                retained: retained_of(&BTreeSet::new()),
            })
        }
    };
    match execute(&cinput, &proposal) {
        Ok(view) => {
            let applied = match assessment.requirement {
                Requirement::Hard => view.tokens_freed >= assessment.min_reclaim,
                _ => view.tokens_freed > 0 || proposal.ops.is_empty(),
            };
            let omitted_set: BTreeSet<String> = view.forgotten.iter().cloned().collect();
            Ok(ReplayedSelection {
                outcome: if applied {
                    SelectionOutcome::Applied
                } else {
                    SelectionOutcome::Ineffective
                },
                via_fallback: rec.fallback_variant.clone(),
                omitted: view.forgotten.clone(),
                retained: retained_of(&omitted_set),
            })
        }
        Err(_) => Ok(ReplayedSelection {
            outcome: SelectionOutcome::Failed,
            via_fallback: rec.fallback_variant.clone(),
            omitted: vec![],
            retained: retained_of(&BTreeSet::new()),
        }),
    }
}

/// `verify_selection(record)` — the AC-1 claim: the replayed prefix
/// reproduces the recorded selection. `Reproduced` requires the
/// omitted/retained sets to match exactly and the outcome kinds to
/// agree (`applied` accepts `Untriggered` for the no-requirement
/// rung; `failed` accepts any non-application, the ladder-exhausted
/// record it is).
pub fn verify_selection(rec: &RetainedSelection) -> SelectionVerdict {
    match replay_selection(rec) {
        Err(r) => SelectionVerdict::Refused(r),
        Ok(re) => {
            if re.omitted != rec.omitted {
                return SelectionVerdict::Diverged {
                    detail: format!(
                        "omitted: recorded {} ids, replayed {} ids",
                        rec.omitted.len(),
                        re.omitted.len()
                    ),
                };
            }
            if re.retained != rec.retained {
                return SelectionVerdict::Diverged {
                    detail: format!(
                        "retained: recorded {} ids, replayed {} ids",
                        rec.retained.len(),
                        re.retained.len()
                    ),
                };
            }
            let ok = match rec.status {
                CompactionStatus::Applied => matches!(
                    re.outcome,
                    SelectionOutcome::Applied | SelectionOutcome::Untriggered
                ),
                CompactionStatus::Ineffective => re.outcome == SelectionOutcome::Ineffective,
                CompactionStatus::Failed => matches!(
                    re.outcome,
                    SelectionOutcome::Ineffective | SelectionOutcome::Failed
                ),
            };
            if !ok {
                return SelectionVerdict::Diverged {
                    detail: format!(
                        "outcome: recorded {}, replayed {:?}",
                        status_str(rec.status),
                        re.outcome
                    ),
                };
            }
            SelectionVerdict::Reproduced {
                outcome: re.outcome,
                via_fallback: re.via_fallback,
            }
        }
    }
}
