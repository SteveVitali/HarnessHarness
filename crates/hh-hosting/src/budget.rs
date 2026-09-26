//! `budget_enforcement` derivation + the boundary decision points (spec §6.6
//! §8 / §12; R-2.10.6; S4.5a).
//!
//! `budget_enforcement` is a map `dimension → {enforced, advisory,
//! unenforceable}` the **Lab derives** — never the participant's claim
//! ([`HostingExt::budget_enforcement`] is the *declared claim* the adapter
//! asserts; this module computes what the Lab can actually enforce from
//! mechanism × placement × interception × environment ownership).
//!
//! The floor (§6.6 §8):
//!
//! * `time.wall_ms`, `turns`, `approvals.requested` are enforced for a live
//!   session-ABI session (the Lab owns the wall clock and the turn surface;
//!   `approvals.requested` rides the mediated permission channel).
//! * A `container_installed` mechanism has no turn/approval surface — turns
//!   and approvals are `unenforceable` (never silently advisory).
//! * `network.calls` and `env.active_ms` are `enforced` only when the
//!   environment is Lab-provisioned; otherwise `unenforceable` (a remote
//!   service's network is out of reach — advisory would pretend).
//! * `spend` is `enforced` iff the model path is gateway-mediated
//!   (`model_io_intercept ≠ none`); otherwise `advisory` (participant-reported
//!   usage can still render — it can never stop a run).
//! * `tokens.*` are `enforced` iff `model_io_intercept ≠ none`; otherwise
//!   `advisory` when the participant reports usage (`usage_reporting`
//!   supported), else `unenforceable`.
//! * `model_calls`/`tool_calls` are `enforced` under interception /
//!   the sealed-tool surface respectively — tool calls through the Lab's own
//!   MCP surface are always mediated at `in_environment`/`lab_host`.
//!
//! The decision points (§6.6 §8): before `submit`, at `request_permission`,
//! before a model call through the proxy, at EP3, and at wall-clock timeout —
//! [`check_ceiling`] is the one predicate the service calls at each. A matched
//! dimension with no `enforced` level never hard-stops; exhaustion lands
//! `budget_exhausted{dimension}` after `session.closed`.

use std::collections::BTreeMap;

use hh_budget::errors::EnforcementLevel;
use hh_ontology::dimensions::DimensionId;
use hh_ontology::participant::HostingMechanism;
use hh_wire::Json;

use crate::abi::HostingError;
use crate::records::{EnforcementClaim, ModelIoIntercept, ProcessPlacement};

/// The declared enforcement map the Lab derives (§6.6 §8). `usage_reporting`
/// is the participant's declared usage-report capability verdict — reported
/// usage lifts a token dimension from `unenforceable` to `advisory` (it can
/// render, never stop).
pub fn derive_budget_enforcement(
    mechanism: HostingMechanism,
    placement: ProcessPlacement,
    intercept: ModelIoIntercept,
    environment_lab_provisioned: bool,
    usage_reporting_supported: bool,
) -> BTreeMap<String, EnforcementClaim> {
    let mut m: BTreeMap<String, EnforcementClaim> = BTreeMap::new();
    let session = mechanism == HostingMechanism::SessionAbi;
    let intercepted = !matches!(
        intercept,
        ModelIoIntercept::None | ModelIoIntercept::Unknown
    );
    let in_lab = matches!(
        placement,
        ProcessPlacement::InEnvironment | ProcessPlacement::LabHost
    );

    // ── the floor ────────────────────────────────────────────────────────
    // The Lab owns the wall clock at every placement it drives — enforced.
    m.insert(
        DimensionId::TimeWallMs.as_str().to_string(),
        EnforcementLevel::Enforced,
    );
    // Turns exist on the session surface only.
    m.insert(
        DimensionId::Turns.as_str().to_string(),
        if session {
            EnforcementLevel::Enforced
        } else {
            EnforcementLevel::Unenforceable
        },
    );
    // Approvals ride the mediated permission channel — session-ABI only.
    m.insert(
        DimensionId::ApprovalsRequested.as_str().to_string(),
        if session && in_lab {
            EnforcementLevel::Enforced
        } else {
            EnforcementLevel::Unenforceable
        },
    );

    // ── environment-owned dimensions ─────────────────────────────────────
    for d in [
        DimensionId::NetworkCalls,
        DimensionId::NetworkBytesOut,
        DimensionId::NetworkBytesIn,
        DimensionId::EnvActiveMs,
    ] {
        m.insert(
            d.as_str().to_string(),
            if environment_lab_provisioned && in_lab {
                EnforcementLevel::Enforced
            } else {
                EnforcementLevel::Unenforceable
            },
        );
    }

    // ── spend / model calls / tokens — gateway-mediation gated ───────────
    m.insert(
        DimensionId::Spend.as_str().to_string(),
        if intercepted {
            EnforcementLevel::Enforced
        } else if usage_reporting_supported {
            EnforcementLevel::Advisory
        } else {
            EnforcementLevel::Unenforceable
        },
    );
    m.insert(
        DimensionId::ModelCalls.as_str().to_string(),
        if intercepted {
            EnforcementLevel::Enforced
        } else {
            // A live session knows it asked the model — a turn implies ≥1
            // call; the count itself is only advisory without interception.
            EnforcementLevel::Advisory
        },
    );
    for d in [
        DimensionId::TokensInputUncached,
        DimensionId::TokensInputCacheRead,
        DimensionId::TokensInputCacheWrite,
        DimensionId::TokensOutputVisible,
        DimensionId::TokensOutputReasoning,
    ] {
        m.insert(
            d.as_str().to_string(),
            if intercepted {
                EnforcementLevel::Enforced
            } else if usage_reporting_supported {
                EnforcementLevel::Advisory
            } else {
                EnforcementLevel::Unenforceable
            },
        );
    }

    // ── tools — the sealed MCP surface is Lab-mediated at Lab placements ──
    m.insert(
        DimensionId::ToolCalls.as_str().to_string(),
        if session && in_lab {
            EnforcementLevel::Enforced
        } else if session {
            // A remote session-ABI peer still reports its tool calls.
            EnforcementLevel::Advisory
        } else {
            EnforcementLevel::Unenforceable
        },
    );
    m
}

