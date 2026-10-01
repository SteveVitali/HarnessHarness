//! Adapter B — model-boundary interception for hook-bearing CLIs without
//! a session protocol (§6.6 §5; R-2.10.6 C2; ADR-0164/0166).
//!
//! The shape: the Lab's interception proxy lives **inside** the
//! environment on the participant's model path (`model_io` rides the
//! `proxy` channel — `mediated`, never participant-claimed); the
//! participant's own hooks arrive on the `hook` channel as `observed`
//! rows; a hook-reported model call the proxy did not intercept is
//! reconstructed from the transcript rows (the fallback — `unobserved`,
//! never `mediated`); the `model` coordinate is set by proxy override
//! (`set_coordinate{model}` lands on the proxy's upstream, never the
//! participant's config); and the generation-parameter policy is a
//! declared adapter parameter whose debt record the `AdapterRecord`
//! carries (T-LCD-05 reflexive).

use std::collections::BTreeMap;

use hh_provenance::AuthorityClass;
use hh_wire::Json;

use crate::adapter_a::LiftedObservation;
use crate::events::{EventChannel, HostedOrigin, Mediation};
use crate::records::{AdapterRecord, ProcessPlacement};

/// The adapter-B id (`hh.adapter.b` — the model-boundary interceptor).
pub const ADAPTER_B_ID: &str = "hh.adapter.b";

/// The generation-parameter policy — a *declared adapter parameter*
/// (§6.6 §5): `forward[]` is the closed set of generation parameters the
/// proxy passes through to the upstream model; `default{param → value}`
/// is what the proxy pins when the participant did not set it. Anything
/// else the participant asks for is stripped and logged, never silently
/// applied.
#[derive(Debug, Clone, PartialEq)]
pub struct GenParamPolicy {
    /// The parameters the proxy forwards (`temperature`, `max_tokens`,
    /// `top_p`, …).
    pub forward: Vec<String>,
    /// The pinned defaults (`param → value`).
    pub default: BTreeMap<String, Json>,
}

impl GenParamPolicy {
    /// Canonical JSON (the adapter record's `ext.model_io_intercept`
    /// policy member and the debt record's hypothesis input).
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "forward",
                Json::Arr(self.forward.iter().map(Json::str).collect()),
            ),
            (
                "default",
                Json::Obj(
                    self.default
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
            ),
        ])
    }

    /// Filter a participant's requested generation parameters through
    /// the policy — `{forwarded, stripped, defaulted}` (every stripped
    /// member is listed, never silently applied).
    pub fn apply(&self, requested: &Json) -> Json {
        let mut forwarded = BTreeMap::new();
        let mut stripped = Vec::new();
        if let Json::Obj(req) = requested {
            for (k, v) in req {
                if self.forward.iter().any(|f| f == k) {
                    forwarded.insert(k.clone(), v.clone());
                } else {
                    stripped.push(Json::str(k));
                }
            }
        }
        let mut defaulted = Vec::new();
        for (k, v) in &self.default {
            if !forwarded.contains_key(k) {
                forwarded.insert(k.clone(), v.clone());
                defaulted.push(Json::str(k));
            }
        }
        Json::obj([
            ("forwarded", Json::Obj(forwarded)),
            ("stripped", Json::Arr(stripped)),
            ("defaulted", Json::Arr(defaulted)),
        ])
    }
}

/// The Adapter-B transport — records-in/records-out: the proxy and the
/// hook channel are supplies, the adapter never owns a handle.
pub trait InterceptTransport {
    /// Drain the participant's hook rows (`{hook, payload, at?}`).
    fn drain_hooks(&mut self) -> Vec<Json>;
    /// Drain the proxy's intercepted `model.call.*` observations.
    fn drain_model_observations(&mut self) -> Vec<Json>;
    /// The proxy's `model` override — the `model` coordinate path
    /// (`set_coordinate{model}` lands here, never on the participant's
    /// config — §6.6 §5).
    fn set_model_override(&mut self, model_ref: &str) -> Result<(), String>;
    /// Raw transcript rows `[from_seq, to_seq]` — the reconstruction
    /// fallback's source (the participant's own transcript format).
    fn transcript_rows(&mut self, from_seq: u64, to_seq: u64) -> Vec<Json>;
}

