// SYNC_PROBE_AUDIT
//! The audit trail (§5g.6 R-2.8.6, C0/Stage 1; ADR-0066/0067/0068) — the run
//! ledger read through `audit_view`. There is no second audit store: the view
//! is a pure fold over the durable prefix (`derived_from` watermark +
//! `view_hash`, like every `project` kind), and `Store::verify` is the
//! byte-exact chain check.
//!
//! Stage reach (the §5g.6 §9 stage map): the audit-grade class list with the
//! `audit_fields`/`content_refs` partition (Rule C), kernel-only producers and
//! `authority = kernel` provenance (Rule P) — both enforced at `append` and
//! re-checked by `verify` — the `AuditObligation` record set evaluated here,
//! `content_refs` presence accounting over the blob pool (tombstone-aware),
//! and the Stage-2 halves: the signed `security.audit.checkpoint` fold
//! (head/chain/linkage recompute + signature status), cross-run anchor
//! verification, the `AuditSigner`/`AuditKeyResolver` seams (C0
//! `hmac-sha256`), and the independent `Auditor` head-holder. The Stage-3
//! completeness veto and deferred obligations still report honestly.

use std::collections::{BTreeMap, BTreeSet};

use hh_wire::json::Json;
use hh_wire::sha256::hmac_sha256;

use crate::classes::{self, ScopeKind};
use crate::effect::{EffectFold, EffectPhase};
use crate::errors::MissingReason;
use crate::event::{EventEnvelope, EventFrame};
use crate::manifest::RunManifest;
use crate::tree::{self, CheckpointClaim};
use crate::views::{View, ViewKind};

/// `AuditObligation` (§5g.6 §3; ADR-0066 D5) — `{id, class, quantifier,
/// target_class, link_field, terminal_only?}`: MUST-data, versioned with the
/// `AuditPolicy`. One row of the Rule-O set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditObligation {
    /// The obligation id (the Rule-O row's name).
    pub id: &'static str,
    /// The source class — the event that must have the link.
    pub class: &'static str,
    /// How many targets the source needs.
    pub quantifier: ObligationQuantifier,
    /// The class(es) the link must resolve to (`|` separates alternatives).
    pub target_class: &'static str,
    /// The payload member carrying the link.
    pub link_field: &'static str,
    /// Evaluate only once `lifecycle.run.finished` has committed — an in-flight
    /// subject is pending work, not a violation (AC-H6-5's fixture reads at
    /// `finished`).
    pub terminal_only: bool,
}

/// The `quantifier` sum — `at_least_one` (a link exists) / `exactly_one`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObligationQuantifier {
    /// At least one matching target row.
    AtLeastOne,
    /// Exactly one matching target row.
    ExactlyOne,
}

impl ObligationQuantifier {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ObligationQuantifier::AtLeastOne => "at_least_one",
            ObligationQuantifier::ExactlyOne => "exactly_one",
        }
    }
}

const fn obligation(
    id: &'static str,
    class: &'static str,
    quantifier: ObligationQuantifier,
    target_class: &'static str,
    link_field: &'static str,
    terminal_only: bool,
) -> AuditObligation {
    AuditObligation {
        id,
        class,
        quantifier,
        target_class,
        link_field,
        terminal_only,
    }
}

/// The Stage-1 obligation set (I-A3; ADR-0066 D5 + P4 log) — the rows whose
/// classes exist at this stage. `terminal_only` rows evaluate only once the run
/// has committed `lifecycle.run.finished`.
pub const OBLIGATIONS: &[AuditObligation] = &[
    // Every non-`read_only` `action.tool.proposed` has exactly one
    // `security.permission.decided` or an `action.effect.refused` (link:
    // `effect_id` — read from the scope or the payload).
    obligation(
        "tool_mediation",
        "action.tool.proposed",
        ObligationQuantifier::ExactlyOne,
        "security.permission.decided|action.effect.refused",
        "effect_id",
        true,
    ),
    // Every opened `effect_id` closes with one terminal (ADR-0030).
    obligation(
        "effect_terminal",
        "action.effect.intended",
        ObligationQuantifier::ExactlyOne,
        "action.effect.observed|action.effect.probed|action.effect.compensated|action.effect.reverted|action.effect.abandoned|action.effect.refused",
        "effect_id",
        true,
    ),
    // Every `decided{decision_scope ≠ once}` has a `granted{scope}` (link:
    // `permission_id` — one act, linked events, one permission_id; I-A5).
    obligation(
        "grant_scope",
        "security.permission.decided",
        ObligationQuantifier::AtLeastOne,
        "security.permission.granted",
        "permission_id",
        false,
    ),
    // Every `security.credential.used` references an `effect_id` with an
    // `action.effect.*` (CF-130).
    obligation(
        "credential_use",
        "security.credential.used",
        ObligationQuantifier::AtLeastOne,
        "action.effect.intended",
        "effect_id",
        false,
    ),
    // Every `action.effect.abandoned` carries an `escalation_ref` resolving to a
    // `lifecycle.escalation.raised` row (§5a.2 `abandon` contract).
    obligation(
        "abandoned_escalated",
        "action.effect.abandoned",
        ObligationQuantifier::ExactlyOne,
        "lifecycle.escalation.raised",
        "escalation_ref",
        false,
    ),
    // Every `unknown` effect open at `finished` has an escalation naming it.
    obligation(
        "unknown_escalated",
        "action.effect.unknown",
        ObligationQuantifier::AtLeastOne,
        "lifecycle.escalation.raised",
        "effect_id",
        true,
    ),
    // Every `control.budget.exceeded` (the hard-exhaustion row — `limit` is the
    // hard bound) has an escalation naming the budget (or `kind = budget_hard`).
    obligation(
        "budget_hard_escalated",
        "control.budget.exceeded",
        ObligationQuantifier::AtLeastOne,
        "lifecycle.escalation.raised",
        "budget_id",
        false,
    ),
    // Every `security.label.endorsed` carries a basis from the ADR-0035 closed
    // list (the append-time check 5 is the enforcement; the obligation is the
    // audit-side recheck).
    obligation(
        "endorsement_basis",
        "security.label.endorsed",
        ObligationQuantifier::ExactlyOne,
        "security.label.endorsed",
        "basis",
        false,
    ),
    // Every persisted widening grant (`authority_delta = widening`,
    // `scope = persisted`) has a human-origin `lifecycle.definition.changed`.
    obligation(
        "persisted_widening",
        "security.permission.granted",
        ObligationQuantifier::AtLeastOne,
        "lifecycle.definition.changed",
        "rule_ref",
        false,
    ),
];

