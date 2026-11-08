//! `ps0` — the §6.6 PS0 dialect-probe driver and the W2 corpus
//! precondition (manifest 279/R6.26; §10.1.4 W2(b); REQ-XD-041 stage
//! order, REQ-XC-210's measured-cached-share point; Appendix B Q-X23
//! default: the PS0 early gate is not used — PS0 runs only inside W2
//! under FP-S, fleshed by R6.40).
//!
//! This module is the *offline machinery* W2 must not write (§10.6:
//! execution rows contain no new code). The probe plan is one call per
//! dialect the first-pass lane serves — [`PROXIED_SURFACES`] from
//! `smoke.rs` (the records-level table the runner may name — CC1) —
//! plus the seed, `tool_choice` and `cache_control` probes; its
//! worst-case cost in integer µ$ is priced through
//! `hharness_budget::pricing` (the one pricing implementation — a
//! second pricer would be a CC1 violation), at no-cache planning
//! values until PS0/PP0 measure the cached share (REQ-XC-210). Any
//! plan over `$0.05` refuses before a single call.
//!
//! The corpus leg's precondition ([`check_corpus_precondition`]) is
//! machinery evidence only: a passed `hh-ps0-stage/1` and original
//! mini-swe-agent's PP0 trials passing AG3/AG5/AG6/AG8 — it reads no
//! verdict field (X-07; REQ-XH-121's AG-evidence set).

use std::collections::BTreeMap;

use hharness_budget::pricing::{self, PricingUnit};
use hharness_budget::usage::TokenDecomposition;
use hharness_budget::ModelRef;
use hharness_wire::json::Json;

use crate::admission::MINI_SWE_AGENT;
use crate::json_ext::JsonExt;
use crate::smoke::PROXIED_SURFACES;

/// `hh-ps0-stage/1` — the PS0 stage record.
pub const STAGE_SCHEMA: &str = "hh-ps0-stage/1";
/// `hh-pp0-ag-evidence/1` — one PP0 trial's AG-check evidence row (the
/// corpus precondition's PP0 input; W2's PP0 fold emits one per
/// trial — admission-gate results only, never a verdict member).
pub const PP0_AG_SCHEMA: &str = "hh-pp0-ag-evidence/1";
/// §6.6's PS0 bound: ≤ $0.05 = 50 000 µ$.
pub const PS0_CAP_MICROS: i64 = 50_000;
/// The window PS0 executes in (FP-4).
pub const PS0_WINDOW: &str = "W2";
/// The spend gate PS0 runs under.
pub const PS0_GATE: &str = "FP-S";
/// The AG gates the corpus precondition reads (§10.1.4 W2(b)).
pub const CORPUS_GATES: [&str; 4] = ["AG3", "AG5", "AG6", "AG8"];

/// The surfaces whose params scope admits `seed` (§4.2's declared
/// table — openai.chat only; anthropic.messages has no `seed` member).
pub const SEED_SURFACES: [&str; 1] = ["openai.chat"];
/// The surfaces whose field policy declares `tool_choice` (both
/// first-pass dialects).
pub const TOOL_CHOICE_SURFACES: [&str; 2] = ["openai.chat", "anthropic.messages"];
/// The surfaces whose field policy declares `cache_control`
/// (anthropic.messages only — `strip_recursive`).
pub const CACHE_CONTROL_SURFACES: [&str; 1] = ["anthropic.messages"];

/// The probe kinds — the closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeKind {
    /// One call per dialect the proxy serves.
    Dialect,
    /// The `seed` probe (the dialect's seed parameter path).
    Seed,
    /// The `tool_choice` probe (tools + `tool_choice: "auto"`).
    ToolChoice,
    /// The `cache_control` probe — prime + read calls measuring the
    /// cached share per endpoint (REQ-XC-210).
    CacheControl,
}

impl ProbeKind {
    /// The recorded spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeKind::Dialect => "dialect",
            ProbeKind::Seed => "seed",
            ProbeKind::ToolChoice => "tool_choice",
            ProbeKind::CacheControl => "cache_control",
        }
    }
}

