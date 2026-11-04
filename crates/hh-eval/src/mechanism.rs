//! The §4.2 mechanism joins and versioned denominator definitions
//! (R5.36; REQ-D-005/106, REQ-E-004/107; dossier F5).
//!
//! Registry names, native units/bounds/direction, detector/extractor
//! ids, windows and reducers are declared before collection
//! ([`MECHANISM_REGISTRY`]). For every completed compaction `c` the
//! subject window is `(completion_seq(c), subject_execution_stop_seq]`
//! — windows include later compactions and retain overlapping
//! `(c,event)` joins; a post-execution row (seq above the projected
//! [`LedgerFacts::subject_stop_seq`]) can never join. `forgotten(c)` is
//! the committed omitted item/content set.
//!
//! Reducers: sum numerator/denominator within subject, divide, average
//! *available* subject ratios within task, then equal-task contrast
//! (`w_t = 1/T`) — events are never pooled as trials and tasks are
//! never weighted by opportunities. Zero denominator ⇒
//! `estimator_undefined`; missing required ID/key/detector ⇒
//! `no_detector`; absent summary ⇒ `inapplicable`. Unavailable
//! required members stay in the family report.

use std::collections::{BTreeMap, BTreeSet};

use crate::facts::{CompactionRow, LedgerFacts};

/// The declared direction of a mechanism metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Higher is better.
    Higher,
    /// Lower is better.
    Lower,
    /// No welfare direction (counts).
    Neutral,
}

/// The declared bounds of a mechanism metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bounds {
    /// `[0,1]` — a ratio.
    Unit,
    /// `[0,∞)` — an intensity or count; may exceed one (never clamped).
    NonNegative,
}

/// One versioned §4.2 registry row — declared before collection.
#[derive(Debug, Clone, PartialEq)]
pub struct MechanismDecl {
    /// The registry name (frozen primaries keep their spec spellings;
    /// the retired similarity heuristic keeps a distinct name).
    pub name: &'static str,
    /// The registry version.
    pub version: u32,
    /// The multiplicity family.
    pub family: &'static str,
    /// The native unit (`ratio | intensity | count`).
    pub unit: &'static str,
    /// The native bounds.
    pub bounds: Bounds,
    /// The native direction.
    pub direction: Direction,
    /// The detector/extractor id the definition binds.
    pub detector: &'static str,
    /// The admitted reducer chain.
    pub reducer: &'static str,
    /// `true` = joins over `(completion_seq, stop]` windows.
    pub windowed: bool,
    /// `true` = a required member — unavailable stays in the family.
    pub required: bool,
}

const R: &str = "subject_ratio→task_mean→equal_task";
const C: &str = "subject_count";

const fn decl(
    name: &'static str,
    unit: &'static str,
    bounds: Bounds,
    direction: Direction,
    detector: &'static str,
    reducer: &'static str,
    windowed: bool,
    required: bool,
) -> MechanismDecl {
    MechanismDecl {
        name,
        version: 1,
        family: "mechanism",
        unit,
        bounds,
        direction,
        detector,
        reducer,
        windowed,
        required,
    }
}