/// Obligations of the I-A3 set whose link machinery lands at a later stage —
/// declared (the set is MUST-data) and reported under `obligations_deferred`,
/// never silently absent:
/// - `subagent_anchor` (`control.subagent.spawned` ↔ child `lifecycle.run.created`
///   + `result|cancelled` carrying the child head — cross-run, Stage 2+);
/// - `evolution_link` (`transitioned{to: proposed}`/`applied` paired by
///   `hypothesis_ref`/`evidence_refs` — the evolution pipeline);
/// - `producer_resolution` (`component_variant_ref` resolves to a loaded,
///   pin-attested extension or a sealed-definition member — the registry fold);
/// - `fleet_anchor` (activation `lifecycle.run.created.causes[]` ↔ the fleet
///   run's `control.work_item.dispatched` — §05i).
pub const DEFERRED_OBLIGATIONS: &[&str] = &[
    "subagent_anchor",
    "evolution_link",
    "producer_resolution",
    "fleet_anchor",
];

// ── checkpoint signing (R-2.8.6 Stage 2; ADR-0050 §8(a) C0) ──────────────

/// The C0 checkpoint signature construction: keyed SHA-256 over the unsigned
/// claim's canonical bytes. `alg_ref` spells `"hmac-sha256"`; `sig` spells
/// `"hmac-sha256:<hex>"`. Rotation lands at C1 (ADR-0050 §8(b)) — `alg_ref`
/// is the seam, never a second spelling in the meantime.
pub const CHECKPOINT_ALG: &str = "hmac-sha256";

/// The checkpoint kind vocabulary — `{periodic, effect_terminal, final,
/// rotation, on_demand}` (§5g.6 checkpoint contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointKind {
    /// Interval-driven head (policy cadence).
    Periodic,
    /// Emitted at an effect terminal boundary.
    EffectTerminal,
    /// The terminal checkpoint — lands *after* `lifecycle.run.finished`
    /// commits, covering it. Exactly one per signed run.
    Final,
    /// Key rotation boundary.
    Rotation,
    /// Operator/explicit call.
    OnDemand,
}

impl CheckpointKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CheckpointKind::Periodic => "periodic",
            CheckpointKind::EffectTerminal => "effect_terminal",
            CheckpointKind::Final => "final",
            CheckpointKind::Rotation => "rotation",
            CheckpointKind::OnDemand => "on_demand",
        }
    }
}

/// A checkpoint signer — the run's registered kernel key, resolved outside the
/// ledger. Custody never crosses into `hh-ledger`: the embed layer resolves
/// the key through `CredentialBroker::kernel_use` (`KernelPurpose::Ledger
/// Signing`, R-2.8.3) and wraps the bytes in an `AuditSigner`; the ledger sees
/// `key_id` + `sign(preimage)` only, and the row carries `key_id`/`sig` —
/// never key material.
pub trait AuditSigner {
    /// The key id — must be a member of the run manifest's `signer_key_ids`.
    fn key_id(&self) -> &str;
    /// Sign the canonical unsigned-claim bytes → the raw signature bytes.
    fn sign(&mut self, preimage: &[u8]) -> Result<Vec<u8>, String>;
}

/// The auditor-side key lookup — resolves a `signer_key_ids` member to the
/// bytes `verify` checks a `sig` against. `None` means the key is not held:
/// `verify_run` then answers `SignerUnavailable` rather than fabricating ok
/// (CC4 — an unverifiable signature is never "passed").
pub trait AuditKeyResolver {
    /// The key bytes for `key_id`, or `None` when not held.
    fn verify_key(&self, key_id: &str) -> Option<Vec<u8>>;
}

/// A fixed `hmac-sha256` key pair — the resolved key material wrapped for
/// signing (`AuditSigner`) and verification (`AuditKeyResolver`). Used by the
/// embed layer over `kernel_use` output and by tests directly.
#[derive(Debug, Clone)]
pub struct FixedSigner {
    key_id: String,
    key: Vec<u8>,
}

impl FixedSigner {
    /// Wrap resolved key bytes under its declared id.
    pub fn new(key_id: impl Into<String>, key: impl Into<Vec<u8>>) -> Self {
        Self {
            key_id: key_id.into(),
            key: key.into(),
        }
    }
}

impl AuditSigner for FixedSigner {
    fn key_id(&self) -> &str {
        &self.key_id
    }

    fn sign(&mut self, preimage: &[u8]) -> Result<Vec<u8>, String> {
        Ok(hmac_sha256(&self.key, preimage).to_vec())
    }
}

impl AuditKeyResolver for FixedSigner {
    fn verify_key(&self, key_id: &str) -> Option<Vec<u8>> {
        (key_id == self.key_id).then(|| self.key.clone())
    }
}

/// A key table resolver — `key_id → bytes` for multi-key verification
/// (rotation histories, witness sets).
#[derive(Debug, Clone, Default)]
pub struct KeyTable(pub BTreeMap<String, Vec<u8>>);

impl AuditKeyResolver for KeyTable {
    fn verify_key(&self, key_id: &str) -> Option<Vec<u8>> {
        self.0.get(key_id).cloned()
    }
}

/// Render raw signature bytes as the canonical `sig` spelling.
pub fn render_sig(sig: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for b in sig {
        hex.push_str(&format!("{b:02x}"));
    }
    format!("{CHECKPOINT_ALG}:{hex}")
}

