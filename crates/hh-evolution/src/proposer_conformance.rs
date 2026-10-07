//! The `evolution_proposer` four-kind conformance driver (§05h §2.4;
//! ADR-0196 D2; AC-R-2.9.5-14) — the class suite's executable surface,
//! driven over a [`ProposerPort`] so the *same* checks run on the
//! first-party in-process variant and on a third-party variant behind
//! the variant host (`plugin_abi/1` — the `registry_ci` driver seam,
//! §06 ADR-0152):
//!
//! * **static** — `declare()` parses as a `ProposerDeclaration`, every
//!   mandatory member is present (`family`, `op_classes_admissible` ⊆
//!   the campaign's one class at 6b, `uses_judge`/`maturity`), and every
//!   conditioned proposer rule carries an `AssumptionDebtRecord`
//!   (`IncompleteDeclaration` on a `None`).
//! * **contract** — `propose` over the golden corpus: every output
//!   parses through the typed `ProposerOutcome` codec, and every emitted
//!   `CandidateProposal`'s diff `classify`es with no widening/loosening
//!   delta (`authority`, `budget`, `validity`, `coordination`) and
//!   inverts byte-identically (`apply(target, invert(diff)) = base`).
//! * **property** — the isolation battery: no foreign read attempted
//!   (`foreign_reads` counter), no held-out member admitted into the
//!   corpus view, never emits a candidate on the exclusion set, and a
//!   `uses_judge = no` declaration with a non-zero `judge_call_attempts`
//!   count is `DRIFT` (`Unsupported{drift}` — the conformance verdict
//!   vocabulary has no `drift`; `Unsupported` + `drift` detail).
//! * **executable** — over the Stage-3-shaped golden corpus the variant
//!   emits ≥ 1 S2-valid `FailureHypothesis` (falsifiable deltas over
//!   declared metrics/tasks) or a `NoAddressableFailure` with a
//!   non-empty reason.

use hh_hir::diff;
use hh_hir::document::HirDocument;
use hh_registry::kinds::ConformanceVerdict;
use hh_wire::json::Json;

use crate::proposer::{
    outcome_from_json, EvidenceCorpus, EvolutionProposer, LineageEntry, ProposalConstraints,
};
use crate::records::{
    FailureHypothesis, ParentSelectionPolicy, ProposerDeclaration, ProposerOutcome, Tri,
};

/// The drive channel a proposer runs over — the first-party variant is
/// [`InProcessPort`]; a `plugin_abi/1` variant rides [`PluginPort`] (the
/// variant host supplies the out-of-process lane — §06's `registry_ci`).
pub trait ProposerPort {
    /// `declare()` → the declaration document.
    fn declare(&mut self) -> Result<Json, String>;
    /// `propose(corpus, base, constraints)` → raw output documents.
    fn propose(&mut self, req: &Json) -> Result<Vec<Json>, String>;
    /// `select_parent(lineage, policy)` → the picked ref document.
    fn select_parent(&mut self, lineage: &Json, policy: &Json) -> Result<Json, String>;
    /// Judge calls the proposer attempted this run (the DRIFT surface —
    /// the port counts mediated calls; a proposer has no unmediated
    /// reach).
    fn judge_call_attempts(&self) -> u64;
    /// Reads the proposer attempted outside the handed corpus (0 by
    /// construction on the in-process port — the trait boundary is the
    /// only data channel).
    fn foreign_read_attempts(&self) -> u64;
}

/// The in-process port — wraps a `dyn EvolutionProposer` (the
/// first-party variant; CPU-only, no handles — `foreign_read_attempts`
/// is structurally 0: the port hands over nothing but the corpus view).
pub struct InProcessPort<'a> {
    /// The proposer under drive.
    pub inner: &'a mut dyn EvolutionProposer,
    /// The corpus/base/constraints the last `propose` ran with (the
    /// port keeps the realised inputs so the contract kind can re-apply
    /// the emitted diffs).
    base: HirDocument,
    corpus: EvidenceCorpus,
    constraints: ProposalConstraints,
    judge_calls: u64,
}

impl<'a> InProcessPort<'a> {
    /// Wrap the variant with the drive inputs.
    pub fn new(
        inner: &'a mut dyn EvolutionProposer,
        base: HirDocument,
        corpus: EvidenceCorpus,
        constraints: ProposalConstraints,
    ) -> InProcessPort<'a> {
        InProcessPort {
            inner,
            base,
            corpus,
            constraints,
            judge_calls: 0,
        }
    }
}

