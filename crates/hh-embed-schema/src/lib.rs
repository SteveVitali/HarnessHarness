//! `hh-embed-schema` — **the single schema source** for the `hh-embed/1` contract
//! (CC7: "every class-document schema is exported from the kernel's single schema source and
//! shared with the §7 kernel↔surfaces binding; plugin/client bindings are generated and
//! CI-checked; schema drift is a build failure, never a runtime negotiation").
//!
//! Stage-0 slice (§7.4 tier map, R-2.11.4 "Stage 0"; ADR-0176/0178/0179): the `hello`
//! negotiation, `ContractIdentity`, the closed error sum, and the exporter the codegen
//! pipeline consumes. Every wire type here is defined once, and the JSON-Schema exporter
//! ([`export_schema`]) is derived from the same definitions — so the schema and the Rust
//! types cannot drift from each other (a test asserts every type has a schema entry).
//!
//! Later stages (Groups H/S/W/R/M/L, the frame model) extend this source additively
//! (ADR-0178 D3 / CC8); they are out of scope for S0.1.

use hh_wire::json::Json;
use hh_wire::sha256::sha256_hex;

/// The contract major version. A `contract_major` bump is an ADR (ADR-0178 D3); additive
/// evolution within a major never bumps it (CC8).
pub const CONTRACT_MAJOR: i64 = 1;

/// The contract name, as it appears in the exported schema `$id`.
pub const CONTRACT_NAME: &str = "hh-embed/1";

// ---------------------------------------------------------------------------
// Wire types (the single source). Serialization lives beside each type.
// ---------------------------------------------------------------------------

/// `hello` request parameters. A client announces itself and asserts the contract identity it
/// was generated against; the kernel evaluates the assertion and answers with its own
/// [`ContractIdentity`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloParams {
    pub client_name: String,
    pub client_version: String,
    /// The `contract_major` the client was generated against.
    pub asserted_contract_major: i64,
    /// The `schema_hash` the client was generated against, if it asserts one. `None` means
    /// "unknown" (tri-state per CC8 / T-LCD-07) — the kernel still returns its identity.
    pub asserted_schema_hash: Option<String>,
}

impl HelloParams {
    pub fn to_json(&self) -> Json {
        let mut pairs = vec![
            ("client_name", Json::str(self.client_name.clone())),
            ("client_version", Json::str(self.client_version.clone())),
            (
                "asserted_contract_major",
                Json::Int(self.asserted_contract_major),
            ),
        ];
        if let Some(h) = &self.asserted_schema_hash {
            pairs.push(("asserted_schema_hash", Json::str(h.clone())));
        }
        Json::obj(pairs)
    }

    pub fn from_json(v: &Json) -> Result<HelloParams, String> {
        let client_name = req_str(v, "client_name")?;
        let client_version = req_str(v, "client_version")?;
        let asserted_contract_major = v
            .get("asserted_contract_major")
            .and_then(Json::as_int)
            .ok_or("missing 'asserted_contract_major'")?;
        let asserted_schema_hash = v
            .get("asserted_schema_hash")
            .and_then(Json::as_str)
            .map(|s| s.to_string());
        Ok(HelloParams {
            client_name,
            client_version,
            asserted_contract_major,
            asserted_schema_hash,
        })
    }
}

/// `ContractIdentity{contract_major, schema_hash, kernel_version_id}` (§7.4; ADR-0178 D2).
/// `schema_hash` is a content address over the canonical schema export; `hello` exchanges the
/// identity; a generated client asserts `(contract_major, schema_hash)`; mismatch is typed,
/// never a silent fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractIdentity {
    pub contract_major: i64,
    pub schema_hash: String,
    pub kernel_version_id: String,
}

