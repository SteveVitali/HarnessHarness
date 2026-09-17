//! `HandleTable` — the kernel's authority source (§5g.1 §2.1 `project`; ADR-0051
//! D1). The table is a **derived view** over `security.permission.granted` /
//! `revoked` events: `project(events, until_seq)` rebuilds it and rebuild
//! equality with the live table is the acceptance invariant (AC-R-2.8.1-14's
//! Stage-1 mechanism — the Stage-3 suite runs it end-to-end).
//!
//! The table is the **only** place authority is resolved from (I-H1/CC2): a
//! `HandleId` spelled in a `Text` leaf, a tool result or a forged `decided`
//! payload is not a row here and confers nothing.

use std::collections::BTreeMap;

use hh_hir::records::Grant;
use hh_hir::EffectDomain;
use hh_ledger::event::EventEnvelope;
use hh_wire::json::Json;

use crate::events;
use crate::handle::{AuthorityHandle, HandleId};

/// A grant's recorded consumption — the `count` constraint's usage meter and
/// the `decided` audit link (§5g.1 §2.1: per-(handle,grant) decision counts).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantUse {
    /// `decided{allow}` rows that relied on the handle, by effect attempt.
    pub decisions: u64,
}

/// The kernel handle table — `handle_id → AuthorityHandle`, plus per-handle
/// usage meters folded from `decided` rows (the `count` constraint's state).
#[derive(Debug, Clone, Default)]
pub struct HandleTable {
    /// The live and revoked rows (revoked rows stay — `HandleRevoked` is a
    /// typed refusal, not an absence).
    pub handles: BTreeMap<HandleId, AuthorityHandle>,
    /// `handle_id → decision count` — folded from `security.permission.decided`
    /// rows that name the handle in `handle_ids[]`.
    pub uses: BTreeMap<HandleId, GrantUse>,
}

impl HandleTable {
    /// `project(run, until_seq)` — rebuild the table from the granted/revoked/
    /// decided envelopes at `seq ≤ until_seq` (ADR-0026 derived view).
    pub fn project(events: &[EventEnvelope], until_seq: u64) -> HandleTable {
        let mut t = HandleTable::default();
        for env in events.iter().filter(|e| e.seq <= until_seq) {
            t.fold(env);
        }
        t
    }

    /// Fold one durable envelope — `granted` inserts, `revoked` stamps
    /// `revoked_by` on the target and every `cascade[]` descendant (one
    /// transaction — I-H4/H-6), `decided` ticks the usage meter for each
    /// `handle_ids[]` member.
    pub fn fold(&mut self, env: &EventEnvelope) {
        match env.class.as_str() {
            "security.permission.granted" => {
                if let Some(h) = events::handle_from_granted(env) {
                    self.handles.insert(h.handle_id.clone(), h);
                }
            }
            "security.permission.revoked" => {
                if let Some(id) = env
                    .payload
                    .get("handle_id")
                    .and_then(Json::as_str)
                    .and_then(HandleId::parse)
                {
                    self.revoke_row(&id, &env.event_id);
                }
                if let Some(Json::Arr(cascade)) = env.payload.get("cascade") {
                    for c in cascade {
                        if let Some(id) = c.as_str().and_then(HandleId::parse) {
                            self.revoke_row(&id, &env.event_id);
                        }
                    }
                }
            }
            "security.permission.decided" => {
                // The usage meter only counts final `allow` verdicts — a deny
                // never consumed the grant.
                if env.payload.get("decision").and_then(Json::as_str) != Some("allow") {
                    return;
                }
                if let Some(Json::Arr(ids)) = env.payload.get("handle_ids") {
                    for id in ids.iter().filter_map(|j| j.as_str()) {
                        if let Some(h) = HandleId::parse(id) {
                            self.uses.entry(h).or_default().decisions += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn revoke_row(&mut self, id: &HandleId, event_id: &str) {
        if let Some(h) = self.handles.get_mut(id) {
            h.validity.revoked_by = Some(event_id.to_string());
        }
    }

    /// The live row, if any, at the given coordinates.
    pub fn live(
        &self,
        id: &HandleId,
        effect_id: &str,
        turn_id: &str,
        run_id: &str,
        session: &str,
    ) -> Option<&AuthorityHandle> {
        self.handles
            .get(id)
            .filter(|h| h.is_live(effect_id, turn_id, run_id, session))
    }

    /// A handle row regardless of liveness (for `HandleRevoked` diagnostics).
    pub fn get(&self, id: &HandleId) -> Option<&AuthorityHandle> {
        self.handles.get(id)
    }

    /// The live handles of `holder` granting `domain` — scope matching is the
    /// caller's (the `covers` step of `resolve_handles`; §5g.1 H-5: scope is
    /// matched over canonical parameters, never surface fields).
    pub fn covering_domain<'a>(
        &'a self,
        holder: &str,
        domain: EffectDomain,
        effect_id: &str,
        turn_id: &str,
        run_id: &str,
        session: &str,
    ) -> Vec<(&'a HandleId, &'a AuthorityHandle, &'a Grant)> {
        let mut out = Vec::new();
        for (id, h) in &self.handles {
            if h.holder.semantic_id != holder {
                continue;
            }
            if !h.is_live(effect_id, turn_id, run_id, session) {
                continue;
            }
            for g in &h.grants {
                if g.effect.domain == domain {
                    out.push((id, h, g));
                }
            }
        }
        out
    }

    /// Descendants of `id` — the revocation cascade's membership (I-H4:
    /// `parent_handle` chains; the fold walks the whole tree).
    pub fn descendants(&self, id: &HandleId) -> Vec<HandleId> {
        let mut out = Vec::new();
        let mut frontier = vec![id.clone()];
        while let Some(cur) = frontier.pop() {
            for (cid, h) in &self.handles {
                if h.parent_handle.as_ref() == Some(&cur) {
                    out.push(cid.clone());
                    frontier.push(cid.clone());
                }
            }
        }
        out
    }
}