/// The versioned §4.2 mechanism registry — every required member plus
/// the retained old approximation under its own distinct name
/// (`required: false` — an approximation never joins the family).
pub const MECHANISM_REGISTRY: &[MechanismDecl] = &[
    decl(
        "grounding.reacquisition_rate",
        "intensity",
        Bounds::NonNegative,
        Direction::Higher,
        "reacquisition_join/1",
        R,
        true,
        true,
    ),
    decl(
        "reliability.repeated_action_rate",
        "ratio",
        Bounds::Unit,
        Direction::Lower,
        "idempotency_join/1",
        R,
        true,
        true,
    ),
    decl(
        "mechanism.activation_start_rate",
        "ratio",
        Bounds::Unit,
        Direction::Higher,
        "compaction_activation/1",
        R,
        false,
        true,
    ),
    decl(
        "mechanism.fidelity_completion_rate",
        "ratio",
        Bounds::Unit,
        Direction::Higher,
        "compaction_fidelity/1",
        R,
        false,
        true,
    ),
    decl(
        "mechanism.fidelity_request_change_rate",
        "ratio",
        Bounds::Unit,
        Direction::Higher,
        "next_dispatch_join/1",
        R,
        true,
        true,
    ),
    decl(
        "mechanism.trigger_first_subject_rate",
        "ratio",
        Bounds::Unit,
        Direction::Higher,
        "trigger_mark/1",
        R,
        false,
        true,
    ),
    decl(
        "mechanism.ineffective_rate",
        "ratio",
        Bounds::Unit,
        Direction::Lower,
        "compaction_outcome/1",
        R,
        false,
        true,
    ),
    decl(
        "mechanism.fallback_rate",
        "ratio",
        Bounds::Unit,
        Direction::Lower,
        "policy_deviation/1",
        R,
        false,
        true,
    ),
    decl(
        "mechanism.recall_rate",
        "ratio",
        Bounds::Unit,
        Direction::Higher,
        "fact_extractor/1",
        R,
        true,
        true,
    ),
    decl(
        "mechanism.summary_followed_rate",
        "ratio",
        Bounds::Unit,
        Direction::Higher,
        "summary_followed/1",
        R,
        false,
        true,
    ),
    decl(
        "mechanism.overhead.tokens_rate",
        "ratio",
        Bounds::Unit,
        Direction::Lower,
        "compaction_charges/1",
        R,
        true,
        true,
    ),
    decl(
        "mechanism.overhead.calls_rate",
        "ratio",
        Bounds::Unit,
        Direction::Lower,
        "compaction_charges/1",
        R,
        true,
        true,
    ),
    decl(
        "mechanism.overhead.spend_rate",
        "ratio",
        Bounds::Unit,
        Direction::Lower,
        "compaction_charges/1",
        R,
        true,
        true,
    ),
    decl(
        "mechanism.opacity_delta",
        "count",
        Bounds::NonNegative,
        Direction::Neutral,
        "opacity_probe/1",
        C,
        false,
        true,
    ),
    decl(
        "mechanism.compaction_count",
        "count",
        Bounds::NonNegative,
        Direction::Neutral,
        "compaction_outcome/1",
        C,
        false,
        true,
    ),
    decl(
        "mechanism.eligible_count",
        "count",
        Bounds::NonNegative,
        Direction::Neutral,
        "compaction_activation/1",
        C,
        false,
        true,
    ),
    decl(
        "mechanism.starts_count",
        "count",
        Bounds::NonNegative,
        Direction::Neutral,
        "compaction_activation/1",
        C,
        false,
        true,
    ),
    decl(
        "mechanism.completions_count",
        "count",
        Bounds::NonNegative,
        Direction::Neutral,
        "compaction_fidelity/1",
        C,
        false,
        true,
    ),
    decl(
        "mechanism.ineffective_count",
        "count",
        Bounds::NonNegative,
        Direction::Neutral,
        "compaction_outcome/1",
        C,
        false,
        true,
    ),
    decl(
        "mechanism.fallback_count",
        "count",
        Bounds::NonNegative,
        Direction::Neutral,
        "policy_deviation/1",
        C,
        false,
        true,
    ),
    decl(
        "reliability.repeated_action_similarity",
        "ratio",
        Bounds::Unit,
        Direction::Lower,
        "similarity_approximation/1",
        C,
        false,
        false,
    ),
];

/// Look up one registry row by name.
pub fn registry_decl(name: &str) -> Option<&'static MechanismDecl> {
    MECHANISM_REGISTRY.iter().find(|d| d.name == name)
}

