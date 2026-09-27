//! The branch-model projection the S2.9 fork/rollback surface consumes
//! (§5a.4 `coherent_fork_points`; AC-R-2.2.4-1 — a shared S2.3/S2.9
//! prerequisite).
//!
//! A seq `s` is a **coherent fork point** iff every `model_call` scope opened
//! at seq ≤ `s` is closed at seq ≤ `s`, and every effect `intended` ≤ `s` is —
//! in the fold *at `s`* — terminal, `deferred`, or a detached child (listed in
//! a committed effect's `detached_effect_ids[]`). Turn boundaries and effect
//! terminals are always coherent. The projection is pure — same ledger, same
//! answer — and [`Store::check_fork_point`] is the refusal `fork`/`rollback`
//! raise (`ForkPointNotCoherent{at_seq, open_scopes[]}`).

use std::collections::{BTreeMap, BTreeSet};

use hh_wire::json::Json;

use crate::classes::{self, ScopeKind};
use crate::effect::{self, EffectFold, EffectPhase};
use crate::errors::LedgerError;
use crate::event::EventEnvelope;
use crate::store::Store;

/// The coherence fold at a cut — `open model_call scopes` + `non-coherent
/// effects` (intended/authorized/prepared/committed/unknown and not
/// detached-listed).
#[derive(Debug, Clone, Default)]
struct CoherenceFold {
    /// Open `model_call` scope ids.
    open_model_calls: BTreeSet<String>,
    /// Open `turn` scopes (a turn is coherent to fork inside — it just carries
    /// context; the spec's gate names model_calls and effects — but an open
    /// turn *containing* open work is flagged through those).
    effects: BTreeMap<String, EffectFold>,
    /// Effect ids a committed effect marked detached (coherent to fork past —
    /// the detached child reconciles independently).
    detached: BTreeSet<String>,
}

impl CoherenceFold {
    fn step(&mut self, e: &EventEnvelope) {
        if let Some(spec) = classes::lookup(&e.class) {
            if spec.opens_scope == Some(ScopeKind::ModelCall) {
                if let Some(id) = e.scope.model_call_id.as_deref() {
                    self.open_model_calls.insert(id.to_string());
                }
            }
            if spec.closes_scope == Some(ScopeKind::ModelCall) {
                if let Some(id) = e.scope.model_call_id.as_deref() {
                    self.open_model_calls.remove(id);
                }
            }
        }
        // A committed effect's `detached_effect_ids[]` marks its children
        // coherent-past (the detached child runs on its own reconciliation).
        if e.class == "action.effect.committed" {
            if let Some(Json::Arr(ids)) = e.payload.get("detached_effect_ids") {
                for id in ids.iter().filter_map(Json::as_str) {
                    self.detached.insert(id.to_string());
                }
            }
        }
        effect::fold_event(&mut self.effects, e);
    }

    /// The scope spellings blocking coherence at this cut.
    fn open_scopes(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .open_model_calls
            .iter()
            .map(|m| format!("model_call:{m}"))
            .collect();
        for (id, f) in &self.effects {
            let coherent = match f.phase {
                // `deferred` is coherent (a speculative-branch hold — the
                // promotion decision is the fork's); terminals are coherent
                // (`observed{partial}` / retryable `not_applied` are not
                // terminal — they gate the cut).
                EffectPhase::Deferred => true,
                EffectPhase::Intended
                | EffectPhase::Authorized
                | EffectPhase::Prepared
                | EffectPhase::Committed
                | EffectPhase::Unknown => self.detached.contains(id),
                _ => f.is_terminal(),
            };
            if !coherent {
                out.push(format!("effect:{id}"));
            }
        }
        out
    }
}

impl Store {
    /// `coherent_fork_points(run, from_seq?, to_seq?) → [seq]` — the pure
    /// projection (AC-R-2.2.4-1). `O(n)` — the fold steps once and a snapshot of
    /// its open-set answers each cut.
    pub fn coherent_fork_points(
        &self,
        run_id: &str,
        from_seq: Option<u64>,
        to_seq: Option<u64>,
    ) -> Result<Vec<u64>, LedgerError> {
        let events = self.events(run_id)?;
        let mut fold = CoherenceFold::default();
        let mut out = Vec::new();
        // Seq 0 (`lifecycle.run.created`) is always coherent — the lineage
        // anchor a `continued_from` binds.
        let lo = from_seq.unwrap_or(0);
        let hi = to_seq.unwrap_or(u64::MAX);
        if lo == 0 {
            out.push(0);
        }
        for e in events {
            fold.step(e);
            if e.seq < lo || e.seq > hi {
                continue;
            }
            if fold.open_scopes().is_empty() {
                out.push(e.seq);
            }
        }
        Ok(out)
    }

    /// `check_fork_point(run, seq)` — the refusal `fork`/`rollback` raise:
    /// `ForkPointNotCoherent{at_seq, open_scopes[]}` names every scope
    /// blocking the cut.
    pub fn check_fork_point(&self, run_id: &str, at_seq: u64) -> Result<(), LedgerError> {
        let events = self.events(run_id)?;
        if at_seq > events.len().saturating_sub(1) as u64 {
            return Err(LedgerError::SourceIncomplete {
                run_id: run_id.to_string(),
                at_seq,
                head: events.len().saturating_sub(1) as u64,
            });
        }
        let mut fold = CoherenceFold::default();
        for e in events {
            if e.seq > at_seq {
                break;
            }
            fold.step(e);
        }
        let open = fold.open_scopes();
        if open.is_empty() {
            Ok(())
        } else {
            Err(LedgerError::ForkPointNotCoherent {
                run_id: run_id.to_string(),
                at_seq,
                open_scopes: open,
            })
        }
    }

    /// `nearest_coherent_seq(run, seq)` — the suggestion a refusal carries
    /// (the greatest coherent seq ≤ `at_seq`, or `None` when only 0 is).
    pub fn nearest_coherent_seq(
        &self,
        run_id: &str,
        at_seq: u64,
    ) -> Result<Option<u64>, LedgerError> {
        Ok(self
            .coherent_fork_points(run_id, None, Some(at_seq))?
            .into_iter()
            .filter(|s| *s <= at_seq)
            .max())
    }
}