/// Parse a `sig` member back to bytes — `None` for a wrong `alg_ref` spelling
/// or malformed hex (the caller maps that to `BadSignature`).
pub fn parse_sig(sig: &str) -> Option<Vec<u8>> {
    let hex = sig.strip_prefix("hmac-sha256:")?;
    if hex.len() != 64 {
        return None;
    }
    let mut out = Vec::with_capacity(32);
    let bytes = hex.as_bytes();
    for i in (0..64).step_by(2) {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

// ── the independent auditor (§5g.6 auditor model) ─────────────────────────

/// A fault the auditor raises — the streaming counterpart of `verify`'s
/// `Tampered{kind}` vocabulary, surfaced as it observes rather than at a
/// whole-run recheck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditFault {
    /// The frame does not continue the auditor's own record — a replayed fork,
    /// a rewritten hash, a wrong `prev_hash` (§5g.6 `fork_equivocation`).
    Equivocation {
        /// The offending seq.
        at_seq: u64,
        /// What disagreed.
        detail: String,
    },
    /// A signed checkpoint's claim disagrees with the auditor's record —
    /// wrong `tree_size`/`tree_head`/`chain_hash`, or a `prev_checkpoint`
    /// link that does not name the claim the auditor holds.
    Inconsistent {
        /// The checkpoint's seq.
        at_seq: u64,
        /// What disagreed.
        detail: String,
    },
    /// A checkpoint signature failed (only raised when a resolver was
    /// supplied — without keys the auditor records, never fabricates).
    BadSignature {
        /// The checkpoint's seq.
        at_seq: u64,
        /// What failed.
        detail: String,
    },
    /// A seq was skipped or duplicated in the stream.
    Gap {
        /// The seq the record expected next.
        expected: u64,
        /// What arrived.
        found: u64,
    },
    /// The frame can't be read as its class.
    Malformed {
        /// The frame's seq.
        at_seq: u64,
        /// Why.
        detail: String,
    },
}

/// The independent auditor (§5g.6: "an auditor subscribes to the durable event
/// stream, verifies each signed head against an independently held copy of the
/// covered range... and holds observed heads — the writer cannot overwrite
/// what the auditor already holds"). Pure: feed durable frames in stream
/// order; the auditor keeps its *own* leaf list and recomputes every signed
/// claim against it. No store access — the independence is the point.
pub struct Auditor {
    run_id: String,
    /// The auditor's own leaf-hash record — its held ground truth.
    leaves: Vec<String>,
    /// The auditor's compact range over `leaves` (its own fold — derived, as
    /// every fold is).
    range: tree::CompactRange,
    /// Held heads — every `(tree_size, tree_head)` the auditor verified,
    /// newest last. The writer can never overwrite these.
    held_heads: Vec<(u64, String)>,
    /// The checkpoint claims observed, in order (event seq, claim).
    claims: Vec<(u64, CheckpointClaim)>,
}

impl Auditor {
    /// Start an audit of `run_id` from genesis.
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            leaves: Vec::new(),
            range: tree::CompactRange::default(),
            held_heads: Vec::new(),
            claims: Vec::new(),
        }
    }

    /// The auditor's own current head `(tree_size, tree_head)` over its
    /// record — what it would compare a signed claim against.
    pub fn head(&self) -> (u64, String) {
        let (n, h) = self.range.head();
        (n as u64, h)
    }

    /// The held heads — `(tree_size, tree_head)` pairs the writer cannot
    /// overwrite.
    pub fn held_heads(&self) -> &[(u64, String)] {
        &self.held_heads
    }

    /// The checkpoint claims observed (seq, claim), in order.
    pub fn claims(&self) -> &[(u64, CheckpointClaim)] {
        &self.claims
    }

    /// Fold one durable frame into the auditor's record. Dense seqs, the
    /// `prev_hash` chain and the event hash recompute run against the
    /// *auditor's* leaves — a replayed fork is `Equivocation`, a hole is
    /// `Gap`. On a `security.audit.checkpoint` row the auditor additionally
    /// recomputes the signed claim against its own head and, when `keys` is
    /// supplied, checks the signature; a verified claim becomes a held head.
    /// Non-durable frames (`Sync`/`Closed`/`Ephemeral`) are ignored — they
    /// carry no hash chain.
    pub fn observe(
        &mut self,
        frame: &EventFrame,
        keys: Option<&dyn AuditKeyResolver>,
    ) -> Result<(), AuditFault> {
        let EventFrame::Durable { seq, hash, event } = frame else {
            return Ok(());
        };
        let seq = *seq;
        let expected = self.leaves.len() as u64;
        if seq != expected {
            return Err(AuditFault::Gap {
                expected,
                found: seq,
            });
        }
        if event.seq != seq {
            return Err(AuditFault::Malformed {
                at_seq: seq,
                detail: "frame seq ≠ envelope seq".into(),
            });
        }
        if event.run_id != self.run_id {
            return Err(AuditFault::Malformed {
                at_seq: seq,
                detail: format!("frame run {} ≠ audited run {}", event.run_id, self.run_id),
            });
        }
        if event.recompute_hash() != event.hash || event.hash != *hash {
            return Err(AuditFault::Equivocation {
                at_seq: seq,
                detail: "event bytes do not recompute to the delivered hash".into(),
            });
        }
        let expect_prev = self
            .leaves
            .last()
            .cloned()
            .unwrap_or_else(|| crate::ids::GENESIS_HASH.to_string());
        // seq 0 anchors on GENESIS or on the lineage link's head hash — both
        // legitimate; the chain check starts at seq 1.
        if seq > 0 && event.prev_hash != expect_prev {
            return Err(AuditFault::Equivocation {
                at_seq: seq,
                detail: "prev_hash does not continue the auditor's record".into(),
            });
        }
        self.leaves.push(event.hash.clone());
        self.range.push(event.hash.clone());
        if event.class == "security.audit.checkpoint" {
            self.check_claim(seq, event, keys)?;
        }
        Ok(())
    }

    /// Recheck a checkpoint claim against the auditor's own record.
    fn check_claim(
        &mut self,
        seq: u64,
        event: &EventEnvelope,
        keys: Option<&dyn AuditKeyResolver>,
    ) -> Result<(), AuditFault> {
        let inconsistent = |detail: String| AuditFault::Inconsistent {
            at_seq: seq,
            detail,
        };
        let claim =
            tree::parse_checkpoint(&event.payload).ok_or_else(|| AuditFault::Malformed {
                at_seq: seq,
                detail: "checkpoint payload does not parse".into(),
            })?;
        if claim.tree_size != Some(seq) {
            return Err(inconsistent(format!(
                "tree_size {:?} ≠ seq {seq}",
                claim.tree_size
            )));
        }
        let my_head = tree::mth_prefix(&self.leaves, seq as usize);
        if claim.tree_head.as_deref() != Some(my_head.as_str()) {
            return Err(AuditFault::Equivocation {
                at_seq: seq,
                detail: "signed tree_head disagrees with the auditor's covered range".into(),
            });
        }
        let expect_chain = self
            .leaves
            .get(seq.saturating_sub(1) as usize)
            .cloned()
            .unwrap_or_else(|| crate::ids::GENESIS_HASH.to_string());
        if claim.chain_hash.as_deref() != Some(expect_chain.as_str()) {
            return Err(inconsistent("chain_hash ≠ the covered tip".into()));
        }
        match self.claims.last() {
            None => {
                if let Some(p) = &claim.prev_checkpoint {
                    if *p != Json::Null {
                        return Err(inconsistent(
                            "first checkpoint carries a prev_checkpoint link".into(),
                        ));
                    }
                }
            }
            Some((_, prev)) => {
                let Some(Json::Obj(link)) = claim.prev_checkpoint.as_ref() else {
                    return Err(inconsistent(
                        "prev_checkpoint missing on a later claim".into(),
                    ));
                };
                let ok = link
                    .get("tree_size")
                    .and_then(Json::as_int)
                    .map(|v| v as u64)
                    == prev.tree_size
                    && link
                        .get("tree_head")
                        .and_then(Json::as_str)
                        .map(str::to_string)
                        .as_deref()
                        == prev.tree_head.as_deref();
                if !ok {
                    return Err(AuditFault::Equivocation {
                        at_seq: seq,
                        detail: "prev_checkpoint does not name the held claim".into(),
                    });
                }
            }
        }
        if let Some(resolver) = keys {
            if claim.signatures.is_empty() {
                return Err(AuditFault::BadSignature {
                    at_seq: seq,
                    detail: "no signatures on the claim".into(),
                });
            }
            let preimage = tree::checkpoint_sig_preimage(&event.payload);
            let mut any = false;
            for s in &claim.signatures {
                let (Some(kid), Some(alg), Some(sig)) = (
                    s.get("key_id").and_then(Json::as_str),
                    s.get("alg_ref").and_then(Json::as_str),
                    s.get("sig").and_then(Json::as_str),
                ) else {
                    return Err(AuditFault::BadSignature {
                        at_seq: seq,
                        detail: "signature entry malformed".into(),
                    });
                };
                if alg != CHECKPOINT_ALG {
                    return Err(AuditFault::BadSignature {
                        at_seq: seq,
                        detail: format!("alg_ref {alg} unsupported"),
                    });
                }
                let Some(sig_bytes) = parse_sig(sig) else {
                    return Err(AuditFault::BadSignature {
                        at_seq: seq,
                        detail: "sig malformed".into(),
                    });
                };
                let Some(key) = resolver.verify_key(kid) else {
                    return Err(AuditFault::BadSignature {
                        at_seq: seq,
                        detail: format!("key_id {kid} unresolvable"),
                    });
                };
                if hmac_sha256(&key, &preimage).to_vec() != sig_bytes {
                    return Err(AuditFault::BadSignature {
                        at_seq: seq,
                        detail: format!("key_id {kid} signature mismatch"),
                    });
                }
                any = true;
            }
            if !any {
                return Err(AuditFault::BadSignature {
                    at_seq: seq,
                    detail: "no signatures on the claim".into(),
                });
            }
        }
        self.held_heads
            .push((seq, claim.tree_head.clone().unwrap_or_default()));
        self.claims.push((seq, claim));
        Ok(())
    }
}