/// The typed per-subject outcome — the §4.2 honest `n/a` vocabulary,
/// never a silently zero-filled or coerced value.
#[derive(Debug, Clone, PartialEq)]
pub enum MechanismOutcome {
    /// A numerator/denominator pair — hand-countable from the retained
    /// joins; the subject ratio is `numerator/denominator`.
    Value { numerator: i64, denominator: i64 },
    /// A count that is always defined (`0` is a value).
    Count { value: i64 },
    /// Missing required ID/key/detector input — `no_detector`.
    NoDetector { reason: &'static str },
    /// Zero denominator — `estimator_undefined`.
    EstimatorUndefined,
    /// The measure does not apply to this subject — `inapplicable`.
    Inapplicable { reason: &'static str },
}

/// One retained `(c,event)` window join — the evidence a reported
/// numerator recomputes from.
#[derive(Debug, Clone, PartialEq)]
pub struct MechanismJoin {
    /// The metric the join feeds.
    pub metric: &'static str,
    /// The compaction `c` whose window the event joined.
    pub compaction_seq: u64,
    /// The joined event's seq.
    pub event_seq: u64,
    /// The join key (item id / idempotency key / call id).
    pub key: String,
}

/// One subject's mechanism input — its projected run facts plus the
/// declared detector metadata the design pins (the R5.15 slot gives
/// `task_id`/`arm_id`/`first_subject`; nothing is inferred).
#[derive(Debug)]
pub struct SubjectMechanism<'a> {
    /// The subject run id.
    pub subject_run: &'a str,
    /// The task (cluster unit).
    pub task_id: &'a str,
    /// The arm (`A`/`B`).
    pub arm_id: &'a str,
    /// `true` = a first-attempt subject (the trigger denominator).
    pub first_subject: bool,
    /// The run's projected facts.
    pub facts: &'a LedgerFacts,
    /// The declared eligible pressure-opportunity count (`None` ⇒
    /// `no_detector`).
    pub eligible_opportunities: Option<u64>,
    /// Whether the trigger fired (`None` = unrecorded).
    pub triggered: Option<bool>,
    /// The declared intended compaction variant — a started row whose
    /// `variant_ref` differs is a fallback (an assigned-arm deviation,
    /// never a silently relabelled arm; `None` ⇒ `no_detector`).
    pub intended_variant: Option<&'a str>,
    /// The fact extractor's extractable-forgotten set (`None` = absent
    /// extractor ⇒ `no_detector`).
    pub extractable_forgotten: Option<Vec<String>>,
    /// The retained pre/post mechanism labels (evidence, not reduced).
    pub pre_label: Option<&'a str>,
    /// See [`SubjectMechanism::pre_label`].
    pub post_label: Option<&'a str>,
    /// Whether a summary exists (`false` ⇒ `inapplicable`).
    pub summary_present: bool,
    /// The declared summary applicability.
    pub summary_applicable: Option<bool>,
    /// The summary-followed detector's outcome.
    pub summary_followed: Option<bool>,
    /// The opacity probe's pre/post values.
    pub opacity_pre: Option<i64>,
    /// See [`SubjectMechanism::opacity_pre`].
    pub opacity_post: Option<i64>,
    /// The subject's declared same-unit totals (`tokens | calls |
    /// micro_usd` keys) the overhead ratios divide by.
    pub subject_totals: BTreeMap<&'a str, i64>,
}

/// One subject's mechanism fold — every registry outcome plus the
/// retained `(c,event)` joins and labels.
#[derive(Debug, Clone, PartialEq)]
pub struct SubjectMechanismReport {
    /// The subject run id.
    pub subject_run: String,
    /// The task.
    pub task_id: String,
    /// The arm.
    pub arm_id: String,
    /// Every registry row's outcome, keyed by name.
    pub metrics: BTreeMap<&'static str, MechanismOutcome>,
    /// The retained `(c,event)` joins.
    pub joins: Vec<MechanismJoin>,
    /// The retained pre/post labels.
    pub pre_label: Option<String>,
    /// See [`SubjectMechanismReport::pre_label`].
    pub post_label: Option<String>,
}

/// One family member row — required metrics stay listed even when no
/// subject yields a value.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyRow {
    /// The registry name.
    pub name: &'static str,
    /// `required` member.
    pub required: bool,
    /// Subjects presented.
    pub subjects: u64,
    /// Subjects whose outcome is a value.
    pub available: u64,
    /// `available | mixed | unavailable`.
    pub status: &'static str,
}

/// The mechanism report — per-subject folds plus the complete family.
#[derive(Debug, Clone, PartialEq)]
pub struct MechanismReport {
    /// One row per presented subject.
    pub subjects: Vec<SubjectMechanismReport>,
    /// Every registry member (unavailable members retained).
    pub family: Vec<FamilyRow>,
}

fn completed(f: &LedgerFacts) -> impl Iterator<Item = &CompactionRow> {
    f.compactions.iter().filter(|c| c.completed)
}

fn starts(f: &LedgerFacts) -> impl Iterator<Item = &CompactionRow> {
    f.compactions.iter().filter(|c| !c.completed)
}

/// `seq ∈ (c_seq, stop]` — the §4.2 subject window.
fn in_window(seq: u64, c_seq: u64, stop: u64) -> bool {
    seq > c_seq && seq <= stop
}

