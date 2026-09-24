//! The strict-codec helpers every `from_json` in this crate shares (CC4 —
//! unknown members are `BadMember`, never silently dropped). Small and private
//! to the crate; the *policy* (which members are registered) lives in each
//! record's own `from_json`.

use hh_wire::json::Json;

use crate::errors::CodecError;

/// `j` must be an object — returns the member map.
pub fn expect_obj<'a>(
    j: &'a Json,
    path: &str,
) -> Result<&'a std::collections::BTreeMap<String, Json>, CodecError> {
    match j {
        Json::Obj(m) => Ok(m),
        _ => Err(CodecError::TypeMismatch {
            path: path.to_string(),
        }),
    }
}

/// Reject every member key not in `allowed` — the closed-record rule.
pub fn reject_unknown(
    m: &std::collections::BTreeMap<String, Json>,
    allowed: &[&str],
    path: &str,
) -> Result<(), CodecError> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(CodecError::BadMember {
                path: format!("{path}.{k}"),
            });
        }
    }
    Ok(())
}

/// A mandatory string member.
pub fn str_at<'a>(
    m: &'a std::collections::BTreeMap<String, Json>,
    key: &str,
    path: &str,
) -> Result<&'a str, CodecError> {
    match m.get(key) {
        Some(Json::Str(s)) => Ok(s.as_str()),
        Some(_) => Err(CodecError::TypeMismatch {
            path: format!("{path}.{key}"),
        }),
        None => Err(CodecError::MissingMember {
            path: format!("{path}.{key}"),
        }),
    }
}

/// An optional string member (`absent` or `null` ⇒ `None`).
pub fn opt_str_at<'a>(
    m: &'a std::collections::BTreeMap<String, Json>,
    key: &str,
    path: &str,
) -> Result<Option<&'a str>, CodecError> {
    match m.get(key) {
        Some(Json::Str(s)) => Ok(Some(s.as_str())),
        Some(Json::Null) | None => Ok(None),
        Some(_) => Err(CodecError::TypeMismatch {
            path: format!("{path}.{key}"),
        }),
    }
}

/// A mandatory integer member.
pub fn int_at(
    m: &std::collections::BTreeMap<String, Json>,
    key: &str,
    path: &str,
) -> Result<i64, CodecError> {
    match m.get(key) {
        Some(Json::Int(i)) => Ok(*i),
        Some(_) => Err(CodecError::TypeMismatch {
            path: format!("{path}.{key}"),
        }),
        None => Err(CodecError::MissingMember {
            path: format!("{path}.{key}"),
        }),
    }
}

/// A mandatory bool member.
pub fn bool_at(
    m: &std::collections::BTreeMap<String, Json>,
    key: &str,
    path: &str,
) -> Result<bool, CodecError> {
    match m.get(key) {
        Some(Json::Bool(b)) => Ok(*b),
        Some(_) => Err(CodecError::TypeMismatch {
            path: format!("{path}.{key}"),
        }),
        None => Err(CodecError::MissingMember {
            path: format!("{path}.{key}"),
        }),
    }
}

/// A mandatory member of any kind.
pub fn member<'a>(
    m: &'a std::collections::BTreeMap<String, Json>,
    key: &str,
    path: &str,
) -> Result<&'a Json, CodecError> {
    m.get(key).ok_or_else(|| CodecError::MissingMember {
        path: format!("{path}.{key}"),
    })
}