/// One planned PS0 call — the declared request the driver issues.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeCall {
    /// The probe id (`ps0-dialect-openai.chat`, `ps0-cache-control-read`, …).
    pub probe_id: String,
    /// The probe kind.
    pub kind: ProbeKind,
    /// The dialect surface the call targets (`openai.chat` …).
    pub surface: String,
    /// The downstream route the call arrives on.
    pub route: String,
    /// The request body.
    pub body: Json,
    /// The declared input-token bound the plan prices at (worst case —
    /// no-cache planning, REQ-XC-210).
    pub declared_input_tokens: i64,
    /// The declared output-token bound (the body's `max_tokens`).
    pub declared_output_tokens: i64,
}

/// The plan's worst-case bound breach — the plan refuses before any
/// call (X-02: mechanical, never a downgrade).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanRefusal {
    /// The priced worst case.
    pub worst_case_micros: i64,
    /// The cap.
    pub cap_micros: i64,
}

impl std::fmt::Display for PlanRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "refused{{plan_bound}}: worst-case {} µ$ exceeds the PS0 cap {} µ$ ($0.05)",
            self.worst_case_micros, self.cap_micros
        )
    }
}
impl std::error::Error for PlanRefusal {}

/// The slot grant every probe call rides — the per-slot token and
/// slice (the proxy's own reservation bounds each call; the plan bound
/// is PS0's own ≤ $0.05 check, X-02).
#[derive(Debug, Clone, PartialEq)]
pub struct SlotGrant {
    /// The slot token's record ref (never the value).
    pub token_ref: String,
    /// The slot's proxy slice in µ$.
    pub slice_micros: i64,
}

/// The usage view a receipted call reports (cached share input —
/// REQ-XC-210's measurement).
#[derive(Debug, Clone, PartialEq)]
pub struct UsageView {
    /// Uncached input tokens.
    pub input_uncached: i64,
    /// Cache-read input tokens.
    pub cache_read: i64,
    /// Output tokens.
    pub output: i64,
}

/// One probe call's receipted outcome — what the transport read off
/// the proxy receipts.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeOutcome {
    /// The call's disposition (`pass` for a completed 2xx receipt; any
    /// typed refusal/failure otherwise — recorded verbatim).
    pub disposition: String,
    /// The proxy receipt's record ref (every probe is receipted — an
    /// empty ref is an `unreceipted` disposition, never dropped).
    pub receipt_ref: String,
    /// The reported usage (cached share input), when the receipt
    /// carries it.
    pub usage: Option<UsageView>,
    /// The metered cost in µ$.
    pub cost_micros: i64,
}

/// The call-boundary seam — the production transport posts each probe
/// to the proxy's model socket on the slot token (W2, R6.40); the
/// offline battery drives a scripted fixture.
pub trait ProbeTransport {
    /// Issue one probe call on the slot token.
    fn call(&mut self, slot: &SlotGrant, call: &ProbeCall) -> Result<ProbeOutcome, Ps0Error>;
}

/// A transport-level failure — the call never produced a receipt.
#[derive(Debug, Clone, PartialEq)]
pub struct Ps0Error {
    /// The verbatim detail.
    pub detail: String,
}
impl std::fmt::Display for Ps0Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ps0: {}", self.detail)
    }
}
impl std::error::Error for Ps0Error {}

/// A typed corpus-precondition refusal — machinery evidence only,
/// never a verdict read (X-07).
#[derive(Debug, Clone, PartialEq)]
pub enum CorpusRefusal {
    /// The `hh-ps0-stage/1` record is absent, malformed or did not
    /// pass.
    Ps0NotPassed {
        /// The verbatim detail.
        detail: String,
    },
    /// No `hh-pp0-ag-evidence/1` record names original mini-swe-agent.
    Pp0EvidenceAbsent,
    /// A PP0 trial failed an AG gate.
    Pp0AgFailed {
        /// The trial ref.
        trial: String,
        /// The failed gate (`AG3`|`AG5`|`AG6`|`AG8`).
        gate: String,
        /// The recorded result.
        result: String,
    },
}