impl ProposerPort for InProcessPort<'_> {
    fn declare(&mut self) -> Result<Json, String> {
        Ok(self.inner.declare().to_json())
    }
    fn propose(&mut self, _req: &Json) -> Result<Vec<Json>, String> {
        self.inner
            .propose(&self.corpus, &self.base, &self.constraints)
            .map(|outs| outs.iter().map(crate::proposer::outcome_to_json).collect())
            .map_err(|f| f.code().to_string())
    }
    fn select_parent(&mut self, lineage: &Json, policy: &Json) -> Result<Json, String> {
        let entries: Vec<LineageEntry> = match lineage {
            Json::Arr(a) => a
                .iter()
                .filter_map(|e| {
                    let m = match e {
                        Json::Obj(m) => m,
                        _ => return None,
                    };
                    let gv = |k: &str| m.get(k).and_then(Json::as_str).unwrap_or("").to_string();
                    let gi = |k: &str| m.get(k).and_then(Json::as_int).unwrap_or(0) as u64;
                    Some(LineageEntry {
                        candidate_id: gv("candidate_id"),
                        target_ref: hh_hir::document::DefinitionVersionRef {
                            semantic_id: gv("semantic_id"),
                            version_id: gv("version_id"),
                        },
                        score_ppm: gi("score_ppm"),
                        children: gi("children") as u32,
                        per_task: Default::default(),
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        let pol = ParentSelectionPolicy::from_json(policy).map_err(|e| format!("{e:?}"))?;
        match self.inner.select_parent(&entries, &pol) {
            Some(r) => Ok(Json::obj([
                ("semantic_id", Json::str(r.semantic_id)),
                ("version_id", Json::str(r.version_id)),
            ])),
            None => Ok(Json::Null),
        }
    }
    fn judge_call_attempts(&self) -> u64 {
        self.judge_calls
    }
    fn foreign_read_attempts(&self) -> u64 {
        0
    }
}

/// One suite row — `{subject: "{kind}.{test}", verdict, detail}` (the
/// `ReportResult` shape minus the registry's report framing — the
/// caller folds rows into a `ConformanceReport` whose `suite_ref` is
/// the class's pinned suite).
#[derive(Debug, Clone)]
pub struct ProposerSuiteRow {
    /// `{kind}.{test_id}`.
    pub subject: String,
    /// The verdict.
    pub verdict: ConformanceVerdict,
    /// The detail (a typed refusal name or `ok`).
    pub detail: String,
}

fn pass(subject: &str) -> ProposerSuiteRow {
    ProposerSuiteRow {
        subject: subject.to_string(),
        verdict: ConformanceVerdict::Supported,
        detail: "ok".to_string(),
    }
}
fn fail(subject: &str, detail: impl Into<String>) -> ProposerSuiteRow {
    ProposerSuiteRow {
        subject: subject.to_string(),
        verdict: ConformanceVerdict::Unsupported,
        detail: detail.into(),
    }
}
fn na(subject: &str, detail: impl Into<String>) -> ProposerSuiteRow {
    ProposerSuiteRow {
        subject: subject.to_string(),
        verdict: ConformanceVerdict::NotApplicable,
        detail: detail.into(),
    }
}

/// The drive inputs (records-in): the golden corpus view + base +
/// constraints the contract/executable kinds run.
pub struct SuiteDrive {
    /// The golden corpus (constructed to hold one addressable failure —
    /// the executable kind's fixture).
    pub corpus: EvidenceCorpus,
    /// The golden base document.
    pub base: HirDocument,
    /// The proposal constraints (one target class).
    pub constraints: ProposalConstraints,
    /// The exclusion set the property kind checks emitted diffs
    /// against (rides `constraints.exclusions`; the corpus's too).
    pub in_process: bool,
}

/// `run_proposer_suite(port, drive)` — the four kinds over the port.
pub fn run_proposer_suite(
    port: &mut dyn ProposerPort,
    drive: &SuiteDrive,
) -> Vec<ProposerSuiteRow> {
    let mut rows = Vec::new();
    let decl_j = match port.declare() {
        Ok(j) => j,
        Err(e) => {
            rows.push(fail("static.declaration", format!("declare() failed: {e}")));
            return rows;
        }
    };
    let decl = match ProposerDeclaration::from_json(&decl_j) {
        Ok(d) => d,
        Err(e) => {
            rows.push(fail(
                "static.declaration",
                format!("declaration does not parse: {e:?}"),
            ));
            return rows;
        }
    };

    // ── static.declaration ────────────────────────────────────────────
    if decl.family.is_empty()
        || decl.op_classes_admissible.is_empty()
        || !matches!(
            decl.maturity.as_str(),
            "instrument-grade" | "research-grade"
        )
    {
        rows.push(fail(
            "static.declaration",
            "mandatory members absent or maturity outside the closed set",
        ));
    } else if decl.op_classes_admissible.len() > 1 {
        rows.push(fail(
            "static.declaration",
            "op_classes_admissible has > 1 class — the 6b one-class rule",
        ));
    } else if decl.uses_judge == Tri::Yes && decl.judge_ref.is_none() {
        rows.push(fail(
            "static.declaration",
            "uses_judge = yes without a judge_ref",
        ));
    } else {
        rows.push(pass("static.declaration"));
    }
    // static.debt — every conditioned rule carries a debt record.
    let undebts: Vec<String> = decl
        .conditioned_rules
        .iter()
        .filter(|r| r.debt_record.is_none())
        .map(|r| r.rule_ref.clone())
        .collect();
    if undebts.is_empty() {
        rows.push(pass("static.debt_completeness"));
    } else {
        rows.push(fail(
            "static.debt_completeness",
            format!("conditioned rules without debt records: {undebts:?}"),
        ));
    }

    // ── contract.propose ──────────────────────────────────────────────
    let outs = match port.propose(&Json::obj([])) {
        Ok(o) => o,
        Err(e) => {
            rows.push(fail(
                "contract.propose",
                format!("propose raised the typed failure {e}"),
            ));
            Vec::new()
        }
    };
    if outs.is_empty() {
        rows.push(fail(
            "contract.propose",
            "empty output — NoAddressableFailure is a typed outcome, never an empty list",
        ));
    } else {
        let mut parsed: Vec<ProposerOutcome> = Vec::new();
        let mut parse_fail = false;
        for o in &outs {
            match outcome_from_json(o) {
                Ok(p) => parsed.push(p),
                Err(e) => {
                    rows.push(fail(
                        "contract.propose",
                        format!("an output does not parse as ProposerOutcome: {e:?}"),
                    ));
                    parse_fail = true;
                }
            }
        }
        if !parse_fail {
            rows.push(pass("contract.propose"));
        }
        // contract.classify — every emitted candidate's diff applies,
        // never widens/loosens, and inverts byte-identically.
        let mut classify_ok = true;
        for p in &parsed {
            let ProposerOutcome::Candidate(c) = p else {
                continue;
            };
            let d = &c.diff;
            let cl = &d.classification;
            if cl.authority_delta == hh_hir::diff::AuthorityDelta::Widening
                || cl.budget_delta == hh_hir::diff::Delta::Loosening
                || cl.validity_delta == hh_hir::diff::Delta::Loosening
                || cl.coordination_delta == hh_hir::diff::Delta::Loosening
            {
                rows.push(fail(
                    "contract.classify",
                    format!("emitted diff {d:?} classifies widening/loosening"),
                ));
                classify_ok = false;
                continue;
            }
            match diff::apply(&drive.base, d) {
                Ok(t) => match diff::apply(&t, &diff::invert(d)) {
                    Ok(b) if b.canonical_bytes() == drive.base.canonical_bytes() => {}
                    Ok(_) => {
                        rows.push(fail(
                            "contract.classify",
                            "invert(diff) does not restore the base byte-identically",
                        ));
                        classify_ok = false;
                    }
                    Err(e) => {
                        rows.push(fail(
                            "contract.classify",
                            format!("invert/apply failed: {e:?}"),
                        ));
                        classify_ok = false;
                    }
                },
                Err(e) => {
                    rows.push(fail(
                        "contract.classify",
                        format!("apply(base, diff) failed: {e:?}"),
                    ));
                    classify_ok = false;
                }
            }
        }
        if classify_ok && !parsed.is_empty() {
            rows.push(pass("contract.classify"));
        }
        // ── property.exclusion_set — no emitted diff touches an
        //    excluded target.
        let mut excluded_hit = false;
        for p in &parsed {
            let ProposerOutcome::Candidate(c) = p else {
                continue;
            };
            for op in &c.diff.ops {
                let id = op.node_id();
                if drive.constraints.exclusions.iter().any(|e| e == id)
                    || drive.constraints.must_code.iter().any(|e| e == id)
                {
                    rows.push(fail(
                        "property.exclusion_set",
                        format!("emitted diff targets excluded/MUST-code `{id}`"),
                    ));
                    excluded_hit = true;
                }
            }
        }
        if !excluded_hit {
            rows.push(pass("property.exclusion_set"));
        }
        // ── executable.golden_corpus — ≥1 S2-valid hypothesis or a
        //    reasoned NoAddressableFailure.
        let mut valid_hyp = false;
        let mut reasoned_naf = false;
        for p in &parsed {
            match p {
                ProposerOutcome::Hypothesis(h) if hypothesis_s2_valid(h, &drive.corpus) => {
                    valid_hyp = true
                }
                ProposerOutcome::Candidate(c)
                    if c.hypothesis
                        .as_ref()
                        .is_some_and(|h| hypothesis_s2_valid(h, &drive.corpus)) =>
                {
                    valid_hyp = true
                }
                ProposerOutcome::NoAddressableFailure { reason } if !reason.is_empty() => {
                    reasoned_naf = true
                }
                _ => {}
            }
        }
        if valid_hyp || reasoned_naf {
            rows.push(pass("executable.golden_corpus"));
        } else {
            rows.push(fail(
                "executable.golden_corpus",
                "no S2-valid hypothesis and no reasoned NoAddressableFailure",
            ));
        }
    }

    // ── property.isolation — the probe battery (the port's counters are
    //    the observable; raw fs/egress/exec probes are the confined
    //    lane's suite — `n/a{in_process}` mirrors `run_kit`).
    if port.foreign_read_attempts() == 0 {
        rows.push(pass("property.isolation.corpus_only"));
    } else {
        rows.push(fail(
            "property.isolation.corpus_only",
            format!("{} foreign read attempts", port.foreign_read_attempts()),
        ));
    }
    let held_out_leak = drive
        .corpus
        .layers
        .iter()
        .any(|l| crate::records::FORBIDDEN_LAYERS.iter().any(|f| f == l));
    if held_out_leak {
        rows.push(fail(
            "property.isolation.held_out",
            "the corpus view carried a held-out layer",
        ));
    } else {
        rows.push(pass("property.isolation.held_out"));
    }
    // `uses_judge = no` + a judge call ⇒ DRIFT.
    match decl.uses_judge {
        Tri::No if port.judge_call_attempts() > 0 => rows.push(fail(
            "property.judge_use",
            format!(
                "DRIFT — uses_judge = no but {} judge calls attempted",
                port.judge_call_attempts()
            ),
        )),
        Tri::Yes if decl.judge_ref.is_none() => rows.push(fail(
            "property.judge_use",
            "uses_judge = yes without a declared judge_ref",
        )),
        _ => rows.push(pass("property.judge_use")),
    }
    if drive.in_process {
        for probe in ["fs", "egress", "exec", "spawn"] {
            rows.push(na(
                &format!("property.isolation.{probe}"),
                "n/a{in_process} — the confined lane's probe",
            ));
        }
    }
    rows
}

/// The S2 shape check the executable kind applies: falsifiable deltas
/// over declared metrics, tasks inside the corpus universe, known kind.
fn hypothesis_s2_valid(h: &FailureHypothesis, corpus: &EvidenceCorpus) -> bool {
    if h.predicted.deltas.is_empty() || h.predicted.affected_task_ids.is_empty() {
        return false;
    }
    let kinds = [
        "observational",
        "designed_ablation",
        "causal_local",
        "causal_global",
    ];
    if !kinds.iter().any(|k| k == &h.kind) {
        return false;
    }
    h.predicted.deltas.iter().all(|d| {
        corpus.metric_refs.iter().any(|m| m == &d.metric)
            && matches!(d.direction.as_str(), "increase" | "decrease" | "preserves")
    }) && h
        .predicted
        .affected_task_ids
        .iter()
        .all(|t| corpus.task_ids.iter().any(|u| u == t))
        && !h.semantic_op_targets.is_empty()
}
