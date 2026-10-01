//! Deterministic-oracle boundary (spec §5h.3/§5h.3.3; AC-R-2.9.2-11; S3.3).
//!
//! Every Stage-3 oracle call — whether in-process or through the
//! `hh-eval-oracle` subprocess — goes through the same [`run_oracle`]
//! dispatch over a canonical-JSON [`OracleRequest`]. A request carries *no*
//! opaque handles: the task contract, the candidate artifact bytes, and the
//! oracle declaration are all content. The subprocess binary is the same
//! code path behind a `stdin → stdout` process boundary, so AC-11's
//! "in-process and out-of-process oracles return equal verdicts" holds by
//! construction.
//!
//! Oracle *failures* are typed (`OracleFailure::…`), never a `0`/false
//! verdict — a crashed or timed-out oracle emits `oracle_failure`, not a
//! scored outcome (spec AC-12; §5h.3.3).

use crate::json_util::SchemaError;
use hh_ontology::eval::{LatticeValue, OracleClass, OracleDeclaration};
use hh_wire::Json;

/// `OracleRequest/1` — the out-of-process oracle contract (canonical JSON on
/// stdin; the verdict on stdout).
#[derive(Debug, Clone, PartialEq)]
pub struct OracleRequest {
    /// The oracle declaration (the class + judge/wall-time policy).
    pub oracle: OracleDeclaration,
    /// The criterion id / predicate being evaluated (the contract ref).
    pub criterion: String,
    /// The expected/reference output (`None` when the oracle is
    /// task-intrinsic, e.g. a task predicate over the transcript).
    pub expected: Option<Vec<u8>>,
    /// The candidate artifact bytes.
    pub actual: Vec<u8>,
    /// The transcript bytes a `task_predicate`/`trace_predicate` oracle reads
    /// (`None` for pure artifact oracles).
    pub transcript: Option<Vec<u8>>,
}

/// `OracleVerdict/1` — the oracle output.
#[derive(Debug, Clone, PartialEq)]
pub struct OracleVerdict {
    /// The oracle declaration id that produced the verdict.
    pub oracle_id: String,
    /// The oracle class.
    pub oracle_class: OracleClass,
    /// The lattice verdict (`P` is a valid verdict — inconclusive is not a
    /// failure).
    pub verdict: LatticeValue,
    /// The verdict's evidence payload (a small canonical-JSON detail object;
    /// the ledger's `verification.validator.verdict` records the digest).
    pub detail: Json,
}

/// The oracle failure taxonomy — a failed oracle is `oracle_failure`, never
/// a verdict (spec AC-12).
#[derive(Debug, Clone, PartialEq)]
pub enum OracleFailure {
    /// The request was malformed (schema violation).
    MalformedRequest(String),
    /// The oracle class needs a judge/model and no deterministic surrogate
    /// is declared (`judge`/`human`/`teacher_relative` can never run
    /// deterministically — the validator must refuse these for C0 rows).
    NondeterministicClass(OracleClass),
    /// The oracle needs `expected` and the request carried none.
    MissingReference,
    /// The oracle needs `transcript` and the request carried none.
    MissingTranscript,
}

impl std::fmt::Display for OracleFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OracleFailure::MalformedRequest(m) => write!(f, "MalformedRequest: {m}"),
            OracleFailure::NondeterministicClass(c) => {
                write!(f, "NondeterministicClass({c:?})")
            }
            OracleFailure::MissingReference => write!(f, "MissingReference"),
            OracleFailure::MissingTranscript => write!(f, "MissingTranscript"),
        }
    }
}

impl std::error::Error for OracleFailure {}

