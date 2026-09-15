//! Model-blind gateway over one wire dialect + one static role (R-2.3.1, R-2.3.2⁰; §5b.1/§5b.2).
//! **Throwaway Stage-0 subset. Offline-only: the model is a scripted stub, never live.**
//!
//! AC-R-2.3.2-1: "With a one-entry role table and `policy.kind = static`, every model call
//! yields exactly one `model.route.decided` with `candidates_considered = [selected]`,
//! `deviation = false` and a `reservation_id`; the run's `model_set_realized` equals the
//! configured table." The router/fallback family, retries and the pricing table land at S1.18;
//! this is the one-entry static subset.
//!
//! Model-blind (R-2.3.1): the gateway routes by role and records `usage` on
//! `model.call.completed` without interpreting model internals; the gateway credential is held
//! kernel-side ([`HeldSecret`]) and never handed to the stub or written to the trace.

use crate::secrets::HeldSecret;
use crate::tools::{ToolCall, ToolResult};

/// A single-entry `ModelRoleTable` (`policy.kind = static`). The multi-entry table and the
/// router policy family land at S1.18.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRoleTable {
    entries: Vec<(String, String)>, // (role, model_ref)
}

impl ModelRoleTable {
    /// The one-entry static table (`role → model_ref`).
    pub fn single(role: impl Into<String>, model_ref: impl Into<String>) -> Self {
        Self {
            entries: vec![(role.into(), model_ref.into())],
        }
    }

    /// The configured model set (the `model_set_realized` target of AC-R-2.3.2-1).
    pub fn model_set(&self) -> Vec<String> {
        let mut s: Vec<String> = self.entries.iter().map(|(_, m)| m.clone()).collect();
        s.sort();
        s.dedup();
        s
    }

    fn resolve(&self, role: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(r, _)| r == role)
            .map(|(_, m)| m.as_str())
    }
}

/// The routing policy. Stage 0 has exactly one: `static` (no fallback chain, no learned
/// predictor — those are S1.18/S4.16a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePolicy {
    Static,
}

impl RoutePolicy {
    pub fn kind(self) -> &'static str {
        match self {
            RoutePolicy::Static => "static",
        }
    }
}

/// The decision recorded on `model.route.decided`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDecision {
    pub selected: String,
    pub candidates_considered: Vec<String>,
    pub deviation: bool,
    pub reservation_id: String,
}

/// Token usage reported on `model.call.completed`. `blended` is the exclusive input+output
/// counter charged against `tokens.blended` (the `TokenVector` view lands at S1.14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: i64,
    pub output_tokens: i64,
}

impl Usage {
    pub fn blended(&self) -> i64 {
        self.input_tokens + self.output_tokens
    }
}

/// One turn's model output. The stub scripts these; a real adapter would parse a provider
/// response into the same shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResponse {
    /// Assistant text (external authority when it reaches the trace/context).
    pub content: String,
    /// An optional tool the model wants to run this turn.
    pub tool_call: Option<ToolCall>,
    pub usage: Usage,
    /// True when the model declares the task done (no further turns).
    pub done: bool,
}

/// The offline model interface. `respond` sees the assembled prompt and the previous tool
/// observation (if any); it never reaches the network.
pub trait Model {
    fn respond(&mut self, prompt: &str, last_observation: Option<&ToolResult>) -> StubOutcome;
}

/// What the stub yields: a normal response, or a transport failure the driver's retry subset
/// must handle (R-2.6.2), or an unparseable turn the driver must refuse (parse refusal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StubOutcome {
    Response(ModelResponse),
    /// Transient transport failure (retryable by the envelope's transport-retry subset).
    TransportError {
        detail: String,
    },
    /// The turn could not be parsed into an action (parse refusal → stop).
    Unparseable {
        detail: String,
    },
}

/// A deterministic scripted model: it replays a fixed list of outcomes, one per call. This is
/// the offline fixture that stands in for a live model at Stage 0.
pub struct StubModel {
    script: std::collections::VecDeque<StubOutcome>,
}

impl StubModel {
    pub fn new(script: Vec<StubOutcome>) -> Self {
        Self {
            script: script.into(),
        }
    }
}

impl Model for StubModel {
    fn respond(&mut self, _prompt: &str, _last: Option<&ToolResult>) -> StubOutcome {
        self.script
            .pop_front()
            .unwrap_or(StubOutcome::Response(ModelResponse {
                content: "(no scripted turn; stopping)".into(),
                tool_call: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                },
                done: true,
            }))
    }
}

