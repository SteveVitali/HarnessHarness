//! `FleetSpec` — the durable activation record (§5i.1 #1). Opened under
//! `lifecycle.fleet.activated` + carried in the activation manifest's
//! `extra.fleet_spec`; `spec_ref = H(canonical(FleetSpec))` is the
//! `stale_spec` leg every dispatch decision re-checks (RC-1).
//!
//! The spec is the *only* declarative input the reconciler reads — agents,
//! capacity, the ownership graph, trigger rules and escalation/retry
//! defaults are all durable before any `WorkItem` can dispatch (CC3:
//! nothing unpinned).

use hh_budget::spec::BudgetSpec;
use hh_embed_schema::types::NarrowingLeaf;
use hh_ledger::wakeup::{Trigger, WakeupPolicy};
use hh_wire::json::Json;
use std::collections::BTreeMap;

use crate::errors::FleetError;
use crate::identity::{js, SPEC_SCHEMA};

/// `capacity = {activate_run: int, items: int}` — RC-6's fleet-scoped
/// concurrency bounds (`activate_run ≤ 0` refuses `capacity`).
#[derive(Debug, Clone, PartialEq)]
pub struct Capacity {
    /// The bound on concurrent `dispatching`+`dispatched` items.
    pub activate_run: u64,
    /// The bound on live (non-terminal) items.
    pub items: u64,
}

/// `defaults` — the activation's escalation/retry/stall knobs (durable;
/// the reconciler never reads process configuration).
#[derive(Debug, Clone, PartialEq)]
pub struct Defaults {
    /// Default escalation deadline when an item's `on.blocked.escalate`
    /// omits `deadline_ms`.
    pub escalation_timeout_ms: Option<u64>,
    /// Default retry budget when an item's `on.dispatch_error` omits
    /// `max_attempts`.
    pub max_retries: u64,
    /// Default retry backoff (ms) — the `retry_due` cadence.
    pub retry_backoff_ms: u64,
    /// Default dispatch lease (ms) for `dispatch{lease}` rows.
    pub dispatch_lease_ms: u64,
    /// Default stall window (ms) for `on.stall` items without their own.
    pub stall_timeout_ms: u64,
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults {
            escalation_timeout_ms: None,
            max_retries: 3,
            retry_backoff_ms: 60_000,
            dispatch_lease_ms: 300_000,
            stall_timeout_ms: 300_000,
        }
    }
}

/// `TriggerRule` — one member of `spec.triggers` (`occurs_on` names these).
/// The `Trigger` sum is `hh-ledger`'s; the fleet admits `external`/`manual`/
/// `timer` only at this boundary and every other member refuses typed
/// (`FleetError::UnsupportedTrigger` → `TriggerUnsupported` at subscribe).
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerRule {
    /// The rule name (`occurs_on` members name it).
    pub name: String,
    /// The §5a.3 trigger — `external{kind}`/`manual{principal}`/`timer{at}`.
    pub trigger: Trigger,
    /// The wakeup policy (`follow_up`; `max_pending` bounds the subscription).
    pub policy: WakeupPolicy,
}

/// Whether a `Trigger` is admissible in a fleet trigger rule — the §5i.1
/// #2 set: `external`/`manual` (the fleet boundary's addition) plus `timer`
/// (the kernel-internal one the activation's own deadlines use). Every
/// other member keeps its typed refusal.
pub fn fleet_trigger_admissible(t: &Trigger) -> bool {
    matches!(
        t,
        Trigger::External { .. } | Trigger::Manual { .. } | Trigger::Timer { .. }
    )
}

