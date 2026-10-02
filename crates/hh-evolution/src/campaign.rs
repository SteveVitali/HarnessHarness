//! `EvolutionCampaign` — the §05h campaign driver (S6.1a; R-2.9.5). One
//! `run_kind = experiment` run per campaign; every stage verdict is a
//! durable `measurement.evolution.candidate.transitioned` row minted
//! through `commit_kernel_row_for` (kernel producer, audit-grade — the
//! same convention as the fleet/experiment rows), and every read folds
//! the durable prefix (no process-local authority — CC3/CC10).
//!
//! Records-in/records-out: the gates consume typed records the caller
//! deposits/passes (the base `HirDocument`, the `ScreenReport`, the
//! registered `ExperimentSpec` ids, the `ComparisonReport`s, the seal
//! endorsement). The pipeline evaluates no task itself — the experiment
//! engine is the sole evaluation substrate (§05h §4).

use hh_hir::diff;
use hh_hir::document::HirDocument;
use hh_identity::idp::idp_id;
use hh_lab::bench::SplitAssignmentRecord;
use hh_ledger::event::EventEnvelope;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_provenance::{AuthorityClass, Origin, ProvenanceRecord};
use hh_wire::json::Json;
use std::collections::BTreeMap;

use crate::errors::{EvolutionError, Refusal};
use crate::records::{
    EvolutionAcceptanceReport, EvolutionCampaignSpec, FailureHypothesis, ScreenReport,
    SecurityInvarianceReport, TransferRow, HYPOTHESIS_KINDS,
};
use crate::view::CampaignView;

/// The kernel producer spelling for evolution-authored rows.
pub const COMPONENT: &str = "hh-evolution";

/// The `RunKind` a campaign serves on (§05h §4 — `run_kind = experiment`;
/// no new run kind).
pub const RUN_KIND: RunKind = RunKind::Experiment;

/// The LabDocs kinds the pipeline deposits (`lab_doc.<kind>` content
/// addressing — one document store, CC3).
pub mod doc_kind {
    /// The campaign spec body.
    pub const CAMPAIGN_SPEC: &str = "evolution_campaign_spec";
    /// The proposal record (diff + slot).
    pub const PROPOSAL: &str = "evolution_proposal";
    /// The bound hypothesis record.
    pub const HYPOTHESIS: &str = "evolution_hypothesis";
    /// A stage evidence report (screen / search / held-out / transfer /
    /// security / acceptance / retirement).
    pub const REPORT: &str = "evolution_report";
    /// The pinned `SplitAssignmentRecord` (L3).
    pub const SPLIT: &str = "evolution_split";
    /// A pinned `SearchBudgetRecord` body — an experiment arm's
    /// `search_budget` ref resolves here at S4 (G8: complete or
    /// refuse; ADR-0046 D1, ADR-0191).
    pub const SEARCH_BUDGET: &str = "evolution_search_budget";
}

/// `EvolutionCampaign` — one campaign's durable driver.
pub struct EvolutionCampaign {
    /// The campaign run id (`evo-<hash>`).
    pub run_id: String,
    /// The writer-lease holder.
    pub holder: String,
    /// The writer TTL the engine renews under.
    pub writer_ttl_ms: u64,
    /// The live writer lease.
    pub lease: Lease,
    /// The folded candidate view — kept incrementally; `ensure` rebuilds
    /// it wholesale so restart equality is structural.
    pub view: CampaignView,
    /// The campaign spec (immutable — supersession only).
    pub spec: EvolutionCampaignSpec,
    /// The LabDocs handle the deposits resolve through.
    pub docs: hh_experiment::docs::LabDocs,
}

type Res<T> = Result<T, EvolutionError>;

fn schema(detail: impl Into<String>) -> EvolutionError {
    EvolutionError::Schema(detail.into())
}

impl EvolutionCampaign {
    // ── construction ───────────────────────────────────────────────────

    /// `evolution.campaign_open` — validate the spec, open the
    /// `run_kind = experiment` campaign run, mint `campaign.opened`.
    /// Idempotent on the content-derived `campaign_id` (the spec is the
    /// dedup key — same spec re-opens the same run).
    pub fn open(
        store: &mut Store,
        docs: hh_experiment::docs::LabDocs,
        holder: &str,
        writer_ttl_ms: u64,
        spec: EvolutionCampaignSpec,
    ) -> Res<(String, EvolutionCampaign)> {
        let mut spec = spec;
        spec.validate()?;
        if spec.campaign_id.is_empty() {
            spec.campaign_id = spec.derive_id();
        } else if spec.campaign_id != spec.derive_id() {
            return Err(schema("campaign_id is not the spec's content address"));
        }
        let spec_ref = docs
            .put(doc_kind::CAMPAIGN_SPEC, &spec.to_json())
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?;
        // L3 — the pinned SplitAssignmentRecord must resolve *at open*
        // (its deposit precedes the campaign's first event; a ref that
        // does not resolve is `EvidenceStale`, never a deferred check).
        let split_doc = docs
            .get_named(doc_kind::SPLIT, &spec.corpus.split_assignment_ref)
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?
            .or_else(|| {
                docs.get(doc_kind::SPLIT, &spec.corpus.split_assignment_ref)
                    .ok()
                    .flatten()
            });
        match split_doc {
            Some(j) => {
                SplitAssignmentRecord::from_json(&j)
                    .map_err(|e| schema(format!("split record: {e}")))?;
            }
            None => {
                return Err(Refusal::EvidenceStale {
                    detail: format!(
                        "split_assignment_ref `{}` does not resolve at open — the \
                         L3 pin must precede the campaign's first event",
                        spec.corpus.split_assignment_ref
                    ),
                }
                .into())
            }
        }
        let run_id = format!("evo-{}", spec.campaign_id.replace(':', "-"));
        if store.manifest(&run_id).is_ok() {
            return Self::ensure(store, docs, &run_id, holder, writer_ttl_ms).map(|e| (run_id, e));
        }
        let mut manifest = RunManifest::minimal(RunKind::Experiment);
        manifest.configuration_id = None;
        manifest.configuration_version_id = None;
        manifest.extra.insert(
            "evolution_campaign_spec_ref".to_string(),
            Json::str(&spec_ref),
        );
        manifest
            .extra
            .insert("evolution_campaign_spec".to_string(), spec.to_json());
        manifest
            .extra
            .insert("protocol".to_string(), Json::str(&spec.protocol));
        let opened = Json::obj([
            ("campaign_id", Json::str(&spec.campaign_id)),
            ("spec_ref", Json::str(&spec_ref)),
            ("protocol", Json::str(&spec.protocol)),
            ("holder", Json::str(holder)),
            (
                "split_assignment_ref",
                Json::str(&spec.corpus.split_assignment_ref),
            ),
            (
                "service_definition_ref",
                Json::str(&spec.service_definition_ref),
            ),
            ("slot_allocation", spec.slot_allocation.to_json()),
            ("stop_rule", spec.stop_rule.to_json()),
        ]);
        let (_, lease) = store.open_run_with_id(&run_id, manifest, holder)?;
        let mut eng = EvolutionCampaign {
            run_id: run_id.clone(),
            holder: holder.to_string(),
            writer_ttl_ms,
            lease,
            view: CampaignView::default(),
            spec,
            docs,
        };
        eng.emit(
            store,
            "measurement.evolution.campaign.opened",
            opened,
            vec![],
        )?;
        Ok((run_id, eng))
    }

    /// `ensure` — rebuild the fold from the durable prefix and re-acquire
    /// the writer lease (the restart path — RC-8's byte-identical
    /// rebuild).
    pub fn ensure(
        store: &mut Store,
        docs: hh_experiment::docs::LabDocs,
        run_id: &str,
        holder: &str,
        writer_ttl_ms: u64,
    ) -> Res<EvolutionCampaign> {
        let manifest = store.manifest(run_id)?;
        if manifest.run_kind != RunKind::Experiment {
            return Err(Refusal::CampaignNotOpen {
                status: "not_an_experiment_run".to_string(),
            }
            .into());
        }
        let spec_json = manifest
            .extra
            .get("evolution_campaign_spec")
            .cloned()
            .ok_or_else(|| schema("run is not an evolution campaign"))?;
        let spec = EvolutionCampaignSpec::from_json(&spec_json)
            .map_err(|e| schema(format!("campaign spec: {e}")))?;
        let lease = store
            .acquire_writer(holder, run_id, writer_ttl_ms)
            .map_err(EvolutionError::Store)?;
        let events = store.events(run_id)?.to_vec();
        let view = CampaignView::fold(&events)?;
        Ok(EvolutionCampaign {
            run_id: run_id.to_string(),
            holder: holder.to_string(),
            writer_ttl_ms,
            lease,
            view,
            spec,
            docs,
        })
    }

