//! `l1_canon` — `hh-l1-canon/1`, the L1 canonicalization allowlist
//! (experiments spec §5 / L1 rules C01–C07; R6.10 PROXY-CORE-b;
//! REQ-XC-208's comparison layer for the conformance harnesses).
//!
//! The allowlist answers one question: *which differences between two
//! request/response documents are masked for L1 comparison?* Exactly
//! the declared rules below — anything else stays byte-exact, so an
//! unmasked difference fails the comparison.
//!
//! - **C01** canonical JSON reserialization: comparison happens on
//!   `PJson::to_provider_string` of the view (key order, whitespace).
//! - **C02** canonical numeric comparison *where admitted*: integers
//!   and integral floats compare equal (e.g. `40` ≡ `40.0`) — never on
//!   the never-normalized parameter members (`temperature`,
//!   `max_tokens`, effort paths, `model`).
//! - **C03** HTTP metadata/header comparison over the declared
//!   allowlist; secret and client-telemetry header names are dropped
//!   from comparison entirely.
//! - **C04** verified substitution of the four mini-swe-agent `uname`
//!   values — only when the document value matches a *recorded* cell
//!   fact carried into [`L1Canon`].
//! - **C05** downstream-only `stream`/`stream_options` dropping; the
//!   transport mode is recorded in [`L1View::transport_mode`].
//! - **C06** the declared optional assistant members with `null`
//!   treated as absent (provisional).
//! - **C07** Terminus session-ID fields dropped.
//!
//! **Never normalized** (an adjacent difference in any of these fails
//! the comparison): message text, tool-call IDs, re-sent reasoning,
//! `model`, `max_tokens`, effort values, `temperature`, tool schema
//! order/content, system/user prompt bytes, observation contents.
//!
//! The module is versioned: any allowlist change requires a new
//! [`L1_CANON_VERSION`] and a new, unused gate partition.

use std::collections::BTreeMap;

use crate::json::PJson;

/// The allowlist version — `hh-l1-canon/1` (C01–C07).
pub const L1_CANON_VERSION: &str = "hh-l1-canon/1";

/// The C06 provisional optional-assistant member set — `null` ≡ absent.
pub const C06_OPTIONAL_ASSISTANT: &[&str] = &["tool_calls", "function_call", "refusal", "audio"];

/// The C07 Terminus session-ID member set.
pub const C07_TERMINUS_SESSION: &[&str] = &[
    "session_id",
    "sessionId",
    "tmux_session",
    "terminal_session_id",
];

/// The C03 header allowlist — only these names compare; secret and
/// client-telemetry names (`*-key|*-token|*-secret`, `set-cookie`,
/// `user-agent`, `x-request-id`, `traceparent`, `x-telemetry-*`) are
/// dropped entirely.
pub const C03_HEADER_ALLOWLIST: &[&str] = &[
    "content-type",
    "accept",
    "retry-after",
    "anthropic-version",
    "anthropic-beta",
    "x-ratelimit-limit-requests",
    "x-ratelimit-remaining-requests",
    "x-ratelimit-limit-tokens",
    "x-ratelimit-remaining-tokens",
    "request-id",
];

/// The never-normalized parameter members — C02's numeric equality
/// does not apply at these paths (any difference there is unmasked).
const NEVER_NUMERIC: &[&str] = &[
    "temperature",
    "max_tokens",
    "max_completion_tokens",
    "top_p",
    "top_k",
    "presence_penalty",
    "frequency_penalty",
    "reasoning_effort",
    "output_config.effort",
    "reasoning.effort",
];

/// One masked difference a rule applied — `{rule, path}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Masked {
    /// The rule id (`c01`…`c07`).
    pub rule: &'static str,
    /// The JSON path the rule applied at.
    pub path: String,
}

/// The L1 canonicalization result — the canonical view plus the masked
/// differences the rules applied.
#[derive(Debug, Clone)]
pub struct L1View {
    /// The canonical document (`to_provider_string` for C01 equality).
    pub doc: PJson,
    /// The masked differences applied (`stream`/`stream_options`
    /// dropped members land here too).
    pub masked: Vec<Masked>,
    /// The transport mode the C05 rule recorded (`streaming` iff a
    /// `stream:true` member was dropped, else `buffered`).
    pub transport_mode: &'static str,
}

/// The comparison verdict — `equal` iff every difference was masked.
#[derive(Debug, Clone)]
pub struct L1Verdict {
    /// `true` iff the canonical views are identical.
    pub equal: bool,
    /// The masked differences applied across both views.
    pub masked: Vec<Masked>,
    /// The first unmasked difference path, when `equal == false`.
    pub first_unmasked: Option<String>,
}

/// The `hh-l1-canon/1` allowlist — constructed once per comparison
/// context with the recorded cell facts C04 needs.
#[derive(Debug, Clone, Default)]
pub struct L1Canon {
    /// The recorded cell facts for C04's verified substitution — the
    /// four mini-swe-agent `uname` values keyed by fact name; a
    /// document value substitutes only when it equals the recorded
    /// fact exactly.
    pub cell_facts: BTreeMap<String, String>,
}

impl L1Canon {
    /// `L1Canon::v1(cell_facts)` — the versioned allowlist.
    pub fn v1(cell_facts: &[(String, String)]) -> L1Canon {
        L1Canon {
            cell_facts: cell_facts.iter().cloned().collect(),
        }
    }