/// How a blob address resolves for `audit_view`'s `content_refs` accounting —
/// the store supplies it (present / tombstoned with the GC or redaction reason
/// / absent with no audit row = `missing`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobStatus {
    /// Bytes exist at the address.
    Present,
    /// No bytes, and no tombstone row accounts for it — the `missing` bucket.
    Missing,
    /// No bytes, but a `lifecycle.ledger.{redacted,gc}` row names it — the
    /// address reports under its reason, never `missing` (AC-R-2.8.6-9).
    Tombstoned(MissingReason),
}

/// One unmet obligation — `{obligation_id, subject}` (the source event's id or
/// the scoped subject it names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmet {
    /// The obligation.
    pub obligation_id: &'static str,
    /// The subject (the source `event_id`, or the linked id when absent).
    pub subject: String,
}

fn str_member<'a>(e: &'a EventEnvelope, k: &str) -> Option<&'a str> {
    e.payload.get(k).and_then(Json::as_str)
}

/// Evaluate [`OBLIGATIONS`] over the durable prefix. Returns `(checked, unmet)`
/// — `checked` lists every obligation id that ran (the deferred set is declared
/// in [`DEFERRED_OBLIGATIONS`], reported separately).
pub fn eval_obligations(
    events: &[&EventEnvelope],
    effects: &BTreeMap<String, EffectFold>,
    finished: bool,
) -> (Vec<&'static str>, Vec<Unmet>) {
    let mut checked = Vec::new();
    let mut unmet = Vec::new();
    for ob in OBLIGATIONS {
        if ob.terminal_only && !finished {
            continue;
        }
        checked.push(ob.id);
        eval_obligation(ob, events, effects, &mut unmet);
    }
    (checked, unmet)
}