impl std::fmt::Display for CorpusRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CorpusRefusal::Ps0NotPassed { detail } => {
                write!(f, "corpus_precondition:ps0_not_passed: {detail}")
            }
            CorpusRefusal::Pp0EvidenceAbsent => {
                write!(f, "corpus_precondition:pp0_evidence_absent")
            }
            CorpusRefusal::Pp0AgFailed {
                trial,
                gate,
                result,
            } => write!(
                f,
                "corpus_precondition:pp0_{gate}_failed: {trial} = {result}"
            ),
        }
    }
}
impl std::error::Error for CorpusRefusal {}

fn dialect_body(model: &str, surface: &str) -> Json {
    match surface {
        "anthropic.messages" => Json::obj([
            ("model", Json::str(model)),
            ("max_tokens", Json::Int(8)),
            ("stream", Json::Bool(false)),
            (
                "messages",
                Json::Arr(vec![Json::obj([
                    ("role", Json::str("user")),
                    (
                        "content",
                        Json::str("PS0 dialect probe — answer with the single token ok."),
                    ),
                ])]),
            ),
        ]),
        _ => Json::obj([
            ("model", Json::str(model)),
            ("max_tokens", Json::Int(8)),
            ("stream", Json::Bool(false)),
            (
                "messages",
                Json::Arr(vec![Json::obj([
                    ("role", Json::str("user")),
                    (
                        "content",
                        Json::str("PS0 dialect probe — answer with the single token ok."),
                    ),
                ])]),
            ),
        ]),
    }
}

/// `surface_route(s)` — the downstream path the surface's calls arrive
/// on (R6.25's records-level table; the strict codec stays in
/// hh-hosting — no K-2 edge). `None` for a surface the proxy does not
/// serve — a lane claiming one refuses typed, never silently routed.
pub fn probe_route(surface: &str) -> Option<&'static str> {
    match surface {
        "anthropic.messages" => Some("\u{2f}v1\u{2f}messages"),
        "openai.chat" => Some("\u{2f}v1\u{2f}chat\u{2f}completions"),
        _ => None,
    }
}

/// `ps0_plan(surfaces, model)` — the §6.6 probe plan: one call per
/// dialect the lane serves (the caller passes the landed lane's
/// dialect list — the runner never chooses it, contract Notes), plus
/// the seed, `tool_choice` and `cache_control` probes on the surfaces
/// whose declared field policy admits them. The `cache_control` probe
/// is two calls (prime + read) so the read's usage measures the cached
/// share (REQ-XC-210). A dialect outside [`PROXIED_SURFACES`] refuses
/// typed — the plan never invents a route.
pub fn ps0_plan(surfaces: &[&str], model: &str) -> Result<Vec<ProbeCall>, Ps0Error> {
    for s in surfaces {
        if probe_route(s).is_none() {
            return Err(Ps0Error {
                detail: format!("unsupported{{dialect}}: {s} is not a lane the proxy serves"),
            });
        }
    }
    let mut out = Vec::new();
    let mut push = |kind: ProbeKind, surface: &str, body: Json, in_tok: i64, out_tok: i64| {
        out.push(ProbeCall {
            probe_id: format!("ps0-{}-{}", kind.as_str().replace('_', "-"), surface),
            kind,
            surface: surface.to_string(),
            route: probe_route(surface).unwrap_or("").to_string(),
            body,
            declared_input_tokens: in_tok,
            declared_output_tokens: out_tok,
        });
    };
    for s in surfaces {
        push(ProbeKind::Dialect, s, dialect_body(model, s), 128, 8);
    }
    for s in surfaces {
        if !SEED_SURFACES.contains(s) {
            continue;
        }
        let mut body = dialect_body(model, s);
        if let Json::Obj(m) = &mut body {
            m.insert("seed".to_string(), Json::Int(42));
        }
        push(ProbeKind::Seed, s, body, 128, 8);
    }
    for s in surfaces {
        if !TOOL_CHOICE_SURFACES.contains(s) {
            continue;
        }
        let mut body = dialect_body(model, s);
        let (tools, choice) = if *s == "anthropic.messages" {
            (
                Json::Arr(vec![Json::obj([
                    ("name", Json::str("ps0_probe")),
                    ("description", Json::str("PS0 tool_choice probe tool")),
                    (
                        "input_schema",
                        Json::obj([("type", Json::str("object")), ("properties", Json::obj([]))]),
                    ),
                ])]),
                Json::obj([("type", Json::str("auto"))]),
            )
        } else {
            (
                Json::Arr(vec![Json::obj([
                    ("type", Json::str("function")),
                    (
                        "function",
                        Json::obj([
                            ("name", Json::str("ps0_probe")),
                            ("description", Json::str("PS0 tool_choice probe tool")),
                            (
                                "parameters",
                                Json::obj([
                                    ("type", Json::str("object")),
                                    ("properties", Json::obj([])),
                                ]),
                            ),
                        ]),
                    ),
                ])]),
                Json::str("auto"),
            )
        };
        if let Json::Obj(m) = &mut body {
            m.insert("tools".to_string(), tools);
            m.insert("tool_choice".to_string(), choice);
        }
        push(ProbeKind::ToolChoice, s, body, 192, 8);
    }
    for s in surfaces {
        if !CACHE_CONTROL_SURFACES.contains(s) {
            continue;
        }
        let mut body = dialect_body(model, s);
        if let Json::Obj(m) = &mut body {
            m.insert(
                "system".to_string(),
                Json::Arr(vec![Json::obj([
                    ("type", Json::str("text")),
                    (
                        "text",
                        Json::str("PS0 cache_control probe — a shared prefix long enough to make a measured cache read meaningful."),
                    ),
                    (
                        "cache_control",
                        Json::obj([("type", Json::str("ephemeral"))]),
                    ),
                ])]),
            );
        }
        for leg in ["prime", "read"] {
            out.push(ProbeCall {
                probe_id: format!("ps0-cache-control-{leg}"),
                kind: ProbeKind::CacheControl,
                surface: s.to_string(),
                route: probe_route(s).unwrap_or("").to_string(),
                body: body.clone(),
                declared_input_tokens: 1_536,
                declared_output_tokens: 8,
            });
        }
    }
    Ok(out)
}

