//! `bind` — the kernel-side handle-binding fold (spec §5f.2 §3 `bind`;
//! ADR-0112 D3; the DF-S1.21-1 emitter half landed at S3.10). The gate's
//! claims bind to `AuthoritativeHandle`s resolved from the durable prefix —
//! never from the claim's own text (F7: `parsed`/`judged` records are never
//! bound; only rows at `authority ≥ environment` become handles, and a
//! `delegate`-origin verdict is excluded even though `delegate` orders above
//! `environment` in the command lattice — model-claimed text is never
//! authoritative evidence).
//!
//! The fold is pure over a [`RowView`] projection so the crate stays free of
//! `hh-ledger`: the driver projects `EventEnvelope`s, the eval plane
//! projects run records — one binding rule, two callers (CC1).

use std::collections::BTreeMap;

use hh_provenance::authority::AuthorityClass;
use hh_wire::Json;

use crate::claims::{binding_kinds, AuthoritativeHandle, Claim, CriterionState, HandleValue};
use crate::gate::TaskContract;
use crate::vocab::{ClaimKind, HandleKind};

/// `RowView` — the ledger-row projection the binder reads (`{seq, class,
/// payload, authority}`). The caller stamps `authority` from the row's
/// provenance (kernel-minted rows → `Kernel`; a judge's verdict row is
/// `Delegate` and is therefore never bound — F7).
#[derive(Debug, Clone, Copy)]
pub struct RowView<'a> {
    /// The row's sequence number.
    pub seq: u64,
    /// The event class.
    pub class: &'a str,
    /// The payload (closed-schema members only — bytes are never read).
    pub payload: &'a Json,
    /// The row's authority (from provenance — CC2: conferred, never read
    /// from content).
    pub authority: AuthorityClass,
    /// The envelope scope's `effect_id` (the `action.effect.*` rows carry
    /// their target on the scope, never the payload — the driver projects
    /// it here).
    pub scope_effect_id: Option<&'a str>,
}

/// A `verification.validator.verdict` row's bound view (the criterion
/// binding reads `criterion_ref`, `status`, `value`, `charged_to`).
#[derive(Debug, Clone, PartialEq)]
pub struct VerdictRow {
    /// The seq the verdict landed at.
    pub seq: u64,
    /// `verdict_id`.
    pub verdict_id: String,
    /// `criterion_ref` (when the verdict binds a criterion).
    pub criterion_ref: Option<String>,
    /// `contract_id` (when the verdict binds a contract).
    pub contract_id: Option<String>,
    /// `status` (`decided` | `inconclusive` | `oracle_failure`).
    pub status: String,
    /// The verdict's affirmative read (`pass`/`true`/`N`).
    pub affirmative: bool,
    /// `phase`.
    pub phase: String,
    /// `detector` (`deterministic` | `judged` | `human`).
    pub detector: String,
    /// `charged_to` (`subject` | `instrument` | …).
    pub charged_to: String,
    /// The row's authority.
    pub authority: AuthorityClass,
}

/// The per-effect terminal fold over `action.effect.*` rows.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectStateRow {
    /// The effect id.
    pub effect_id: String,
    /// Whether a terminal row landed (`observed`/`refused`/`probed`/
    /// `compensated`/`reverted`/`abandoned`).
    pub terminal: bool,
    /// Whether the terminal row was a refusal.
    pub refused: bool,
    /// The canonical outcome tag.
    pub outcome: String,
    /// The seq the latest state row landed at.
    pub seq: u64,
}

/// The terminal effect-state classes (ADR-0030's terminal set — `unknown`
/// is *non-terminal* until probed/abandoned; `committed` is non-terminal —
/// the observation completes it).
const TERMINAL_EFFECT_CLASSES: &[&str] = &[
    "action.effect.observed",
    "action.effect.refused",
    "action.effect.probed",
    "action.effect.compensated",
    "action.effect.reverted",
    "action.effect.abandoned",
];