fn stop_of(f: &LedgerFacts) -> Result<u64, MechanismOutcome> {
    f.subject_stop_seq.ok_or(MechanismOutcome::NoDetector {
        reason: "subject_execution_stop_seq unrecorded",
    })
}

fn ratio_or(num: i64, den: i64) -> MechanismOutcome {
    if den == 0 {
        MechanismOutcome::EstimatorUndefined
    } else {
        MechanismOutcome::Value {
            numerator: num,
            denominator: den,
        }
    }
}

/// The `(c, seq, id)` delivered-in-window rows across every completed
/// compaction — artefact deliveries and memory reads, overlapping
/// `(c,event)` membership retained.
fn deliveries<'a>(f: &'a LedgerFacts, stop: u64) -> Vec<(&'a CompactionRow, u64, &'a str)> {
    let mut v = Vec::new();
    for c in completed(f) {
        for a in &f.artefacts_delivered {
            if in_window(a.seq, c.seq, stop) {
                v.push((c, a.seq, a.artefact_id.as_str()));
            }
        }
        for r in &f.memory_reads {
            if in_window(r.seq, c.seq, stop) {
                for d in &r.delivered {
                    v.push((c, r.seq, d.as_str()));
                }
            }
        }
    }
    v
}

/// `grounding.reacquisition_rate` — matching post-c deliveries/reads
/// over forgotten items. Repeated deliveries count; the intensity may
/// exceed one (never clamped).
fn reacquisition(s: &SubjectMechanism, joins: &mut Vec<MechanismJoin>) -> MechanismOutcome {
    const M: &str = "grounding.reacquisition_rate";
    let f = s.facts;
    let stop = match stop_of(f) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let mut num = 0i64;
    for (c, ev, id) in deliveries(f, stop) {
        if c.forgotten.iter().any(|x| x == id) {
            num += 1;
            joins.push(MechanismJoin {
                metric: M,
                compaction_seq: c.seq,
                event_seq: ev,
                key: id.to_string(),
            });
        }
    }
    let den: i64 = completed(f).map(|c| c.forgotten.len() as i64).sum();
    ratio_or(num, den)
}

/// `reliability.repeated_action_rate` — post-c committed effects
/// matching a pre-c `idempotency_key` over all post-c committed
/// effects; `(c,effect)` joins + keys retained. A missing required key
/// reports `no_detector` — the exact join never guesses.
fn repeated_action(s: &SubjectMechanism, joins: &mut Vec<MechanismJoin>) -> MechanismOutcome {
    const M: &str = "reliability.repeated_action_rate";
    let f = s.facts;
    let stop = match stop_of(f) {
        Ok(v) => v,
        Err(o) => return o,
    };
    if f.effects_committed
        .iter()
        .any(|e| e.seq <= stop && e.idempotency_key.is_none())
    {
        return MechanismOutcome::NoDetector {
            reason: "missing idempotency_key",
        };
    }
    let mut num = 0i64;
    let mut den = 0i64;
    for c in completed(f) {
        let pre: BTreeSet<&str> = f
            .effects_committed
            .iter()
            .filter(|e| e.seq <= c.seq)
            .filter_map(|e| e.idempotency_key.as_deref())
            .collect();
        for e in &f.effects_committed {
            if !in_window(e.seq, c.seq, stop) {
                continue;
            }
            den += 1;
            if e.idempotency_key
                .as_deref()
                .is_some_and(|k| pre.contains(k))
            {
                num += 1;
                joins.push(MechanismJoin {
                    metric: M,
                    compaction_seq: c.seq,
                    event_seq: e.seq,
                    key: e.idempotency_key.clone().unwrap_or_default(),
                });
            }
        }
    }
    ratio_or(num, den)
}