/// A hard-ceiling decision point input — `{budget_id, dimension, used, cap}`:
/// the run's accumulated consumption and the slice's hard cap on `dimension`.
#[derive(Debug, Clone, PartialEq)]
pub struct CeilingCheck {
    /// The budget slice.
    pub budget_id: String,
    /// The dimension (a `DimensionId` spelling).
    pub dimension: String,
    /// The consumption so far.
    pub used: i64,
    /// The hard cap (`None` = uncapped — never exhausted).
    pub cap: Option<i64>,
}

/// The one ceiling predicate (§6.6 §8 — called before `submit`, at
/// `request_permission`, before a proxied model call, at EP3, and at the
/// wall-clock timeout). `enforcement` is the derived map: an `enforced`
/// dimension whose `used ≥ cap` refuses `BudgetExhausted{dimension}` (the
/// run ends `budget_exhausted` after `session.closed`); `advisory` never
/// refuses; `unenforceable`/undeclared never refuses — a matched dimension
/// without `enforced` is `validate_match`'s refusal, not this one's.
pub fn check_ceiling(
    check: &CeilingCheck,
    enforcement: &BTreeMap<String, EnforcementClaim>,
) -> Result<(), HostingError> {
    let Some(cap) = check.cap else { return Ok(()) };
    if check.used < cap {
        return Ok(());
    }
    let enforced = matches!(
        enforcement.get(&check.dimension),
        Some(EnforcementLevel::Enforced)
    );
    if enforced {
        Err(HostingError::BudgetExhausted {
            budget_id: check.budget_id.clone(),
            dimension: check.dimension.clone(),
        })
    } else {
        Ok(())
    }
}

/// The enforcement map as `Json` (the `session{ budget_enforcement }` attach
/// member + the manifest stratum — `{dimension → spelling}`).
pub fn enforcement_json(m: &BTreeMap<String, EnforcementClaim>) -> Json {
    Json::Obj(
        m.iter()
            .map(|(d, l)| (d.clone(), Json::str(l.as_str())))
            .collect(),
    )
}

