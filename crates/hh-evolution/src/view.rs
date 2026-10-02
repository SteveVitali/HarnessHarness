//! `CampaignView` — the `candidate_view` projection over the campaign
//! run's durable prefix (§05h §6's `project(campaign_run, candidate_view,
//! until_seq)`; S6.1a). A pure fold — a rebuild at any `until_seq`
//! re-derives the identical view (RC-8's equality; the V3/V6 renders read
//! this fold).

use hh_ledger::event::EventEnvelope;
use hh_wire::json::Json;
use std::collections::{BTreeMap, BTreeSet};

use crate::errors::EvolutionError;
use crate::state::{CandidateState, Transition};

/// One candidate's folded record — the lineage member (§05h §4's
/// `CandidateRecord`: state, slot, base/diff refs, the bound hypothesis,
/// per-stage report refs, and the full transition history — rejected
/// candidates stay in lineage, G9/AC-R-2.9.5-11).
#[derive(Debug, Clone)]
pub struct CandidateRecord {
    /// The candidate id (`H(base ∥ diff)` under `evolution_candidate`).
    pub candidate_id: String,
    /// The folded current state (last transition whose `from` matched —
    /// out-of-order/duplicate rows land in `history` only).
    pub state: String,
    /// The slot the proposal claimed.
    pub slot: Option<String>,
    /// The base definition ref (proposed row).
    pub base_ref: Option<String>,
    /// The diff's content ref.
    pub diff_ref: Option<String>,
    /// The bound hypothesis ref (hypothesized row).
    pub hypothesis_ref: Option<String>,
    /// The evidence refs the hypothesis cited.
    pub evidence_refs: Vec<String>,
    /// The hypothesis kind (`observational` bounds the candidate to
    /// S3–S4 — §05h §4 S2).
    pub hypothesis_kind: Option<String>,
    /// `stage → report_ref` — the per-stage evidence table.
    pub reports: BTreeMap<String, String>,
    /// The full transition history (every `transitioned` row naming this
    /// candidate — including duplicate-intake and out-of-order attempts).
    pub history: Vec<Transition>,
    /// The attribution label the validated/accepted evidence earned
    /// (`designed_ablation` floor on survivors — §05h §4 S5).
    pub attribution_label: Option<String>,
    /// `diff_ref` of the rolled-back candidate (the revert record).
    pub reverted_to: Option<String>,
}

impl CandidateRecord {
    /// The record's canonical JSON (the `candidate_view` member shape).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("candidate_id".into(), Json::str(&self.candidate_id));
        m.insert("state".into(), Json::str(&self.state));
        for (k, v) in [
            ("slot", &self.slot),
            ("base_ref", &self.base_ref),
            ("diff_ref", &self.diff_ref),
            ("hypothesis_ref", &self.hypothesis_ref),
            ("hypothesis_kind", &self.hypothesis_kind),
            ("attribution_label", &self.attribution_label),
            ("reverted_to", &self.reverted_to),
        ] {
            if let Some(s) = v {
                m.insert(k.into(), Json::str(s));
            }
        }
        if !self.evidence_refs.is_empty() {
            m.insert(
                "evidence_refs".into(),
                Json::Arr(self.evidence_refs.iter().map(Json::str).collect()),
            );
        }
        if !self.reports.is_empty() {
            m.insert(
                "reports".into(),
                Json::Obj(
                    self.reports
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v)))
                        .collect(),
                ),
            );
        }
        m.insert(
            "history".into(),
            Json::Arr(self.history.iter().map(|t| t.to_json()).collect()),
        );
        Json::Obj(m)
    }
}

/// The folded campaign state.
#[derive(Debug, Clone, Default)]
pub struct CampaignView {
    /// The campaign id (the run's coordinate).
    pub campaign_id: String,
    /// The spec's content ref.
    pub spec_ref: Option<String>,
    /// `open` | `stopped` | `closed`.
    pub status: String,
    /// The stop/close reason when set.
    pub reason: Option<String>,
    /// `candidate_id → CandidateRecord`.
    pub candidates: BTreeMap<String, CandidateRecord>,
    /// Events folded.
    pub event_count: usize,
}

impl CampaignView {
    /// `fold(events)` — rebuild the projection over the run's events
    /// (the `candidate_view` projection — `until_seq` callers pass a
    /// prefix).
    pub fn fold(events: &[EventEnvelope]) -> Result<CampaignView, EvolutionError> {
        let mut v = CampaignView {
            status: "open".to_string(),
            ..CampaignView::default()
        };
        for e in events {
            v.fold_one(e);
        }
        Ok(v)
    }

