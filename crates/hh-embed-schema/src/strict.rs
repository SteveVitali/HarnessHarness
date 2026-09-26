//! Strict decoding helpers (I2 — `UnknownField{path}` on every unknown
//! non-`ext` member). Each `from_json` consumes its declared fields through
//! a [`StrictObj`]; `finish()` refuses any member the type never declared,
//! except the registered `ext`/`extensions` slots (ADR-0176 D3: preserved
//! byte-for-byte, never deciding).

use crate::errors::EmbedError;
use hh_wire::json::Json;
use std::collections::BTreeSet;

/// A decoding cursor over one JSON object. `take*` marks a declared member
/// consumed; `finish` fails on anything left over that is not a registered
/// extension slot.
pub struct StrictObj<'a> {
    path: String,
    obj: &'a std::collections::BTreeMap<String, Json>,
    consumed: BTreeSet<String>,
}

/// Member names that are registered extension slots — their *contents* are
/// free-form (prefixed ids inside `extensions`, dialect payloads inside
/// `ext`), but the slot itself must be declared on the type.
pub fn is_ext_slot(key: &str) -> bool {
    key == "ext" || key == "extensions"
}

impl<'a> StrictObj<'a> {
    /// Open a cursor over `v` (must be an object) at `path` for error paths.
    pub fn new(v: &'a Json, path: &str) -> Result<StrictObj<'a>, EmbedError> {
        match v {
            Json::Obj(m) => Ok(StrictObj {
                path: path.to_string(),
                obj: m,
                consumed: BTreeSet::new(),
            }),
            _ => Err(EmbedError::SchemaViolation {
                path: path.to_string(),
                code: "expected_object".to_string(),
            }),
        }
    }

    /// Consume a member (declared or not — used for `ext`/`extensions`).
    pub fn take(&mut self, key: &str) -> Option<&'a Json> {
        self.consumed.insert(key.to_string());
        self.obj.get(key)
    }

    /// Consume and require a member.
    pub fn req(&mut self, key: &str) -> Result<&'a Json, EmbedError> {
        self.take(key).ok_or_else(|| EmbedError::SchemaViolation {
            path: format!("{}/{}", self.path, key),
            code: "missing".to_string(),
        })
    }

    /// Consume a member, decoding it as a string.
    pub fn req_str(&mut self, key: &str) -> Result<String, EmbedError> {
        let path = format!("{}/{}", self.path, key);
        self.req(key)?
            .as_str()
            .map(|s| s.to_string())
            .ok_or(EmbedError::SchemaViolation {
                path,
                code: "expected_string".to_string(),
            })
    }

    /// Consume an optional string member.
    pub fn opt_str(&mut self, key: &str) -> Result<Option<String>, EmbedError> {
        let path = format!("{}/{}", self.path, key);
        match self.take(key) {
            None | Some(Json::Null) => Ok(None),
            Some(v) => v
                .as_str()
                .map(|s| Some(s.to_string()))
                .ok_or(EmbedError::SchemaViolation {
                    path,
                    code: "expected_string".to_string(),
                }),
        }
    }

    /// Consume and require an integer member.
    pub fn req_int(&mut self, key: &str) -> Result<i64, EmbedError> {
        let path = format!("{}/{}", self.path, key);
        self.req(key)?.as_int().ok_or(EmbedError::SchemaViolation {
            path,
            code: "expected_integer".to_string(),
        })
    }

    /// Consume an optional integer member.
    pub fn opt_int(&mut self, key: &str) -> Result<Option<i64>, EmbedError> {
        let path = format!("{}/{}", self.path, key);
        match self.take(key) {
            None | Some(Json::Null) => Ok(None),
            Some(v) => v.as_int().map(Some).ok_or(EmbedError::SchemaViolation {
                path,
                code: "expected_integer".to_string(),
            }),
        }
    }

    /// Consume an optional bool member (absent ⇒ `None`, never `unknown` —
    /// the `HostCapabilities` rule: absent means `false`, decided by the
    /// caller's `unwrap_or(false)`).
    pub fn opt_bool(&mut self, key: &str) -> Result<Option<bool>, EmbedError> {
        let path = format!("{}/{}", self.path, key);
        match self.take(key) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(EmbedError::SchemaViolation {
                path,
                code: "expected_bool".to_string(),
            }),
        }
    }

    /// Consume an optional array member.
    pub fn opt_arr(&mut self, key: &str) -> Result<Option<&'a Vec<Json>>, EmbedError> {
        let path = format!("{}/{}", self.path, key);
        match self.take(key) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::Arr(a)) => Ok(Some(a)),
            Some(_) => Err(EmbedError::SchemaViolation {
                path,
                code: "expected_array".to_string(),
            }),
        }
    }

    /// Consume a required array member.
    pub fn req_arr(&mut self, key: &str) -> Result<&'a Vec<Json>, EmbedError> {
        let path = format!("{}/{}", self.path, key);
        match self.req(key)? {
            Json::Arr(a) => Ok(a),
            _ => Err(EmbedError::SchemaViolation {
                path,
                code: "expected_array".to_string(),
            }),
        }
    }

    /// Consume an optional object member.
    pub fn opt_obj(
        &mut self,
        key: &str,
    ) -> Result<Option<&'a std::collections::BTreeMap<String, Json>>, EmbedError> {
        let path = format!("{}/{}", self.path, key);
        match self.take(key) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::Obj(m)) => Ok(Some(m)),
            Some(_) => Err(EmbedError::SchemaViolation {
                path,
                code: "expected_object".to_string(),
            }),
        }
    }

    /// Refuse any unconsumed member that is not a registered extension
    /// slot — `UnknownField{path}` (I2).
    pub fn finish(self) -> Result<(), EmbedError> {
        for k in self.obj.keys() {
            if !self.consumed.contains(k) && !is_ext_slot(k) {
                return Err(EmbedError::UnknownField {
                    path: format!("{}/{}", self.path, k),
                });
            }
        }
        Ok(())
    }
}

/// Decode a closed string sum (`kind ∈ {…}`) — an unlisted spelling is a
/// `SchemaViolation{code: "unknown_variant"}` (CC8: closed sums refuse
/// unknowns, never coerce).
pub fn closed_str(v: &Json, allowed: &[&'static str], path: &str) -> Result<String, EmbedError> {
    match v.as_str() {
        Some(s) if allowed.contains(&s) => Ok(s.to_string()),
        Some(_) => Err(EmbedError::SchemaViolation {
            path: path.to_string(),
            code: "unknown_variant".to_string(),
        }),
        None => Err(EmbedError::SchemaViolation {
            path: path.to_string(),
            code: "expected_string".to_string(),
        }),
    }
}
