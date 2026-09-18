//! `ControlState` (§5e.1 data model; ADR-0103 D2): the closed per-variant
//! state with the **mandatory header** `{variant_ref, cursor, decision_count,
//! open_effects[], last_cue_seq, boundary_view}` — canonical-serialisable,
//! never containing secrets, model I/O bytes, environment state or
//! `HandleId`s (references only; ADR-0051 H-1).
//!
//! The checkpoint is the canonical form of the state (ADR-0029 class) —
//! `checkpoint(state) → bytes` is `to_json().to_canonical_string()`;
//! `restore(bytes, ctx)` parses it back and validates `variant_ref` and the
//! dialect stamp (`RestoreError{VariantMismatch, DialectMismatch}`).

use std::collections::BTreeMap;

use hh_ontology::control::{DecisionPoint, Owner};
use hh_wire::json::Json;

/// The checkpoint dialect tag — a `restore` against a different tag is
/// `DialectMismatch`.
pub const CONTROL_STATE_DIALECT: &str = "ControlState/1";

/// `PlanCursor{node_id, iteration, bound_ref}` — position over the
/// `RuntimePlan/1` control nodes (§5e.1 `ControlState` header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCursor {
    /// The current control node (the implicit `Loop` for react/minimal).
    pub node_id: String,
    /// The iteration counter within the node.
    pub iteration: u64,
    /// The budget ref bounding the loop (`cursor.bound` — `decide` must
    /// return `stop` when it is exhausted).
    pub bound_ref: String,
}

impl PlanCursor {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("node_id", Json::str(&self.node_id)),
            ("iteration", Json::Int(self.iteration as i64)),
            ("bound_ref", Json::str(&self.bound_ref)),
        ])
    }

    /// Parse the canonical form.
    pub fn from_json(j: &Json) -> Option<PlanCursor> {
        Some(PlanCursor {
            node_id: j.get("node_id")?.as_str()?.into(),
            iteration: j.get("iteration")?.as_int()? as u64,
            bound_ref: j.get("bound_ref")?.as_str()?.into(),
        })
    }
}

/// `ControlState` — the mandatory header plus a per-variant extension
/// payload (a closed sum per variant; `react/minimal` extends it with
/// `format_error_streak`). `boundary_view` is the *effective* owner map the
/// strategy observes (`DecisionPoint → Owner`) — what I8's
/// `boundary_observed` is diffed against.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlState {
    /// The variant ref (e.g. `hh/react-minimal@1`).
    pub variant_ref: String,
    /// The plan cursor.
    pub cursor: PlanCursor,
    /// Decisions taken (`decision_count` — capped by the `turns` hard
    /// ceiling, driver obligation).
    pub decision_count: u64,
    /// Currently open effect ids (`decide` may not return `act` while this
    /// is non-empty unless `capabilities.parallel_effects`).
    pub open_effects: Vec<String>,
    /// The greatest cue seq consumed (idempotent `observe`/`decide` on it).
    pub last_cue_seq: u64,
    /// `DecisionPoint → Owner` — the boundary as the variant sees it.
    pub boundary_view: BTreeMap<DecisionPoint, Owner>,
    /// The variant-private extension (react/minimal:
    /// `{format_error_streak, submissions_seen}`; a closed per-variant
    /// record — canonical-serialisable by construction).
    pub extension: Json,
}

impl ControlState {
    /// The mandatory-header canonical form `{dialect, variant_ref, cursor,
    /// decision_count, open_effects, last_cue_seq, boundary_view,
    /// extension}`.
    pub fn to_json(&self) -> Json {
        let bv: Vec<(String, Json)> = self
            .boundary_view
            .iter()
            .map(|(p, o)| (decision_point_str(*p).to_string(), Json::str(owner_str(*o))))
            .collect();
        Json::obj([
            ("dialect", Json::str(CONTROL_STATE_DIALECT)),
            ("variant_ref", Json::str(&self.variant_ref)),
            ("cursor", self.cursor.to_json()),
            ("decision_count", Json::Int(self.decision_count as i64)),
            (
                "open_effects",
                Json::Arr(self.open_effects.iter().map(Json::str).collect()),
            ),
            ("last_cue_seq", Json::Int(self.last_cue_seq as i64)),
            ("boundary_view", Json::Obj(bv.into_iter().collect())),
            ("extension", self.extension.clone()),
        ])
    }