/// `mechanism.fidelity_request_change_rate` — changed actual requests
/// over completions with a next dispatch: the next in-window
/// `model.call.requested` is `changed` when the builder emitted a fresh
/// `context.assembled` view for it.
fn request_change(s: &SubjectMechanism, joins: &mut Vec<MechanismJoin>) -> MechanismOutcome {
    const M: &str = "mechanism.fidelity_request_change_rate";
    let f = s.facts;
    let stop = match stop_of(f) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let assembled: BTreeSet<&str> = f.assembled.iter().map(|(call, _)| call.as_str()).collect();
    let mut num = 0i64;
    let mut den = 0i64;
    for c in completed(f) {
        if let Some(r) = f
            .call_requests
            .values()
            .filter(|r| in_window(r.seq, c.seq, stop))
            .min_by_key(|r| r.seq)
        {
            den += 1;
            if assembled.contains(r.model_call_id.as_str()) {
                num += 1;
                joins.push(MechanismJoin {
                    metric: M,
                    compaction_seq: c.seq,
                    event_seq: r.seq,
                    key: r.model_call_id.clone(),
                });
            }
        }
    }
    ratio_or(num, den)
}

/// `mechanism.recall_rate` — extractable forgotten facts delivered
/// in-window over the extractor's extractable-forgotten set. An absent
/// extractor reports `no_detector`.
fn recall(s: &SubjectMechanism, joins: &mut Vec<MechanismJoin>) -> MechanismOutcome {
    const M: &str = "mechanism.recall_rate";
    let extractable = match &s.extractable_forgotten {
        Some(v) => v,
        None => {
            return MechanismOutcome::NoDetector {
                reason: "fact extractor absent",
            }
        }
    };
    let f = s.facts;
    let stop = match stop_of(f) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let recovered: BTreeSet<&str> = deliveries(f, stop)
        .into_iter()
        .map(|(_, _, id)| id)
        .collect();
    let mut num = 0i64;
    for fct in extractable {
        if recovered.contains(fct.as_str()) {
            num += 1;
            joins.push(MechanismJoin {
                metric: M,
                compaction_seq: 0,
                event_seq: 0,
                key: fct.clone(),
            });
        }
    }
    ratio_or(num, extractable.len() as i64)
}

/// One overhead dimension: compaction-attributed resource in-window
/// over the declared same-unit subject total. No compaction ⇒
/// `inapplicable` (the conditional rate is unavailable, not 0).
fn overhead(
    s: &SubjectMechanism,
    dims: &[&str],
    total_key: &str,
    extra: i64,
    spend: bool,
) -> MechanismOutcome {
    let f = s.facts;
    if completed(f).next().is_none() {
        return MechanismOutcome::Inapplicable {
            reason: "no_compaction",
        };
    }
    let stop = match stop_of(f) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let den = match s.subject_totals.get(total_key) {
        Some(v) => *v,
        None => {
            return MechanismOutcome::NoDetector {
                reason: "subject total undeclared",
            }
        }
    };
    let lo = completed(f).map(|c| c.seq).min().unwrap_or(0);
    let instrument = |t: Option<&str>| t.is_some_and(|t| t == "instrument" || t == "harness");
    let mut num = extra;
    for ch in &f.charges {
        if instrument(ch.charged_to.as_deref())
            && dims.contains(&ch.dimension.as_str())
            && in_window(ch.seq, lo, stop)
        {
            num += ch.amount;
        }
    }
    if spend {
        for r in &f.spend_rows {
            if instrument(r.charged_to.as_deref()) && in_window(r.seq, lo, stop) {
                num += r.micro_units.unwrap_or(0);
            }
        }
    }
    ratio_or(num, den)
}

