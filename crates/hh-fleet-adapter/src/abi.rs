//! The `work_source_adapter` class contract — op names + the canonical
//! wire codec. One scheme (CC1): every op output crosses as
//! `{result: "<canonical-json>"}` — contract documents legitimately
//! carry `label`/`provenance.authority` members the variant host's V1
//! inbound screen forbids as *envelope* members; inside the canonical
//! string they are the data they always were (the `memory_store` rule —
//! see `hh_context::memory_abi::wrap_output`).

use hh_wire::json::Json;

/// The class id the contract binds under (`plugin_abi/1` `class_contract`).
pub const WORK_SOURCE_CLASS: &str = "work_source_adapter";

/// The contract version this codec implements.
pub const WORK_SOURCE_CONTRACT: &str = "1.0";

/// The op set — `WorkSourceAdapter`'s seam, verbatim (S5.6).
pub const OPS: &[&str] = &[
    "capabilities",
    "occurrences",
    "suspended",
    "activate_run",
    "list",
    "get",
];

/// `wrap_output(doc)` — the `plugin_abi/1`-safe envelope: `{result:
/// "<canonical-json>"}` (the screen sees only the envelope).
pub fn wrap_output(doc: &Json) -> Json {
    Json::obj([("result", Json::str(doc.to_canonical_string()))])
}

/// `unwrap_output(envelope)` — the host side's strict unwrap: one
/// `result` member, canonical-JSON inside.
pub fn unwrap_output(env: &Json) -> Result<Json, AbiCodecError> {
    let Json::Obj(o) = env else {
        return Err(AbiCodecError::Malformed {
            detail: "output envelope must be an object".into(),
        });
    };
    for k in o.keys() {
        if k != "result" {
            return Err(AbiCodecError::Malformed {
                detail: format!("output envelope unknown member {k}"),
            });
        }
    }
    let Some(Json::Str(s)) = o.get("result") else {
        return Err(AbiCodecError::Malformed {
            detail: "output envelope requires result:string".into(),
        });
    };
    hh_wire::json::parse(s).map_err(|_| AbiCodecError::Malformed {
        detail: "result is not canonical JSON".into(),
    })
}

/// The codec's own failure (a malformed *envelope* — domain refusals
/// are data inside `outputs`, never this).
#[derive(Debug, Clone, PartialEq)]
pub enum AbiCodecError {
    /// The envelope or its members failed strict decode.
    Malformed {
        /// What failed.
        detail: String,
    },
}

/// `arg(inputs, n)` — the nth input or `Json::Null` (the ops take
/// positional document arguments like the memory-store contract).
pub fn arg(inputs: &[Json], n: usize) -> Json {
    inputs.get(n).cloned().unwrap_or(Json::Null)
}

/// `str_members(j, member)` — a `string[]` member decoded strictly.
pub fn str_members(j: &Json, member: &str) -> Result<Vec<String>, AbiCodecError> {
    match j.get(member) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Arr(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| AbiCodecError::Malformed {
                        detail: format!("{member} members must be strings"),
                    })
            })
            .collect(),
        _ => Err(AbiCodecError::Malformed {
            detail: format!("{member} must be an array"),
        }),
    }
}
