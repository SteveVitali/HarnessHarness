//! `native` — the HH-native participant's runner half (manifest 302 /
//! R6.41; E2-T8 (b); REQ-XE-121/122-adjacent, REQ-XE-126, REQ-XD-102;
//! C-05's recorded rung; design/38 §6.3–§6.4).
//!
//! Three machines, each honest:
//!
//! - **`participant.hir{…}`** — the REQ-XE-126 member set every
//!   HH-native slot record carries (the sealed definition's identity
//!   plus the H-set digests R6.23 freezes at G-L1). The codec check
//!   lives beside `hh-experiment-slot/1`'s strict decode — the member
//!   list here is the one table (`HIR_REQUIRED`, `HIR_OPTIONAL`) both
//!   the builder and the checker read (CC1).
//! - **the rung-conditional topology** — the C-05 ladder recorded in
//!   `docs/build/reports/r6-39-c05-rung.md` reached **rung 1** (the
//!   kernel child over binding (b) with detach/reattach custody). A
//!   declared rung other than 1 refuses typed: rung 2 stays UNVERIFIED
//!   (the record says so); rung 3 — the in-process binding (a)
//!   exception — needs an admitted AR-04 amendment, and only an
//!   `ar04_amendment_ref` member admits it (X-01, never relaxed
//!   silently).
//! - **the REQ-XD-102 schedule gate** — the runner refuses to schedule
//!   a paid L2 pair (and the HH-native arm) without a matching
//!   `hh-l1-conformance/1` pass id for the **exact** H version: the
//!   match recomputes `l1_gate::l1_pass_id` over the arm's declared
//!   `hir` identity — a changed `definition_version_id` or
//!   `kernel_binary_sha256` invalidates it (REQ-XD-102's "any H change
//!   requires a full L1 rerun"), and the refusal is
//!   `withheld{l1_not_passed}` (R6.8's closed disposition set; the FP-E
//!   fallback's claim-level withholding shares the spelling).
//!
//! The model lane is REQ-XE-121's: the HH-native lane is *the same
//! projection* O's slots receive — `hh-env`'s
//! `cell_env_projection` binds `127.0.0.1:{model_port}` (the proxy's
//! token-gated loopback listener) with the same pins, and the proxy's
//! one FIFO `Gate` is the shared semaphore (one capture point, one
//! pin set — the lane never dials upstream). [`native_lane_check`] is
//! the lane-config check the acceptance battery runs: it refuses a
//! stamp whose base URLs are not loopback, whose token member is
//! empty, or whose `no_proxy` pin does not cover loopback.

use std::collections::BTreeMap;

use hharness_wire::json::Json;

use crate::disposition::{Disposition, WithheldReason};
use crate::json_ext::JsonExt;
use crate::l1_gate::{l1_pass_id, L1Identity};

// ── REQ-XE-126 — `participant.hir{…}` ────────────────────────────────────

/// The record kind's `kind` spelling for the HH-native participant
/// (design/38 §6.3's `kind: native`).
pub const NATIVE_KIND: &str = "native";

/// REQ-XE-126's required `participant.hir` members — the exact list the
/// requirement names (`l1_report_id` is the only optional member;
/// `declared_delta_report_id` stays out — REQ-XE-126's list governs,
/// design/38 Part 6.3's extra member is not admitted without an ADR).
pub const HIR_REQUIRED: &[&str] = &[
    "definition_version_id",
    "semantic_id",
    "profile_hash",
    "bundle_id",
    "derivation_key",
    "hh_commit",
    "kernel_binary_sha256",
    "representation_ledger_ref",
    "level_assignment",
    "override_spellings",
];

/// REQ-XE-126's optional `participant.hir` member.
pub const HIR_OPTIONAL: &[&str] = &["l1_report_id"];

/// `check_hir(hir)` — the `participant.hir` member-set check:
/// every required member present and string-typed (`level_assignment`
/// an object of `factor → level_id` strings, `override_spellings` a
/// string array), `l1_report_id` string-typed when present, no unknown
/// member. Returns the member's name on the first failure.
pub fn check_hir(hir: &Json) -> Result<(), String> {
    let m = hir
        .as_obj()
        .ok_or_else(|| "participant.hir must be an object".to_string())?;
    for k in m.keys() {
        if !HIR_REQUIRED.contains(&k.as_str()) && !HIR_OPTIONAL.contains(&k.as_str()) {
            return Err(format!("participant.hir: unknown member {k}"));
        }
    }
    for k in HIR_REQUIRED {
        let v = m
            .get(*k)
            .ok_or_else(|| format!("participant.hir.{k} required"))?;
        match *k {
            "level_assignment" => {
                let lm = v
                    .as_obj()
                    .ok_or_else(|| format!("participant.hir.{k} must be an object"))?;
                for (f, l) in lm {
                    if l.as_str().is_none() {
                        return Err(format!(
                            "participant.hir.level_assignment.{f} must be a string"
                        ));
                    }
                }
            }
            "override_spellings" => {
                let a = v
                    .as_arr()
                    .ok_or_else(|| format!("participant.hir.{k} must be an array"))?;
                if !a.iter().all(|s| s.as_str().is_some()) {
                    return Err(format!("participant.hir.{k} must be a string array"));
                }
            }
            _ => {
                if v.as_str().is_none() {
                    return Err(format!("participant.hir.{k} must be a string"));
                }
            }
        }
    }
    if let Some(l1) = m.get("l1_report_id") {
        if l1.as_str().is_none() {
            return Err("participant.hir.l1_report_id must be a string".to_string());
        }
    }
    Ok(())
}

