//! Canonical-JSON codec helpers shared by the `hh-eval` codecs.
//!
//! The helpers mirror `hh-ontology::eval`'s private `str_at`/`expect_obj`/
//! `reject_unknown` surface (they are `pub(crate)` there) — one shape, one
//! error idiom (`{member, detail}`), strict member sets (CC1/CC9).

use std::collections::BTreeMap;
use std::fmt::Display;

use hh_wire::Json;

/// One typed schema error shared by the Lab codecs — `{member, detail}` with
/// the record name embedded in `detail` (the same idiom as `EvalError::
/// SchemaViolation`; typed, never a warning).
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaError {
    /// The member that failed (or the record name for record-level checks).
    pub member: String,
    /// The failure detail.
    pub detail: String,
}

impl SchemaError {
    /// Construct a violation for `member`.
    pub fn v(member: impl Into<String>, detail: impl Into<String>) -> SchemaError {
        SchemaError {
            member: member.into(),
            detail: detail.into(),
        }
    }
}

impl Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.member, self.detail)
    }
}

impl std::error::Error for SchemaError {}

/// `expect_obj` — `j` must be a JSON object; return its member map.
pub fn expect_obj<'a>(j: &'a Json, rec: &str) -> Result<&'a BTreeMap<String, Json>, SchemaError> {
    match j {
        Json::Obj(m) => Ok(m),
        _ => Err(SchemaError::v(rec, format!("{rec} must be a JSON object"))),
    }
}

/// `reject_unknown` — every member key of `m` must be in `allowed`.
pub fn reject_unknown(
    m: &BTreeMap<String, Json>,
    allowed: &[&str],
    rec: &str,
) -> Result<(), SchemaError> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(SchemaError::v(
                k.clone(),
                format!("unknown member of {rec}"),
            ));
        }
    }
    Ok(())
}

/// `str_at` — a required string member.
pub fn str_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &str,
    rec: &str,
) -> Result<&'a str, SchemaError> {
    m.get(member)
        .and_then(Json::as_str)
        .ok_or_else(|| SchemaError::v(member, format!("{rec}.{member} must be a string")))
}

/// `opt_str_at` — an optional string member (`absent`/`null` → `None`).
pub fn opt_str_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &str,
) -> Result<Option<&'a str>, SchemaError> {
    match m.get(member) {
        None | Some(Json::Null) => Ok(None),
        Some(j) => j
            .as_str()
            .map(Some)
            .ok_or_else(|| SchemaError::v(member, "must be a string")),
    }
}

/// `int_at` — a required integer member.
pub fn int_at(m: &BTreeMap<String, Json>, member: &str, rec: &str) -> Result<i64, SchemaError> {
    m.get(member)
        .and_then(Json::as_int)
        .ok_or_else(|| SchemaError::v(member, format!("{rec}.{member} must be an int")))
}

/// `opt_int_at` — an optional integer member.
pub fn opt_int_at(m: &BTreeMap<String, Json>, member: &str) -> Result<Option<i64>, SchemaError> {
    match m.get(member) {
        None | Some(Json::Null) => Ok(None),
        Some(j) => j
            .as_int()
            .map(Some)
            .ok_or_else(|| SchemaError::v(member, "must be an int")),
    }
}

/// `bool_at` — a required bool member.
pub fn bool_at(m: &BTreeMap<String, Json>, member: &str, rec: &str) -> Result<bool, SchemaError> {
    match m.get(member) {
        Some(Json::Bool(b)) => Ok(*b),
        _ => Err(SchemaError::v(
            member,
            format!("{rec}.{member} must be a bool"),
        )),
    }
}

/// `opt_bool_at` — an optional bool member.
pub fn opt_bool_at(m: &BTreeMap<String, Json>, member: &str) -> Result<Option<bool>, SchemaError> {
    match m.get(member) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(SchemaError::v(member, "must be a bool")),
    }
}

/// `arr_at` — a required array member.
pub fn arr_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &str,
    rec: &str,
) -> Result<&'a Vec<Json>, SchemaError> {
    match m.get(member) {
        Some(Json::Arr(a)) => Ok(a),
        _ => Err(SchemaError::v(
            member,
            format!("{rec}.{member} must be an array"),
        )),
    }
}

/// `opt_arr_at` — an optional array member.
#[allow(dead_code)]
pub fn opt_arr_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &str,
) -> Result<Option<&'a Vec<Json>>, SchemaError> {
    match m.get(member) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Arr(a)) => Ok(Some(a)),
        Some(_) => Err(SchemaError::v(member, "must be an array")),
    }
}

/// `str_vec_at` — a required array-of-strings member.
pub fn str_vec_at(
    m: &BTreeMap<String, Json>,
    member: &str,
    rec: &str,
) -> Result<Vec<String>, SchemaError> {
    arr_at(m, member, rec)?
        .iter()
        .map(|j| {
            j.as_str()
                .map(str::to_string)
                .ok_or_else(|| SchemaError::v(member, format!("{rec}.{member}[] must be strings")))
        })
        .collect()
}

/// `enum_vec_at` — a required array member decoded through `parse`.
pub fn enum_vec_at<T>(
    m: &BTreeMap<String, Json>,
    member: &str,
    rec: &str,
    parse: impl Fn(&Json) -> Option<T>,
) -> Result<Vec<T>, SchemaError> {
    arr_at(m, member, rec)?
        .iter()
        .map(|j| {
            parse(j).ok_or_else(|| {
                SchemaError::v(member, format!("{rec}.{member}[] has an invalid element"))
            })
        })
        .collect()
}

/// `member_at` — a required member returned as `&Json`.
pub fn member_at<'a>(
    m: &'a BTreeMap<String, Json>,
    member: &str,
    rec: &str,
) -> Result<&'a Json, SchemaError> {
    m.get(member)
        .ok_or_else(|| SchemaError::v(member, format!("{rec}.{member} missing")))
}

/// `insert_opt` — insert `member → value` when `value` is `Some`.
pub fn insert_opt(m: &mut BTreeMap<String, Json>, member: &str, value: Option<Json>) {
    if let Some(v) = value {
        m.insert(member.to_string(), v);
    }
}