/// The non-terminal-but-materialized classes (`intended`/`authorized`/
/// `prepared`/`committed`/`deferred`/`unknown`).
const OPEN_EFFECT_CLASSES: &[&str] = &[
    "action.effect.intended",
    "action.effect.authorized",
    "action.effect.prepared",
    "action.effect.committed",
    "action.effect.deferred",
    "action.effect.unknown",
];

/// Fold `action.effect.*` rows into per-effect states (deterministic —
/// the same prefix always yields the same table).
pub fn fold_effect_states(rows: &[RowView]) -> BTreeMap<String, EffectStateRow> {
    let mut states: BTreeMap<String, EffectStateRow> = BTreeMap::new();
    for r in rows {
        let is_terminal = TERMINAL_EFFECT_CLASSES.contains(&r.class);
        let is_open = OPEN_EFFECT_CLASSES.contains(&r.class);
        if !is_terminal && !is_open {
            continue;
        }
        let Some(id) = r
            .payload
            .get("effect_id")
            .and_then(Json::as_str)
            .map(str::to_string)
            .or_else(|| {
                r.payload
                    .get("effect_ref")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            })
            .or_else(|| r.scope_effect_id.map(str::to_string))
        else {
            continue;
        };
        let entry = states.entry(id.clone()).or_insert(EffectStateRow {
            effect_id: id,
            terminal: false,
            refused: false,
            outcome: "intended".into(),
            seq: r.seq,
        });
        entry.seq = r.seq;
        match r.class {
            "action.effect.observed" => {
                entry.terminal = true;
                entry.outcome = r
                    .payload
                    .get("outcome")
                    .and_then(Json::as_str)
                    .unwrap_or("ok")
                    .to_string();
            }
            "action.effect.refused" => {
                entry.terminal = true;
                entry.refused = true;
                entry.outcome = "refused".into();
            }
            "action.effect.probed" => {
                entry.terminal = true;
                entry.outcome = r
                    .payload
                    .get("outcome")
                    .and_then(Json::as_str)
                    .unwrap_or("probed")
                    .to_string();
            }
            "action.effect.compensated" | "action.effect.reverted" => {
                entry.terminal = true;
                entry.outcome = r
                    .class
                    .strip_prefix("action.effect.")
                    .unwrap_or(r.class)
                    .to_string();
            }
            "action.effect.abandoned" => {
                entry.terminal = true;
                entry.outcome = "abandoned".into();
            }
            "action.effect.unknown" => {
                entry.outcome = "unknown".into();
            }
            "action.effect.committed" => {
                entry.outcome = "committed".into();
            }
            "action.effect.deferred" => {
                entry.outcome = "deferred".into();
            }
            _ => {}
        }
    }
    states
}

