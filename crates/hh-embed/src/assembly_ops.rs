//! `lab.assembly.*` — the Group L assembly-service boundary (§6.1; S3.5;
//! records-in/records-out, CC5). Every verb delegates to
//! `hh_lab::assembly::service::AssemblyService` over the embed service's one
//! `RegistryStore`/`Stage1Catalog`; the boundary never re-implements a kernel
//! rule (V-1: failures are typed diagnostics inside the result records, not
//! boundary errors — `EmbedError` is reserved for malformed *params*).

use hh_embed_schema::errors::EmbedError;
use hh_lab::assembly::drift::drift_json;
use hh_lab::assembly::service::{
    diagnostics_json, AssembleMode, AssemblyService, BatchPoint, PublishSpec,
};
use hh_lab::assembly::source::AssemblySource;
use hh_provenance::ProvenanceRecord;
use hh_registry::records::RegistryRecord;
use hh_wire::json::Json;

use crate::service::EmbedService;

fn bad(path: &str, code: &str) -> EmbedError {
    EmbedError::SchemaViolation {
        path: path.to_string(),
        code: code.to_string(),
    }
}

fn req<'a>(j: &'a Json, k: &str) -> Result<&'a Json, EmbedError> {
    j.get(k)
        .ok_or_else(|| bad(&format!("/{k}"), "missing_field"))
}

fn req_str<'a>(j: &'a Json, k: &str) -> Result<&'a str, EmbedError> {
    req(j, k)?
        .as_str()
        .ok_or_else(|| bad(&format!("/{k}"), "type_mismatch"))
}

fn opt_str(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn source_of(params: &Json) -> Result<AssemblySource, EmbedError> {
    AssemblySource::from_json(req(params, "source")?, "/source").map_err(|e| bad("/source", &e))
}

/// The caller's `registrar` provenance (publish/adopt only — decode, never
/// trust: the store re-validates origin ⇒ authority).
fn registrar_of(params: &Json) -> Result<ProvenanceRecord, EmbedError> {
    ProvenanceRecord::from_json(req(params, "registrar")?)
        .map_err(|e| bad("/registrar", &format!("{e:?}")))
}

/// `snapshot` param — `"head"`/absent → `None` (a fresh cut through the
/// registry's own verb).
fn snapshot_of(params: &Json) -> Option<String> {
    match opt_str(params, "snapshot") {
        Some(s) if s == "head" => None,
        other => other,
    }
}

fn svc<'a>(s: &'a mut EmbedService, registrar: &ProvenanceRecord) -> AssemblyService<'a> {
    let now = s.store.now_ms();
    AssemblyService {
        registry: &mut s.registry,
        catalog: &s.catalog,
        kernel: s.kernel_prov.clone(),
        registrar: registrar.clone(),
        resolved_at: now,
    }
}

/// Fetch a sealed definition by `version_id` (registry record kind
/// `sealed_definition`).
fn sealed_of(
    s: &EmbedService,
    version_id: &str,
) -> Result<hh_hir::document::SealedDefinition, EmbedError> {
    let (_, rec) = s
        .registry
        .get(version_id)
        .ok_or_else(|| EmbedError::Refused {
            reason: format!("unknown version_id `{version_id}`"),
        })?;
    match rec {
        RegistryRecord::SealedDefinition(d) => Ok(d.clone()),
        other => Err(EmbedError::Refused {
            reason: format!(
                "`{version_id}` is a `{:?}`, not a `sealed_definition`",
                other.kind()
            ),
        }),
    }
}

impl EmbedService {
    /// `lab.assembly.assemble` — `{source, snapshot?, mode ∈ {plan, seal}}`.
    pub(crate) fn lab_assembly_assemble(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let source = source_of(params)?;
        let mode = match opt_str(params, "mode").as_deref().unwrap_or("plan") {
            "plan" => AssembleMode::Plan,
            "seal" => AssembleMode::Seal,
            other => return Err(bad("/mode", &format!("unknown mode {other}"))),
        };
        let snap = snapshot_of(params);
        let registrar = self.kernel_prov.clone();
        let r = svc(self, &registrar).assemble(&source, snap.as_deref(), mode);
        Ok(r.to_json())
    }

    /// `lab.assembly.plan` — `{source, snapshot?}` → `AssemblyPlan`.
    pub(crate) fn lab_assembly_plan(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let source = source_of(params)?;
        let snap = snapshot_of(params);
        let registrar = self.kernel_prov.clone();
        let plan = svc(self, &registrar).plan(&source, snap.as_deref());
        Ok(plan.to_json())
    }