    /// `view(doc)` — apply C02/C04/C05/C06/C07 and return the canonical
    /// view (C01 is `doc.to_provider_string()`).
    pub fn view(&self, doc: &PJson) -> L1View {
        let mut masked = Vec::new();
        let mut transport = "buffered";
        let mut v = doc.clone();
        // C05 — downstream-only `stream`/`stream_options` dropping.
        if let PJson::Obj(m) = &mut v {
            if m.remove("stream_options").is_some() {
                masked.push(Masked {
                    rule: "c05",
                    path: "stream_options".into(),
                });
            }
            if let Some(s) = m.remove("stream") {
                masked.push(Masked {
                    rule: "c05",
                    path: "stream".into(),
                });
                if matches!(s, PJson::Bool(true)) {
                    transport = "streaming";
                }
            }
        }
        // C06 — provisional optional-assistant `null` ≡ absent.
        if let PJson::Obj(m) = &mut v {
            for name in C06_OPTIONAL_ASSISTANT {
                if m.get(*name) == Some(&PJson::Null) {
                    m.remove(*name);
                    masked.push(Masked {
                        rule: "c06",
                        path: (*name).into(),
                    });
                }
            }
        }
        // C07 — Terminus session fields dropped.
        if let PJson::Obj(m) = &mut v {
            for name in C07_TERMINUS_SESSION {
                if m.remove(*name).is_some() {
                    masked.push(Masked {
                        rule: "c07",
                        path: (*name).into(),
                    });
                }
            }
        }
        // C02 + C04 — walk the tree: admitted numeric equality and
        // verified uname substitution.
        let facts = self.cell_facts.clone();
        canon_walk(&mut v, "", &facts, &mut masked);
        L1View {
            doc: v,
            masked,
            transport_mode: transport,
        }
    }

    /// `header_view(headers)` — C03: the declared allowlist,
    /// value-order as received; everything else dropped unrecorded.
    pub fn header_view(&self, headers: &BTreeMap<String, String>) -> Vec<(String, String)> {
        headers
            .iter()
            .filter(|(k, _)| C03_HEADER_ALLOWLIST.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// `compare(a, b)` — `equal` iff the canonical views are identical
    /// (every difference masked); otherwise the first unmasked path.
    pub fn compare(&self, a: &PJson, b: &PJson) -> L1Verdict {
        let va = self.view(a);
        let vb = self.view(b);
        let equal = va.doc.to_provider_string() == vb.doc.to_provider_string();
        let mut masked = va.masked.clone();
        masked.extend(vb.masked.clone());
        L1Verdict {
            equal,
            masked,
            first_unmasked: if equal {
                None
            } else {
                first_diff(&va.doc, &vb.doc, "")
            },
        }
    }
}

/// Recursive walk for C02 (admitted numeric equality) and C04
/// (verified `uname` substitution). Never-normalized paths are
/// skipped — a difference there is unmasked by construction.
fn canon_walk(
    v: &mut PJson,
    path: &str,
    facts: &BTreeMap<String, String>,
    masked: &mut Vec<Masked>,
) {
    match v {
        PJson::Obj(m) => {
            for (k, child) in m.iter_mut() {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                canon_walk(child, &p, facts, masked);
            }
        }
        PJson::Arr(items) => {
            for (i, child) in items.iter_mut().enumerate() {
                canon_walk(child, &format!("{path}[{i}]"), facts, masked);
            }
        }
        PJson::Num(lex) => {
            if NEVER_NUMERIC.contains(&path) {
                return;
            }
            // C02 — an integral float lexeme canonicalizes to its
            // integer form where the member is not a
            // never-normalized param.
            if let Ok(n) = lex.parse::<f64>() {
                if n.fract() == 0.0 && n.is_finite() {
                    let as_int = n as i64;
                    if (as_int as f64) == n {
                        *v = PJson::Int(as_int);
                        masked.push(Masked {
                            rule: "c02",
                            path: path.to_string(),
                        });
                    }
                }
            }
        }
        PJson::Str(s) => {
            // C04 — substitute only a recorded cell fact, only at a
            // `uname`-bearing member.
            if (path.ends_with("uname") || path.ends_with("uname_output"))
                && facts.values().any(|f| f == s)
            {
                *s = "hh.cell_fact".to_string();
                masked.push(Masked {
                    rule: "c04",
                    path: path.to_string(),
                });
            }
        }
        _ => {}
    }
}

/// `first_diff_path(a, b)` — the first structural difference path
/// between two provider-bytes trees (the replay matcher's
/// `diverged{first_diff_path}` input; the L1 allowlist's own
/// `first_unmasked` leg). `None` when the trees are equal.
pub fn first_diff_path(a: &PJson, b: &PJson) -> Option<String> {
    first_diff(a, b, "")
}

/// The first structural difference path between two canonical views.
fn first_diff(a: &PJson, b: &PJson, path: &str) -> Option<String> {
    match (a, b) {
        (PJson::Obj(ma), PJson::Obj(mb)) => {
            for (k, va) in ma {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match mb.get(k) {
                    None => return Some(p),
                    Some(vb) => {
                        if let Some(d) = first_diff(va, vb, &p) {
                            return Some(d);
                        }
                    }
                }
            }
            for k in mb.keys() {
                if !ma.contains_key(k) {
                    return Some(if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    });
                }
            }
            None
        }
        (PJson::Arr(x), PJson::Arr(y)) => {
            for (i, (va, vb)) in x.iter().zip(y.iter()).enumerate() {
                if let Some(d) = first_diff(va, vb, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
            if x.len() != y.len() {
                Some(format!("{path}.len"))
            } else {
                None
            }
        }
        _ => {
            if a == b {
                None
            } else {
                Some(path.to_string())
            }
        }
    }
}
