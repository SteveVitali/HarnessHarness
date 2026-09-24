//! Strict-codec helpers — the closed-record rule (`BadMember` on any member the
//! shape does not declare, `MissingMember`/`TypeMismatch` on the rest), the same
//! pattern `hh-secrets`/`hh-containment` use (CC8/CC3).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::errors::CodecError;

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

/// An optional string member (`None` on absent or explicit null).
pub(crate) fn opt_str_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &'static str,
) -> Result<Option<&'a str>, CodecError> {
    match m.get(member) {
        Some(Json::Str(s)) => Ok(Some(s.as_str())),
        Some(Json::Null) | None => Ok(None),
        Some(_) => Err(CodecError::TypeMismatch {
            member: member.to_string(),
            expected: "string",
        }),
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

/// An optional integer member.
pub(crate) fn opt_int_at(
    m: &BTreeMap<String, Json>,
    member: &'static str,
) -> Result<Option<i64>, CodecError> {
    match m.get(member) {
        Some(Json::Int(i)) => Ok(Some(*i)),
        Some(Json::Null) | None => Ok(None),
        Some(_) => Err(CodecError::TypeMismatch {
            member: member.to_string(),
            expected: "integer",
        }),
    }
}

/// A required boolean member.
pub(crate) fn bool_at(
    m: &BTreeMap<String, Json>,
    member: &'static str,
    record: &'static str,
) -> Result<bool, CodecError> {
    match m.get(member) {
        Some(Json::Bool(b)) => Ok(*b),
        Some(_) => Err(CodecError::TypeMismatch {
            member: member.to_string(),
            expected: "boolean",
        }),
        None => Err(CodecError::MissingMember { member, record }),
    }
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