/// The deterministic oracle dispatch — the one code path both in-process
/// callers and `hh-eval-oracle` run.
pub fn run_oracle(req: &OracleRequest) -> Result<OracleVerdict, OracleFailure> {
    let class = req.oracle.class;
    // Nondeterministic classes cannot run at the deterministic boundary —
    // refuse, do not degrade to a coin flip (T-LCD-14).
    match class {
        OracleClass::Judge | OracleClass::Human | OracleClass::TeacherRelative => {
            return Err(OracleFailure::NondeterministicClass(class));
        }
        _ => {}
    }
    let verdict = match &class {
        OracleClass::Executable | OracleClass::OutputCheck | OracleClass::ReferenceRelative => {
            // Equality/exactness family: byte-exact against the reference
            // (three-valued for reference_relative — a missing reference is
            // inconclusive, never a failure).
            let expected = req
                .expected
                .as_deref()
                .ok_or(OracleFailure::MissingReference)?;
            if class == OracleClass::ReferenceRelative && expected.is_empty() {
                LatticeValue::I
            } else if &req.actual[..] == expected {
                LatticeValue::N
            } else {
                LatticeValue::C
            }
        }
        OracleClass::EndState => {
            // End-state: the candidate carries a canonical-JSON object with a
            // `goal_satisfied` member (the environment's goal predicate over
            // the end state — the environment computes it; the oracle reads
            // the declared bit).
            match serde_free_parse_bool(&req.actual[..], "goal_satisfied") {
                Some(true) => LatticeValue::N,
                Some(false) => LatticeValue::C,
                None => LatticeValue::I,
            }
        }
        OracleClass::TracePredicate | OracleClass::ProtocolCheck => {
            // Predicate oracles read the transcript. The §5c.2 Stage-3
            // trace predicates (`reacquisition`, `repeated_action`,
            // `recall_probe` — spelled bare or `trace_predicate:<name>`)
            // evaluate the deterministic [`crate::context_metrics`] fold
            // over the transcript's ledger rows (`LedgerFacts` decoded from
            // a `ledger_facts/1` document or a raw `{seq,class,payload}`
            // array). Any other criterion keeps the declared verdict-event
            // contract: `"<criterion>":true` in the transcript bytes.
            let t = req
                .transcript
                .as_deref()
                .ok_or(OracleFailure::MissingTranscript)?;
            match trace_predicate(&req.criterion, t) {
                Some(v) => v,
                None => match serde_free_parse_bool(t, &req.criterion) {
                    Some(true) => LatticeValue::N,
                    Some(false) => LatticeValue::C,
                    None => LatticeValue::I,
                },
            }
        }
        _ => LatticeValue::I,
    };
    Ok(OracleVerdict {
        oracle_id: req.oracle.oracle_id.clone(),
        oracle_class: class,
        verdict,
        detail: Json::obj([
            ("criterion", Json::str(&req.criterion)),
            ("deterministic", Json::Bool(true)),
        ]),
    })
}

/// The §5c.2 Stage-3 trace predicates — `reacquisition` (no forgotten item
/// was retrieved again), `repeated_action` (no `(capability, args)` pair
/// completed twice across a compaction boundary), `recall_probe` (every
/// probe assembly still contains a needed forgotten item). Returns `None`
/// when `criterion` is not one of the three (the generic verdict-event
/// contract then applies); `Some(I)` when the predicate's evidence class is
/// absent from the transcript (inapplicable — never a false verdict).
pub fn trace_predicate(criterion: &str, transcript: &[u8]) -> Option<LatticeValue> {
    let name = criterion
        .strip_prefix("trace_predicate:")
        .unwrap_or(criterion);
    if !matches!(name, "reacquisition" | "repeated_action" | "recall_probe") {
        return None;
    }
    let facts = decode_facts(transcript)?;
    let m = crate::context_metrics::fold(&facts);
    let verdict = match name {
        // N — the predicate holds (no damage); C — measured damage; I — the
        // transcript carries no compaction evidence at all.
        "reacquisition" => {
            if facts.compactions.iter().all(|c| !c.completed) {
                LatticeValue::I
            } else if m.reacquisition_count == 0 {
                LatticeValue::N
            } else {
                LatticeValue::C
            }
        }
        "repeated_action" => {
            if facts.compactions.iter().all(|c| !c.completed) {
                LatticeValue::I
            } else if m.repeated_action_count == 0 {
                LatticeValue::N
            } else {
                LatticeValue::C
            }
        }
        "recall_probe" => match m.recall_probe_hit_rate {
            None => LatticeValue::I,
            Some(r) if r >= 1_000_000 => LatticeValue::N,
            Some(_) => LatticeValue::C,
        },
        _ => unreachable!(),
    };
    Some(verdict)
}