fn eval_obligation(
    ob: &AuditObligation,
    events: &[&EventEnvelope],
    effects: &BTreeMap<String, EffectFold>,
    unmet: &mut Vec<Unmet>,
) {
    let targets: Vec<&EventEnvelope> = events
        .iter()
        .copied()
        .filter(|e| ob.target_class.split('|').any(|c| c == e.class))
        .collect();
    match ob.id {
        "tool_mediation" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                if e.payload.get("read_only") == Some(&Json::Bool(true)) {
                    continue;
                }
                let effect = e
                    .scope
                    .effect_id
                    .as_deref()
                    .or_else(|| str_member(e, "effect_id"));
                let count = match effect {
                    Some(eid) => targets
                        .iter()
                        .filter(|t| str_member(t, "effect_id") == Some(eid))
                        .count(),
                    None => 0,
                };
                if count != 1 {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "effect_terminal" => {
            for f in effects.values() {
                if !f.is_terminal() {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: f.effect_id.clone(),
                    });
                }
            }
        }
        "grant_scope" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let scope = str_member(e, "decision_scope").unwrap_or("once");
                if scope == "once" {
                    continue;
                }
                let pid = str_member(e, "permission_id");
                let ok = match pid {
                    Some(pid) => targets
                        .iter()
                        .any(|t| str_member(t, "permission_id") == Some(pid)),
                    None => false,
                };
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "credential_use" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let ok = str_member(e, "effect_id")
                    .map(|eid| effects.contains_key(eid))
                    .unwrap_or(false);
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "abandoned_escalated" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let ok = str_member(e, "escalation_ref")
                    .map(|r| targets.iter().any(|t| t.event_id == r))
                    .unwrap_or(false);
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "unknown_escalated" => {
            for f in effects.values() {
                if f.phase != EffectPhase::Unknown {
                    continue;
                }
                let ok = targets.iter().any(|t| {
                    str_member(t, "effect_id") == Some(f.effect_id.as_str())
                        || str_member(t, "kind") == Some("effect_unknown")
                });
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: f.effect_id.clone(),
                    });
                }
            }
        }
        "budget_hard_escalated" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let ok = targets.iter().any(|t| {
                    str_member(t, "budget_id") == str_member(e, "budget_id")
                        || str_member(t, "kind") == Some("budget_hard")
                });
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "endorsement_basis" => {
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let ok = str_member(e, "basis")
                    .map(|b| {
                        hh_provenance::EndorsementBasis::ALL
                            .iter()
                            .any(|k| k.as_str() == b)
                    })
                    .unwrap_or(false);
                if !ok {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        "persisted_widening" => {
            let human_changed = events.iter().any(|e| {
                e.class == "lifecycle.definition.changed"
                    && e.provenance
                        .as_ref()
                        .map(|p| matches!(p.origin, hh_provenance::Origin::Human { .. }))
                        .unwrap_or(false)
            });
            for e in events.iter().copied().filter(|e| e.class == ob.class) {
                let widening = str_member(e, "authority_delta") == Some("widening");
                let persisted = str_member(e, "scope") == Some("persisted");
                if widening && persisted && !human_changed {
                    unmet.push(Unmet {
                        obligation_id: ob.id,
                        subject: e.event_id.clone(),
                    });
                }
            }
        }
        _ => {}
    }
}