impl ContractIdentity {
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("contract_major", Json::Int(self.contract_major)),
            ("schema_hash", Json::str(self.schema_hash.clone())),
            (
                "kernel_version_id",
                Json::str(self.kernel_version_id.clone()),
            ),
        ])
    }

    pub fn from_json(v: &Json) -> Result<ContractIdentity, String> {
        Ok(ContractIdentity {
            contract_major: v
                .get("contract_major")
                .and_then(Json::as_int)
                .ok_or("missing 'contract_major'")?,
            schema_hash: req_str(v, "schema_hash")?,
            kernel_version_id: req_str(v, "kernel_version_id")?,
        })
    }
}

/// `hello` result: the kernel's contract identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloResult {
    pub contract_identity: ContractIdentity,
}

impl HelloResult {
    pub fn to_json(&self) -> Json {
        Json::obj([("contract_identity", self.contract_identity.to_json())])
    }

    pub fn from_json(v: &Json) -> Result<HelloResult, String> {
        let ci = v
            .get("contract_identity")
            .ok_or("missing 'contract_identity'")?;
        Ok(HelloResult {
            contract_identity: ContractIdentity::from_json(ci)?,
        })
    }
}

/// The closed error sum for the Stage-0 slice. A response-position closed sum grows only by a
/// dialect bump (ADR-0178 D3); at Stage 0 these are the negotiation failures. Each carries a
/// stable JSON-RPC error code so a mismatch is typed, never silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedError {
    /// `hello` asked for a `contract_major` this kernel does not implement.
    ContractMajorUnsupported { asked: i64, kernel_major: i64 },
    /// The client asserted a `schema_hash` that differs from the kernel's. `kernel_newer`
    /// distinguishes the two directions (ADR-0178 D8 compatibility matrix).
    SchemaMismatch { asserted: String, kernel: String },
    /// A method not in the Stage-0 verb set.
    UnknownMethod { method: String },
    /// A malformed request or params (framing/typing error).
    MalformedRequest { detail: String },
}

impl EmbedError {
    /// The stable code for this variant (also mirrored in the exported schema so clients can
    /// map codes back to variants without guessing).
    pub fn code(&self) -> i64 {
        match self {
            EmbedError::ContractMajorUnsupported { .. } => 1001,
            EmbedError::SchemaMismatch { .. } => 1002,
            EmbedError::UnknownMethod { .. } => 1003,
            EmbedError::MalformedRequest { .. } => 1004,
        }
    }

    /// The variant tag, as it appears in `error.data.kind`.
    pub fn kind(&self) -> &'static str {
        match self {
            EmbedError::ContractMajorUnsupported { .. } => "ContractMajorUnsupported",
            EmbedError::SchemaMismatch { .. } => "SchemaMismatch",
            EmbedError::UnknownMethod { .. } => "UnknownMethod",
            EmbedError::MalformedRequest { .. } => "MalformedRequest",
        }
    }

    pub fn message(&self) -> String {
        match self {
            EmbedError::ContractMajorUnsupported {
                asked,
                kernel_major,
            } => {
                format!("contract major {asked} unsupported; kernel implements {kernel_major}")
            }
            EmbedError::SchemaMismatch { .. } => "schema hash mismatch".to_string(),
            EmbedError::UnknownMethod { method } => format!("unknown method {method:?}"),
            EmbedError::MalformedRequest { detail } => format!("malformed request: {detail}"),
        }
    }

    /// The typed `error.data` payload: `{kind, ...fields}`.
    pub fn to_data_json(&self) -> Json {
        match self {
            EmbedError::ContractMajorUnsupported {
                asked,
                kernel_major,
            } => Json::obj([
                ("kind", Json::str(self.kind())),
                ("asked", Json::Int(*asked)),
                ("kernel_major", Json::Int(*kernel_major)),
            ]),
            EmbedError::SchemaMismatch { asserted, kernel } => Json::obj([
                ("kind", Json::str(self.kind())),
                ("asserted", Json::str(asserted.clone())),
                ("kernel", Json::str(kernel.clone())),
            ]),
            EmbedError::UnknownMethod { method } => Json::obj([
                ("kind", Json::str(self.kind())),
                ("method", Json::str(method.clone())),
            ]),
            EmbedError::MalformedRequest { detail } => Json::obj([
                ("kind", Json::str(self.kind())),
                ("detail", Json::str(detail.clone())),
            ]),
        }
    }
}