    /// `lab.assembly.apply` — `{source, snapshot?, publish?, registrar}`.
    pub(crate) fn lab_assembly_apply(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let source = source_of(params)?;
        let snap = snapshot_of(params);
        let registrar = registrar_of(params)?;
        let publish = params
            .get("publish")
            .map(|p| -> Result<PublishSpec, EmbedError> {
                Ok(PublishSpec {
                    namespace: req_str(p, "namespace")?.to_string(),
                    name: req_str(p, "name")?.to_string(),
                    label: opt_str(p, "label"),
                    supersedes: opt_str(p, "supersedes"),
                })
            })
            .transpose()?;
        let r = svc(self, &registrar).apply(&source, snap.as_deref(), publish.as_ref());
        Ok(r.to_json())
    }

    /// `lab.assembly.validate_batch` — `{points[], snapshot?}`: each point is
    /// `{"source": …}` or `{"sealed": "<version_id>"}`.
    pub(crate) fn lab_assembly_validate_batch(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let pts = match req(params, "points")? {
            Json::Arr(v) => v.clone(),
            _ => return Err(bad("/points", "type_mismatch")),
        };
        let snap = snapshot_of(params);
        let registrar = self.kernel_prov.clone();
        let mut points = Vec::new();
        for (i, p) in pts.iter().enumerate() {
            if let Some(vid) = p.get("sealed").and_then(Json::as_str) {
                points.push(BatchPoint::Sealed(Box::new(sealed_of(self, vid)?)));
            } else if let Some(sj) = p.get("source") {
                points.push(BatchPoint::Source(Box::new(
                    AssemblySource::from_json(sj, &format!("/points/{i}/source"))
                        .map_err(|e| bad(&format!("/points/{i}/source"), &e))?,
                )));
            } else {
                return Err(bad(&format!("/points/{i}"), "missing_field"));
            }
        }
        let reports = svc(self, &registrar).validate_batch(&points, snap.as_deref());
        Ok(Json::Arr(
            reports
                .iter()
                .map(|(k, r)| {
                    Json::obj([
                        ("key", Json::str(k.clone())),
                        ("report", hh_assembly::diagnostics::report_json(r)),
                    ])
                })
                .collect(),
        ))
    }

    /// `lab.assembly.explain` — `{source, snapshot?}` or `{sealed: vid}`.
    pub(crate) fn lab_assembly_explain(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let registrar = self.kernel_prov.clone();
        if let Some(vid) = params.get("sealed").and_then(Json::as_str) {
            let sealed = sealed_of(self, vid)?;
            return Ok(svc(self, &registrar).explain_sealed(&sealed));
        }
        let source = source_of(params)?;
        let snap = snapshot_of(params);
        Ok(svc(self, &registrar).explain(&source, snap.as_deref()))
    }

    /// `lab.assembly.diff` — `{a: vid, b: vid}` → `AssemblyDiff` (T-2: sealed
    /// definitions only).
    pub(crate) fn lab_assembly_diff(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let a = sealed_of(self, req_str(params, "a")?)?;
        let b = sealed_of(self, req_str(params, "b")?)?;
        let registrar = self.kernel_prov.clone();
        let d = svc(self, &registrar).diff_sealed(&a, &b);
        Ok(hh_lab::assembly::diff_view::diff_json(&d))
    }

    /// `lab.assembly.identity` — `{sealed: vid}` → `{semantic_id, version_id}`
    /// (the definition's root coordinates — §3.3.6).
    pub(crate) fn lab_assembly_identity(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let s = sealed_of(self, req_str(params, "sealed")?)?;
        Ok(Json::obj([
            (
                "semantic_id",
                Json::str(s.definition_ref.semantic_id.clone()),
            ),
            ("version_id", Json::str(s.definition_ref.version_id.clone())),
        ]))
    }

    /// `lab.assembly.drift` — `{source, snapshot_old, snapshot_new}` → the
    /// `C-REF-6` drift report (`freeze` is the default — this call is a read).
    pub(crate) fn lab_assembly_drift(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let source = source_of(params)?;
        let old = req_str(params, "snapshot_old")?.to_string();
        let new = req_str(params, "snapshot_new")?.to_string();
        let registrar = self.kernel_prov.clone();
        let r = svc(self, &registrar).drift(&source, &old, &new);
        Ok(drift_json(&r))
    }

    /// `lab.assembly.adopt` — `{source, snapshot_old, snapshot_new,
    /// registrar}` → the adoption `HirDiff` (T-3; explicit, gated).
    pub(crate) fn lab_assembly_adopt(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let source = source_of(params)?;
        let old = req_str(params, "snapshot_old")?.to_string();
        let new = req_str(params, "snapshot_new")?.to_string();
        let registrar = registrar_of(params)?;
        match svc(self, &registrar).adopt(&source, &old, &new, &registrar) {
            Ok(d) => Ok(Json::obj([
                ("status", Json::str("ok")),
                ("diff", d.to_json()),
            ])),
            Err(diags) => Ok(Json::obj([
                ("status", Json::str("error")),
                ("diagnostics", diagnostics_json(&diags)),
            ])),
        }
    }
}