/// `project(audit_view)` — the §5g.6 §2 record over the durable prefix:
/// `events_seen`, `chain_ok` (the pure recompute — `Store::verify` is the
/// byte-exact disk check), `checkpoints` (the signed-head fold — Stage 2),
/// `scopes_unclosed`, `content_refs` presence accounting, `producer_violations`
/// (Rule-P recheck over stored rows), `coverage` (the obligation evaluation),
/// `cross_run` (anchor verification), `redactions`, `sink_deliveries`, and the
/// `completeness` vector. Components that legitimately do not apply render
/// `n/a{reason}` — never `false` by absence, never fabricated `true`.
///
/// `blob_status` is the store's tombstone-aware lookup; `other_head` resolves
/// a cross-run anchor's claimed `{tree_size, tree_head}` against the named
/// run's durable prefix (`None` = the run is not held — `unresolvable`, an
/// honest status, not a failure); `keys`, when supplied, lets signature checks
/// report `verified`/`failed` instead of `unverified`.
#[allow(clippy::too_many_arguments)]
pub fn audit_view(
    run_id: &str,
    events: &[EventEnvelope],
    open_scopes: &BTreeMap<String, ScopeKind>,
    finished: bool,
    blob_status: impl Fn(&str) -> BlobStatus,
    until: Option<u64>,
    manifest: &RunManifest,
    other_head: impl Fn(&str, u64) -> Option<(u64, String)>,
    keys: Option<&dyn AuditKeyResolver>,
) -> View {
    let events: Vec<&EventEnvelope> = events
        .iter()
        .filter(|e| until.map(|u| e.seq <= u).unwrap_or(true))
        .collect();
    let mut watermark = None;

    // chain_ok — recompute the hash chain over the folded prefix (the same rule
    // `verify` runs over the WAL bytes).
    let mut chain_ok = true;
    let mut prev_hash = crate::ids::GENESIS_HASH.to_string();
    for (i, e) in events.iter().enumerate() {
        if e.seq != i as u64 || e.prev_hash != prev_hash || e.recompute_hash() != e.hash {
            chain_ok = false;
        }
        prev_hash = e.hash.clone();
        watermark = Some(e.seq);
    }

    // producer_violations — Rule P rechecked over the stored rows (empty on any
    // run written through `append`; a forged row shows here and fails `verify`).
    let mut producer_violations = Vec::new();
    for e in &events {
        if let Some(spec) = classes::lookup(&e.class) {
            if !spec.audit_grade {
                continue;
            }
            let producer_ok = spec
                .producers
                .iter()
                .any(|p| *p == e.producer.component_class);
            let authority_ok = e
                .provenance
                .as_ref()
                .map(|p| p.authority == hh_provenance::AuthorityClass::Kernel)
                .unwrap_or(false);
            if !producer_ok || !authority_ok {
                producer_violations.push(Json::obj([
                    ("seq", Json::Int(e.seq as i64)),
                    ("event_id", Json::str(&e.event_id)),
                    ("class", Json::str(&e.class)),
                    ("producer", Json::str(e.producer.component_class.as_str())),
                ]));
            }
        }
    }

    // content_refs accounting — declared content-ref members resolved against
    // the blob pool; tombstoned addresses report under their reason
    // (redacted/gc) whether the tombstone row is this run's or another's —
    // cross-run tombstones keep *this* run's accounting complete.
    let mut refs_present: BTreeSet<String> = BTreeSet::new();
    let mut refs_missing: BTreeSet<String> = BTreeSet::new();
    let mut refs_redacted: BTreeSet<String> = BTreeSet::new();
    let mut refs_gc: BTreeSet<String> = BTreeSet::new();
    let mut redaction_rows = Vec::new();
    for e in &events {
        if e.class == "lifecycle.ledger.redacted" {
            redaction_rows.push(Json::str(&e.event_id));
            if let Some(Json::Arr(ts)) = e.payload.get("targets") {
                for t in ts.iter().filter_map(Json::as_str) {
                    refs_redacted.insert(t.to_string());
                }
            }
        }
        if e.class == "lifecycle.ledger.gc" {
            if let Some(Json::Arr(ts)) = e.payload.get("addresses") {
                for t in ts.iter().filter_map(Json::as_str) {
                    refs_gc.insert(t.to_string());
                }
            }
        }
    }
    for e in &events {
        let Some(spec) = classes::lookup(&e.class) else {
            continue;
        };
        if !spec.audit_grade {
            continue;
        }
        let Json::Obj(m) = &e.payload else { continue };
        for (name, v) in m {
            if !spec.content_refs.contains(&name.as_str()) {
                continue;
            }
            let addrs: Vec<&str> = match v {
                Json::Str(s) => vec![s.as_str()],
                Json::Arr(items) => items.iter().filter_map(Json::as_str).collect(),
                _ => vec![],
            };
            for a in addrs {
                match blob_status(a) {
                    BlobStatus::Present => {
                        refs_present.insert(a.to_string());
                    }
                    BlobStatus::Tombstoned(MissingReason::Redacted) => {
                        refs_redacted.insert(a.to_string());
                    }
                    BlobStatus::Tombstoned(_) => {
                        refs_gc.insert(a.to_string());
                    }
                    BlobStatus::Missing => {
                        refs_missing.insert(a.to_string());
                    }
                }
            }
        }
    }
    for a in refs_redacted.iter().chain(refs_gc.iter()) {
        refs_present.remove(a);
        refs_missing.remove(a);
    }

    // coverage — the Rule-O obligation set, evaluated.
    let mut effects = BTreeMap::new();
    for e in &events {
        crate::effect::fold_event(&mut effects, e);
    }
    let (checked, unmet) = eval_obligations(&events, &effects, finished);
    let coverage = Json::obj([
        (
            "obligations_checked",
            Json::Arr(checked.iter().map(|id| Json::str(*id)).collect()),
        ),
        (
            "obligations_deferred",
            Json::Arr(
                DEFERRED_OBLIGATIONS
                    .iter()
                    .map(|id| Json::str(*id))
                    .collect(),
            ),
        ),
        (
            "unmet",
            Json::Arr(
                unmet
                    .iter()
                    .map(|u| {
                        Json::obj([
                            ("obligation_id", Json::str(u.obligation_id)),
                            ("subject", Json::str(&u.subject)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);

    // sink_deliveries — every `measurement.export.delivered` row (what left the
    // run through a declared sink).
    let sink_deliveries: Vec<Json> = events
        .iter()
        .filter(|e| e.class == "measurement.export.delivered")
        .map(|e| {
            Json::obj([
                ("seq", Json::Int(e.seq as i64)),
                ("event_id", Json::str(&e.event_id)),
                (
                    "sink_id",
                    str_member(e, "sink_id")
                        .map(Json::str)
                        .unwrap_or(Json::Null),
                ),
                (
                    "view_kind",
                    str_member(e, "view_kind")
                        .map(Json::str)
                        .unwrap_or(Json::Null),
                ),
            ])
        })
        .collect();

    // checkpoints — the Stage-2 signed-head fold. Every
    // `security.audit.checkpoint` row is re-read as a claim and recomputed
    // against the covered prefix: `tree_size == seq`, `tree_head` is the
    // covered range's own MTH, `chain_hash` is the covered tip, the
    // `prev_checkpoint` chain links, and `idp` re-derives over the unsigned
    // claim. Signatures are shape-checked always and value-checked when the
    // caller supplies a key resolver (`verified`/`unverified`/`failed` — an
    // unverifiable signature is never "ok").
    let leaf_hashes: Vec<String> = events.iter().map(|e| e.hash.clone()).collect();
    let has_signers = !manifest.signer_key_ids.is_empty();
    let mut checkpoints = Vec::new();
    let mut checkpoints_ok = true;
    let mut prev_claim: Option<(u64, CheckpointClaim)> = None;
    let mut final_seen = false;
    for e in &events {
        if e.class != "security.audit.checkpoint" {
            continue;
        }
        let mut status: &'static str = "verified";
        let Some(claim) = tree::parse_checkpoint(&e.payload) else {
            checkpoints.push(Json::obj([
                ("seq", Json::Int(e.seq as i64)),
                ("verification_status", Json::str("failed")),
            ]));
            checkpoints_ok = false;
            continue;
        };
        // Structural recomputation — the claim vs the covered prefix.
        let size_ok = claim.tree_size == Some(e.seq);
        let head_ok = size_ok
            && claim.tree_head.as_deref()
                == Some(tree::mth_prefix(&leaf_hashes, e.seq as usize).as_str());
        let chain_hash_ok = size_ok
            && e.seq > 0
            && claim.chain_hash.as_deref() == Some(leaf_hashes[(e.seq - 1) as usize].as_str());
        let idp_ok = claim.idp.as_deref() == Some(tree::checkpoint_idp(&e.payload).as_str());
        let link_ok = match &prev_claim {
            None => claim
                .prev_checkpoint
                .as_ref()
                .map(|p| p == &Json::Null)
                .unwrap_or(true),
            Some((_, prev)) => match claim.prev_checkpoint.as_ref() {
                Some(Json::Obj(link)) => {
                    link.get("tree_size")
                        .and_then(Json::as_int)
                        .map(|v| v as u64)
                        == prev.tree_size
                        && link.get("tree_head").and_then(Json::as_str) == prev.tree_head.as_deref()
                }
                _ => false,
            },
        };
        if !(size_ok && head_ok && chain_hash_ok && idp_ok && link_ok) {
            status = "failed";
        }
        // Signatures — shape always; value when a resolver is held.
        if status == "verified" {
            if claim.signatures.is_empty() {
                status = "failed";
            } else if let Some(resolver) = keys {
                let preimage = tree::checkpoint_sig_preimage(&e.payload);
                for s in &claim.signatures {
                    let ok = (|| -> Option<bool> {
                        let kid = s.get("key_id")?.as_str()?;
                        if s.get("alg_ref")?.as_str()? != CHECKPOINT_ALG {
                            return None;
                        }
                        if !manifest.signer_key_ids.iter().any(|k| k == kid) {
                            return None;
                        }
                        let sig = parse_sig(s.get("sig")?.as_str()?)?;
                        let key = resolver.verify_key(kid)?;
                        Some(hmac_sha256(&key, &preimage).to_vec() == sig)
                    })()
                    .unwrap_or(false);
                    if !ok {
                        status = "failed";
                        break;
                    }
                }
            } else {
                // Shape-check only — key ids must be registered, alg must be
                // the C0 construction; the HMAC itself reports unverified.
                let shape_ok = claim.signatures.iter().all(|s| {
                    s.get("key_id")
                        .and_then(Json::as_str)
                        .map(|k| manifest.signer_key_ids.iter().any(|r| r == k))
                        .unwrap_or(false)
                        && s.get("alg_ref").and_then(Json::as_str) == Some(CHECKPOINT_ALG)
                        && s.get("sig")
                            .and_then(Json::as_str)
                            .and_then(parse_sig)
                            .is_some()
                });
                status = if shape_ok { "unverified" } else { "failed" };
            }
        }
        if claim.kind == "final" {
            final_seen = true;
        }
        if status == "failed" {
            checkpoints_ok = false;
        }
        checkpoints.push(Json::obj([
            ("seq", Json::Int(e.seq as i64)),
            ("kind", Json::str(&claim.kind)),
            (
                "tree_size",
                claim
                    .tree_size
                    .map(|v| Json::Int(v as i64))
                    .unwrap_or(Json::Null),
            ),
            (
                "tree_head",
                claim
                    .tree_head
                    .as_deref()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            ("signatures", Json::Arr(claim.signatures.clone())),
            ("verification_status", Json::str(status)),
        ]));
        prev_claim = Some((e.seq, claim));
    }
    // A finished run that declared signers owes a final checkpoint — its
    // absence is a failed component, not an unnoticed gap (the `truncate`
    // verdict belongs to `verify_run`; the view surfaces the same truth).
    let na = |reason: &str| Json::obj([("n/a", Json::str(reason))]);
    let checkpoints_component = if !has_signers && checkpoints.is_empty() {
        na("no signer_key_ids declared")
    } else if !checkpoints_ok || (finished && has_signers && !final_seen) {
        Json::Bool(false)
    } else {
        Json::Bool(checkpoints_ok)
    };

    // cross_run — every anchor the checkpoints sign plus the manifest's own
    // lineage links, each resolved through `other_head` (the named run's
    // durable prefix recomputes the claimed head). `unresolvable` is an
    // honest status for a run this store does not hold — never silently ok.
    let mut cross_run = Vec::new();
    let mut cross_run_ok: Option<bool> = None; // None → n/a (no anchors exist)
                                               // verified ⇒ the claimed head recomputes over the named run; failed ⇒ a
                                               // contradiction; unresolvable ⇒ the run is not held here (honest, not ok).
                                               // cross_run_ok: any `failed` ⇒ false; else any `verified` ⇒ true; else
                                               // stays None (all unresolvable / none at all ⇒ n/a).
    let push_anchor = |other_run: &str,
                       size: u64,
                       head: &str,
                       relation: &str,
                       origin: &str,
                       cross_run: &mut Vec<Json>,
                       ok: &mut Option<bool>| {
        let status = match other_head(other_run, size) {
            Some((_, actual)) if actual == head => "verified",
            Some(_) => "failed",
            None => "unresolvable",
        };
        match status {
            "failed" => *ok = Some(false),
            "verified" => {
                if *ok != Some(false) {
                    *ok = Some(true);
                }
            }
            _ => {}
        }
        cross_run.push(Json::obj([
            ("other_run", Json::str(other_run)),
            ("tree_size", Json::Int(size as i64)),
            ("tree_head", Json::str(head)),
            ("relation", Json::str(relation)),
            ("origin", Json::str(origin)),
            ("status", Json::str(status)),
        ]));
    };
    // Manifest lineage links — anchors known from seq 0.
    for (link, relation) in [
        (manifest.forked_from.as_ref(), "forked_from"),
        (manifest.continued_from.as_ref(), "continued_from"),
    ]
    .into_iter()
    {
        let Some(link) = link else { continue };
        push_anchor(
            &link.run_id,
            link.at_seq + 1,
            &link.head_hash,
            relation,
            "manifest",
            &mut cross_run,
            &mut cross_run_ok,
        );
    }
    if let Some(parent) = &manifest.parent_run_id {
        // The parent anchor claims only that the named run is held — its head
        // at size 0 resolves iff the run is.
        let status = if other_head(parent, 0).is_some() {
            "verified"
        } else {
            "unresolvable"
        };
        if status == "verified" && cross_run_ok.is_none() {
            cross_run_ok = Some(true);
        }
        cross_run.push(Json::obj([
            ("other_run", Json::str(parent)),
            ("relation", Json::str("parent")),
            ("origin", Json::str("manifest")),
            ("status", Json::str(status)),
        ]));
    }
    // Checkpoint-signed anchors — the claims as they were emitted.
    for e in &events {
        if e.class != "security.audit.checkpoint" {
            continue;
        }
        let Some(claim) = tree::parse_checkpoint(&e.payload) else {
            continue;
        };
        for a in &claim.cross_run_anchors {
            let (Some(other), Some(oh)) = (
                a.get("other_run").and_then(Json::as_str),
                a.get("other_head"),
            ) else {
                continue;
            };
            let (Some(size), Some(head)) = (
                oh.get("tree_size").and_then(Json::as_int).map(|v| v as u64),
                oh.get("tree_head").and_then(Json::as_str),
            ) else {
                continue;
            };
            let relation = a
                .get("relation")
                .and_then(Json::as_str)
                .unwrap_or("unspecified");
            push_anchor(
                other,
                size,
                head,
                relation,
                "checkpoint",
                &mut cross_run,
                &mut cross_run_ok,
            );
        }
    }

    let cross_run_component = match cross_run_ok {
        Some(v) => Json::Bool(v),
        None => na("no cross-run anchors"),
    };

    // extensions — the AC-H5-10 provenance fold (§5g.5 §6 audit obligation +
    // §7 AC-R-2.8.5-10): for every effect in the run, the extensions whose
    // text was in the proposing call's context and whose code produced the
    // effect. The fold reads only what the durable record carries —
    // `context.assembled{model_call_id}.context_label.taint[]` supplies the
    // `extension:<id>` tags the proposing call saw; the producer side joins
    // the effect's `capability`/`capability_ref` through the run's own
    // `security.extension.*{extension_id, contributes[]}` declarations and
    // `lifecycle.capability.registered{version_id|semantic_id, produced_by}`
    // rows (`Origin.tool` on the row's provenance names the capability — the
    // extension link is the ledger-visible `contributes`/`produced_by`
    // evidence, never a registry lookup — CC5).
    let mut extension_ids: BTreeSet<String> = BTreeSet::new();
    // capability ref → the extension that declared contributing it.
    let mut capability_owner: BTreeMap<String, String> = BTreeMap::new();
    // model_call_id → the `extension:<id>` taint tags its assembled context
    // carried.
    let mut call_extension_taint: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in &events {
        if e.class.starts_with("security.extension.") {
            if let Some(id) = str_member(e, "extension_id") {
                extension_ids.insert(id.to_string());
                if let Some(Json::Arr(cs)) = e.payload.get("contributes") {
                    for c in cs.iter().filter_map(Json::as_str) {
                        capability_owner.insert(c.to_string(), id.to_string());
                    }
                }
            }
        } else if e.class == "lifecycle.capability.registered" {
            if let Some(producer) = str_member(e, "produced_by") {
                let ext_id = producer.strip_prefix("extension:").unwrap_or(producer);
                for cap in [str_member(e, "version_id"), str_member(e, "semantic_id")]
                    .into_iter()
                    .flatten()
                {
                    capability_owner
                        .entry(cap.to_string())
                        .or_insert_with(|| ext_id.to_string());
                }
            }
        } else if e.class == "context.assembled" {
            if let Some(mc) = str_member(e, "model_call_id") {
                let set = call_extension_taint.entry(mc.to_string()).or_default();
                if let Some(Json::Arr(tags)) =
                    e.payload.get("context_label").and_then(|l| l.get("taint"))
                {
                    for t in tags.iter().filter_map(Json::as_str) {
                        if let Some(id) = t.strip_prefix("extension:") {
                            set.insert(id.to_string());
                        }
                    }
                }
            }
        }
    }
    let extension_effects: Vec<Json> = events
        .iter()
        .filter(|e| e.class == "action.effect.intended")
        .map(|e| {
            let effect_id = str_member(e, "effect_id")
                .map(str::to_string)
                .or_else(|| e.scope.effect_id.clone())
                .unwrap_or_default();
            let capability = str_member(e, "capability")
                .or_else(|| str_member(e, "capability_ref"))
                .map(str::to_string);
            let context_exts: Vec<String> = e
                .scope
                .model_call_id
                .as_ref()
                .and_then(|mc| call_extension_taint.get(mc))
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            let producer_exts: Vec<String> = capability
                .as_ref()
                .map(|c| {
                    let mut v = Vec::new();
                    if let Some(owner) = capability_owner.get(c) {
                        v.push(owner.clone());
                    }
                    if extension_ids.contains(c) && !v.contains(c) {
                        v.push(c.clone());
                    }
                    v
                })
                .unwrap_or_default();
            Json::obj([
                ("effect_id", Json::str(&effect_id)),
                (
                    "model_call_id",
                    e.scope
                        .model_call_id
                        .as_ref()
                        .map(|c| Json::str(c.clone()))
                        .unwrap_or(Json::Null),
                ),
                (
                    "capability_ref",
                    capability.map(Json::str).unwrap_or(Json::Null),
                ),
                (
                    "context_extensions",
                    Json::Arr(context_exts.iter().map(Json::str).collect()),
                ),
                (
                    "producer_extensions",
                    Json::Arr(producer_exts.iter().map(Json::str).collect()),
                ),
            ])
        })
        .collect();

    let completeness = Json::obj([
        ("chain_ok", Json::Bool(chain_ok)),
        ("checkpoints_ok", checkpoints_component.clone()),
        ("scopes_closed", Json::Bool(open_scopes.is_empty())),
        ("blobs_accounted", Json::Bool(refs_missing.is_empty())),
        ("producers_ok", Json::Bool(producer_violations.is_empty())),
        ("coverage_ok", Json::Bool(unmet.is_empty())),
        ("cross_run_ok", cross_run_component.clone()),
        (
            "headline",
            Json::Bool(
                chain_ok
                    && open_scopes.is_empty()
                    && refs_missing.is_empty()
                    && producer_violations.is_empty()
                    && unmet.is_empty()
                    && checkpoints_component != Json::Bool(false)
                    && cross_run_component != Json::Bool(false),
            ),
        ),
    ]);

    let payload = Json::obj([
        ("kind", Json::str("audit_view")),
        ("events_seen", Json::Int(events.len() as i64)),
        ("chain_ok", Json::Bool(chain_ok)),
        ("checkpoints", Json::Arr(checkpoints)),
        (
            "scopes_unclosed",
            Json::Arr(open_scopes.keys().map(Json::str).collect()),
        ),
        (
            "content_refs",
            Json::obj([
                (
                    "present",
                    Json::Arr(refs_present.iter().map(Json::str).collect()),
                ),
                (
                    "redacted",
                    Json::Arr(refs_redacted.iter().map(Json::str).collect()),
                ),
                ("gc", Json::Arr(refs_gc.iter().map(Json::str).collect())),
                (
                    "missing",
                    Json::Arr(refs_missing.iter().map(Json::str).collect()),
                ),
            ]),
        ),
        ("producer_violations", Json::Arr(producer_violations)),
        ("coverage", coverage),
        ("cross_run", Json::Arr(cross_run)),
        ("redactions", Json::Arr(redaction_rows)),
        ("sink_deliveries", Json::Arr(sink_deliveries)),
        // AC-H5-10 — the extension provenance component: the run's declared
        // extension ids plus the per-effect context/producer answer.
        (
            "extensions",
            Json::obj([
                (
                    "declared",
                    Json::Arr(extension_ids.iter().map(Json::str).collect()),
                ),
                ("effects", Json::Arr(extension_effects)),
            ]),
        ),
        ("completeness", completeness),
    ]);
    View::stamped(run_id, ViewKind::AuditView, watermark, payload)
}