/// The per-subject fold — every registry row's outcome plus the
/// retained joins/labels.
pub fn detect(s: &SubjectMechanism) -> SubjectMechanismReport {
    let f = s.facts;
    let mut joins = Vec::new();
    let mut m: BTreeMap<&'static str, MechanismOutcome> = BTreeMap::new();
    let nd = |r: &'static str| MechanismOutcome::NoDetector { reason: r };
    let cnt = |value: i64| MechanismOutcome::Count { value };

    m.insert("grounding.reacquisition_rate", reacquisition(s, &mut joins));
    m.insert(
        "reliability.repeated_action_rate",
        repeated_action(s, &mut joins),
    );
    // The retained approximation — capability/args similarity under its
    // own distinct name; the old fold's semantics unchanged.
    m.insert(
        "reliability.repeated_action_similarity",
        cnt(crate::context_metrics::fold(f).repeated_action_count as i64),
    );

    let n_starts = starts(f).count() as i64;
    let n_done = completed(f).count() as i64;
    m.insert(
        "mechanism.activation_start_rate",
        match (f.subject_stop_seq, s.eligible_opportunities) {
            (None, _) => nd("subject_execution_stop_seq unrecorded"),
            (_, None) => nd("eligible pressure opportunities undeclared"),
            (Some(stop), Some(el)) => ratio_or(
                starts(f).filter(|c| c.seq <= stop).count() as i64,
                el as i64,
            ),
        },
    );
    m.insert(
        "mechanism.fidelity_completion_rate",
        ratio_or(n_done, n_starts),
    );
    m.insert(
        "mechanism.fidelity_request_change_rate",
        request_change(s, &mut joins),
    );
    m.insert(
        "mechanism.trigger_first_subject_rate",
        if !s.first_subject {
            MechanismOutcome::Inapplicable {
                reason: "not a first subject",
            }
        } else {
            match s.triggered {
                None => nd("trigger mark unrecorded"),
                Some(t) => MechanismOutcome::Value {
                    numerator: t as i64,
                    denominator: 1,
                },
            }
        },
    );

    let n_ineff = completed(f)
        .filter(|c| c.status.as_deref() == Some("ineffective"))
        .count() as i64;
    m.insert("mechanism.ineffective_rate", ratio_or(n_ineff, n_done));
    let n_fallback = s.intended_variant.map(|want| {
        starts(f)
            .filter(|c| c.variant_ref.as_deref().is_some_and(|v| v != want))
            .count() as i64
    });
    m.insert(
        "mechanism.fallback_rate",
        match n_fallback {
            None => nd("intended policy undeclared"),
            Some(n) => ratio_or(n, n_starts),
        },
    );

    m.insert("mechanism.recall_rate", recall(s, &mut joins));
    m.insert(
        "mechanism.summary_followed_rate",
        if !s.summary_present || s.summary_applicable == Some(false) {
            MechanismOutcome::Inapplicable {
                reason: "no applicable summary",
            }
        } else {
            match s.summary_followed {
                None => nd("summary_followed detector absent"),
                Some(t) => MechanismOutcome::Value {
                    numerator: t as i64,
                    denominator: 1,
                },
            }
        },
    );

    let calls_extra: i64 = completed(f).map(|c| c.model_calls.unwrap_or(0)).sum();
    m.insert(
        "mechanism.overhead.tokens_rate",
        overhead(s, &["tokens", "token"], "tokens", 0, false),
    );
    m.insert(
        "mechanism.overhead.calls_rate",
        overhead(s, &["model_calls", "calls"], "calls", calls_extra, false),
    );
    m.insert(
        "mechanism.overhead.spend_rate",
        overhead(s, &[], "micro_usd", 0, true),
    );

    m.insert(
        "mechanism.opacity_delta",
        match (s.opacity_pre, s.opacity_post) {
            (Some(a), Some(b)) => cnt(b - a),
            _ => nd("opacity probe absent"),
        },
    );
    m.insert("mechanism.compaction_count", cnt(n_done));
    m.insert(
        "mechanism.eligible_count",
        match s.eligible_opportunities {
            Some(v) => cnt(v as i64),
            None => nd("eligible pressure opportunities undeclared"),
        },
    );
    m.insert("mechanism.starts_count", cnt(n_starts));
    m.insert("mechanism.completions_count", cnt(n_done));
    m.insert("mechanism.ineffective_count", cnt(n_ineff));
    m.insert(
        "mechanism.fallback_count",
        match n_fallback {
            None => nd("intended policy undeclared"),
            Some(n) => cnt(n),
        },
    );

    SubjectMechanismReport {
        subject_run: s.subject_run.to_string(),
        task_id: s.task_id.to_string(),
        arm_id: s.arm_id.to_string(),
        metrics: m,
        joins,
        pre_label: s.pre_label.map(str::to_string),
        post_label: s.post_label.map(str::to_string),
    }
}

/// The whole mechanism report — folds every presented subject and keeps
/// every registry member in the family, available or not.
pub fn report(subjects: &[SubjectMechanism]) -> MechanismReport {
    let subs: Vec<SubjectMechanismReport> = subjects.iter().map(detect).collect();
    let family = MECHANISM_REGISTRY
        .iter()
        .map(|d| {
            let total = subs
                .iter()
                .filter(|r| r.metrics.contains_key(d.name))
                .count() as u64;
            let avail = subs
                .iter()
                .filter(|r| {
                    matches!(
                        r.metrics.get(d.name),
                        Some(MechanismOutcome::Value { .. }) | Some(MechanismOutcome::Count { .. })
                    )
                })
                .count() as u64;
            FamilyRow {
                name: d.name,
                required: d.required,
                subjects: total,
                available: avail,
                status: if total > 0 && avail == total {
                    "available"
                } else if avail > 0 {
                    "mixed"
                } else {
                    "unavailable"
                },
            }
        })
        .collect();
    MechanismReport {
        subjects: subs,
        family,
    }
}