/// Decode a transcript into `LedgerFacts` — accepts a `ledger_facts/1`
/// document or a raw array of `{"seq","class","payload"}` rows (both are
/// canonical JSON; a malformed transcript is no evidence — `None`, which
/// the caller treats as the generic-contract path, yielding `I`).
fn decode_facts(bytes: &[u8]) -> Option<crate::facts::LedgerFacts> {
    let text = std::str::from_utf8(bytes).ok()?;
    let j = hh_wire::json::parse(text).ok()?;
    // `ledger_facts/1` document.
    if j.get("schema").and_then(Json::as_str) == Some("ledger_facts/1") {
        return crate::facts::LedgerFacts::from_json(&j).ok();
    }
    // Raw row array: `[{"seq":…,"class":…,"payload":{…}}, …]` or
    // `{"events":[…]}`.
    let rows = match &j {
        Json::Arr(rs) => Some(rs.clone()),
        Json::Obj(_) => match j.get("events") {
            Some(Json::Arr(v)) => Some(v.clone()),
            _ => None,
        },
        _ => None,
    }?;
    let fact_rows: Vec<crate::facts::FactRow> = rows
        .iter()
        .filter_map(|r| {
            Some(crate::facts::FactRow {
                seq: r.get("seq").and_then(Json::as_int)? as u64,
                event_id: r.get("event_id").and_then(Json::as_str).map(str::to_string),
                class: r.get("class").and_then(Json::as_str)?.to_string(),
                payload: r.get("payload").cloned().unwrap_or(Json::Null),
            })
        })
        .collect();
    if fact_rows.is_empty() {
        return None;
    }
    Some(crate::facts::LedgerFacts::from_rows(&fact_rows))
}