    /// `fold_tail` — the incremental fold from `event_count` (the
    /// driver's post-append refresh).
    pub fn fold_tail(&mut self, events: &[EventEnvelope]) {
        for e in events.iter().skip(self.event_count) {
            self.fold_one(e);
        }
    }

    /// `n_candidates_registered` — the `to: "proposed"` count (G9's
    /// selection denominator: every registered candidate counts —
    /// rejected included).
    pub fn n_candidates_registered(&self) -> u64 {
        self.candidates
            .values()
            .map(|c| c.history.iter().filter(|t| t.to == "proposed").count() as u64)
            .sum()
    }

    /// The candidate's record (`None` = not registered).
    pub fn candidate(&self, id: &str) -> Option<&CandidateRecord> {
        self.candidates.get(id)
    }

    /// The folded `state` for `id` (`"proposed"` once intake lands).
    pub fn state_of(&self, id: &str) -> Option<&str> {
        self.candidates.get(id).map(|c| c.state.as_str())
    }

    /// The terminal-state candidates — for `lineage`/`status` reports.
    pub fn terminal(&self) -> BTreeSet<String> {
        self.candidates
            .iter()
            .filter(|(_, c)| {
                CandidateState::parse(&c.state)
                    .map(|s| matches!(s, "rejected" | "withdrawn" | "reverted" | "retired"))
                    .unwrap_or(false)
            })
            .map(|(k, _)| k.clone())
            .collect()
    }

    fn fold_one(&mut self, e: &EventEnvelope) {
        self.event_count = (e.seq + 1) as usize;
        match e.class.as_str() {
            "measurement.evolution.campaign.opened" => {
                let p = &e.payload;
                self.campaign_id = p
                    .get("campaign_id")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.spec_ref = p.get("spec_ref").and_then(Json::as_str).map(str::to_string);
                self.status = "open".to_string();
            }
            "measurement.evolution.campaign.stopped" => {
                self.status = "stopped".to_string();
                self.reason = e
                    .payload
                    .get("reason")
                    .and_then(Json::as_str)
                    .map(str::to_string);
            }
            "measurement.evolution.campaign.closed" => {
                self.status = "closed".to_string();
            }
            "measurement.evolution.candidate.transitioned" => {
                let cid = match e.payload.get("candidate_id").and_then(Json::as_str) {
                    Some(c) => c.to_string(),
                    None => return,
                };
                let t = Transition::from_event(e);
                let rec = self
                    .candidates
                    .entry(cid.clone())
                    .or_insert_with(|| CandidateRecord {
                        candidate_id: cid.clone(),
                        state: String::new(),
                        slot: None,
                        base_ref: None,
                        diff_ref: None,
                        hypothesis_ref: None,
                        evidence_refs: Vec::new(),
                        hypothesis_kind: None,
                        reports: BTreeMap::new(),
                        history: Vec::new(),
                        attribution_label: None,
                        reverted_to: None,
                    });
                // State advances only when the transition's `from` matches the
                // folded state — a duplicate intake or a refused attempt on an
                // already-advanced candidate lands in `history` only (the
                // durable audit trail without a state clobber).
                let intake = t.to == "proposed" && rec.state.is_empty();
                let matches_current = t.from == rec.state;
                if intake || matches_current {
                    rec.state = t.to.clone();
                }
                match t.to.as_str() {
                    "proposed" if intake => {
                        rec.slot = t.slot.clone();
                        rec.base_ref = e
                            .payload
                            .get("base_ref")
                            .and_then(Json::as_str)
                            .map(str::to_string);
                        rec.diff_ref = t.report_ref.clone().or_else(|| {
                            e.payload
                                .get("diff_ref")
                                .and_then(Json::as_str)
                                .map(str::to_string)
                        });
                    }
                    "hypothesized" if matches_current => {
                        rec.hypothesis_ref = t.hypothesis_ref.clone();
                        rec.evidence_refs = t.evidence_refs.clone();
                        rec.hypothesis_kind = e
                            .payload
                            .get("hypothesis_kind")
                            .and_then(Json::as_str)
                            .map(str::to_string);
                    }
                    _ => {}
                }
                if let Some(r) = &t.report_ref {
                    rec.reports.insert(t.stage.clone(), r.clone());
                }
                if t.to == "validated" && matches_current {
                    rec.attribution_label = Some("designed_ablation".to_string());
                }
                if t.to == "reverted" && matches_current {
                    rec.reverted_to = t.report_ref.clone();
                }
                rec.history.push(t);
            }
            _ => {}
        }
    }
}
