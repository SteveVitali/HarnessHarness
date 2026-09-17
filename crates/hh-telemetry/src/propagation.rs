//! The propagation contract (§5h.1 §2.3; ADR-0042 D3): cross-process
//! correlation that never touches an IR entity (T-LCD-06). Rendered ids are
//! `surface_ids` aliases — ledger identity stays `(run_id, event_id)`.
//!
//! `outbound_context` renders the W3C `traceparent`/`tracestate` pair under the
//! provisional derivation (ADR-0042; OQ-115): `trace_id = H(root_run_id)[0:16]`,
//! `span_id = H(event_id)[0:8]`, `sampled = 1`, `tracestate` member
//! `hh=run:<run_id>;ev:<event_id>`, optional `baggage{hh.configuration_id}`.
//! `H` is the one canonical hash — `hh_identity::idp::idp_digest` under the
//! `telemetry.trace_id`/`telemetry.span_id` domains (CC1).
//!
//! The Stage-1 seam is the **subprocess environment**: `subprocess_env` renders
//! the pairs the executor injects (S1.16 lowers them into the child env); the
//! MCP/ACP `_meta` slots land at Stage 2 (§5h.1 §9).

use hh_identity::idp::idp_digest;

use crate::errors::TelemetryError;

/// The env var carrying `traceparent` into a subprocess (the Stage-1 seam).
pub const TRACEPARENT_ENV: &str = "HH_TRACEPARENT";
/// The env var carrying `tracestate` into a subprocess.
pub const TRACESTATE_ENV: &str = "HH_TRACESTATE";
/// The env var carrying `baggage` into a subprocess (optional).
pub const BAGGAGE_ENV: &str = "HH_BAGGAGE";

/// The `idp/1` domain for surface trace ids (`trace_id = H(root_run_id)[0:16]`).
pub const TRACE_ID_DOMAIN: &str = "telemetry.trace_id";
/// The `idp/1` domain for surface span ids (`span_id = H(event_id)[0:8]`).
pub const SPAN_ID_DOMAIN: &str = "telemetry.span_id";

/// `trace_id` for a run tree — `H(root_run_id)[0:16]` rendered as 32 lowercase
/// hex chars (the W3C trace-id shape). Deterministic — every span of a run tree
/// shares it, and it is derivable from the root run id alone.
pub fn trace_id(root_run_id: &str) -> String {
    idp_digest(TRACE_ID_DOMAIN, root_run_id.as_bytes())[..32].to_string()
}

/// `span_id` for an event — `H(event_id)[0:8]` as 16 hex chars.
pub fn span_id(event_id: &str) -> String {
    idp_digest(SPAN_ID_DOMAIN, event_id.as_bytes())[..16].to_string()
}

/// `PropagationContext{traceparent, tracestate, baggage?}` (§5h.1 §2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropagationContext {
    /// The W3C `traceparent` — `00-<trace_id>-<span_id>-01`.
    pub traceparent: String,
    /// The W3C `tracestate` — member `hh=run:<run_id>;ev:<event_id>`.
    pub tracestate: String,
    /// Optional baggage — `hh.configuration_id=<configuration_id>`.
    pub baggage: Option<String>,
}

/// `outbound_context(run_id, event_id)` → the propagation context (§5h.1 §2.3).
/// `root_run_id` is the run-tree root (the trace identity); `run_id`/`event_id`
/// are this emission's ledger coordinates — the `hh` tracestate member carries
/// them verbatim so lifting resolves the real `EventRef`.
pub fn outbound_context(
    root_run_id: &str,
    run_id: &str,
    event_id: &str,
    configuration_id: Option<&str>,
) -> PropagationContext {
    PropagationContext {
        traceparent: format!("00-{}-{}-01", trace_id(root_run_id), span_id(event_id)),
        tracestate: format!("hh=run:{run_id};ev:{event_id}"),
        baggage: configuration_id.map(|c| format!("hh.configuration_id={c}")),
    }
}

/// The subprocess-seam rendering: the env pairs a lowering injects into the
/// child process environment (§5h.1 §2.3 — "injected by lowering into … the
/// subprocess environment"). Pure — the executor performs the injection.
pub fn subprocess_env(ctx: &PropagationContext) -> Vec<(String, String)> {
    let mut env = vec![
        (TRACEPARENT_ENV.to_string(), ctx.traceparent.clone()),
        (TRACESTATE_ENV.to_string(), ctx.tracestate.clone()),
    ];
    if let Some(b) = &ctx.baggage {
        env.push((BAGGAGE_ENV.to_string(), b.clone()));
    }
    env
}

/// Read the seam back — the child side's `inbound_context` over the env pairs.
/// `None` when the pair is absent (not a propagation failure — the target may
/// simply not have been instrumented).
pub fn inbound_from_env(vars: &[(String, String)]) -> Result<Option<InboundLink>, TelemetryError> {
    let traceparent = vars
        .iter()
        .find(|(k, _)| k == TRACEPARENT_ENV)
        .map(|(_, v)| v.as_str());
    let tracestate = vars
        .iter()
        .find(|(k, _)| k == TRACESTATE_ENV)
        .map(|(_, v)| v.as_str());
    match traceparent {
        Some(tp) => inbound_context(tp, tracestate).map(Some),
        None => Ok(None),
    }
}