/// `FleetSpec` — the whole declarative surface of one activation.
#[derive(Debug, Clone, PartialEq)]
pub struct FleetSpec {
    /// The spec document schema (`hh.fleet.spec/1`).
    pub schema: String,
    /// The spec version label (operator-authored; `spec_ref` is the content
    /// coordinate — `version` is a label only).
    pub version: String,
    /// The activation name.
    pub name: String,
    /// `purpose = "fleet_activation"` — the manifest's declared purpose.
    pub purpose: String,
    /// The source fixture document ref (the work-source adapter's pinned
    /// document — the activation replays the *same* source snapshot).
    pub fixture_ref: String,
    /// The declared agents (the ownership graph's vertex set; item `owner`s
    /// and `set_owner` targets must resolve inside it — `owner_unknown` /
    /// `cross_fleet` otherwise).
    pub agents: Vec<String>,
    /// `capacity = {activate_run, items}`.
    pub capacity: Capacity,
    /// The ownership edges: `agent_id → owner_agent_id` (acyclic; checked at
    /// open and under `set_owner` — `cycle` is a `GraphError`).
    pub ownership: BTreeMap<String, String>,
    /// The Π table coordinate the activation's dispatches run under
    /// (`policy_ref` lands in every `FleetAnchor` + `policy_fingerprint`).
    pub policy_ref: String,
    /// The declared Π narrowing leaves — persisted to
    /// `manifest.extra["narrowing_leaves"]` (the same member `hh-embed`
    /// fingerprints; CC1 one leg) and narrowed per-item at `state_map`.
    pub narrowing: Vec<NarrowingLeaf>,
    /// The activation's budget spec — the matched-budget conditional's
    /// `matched` arm (one root allocation on the activation's account).
    pub budget: Option<BudgetSpec>,
    /// `budget_ref` — the deterministic budget coordinate durable rows
    /// name (`H(fleet_run)`; the allocated `budget_*` id joins in the fold).
    pub budget_ref: Option<String>,
    /// The conditional's other arm — `true` declares the activation's
    /// dispatches are out-of-scope for budget matching (still recorded
    /// `matched: false`, never silently unbudgeted — CC9).
    pub out_of_scope: bool,
    /// `trigger_set` — named `TriggerRule`s (`occurs_on`/`escalate` targets).
    pub triggers: BTreeMap<String, TriggerRule>,
    /// Escalation/retry/stall defaults.
    pub defaults: Defaults,
    /// `state_map` human-gate members — source `state` spellings that
    /// put an item in the `handoff` state on observation (§5i.1 #2
    /// reconcile step (2): "source entered a human-gate state through
    /// the agent's own effect ⇒ `handoff{state}`" — the gate set is
    /// durable spec data, `blocked{human_gate}` is the durable record).
    pub human_gate_states: Vec<String>,
}