/// `probe_worst_case_micros(call, model_ref, table)` — one probe's
/// worst case priced through `hharness_budget::pricing::price` (the
/// one pricer — CC1): declared input at the uncached rate (no-cache
/// planning, REQ-XC-210) plus the declared output bound.
pub fn probe_worst_case_micros(
    call: &ProbeCall,
    model_ref: &ModelRef,
    table: &impl PricingUnit,
) -> Result<i64, hharness_budget::SpendError> {
    let d = TokenDecomposition {
        input_uncached: call.declared_input_tokens,
        output_visible: call.declared_output_tokens,
        ..Default::default()
    };
    let (money, _roles) = pricing::price(&d, model_ref, table)?;
    Ok(money.micro_units)
}

/// `plan_worst_case_micros(calls, model_ref, table)` — Σ per-probe
/// worst case. The plan refuses typed when the sum exceeds
/// [`PS0_CAP_MICROS`] — before any call is issued.
pub fn plan_worst_case_micros(
    calls: &[ProbeCall],
    model_ref: &ModelRef,
    table: &impl PricingUnit,
) -> Result<i64, hharness_budget::SpendError> {
    let mut sum = 0i64;
    for c in calls {
        sum += probe_worst_case_micros(c, model_ref, table)?;
    }
    Ok(sum)
}

/// `check_plan_bound(calls, model_ref, table)` — the mechanical ≤
/// $0.05 bound: [`PlanRefusal`] when the priced worst case exceeds the
/// cap. Runs before the first probe (X-02).
pub fn check_plan_bound(
    calls: &[ProbeCall],
    model_ref: &ModelRef,
    table: &impl PricingUnit,
) -> Result<i64, PlanRefusal> {
    let worst = plan_worst_case_micros(calls, model_ref, table).unwrap_or(i64::MAX);
    if worst > PS0_CAP_MICROS {
        return Err(PlanRefusal {
            worst_case_micros: worst,
            cap_micros: PS0_CAP_MICROS,
        });
    }
    Ok(worst)
}

/// One probe row of the stage record.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeRow {
    /// The plan call.
    pub call: ProbeCall,
    /// The recorded disposition.
    pub disposition: String,
    /// The receipt ref.
    pub receipt_ref: String,
    /// The reported usage.
    pub usage: Option<UsageView>,
    /// The metered cost in µ$.
    pub cost_micros: i64,
}