/// `check_participant(participant)` — the record's `participant`
/// member check (REQ-XE-126): `kind: native` requires `hir`; a present
/// `hir` member validates against [`check_hir`]. Hosted/oracle
/// participants carry no `hir` member and are untouched.
pub fn check_participant(participant: &Json) -> Result<(), String> {
    let pm = participant
        .as_obj()
        .ok_or_else(|| "participant must be an object".to_string())?;
    let native = pm.get("kind").and_then(Json::as_str) == Some(NATIVE_KIND);
    match (native, pm.get("hir")) {
        (true, None) => Err("participant.kind native requires participant.hir".to_string()),
        (_, Some(h)) => check_hir(h),
        _ => Ok(()),
    }
}

/// `native_participants[arm_id]` — the declared participant record
/// for one HH-native arm: the rung-conditional topology plus the
/// REQ-XE-126 `hir` member stamped into the slot record's
/// `participant` (records-in — every member is declared, never
/// inferred).
#[derive(Debug, Clone, PartialEq)]
pub struct NativeParticipantDecl {
    /// The declared C-05 topology.
    pub topology: NativeTopology,
    /// The `participant.hir` member (validated against [`check_hir`];
    /// the JSON is carried verbatim into the slot record — no
    /// reformatting, the sealed declaration is the record).
    pub hir: Json,
}

impl NativeParticipantDecl {
    /// Strict decode — `{topology{…}, hir{…}}`, unknown members refuse;
    /// the `hir` member validates through [`check_hir`] at decode (a
    /// member-missing record never reaches the schedule check).
    pub fn from_json(j: &Json) -> Result<NativeParticipantDecl, TopologyError> {
        let m = j
            .as_obj()
            .ok_or_else(|| terr("native_participants[] must be an object"))?;
        for k in m.keys() {
            if !["topology", "hir"].contains(&k.as_str()) {
                return Err(terr(format!("native_participants[]: unknown member {k}")));
            }
        }
        let topology = NativeTopology::from_json(
            m.get("topology")
                .ok_or_else(|| terr("native_participants[].topology required"))?,
        )?;
        let hir = m
            .get("hir")
            .cloned()
            .ok_or_else(|| terr("native_participants[].hir required"))?;
        check_hir(&hir).map_err(terr)?;
        Ok(NativeParticipantDecl { topology, hir })
    }
}

// ── the C-05 rung (X-01 — the recorded rung only) ────────────────────────

/// The rung `docs/build/reports/r6-39-c05-rung.md` records: **1** —
/// the detach/reattach spike passes (the kernel child over binding
/// (b), sole writer during the agent phase, custody returned through
/// `env.close{detach}`/`env.attach`).
pub const RECORDED_RUNG: u8 = 1;

/// `topology{rung, ar04_amendment_ref?}` — the HH-native participant's
/// declared topology. Decoded strictly; [`NativeTopology::check`] is
/// the rung-conditional gate.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeTopology {
    /// The declared C-05 rung.
    pub rung: u8,
    /// The admitted AR-04 amendment ref (rung 3 only).
    pub ar04_amendment_ref: Option<String>,
}

/// A topology refusal — typed, never a silent downgrade.
#[derive(Debug, Clone, PartialEq)]
pub struct TopologyError {
    /// What refused.
    pub detail: String,
}

impl std::fmt::Display for TopologyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "hh-native topology: {}", self.detail)
    }
}
impl std::error::Error for TopologyError {}

fn terr(detail: impl Into<String>) -> TopologyError {
    TopologyError {
        detail: detail.into(),
    }
}