/// Adapter B.
pub struct AdapterB {
    transport: Box<dyn InterceptTransport>,
    record: AdapterRecord,
    /// The declared generation-parameter policy.
    pub gen_param_policy: GenParamPolicy,
    /// `model_call_id`s the proxy observed (the fallback's covered set).
    proxied: BTreeMap<String, ()>,
    /// The highest transcript seq the reconstruction has consumed.
    transcript_watermark: u64,
    /// A per-adapter monotone counter for `raw_ref` correlation.
    observation_seq: u64,
}

impl AdapterB {
    /// Build over a transport + the adapter's registry record.
    pub fn new(
        transport: Box<dyn InterceptTransport>,
        record: AdapterRecord,
        gen_param_policy: GenParamPolicy,
    ) -> AdapterB {
        AdapterB {
            transport,
            record,
            gen_param_policy,
            proxied: BTreeMap::new(),
            transcript_watermark: 0,
            observation_seq: 0,
        }
    }

    /// The adapter's registry record.
    pub fn record(&self) -> &AdapterRecord {
        &self.record
    }

    fn next_raw(&mut self, tag: &str) -> String {
        self.observation_seq += 1;
        format!(
            "{}:{}:{}",
            self.record.adapter_id, tag, self.observation_seq
        )
    }

    /// Lift the proxy's model-I/O observations — `intercept` origin,
    /// `mediated` mediation on the `proxy` channel (the kernel-gated
    /// channel the mediation stamp is lawful on; the gateway's own rows
    /// are `origin = kernel` when the proxy is kernel-hosted).
    pub fn lift_proxy(&mut self) -> Vec<LiftedObservation> {
        let mut out = Vec::new();
        for obs in self.transport.drain_model_observations() {
            let kind = obs
                .get("kind")
                .and_then(Json::as_str)
                .unwrap_or("model.call.completed")
                .to_string();
            if let Some(id) = obs.get("model_call_id").and_then(Json::as_str) {
                self.proxied.insert(id.to_string(), ());
            }
            let raw = self.next_raw("proxy");
            let mut lift = LiftedObservation {
                kind,
                payload: obs,
                origin: HostedOrigin::Intercept,
                authority: AuthorityClass::Environment,
                mediation: Mediation::Mediated,
                event_channel: EventChannel::Proxy,
                raw_ref: None,
                ext: BTreeMap::new(),
            };
            lift.raw_ref = Some(raw);
            out.push(lift);
        }
        out
    }

    /// Lift the hook channel — `observed` participant rows (`hook`
    /// channel), each carrying the hook's name in the payload.
    pub fn lift_hooks(&mut self) -> Vec<LiftedObservation> {
        let mut out = Vec::new();
        for h in self.transport.drain_hooks() {
            let hook = h
                .get("hook")
                .and_then(Json::as_str)
                .unwrap_or("unknown")
                .to_string();
            let payload = h.get("payload").cloned().unwrap_or(Json::Null);
            let raw = self.next_raw("hook");
            out.push(LiftedObservation {
                kind: format!("hook.{hook}"),
                payload,
                origin: HostedOrigin::Participant,
                authority: AuthorityClass::Delegate,
                mediation: Mediation::Observed,
                event_channel: EventChannel::Hook,
                raw_ref: Some(raw),
                ext: BTreeMap::new(),
            });
        }
        out
    }