/// `Link{run_id?, event_id?, external_span_id, external_trace_id}` — the
/// lifting result (§5h.1 §2.3). `run_id`/`event_id` resolve only through the
/// `hh` tracestate member; without it the external ids are aliases on the
/// lifting event, never a parent (a foreign context is never grafted onto an
/// unrelated span).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundLink {
    /// The ledger run — present only via the `hh` tracestate member.
    pub run_id: Option<String>,
    /// The ledger event — same rule.
    pub event_id: Option<String>,
    /// The inbound `trace_id` (alias).
    pub external_trace_id: String,
    /// The inbound `span_id` (alias).
    pub external_span_id: String,
    /// The `hh.configuration_id` baggage member, when carried.
    pub configuration_id: Option<String>,
}

/// `inbound_context(traceparent, tracestate)` — parse the W3C shape and resolve
/// the `hh` member. `InvalidInboundContext` on any malformed input — the caller
/// logs it `structural` and parents nothing.
pub fn inbound_context(
    traceparent: &str,
    tracestate: Option<&str>,
) -> Result<InboundLink, TelemetryError> {
    let bad = |detail: &str| TelemetryError::InvalidInboundContext {
        detail: detail.to_string(),
    };
    // W3C: `version(2hex)-trace_id(32hex)-span_id(16hex)-flags(2hex)`.
    let parts: Vec<&str> = traceparent.split('-').collect();
    if parts.len() != 4 {
        return Err(bad("traceparent is not version-trace-span-flags"));
    }
    let hex = |s: &str, n: usize| {
        s.len() == n
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    };
    if !hex(parts[0], 2) || !hex(parts[1], 32) || !hex(parts[2], 16) || !hex(parts[3], 2) {
        return Err(bad("traceparent member shape"));
    }
    if parts[1].bytes().all(|b| b == b'0') || parts[2].bytes().all(|b| b == b'0') {
        return Err(bad("all-zero trace/span id"));
    }
    // The `hh` tracestate member — `hh=run:<run_id>;ev:<event_id>`.
    let (mut run_id, mut event_id) = (None, None);
    if let Some(ts) = tracestate {
        for member in ts.split(',') {
            if let Some(v) = member.trim().strip_prefix("hh=") {
                for kv in v.split(';') {
                    if let Some(r) = kv.strip_prefix("run:") {
                        run_id = Some(r.to_string());
                    } else if let Some(e) = kv.strip_prefix("ev:") {
                        event_id = Some(e.to_string());
                    }
                }
            }
        }
    }
    Ok(InboundLink {
        run_id,
        event_id,
        external_trace_id: parts[1].to_string(),
        external_span_id: parts[2].to_string(),
        configuration_id: None,
    })
}

/// `PropagationUnsupported(target)` — recorded into the loss report as a
/// `dropped_fields` entry `propagation.<target>` (§5h.1 §2.3/§5 — never silent).
pub fn propagation_unsupported_loss(target: &str) -> String {
    format!("propagation.{target}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbound_context_is_the_provisional_derivation() {
        let ctx = outbound_context("run:root", "run:child", "evt:9", Some("cfg:1"));
        // trace_id = H(root_run_id)[0:16] — stable, shared across the tree.
        assert_eq!(ctx.traceparent.len(), 2 + 1 + 32 + 1 + 16 + 1 + 2);
        assert!(ctx.traceparent.starts_with("00-"));
        assert!(ctx.traceparent.ends_with("-01"));
        assert_eq!(&ctx.traceparent[3..35], &trace_id("run:root"));
        assert_eq!(&ctx.traceparent[36..52], &span_id("evt:9"));
        assert_eq!(ctx.tracestate, "hh=run:run:child;ev:evt:9");
        assert_eq!(ctx.baggage.as_deref(), Some("hh.configuration_id=cfg:1"));
        // Deterministic — two renders agree byte-for-byte.
        assert_eq!(
            ctx,
            outbound_context("run:root", "run:child", "evt:9", Some("cfg:1"))
        );
    }

    #[test]
    fn subprocess_env_round_trips_through_the_seam() {
        let ctx = outbound_context("run:root", "run:r", "evt:1", None);
        let env = subprocess_env(&ctx);
        assert_eq!(env.len(), 2); // no baggage member
        let link = inbound_from_env(&env).unwrap().unwrap();
        assert_eq!(link.run_id.as_deref(), Some("run:r"));
        assert_eq!(link.event_id.as_deref(), Some("evt:1"));
        assert_eq!(link.external_trace_id, trace_id("run:root"));
        assert_eq!(link.external_span_id, span_id("evt:1"));
        // Absent vars are not a propagation failure.
        assert_eq!(inbound_from_env(&[]).unwrap(), None);
    }

    #[test]
    fn invalid_inbound_context_is_refused_never_parented() {
        for bad in [
            "bogus",
            "00-xyz-0000000000000000-01",
            "00-00000000000000000000000000000000-0000000000000000-01", // all-zero
            "00-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-bbbbbbbbbbbbbbbb-01", // uppercase hex
            "00-0123456789abcdef0123456789abcdef-0123456789abcdef",    // missing flags
        ] {
            assert!(
                matches!(
                    inbound_context(bad, None),
                    Err(TelemetryError::InvalidInboundContext { .. })
                ),
                "{bad}"
            );
        }
        // A well-formed foreign context lifts as aliases only — no run/event.
        let foreign = "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01";
        let link = inbound_context(foreign, Some("vendor=x")).unwrap();
        assert_eq!(link.run_id, None);
        assert_eq!(link.event_id, None);
        assert_eq!(link.external_trace_id, "0123456789abcdef0123456789abcdef");
    }
}