impl NativeTopology {
    /// Strict decode — `rung` a small integer; `ar04_amendment_ref`
    /// optional; unknown members refuse.
    pub fn from_json(j: &Json) -> Result<NativeTopology, TopologyError> {
        let m = j
            .as_obj()
            .ok_or_else(|| terr("topology must be an object"))?;
        for k in m.keys() {
            if !["rung", "ar04_amendment_ref"].contains(&k.as_str()) {
                return Err(terr(format!("topology: unknown member {k}")));
            }
        }
        let rung = m
            .get("rung")
            .and_then(Json::as_int)
            .ok_or_else(|| terr("topology.rung required"))?;
        if !(0..=3).contains(&rung) {
            return Err(terr(format!("topology.rung {rung} outside 0..=3")));
        }
        Ok(NativeTopology {
            rung: rung as u8,
            ar04_amendment_ref: m
                .get("ar04_amendment_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
        })
    }

    /// The rung-conditional gate (AC-5): the recorded rung admits the
    /// binding-(b) child path; rung 2 stays UNVERIFIED (the record says
    /// so — left unclaimed, not pretended); rung 3 needs an admitted
    /// AR-04 amendment — `ar04_amendment_ref` present and non-empty —
    /// else refuses typed; rung 0 is not a C-05 rung.
    pub fn check(&self) -> Result<(), TopologyError> {
        match self.rung {
            1 => Ok(()),
            2 => Err(terr(
                "rung 2 (child custody through seal+verify) is UNVERIFIED — the recorded C-05 rung is 1",
            )),
            3 => match &self.ar04_amendment_ref {
                Some(r) if !r.is_empty() => Ok(()),
                _ => Err(terr(
                    "rung 3 (in-process binding (a)) requires an admitted AR-04 amendment — none declared",
                )),
            },
            _ => Err(terr("rung 0 is not a C-05 rung")),
        }
    }

    /// The canonical member stamped into `participant.topology`.
    pub fn to_json(&self) -> Json {
        let mut m = vec![("rung", Json::Int(self.rung as i64))];
        if let Some(r) = &self.ar04_amendment_ref {
            m.push(("ar04_amendment_ref", Json::str(r)));
        }
        Json::obj(m)
    }
}

// ── REQ-XD-102 — the L1-conformance schedule gate ────────────────────────

/// The declared `hh-l1-conformance/1` gate facts the schedule reads:
/// the recorded pass id plus the identity triple it binds and the two
/// digests the id's preimage carries (R6.28's `l1_pass_id` inputs —
/// the conformance record publishes exactly these members).
#[derive(Debug, Clone)]
pub struct L1GateDecl {
    /// `l1-pass:sha256:…` — the recorded pass id.
    pub pass_id: String,
    /// The H identity the pass binds.
    pub h_identity: L1Identity,
    /// `hh-l1-canon` version id the pass ran under.
    pub canon_version: String,
    /// The corpus digest the pass ran over.
    pub corpus_digest: String,
}

impl PartialEq for L1GateDecl {
    fn eq(&self, other: &Self) -> bool {
        self.pass_id == other.pass_id
            && self.canon_version == other.canon_version
            && self.corpus_digest == other.corpus_digest
            && self.h_identity.participant_ref == other.h_identity.participant_ref
            && self.h_identity.version == other.h_identity.version
            && self.h_identity.digest == other.h_identity.digest
    }
}

/// A gate decode/refusal error — typed.
#[derive(Debug, Clone, PartialEq)]
pub struct L1GateError {
    /// What refused.
    pub detail: String,
}
impl std::fmt::Display for L1GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "l1_gate: {}", self.detail)
    }
}
impl std::error::Error for L1GateError {}

fn gerr(detail: impl Into<String>) -> L1GateError {
    L1GateError {
        detail: detail.into(),
    }
}

impl L1GateDecl {
    /// Strict decode of the drive packet's `l1_gate` member:
    /// `{pass_id, h_identity{participant_ref, version, digest},
    /// canon_version, corpus_digest}`.
    pub fn from_json(j: &Json) -> Result<L1GateDecl, L1GateError> {
        let m = j.as_obj().ok_or_else(|| gerr("must be an object"))?;
        for k in m.keys() {
            if !["pass_id", "h_identity", "canon_version", "corpus_digest"].contains(&k.as_str()) {
                return Err(gerr(format!("unknown member {k}")));
            }
        }
        let need = |k: &str| -> Result<String, L1GateError> {
            m.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| gerr(format!("{k} required")))
        };
        let hi = m
            .get("h_identity")
            .and_then(JsonExt::as_obj)
            .ok_or_else(|| gerr("h_identity must be an object"))?;
        for k in hi.keys() {
            if !["participant_ref", "version", "digest"].contains(&k.as_str()) {
                return Err(gerr(format!("h_identity: unknown member {k}")));
            }
        }
        let hneed = |k: &str| -> Result<String, L1GateError> {
            hi.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| gerr(format!("h_identity.{k} required")))
        };
        Ok(L1GateDecl {
            pass_id: need("pass_id")?,
            h_identity: L1Identity {
                participant_ref: hneed("participant_ref")?,
                version: hneed("version")?,
                digest: hneed("digest")?,
            },
            canon_version: need("canon_version")?,
            corpus_digest: need("corpus_digest")?,
        })
    }
}