/// The `hh-ps0-stage/1` record — one disposition per probe plus the
/// measured cached share per endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct Ps0Stage {
    /// The plan calls, in issued order.
    pub probes: Vec<ProbeRow>,
    /// The priced worst case (integer µ$).
    pub worst_case_micros: i64,
    /// The measured cached share per surface: ppm of 1.0, `None` =
    /// `unmeasured` (no usage rows reported cache reads).
    pub cached_share_ppm: BTreeMap<String, Option<i64>>,
    /// Σ metered probe cost (informational — the bound is the plan's).
    pub metered_micros: i64,
    /// `pass` iff every probe's disposition is `pass`.
    pub disposition: String,
    /// The slot grant the probes rode.
    pub slot: SlotGrant,
    /// The recorded instant.
    pub recorded_at: String,
    /// The evidence label — `recorded_execution` until W2's leg earns
    /// `live-executed` through `hh-live-evidence/1` (REQ-XH-027).
    pub evidence_label: String,
}

impl Ps0Stage {
    /// The canonical record.
    pub fn to_json(&self) -> Json {
        let probes: Vec<Json> = self
            .probes
            .iter()
            .map(|p| {
                let mut m = vec![
                    ("probe_id", Json::str(&p.call.probe_id)),
                    ("kind", Json::str(p.call.kind.as_str())),
                    ("surface", Json::str(&p.call.surface)),
                    ("route", Json::str(&p.call.route)),
                    ("disposition", Json::str(&p.disposition)),
                    ("receipt_ref", Json::str(&p.receipt_ref)),
                    ("cost_micros", Json::Int(p.cost_micros)),
                ];
                if let Some(u) = &p.usage {
                    m.push((
                        "usage",
                        Json::obj([
                            ("input_uncached", Json::Int(u.input_uncached)),
                            ("cache_read", Json::Int(u.cache_read)),
                            ("output", Json::Int(u.output)),
                        ]),
                    ));
                }
                Json::obj(m)
            })
            .collect();
        Json::obj([
            ("schema", Json::str(STAGE_SCHEMA)),
            ("stage", Json::str("PS0")),
            ("window", Json::str(PS0_WINDOW)),
            ("gate", Json::str(PS0_GATE)),
            (
                "slot",
                Json::obj([
                    ("token_ref", Json::str(&self.slot.token_ref)),
                    ("slice_micros", Json::Int(self.slot.slice_micros)),
                ]),
            ),
            (
                "plan",
                Json::obj([
                    ("calls", Json::Int(self.probes.len() as i64)),
                    ("worst_case_micros", Json::Int(self.worst_case_micros)),
                    ("cap_micros", Json::Int(PS0_CAP_MICROS)),
                ]),
            ),
            ("probes", Json::Arr(probes)),
            (
                "cached_share",
                Json::Obj(
                    self.cached_share_ppm
                        .iter()
                        .map(|(s, ppm)| {
                            (
                                s.clone(),
                                match ppm {
                                    Some(p) => Json::obj([("ppm", Json::Int(*p))]),
                                    None => Json::str("unmeasured"),
                                },
                            )
                        })
                        .collect(),
                ),
            ),
            ("metered_micros", Json::Int(self.metered_micros)),
            ("disposition", Json::str(&self.disposition)),
            ("evidence_label", Json::str(&self.evidence_label)),
            ("recorded_at", Json::str(&self.recorded_at)),
        ])
    }
}