    /// Parse the canonical form (variant-agnostic — the extension is checked
    /// by the variant's `restore`).
    pub fn from_json(j: &Json) -> Option<ControlState> {
        if j.get("dialect")?.as_str()? != CONTROL_STATE_DIALECT {
            return None;
        }
        let bv = match j.get("boundary_view")? {
            Json::Obj(m) => m,
            _ => return None,
        };
        let mut boundary_view = BTreeMap::new();
        for (p, o) in bv {
            boundary_view.insert(parse_decision_point(p)?, parse_owner(o.as_str()?)?);
        }
        Some(ControlState {
            variant_ref: j.get("variant_ref")?.as_str()?.into(),
            cursor: PlanCursor::from_json(j.get("cursor")?)?,
            decision_count: j.get("decision_count")?.as_int()? as u64,
            open_effects: match j.get("open_effects")? {
                Json::Arr(items) => items
                    .iter()
                    .map(|i| i.as_str().map(String::from))
                    .collect::<Option<Vec<_>>>()?,
                _ => return None,
            },
            last_cue_seq: j.get("last_cue_seq")?.as_int()? as u64,
            boundary_view,
            extension: j.get("extension")?.clone(),
        })
    }

    /// `checkpoint(state)` — the canonical bytes (ADR-0029 class).
    pub fn checkpoint(&self) -> Vec<u8> {
        self.to_json().to_canonical_string().into_bytes()
    }

    /// `decision_point` canonical spelling (the `DecisionPoint` codec lives
    /// here — the ontology enum is a plain sum without a wire spelling; the
    /// control crate is its sole Stage-1 consumer boundary).
    pub fn record_decision(&mut self, point: DecisionPoint, owner: Owner) {
        self.boundary_view.insert(point, owner);
        self.decision_count += 1;
    }
}

/// `DecisionPoint` canonical spellings (the closed ten-member set).
pub fn decision_point_str(p: DecisionPoint) -> &'static str {
    match p {
        DecisionPoint::Plan => "plan",
        DecisionPoint::Act => "act",
        DecisionPoint::Retrieve => "retrieve",
        DecisionPoint::Compact => "compact",
        DecisionPoint::Verify => "verify",
        DecisionPoint::Delegate => "delegate",
        DecisionPoint::Authorize => "authorize",
        DecisionPoint::Retry => "retry",
        DecisionPoint::Stop => "stop",
        DecisionPoint::Escalate => "escalate",
    }
}

/// Parse a `DecisionPoint` spelling.
pub fn parse_decision_point(s: &str) -> Option<DecisionPoint> {
    Some(match s {
        "plan" => DecisionPoint::Plan,
        "act" => DecisionPoint::Act,
        "retrieve" => DecisionPoint::Retrieve,
        "compact" => DecisionPoint::Compact,
        "verify" => DecisionPoint::Verify,
        "delegate" => DecisionPoint::Delegate,
        "authorize" => DecisionPoint::Authorize,
        "retry" => DecisionPoint::Retry,
        "stop" => DecisionPoint::Stop,
        "escalate" => DecisionPoint::Escalate,
        _ => return None,
    })
}

/// `Owner` canonical spellings.
pub fn owner_str(o: Owner) -> &'static str {
    match o {
        Owner::Code => "code",
        Owner::Model => "model",
        Owner::Human => "human",
    }
}

/// Parse an `Owner` spelling.
pub fn parse_owner(s: &str) -> Option<Owner> {
    Some(match s {
        "code" => Owner::Code,
        "model" => Owner::Model,
        "human" => Owner::Human,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_through_the_canonical_checkpoint() {
        let mut s = ControlState {
            variant_ref: "hh/react-minimal@1".into(),
            cursor: PlanCursor {
                node_id: "loop-1".into(),
                iteration: 3,
                bound_ref: "budget-1".into(),
            },
            decision_count: 4,
            open_effects: vec!["eff-1".into()],
            last_cue_seq: 42,
            boundary_view: BTreeMap::new(),
            extension: Json::obj([("format_error_streak", Json::Int(1))]),
        };
        s.boundary_view.insert(DecisionPoint::Act, Owner::Model);
        s.boundary_view.insert(DecisionPoint::Stop, Owner::Code);
        let bytes = s.checkpoint();
        let back = ControlState::from_json(
            &hh_wire::json::parse(std::str::from_utf8(&bytes).unwrap()).unwrap(),
        );
        assert_eq!(back.as_ref(), Some(&s));
    }

    #[test]
    fn boundary_view_serializes_owner_spelling() {
        let s = ControlState {
            variant_ref: "v".into(),
            cursor: PlanCursor {
                node_id: "n".into(),
                iteration: 0,
                bound_ref: "b".into(),
            },
            decision_count: 0,
            open_effects: vec![],
            last_cue_seq: 0,
            boundary_view: BTreeMap::from([(DecisionPoint::Authorize, Owner::Code)]),
            extension: Json::Null,
        };
        let j = s.to_json();
        let bv = j.get("boundary_view").unwrap();
        assert_eq!(bv.get("authorize").and_then(Json::as_str), Some("code"));
    }

    #[test]
    fn every_decision_point_and_owner_spelling_round_trips() {
        for p in hh_ontology::control::DecisionPoint::ALL {
            assert_eq!(parse_decision_point(decision_point_str(p)), Some(p));
        }
        for o in [Owner::Code, Owner::Model, Owner::Human] {
            assert_eq!(parse_owner(owner_str(o)), Some(o));
        }
    }
}