/// `h_identity_of(hir)` — the arm's H identity read off its declared
/// `participant.hir` (REQ-XE-126): `version := definition_version_id`,
/// `digest := kernel_binary_sha256`, `participant_ref := bundle_id`
/// (R6.23's carried-set coordinates are the identity legs the L1 gate
/// binds). A changed `definition_version_id` or
/// `kernel_binary_sha256` yields a different identity — the pass-id
/// match then fails, exactly REQ-XD-102's "any H change requires a
/// full L1 rerun".
pub fn h_identity_of(hir: &Json) -> Option<L1Identity> {
    let s = |k: &str| hir.get(k).and_then(Json::as_str).map(str::to_string);
    Some(L1Identity {
        participant_ref: s("bundle_id")?,
        version: s("definition_version_id")?,
        digest: s("kernel_binary_sha256")?,
    })
}

/// `l1_gate_match(gate, hir)` — the REQ-XD-102 schedule predicate:
/// the declared gate's recorded H identity equals the arm's, **and**
/// the pass id recomputes to `l1_pass_id(h, canon, corpus)`. Both legs
/// must hold — a pass id minted for a different H version refuses
/// (a different `definition_version_id` or `kernel_binary_sha256`
/// changes the preimage).
pub fn l1_gate_match(gate: &L1GateDecl, hir: &Json) -> bool {
    let Some(h) = h_identity_of(hir) else {
        return false;
    };
    h.participant_ref == gate.h_identity.participant_ref
        && h.version == gate.h_identity.version
        && h.digest == gate.h_identity.digest
        && l1_pass_id(&h, &gate.canon_version, &gate.corpus_digest) == gate.pass_id
}

/// The slot-level refusal the gate's failure stamps —
/// `withheld{l1_not_passed}` (R6.8's closed set; REQ-XD-102/REQ-XH-114).
pub fn l1_not_passed_disposition() -> Disposition {
    Disposition::Withheld {
        reason: WithheldReason::L1NotPassed,
    }
}

// ── REQ-XE-121 — the loopback lane check ─────────────────────────────────

/// `native_lane_check(env)` — the lane-config check (AC-2): the
/// HH-native slot's model lane binds the proxy's token-gated loopback
/// listener with O's pins. `env` is the `cell_env_projection` stamp
/// (the one home — `hh_env::egress_allowlist`; the same function O's
/// `slot_open` stamp writes, so the pins are identical by
/// construction). The check refuses when:
/// - `OPENAI_BASE_URL`/`ANTHROPIC_BASE_URL` are absent or not
///   `http://127.0.0.1:<port>` spellings (the loopback listener —
///   never an upstream host);
/// - the base ports disagree (one listener);
/// - `OPENAI_API_KEY`/`ANTHROPIC_API_KEY` are absent or empty (the
///   per-slot token — X-05; the provider key never enters);
/// - `no_proxy` does not cover `127.0.0.1` **and** `localhost` (O's
///   mandatory pin — without it harness clients proxy the model route
///   through the registry relay).
/// The shared semaphore is the proxy process's own FIFO `Gate`
/// (`proxy::core_::Gate`) — a single queue both arms' calls serialize
/// through; the check's caller asserts the one-listener fact.
pub fn native_lane_check(env: &BTreeMap<String, String>) -> Result<u16, String> {
    let port_of = |k: &str| -> Result<u16, String> {
        let v = env.get(k).ok_or_else(|| format!("lane env {k} absent"))?;
        let port = v
            .strip_prefix("http://127.0.0.1:")
            .and_then(|rest| rest.split('/').next())
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or_else(|| format!("lane env {k} is not an http://127.0.0.1:<port> binding"))?;
        Ok(port)
    };
    let openai = port_of("OPENAI_BASE_URL")?;
    let anthropic = port_of("ANTHROPIC_BASE_URL")?;
    if openai != anthropic {
        return Err("the two base URLs name different loopback ports — not one listener".into());
    }
    for k in ["OPENAI_API_KEY", "ANTHROPIC_API_KEY"] {
        match env.get(k) {
            Some(t) if !t.is_empty() => {}
            _ => {
                return Err(format!(
                    "lane env {k} absent — the per-slot token is required"
                ))
            }
        }
    }
    match env.get("no_proxy") {
        Some(np) if np.contains("127.0.0.1") && np.contains("localhost") => {}
        _ => return Err("lane env no_proxy does not cover loopback — O's pin".into()),
    }
    Ok(openai)
}