    /// The transcript-reconstruction fallback — for every hook-reported
    /// model call the proxy did not observe, the transcript rows carry
    /// it: `unobserved` mediation on the `log` channel, `adapter` origin
    /// (the Lab reconstructed it; the Lab saw no channel — §6.6 §5).
    /// Returns the lifted rows; each carries `reconstructed_from` naming
    /// the transcript window.
    pub fn reconstruct_transcript(&mut self, to_seq: u64) -> Vec<LiftedObservation> {
        let from = self.transcript_watermark + 1;
        let rows = self.transport.transcript_rows(from, to_seq);
        self.transcript_watermark = to_seq;
        let mut out = Vec::new();
        for row in rows {
            let call_id = row
                .get("model_call_id")
                .and_then(Json::as_str)
                .map(str::to_string);
            // A call the proxy already intercepted is never re-lifted
            // (one channel of record per fact).
            if let Some(id) = &call_id {
                if self.proxied.contains_key(id) {
                    continue;
                }
            }
            let kind = row
                .get("kind")
                .and_then(Json::as_str)
                .unwrap_or("model.call.completed")
                .to_string();
            let raw = self.next_raw("transcript");
            let mut payload = match row {
                Json::Obj(m) => m,
                other => {
                    let mut m = BTreeMap::new();
                    m.insert("row".into(), other);
                    m
                }
            };
            payload.insert(
                "reconstructed_from".into(),
                Json::str(format!("transcript[{from}..{to_seq}]")),
            );
            out.push(LiftedObservation {
                kind,
                payload: Json::Obj(payload),
                origin: HostedOrigin::Adapter,
                authority: AuthorityClass::Unverified,
                mediation: Mediation::Unobserved,
                event_channel: EventChannel::Log,
                raw_ref: Some(raw),
                ext: BTreeMap::new(),
            });
        }
        out
    }

    /// `set_coordinate{model}` — the proxy override path: the Lab's
    /// bound model coordinate lands on the proxy's upstream (never on
    /// the participant's own config).
    pub fn set_model(&mut self, model_ref: &str) -> Result<LiftedObservation, String> {
        self.transport.set_model_override(model_ref)?;
        let raw = self.next_raw("coordinate");
        let mut ext = BTreeMap::new();
        ext.insert("coordinate".to_string(), Json::str("model"));
        Ok(LiftedObservation {
            kind: "lifecycle.hosted.coordinate_set".to_string(),
            payload: Json::obj([
                ("coordinate", Json::str("model")),
                ("value", Json::str(model_ref)),
                ("via", Json::str("proxy_override")),
            ]),
            origin: HostedOrigin::Adapter,
            authority: AuthorityClass::Environment,
            mediation: Mediation::Observed,
            event_channel: EventChannel::Proxy,
            raw_ref: Some(raw),
            ext,
        })
    }
}

/// The Adapter-B `AdapterRecord` — `model_boundary_intercept`,
/// `in_environment` placement, the declared `gen_param_policy` on
/// `ext.model_io_intercept`, and the caller-supplied debt record (the
/// generation-parameter policy is a declared adapter parameter *with a
/// debt record* — T-LCD-05 reflexive; §6.6 §5).
pub fn adapter_b_record(
    adapter_version: &str,
    gen_param_policy: &GenParamPolicy,
    debt: hh_hir::records::AssumptionDebtRecord,
) -> AdapterRecord {
    let mut defaults = BTreeMap::new();
    for (dim, v) in [
        ("coordinate_model", "supported"),
        ("usage_reporting", "supported"),
        ("end_state", "supported"),
        ("streaming", "unknown"),
        ("resume_cold", "unknown"),
        ("resume_warm", "unknown"),
        ("steer", "unknown"),
    ] {
        defaults.insert(dim.to_string(), Json::str(v));
    }
    let mut ext = BTreeMap::new();
    ext.insert("model_io_intercept".to_string(), Json::str("base_url"));
    ext.insert("gen_param_policy".to_string(), gen_param_policy.to_json());
    AdapterRecord {
        adapter_id: ADAPTER_B_ID.to_string(),
        version_id: adapter_version.to_string(),
        hosting_mechanism: hh_ontology::participant::HostingMechanism::ModelBoundaryIntercept,
        participant_selector: Json::obj([("mechanism", Json::str("model_boundary_intercept"))]),
        declaration_defaults: defaults,
        placement_supported: [ProcessPlacement::InEnvironment].into_iter().collect(),
        lowering_table_ref: None,
        loss_report_ref: None,
        debt,
        ext,
    }
}