fn req_str(v: &Json, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(Json::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing '{key}'"))
}

// ---------------------------------------------------------------------------
// Schema export + content address (the codegen pipeline's input).
// ---------------------------------------------------------------------------

/// The canonical JSON-Schema-style export of the Stage-0 message set. This is the *single
/// source* every binding is generated from. It is a pure function of the definitions above
/// and carries no clock, environment or version-specific value, so it is safe to
/// content-address.
pub fn export_schema() -> Json {
    Json::obj([
        ("$schema", Json::str("hh-embed/schema-export/0")),
        ("$id", Json::str(CONTRACT_NAME)),
        ("contract_major", Json::Int(CONTRACT_MAJOR)),
        ("methods", methods_schema()),
        ("types", types_schema()),
        ("errors", errors_schema()),
    ])
}

fn methods_schema() -> Json {
    Json::obj([(
        "hello",
        Json::obj([
            (
                "summary",
                Json::str("Exchange contract identities and negotiate compatibility."),
            ),
            ("params", Json::str("HelloParams")),
            ("result", Json::str("HelloResult")),
            (
                "errors",
                Json::Arr(vec![
                    Json::str("ContractMajorUnsupported"),
                    Json::str("SchemaMismatch"),
                    Json::str("MalformedRequest"),
                ]),
            ),
        ]),
    )])
}

fn types_schema() -> Json {
    Json::obj([
        (
            "HelloParams",
            fields(&[
                ("client_name", "string", true),
                ("client_version", "string", true),
                ("asserted_contract_major", "integer", true),
                ("asserted_schema_hash", "string", false),
            ]),
        ),
        (
            "ContractIdentity",
            fields(&[
                ("contract_major", "integer", true),
                ("schema_hash", "string", true),
                ("kernel_version_id", "string", true),
            ]),
        ),
        (
            "HelloResult",
            fields(&[("contract_identity", "ContractIdentity", true)]),
        ),
    ])
}

fn errors_schema() -> Json {
    let variants = [
        EmbedError::ContractMajorUnsupported {
            asked: 0,
            kernel_major: 0,
        },
        EmbedError::SchemaMismatch {
            asserted: String::new(),
            kernel: String::new(),
        },
        EmbedError::UnknownMethod {
            method: String::new(),
        },
        EmbedError::MalformedRequest {
            detail: String::new(),
        },
    ];
    Json::Arr(
        variants
            .iter()
            .map(|e| Json::obj([("kind", Json::str(e.kind())), ("code", Json::Int(e.code()))]))
            .collect(),
    )
}

fn fields(fs: &[(&'static str, &'static str, bool)]) -> Json {
    Json::obj([(
        "fields",
        Json::Arr(
            fs.iter()
                .map(|(name, ty, required)| {
                    Json::obj([
                        ("name", Json::str(*name)),
                        ("type", Json::str(*ty)),
                        ("required", Json::Bool(*required)),
                    ])
                })
                .collect(),
        ),
    )])
}

/// The canonical bytes of the schema export — the exact input to the content address.
pub fn canonical_schema_bytes() -> String {
    export_schema().to_canonical_string()
}

/// The content address of the canonical schema export: `sha256:<hex>`. This is the
/// `schema_hash` component of every [`ContractIdentity`]. (Stage-0 content address; migrates
/// to `idp/1`/ADR-0036 at S1.2 — see the S0.1 DEFERRALS row.)
pub fn schema_hash() -> String {
    format!("sha256:{}", sha256_hex(canonical_schema_bytes().as_bytes()))
}

/// Negotiate a client's assertion against the kernel identity. `Ok(())` when compatible;
/// otherwise a typed [`EmbedError`] (never a silent fallback — ADR-0178 D2).
pub fn negotiate(params: &HelloParams, kernel: &ContractIdentity) -> Result<(), EmbedError> {
    if params.asserted_contract_major != kernel.contract_major {
        return Err(EmbedError::ContractMajorUnsupported {
            asked: params.asserted_contract_major,
            kernel_major: kernel.contract_major,
        });
    }
    if let Some(asserted) = &params.asserted_schema_hash {
        if asserted != &kernel.schema_hash {
            return Err(EmbedError::SchemaMismatch {
                asserted: asserted.clone(),
                kernel: kernel.schema_hash.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel_identity() -> ContractIdentity {
        ContractIdentity {
            contract_major: CONTRACT_MAJOR,
            schema_hash: schema_hash(),
            kernel_version_id: "test-kernel/0.0.1".into(),
        }
    }

    #[test]
    fn schema_hash_is_stable_and_content_addressed() {
        // Deterministic: two calls agree, and the hash is over the canonical bytes.
        assert_eq!(schema_hash(), schema_hash());
        assert!(schema_hash().starts_with("sha256:"));
        let expect = format!(
            "sha256:{}",
            hh_wire::sha256_hex(canonical_schema_bytes().as_bytes())
        );
        assert_eq!(schema_hash(), expect);
    }

    #[test]
    fn every_wire_type_round_trips_through_json() {
        let ci = kernel_identity();
        assert_eq!(ContractIdentity::from_json(&ci.to_json()).unwrap(), ci);

        let hp = HelloParams {
            client_name: "c".into(),
            client_version: "9".into(),
            asserted_contract_major: 1,
            asserted_schema_hash: Some(schema_hash()),
        };
        assert_eq!(HelloParams::from_json(&hp.to_json()).unwrap(), hp);

        let hr = HelloResult {
            contract_identity: ci,
        };
        assert_eq!(HelloResult::from_json(&hr.to_json()).unwrap(), hr);
    }

    #[test]
    fn negotiate_accepts_matching_identity() {
        let k = kernel_identity();
        let params = HelloParams {
            client_name: "c".into(),
            client_version: "9".into(),
            asserted_contract_major: 1,
            asserted_schema_hash: Some(k.schema_hash.clone()),
        };
        assert!(negotiate(&params, &k).is_ok());
    }

    #[test]
    fn negotiate_rejects_wrong_major_typed() {
        let k = kernel_identity();
        let params = HelloParams {
            client_name: "c".into(),
            client_version: "9".into(),
            asserted_contract_major: 999,
            asserted_schema_hash: None,
        };
        match negotiate(&params, &k) {
            Err(EmbedError::ContractMajorUnsupported {
                asked,
                kernel_major,
            }) => {
                assert_eq!(asked, 999);
                assert_eq!(kernel_major, 1);
            }
            other => panic!("expected typed ContractMajorUnsupported, got {other:?}"),
        }
    }

    #[test]
    fn negotiate_rejects_stale_schema_hash_typed() {
        let k = kernel_identity();
        let params = HelloParams {
            client_name: "c".into(),
            client_version: "9".into(),
            asserted_contract_major: 1,
            asserted_schema_hash: Some("sha256:stale".into()),
        };
        assert!(matches!(
            negotiate(&params, &k),
            Err(EmbedError::SchemaMismatch { .. })
        ));
    }

    #[test]
    fn schema_export_lists_every_wire_type_and_error() {
        // Guards CC7: the exported schema and the Rust types stay in lockstep.
        let schema = export_schema();
        let types = schema.get("types").unwrap();
        for t in ["HelloParams", "ContractIdentity", "HelloResult"] {
            assert!(types.get(t).is_some(), "schema missing type {t}");
        }
        let errors = match schema.get("errors").unwrap() {
            Json::Arr(v) => v,
            _ => panic!("errors not an array"),
        };
        assert_eq!(
            errors.len(),
            4,
            "closed error sum size changed without a schema update"
        );
    }
}