/// `run_ps0(calls, slot, transport, model_ref, table, recorded_at)` —
/// the driver: bound the plan first (a breach refuses before any
/// call), then issue exactly the planned calls in order on the slot
/// token, each receipted, and fold the stage record with per-probe
/// dispositions and the measured cached share per endpoint.
pub fn run_ps0(
    calls: &[ProbeCall],
    slot: &SlotGrant,
    transport: &mut dyn ProbeTransport,
    model_ref: &ModelRef,
    table: &impl PricingUnit,
    recorded_at: &str,
) -> Result<Ps0Stage, PlanRefusal> {
    let worst = check_plan_bound(calls, model_ref, table)?;
    let mut probes = Vec::new();
    let mut metered = 0i64;
    for call in calls {
        let row = match transport.call(slot, call) {
            Ok(o) => ProbeRow {
                call: call.clone(),
                disposition: if o.receipt_ref.is_empty() {
                    "unreceipted".to_string()
                } else {
                    o.disposition.clone()
                },
                receipt_ref: o.receipt_ref,
                usage: o.usage,
                cost_micros: o.cost_micros,
            },
            Err(e) => ProbeRow {
                call: call.clone(),
                disposition: format!("unreceipted{{transport:{}}}", e.detail),
                receipt_ref: String::new(),
                usage: None,
                cost_micros: 0,
            },
        };
        metered += row.cost_micros;
        probes.push(row);
    }
    // The measured cached share per endpoint: cache_read over total
    // input across that surface's receipted calls (REQ-XC-210's first
    // measured stage). A surface with no usage is `unmeasured`.
    let mut share: BTreeMap<String, Option<i64>> = BTreeMap::new();
    for s in PROXIED_SURFACES {
        share.insert(s.to_string(), None);
    }
    for s in PROXIED_SURFACES {
        let mut rd = 0i64;
        let mut tot = 0i64;
        for p in &probes {
            if p.call.surface != s {
                continue;
            }
            if let Some(u) = &p.usage {
                rd += u.cache_read;
                tot += u.input_uncached + u.cache_read;
            }
        }
        if tot > 0 {
            share.insert(s.to_string(), Some(rd * 1_000_000 / tot));
        }
    }
    let passed = probes.iter().all(|p| p.disposition == "pass");
    Ok(Ps0Stage {
        probes,
        worst_case_micros: worst,
        cached_share_ppm: share,
        metered_micros: metered,
        disposition: if passed { "pass" } else { "failed" }.to_string(),
        slot: slot.clone(),
        recorded_at: recorded_at.to_string(),
        evidence_label: "recorded_execution".to_string(),
    })
}

/// `check_corpus_precondition(ps0_stage, pp0_evidence)` — the §10.1.4
/// W2(b) corpus start condition, machinery evidence only (X-07): the
/// `hh-ps0-stage/1` record passed, and every original mini-swe-agent
/// `hh-pp0-ag-evidence/1` row passes AG3/AG5/AG6/AG8. Reads no verdict
/// member — the inputs carry none by schema.
pub fn check_corpus_precondition(
    ps0_stage: &Json,
    pp0_evidence: &[Json],
) -> Result<(), CorpusRefusal> {
    let m = ps0_stage.as_obj();
    let ok = m
        .map(|m| {
            m.get("schema").and_then(Json::as_str) == Some(STAGE_SCHEMA)
                && m.get("disposition").and_then(Json::as_str) == Some("pass")
        })
        .unwrap_or(false);
    if !ok {
        let detail = match m.and_then(|m| m.get("disposition")).and_then(Json::as_str) {
            Some(d) => format!("hh-ps0-stage/1 disposition {d}"),
            None => "no passed hh-ps0-stage/1 record".to_string(),
        };
        return Err(CorpusRefusal::Ps0NotPassed { detail });
    }
    let mut saw = false;
    for rec in pp0_evidence {
        let rm = match rec.as_obj() {
            Some(rm) if rm.get("schema").and_then(Json::as_str) == Some(PP0_AG_SCHEMA) => rm,
            _ => continue,
        };
        if rm.get("harness").and_then(Json::as_str) != Some(MINI_SWE_AGENT) {
            continue;
        }
        saw = true;
        let trial = rm
            .get("trial_ref")
            .and_then(Json::as_str)
            .unwrap_or("?")
            .to_string();
        for g in CORPUS_GATES {
            let result = rm
                .get("checks")
                .and_then(Json::as_obj)
                .and_then(|c| c.get(g))
                .and_then(Json::as_str)
                .unwrap_or("absent");
            if result != "pass" {
                return Err(CorpusRefusal::Pp0AgFailed {
                    trial,
                    gate: g.to_string(),
                    result: result.to_string(),
                });
            }
        }
    }
    if !saw {
        return Err(CorpusRefusal::Pp0EvidenceAbsent);
    }
    Ok(())
}