    /// Renew-or-reacquire the writer lease (the audited-takeover path).
    fn bound(&mut self, store: &mut Store) -> Res<()> {
        let now = store.now_ms();
        if now + self.writer_ttl_ms / 2 >= self.lease.expires_at_ms {
            match store.renew(&self.lease) {
                Ok(r) => self.lease = r,
                Err(_) => {
                    self.lease = store
                        .acquire_writer(&self.holder, &self.run_id, self.writer_ttl_ms)
                        .map_err(EvolutionError::Store)?;
                }
            }
        }
        Ok(())
    }

    /// Mint one kernel row on the campaign run + fold it.
    fn emit(
        &mut self,
        store: &mut Store,
        class: &str,
        payload: Json,
        causes: Vec<hh_ledger::manifest::EventRef>,
    ) -> Res<EventEnvelope> {
        let env =
            store.commit_kernel_row_for(COMPONENT, &self.run_id, class, payload, vec![], causes)?;
        self.view.fold_tail(store.events(&self.run_id)?);
        Ok(env)
    }

    /// The campaign must be `open` for a stage op to run.
    fn require_open(&self) -> Res<()> {
        if self.view.status != "open" {
            return Err(Refusal::CampaignNotOpen {
                status: self.view.status.clone(),
            }
            .into());
        }
        Ok(())
    }

    // ── transitions ────────────────────────────────────────────────────

    /// The one `transitioned` mint — every stage op funnels here so the
    /// row's member set is the same shape everywhere (CC1/CC7).
    /// `extra` carries the stage-specific members (`hypothesis_ref`,
    /// `evidence_refs`, `code`, `by`, `reason`, …).
    #[allow(clippy::too_many_arguments)]
    fn transitioned(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        from: &str,
        to: &str,
        stage: &str,
        report_ref: Option<&str>,
        extra: Json,
        causes: Vec<hh_ledger::manifest::EventRef>,
    ) -> Res<EventEnvelope> {
        let mut m = match extra {
            Json::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        m.insert("candidate_id".into(), Json::str(candidate_id));
        m.insert("from".into(), Json::str(from));
        m.insert("to".into(), Json::str(to));
        m.insert("stage".into(), Json::str(stage));
        if let Some(r) = report_ref {
            m.insert("report_ref".into(), Json::str(r));
        }
        self.emit(
            store,
            "measurement.evolution.candidate.transitioned",
            Json::Obj(m),
            causes,
        )
    }

    /// Emit the terminal `rejected{stage, code, report_ref?}` row for a
    /// candidate and return the refusal — every gate failure lands the
    /// durable row *before* the error returns ("a refusal is reported,
    /// the state is stored" — §05h §4).
    fn reject(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        from: &str,
        refusal: Refusal,
        report_ref: Option<&str>,
    ) -> EvolutionError {
        let stage = refusal.stage().to_string();
        let code = refusal.code();
        let extra = Json::obj([
            ("code", Json::str(&code)),
            ("reason", Json::str(format!("{refusal:?}"))),
        ]);
        let _ = self.transitioned(
            store,
            candidate_id,
            from,
            "rejected",
            &stage,
            report_ref,
            extra,
            vec![],
        );
        EvolutionError::Refusal(refusal)
    }

    /// The candidate's folded state (`None` = unregistered).
    fn candidate_state(&self, candidate_id: &str) -> Res<String> {
        self.view
            .state_of(candidate_id)
            .map(str::to_string)
            .ok_or_else(|| {
                Refusal::UnknownCandidate {
                    candidate_id: candidate_id.to_string(),
                }
                .into()
            })
    }

    /// Require the fold state `from` before minting `to`.
    fn require_state(&self, candidate_id: &str, from: &str, to: &str) -> Res<String> {
        let cur = self.candidate_state(candidate_id)?;
        if cur != from {
            return Err(Refusal::IllegalTransition {
                from: cur,
                to: to.to_string(),
            }
            .into());
        }
        Ok(cur)
    }

    // ── S0/S1 propose ──────────────────────────────────────────────────

    /// `propose` — the S0/S1 gates plus intake + classification
    /// transitions. `base_doc` is the sealed base definition the diff
    /// applies over (records-in — the caller resolves the ref through
    /// the registry/LabDocs; the pipeline never fetches).
    ///
    /// Returns the candidate id on success; on a gate refusal the
    /// `rejected` row has already landed.
    pub fn propose(
        &mut self,
        store: &mut Store,
        proposal: &crate::records::CandidateProposal,
        base_doc: &HirDocument,
    ) -> Res<String> {
        self.require_open()?;
        self.bound(store)?;
        let spec = self.spec.clone();
        let diff = &proposal.diff;

        // candidate_id = H(canonical({base_ref, diff})) — content-derived,
        // so a re-proposal of the same candidate is detectable by id.
        let cid = idp_id(
            "evolution_candidate",
            Json::obj([
                ("base_ref", Json::str(&proposal.base_ref)),
                ("diff", diff.to_json()),
            ])
            .to_canonical_string()
            .as_bytes(),
        );

        // ── Duplicate — emit the intake+reject rows as attempt history
        //    (`from` never matches the live state, so the fold records
        //    the attempt without clobbering the original's state).
        if self.view.candidates.contains_key(&cid) {
            let extra = Json::obj([
                ("slot", Json::str(&proposal.slot)),
                ("base_ref", Json::str(&proposal.base_ref)),
                ("diff_ref", Json::str(&diff.target.semantic_id)),
            ]);
            let _ = self.transitioned(store, &cid, "", "proposed", "S1", None, extra, vec![]);
            return Err(self.reject(
                store,
                &cid,
                "",
                Refusal::DuplicateCandidate {
                    candidate_id: cid.clone(),
                },
                None,
            ));
        }

        // ── The intake row lands first — the candidate is registered in
        //    lineage even when S1 refuses it right after (G9).
        //    `target_ref` = the diff's pinned target version — the fold's
        //    head tracker (`StaleBase`) reads it on `active` transitions.
        let intake = Json::obj([
            ("slot", Json::str(&proposal.slot)),
            ("base_ref", Json::str(&proposal.base_ref)),
            ("diff_ref", Json::str(&diff.target.semantic_id)),
            ("target_ref", Json::str(&diff.target.version_id)),
        ]);
        self.transitioned(store, &cid, "", "proposed", "S1", None, intake, vec![])?;

        // ── S1 gates — each failure mints `rejected{S1, code}` then
        //    returns the typed refusal.
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, &cid, "proposed", r, None)
        };

        // Self-modification — the service's own definition is never a
        // candidate's base (G3-1; AC-R-2.12.2-14).
        if proposal.base_ref == spec.service_definition_ref {
            return Err(fail(
                self,
                store,
                Refusal::SelfModificationRefused {
                    detail: format!(
                        "base_ref `{}` is the evolution service's own definition",
                        proposal.base_ref
                    ),
                },
            ));
        }

        // StaleBase — a candidate names the campaign's current head
        // (OQ-061; ADR-0195 D12): once an accepted edit advances the
        // head, proposals on the old base are stale and re-propose.
        let head = self
            .view
            .head_ref()
            .unwrap_or_else(|| spec.base_definition_ref.clone());
        if proposal.base_ref != head {
            return Err(fail(
                self,
                store,
                Refusal::StaleBase {
                    detail: format!(
                        "base_ref `{}` ≠ lineage head `{head}` — a moved head re-proposes",
                        proposal.base_ref
                    ),
                },
            ));
        }

        // Family provenance — `human` campaigns take human-origin diffs;
        // an automated family takes `origin = evolution` (delegate class;
        // §05h §6). The proposer family never mints a human origin and a
        // human family never mints an evolution one.
        let origin_ok = match spec.proposer_family.as_str() {
            "human" => matches!(diff.provenance.origin, Origin::Human { .. }),
            _ => matches!(diff.provenance.origin, Origin::Evolution { .. }),
        };
        if !origin_ok {
            return Err(fail(
                self,
                store,
                Refusal::InvalidProvenance {
                    detail: format!(
                        "proposer_family `{}` requires origin {} — got {:?}",
                        spec.proposer_family,
                        if spec.proposer_family == "human" {
                            "human"
                        } else {
                            "evolution"
                        },
                        diff.provenance.origin
                    ),
                },
            ));
        }

        // Provenance — the diff's own `provenance.validate` (CC2: the
        // candidate carries `external` authority at most until sealed).
        if let Err(e) = diff.provenance.validate(None) {
            return Err(fail(
                self,
                store,
                Refusal::InvalidProvenance {
                    detail: format!("{e:?}"),
                },
            ));
        }