/// The gateway: routes by role, holds the credential kernel-side, and drives the (offline)
/// model.
pub struct Gateway<M: Model> {
    table: ModelRoleTable,
    policy: RoutePolicy,
    model: M,
    #[allow(dead_code)] // held kernel-side; deliberately never read into any produced byte.
    credential: HeldSecret,
    reservation_seq: u64,
    realized: Vec<String>,
}

/// A gateway call that could not be routed (the one Stage-0 routing refusal: no table entry).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayError {
    NoRoute { role: String },
}

impl<M: Model> Gateway<M> {
    pub fn new(
        table: ModelRoleTable,
        policy: RoutePolicy,
        model: M,
        credential: HeldSecret,
    ) -> Self {
        Self {
            table,
            policy,
            model,
            credential,
            reservation_seq: 0,
            realized: Vec::new(),
        }
    }

    pub fn policy(&self) -> RoutePolicy {
        self.policy
    }

    /// Decide the route for `role` (static policy: the sole candidate is the selected model).
    pub fn route(&mut self, role: &str) -> Result<RouteDecision, GatewayError> {
        let selected = self
            .table
            .resolve(role)
            .ok_or_else(|| GatewayError::NoRoute { role: role.into() })?
            .to_string();
        self.reservation_seq += 1;
        Ok(RouteDecision {
            candidates_considered: vec![selected.clone()],
            deviation: false,
            reservation_id: format!("resv-{}", self.reservation_seq),
            selected,
        })
    }

    /// One model call: route, then drive the (offline) model, recording the realized model.
    pub fn call(
        &mut self,
        role: &str,
        prompt: &str,
        last: Option<&ToolResult>,
    ) -> Result<(RouteDecision, StubOutcome), GatewayError> {
        let decision = self.route(role)?;
        let outcome = self.model.respond(prompt, last);
        if let StubOutcome::Response(_) = outcome {
            self.realized.push(decision.selected.clone());
        }
        Ok((decision, outcome))
    }

    /// The set of models actually used (`model_set_realized`); equals the configured table's
    /// model set on a completed run (AC-R-2.3.2-1).
    pub fn model_set_realized(&self) -> Vec<String> {
        let mut s = self.realized.clone();
        s.sort();
        s.dedup();
        s
    }

    pub fn configured_model_set(&self) -> Vec<String> {
        self.table.model_set()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::SecretRef;

    fn cred() -> HeldSecret {
        HeldSecret::new(
            SecretRef::new("API_TOKEN", "gateway credential"),
            "sk-live-abc",
        )
    }

    fn one_turn() -> Vec<StubOutcome> {
        vec![StubOutcome::Response(ModelResponse {
            content: "done".into(),
            tool_call: None,
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
            done: true,
        })]
    }

    #[test]
    fn static_route_has_single_candidate_no_deviation_and_a_reservation() {
        let mut g = Gateway::new(
            ModelRoleTable::single("driver", "stub/model-A"),
            RoutePolicy::Static,
            StubModel::new(one_turn()),
            cred(),
        );
        let d = g.route("driver").unwrap();
        // AC-R-2.3.2-1
        assert_eq!(d.selected, "stub/model-A");
        assert_eq!(d.candidates_considered, vec!["stub/model-A".to_string()]);
        assert!(!d.deviation);
        assert_eq!(d.reservation_id, "resv-1");
        assert_eq!(g.policy().kind(), "static");
    }

    #[test]
    fn reservation_ids_are_unique_per_call() {
        let mut g = Gateway::new(
            ModelRoleTable::single("driver", "stub/model-A"),
            RoutePolicy::Static,
            StubModel::new(one_turn()),
            cred(),
        );
        assert_ne!(
            g.route("driver").unwrap().reservation_id,
            g.route("driver").unwrap().reservation_id
        );
    }

    #[test]
    fn realized_model_set_equals_configured_after_a_call() {
        let mut g = Gateway::new(
            ModelRoleTable::single("driver", "stub/model-A"),
            RoutePolicy::Static,
            StubModel::new(one_turn()),
            cred(),
        );
        g.call("driver", "hello", None).unwrap();
        assert_eq!(g.model_set_realized(), g.configured_model_set());
        assert_eq!(g.model_set_realized(), vec!["stub/model-A".to_string()]);
    }

    #[test]
    fn unknown_role_is_typed_no_route() {
        let mut g = Gateway::new(
            ModelRoleTable::single("driver", "stub/model-A"),
            RoutePolicy::Static,
            StubModel::new(one_turn()),
            cred(),
        );
        assert_eq!(
            g.route("planner"),
            Err(GatewayError::NoRoute {
                role: "planner".into()
            })
        );
    }

    #[test]
    fn usage_blends_input_and_output() {
        assert_eq!(
            Usage {
                input_tokens: 10,
                output_tokens: 5
            }
            .blended(),
            15
        );
    }
}
