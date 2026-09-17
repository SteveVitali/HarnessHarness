//! Strict-codec helpers — the closed-record rule (`BadMember` on any member the
//! shape does not declare, `MissingMember`/`TypeMismatch` on the rest), the same
//! pattern `hh-telemetry`/`hh-secrets`/`hh-containment` use (CC8/CC3).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::json::Json;

/// The strict-codec failure modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// A member the closed shape does not declare.
    BadMember {
        /// The unknown member.
        member: String,
        /// The record being decoded.
        record: &'static str,
    },
    /// A required member is absent.
    MissingMember {
        /// The absent member.
        member: &'static str,
        /// The record being decoded.
        record: &'static str,
    },
    /// A member carried the wrong JSON shape.
    TypeMismatch {
        /// The member.
        member: String,
        /// What was expected.
        expected: &'static str,
    },
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::BadMember { member, record } => {
                write!(f, "{record}: unknown member {member}")
            }
            CodecError::MissingMember { member, record } => {
                write!(f, "{record}: missing member {member}")
            }
            CodecError::TypeMismatch { member, expected } => {
                write!(f, "{member}: expected {expected}")
            }
        }
    }
}

impl std::error::Error for CodecError {}

/// The object map, or `TypeMismatch`.
pub(crate) fn expect_obj<'a>(
    j: &'a Json,
    record: &'static str,
) -> Result<&'a BTreeMap<String, Json>, CodecError> {
    match j {
        Json::Obj(m) => Ok(m),
        _ => Err(CodecError::TypeMismatch {
            member: record.to_string(),
            expected: "object",
        }),
    }
}

/// Refuse any member the closed shape does not declare.
pub(crate) fn reject_unknown(
    m: &BTreeMap<String, Json>,
    known: &[&str],
    record: &'static str,
) -> Result<(), CodecError> {
    for k in m.keys() {
        if !known.contains(&k.as_str()) {
            return Err(CodecError::BadMember {
                member: k.clone(),
                record,
            });
        }
    }
    Ok(())
}

/// A required string member.
pub(crate) fn str_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &'static str,
    record: &'static str,
) -> Result<&'a str, CodecError> {
    match m.get(member) {
        Some(Json::Str(s)) => Ok(s.as_str()),
        Some(_) => Err(CodecError::TypeMismatch {
            member: member.to_string(),
            expected: "string",
        }),
        None => Err(CodecError::MissingMember { member, record }),
    }
}

/// A required integer member.
pub(crate) fn int_at(
    m: &BTreeMap<String, Json>,
    member: &'static str,
    record: &'static str,
) -> Result<i64, CodecError> {
    match m.get(member) {
        Some(Json::Int(i)) => Ok(*i),
        Some(_) => Err(CodecError::TypeMismatch {
            member: member.to_string(),
            expected: "integer",
        }),
        None => Err(CodecError::MissingMember { member, record }),
    }
}

/// An optional bool member.
pub(crate) fn opt_bool_at(
    m: &BTreeMap<String, Json>,
    member: &'static str,
) -> Result<Option<bool>, CodecError> {
    match m.get(member) {
        Some(Json::Bool(b)) => Ok(Some(*b)),
        Some(Json::Null) | None => Ok(None),
        Some(_) => Err(CodecError::TypeMismatch {
            member: member.to_string(),
            expected: "bool",
        }),
    }
}

/// A required bool member.
pub(crate) fn bool_at(
    m: &BTreeMap<String, Json>,
    member: &'static str,
    record: &'static str,
) -> Result<bool, CodecError> {
    opt_bool_at(m, member)?.ok_or(CodecError::MissingMember { member, record })
}

/// A required array member.
pub(crate) fn arr_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &'static str,
    record: &'static str,
) -> Result<&'a [Json], CodecError> {
    match m.get(member) {
        Some(Json::Arr(a)) => Ok(a.as_slice()),
        Some(_) => Err(CodecError::TypeMismatch {
            member: member.to_string(),
            expected: "array",
        }),
        None => Err(CodecError::MissingMember { member, record }),
    }
}

/// A set of strings from a required array member.
pub(crate) fn str_set_at(
    m: &BTreeMap<String, Json>,
    member: &'static str,
    record: &'static str,
) -> Result<BTreeSet<String>, CodecError> {
    let mut out = BTreeSet::new();
    for j in arr_at(m, member, record)? {
        match j {
            Json::Str(s) => {
                out.insert(s.clone());
            }
            _ => {
                return Err(CodecError::TypeMismatch {
                    member: format!("{member}[]"),
                    expected: "string",
                })
            }
        }
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Cross-crate render helpers — `Label`/`ReaderSet`/`Validity`/`EventRef` carry
// no `to_json` in their owning crates; the payload writers here share these
// (one spelling per shape — CC7).
// ─────────────────────────────────────────────────────────────────────────────

/// `Label{authority, taint, readers}` → JSON.
pub(crate) fn label_json(l: &hh_provenance::label::Label) -> Json {
    Json::obj([
        ("authority", Json::str(l.authority.as_str())),
        (
            "taint",
            Json::Arr(l.taint.iter().map(|t| Json::str(t.as_string())).collect()),
        ),
        ("readers", readers_json(&l.readers)),
    ])
}

/// `ReaderSet` → JSON (`"public"` | `{restricted: [...]}`).
pub(crate) fn readers_json(r: &hh_provenance::authority::ReaderSet) -> Json {
    match r {
        hh_provenance::authority::ReaderSet::Public => Json::str("public"),
        hh_provenance::authority::ReaderSet::Restricted(s) => Json::obj([(
            "restricted",
            Json::Arr(s.iter().map(|p| Json::str(p.clone())).collect()),
        )]),
    }
}

/// `Validity{from, until|condition}` → JSON.
pub(crate) fn validity_json(v: &hh_hir::records::Validity) -> Json {
    let mut m = BTreeMap::new();
    m.insert("from".to_string(), Json::Int(v.from as i64));
    if let Some(u) = v.until {
        m.insert("until".to_string(), Json::Int(u as i64));
    }
    if let Some(c) = &v.condition {
        m.insert("condition".to_string(), Json::str(c.clone()));
    }
    Json::Obj(m)
}

/// `EventRef{run_id, event_id}` → `"run_id:event_id"` (the coordinate's
/// rendered form — manifest.rs's private `event_ref_json` renders the same
/// pair; this is the crate-local spelling).
pub(crate) fn event_ref_str(e: &hh_ledger::manifest::EventRef) -> String {
    format!("{}:{}", e.run_id, e.event_id)
}