/// Fold `verification.validator.verdict` rows (deterministic-detector rows
/// only — a `judged`/`human` verdict is never a gate fact, F7; a
/// `delegate`-authority row is never bound — CC2).
pub fn fold_verdicts(rows: &[RowView]) -> Vec<VerdictRow> {
    let mut out = Vec::new();
    for r in rows {
        if r.class != "verification.validator.verdict" {
            continue;
        }
        let detector = r
            .payload
            .get("detector")
            .and_then(Json::as_str)
            .unwrap_or("deterministic")
            .to_string();
        // F7 — only deterministic (kernel) verdicts bind as handles.
        if detector != "deterministic" || r.authority < AuthorityClass::Environment {
            continue;
        }
        if r.authority == AuthorityClass::Delegate {
            continue;
        }
        let status = r
            .payload
            .get("status")
            .and_then(Json::as_str)
            .unwrap_or("decided")
            .to_string();
        let value = r.payload.get("value").cloned().unwrap_or(Json::Null);
        let affirmative = verdict_affirmative(&value);
        out.push(VerdictRow {
            seq: r.seq,
            verdict_id: r
                .payload
                .get("verdict_id")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            criterion_ref: r
                .payload
                .get("criterion_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            contract_id: r
                .payload
                .get("contract_id")
                .and_then(Json::as_str)
                .map(str::to_string),
            status,
            affirmative,
            phase: r
                .payload
                .get("phase")
                .and_then(Json::as_str)
                .unwrap_or("local")
                .to_string(),
            detector,
            charged_to: r
                .payload
                .get("charged_to")
                .and_then(Json::as_str)
                .unwrap_or("subject")
                .to_string(),
            authority: r.authority,
        });
    }
    out
}

/// The affirmative read of a `value` payload member (`bool`, `three_valued`,
/// `lattice` spellings — `inconclusive`/`C`/`I`/`P` are never affirmative).
pub fn verdict_affirmative(value: &Json) -> bool {
    match value {
        Json::Bool(b) => *b,
        Json::Str(s) => matches!(s.as_str(), "pass" | "true" | "N"),
        Json::Int(n) => *n > 0,
        _ => false,
    }
}

/// `bind(rows, claim, contract)` — resolve `binding_kinds(claim.kind)` over
/// the durable prefix into `AuthoritativeHandle`s (ADR-0112 D3). The
/// binding is a pure projection — it performs no effects and never mutates
/// the claim.
///
/// Per kind:
/// - `task_contract` (an `achieved` claim): one `Criterion` handle per
///   contract criterion, measured by the *latest decided* deterministic
///   verdict naming the criterion (`unrun` when none); plus `EffectState`
///   handles for every effect still non-terminal at the claim (the D6
///   detector's input) and every `effect:<id>` the claim cites.
/// - `validator_verdict` (`verified`/`achieved`): the decided verdict rows
///   the claim cites (by `verdict_id` or `criterion_ref`) — or, for an
///   `achieved` claim, every verdict bound to the contract.
/// - `effect_state` (`effected`/`pending`): the fold's per-effect state for
///   every `effect:<id>`/`effect_id` the claim cites.
/// - `ledger_event` (`observed`/`effected`): the claim's `evidence_refs`
///   resolved to rows by `event_id` — `HandleValue::Value(payload)` (the
///   closed-schema equality comparator only).
/// - `world_state`/`environment_probe`/`progress_artifact_version`: bound
///   when a cited ref resolves to the matching row class; otherwise absent
///   (unbindable ⇒ `unverifiable`, never `agree` — ADR-0112 D3).
pub fn bind_claim(
    rows: &[RowView],
    claim: &Claim,
    contract: Option<&TaskContract>,
) -> Vec<AuthoritativeHandle> {
    let mut handles = Vec::new();
    let verdicts = fold_verdicts(rows);
    let effects = fold_effect_states(rows);
    let kinds = binding_kinds(claim.kind);

    let mut push = |kind: HandleKind, handle_ref: String, value: Option<HandleValue>, seq: u64| {
        handles.push(AuthoritativeHandle {
            kind,
            handle_ref,
            value,
            authority: AuthorityClass::Kernel,
            produced_at_seq: seq,
        });
    };

    for kind in kinds {
        match kind {
            HandleKind::TaskContract => {
                if let Some(c) = contract {
                    for crit in c.criteria.iter().chain(c.invariants.iter()) {
                        // The criterion's measured state: the latest decided
                        // verdict naming it (a non-decided verdict is
                        // `unverifiable`-class evidence, never a pass).
                        let latest = verdicts
                            .iter()
                            .filter(|v| {
                                v.criterion_ref.as_deref() == Some(crit.criterion_id.as_str())
                            })
                            .max_by_key(|v| v.seq);
                        let (state, at) = match latest {
                            Some(v) if v.status == "decided" => (
                                if v.affirmative {
                                    CriterionState::Met
                                } else {
                                    CriterionState::Unmet
                                },
                                v.seq,
                            ),
                            Some(v) => (CriterionState::Unverifiable, v.seq),
                            None => (CriterionState::Unrun, 0),
                        };
                        push(
                            HandleKind::TaskContract,
                            format!("criterion:{}", crit.criterion_id),
                            Some(HandleValue::Criterion { status: state }),
                            at,
                        );
                    }
                    // D6 — every still-open effect is a `Criterion`-adjacent
                    // handle on the completion claim (open_effect_at_completion).
                    for (id, st) in &effects {
                        if !st.terminal {
                            push(
                                HandleKind::EffectState,
                                format!("effect:{id}"),
                                Some(HandleValue::EffectState {
                                    effect_id: id.clone(),
                                    terminal: false,
                                    refused: st.refused,
                                    outcome: st.outcome.clone(),
                                }),
                                st.seq,
                            );
                        }
                    }
                }
            }
            HandleKind::ValidatorVerdict => {
                for v in &verdicts {
                    let cited = claim.evidence_refs.iter().any(|r| {
                        r == &v.verdict_id
                            || Some(r.as_str()) == v.criterion_ref.as_deref()
                            || r == &format!("verdict:{}", v.verdict_id)
                    });
                    let in_contract = claim.kind == ClaimKind::Achieved
                        && contract.is_some_and(|c| {
                            v.contract_id.as_deref() == Some(c.contract_id.as_str())
                                || c.criteria.iter().any(|cr| {
                                    Some(cr.criterion_id.as_str()) == v.criterion_ref.as_deref()
                                })
                        });
                    if cited || in_contract {
                        push(
                            HandleKind::ValidatorVerdict,
                            format!("verdict:{}", v.verdict_id),
                            Some(HandleValue::Verdict {
                                affirmative: v.affirmative,
                            }),
                            v.seq,
                        );
                    }
                }
            }
            HandleKind::EffectState => {
                for r in &claim.evidence_refs {
                    let id = r
                        .strip_prefix("effect:")
                        .unwrap_or(r.as_str())
                        .to_string();
                    if let Some(st) = effects.get(&id) {
                        push(
                            HandleKind::EffectState,
                            format!("effect:{id}"),
                            Some(HandleValue::EffectState {
                                effect_id: id.clone(),
                                terminal: st.terminal,
                                refused: st.refused,
                                outcome: st.outcome.clone(),
                            }),
                            st.seq,
                        );
                    }
                }
            }
            HandleKind::LedgerEvent => {
                for r in &claim.evidence_refs {
                    if let Some(row) = rows.iter().find(|row| {
                        row.payload
                            .get("event_id")
                            .and_then(Json::as_str)
                            .is_some_and(|id| id == r.as_str())
                    }) {
                        push(
                            HandleKind::LedgerEvent,
                            r.clone(),
                            Some(HandleValue::Value(row.payload.clone())),
                            row.seq,
                        );
                    }
                }
            }
            HandleKind::WorldState | HandleKind::EnvironmentProbe => {
                for r in &claim.evidence_refs {
                    if let Some(row) = rows.iter().find(|row| {
                        row.class.starts_with("action.world_state.")
                            && row
                                .payload
                                .get("ref")
                                .and_then(Json::as_str)
                                .is_some_and(|id| id == r.as_str())
                    }) {
                        push(
                            *kind,
                            r.clone(),
                            Some(HandleValue::Value(row.payload.clone())),
                            row.seq,
                        );
                    }
                }
            }
            HandleKind::ProgressArtifactVersion => {
                for r in &claim.evidence_refs {
                    if let Some(row) = rows.iter().find(|row| {
                        row.class == "context.progress.recorded"
                            && row
                                .payload
                                .get("version_ref")
                                .and_then(Json::as_str)
                                .is_some_and(|id| id == r.as_str())
                    }) {
                        push(
                            *kind,
                            r.clone(),
                            Some(HandleValue::Value(row.payload.clone())),
                            row.seq,
                        );
                    }
                }
            }
        }
    }
    handles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims::{reconcile_ledger_only, Claim};
    use crate::vocab::{
        Agreement, ClaimKind, CompletionPolicy, DivergenceClass, ExtractedBy, SubjectRef,
    };
    use crate::gate::{AcceptanceCriterion, ValidatesRecord};
    use crate::vocab::{CriterionRole, VerdictPhase, Visibility};
    use hh_provenance::origin::Origin;
    use hh_provenance::record::ProvenanceRecord;
    use hh_provenance::authority::PersistenceScope;

    fn prov() -> ProvenanceRecord {
        ProvenanceRecord::minted(
            Origin::model("m", "r", "call:1"),
            PersistenceScope::Run,
            1,
        )
    }

    fn kprov() -> ProvenanceRecord {
        ProvenanceRecord::kernel("reconciler/1", 1)
    }

    fn claim(kind: ClaimKind, evidence_refs: Vec<String>) -> Claim {
        Claim {
            claim_id: "c1".into(),
            run_id: "r".into(),
            model_call_id: "mc-1".into(),
            at_seq: 10,
            kind,
            subject: SubjectRef::Run,
            predicate: "is_done".into(),
            asserted: Json::Bool(true),
            evidence_refs,
            extracted_by: ExtractedBy::Structured("finish".into()),
            extraction_confidence_ppm: 1_000_000,
            criteria_status: vec![],
            provenance: prov(),
        }
    }

    fn contract(required_met: &[&str]) -> TaskContract {
        TaskContract {
            contract_id: "contract:1".into(),
            goal_ref: "goal:1".into(),
            criteria: required_met
                .iter()
                .map(|c| AcceptanceCriterion {
                    criterion_id: c.to_string(),
                    validator_ref: format!("val:{c}@1"),
                    record: ValidatesRecord {
                        role: CriterionRole::Acceptance,
                        phase: VerdictPhase::Completion,
                        required: true,
                        visibility: Visibility::Visible,
                        veto: false,
                        evidence_requirements: vec![],
                        window: None,
                        weight_ppm: None,
                    },
                })
                .collect(),
            invariants: vec![],
            completion_policy: CompletionPolicy::AllRequired,
            evidence_kinds_required: vec![],
            budget_ref: "b".into(),
            sealed: true,
        }
    }

    #[test]
    fn achieved_binds_criteria_and_open_effects() {
        let verdict = Json::obj([
            ("verdict_id", Json::str("v-1")),
            ("criterion_ref", Json::str("crit:tests")),
            ("status", Json::str("decided")),
            ("value", Json::Bool(true)),
            ("detector", Json::str("deterministic")),
            ("phase", Json::str("completion")),
        ]);
        let open = Json::obj([("effect_id", Json::str("e-open"))]);
        let rows = [
            RowView {
                seq: 5,
                class: "verification.validator.verdict",
                payload: &verdict,
                authority: AuthorityClass::Kernel,
                scope_effect_id: None,
            },
            RowView {
                seq: 7,
                class: "action.effect.committed",
                payload: &open,
                authority: AuthorityClass::Kernel,
                scope_effect_id: None,
            },
        ];
        let c = contract(&["crit:tests"]);
        let cl = claim(ClaimKind::Achieved, vec![]);
        let handles = bind_claim(&rows, &cl, Some(&c));
        assert!(handles.iter().any(|h| matches!(
            h.value,
            Some(HandleValue::Criterion {
                status: CriterionState::Met
            })
        )));
        assert!(handles.iter().any(|h| matches!(
            h.value,
            Some(HandleValue::EffectState {
                ref terminal, ..
            }) if !terminal
        )));
        let rec = reconcile_ledger_only(&cl, &handles, "hir/kernel/reconcile:0", 20, kprov());
        // The open effect diverges (D6); the met criterion does not.
        assert_eq!(
            rec.agreement,
            Agreement::Diverge(DivergenceClass::OpenEffectAtCompletion)
        );
    }

    #[test]
    fn judged_verdict_is_never_bound() {
        let verdict = Json::obj([
            ("verdict_id", Json::str("v-j")),
            ("status", Json::str("decided")),
            ("value", Json::Bool(false)),
            ("detector", Json::str("judged")),
        ]);
        let rows = [RowView {
            seq: 5,
            class: "verification.validator.verdict",
            payload: &verdict,
            authority: AuthorityClass::Delegate,
            scope_effect_id: None,
        }];
        let cl = claim(ClaimKind::Verified, vec!["v-j".into()]);
        let handles = bind_claim(&rows, &cl, None);
        assert!(handles.is_empty(), "F7 — a judged record never binds");
        let rec = reconcile_ledger_only(&cl, &handles, "hir/kernel/reconcile:0", 20, kprov());
        assert_eq!(rec.agreement, Agreement::Unverifiable);
    }

    #[test]
    fn detector_field_excludes_judged_even_at_kernel_authority() {
        let verdict = Json::obj([
            ("verdict_id", Json::str("v-j")),
            ("status", Json::str("decided")),
            ("value", Json::Bool(false)),
            ("detector", Json::str("judged")),
        ]);
        let rows = [RowView {
            seq: 5,
            class: "verification.validator.verdict",
            payload: &verdict,
            authority: AuthorityClass::Kernel,
            scope_effect_id: None,
        }];
        let vs = fold_verdicts(&rows);
        assert!(vs.is_empty());
    }

    #[test]
    fn unrun_criterion_binds_unrun() {
        let c = contract(&["crit:never-run"]);
        let cl = claim(ClaimKind::Achieved, vec![]);
        let handles = bind_claim(&[], &cl, Some(&c));
        assert!(handles.iter().any(|h| matches!(
            h.value,
            Some(HandleValue::Criterion {
                status: CriterionState::Unrun
            })
        )));
        let rec = reconcile_ledger_only(&cl, &handles, "hir/kernel/reconcile:0", 20, kprov());
        assert_eq!(
            rec.agreement,
            Agreement::Diverge(DivergenceClass::ContractGap)
        );
    }

    /// AC-R-2.7.2a-8 — a 500-event corpus folds → binds → reconciles to the
    /// identical records on a rebuild (`view_hash` equality), and the
    /// reconciled row reports the per-claim check as an instrument cost.
    #[test]
    fn ledger_only_reconcile_rebuilds_identically_over_500_events() {
        let mut payloads: Vec<Json> = Vec::new();
        // 200 effect lifecycles (intended + committed + observed = 600…
        // trim to 450) + 50 verdict rows = 500.
        for i in 0..150u64 {
            payloads.push(Json::obj([
                ("effect_id", Json::str(format!("e-{i}"))),
                ("attempt_no", Json::Int(1)),
            ])); // intended
        }
        for i in 0..150u64 {
            payloads.push(Json::obj([
                ("effect_id", Json::str(format!("e-{i}"))),
                ("attempt_no", Json::Int(1)),
            ])); // committed
        }
        for i in 0..150u64 {
            payloads.push(Json::obj([
                ("effect_id", Json::str(format!("e-{i}"))),
                ("outcome", Json::str("applied")),
            ])); // observed
        }
        for i in 0..50u64 {
            payloads.push(Json::obj([
                ("verdict_id", Json::str(format!("v-{i}"))),
                ("criterion_ref", Json::str(format!("crit:{}", i % 5))),
                ("status", Json::str("decided")),
                ("value", Json::Bool(true)),
                ("detector", Json::str("deterministic")),
                ("phase", Json::str("completion")),
            ]));
        }
        assert_eq!(payloads.len(), 500);
        let rows: Vec<RowView> = payloads
            .iter()
            .enumerate()
            .map(|(i, p)| RowView {
                seq: (i + 1) as u64,
                class: if i < 150 {
                    "action.effect.intended"
                } else if i < 300 {
                    "action.effect.committed"
                } else if i < 450 {
                    "action.effect.observed"
                } else {
                    "verification.validator.verdict"
                },
                payload: p,
                authority: AuthorityClass::Kernel,
                scope_effect_id: None,
            })
            .collect();
        let c = contract(&["crit:0", "crit:1", "crit:2", "crit:3", "crit:4"]);
        let cl = claim(ClaimKind::Achieved, vec![]);
        // The rebuild — two independent folds produce identical handles and
        // identical reconciliation records (view_hash equality).
        let h1 = bind_claim(&rows, &cl, Some(&c));
        let h2 = bind_claim(&rows, &cl, Some(&c));
        assert_eq!(h1, h2);
        let r1 = reconcile_ledger_only(&cl, &h1, "hir/kernel/reconcile:0", 600, kprov());
        let r2 = reconcile_ledger_only(&cl, &h2, "hir/kernel/reconcile:0", 600, kprov());
        assert_eq!(r1, r2);
        let j1 = crate::events::claim_reconciled(&r1).to_canonical_string();
        let j2 = crate::events::claim_reconciled(&r2).to_canonical_string();
        assert_eq!(j1, j2, "rebuild equality — the reconciled bytes");
        // All criteria met, all effects applied → `agree` (the clean arm).
        assert_eq!(r1.agreement, Agreement::Agree);
        // The per-claim deterministic check reports as instrument cost.
        let payload = crate::events::claim_reconciled(&r1);
        assert_eq!(
            payload.get("charged_to").and_then(Json::as_str),
            Some("instrument")
        );
    }

    /// AC-R-2.7.2a-3 + AC-R-2.7.2a-6 — the negative corpus: honest claims,
    /// `assumption` claims, retried-then-applied effects and judged-only
    /// contradiction records produce zero `diverge` (0 false positives;
    /// F7 — a judged/parsed record alone never causes a hold or diverge).
    #[test]
    fn negative_corpus_yields_zero_divergence() {
        let applied = |id: &str| {
            Json::obj([
                ("effect_id", Json::str(id.to_string())),
                ("outcome", Json::str("applied")),
            ])
        };
        // (a) honest `effected` claim — the effect applied.
        let applied_row = applied("e-ok");
        let rows = [RowView {
            seq: 1,
            class: "action.effect.observed",
            payload: &applied_row,
            authority: AuthorityClass::Kernel,
            scope_effect_id: None,
        }];
        let cl = {
            let mut c = claim(ClaimKind::Effected, vec!["e-ok".into()]);
            c.subject = SubjectRef::Effect("e-ok".into());
            c
        };
        let h = bind_claim(&rows, &cl, None);
        let r = reconcile_ledger_only(&cl, &h, "hir/kernel/reconcile:0", 20, kprov());
        assert_ne!(
            r.agreement,
            Agreement::Diverge(crate::vocab::DivergenceClass::PhantomEffect)
        );
        // (b) `assumption` claim — unbindable ⇒ unverifiable, never diverge.
        let c = claim(ClaimKind::Assumption, vec![]);
        let r = reconcile_ledger_only(&c, &bind_claim(&rows, &c, None), "hir/kernel/reconcile:0", 20, kprov());
        assert_eq!(r.agreement, Agreement::Unverifiable);
        // (c) honest `unachievable` — no contradiction ⇒ not diverge.
        let c = claim(ClaimKind::Unachievable, vec![]);
        let h = bind_claim(&rows, &c, None);
        let r = reconcile_ledger_only(&c, &h, "hir/kernel/reconcile:0", 20, kprov());
        assert!(!matches!(r.agreement, Agreement::Diverge(_)));
        // (d) a judged verdict contradicting the claim never binds (F7) —
        // the fold is `unverifiable`, never `diverge`.
        let jv = Json::obj([
            ("verdict_id", Json::str("v-j")),
            ("status", Json::str("decided")),
            ("value", Json::Bool(false)),
            ("detector", Json::str("judged")),
        ]);
        let jrows = [RowView {
            seq: 1,
            class: "verification.validator.verdict",
            payload: &jv,
            authority: AuthorityClass::Kernel,
            scope_effect_id: None,
        }];
        let c = claim(ClaimKind::Verified, vec!["v-j".into()]);
        let h = bind_claim(&jrows, &c, None);
        let r = reconcile_ledger_only(&c, &h, "hir/kernel/reconcile:0", 20, kprov());
        assert_eq!(r.agreement, Agreement::Unverifiable);
        assert!(!matches!(r.agreement, Agreement::Diverge(_)));
    }
}