// ── reducers ───────────────────────────────────────────────────────────

/// The available subject ratio a `Value` outcome carries (`None` for
/// every typed non-value — never coerced).
pub fn subject_ratio(o: &MechanismOutcome) -> Option<f64> {
    match o {
        MechanismOutcome::Value {
            numerator,
            denominator,
        } if *denominator != 0 => Some(*numerator as f64 / *denominator as f64),
        _ => None,
    }
}

/// One task×arm cell's mean over available subject ratios plus its
/// coverage — subjects without a value never enter the mean.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskMean {
    /// The task.
    pub task_id: String,
    /// The arm.
    pub arm_id: String,
    /// Mean of available subject ratios (`NAN` at zero coverage).
    pub mean: f64,
    /// Subjects contributing a value.
    pub available: u64,
    /// Subjects presented.
    pub total: u64,
}

/// The equal-task contrast — `Σ_t w_t·(mean_B,t − mean_A,t)` with
/// `w_t = 1/T` over the tasks both arms resolve; unpaired tasks are
/// retained beside, never folded.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricContrast {
    /// The registry metric.
    pub metric: String,
    /// The declared unit the contrast is expressed in.
    pub unit: String,
    /// `Σ_t w_t d_t` over the paired tasks (`0` when none pair).
    pub delta: f64,
    /// One `(task, mean_A, mean_B)` per paired task.
    pub paired: Vec<(String, TaskMean, TaskMean)>,
    /// Tasks one arm never resolved (retained beside).
    pub unpaired: Vec<String>,
}

fn arm_means(subs: &[SubjectMechanismReport], metric: &str, arm: &str) -> Vec<TaskMean> {
    let mut tasks: BTreeMap<&str, (f64, u64, u64)> = BTreeMap::new();
    for r in subs.iter().filter(|r| r.arm_id == arm) {
        let e = tasks.entry(r.task_id.as_str()).or_default();
        e.2 += 1;
        if let Some(v) = r.metrics.get(metric).and_then(subject_ratio) {
            e.0 += v;
            e.1 += 1;
        }
    }
    tasks
        .into_iter()
        .map(|(t, (sum, n, total))| TaskMean {
            task_id: t.to_string(),
            arm_id: arm.to_string(),
            mean: if n == 0 { f64::NAN } else { sum / n as f64 },
            available: n,
            total,
        })
        .collect()
}

/// `contrast(report, metric, "A", "B")` — the declared-units
/// equal-task contrast over the subjects' reports.
pub fn contrast(rep: &MechanismReport, metric: &str, arm_a: &str, arm_b: &str) -> MetricContrast {
    let key = |v: Vec<TaskMean>| -> BTreeMap<String, TaskMean> {
        v.into_iter().map(|m| (m.task_id.clone(), m)).collect()
    };
    let (a, b) = (
        key(arm_means(&rep.subjects, metric, arm_a)),
        key(arm_means(&rep.subjects, metric, arm_b)),
    );
    let mut paired = Vec::new();
    let mut unpaired = Vec::new();
    for (t, ma) in &a {
        match b.get(t) {
            Some(mb) if ma.available > 0 && mb.available > 0 => {
                paired.push((t.clone(), ma.clone(), mb.clone()))
            }
            _ => unpaired.push(t.clone()),
        }
    }
    for t in b.keys() {
        if !a.contains_key(t) {
            unpaired.push(t.clone());
        }
    }
    unpaired.sort();
    let t = paired.len() as f64;
    let delta = if t == 0.0 {
        0.0
    } else {
        paired
            .iter()
            .map(|(_, ma, mb)| mb.mean - ma.mean)
            .sum::<f64>()
            / t
    };
    MetricContrast {
        metric: metric.to_string(),
        unit: registry_decl(metric)
            .map(|d| d.unit)
            .unwrap_or("undeclared")
            .to_string(),
        delta,
        paired,
        unpaired,
    }
}