impl FleetSpec {
    /// The canonical JSON form (deterministic — `spec_ref` covers it).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), js(&self.schema));
        m.insert("version".into(), js(&self.version));
        m.insert("name".into(), js(&self.name));
        m.insert("purpose".into(), js(&self.purpose));
        m.insert("fixture_ref".into(), js(&self.fixture_ref));
        m.insert(
            "agents".into(),
            Json::Arr(self.agents.iter().map(js).collect()),
        );
        m.insert(
            "capacity".into(),
            Json::obj([
                ("activate_run", Json::Int(self.capacity.activate_run as i64)),
                ("items", Json::Int(self.capacity.items as i64)),
            ]),
        );
        m.insert(
            "ownership".into(),
            Json::Obj(
                self.ownership
                    .iter()
                    .map(|(k, v)| (k.clone(), js(v)))
                    .collect(),
            ),
        );
        m.insert("policy_ref".into(), js(&self.policy_ref));
        m.insert(
            "narrowing".into(),
            Json::Arr(self.narrowing.iter().map(|l| l.to_json()).collect()),
        );
        m.insert(
            "budget".into(),
            self.budget
                .as_ref()
                .map(|b| b.to_json())
                .unwrap_or(Json::Null),
        );
        m.insert(
            "budget_ref".into(),
            self.budget_ref
                .as_ref()
                .map(js)
                .unwrap_or(Json::Null),
        );
        m.insert("out_of_scope".into(), Json::Bool(self.out_of_scope));
        m.insert(
            "triggers".into(),
            Json::Obj(
                self.triggers
                    .iter()
                    .map(|(k, r)| {
                        (
                            k.clone(),
                            Json::obj([
                                ("trigger", r.trigger.to_json()),
                                ("policy", r.policy.to_json()),
                            ]),
                        )
                    })
                    .collect(),
            ),
        );
        m.insert(
            "defaults".into(),
            Json::Obj(BTreeMap::from([
                (
                    "escalation_timeout_ms".into(),
                    self.defaults
                        .escalation_timeout_ms
                        .map(|t| Json::Int(t as i64))
                        .unwrap_or(Json::Null),
                ),
                ("max_retries".into(), Json::Int(self.defaults.max_retries as i64)),
                (
                    "retry_backoff_ms".into(),
                    Json::Int(self.defaults.retry_backoff_ms as i64),
                ),
                (
                    "dispatch_lease_ms".into(),
                    Json::Int(self.defaults.dispatch_lease_ms as i64),
                ),
                (
                    "stall_timeout_ms".into(),
                    Json::Int(self.defaults.stall_timeout_ms as i64),
                ),
            ])),
        );
        m.insert(
            "human_gate_states".into(),
            Json::Arr(self.human_gate_states.iter().map(js).collect()),
        );
        Json::Obj(m)
    }

    /// The strict decoder — unknown members fail closed (CC7/CC8: a spec
    /// written by a newer build is never silently truncated).
    pub fn from_json(j: &Json) -> Result<FleetSpec, FleetError> {
        let o = match j {
            Json::Obj(m) => m,
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "fleet spec must be an object".into(),
                })
            }
        };
        let req = |k: &str| -> Result<&Json, FleetError> {
            o.get(k).ok_or_else(|| FleetError::SchemaViolation {
                detail: format!("fleet spec missing {k}"),
            })
        };
        let str_of = |v: &Json, k: &str| -> Result<String, FleetError> {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| FleetError::SchemaViolation {
                    detail: format!("fleet spec {k} must be a string"),
                })
        };
        const KNOWN: &[&str] = &[
            "schema", "version", "name", "purpose", "fixture_ref", "agents",
            "capacity", "ownership", "policy_ref", "narrowing", "budget",
            "budget_ref", "out_of_scope", "triggers", "defaults",
            "human_gate_states",
        ];
        for k in o.keys() {
            if !KNOWN.contains(&k.as_str()) {
                return Err(FleetError::SchemaViolation {
                    detail: format!("fleet spec unknown member {k}"),
                });
            }
        }
        let schema = str_of(req("schema")?, "schema")?;
        if schema != SPEC_SCHEMA {
            return Err(FleetError::SchemaViolation {
                detail: format!("fleet spec schema {schema} (expected {SPEC_SCHEMA})"),
            });
        }
        let agents: Vec<String> = match req("agents")? {
            Json::Arr(a) => a
                .iter()
                .map(|v| str_of(v, "agents[]"))
                .collect::<Result<_, _>>()?,
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "fleet spec agents must be an array".into(),
                })
            }
        };
        let cap = req("capacity")?;
        let cap_i = |k: &str| -> Result<u64, FleetError> {
            cap.get(k)
                .and_then(Json::as_int)
                .map(|i| i.max(0) as u64)
                .ok_or_else(|| FleetError::SchemaViolation {
                    detail: format!("fleet spec capacity.{k} must be a non-negative int"),
                })
        };
        let ownership = match req("ownership")? {
            Json::Obj(m) => m
                .iter()
                .map(|(k, v)| {
                    str_of(v, "ownership[]").map(|s| (k.clone(), s))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?,
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "fleet spec ownership must be an object".into(),
                })
            }
        };
        let narrowing: Vec<NarrowingLeaf> = match req("narrowing")? {
            Json::Arr(a) => {
                let mut out = Vec::new();
                for v in a {
                    out.push(NarrowingLeaf::from_json(v, "/narrowing").map_err(
                        |e| FleetError::SchemaViolation {
                            detail: format!("narrowing leaf: {e:?}"),
                        },
                    )?);
                }
                out
            }
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "fleet spec narrowing must be an array".into(),
                })
            }
        };
        let mut triggers = BTreeMap::new();
        match req("triggers")? {
            Json::Obj(m) => {
                for (k, v) in m {
                    let trigger = Trigger::from_json(v.get("trigger").unwrap_or(&Json::Null))
                        .ok_or_else(|| FleetError::SchemaViolation {
                            detail: format!("trigger {k} malformed"),
                        })?;
                    if !fleet_trigger_admissible(&trigger) {
                        return Err(FleetError::UnsupportedTrigger {
                            trigger: trigger.type_name().to_string(),
                        });
                    }
                    let policy =
                        WakeupPolicy::from_json(v.get("policy").unwrap_or(&Json::Null))
                            .ok_or_else(|| FleetError::SchemaViolation {
                                detail: format!("trigger {k} policy malformed"),
                            })?;
                    triggers.insert(
                        k.clone(),
                        TriggerRule {
                            name: k.clone(),
                            trigger,
                            policy,
                        },
                    );
                }
            }
            _ => {
                return Err(FleetError::SchemaViolation {
                    detail: "fleet spec triggers must be an object".into(),
                })
            }
        }
        let defaults = {
            let d = req("defaults")?;
            let i = |k: &str, def: u64| -> Result<u64, FleetError> {
                match d.get(k) {
                    None | Some(Json::Null) => Ok(def),
                    Some(v) => v
                        .as_int()
                        .map(|x| x.max(0) as u64)
                        .ok_or_else(|| FleetError::SchemaViolation {
                            detail: format!("fleet spec defaults.{k} must be an int"),
                        }),
                }
            };
            let esc = match d.get("escalation_timeout_ms") {
                None | Some(Json::Null) => None,
                Some(v) => {
                    let t = v
                        .as_int()
                        .ok_or_else(|| FleetError::SchemaViolation {
                            detail: "defaults.escalation_timeout_ms must be an int".into(),
                        })?;
                    Some(t.max(0) as u64)
                }
            };
            Defaults {
                escalation_timeout_ms: esc,
                max_retries: i("max_retries", 3)?,
                retry_backoff_ms: i("retry_backoff_ms", 60_000)?,
                dispatch_lease_ms: i("dispatch_lease_ms", 300_000)?,
                stall_timeout_ms: i("stall_timeout_ms", 300_000)?,
            }
        };
        Ok(FleetSpec {
            schema,
            version: str_of(req("version")?, "version")?,
            name: str_of(req("name")?, "name")?,
            purpose: str_of(req("purpose")?, "purpose")?,
            fixture_ref: str_of(req("fixture_ref")?, "fixture_ref")?,
            agents,
            capacity: Capacity {
                activate_run: cap_i("activate_run")?,
                items: cap_i("items")?,
            },
            ownership,
            policy_ref: str_of(req("policy_ref")?, "policy_ref")?,
            narrowing,
            budget: match req("budget")? {
                Json::Null => None,
                v => Some(BudgetSpec::from_json(v).ok_or_else(|| {
                    FleetError::SchemaViolation {
                        detail: "fleet spec budget malformed".into(),
                    }
                })?),
            },
            budget_ref: match req("budget_ref")? {
                Json::Null => None,
                v => Some(str_of(v, "budget_ref")?),
            },
            out_of_scope: matches!(req("out_of_scope")?, Json::Bool(true)),
            triggers,
            defaults,
            human_gate_states: match req("human_gate_states")? {
                Json::Arr(a) => a
                    .iter()
                    .map(|v| str_of(v, "human_gate_states[]"))
                    .collect::<Result<Vec<_>, _>>()?,
                _ => {
                    return Err(FleetError::SchemaViolation {
                        detail: "fleet spec human_gate_states must be a list".into(),
                    })
                }
            },
        })
    }

    /// Structural validation (beyond the codec's): `purpose`, the
    /// matched-budget conditional, the ownership graph's edge set, and
    /// `capacity` coherence — the `fleet.open` checks.
    pub fn validate(&self) -> Result<(), FleetError> {
        if self.purpose != "fleet_activation" {
            return Err(FleetError::SchemaViolation {
                detail: format!(
                    "fleet spec purpose {} (expected fleet_activation)",
                    self.purpose
                ),
            });
        }
        // The matched-budget conditional: `budget`+`budget_ref` (matched) or
        // `out_of_scope: true` — never neither, never silently unbudgeted.
        match (&self.budget, &self.budget_ref, self.out_of_scope) {
            (Some(_), Some(_), _) => {}
            (None, None, true) => {}
            (None, _, false) => return Err(FleetError::MissingBudgetRef),
            (Some(_), None, _) => return Err(FleetError::MissingBudgetRef),
            (None, Some(_), _) => return Err(FleetError::MissingBudgetRef),
        }
        for a in &self.agents {
            if a.is_empty() {
                return Err(FleetError::SchemaViolation {
                    detail: "fleet spec agents[] members must be non-empty".into(),
                });
            }
        }
        Ok(())
    }
}