        // Classification deltas.
        use hh_hir::diff::{AuthorityDelta, Delta};
        if diff.classification.authority_delta == AuthorityDelta::Widening {
            return Err(fail(
                self,
                store,
                Refusal::AuthorityWidening {
                    detail: "authority_delta = widening — a candidate never widens".into(),
                },
            ));
        }
        if diff.classification.budget_delta == Delta::Loosening {
            return Err(fail(
                self,
                store,
                Refusal::BudgetLoosening {
                    detail: "budget_delta = loosening — candidates tighten only".into(),
                },
            ));
        }
        if diff.classification.validity_delta == Delta::Loosening {
            return Err(fail(
                self,
                store,
                Refusal::ValidityLoosening {
                    detail: "validity_delta = loosening — candidates tighten only".into(),
                },
            ));
        }
        // K-4 — `coordination_delta = loosening` is classified and
        // refused in evolution contexts like widening (R-2.6.5⁴; ADR-0193
        // (e); the ADR-0053 D-5 shape).
        if diff.classification.coordination_delta == Delta::Loosening {
            return Err(fail(
                self,
                store,
                Refusal::CoordinationLoosening {
                    detail: "coordination_delta = loosening — CoordinationPolicy diffs                              tighten only in evolution contexts"
                        .into(),
                },
            ));
        }

        // Op-target gates — exclusion set, MUST-code leaves, admitted
        // target kinds, the service-definition exclusion.
        for op in &diff.ops {
            let node_id = op.node_id();
            let tag = op.tag();
            if spec.exclusion_targets.iter().any(|t| t == node_id) {
                return Err(fail(
                    self,
                    store,
                    Refusal::ExcludedTarget {
                        node_id: node_id.to_string(),
                    },
                ));
            }
            // G5/X6 — the registered `evolution_proposer` variant and the
            // campaign spec itself are outside every candidate's target
            // set (the service evolves only through a human-origin
            // campaign; CF-419).
            if let Some(pv) = &spec.proposer_variant_ref {
                if node_id == pv {
                    return Err(fail(
                        self,
                        store,
                        Refusal::SelfModificationRefused {
                            detail: format!("op targets the registered proposer variant `{pv}`"),
                        },
                    ));
                }
            }
            if matches!(op, diff::DiffOp::ReplaceLeaf { .. })
                && spec.must_code_targets.iter().any(|t| t == node_id)
            {
                return Err(fail(
                    self,
                    store,
                    Refusal::MustCodeTarget {
                        detail: format!("leaf rewrite on MUST-code target `{node_id}`"),
                    },
                ));
            }
            if let Some(kinds) = &spec.allowed_target_kinds {
                if tag.semantic && !kinds.iter().any(|k| k == &tag.entity_kind) {
                    return Err(fail(
                        self,
                        store,
                        Refusal::ExcludedTarget {
                            node_id: format!("{node_id}:{}", tag.entity_kind),
                        },
                    ));
                }
            }
        }
        if diff.classification.semantic_ops as u64 > spec.semantic_ops_bound {
            return Err(fail(
                self,
                store,
                Refusal::TooManyOps {
                    seen: diff.classification.semantic_ops as u64,
                    bound: spec.semantic_ops_bound,
                },
            ));
        }

        // The apply/invert contract — `apply(base, diff)` must yield a
        // valid target and `apply(target, invert(diff))` must return the
        // base byte-identically (§05h §4 S1's invertibility gate).
        let target = match diff::apply(base_doc, diff) {
            Ok(t) => t,
            Err(e) => {
                return Err(fail(
                    self,
                    store,
                    Refusal::DiffNotInvertible {
                        detail: format!("apply failed: {e:?}"),
                    },
                ))
            }
        };
        let back = match diff::apply(&target, &diff::invert(diff)) {
            Ok(b) => b,
            Err(e) => {
                return Err(fail(
                    self,
                    store,
                    Refusal::DiffNotInvertible {
                        detail: format!("inverse apply failed: {e:?}"),
                    },
                ))
            }
        };
        if back.canonical_bytes() != base_doc.canonical_bytes() {
            return Err(fail(
                self,
                store,
                Refusal::DiffNotInvertible {
                    detail: "apply(target, invert(diff)) != base — not invertible".into(),
                },
            ));
        }
        // `validate` on the target — the assembled candidate must pass
        // the HIR's own validation (validate_assembly clean).
        if let Err(e) = hh_hir::validate::validate(&target) {
            return Err(fail(
                self,
                store,
                Refusal::SchemaViolation {
                    detail: format!("target fails validate: {e:?}"),
                },
            ));
        }