/// A minimal canonical-JSON boolean member scan (the oracle needs no JSON
/// DOM — a deterministic byte scan over the canonical member `"k":true` —
/// enough for the Stage-3 oracle family; richer predicates are later-stage).
fn serde_free_parse_bool(bytes: &[u8], member: &str) -> Option<bool> {
    let needle = format!("\"{member}\":");
    let text = String::from_utf8_lossy(bytes);
    let idx = text.find(&needle)?;
    let rest = &text[idx + needle.len()..];
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

impl OracleRequest {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("schema".into(), Json::str("oracle_request/1"));
        m.insert("oracle".into(), self.oracle.to_json());
        m.insert("criterion".into(), Json::str(&self.criterion));
        if let Some(e) = &self.expected {
            m.insert("expected".into(), Json::str(hex_encode(e)));
        }
        m.insert("actual".into(), Json::str(hex_encode(&self.actual)));
        if let Some(t) = &self.transcript {
            m.insert("transcript".into(), Json::str(hex_encode(t)));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<OracleRequest, SchemaError> {
        const REC: &str = "OracleRequest/1";
        let m = crate::json_util::expect_obj(j, REC)?;
        let oracle = OracleDeclaration::from_json(
            m.get("oracle")
                .ok_or_else(|| SchemaError::v("oracle", "missing"))?,
        )
        .map_err(|e| SchemaError::v("oracle", format!("{e:?}")))?;
        let b64 = |key: &str| -> Result<Option<Vec<u8>>, SchemaError> {
            match m.get(key) {
                Some(Json::Str(s)) => Ok(Some(
                    hex_decode(s).ok_or_else(|| SchemaError::v(key, "bad hex"))?,
                )),
                Some(_) => Err(SchemaError::v(key, "expected string")),
                None => Ok(None),
            }
        };
        Ok(OracleRequest {
            oracle,
            criterion: m
                .get("criterion")
                .and_then(Json::as_str)
                .ok_or_else(|| SchemaError::v("criterion", "missing"))?
                .to_string(),
            expected: b64("expected")?,
            actual: b64("actual")?.unwrap_or_default(),
            transcript: b64("transcript")?,
        })
    }
}

impl OracleVerdict {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("oracle_verdict/1")),
            ("oracle_id", Json::str(&self.oracle_id)),
            ("oracle_class", Json::str(self.oracle_class.as_str())),
            ("verdict", Json::str(self.verdict.as_str())),
            ("detail", self.detail.clone()),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<OracleVerdict, SchemaError> {
        const REC: &str = "OracleVerdict/1";
        let m = crate::json_util::expect_obj(j, REC)?;
        let cls = m
            .get("oracle_class")
            .and_then(Json::as_str)
            .ok_or_else(|| SchemaError::v("oracle_class", "missing"))?;
        let v = m
            .get("verdict")
            .and_then(Json::as_str)
            .ok_or_else(|| SchemaError::v("verdict", "missing"))?;
        Ok(OracleVerdict {
            oracle_id: m
                .get("oracle_id")
                .and_then(Json::as_str)
                .ok_or_else(|| SchemaError::v("oracle_id", "missing"))?
                .to_string(),
            oracle_class: OracleClass::parse(cls)
                .ok_or_else(|| SchemaError::v("oracle_class", "unknown class"))?,
            verdict: LatticeValue::parse(v)
                .ok_or_else(|| SchemaError::v("verdict", "unknown lattice value"))?,
            detail: m.get("detail").cloned().unwrap_or(Json::Null),
        })
    }
}

/// A minimal lowercase-hex encoder (canonical JSON carries bytes as hex —
/// no external codec in the kernel ecosystem).
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// The `hex_encode` inverse — `None` on malformed input (never a lossy
/// decode).
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return None;
    }
    let nib = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks_exact(2) {
        out.push((nib(pair[0])? << 4) | nib(pair[1])?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_ontology::eval::{ChargedTo, VerdictType};

    fn decl(class: OracleClass) -> OracleDeclaration {
        let mut d = OracleDeclaration {
            oracle_id: "oracle.test".into(),
            class,
            deterministic: true,
            requires_observability: Default::default(),
            verdict_type: VerdictType::Bool,
            evidence_out: vec![],
            charged_to: ChargedTo::Subject,
            calibration_ref: None,
            provenance: hh_provenance::ProvenanceRecord::kernel("hh-eval/test", 0),
        };
        if class == OracleClass::Judge {
            d.deterministic = false;
            d.charged_to = ChargedTo::Instrument;
        }
        if class == OracleClass::ReferenceRelative {
            d.verdict_type = VerdictType::ThreeValued;
        }
        d
    }

    #[test]
    fn output_check_round_trips_through_canonical_json() {
        let req = OracleRequest {
            oracle: decl(OracleClass::OutputCheck),
            criterion: "exact".into(),
            expected: Some(b"42".to_vec()),
            actual: b"42".to_vec(),
            transcript: None,
        };
        let v1 = run_oracle(&req).unwrap();
        // Round-trip through canonical JSON (the subprocess contract).
        let req2 = OracleRequest::from_json(&req.to_json()).unwrap();
        let v2 = run_oracle(&req2).unwrap();
        assert_eq!(v1, v2);
        assert_eq!(v1.verdict, LatticeValue::N);
    }

    #[test]
    fn judge_is_refused_at_the_deterministic_boundary() {
        let req = OracleRequest {
            oracle: decl(OracleClass::Judge),
            criterion: "quality".into(),
            expected: None,
            actual: b"x".to_vec(),
            transcript: None,
        };
        assert_eq!(
            run_oracle(&req),
            Err(OracleFailure::NondeterministicClass(OracleClass::Judge))
        );
    }

    #[test]
    fn missing_reference_is_a_failure_not_a_verdict() {
        let req = OracleRequest {
            oracle: decl(OracleClass::ReferenceRelative),
            criterion: "match".into(),
            expected: None,
            actual: b"x".to_vec(),
            transcript: None,
        };
        assert_eq!(run_oracle(&req), Err(OracleFailure::MissingReference));
    }
}