/// The derived map → the engine-side `BudgetEnforcement` (`hh_budget`
/// records-in — unknown dimensions are dropped from the *map* with the map
/// itself carried verbatim in `extra_dims` for honesty).
pub fn to_budget_enforcement(
    m: &BTreeMap<String, EnforcementClaim>,
) -> (
    hh_budget::matchspec::BudgetEnforcement,
    Vec<(String, EnforcementClaim)>,
) {
    let mut levels = BTreeMap::new();
    let mut extra = Vec::new();
    for (d, l) in m {
        match DimensionId::parse(d) {
            Some(dim) => {
                levels.insert(dim, *l);
            }
            None => extra.push((d.clone(), *l)),
        }
    }
    (
        hh_budget::matchspec::BudgetEnforcement::hosted(&levels.into_iter().collect::<Vec<_>>()),
        extra,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::ModelIoIntercept as I;
    use crate::records::ProcessPlacement as P;
    use hh_ontology::participant::HostingMechanism as M;

    fn derive(
        mech: M,
        place: P,
        intercept: I,
        env: bool,
        usage: bool,
    ) -> BTreeMap<String, EnforcementLevel> {
        derive_budget_enforcement(mech, place, intercept, env, usage)
    }

    #[test]
    fn session_abi_in_environment_enforces_the_floor() {
        let m = derive(M::SessionAbi, P::InEnvironment, I::None, true, true);
        assert_eq!(m["time.wall_ms"], EnforcementLevel::Enforced);
        assert_eq!(m["turns"], EnforcementLevel::Enforced);
        assert_eq!(m["approvals.requested"], EnforcementLevel::Enforced);
        assert_eq!(m["network.calls"], EnforcementLevel::Enforced);
        assert_eq!(m["env.active_ms"], EnforcementLevel::Enforced);
        // No interception: spend + tokens advisory when the participant
        // reports usage (renders, never stops).
        assert_eq!(m["spend"], EnforcementLevel::Advisory);
        assert_eq!(m["tokens.input.uncached"], EnforcementLevel::Advisory);
        assert_eq!(m["tool_calls"], EnforcementLevel::Enforced);
    }

    #[test]
    fn container_installed_cannot_enforce_turns_or_approvals() {
        let m = derive(
            M::ContainerInstalled,
            P::InEnvironment,
            I::None,
            true,
            false,
        );
        assert_eq!(m["turns"], EnforcementLevel::Unenforceable);
        assert_eq!(m["approvals.requested"], EnforcementLevel::Unenforceable);
        assert_eq!(m["time.wall_ms"], EnforcementLevel::Enforced);
        assert_eq!(m["spend"], EnforcementLevel::Unenforceable);
    }

    #[test]
    fn interception_upgrades_spend_and_tokens_to_enforced() {
        let m = derive(M::SessionAbi, P::LabHost, I::BaseUrl, false, false);
        assert_eq!(m["spend"], EnforcementLevel::Enforced);
        assert_eq!(m["tokens.output.visible"], EnforcementLevel::Enforced);
        assert_eq!(m["model_calls"], EnforcementLevel::Enforced);
        // A non-Lab-provisioned env's dims are unenforceable — never advisory.
        assert_eq!(m["network.calls"], EnforcementLevel::Unenforceable);
        assert_eq!(m["env.active_ms"], EnforcementLevel::Unenforceable);
    }

    #[test]
    fn remote_service_keeps_only_the_lab_owned_floor() {
        let m = derive(M::SessionAbi, P::RemoteService, I::None, false, true);
        assert_eq!(m["time.wall_ms"], EnforcementLevel::Enforced);
        assert_eq!(m["turns"], EnforcementLevel::Enforced);
        assert_eq!(m["approvals.requested"], EnforcementLevel::Unenforceable);
        assert_eq!(m["tool_calls"], EnforcementLevel::Advisory);
        assert_eq!(m["network.calls"], EnforcementLevel::Unenforceable);
    }

    #[test]
    fn the_ceiling_refuses_only_enforced_dimensions() {
        let enforced = [("turns".to_string(), EnforcementLevel::Enforced)]
            .into_iter()
            .collect();
        let c = |dim: &str, used, cap| CeilingCheck {
            budget_id: "b1".into(),
            dimension: dim.into(),
            used,
            cap,
        };
        assert!(check_ceiling(&c("turns", 3, Some(4)), &enforced).is_ok());
        assert!(matches!(
            check_ceiling(&c("turns", 4, Some(4)), &enforced),
            Err(HostingError::BudgetExhausted { .. })
        ));
        // Advisory/unenforceable/absent never refuse (the match gate owns
        // that decision); uncapped never refuses.
        let adv = [("turns".to_string(), EnforcementLevel::Advisory)]
            .into_iter()
            .collect();
        assert!(check_ceiling(&c("turns", 99, Some(1)), &adv).is_ok());
        assert!(check_ceiling(&c("other", 99, Some(1)), &enforced).is_ok());
        assert!(check_ceiling(&c("turns", 99, None), &enforced).is_ok());
    }

    #[test]
    fn the_engine_map_projects_known_dimensions_and_keeps_extras() {
        let mut m = derive(M::SessionAbi, P::InEnvironment, I::None, true, true);
        m.insert("vendor.dim".to_string(), EnforcementLevel::Advisory);
        let (be, extra) = to_budget_enforcement(&m);
        assert!(!be.native);
        assert_eq!(be.level(DimensionId::Turns), EnforcementLevel::Enforced);
        assert_eq!(
            extra,
            vec![("vendor.dim".to_string(), EnforcementLevel::Advisory)]
        );
    }
}