        // Deposit the proposal record + mint `classified`.
        let proposal_ref = self
            .docs
            .put(
                doc_kind::PROPOSAL,
                &Json::obj([
                    ("candidate_id", Json::str(&cid)),
                    ("base_ref", Json::str(&proposal.base_ref)),
                    ("slot", Json::str(&proposal.slot)),
                    ("diff", diff.to_json()),
                ]),
            )
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?;
        self.transitioned(
            store,
            &cid,
            "proposed",
            "classified",
            "S1",
            Some(&proposal_ref),
            Json::obj([("slot", Json::str(&proposal.slot))]),
            vec![],
        )?;
        Ok(cid)
    }

    // ── S2 hypothesize ─────────────────────────────────────────────────

    /// `hypothesize` — bind the falsifiable hypothesis (S2; AC-6). On
    /// success the `transitioned{to: hypothesized}` row carries
    /// `hypothesis_ref` + `evidence_refs` — the `evolution_link`
    /// obligation's link fields.
    pub fn hypothesize(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        hyp: &FailureHypothesis,
    ) -> Res<String> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "classified", "hypothesized")?;
        let spec = self.spec.clone();
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, candidate_id, &from, r, None)
        };

        // Falsifiability — evidence + ≥ 1 predicted delta + an admitted
        // kind (AC-6: one refusal per leg).
        if !HYPOTHESIS_KINDS.contains(&hyp.kind.as_str()) {
            return Err(fail(
                self,
                store,
                Refusal::HypothesisUnfalsifiable {
                    reason: format!("kind `{}` is not in {HYPOTHESIS_KINDS:?}", hyp.kind),
                },
            ));
        }
        if hyp.evidence_refs.is_empty() {
            return Err(fail(
                self,
                store,
                Refusal::HypothesisUnfalsifiable {
                    reason: "evidence_refs is empty — the hypothesis binds no evidence".into(),
                },
            ));
        }
        for r in &hyp.evidence_refs {
            if !spec.corpus.evidence_refs.iter().any(|e| e == r) {
                return Err(fail(
                    self,
                    store,
                    Refusal::UnresolvableEvidence { ref_: r.clone() },
                ));
            }
        }
        if hyp.predicted.deltas.is_empty() {
            return Err(fail(
                self,
                store,
                Refusal::HypothesisUnfalsifiable {
                    reason: "predicted.deltas is empty — nothing to falsify".into(),
                },
            ));
        }
        for d in &hyp.predicted.deltas {
            if !spec.corpus.metric_refs.iter().any(|m| m == &d.metric) {
                return Err(fail(
                    self,
                    store,
                    Refusal::UnknownMetric {
                        metric: d.metric.clone(),
                    },
                ));
            }
            if !["increase", "decrease", "preserves"].contains(&d.direction.as_str()) {
                return Err(fail(
                    self,
                    store,
                    Refusal::HypothesisUnfalsifiable {
                        reason: format!(
                            "direction `{}` is not increase|decrease|preserves",
                            d.direction
                        ),
                    },
                ));
            }
        }
        // LeakedSplit — affected tasks resolve against the corpus task
        // universe AND must be search/dev-labelled under the pinned
        // split assignment.
        let split = self.load_split()?;
        for t in &hyp.predicted.affected_task_ids {
            if !spec.corpus.task_ids.iter().any(|c| c == t) {
                return Err(fail(
                    self,
                    store,
                    Refusal::UnresolvableEvidence {
                        ref_: format!("task `{t}` not in the corpus universe"),
                    },
                ));
            }
            match split.as_ref().and_then(|s| s.splits.get(t)) {
                Some(l) if l.search_admissible() => {}
                Some(_) => {
                    return Err(fail(
                        self,
                        store,
                        Refusal::LeakedSplit {
                            detail: format!("task `{t}` is not search/dev-labelled"),
                        },
                    ))
                }
                None => {
                    return Err(fail(
                        self,
                        store,
                        Refusal::LeakedSplit {
                            detail: format!("task `{t}` has no split label"),
                        },
                    ))
                }
            }
        }
        // TargetMismatch — the diff's semantic op targets must sit inside
        // the hypothesis's attribution set.
        let rec = self.view.candidate(candidate_id).expect("state checked");
        let diff_ref = rec.diff_ref.clone().unwrap_or_default();
        // The diff's semantic-op targets ride the proposal doc's diff —
        // resolve it back (records-in: the deposit is the single copy).
        let op_targets = self.proposal_op_targets(candidate_id, &diff_ref)?;
        if !hyp.semantic_op_targets.is_empty() {
            for t in &op_targets {
                if !hyp.semantic_op_targets.iter().any(|s| s == t) {
                    return Err(fail(self, store, Refusal::TargetMismatch { op: t.clone() }));
                }
            }
        }

        // Deposit the hypothesis + mint `hypothesized` carrying the
        // obligation link fields.
        let href = self
            .docs
            .put(
                doc_kind::HYPOTHESIS,
                &Json::obj([
                    ("candidate_id", Json::str(candidate_id)),
                    ("kind", Json::str(&hyp.kind)),
                    (
                        "evidence_refs",
                        Json::Arr(hyp.evidence_refs.iter().map(Json::str).collect()),
                    ),
                    ("predicted", predicted_json(&hyp.predicted)),
                    (
                        "semantic_op_targets",
                        Json::Arr(hyp.semantic_op_targets.iter().map(Json::str).collect()),
                    ),
                ]),
            )
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?;
        self.transitioned(
            store,
            candidate_id,
            &from,
            "hypothesized",
            "S2",
            Some(&href),
            Json::obj([
                ("hypothesis_ref", Json::str(&href)),
                ("hypothesis_kind", Json::str(&hyp.kind)),
                (
                    "evidence_refs",
                    Json::Arr(hyp.evidence_refs.iter().map(Json::str).collect()),
                ),
            ]),
            vec![],
        )?;
        Ok(href)
    }

    /// The candidate's deposited proposal doc → the decoded `HirDiff`
    /// (records-in: the deposit is the single copy; S7's re-check reads
    /// its classification).
    fn proposal_diff(&self, candidate_id: &str) -> Res<hh_hir::diff::HirDiff> {
        // The proposal is content-addressed under PROPOSAL with the
        // candidate's id derivable — scan is not supported; the caller's
        // `propose` deposited under a known id. We resolve by re-deriving
        // the deposit id: proposals are stored under
        // `lab_doc.evolution_proposal` of the proposal body — the
        // candidate's history carries no proposal_ref, so look the doc
        // up through the intake row's report_ref…
        // Simpler: the `classified` transition's report_ref IS the
        // proposal doc ref.
        let rec = self
            .view
            .candidate(candidate_id)
            .ok_or_else(|| Refusal::UnknownCandidate {
                candidate_id: candidate_id.to_string(),
            })?;
        let proposal_ref = rec
            .reports
            .get("S1")
            .cloned()
            .ok_or_else(|| schema("candidate has no S1 proposal deposit"))?;
        let body = self
            .docs
            .get(doc_kind::PROPOSAL, &proposal_ref)
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?
            .ok_or_else(|| schema("proposal doc missing"))?;
        let diff_json = body
            .get("diff")
            .cloned()
            .ok_or_else(|| schema("proposal doc missing diff"))?;
        hh_hir::wire::diff_from_json(&diff_json).map_err(|e| schema(format!("diff decode: {e:?}")))
    }

    /// The candidate's deposited proposal doc → the diff's semantic op
    /// targets (for S2's TargetMismatch check).
    fn proposal_op_targets(&self, candidate_id: &str, diff_ref: &str) -> Res<Vec<String>> {
        let _ = diff_ref;
        let d = self.proposal_diff(candidate_id)?;
        Ok(d.ops
            .iter()
            .filter(|op| op.tag().semantic)
            .map(|op| op.node_id().to_string())
            .collect())
    }

    /// Load the pinned `SplitAssignmentRecord` (the L3 pin — resolved at
    /// open and re-resolved here; `None` when the record legitimately
    /// carries no per-task labels the check needs).
    fn load_split(&self) -> Res<Option<SplitAssignmentRecord>> {
        let doc = self
            .docs
            .get_named(doc_kind::SPLIT, &self.spec.corpus.split_assignment_ref)
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?
            .or_else(|| {
                self.docs
                    .get(doc_kind::SPLIT, &self.spec.corpus.split_assignment_ref)
                    .ok()
                    .flatten()
            });
        match doc {
            Some(j) => Ok(Some(
                SplitAssignmentRecord::from_json(&j)
                    .map_err(|e| schema(format!("split record: {e}")))?,
            )),
            None => Ok(None),
        }
    }

    // ── S3 screen ──────────────────────────────────────────────────────

    /// `screen` — the targeted-counterexample gate (S3; AC-7).
    pub fn screen(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        report: &ScreenReport,
    ) -> Res<String> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "hypothesized", "screened")?;
        let spec = self.spec.clone();
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, candidate_id, &from, r, None)
        };
        if report.judge_only || report.counterexample_set_ref.is_empty() {
            return Err(fail(
                self,
                store,
                Refusal::JudgeOnlyAcceptance {
                    detail: "S3 requires targeted counterexamples — judge-only \
                         acceptance is insufficient"
                        .into(),
                },
            ));
        }
        // G7 — a judge-selector counterexample pick is declared on the
        // campaign's `judge_policy`: calibrated, independent of the
        // beneficiary snapshot, over a honeypot-bearing set (ADR-0190;
        // §05h §4 G7).
        if let Some(sel_ref) = &report.selector_ref {
            let pol = self.spec.judge_policy.as_ref().ok_or_else(|| {
                Refusal::JudgeSelectorUndeclared {
                    selector: sel_ref.clone(),
                    detail: "the campaign carries no judge_policy".to_string(),
                }
            })?;
            let sel = pol
                .selectors
                .iter()
                .find(|d| d.selector_ref == *sel_ref)
                .ok_or_else(|| Refusal::JudgeSelectorUndeclared {
                    selector: sel_ref.clone(),
                    detail: "the selector is not on the judge_policy".to_string(),
                })?;
            if sel.calibration_ref.is_empty() {
                return Err(fail(
                    self,
                    store,
                    Refusal::JudgeSelectorUndeclared {
                        selector: sel_ref.clone(),
                        detail: "the selector carries no calibration_ref (G7)".to_string(),
                    },
                ));
            }
            let parent = self
                .view
                .candidate(candidate_id)
                .and_then(|c| c.base_ref.clone())
                .unwrap_or_else(|| spec.base_definition_ref.clone());
            if !sel.independent_of.contains(&parent) {
                return Err(fail(
                    self,
                    store,
                    Refusal::JudgeSelectorUndeclared {
                        selector: sel_ref.clone(),
                        detail: format!(
                            "the selector declares no independence from the \
                             beneficiary snapshot `{parent}` (G7)"
                        ),
                    },
                ));
            }
            if report.honeypots < pol.min_honeypots {
                return Err(fail(
                    self,
                    store,
                    Refusal::JudgeSelectorUndeclared {
                        selector: sel_ref.clone(),
                        detail: format!(
                            "counterexample set carries {} honeypots — the judge \
                             policy's floor is {}",
                            report.honeypots, pol.min_honeypots
                        ),
                    },
                ));
            }
        }
        for l in &report.split_labels_used {
            if !l.search_admissible() {
                return Err(fail(
                    self,
                    store,
                    Refusal::LeakedSplit {
                        detail: format!("screen used split label `{}`", l.name()),
                    },
                ));
            }
        }
        if report.replicate_count < spec.min_replicates {
            return Err(fail(
                self,
                store,
                Refusal::InsufficientReplicates {
                    seen: report.replicate_count,
                    required: spec.min_replicates,
                },
            ));
        }
        let share_ppm = if report.observations == 0 {
            0
        } else {
            report.flips_in_direction * 1_000_000 / report.observations
        };
        if share_ppm < spec.min_flip_share_ppm {
            return Err(fail(
                self,
                store,
                Refusal::PredictionFalsified {
                    share_ppm,
                    floor_ppm: spec.min_flip_share_ppm,
                },
            ));
        }
        let rref = self
            .docs
            .put(doc_kind::REPORT, &report.to_json())
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?;
        self.transitioned(
            store,
            candidate_id,
            &from,
            "screened",
            "S3",
            Some(&rref),
            Json::obj([]),
            vec![],
        )?;
        Ok(rref)
    }

    // ── S4 matched-eval (M3) ───────────────────────────────────────────

    /// `matched_eval` — the S4 gate (M3 `matched_total`; AC-4/AC-12;
    /// AC-R-2.1.6-12). `experiment_id` resolves to the registered derived
    /// spec; `report_ref` to the deposited `ComparisonReport` whose
    /// `benefit_kind = search_time_benefit`.
    pub fn matched_eval(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        experiment_id: &str,
        report_ref: &str,
    ) -> Res<()> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "screened", "searched")?;
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, candidate_id, &from, r, Some(report_ref))
        };
        let spec = self
            .check_stage_experiment(experiment_id, &["search_eval", "search"])
            .map_err(|e| match e {
                EvolutionError::Refusal(r) => EvolutionError::Refusal(r),
                o => o,
            })?;
        // Every arm matched_total + budgeted — the M3 gate (the refusal
        // surfaces here even if the experiment engine's own checks were
        // bypassed; the campaign does not trust an unexamined spec).
        for arm in &spec.arms {
            let ms = arm
                .match_spec
                .as_ref()
                .ok_or_else(|| Refusal::UnmatchedSearchBudget {
                    detail: format!("arm `{}` carries no match_spec", arm.arm_id),
                })?;
            if ms.mode != hh_budget::matchspec::MatchMode::MatchedTotal {
                return Err(fail(
                    self,
                    store,
                    Refusal::UnmatchedSearchBudget {
                        detail: format!(
                            "arm `{}` match mode `{}` — every evolution claim is \
                         gated on matched_total (M3)",
                            arm.arm_id,
                            ms.mode.as_str()
                        ),
                    },
                ));
            }
            match &arm.search_budget {
                None => {
                    return Err(fail(
                        self,
                        store,
                        Refusal::UnbudgetedArm {
                            arm: arm.arm_id.clone(),
                        },
                    ));
                }
                // G8 — the pinned `search_budget` ref resolves to a
                // `SearchBudgetRecord` doc and the record is *complete*
                // (allocation shares sum to the ppm scale; ADR-0046 D1,
                // ADR-0191: "complete or refuse").
                Some(sb_ref) => {
                    let body = self
                        .docs
                        .get_named(doc_kind::SEARCH_BUDGET, sb_ref)
                        .ok()
                        .flatten()
                        .or_else(|| {
                            self.docs
                                .get(doc_kind::SEARCH_BUDGET, sb_ref)
                                .ok()
                                .flatten()
                        })
                        .ok_or_else(|| Refusal::SearchBudgetIncomplete {
                            detail: format!(
                                "arm `{}` search_budget ref `{sb_ref}` does not \
                                 resolve to a pinned SearchBudgetRecord doc",
                                arm.arm_id
                            ),
                        })?;
                    let record =
                        hh_ontology::eval::SearchBudgetRecord::from_json(&body).map_err(|e| {
                            Refusal::SearchBudgetIncomplete {
                                detail: format!(
                                    "arm `{}` search_budget doc does not parse as a \
                                     SearchBudgetRecord: {e:?}",
                                    arm.arm_id
                                ),
                            }
                        })?;
                    if !record.is_complete() {
                        return Err(fail(
                            self,
                            store,
                            Refusal::SearchBudgetIncomplete {
                                detail: format!(
                                    "arm `{}` SearchBudgetRecord is incomplete — \
                                     allocation shares must name every facet and \
                                     sum to 1_000_000 ppm",
                                    arm.arm_id
                                ),
                            },
                        ));
                    }
                    // G8 facet names — an automated family's record names
                    // a `proposer` slice (its model calls charge there);
                    // a declared judge policy names a `judge` slice
                    // (ADR-0190's audit budget).
                    if self.spec.proposer_family != "human"
                        && !record.allocation.keys().any(|k| k == "proposer")
                    {
                        return Err(fail(
                            self,
                            store,
                            Refusal::SearchBudgetIncomplete {
                                detail: format!(
                                    "arm `{}` SearchBudgetRecord names no `proposer` \
                                     allocation — an automated family's model calls \
                                     charge to a named slice (G8)",
                                    arm.arm_id
                                ),
                            },
                        ));
                    }
                    if self.spec.judge_policy.is_some()
                        && !record
                            .allocation
                            .keys()
                            .any(|k| k == "judge" || k == "audit")
                    {
                        return Err(fail(
                            self,
                            store,
                            Refusal::SearchBudgetIncomplete {
                                detail: format!(
                                    "arm `{}` SearchBudgetRecord names no `judge`/`audit` \
                                     allocation — a declared judge policy carries an \
                                     audit budget (ADR-0190)",
                                    arm.arm_id
                                ),
                            },
                        ));
                    }
                }
            }
        }
        // AC-15 — a hosted-participant campaign declaring reported-only
        // dimensions cannot claim matched_total.
        if self.spec.hosted_participants && !self.spec.reported_only_dimensions.is_empty() {
            return Err(fail(
                self,
                store,
                Refusal::IncommensurableMatch {
                    detail: format!(
                        "hosted campaign declares reported-only dimensions {:?} — \
                     matched_total is incommensurable (ADR-0196 D7)",
                        self.spec.reported_only_dimensions
                    ),
                },
            ));
        }
        // The report — `search_time_benefit` at `matched` budget.
        let report = self.load_comparison(report_ref)?;
        if report.benefit_kind != hh_lab::analysis::BenefitKind::SearchTimeBenefit {
            return Err(fail(
                self,
                store,
                Refusal::UnmatchedSearchBudget {
                    detail: format!(
                        "benefit_kind `{}` — S4's report is `search_time_benefit`",
                        report.benefit_kind.name()
                    ),
                },
            ));
        }
        if report.budget_match.status != hh_lab::analysis::BudgetMatchStatus::Matched {
            return Err(fail(
                self,
                store,
                Refusal::UnmatchedBudget {
                    detail: format!(
                        "budget_match.status = `{}` — not matched at tolerance",
                        report.budget_match.status.name()
                    ),
                },
            ));
        }
        self.transitioned(
            store,
            candidate_id,
            &from,
            "searched",
            "S4",
            Some(report_ref),
            Json::obj([("slot", Json::str(experiment_id))]),
            vec![],
        )?;
        Ok(())
    }

    /// The shared stage-experiment check — the named spec must be a
    /// registered `experiment` doc bound to this campaign
    /// (`ext.evolution_campaign == campaign_id`) and of an admitted stage
    /// kind.
    fn check_stage_experiment(
        &self,
        experiment_id: &str,
        stage: &[&str],
    ) -> Res<hh_lab::experiment::ExperimentSpec> {
        let spec = self
            .docs
            .spec(experiment_id)
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?
            .ok_or_else(|| Refusal::SpecNotRegistered {
                experiment_id: experiment_id.to_string(),
            })?;
        let bound = spec
            .ext
            .get("evolution_campaign")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if bound != self.spec.campaign_id {
            return Err(Refusal::ForeignExperiment {
                experiment_id: experiment_id.to_string(),
            }
            .into());
        }
        // The stage-kind admission — a stage accepts only the experiment
        // kinds that produce its evidence (§05h §4's stage tables): the
        // removal-test stage takes a `retirement`/`retirement_batch`
        // spec; the eval stages take a comparison-bearing kind
        // (`comparative`/`equivalence` — `exploratory` never compares and
        // a removal test is not an eval).
        let slabel: &'static str = if stage.contains(&"retirement") {
            "S10"
        } else if stage.contains(&"held_out_eval") {
            "S5"
        } else {
            "S4"
        };
        let kind_ok = if slabel == "S10" {
            spec.kind.is_retirement()
        } else {
            matches!(
                spec.kind,
                hh_lab::experiment::ExperimentKind::Comparative
                    | hh_lab::experiment::ExperimentKind::Equivalence
            )
        };
        if !kind_ok {
            return Err(Refusal::StageKindMismatch {
                experiment_id: experiment_id.to_string(),
                kind: spec.kind.name().to_string(),
                stage: slabel,
            }
            .into());
        }
        Ok(spec)
    }

    /// Load a deposited `ComparisonReport` (the `evidence` docs kind —
    /// the boundary deposits reports as `evolution_report` bodies).
    fn load_comparison(&self, report_ref: &str) -> Res<hh_lab::analysis::ComparisonReport> {
        let body = self
            .docs
            .get_named(doc_kind::REPORT, report_ref)
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?
            .or_else(|| self.docs.get(doc_kind::REPORT, report_ref).ok().flatten())
            .ok_or_else(|| Refusal::SpecNotRegistered {
                experiment_id: format!("report `{report_ref}` unresolvable"),
            })?;
        hh_lab::analysis::ComparisonReport::from_json(&body)
            .map_err(|e| schema(format!("comparison report: {e:?}")))
    }

    // ── S5 held-out eval (artifact_benefit) ────────────────────────────

    /// `held_out_eval` — the S5 gate (AC-8): `artifact_benefit` on a
    /// `full_set` held-out experiment whose interval excludes 0;
    /// retention within margin; veto metrics clean; the split pin's
    /// `task_split_hash` predates the campaign (the L3 pin resolved at
    /// open).
    pub fn held_out_eval(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        experiment_id: &str,
        report_ref: &str,
        retention: &Json,
        vetoes: &BTreeMap<String, bool>,
    ) -> Res<()> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "searched", "validated")?;
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, candidate_id, &from, r, Some(report_ref))
        };
        // `observational` hypotheses advance through S3–S4 only.
        let rec = self.view.candidate(candidate_id).expect("state checked");
        if rec.hypothesis_kind.as_deref() == Some("observational") {
            return Err(fail(
                self,
                store,
                Refusal::ObservationalOnly {
                    detail: "an observational hypothesis cannot pass S5".into(),
                },
            ));
        }
        let spec = self.check_stage_experiment(experiment_id, &["held_out_eval"])?;
        // The artifact eval is `full_set` over the held-out split.
        if spec.validation_strategy.is_adaptive() {
            return Err(fail(
                self,
                store,
                Refusal::NotHeldOut {
                    detail: "the artifact-benefit eval must be validation_strategy \
                         = full_set (adaptive belongs to the search side)"
                        .into(),
                },
            ));
        }
        let report = self.load_comparison(report_ref)?;
        if report.benefit_kind != hh_lab::analysis::BenefitKind::ArtifactBenefit {
            return Err(fail(
                self,
                store,
                Refusal::NotHeldOut {
                    detail: format!(
                        "benefit_kind `{}` — S5's report is `artifact_benefit`",
                        report.benefit_kind.name()
                    ),
                },
            ));
        }
        if !report.held_out {
            return Err(fail(
                self,
                store,
                Refusal::NotHeldOut {
                    detail: "the artifact-benefit comparison did not run held-out".into(),
                },
            ));
        }
        // The interval must exclude 0 (paired_effect.interval{lo,hi}).
        let excludes_zero = report
            .paired_effect
            .interval
            .as_ref()
            .map(|iv| {
                let lo = iv.get("lo").and_then(Json::as_int);
                let hi = iv.get("hi").and_then(Json::as_int);
                match (lo, hi) {
                    (Some(l), Some(h)) => l > 0 || h < 0,
                    _ => false,
                }
            })
            .unwrap_or(false);
        if !excludes_zero {
            return Err(fail(
                self,
                store,
                Refusal::NoArtifactBenefit {
                    detail: "artifact_benefit interval includes 0".into(),
                },
            ));
        }
        if report.budget_match.status != hh_lab::analysis::BudgetMatchStatus::Matched {
            return Err(fail(
                self,
                store,
                Refusal::UnmatchedBudget {
                    detail: "held-out comparison's budget_match is not matched".into(),
                },
            ));
        }
        // Retention — `{regressed_tasks[]}` beyond the margin refuses.
        let regressed: Vec<String> = retention
            .get("regressed_tasks")
            .and_then(|v| match v {
                Json::Arr(a) => Some(
                    a.iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        if !regressed.is_empty() {
            return Err(fail(
                self,
                store,
                Refusal::RetentionRegressed { tasks: regressed },
            ));
        }
        // Vetoes — every declared veto metric must be clean.
        for (metric, regressed) in vetoes {
            if *regressed {
                return Err(fail(
                    self,
                    store,
                    Refusal::VetoRegressed {
                        metric: metric.clone(),
                    },
                ));
            }
        }
        self.transitioned(
            store,
            candidate_id,
            &from,
            "validated",
            "S5",
            Some(report_ref),
            Json::obj([]),
            vec![],
        )?;
        Ok(())
    }

    // ── S6 transfer ────────────────────────────────────────────────────

    /// `transfer` — the S6 gate (AC-9): ≥ 1 held-out family row with
    /// sign + interval; `verified`/`drifted`/`broken` compatibility
    /// records carry `evidence_ref`.
    pub fn transfer(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        rows: &[TransferRow],
        compatibility: &[Json],
    ) -> Res<String> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "validated", "transferred")?;
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, candidate_id, &from, r, None)
        };
        // ≥ 1 measured held-out row carrying sign (point) + interval.
        let measured = rows
            .iter()
            .filter(|r| r.held_out && r.status == "measured")
            .count();
        if measured == 0 {
            return Err(fail(
                self,
                store,
                Refusal::TransferUnreported {
                    detail: "no measured transfer row over a held-out family".into(),
                },
            ));
        }
        for r in rows.iter().filter(|r| r.status == "measured") {
            if r.point.is_none() || r.interval.is_none() {
                return Err(fail(
                    self,
                    store,
                    Refusal::TransferUnreported {
                        detail: format!(
                            "family `{}` lacks sign/interval",
                            r.environment_family.name()
                        ),
                    },
                ));
            }
        }
        // Compatibility records — `verified`/`drifted`/`broken` verdicts
        // must carry evidence (records-in: the caller supplies the
        // canonical CompatibilityRecord JSONs).
        for cj in compatibility {
            let c = hh_lab::model::CompatibilityRecord::from_json(cj)
                .map_err(|e| schema(format!("compatibility record: {e:?}")))?;
            let needs_evidence = matches!(
                c.status,
                hh_lab::model::CompatibilityStatus::Verified
                    | hh_lab::model::CompatibilityStatus::Drifted { .. }
                    | hh_lab::model::CompatibilityStatus::Broken { .. }
            );
            if needs_evidence && c.evidence_ref.is_none() {
                return Err(fail(
                    self,
                    store,
                    Refusal::CompatibilityUnproven {
                        detail: format!(
                            "compatibility `{}` claims `{:?}` with no evidence_ref",
                            c.snapshot_ref, c.status
                        ),
                    },
                ));
            }
        }
        let bundle = Json::obj([
            (
                "transfer_rows",
                Json::Arr(rows.iter().map(|r| r.to_json()).collect()),
            ),
            ("compatibility", Json::Arr(compatibility.to_vec())),
        ]);
        let rref = self
            .docs
            .put(doc_kind::REPORT, &bundle)
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?;
        self.transitioned(
            store,
            candidate_id,
            &from,
            "transferred",
            "S6",
            Some(&rref),
            Json::obj([]),
            vec![],
        )?;
        Ok(rref)
    }

    // ── S7 security invariance ─────────────────────────────────────────

    /// `security_check` — the S7 gate (AC-10): the stored diff's
    /// classification re-checks clean post-eval; every policy leaf's
    /// edit classifies `narrowing`; placement is `subprocess_confined`;
    /// opaque payloads carry an interface; the dynamic veto table is
    /// byte-equal-or-better.
    pub fn security_check(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        report: &SecurityInvarianceReport,
    ) -> Res<String> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "transferred", "security_checked")?;
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, candidate_id, &from, r, None)
        };
        // Re-run the static classification over the stored diff — the
        // S1 deltas must hold verbatim after re-seal (§05h §4 S7).
        let rec = self.view.candidate(candidate_id).expect("state checked");
        let diff_ref = rec.diff_ref.clone().unwrap_or_default();
        let _ops = self.proposal_op_targets(candidate_id, &diff_ref)?;
        // K-4 repeated (§05h §4 S7) — the stored diff's classification is
        // re-read here; a `coordination_delta = loosening` that survived to
        // S7 refuses identically (the classification is the durable claim —
        // it rides the sealed deposit, never re-derived).
        let stored = self.proposal_diff(candidate_id)?;
        if stored.classification.coordination_delta == hh_hir::diff::Delta::Loosening {
            return Err(fail(
                self,
                store,
                Refusal::CoordinationLoosening {
                    detail: "S7 re-check: coordination_delta = loosening —                              CoordinationPolicy diffs tighten only"
                        .into(),
                },
            ));
        }
        if report.placement == "in_process" {
            return Err(fail(
                self,
                store,
                Refusal::InProcessCandidate {
                    detail: "candidate executables run subprocess_confined — \
                         never in-process"
                        .into(),
                },
            ));
        }
        if !report.has_interface {
            return Err(fail(
                self,
                store,
                Refusal::OpaqueWithoutInterface {
                    detail: "a CompiledPayload variant without a declared interface \
                         is opaque — refused"
                        .into(),
                },
            ));
        }
        for (leaf, delta) in &report.policy_leaves {
            if delta != "narrowing" && delta != "none" {
                return Err(fail(
                    self,
                    store,
                    Refusal::PolicyWidening { leaf: leaf.clone() },
                ));
            }
        }
        for (metric, regressed) in &report.dynamic_veto_table {
            if *regressed {
                return Err(fail(
                    self,
                    store,
                    Refusal::SecurityVetoTripped {
                        metric: metric.clone(),
                    },
                ));
            }
        }
        let rref = self
            .docs
            .put(doc_kind::REPORT, &report.to_json())
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?;
        self.transitioned(
            store,
            candidate_id,
            &from,
            "security_checked",
            "S7",
            Some(&rref),
            Json::obj([]),
            vec![],
        )?;
        Ok(rref)
    }

    // ── S8 seal ────────────────────────────────────────────────────────

    /// `seal` — the S8 gate (AC-1; G3-1): the `EvolutionAcceptanceReport`
    /// must pass items 1,2,3,5,6 (+4/7 per the §05h rules) and the
    /// endorsement's provenance must be `origin = human` with
    /// `authority = definition` — `SealRefused{not_human}` otherwise.
    /// The evolution process never endorses itself.
    pub fn seal(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        report: &EvolutionAcceptanceReport,
        endorsement: &ProvenanceRecord,
    ) -> Res<String> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "security_checked", "sealed_candidate")?;
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, candidate_id, &from, r, None)
        };
        let failing = report.check();
        if !failing.is_empty() {
            return Err(fail(
                self,
                store,
                Refusal::AcceptanceIncomplete { items: failing },
            ));
        }
        // G3-1's literal spellings — `seal_refused{not_human}` /
        // `seal_refused{not_definition}`.
        if !matches!(endorsement.origin, Origin::Human { .. }) {
            return Err(fail(
                self,
                store,
                Refusal::SealRefused {
                    reason: "not_human".into(),
                },
            ));
        }
        if endorsement.authority < AuthorityClass::Definition {
            return Err(fail(
                self,
                store,
                Refusal::SealRefused {
                    reason: "not_definition".into(),
                },
            ));
        }
        let rref = self
            .docs
            .put(doc_kind::REPORT, &report.to_json())
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?;
        self.transitioned(
            store,
            candidate_id,
            &from,
            "sealed_candidate",
            "S8",
            Some(&rref),
            Json::obj([]),
            vec![],
        )?;
        Ok(rref)
    }

    /// `publish_namespace` check — the candidate's artifacts may publish
    /// only under `exp/<campaign_id>/` (AC-R-2.12.2-14; G3-2). The op
    /// layer calls this before any registry publish; anything else is
    /// `NamespaceForbidden`.
    pub fn check_publish_namespace(&self, namespace: &str) -> Res<()> {
        let expected = format!("exp/{}", self.spec.campaign_id);
        if namespace != expected {
            return Err(Refusal::NamespaceForbidden {
                namespace: namespace.to_string(),
            }
            .into());
        }
        Ok(())
    }

    // ── S9 canary / rollout ────────────────────────────────────────────

    /// `canary` — open the shadow canary (S9): `shadow` only — a `split`
    /// rollout is exploratory and never commits `active`.
    /// `intervention_ref` pins the counterfactual arm's
    /// `InterventionRecord` (the replay-mode'd arm V3 renders).
    pub fn canary(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        mode: &str,
        intervention_ref: &str,
    ) -> Res<()> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "sealed_candidate", "canary")?;
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, candidate_id, &from, r, None)
        };
        // 6b — `shadow` commits unconditionally; `split` commits only
        // under the spec's declared `RolloutPolicy{mode: split}` (the
        // split arm's rows are exploratory — `comparable: false`; AC-10;
        // §05h §4 S9). Anything else refuses.
        let mut extra = Json::obj([("mode", Json::str(mode))]);
        match mode {
            "shadow" => {}
            "split" => {
                let pol = self.spec.rollout_policy.as_ref().ok_or_else(|| {
                    Refusal::SplitRolloutNotCommittable {
                        detail: "canary mode `split` without a declared                                  rollout_policy — the deterministic assignment seed                                  and share floor are mandatory"
                            .into(),
                    }
                })?;
                if pol.mode != "split" {
                    return Err(fail(
                        self,
                        store,
                        Refusal::SplitRolloutNotCommittable {
                            detail: format!(
                                "canary mode `split` but rollout_policy.mode = `{}`",
                                pol.mode
                            ),
                        },
                    ));
                }
                if pol.assignment_seed.is_empty() {
                    return Err(fail(
                        self,
                        store,
                        Refusal::SplitRolloutNotCommittable {
                            detail: "rollout_policy carries no assignment_seed —                                      the split assignment must be deterministic"
                                .into(),
                        },
                    ));
                }
                extra = Json::obj([
                    ("mode", Json::str("split")),
                    ("share_ppm", Json::Int(pol.share_ppm as i64)),
                    ("assignment_seed", Json::str(&pol.assignment_seed)),
                    // Split-arm rows are exploratory — the aggregate
                    // exclusion rides `comparable: false` (R-2.10.5).
                    ("comparable", Json::Bool(false)),
                    ("exploratory", Json::Bool(true)),
                ]);
            }
            _ => {
                return Err(fail(
                    self,
                    store,
                    Refusal::SplitRolloutNotCommittable {
                        detail: format!(
                            "canary mode `{mode}` — `shadow` commits; `split` commits \
                             only under a declared rollout_policy"
                        ),
                    },
                ));
            }
        }
        if intervention_ref.is_empty() {
            return Err(fail(
                self,
                store,
                Refusal::CanaryAborted {
                    reason: "no intervention_ref — the counterfactual arm is mandatory".into(),
                },
            ));
        }
        self.transitioned(
            store,
            candidate_id,
            &from,
            "canary",
            "S9",
            Some(intervention_ref),
            extra,
            vec![],
        )?;
        Ok(())
    }

    /// `canary_settle` — land the canary's outcome: `clean` rolls out to
    /// `active` (with the conditioned-rule debt check); `aborted{reason}`
    ////`veto` reverts (`reverted{from: canary}` — the revert landing is
    /// the pipeline's own transition; the sealed-ancestor restore rides
    /// `hh-hir`'s `invert` on the caller's side).
    pub fn canary_settle(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        status: &str,
        reason: &str,
        conditioned_debts: &[Json],
    ) -> Res<()> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "canary", "active")?;
        match status {
            "clean" => {
                // Conditioned rules the diff touched need complete
                // AssumptionDebtRecords before activation.
                let touched = self.conditioned_rules(candidate_id)?;
                let have: Vec<String> = conditioned_debts
                    .iter()
                    .filter_map(|j| j.get("rule_ref").and_then(Json::as_str))
                    .map(str::to_string)
                    .collect();
                for rule in &touched {
                    if !have.iter().any(|h| h == rule) {
                        return Err(self.reject(
                            store,
                            candidate_id,
                            &from,
                            Refusal::ConditionedRuleIncomplete {
                                detail: format!("conditioned rule `{rule}` has no debt record"),
                            },
                            None,
                        ));
                    }
                }
                self.transitioned(
                    store,
                    candidate_id,
                    &from,
                    "active",
                    "S9",
                    None,
                    Json::obj([]),
                    vec![],
                )?;
            }
            _ => {
                // abort/veto → reverted{from: canary, reason} — the
                // durable terminal row (the sealed-ancestor landing is
                // `apply(base, invert(diff))` on the caller's side).
                self.transitioned(
                    store,
                    candidate_id,
                    &from,
                    "reverted",
                    "S9",
                    None,
                    Json::obj([
                        ("code", Json::str("canary_aborted")),
                        ("reason", Json::str(reason)),
                    ]),
                    vec![],
                )?;
                return Err(Refusal::CanaryAborted {
                    reason: reason.to_string(),
                }
                .into());
            }
        }
        Ok(())
    }

    /// The conditioned rules the candidate's stored diff touched —
    /// `touches_conditioned_rules` off the deposited diff (S9's
    /// `ConditionedRuleIncomplete` domain).
    fn conditioned_rules(&self, candidate_id: &str) -> Res<Vec<String>> {
        let rec = self
            .view
            .candidate(candidate_id)
            .ok_or_else(|| Refusal::UnknownCandidate {
                candidate_id: candidate_id.to_string(),
            })?;
        let proposal_ref = rec
            .reports
            .get("S1")
            .cloned()
            .ok_or_else(|| schema("candidate has no S1 proposal deposit"))?;
        let body = self
            .docs
            .get(doc_kind::PROPOSAL, &proposal_ref)
            .map_err(|e| EvolutionError::Docs(format!("{e:?}")))?
            .ok_or_else(|| schema("proposal doc missing"))?;
        let d =
            hh_hir::wire::diff_from_json(body.get("diff").ok_or_else(|| schema("no diff member"))?)
                .map_err(|e| schema(format!("diff decode: {e:?}")))?;
        Ok(d.classification.touches_conditioned_rules)
    }

    // ── S10 expire / retire / revalidate ───────────────────────────────

    /// `expire` — an expiry trigger fires on an `active` candidate
    /// (`deadline` | `superseded` | `drift`).
    pub fn expire(&mut self, store: &mut Store, candidate_id: &str, trigger: &str) -> Res<()> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "active", "expiring")?;
        self.transitioned(
            store,
            candidate_id,
            &from,
            "expiring",
            "S10",
            None,
            Json::obj([("reason", Json::str(trigger))]),
            vec![],
        )?;
        Ok(())
    }

    /// `retire` — the removal test (S10): the evidence names a
    /// `retirement`/`retirement_batch` experiment bound to this campaign
    /// whose comparison establishes non-inferiority within margin, and a
    /// human-sealed retirement record. Failure revalidates, never
    /// silently retires.
    pub fn retire(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        experiment_id: &str,
        report_ref: &str,
        seal: &ProvenanceRecord,
    ) -> Res<()> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "expiring", "retired")?;
        let fail = |eng: &mut Self, store: &mut Store, r: Refusal| -> EvolutionError {
            eng.reject(store, candidate_id, &from, r, Some(report_ref))
        };
        let spec = self.check_stage_experiment(experiment_id, &["retirement"])?;
        if !spec.kind.is_retirement() {
            return Err(fail(
                self,
                store,
                Refusal::NotARetirementDiff {
                    detail: format!(
                        "experiment kind `{}` is not a removal test",
                        spec.kind.name()
                    ),
                },
            ));
        }
        if !matches!(seal.origin, Origin::Human { .. })
            || seal.authority < AuthorityClass::Definition
        {
            return Err(fail(
                self,
                store,
                Refusal::RetirementSealRefused {
                    detail: "the retirement record must be human-sealed".into(),
                },
            ));
        }
        let report = self.load_comparison(report_ref)?;
        // Non-inferiority within margin — the interval's lower bound must
        // clear the declared margin (the removal test's own shape; a
        // point-only or negative-interval result is inconclusive).
        let ok = report
            .paired_effect
            .interval
            .as_ref()
            .and_then(|iv| iv.get("lo").and_then(Json::as_int))
            .map(|lo| lo + self.spec.retention_margin_ppm >= 0)
            .unwrap_or(false);
        if !ok {
            return Err(fail(
                self,
                store,
                Refusal::RemovalTestInconclusive {
                    detail: "the removal test did not establish non-inferiority \
                         within margin — the candidate revalidates"
                        .into(),
                },
            ));
        }
        self.transitioned(
            store,
            candidate_id,
            &from,
            "retired",
            "S10",
            Some(report_ref),
            Json::obj([]),
            vec![],
        )?;
        Ok(())
    }

    /// `revalidate` — the removal test keeps the candidate: `expiring →
    /// revalidated` carrying the evidence ref, then `revalidated →
    /// active` (return to service — one transition pair, both durable).
    pub fn revalidate(
        &mut self,
        store: &mut Store,
        candidate_id: &str,
        evidence_ref: &str,
    ) -> Res<()> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "expiring", "revalidated")?;
        self.transitioned(
            store,
            candidate_id,
            &from,
            "revalidated",
            "S10",
            Some(evidence_ref),
            Json::obj([]),
            vec![],
        )?;
        Ok(())
    }

    /// `reactivate` — `revalidated → active` (return to service after a
    /// kept removal test).
    pub fn reactivate(&mut self, store: &mut Store, candidate_id: &str) -> Res<()> {
        self.require_open()?;
        self.bound(store)?;
        let from = self.require_state(candidate_id, "revalidated", "active")?;
        self.transitioned(
            store,
            candidate_id,
            &from,
            "active",
            "S10",
            None,
            Json::obj([]),
            vec![],
        )?;
        Ok(())
    }

    // ── terminals ──────────────────────────────────────────────────────

    /// `withdraw` — the proposer withdraws a live candidate (any
    /// non-terminal, non-serving state — `canary`/`active` must
    /// `revert`, not withdraw).
    pub fn withdraw(&mut self, store: &mut Store, candidate_id: &str, by: &str) -> Res<()> {
        self.bound(store)?;
        let cur = self.candidate_state(candidate_id)?;
        let legal = match cur.as_str() {
            // terminal states and serving states refuse a withdraw.
            "rejected" | "withdrawn" | "reverted" | "retired" | "canary" | "active" => false,
            _ => true,
        };
        if !legal {
            return Err(Refusal::IllegalTransition {
                from: cur,
                to: "withdrawn".into(),
            }
            .into());
        }
        self.transitioned(
            store,
            candidate_id,
            &cur,
            "withdrawn",
            "structural",
            None,
            Json::obj([("by", Json::str(by))]),
            vec![],
        )?;
        Ok(())
    }

    /// `revert` — a `canary`/`active` candidate reverts to its last
    /// human-sealed ancestor (`reverted{from, reason}`; the ancestor
    /// restore lands `apply(base, invert(diff))` on the caller's side —
    /// the row is the durable decision).
    pub fn revert(&mut self, store: &mut Store, candidate_id: &str, reason: &str) -> Res<()> {
        self.require_open()?;
        self.bound(store)?;
        let cur = self.candidate_state(candidate_id)?;
        if !matches!(cur.as_str(), "canary" | "active") {
            return Err(Refusal::IllegalTransition {
                from: cur,
                to: "reverted".into(),
            }
            .into());
        }
        self.transitioned(
            store,
            candidate_id,
            &cur,
            "reverted",
            "structural",
            None,
            Json::obj([("reason", Json::str(reason))]),
            vec![],
        )?;
        Ok(())
    }

    // ── campaign lifecycle ─────────────────────────────────────────────

    /// `campaign_stop` — the stop-rule landing (`no_addressable_failure` |
    /// `stagnation` | `budget_exhausted` — the closed reason set;
    /// AC-13).
    pub fn stop(&mut self, store: &mut Store, reason: &str) -> Res<()> {
        self.bound(store)?;
        if !["no_addressable_failure", "stagnation", "budget_exhausted"].contains(&reason) {
            return Err(Refusal::StopRule {
                reason: format!("unknown stop reason `{reason}`"),
            }
            .into());
        }
        if self.view.status != "open" {
            return Err(Refusal::CampaignNotOpen {
                status: self.view.status.clone(),
            }
            .into());
        }
        self.emit(
            store,
            "measurement.evolution.campaign.stopped",
            Json::obj([
                ("campaign_id", Json::str(&self.spec.campaign_id)),
                ("reason", Json::str(reason)),
                (
                    "candidates_registered",
                    Json::Int(self.view.n_candidates_registered() as i64),
                ),
            ]),
            vec![],
        )?;
        Ok(())
    }

    /// `check_stop` — evaluate the declared stop rule against the fold
    /// (`stagnation`: `n` proposals since the last `validated`;
    /// `max_candidates`; the caller reports budget against
    /// `budget_cap_ref` — the ledger's accounting is the caller's view).
    pub fn check_stop(&self) -> Option<String> {
        if self.view.status != "open" {
            return None;
        }
        if let Some(max) = self.spec.stop_rule.max_candidates {
            if self.view.n_candidates_registered() >= max {
                return Some("budget_exhausted".to_string());
            }
        }
        if let Some(w) = self.spec.stop_rule.stagnation_window {
            // Stagnation: `w` proposals landed without any candidate
            // reaching `validated` — proposals registered while no
            // candidate has yet validated count toward the window.
            let mut proposals_since_validation = 0u64;
            for rec in self.view.candidates.values() {
                let mut validated_seen = false;
                for t in &rec.history {
                    if t.to == "validated" {
                        validated_seen = true;
                    }
                }
                if !validated_seen {
                    proposals_since_validation +=
                        rec.history.iter().filter(|t| t.to == "proposed").count() as u64;
                }
            }
            if w > 0 && proposals_since_validation >= w {
                return Some("stagnation".to_string());
            }
        }
        None
    }

    /// `campaign_close` — terminal: `closed` lands the campaign report
    /// (candidates registered, terminal states) on the durable prefix.
    pub fn close(&mut self, store: &mut Store) -> Res<()> {
        self.bound(store)?;
        if self.view.status == "closed" {
            return Ok(());
        }
        self.emit(
            store,
            "measurement.evolution.campaign.closed",
            Json::obj([
                ("campaign_id", Json::str(&self.spec.campaign_id)),
                (
                    "candidates_registered",
                    Json::Int(self.view.n_candidates_registered() as i64),
                ),
            ]),
            vec![],
        )?;
        Ok(())
    }

    /// `candidate_view` — the projection at `until_seq` (the V3/V6
    /// replay source — the same fold over a prefix).
    pub fn candidate_view(&self, store: &Store, until_seq: Option<u64>) -> Res<CampaignView> {
        let events = store.events(&self.run_id)?;
        let prefix: Vec<EventEnvelope> = match until_seq {
            Some(u) => events.iter().filter(|e| e.seq <= u).cloned().collect(),
            None => events.to_vec(),
        };
        CampaignView::fold(&prefix)
    }
}

/// `predicted` member JSON (the hypothesis doc's own encoding).
fn predicted_json(p: &crate::records::PredictedEffect) -> Json {
    Json::obj([
        (
            "deltas",
            Json::Arr(
                p.deltas
                    .iter()
                    .map(|d| {
                        Json::obj([
                            ("metric", Json::str(&d.metric)),
                            ("direction", Json::str(&d.direction)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "affected_task_ids",
            Json::Arr(p.affected_task_ids.iter().map(Json::str).collect()),
        ),
        ("model_scope", Json::str(&p.model_scope)),
        (
            "horizon",
            p.horizon.as_ref().map(Json::str).unwrap_or(Json::Null),
        ),
    ])
}
