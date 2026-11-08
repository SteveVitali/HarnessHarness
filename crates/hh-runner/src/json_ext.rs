//! `Json` convenience accessors (the enum's own surface is
//! `as_str`/`as_int`/`get`; object/array/bool views live here so codecs
//! read uniformly).

use std::collections::BTreeMap;

use hharness_wire::json::Json;

/// Object/array/bool views over `Json`.
pub trait JsonExt {
    /// The `Obj` map view.
    fn as_obj(&self) -> Option<&BTreeMap<String, Json>>;
    /// The `Arr` view.
    fn as_arr(&self) -> Option<&Vec<Json>>;
    /// The `Bool` view.
    fn as_bool(&self) -> Option<bool>;
}

impl JsonExt for Json {
    fn as_obj(&self) -> Option<&BTreeMap<String, Json>> {
        match self {
            Json::Obj(m) => Some(m),
            _ => None,
        }
    }
    fn as_arr(&self) -> Option<&Vec<Json>> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
    fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
}
