//! The `lab.extension.*` + `lab.permission.{grant,revoke}_approver` boundary
//! ops (S4.14a — §5g.5/§6.2/§5g.7; R-2.8.5 C1, R-2.8.7 C1, R-2.12.2¹). Thin
//! records-in/records-out wrappers over `hh_registry::extension::lifecycle`
//! and `hh_registry::extension::foreign_plugin`: the boundary passes records
//! and parameters, the lifecycle layer runs the pure gates (TrustView source
//! allowlists, attestation verification, the signature-required quarantine,
//! surface drift, widening/label-regression assessment, the model-install
//! procedure, the revocation-propagation plan), and the register/ledger arms
//! land through the standing `self.registry`/`flush_registry_events` path.
//!
//! Request shapes are the record spellings, never invented detail (CC2):
//! `{candidate{name, kind, source{…}, uri}}`, `{outcome{resolved, content,
//! fetched_at, code_identity?, source_snapshot?, surface_pin?}}`,
//! `{attestations[]}`, `{proposer{…}}` — a malformed member is a typed
//! refusal, never a default.

use std::collections::BTreeSet;

use hh_embed_schema::errors::EmbedError;
use hh_provenance::{Attestation, ProvenanceRecord};
use hh_registry::extension::foreign_plugin::{self, ForeignPluginFormat};
use hh_registry::extension::lifecycle::{self, ProposerContext, RunExtensionView};
use hh_registry::extension::{
    declared_source_from_json, Candidate, ExtensionKind, ExtensionRecord, FetchOutcome,
    SourceLocator,
};
use hh_registry::records::RegistryRecord;
use hh_wire::json::Json;

use crate::registry_ops::{bad, opt_str, parse_reason, reg_err, registrar_of, req, req_str};
use crate::service::EmbedService;

/// Decode a `candidate{name, kind, source{…}, uri, selector?}` member.
fn candidate_from_params(j: &Json) -> Result<Candidate, EmbedError> {
    let name = req_str(j, "name")?.to_string();
    let kind = ExtensionKind::parse(req_str(j, "kind")?);
    let source =
        declared_source_from_json(req(j, "source")?, "/candidate.source").map_err(reg_err)?;
    let uri = req_str(j, "uri")?.to_string();
    Ok(Candidate {
        name,
        kind,
        locator: SourceLocator {
            scheme: source.kind_str().to_string(),
            credential_free_uri: uri,
            selector: opt_str(j, "selector"),
            resolved: None,
            fetched_at: None,
        },
        source,
    })
}

/// Decode a `ContentAddress` — `"sha256:<hex>"` shorthand or the full member
/// set `{digest, media_type?, size?}` under `idp/1`+`sha256`.
fn content_addr(j: &Json, path: &str) -> Result<hh_identity::idp::ContentAddress, EmbedError> {
    match j {
        Json::Str(s) => {
            let parsed =
                hh_identity::idp::parse_id(s).map_err(|_| bad(path, "bad_content_address"))?;
            Ok(hh_identity::idp::ContentAddress {
                idp: "idp/1",
                algorithm: "sha256",
                digest: parsed.digest_hex,
                media_type: String::new(),
                size: 0,
            })
        }
        Json::Obj(m) => {
            let digest = m
                .get("digest")
                .and_then(Json::as_str)
                .ok_or_else(|| bad(path, "missing digest"))?;
            Ok(hh_identity::idp::ContentAddress {
                idp: "idp/1",
                algorithm: "sha256",
                digest: digest.strip_prefix("sha256:").unwrap_or(digest).to_string(),
                media_type: m
                    .get("media_type")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                size: m
                    .get("size")
                    .and_then(Json::as_int)
                    .map(|n| n.max(0) as u64)
                    .unwrap_or(0),
            })
        }
        _ => Err(bad(path, "type_mismatch")),
    }
}

/// Decode the `outcome` member — the resolver's fetch record.
fn outcome_from_params(j: &Json) -> Result<FetchOutcome, EmbedError> {
    Ok(FetchOutcome {
        resolved: req_str(j, "resolved")?.to_string(),
        content: content_addr(req(j, "content")?, "/outcome.content")?,
        fetched_at: j
            .get("fetched_at")
            .and_then(Json::as_int)
            .map(|n| n.max(0) as u64)
            .ok_or_else(|| bad("/outcome.fetched_at", "missing_field"))?,
        code_identity: j
            .get("code_identity")
            .and_then(|v| match v {
                Json::Arr(items) => Some(
                    items
                        .iter()
                        .filter_map(|i| i.as_str().map(str::to_string))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default(),
        source_snapshot: opt_str(j, "source_snapshot"),
        surface_pin: j
            .get("surface_pin")
            .map(|p| content_addr(p, "/outcome.surface_pin"))
            .transpose()?,
    })
}

/// Decode `{attestations[]}` — each a canonical `Attestation` JSON.
fn attestations_from_params(j: &Json) -> Result<Vec<Attestation>, EmbedError> {
    match j.get("attestations") {
        Some(Json::Arr(items)) => items
            .iter()
            .enumerate()
            .map(|(i, a)| {
                Attestation::from_json(a)
                    .map_err(|_| bad(&format!("/attestations[{i}]"), "malformed_attestation"))
            })
            .collect(),
        _ => Ok(Vec::new()),
    }
}

/// Decode the `proposer{origin?, authority?, grants[]}` member — the install
/// proposer's record. `provenance` decodes the full ProvenanceRecord member
/// when present; otherwise a `model:`-origin principal-substitute is a
/// *schema violation* — the caller declares the proposer's record.
fn proposer_from_params(j: &Json) -> Result<ProposerContext, EmbedError> {
    let provenance = ProvenanceRecord::from_json(req(j, "provenance")?)
        .map_err(|e| bad("/proposer.provenance", &format!("{e:?}")))?;
    let grants = match j.get("grants") {
        Some(Json::Arr(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => BTreeSet::new(),
    };
    Ok(ProposerContext { provenance, grants })
}

/// Decode the `run_view` member — the caller's live-lane/surface/grant/
/// in-flight projection the revocation plan reads.
fn run_view_from_params(j: &Json) -> Result<RunExtensionView, EmbedError> {
    fn pairs(j: &Json, key: &str) -> Vec<(String, String)> {
        match j.get(key) {
            Some(Json::Obj(m)) => m
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect(),
            _ => Vec::new(),
        }
    }
    let surfaces = match j.get("surfaces") {
        Some(Json::Obj(m)) => m
            .iter()
            .map(|(k, v)| {
                let tools = match v {
                    Json::Arr(items) => items
                        .iter()
                        .filter_map(|i| i.as_str().map(str::to_string))
                        .collect(),
                    _ => Vec::new(),
                };
                (k.clone(), tools)
            })
            .collect(),
        _ => Vec::new(),
    };
    let results = match j.get("results") {
        Some(Json::Obj(m)) => m
            .iter()
            .map(|(k, v)| {
                let vids = match v {
                    Json::Arr(items) => items
                        .iter()
                        .filter_map(|i| i.as_str().map(str::to_string))
                        .collect(),
                    _ => Vec::new(),
                };
                (k.clone(), vids)
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok(RunExtensionView {
        servers: pairs(j, "servers"),
        surfaces,
        grants: pairs(j, "grants"),
        in_flight: pairs(j, "in_flight"),
        results,
    })
}

/// The record under `version_id` as an `ExtensionRecord` — a typed refusal
/// for a missing/wrong-kind member (never a silent projection).
fn extension_record(svc: &EmbedService, version_id: &str) -> Result<ExtensionRecord, EmbedError> {
    match svc.registry.get(version_id) {
        Some((_, RegistryRecord::Extension(e))) => Ok(e.clone()),
        Some(_) => Err(bad(
            "/version_id",
            &format!("record {version_id} is not an extension"),
        )),
        None => Err(EmbedError::Refused {
            reason: format!("unknown version_id {version_id}"),
        }),
    }
}

impl EmbedService {
    /// `lab.extension.discover` — `{candidates[]}` → the source-allowlist
    /// gate's surviving candidates (a refused source is `SourceNotAllowed`,
    /// typed — the check is over the live `TrustRootPolicy` union).
    pub(crate) fn lab_extension_discover(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let candidates: Vec<Candidate> = match req(params, "candidates")? {
            Json::Arr(items) => items
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    candidate_from_params(c)
                        .map_err(|e| bad(&format!("/candidates[{i}]"), &format!("{e:?}")))
                })
                .collect::<Result<_, EmbedError>>()?,
            _ => return Err(bad("/candidates", "type_mismatch")),
        };
        let view = self.registry.trust_view();
        let out = lifecycle::discover(&candidates, &view).map_err(reg_err)?;
        Ok(Json::obj([(
            "candidates",
            Json::Arr(
                out.iter()
                    .map(|c| {
                        Json::obj([
                            ("name", Json::str(c.name.clone())),
                            ("kind", Json::str(c.kind.as_str())),
                            ("source", Json::str(c.source.kind_str())),
                            ("uri", Json::str(c.locator.credential_free_uri.clone())),
                        ])
                    })
                    .collect(),
            ),
        )]))
    }

    /// `lab.extension.resolve` — `{candidate{…}, outcome{…}, attestations?,
    /// payload?, registrar}` → resolve under the live TrustView (allowed
    /// sources, attestation verification, `required_predicates`, `max_age`,
    /// the signature-required quarantine, the hash-only ceiling), then
    /// `register` the minted record. Returns the `version_id`, the register
    /// admission, the verified attestation kinds and the quarantine flag.
    pub(crate) fn lab_extension_resolve(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let candidate = candidate_from_params(req(params, "candidate")?)?;
        let outcome = outcome_from_params(req(params, "outcome")?)?;
        let attestations = attestations_from_params(params)?;
        let registrar = registrar_of(params)?;
        let now = self.store.now_ms();
        let view = self.registry.trust_view();
        let report = lifecycle::resolve_extension(
            &candidate,
            &outcome,
            &attestations,
            None,
            registrar.origin.clone(),
            &view,
            now,
        )
        .map_err(reg_err)?;
        let vref = self
            .registry
            .register(
                RegistryRecord::Extension(report.record),
                &registrar,
                opt_str(params, "trust_record_ref"),
            )
            .map_err(reg_err)?;
        self.flush_registry_events()?;
        let admission = self
            .registry
            .get(&vref.version_id)
            .map(|(env, _)| env.admission.as_str().to_string())
            .unwrap_or_else(|| "quarantined".to_string());
        Ok(Json::obj([
            ("version_id", Json::str(vref.version_id)),
            ("admission", Json::str(admission)),
            (
                "verified_kinds",
                Json::Arr(
                    report
                        .verified_kinds
                        .iter()
                        .map(|k| Json::str(k.clone()))
                        .collect(),
                ),
            ),
            ("quarantined", Json::Bool(report.quarantined)),
            (
                "notes",
                Json::Arr(report.notes.iter().map(|n| Json::str(n.clone())).collect()),
            ),
        ]))
    }

    /// `lab.extension.review` — `{version_id, previous_version_id?}` → the
    /// review document (`lifecycle::review`): the install/exec plan the
    /// human reads — claims, hygiene, attestation status, the diff legs when
    /// a previous version is named.
    pub(crate) fn lab_extension_review(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let record = extension_record(self, req_str(params, "version_id")?)?;
        let previous = match opt_str(params, "previous_version_id") {
            Some(v) => Some(extension_record(self, &v)?),
            None => None,
        };
        Ok(lifecycle::review(&record, previous.as_ref()))
    }

    /// `lab.extension.check_surface` — `{extension_id, pinned, live,
    /// run_ref?}` → `{status: "unchanged", listing_hash}` or
    /// `{status: "drifted", drift{…}, event}` — the `security.extension.drift`
    /// payload rides the result; the caller appends it through the run's
    /// fenced writer (the boundary mints evidence, never pretends to have
    /// appended it — AC-R-2.8.5-4).
    pub(crate) fn lab_extension_check_surface(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let extension_id = req_str(params, "extension_id")?;
        let pinned = req(params, "pinned")?;
        let live = req(params, "live")?;
        let run_ref = opt_str(params, "run_ref");
        let observer = self.kernel_prov.clone();
        match lifecycle::check_surface(pinned, live, &observer, extension_id, run_ref.as_deref()) {
            Ok(hash) => Ok(Json::obj([
                ("status", Json::str("unchanged")),
                ("listing_hash", Json::str(hash)),
            ])),
            Err(d) => Ok(Json::obj([
                ("status", Json::str("drifted")),
                (
                    "drift",
                    Json::obj([
                        ("extension_id", Json::str(d.extension_id.clone())),
                        (
                            "pinned_listing_hash",
                            Json::str(d.pinned_listing_hash.clone()),
                        ),
                        ("live_listing_hash", Json::str(d.live_listing_hash.clone())),
                        (
                            "added",
                            Json::Arr(d.added.iter().map(|t| Json::str(t.clone())).collect()),
                        ),
                        (
                            "removed",
                            Json::Arr(d.removed.iter().map(|t| Json::str(t.clone())).collect()),
                        ),
                        (
                            "changed",
                            Json::Arr(d.changed.iter().map(|t| Json::str(t.clone())).collect()),
                        ),
                    ]),
                ),
                ("event", d.event),
            ])),
        }
    }

    /// `lab.extension.update` — `{old_version_id, new_version_id}` → the
    /// update assessment (`lifecycle::update_assessment`): `widening`,
    /// `regressions[]` (the `LabelRegression` rows), `requires_human` (the
    /// publish gate's human+attestation demand on a widening update).
    pub(crate) fn lab_extension_update(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let old = extension_record(self, req_str(params, "old_version_id")?)?;
        let new = extension_record(self, req_str(params, "new_version_id")?)?;
        let a = lifecycle::update_assessment(&old, &new);
        Ok(Json::obj([
            ("widening", Json::Bool(a.widening)),
            ("requires_human", Json::Bool(a.requires_human)),
            (
                "regressions",
                Json::Arr(
                    a.regressions
                        .iter()
                        .map(|r| {
                            Json::obj([
                                ("member", Json::str(r.field.clone())),
                                ("from", Json::str(r.was.clone())),
                                ("to", Json::str(r.now.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]))
    }

    /// `lab.extension.revoke` — `{version_id, reason, registrar, run_view?}`
    /// → the registry revocation plus the propagation plan
    /// (`lifecycle::propagate_revocation` over the caller's run view):
    /// `revoked_event`, `terminations`, `dropped_surface_tools`,
    /// `denied_grants`, `unknown_effects[]`, `depends_on_revoked`,
    /// `stale_dependants` — every member an instruction the caller enforces
    /// (AC-R-2.8.5-8).
    pub(crate) fn lab_extension_revoke(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let version_id = req_str(params, "version_id")?;
        let reason_str = opt_str(params, "reason").unwrap_or_else(|| "revocation".to_string());
        let reason = parse_reason(&reason_str)?;
        let registrar = registrar_of(params)?;
        self.registry
            .revoke(
                version_id,
                reason,
                &registrar,
                opt_str(params, "replacement"),
            )
            .map_err(reg_err)?;
        self.flush_registry_events()?;
        let view = match params.get("run_view") {
            Some(v) => run_view_from_params(v)?,
            None => RunExtensionView {
                servers: Vec::new(),
                surfaces: Vec::new(),
                grants: Vec::new(),
                in_flight: Vec::new(),
                results: Vec::new(),
            },
        };
        let stale: Vec<String> = match params.get("stale_dependants") {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        };
        let plan = lifecycle::propagate_revocation(
            version_id,
            &reason_str,
            &stale,
            &view,
            &self.kernel_prov.clone(),
        );
        Ok(Json::obj([
            ("revoked_event", plan.revoked_event),
            (
                "terminations",
                Json::Arr(
                    plan.terminations
                        .iter()
                        .map(|t| Json::str(t.clone()))
                        .collect(),
                ),
            ),
            (
                "dropped_surface_tools",
                Json::Arr(
                    plan.dropped_surface_tools
                        .iter()
                        .map(|t| Json::str(t.clone()))
                        .collect(),
                ),
            ),
            (
                "denied_grants",
                Json::Arr(
                    plan.denied_grants
                        .iter()
                        .map(|g| Json::str(g.clone()))
                        .collect(),
                ),
            ),
            ("unknown_effects", Json::Arr(plan.unknown_effects)),
            (
                "depends_on_revoked",
                Json::Arr(
                    plan.depends_on_revoked
                        .iter()
                        .map(|r| Json::str(r.clone()))
                        .collect(),
                ),
            ),
            (
                "stale_dependants",
                Json::Arr(
                    plan.stale_dependants
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            ),
        ]))
    }

    /// `lab.extension.install` — the model-install procedure (AC-R-2.8.5-7):
    /// `{candidate{…}, outcome{…}, requested_grants[]?, attestations?,
    /// proposer{…}, approved?}` → `install_plan` (the `model_install` rule +
    /// the grants-⊆-proposer widening gate) then `install_completed` mints
    /// the attenuated+tainted record; `register` lands it quarantined under
    /// a non-first-party proposer. `approved` names the resolved
    /// `permission_id` under `model_install = ask`.
    pub(crate) fn lab_extension_install(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let candidate = candidate_from_params(req(params, "candidate")?)?;
        let outcome = outcome_from_params(req(params, "outcome")?)?;
        let attestations = attestations_from_params(params)?;
        let proposer = proposer_from_params(req(params, "proposer")?)?;
        let requested_grants: BTreeSet<String> = match params.get("requested_grants") {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            _ => BTreeSet::new(),
        };
        let view = self.registry.trust_view();
        let plan =
            lifecycle::install_plan(&candidate, &outcome, requested_grants, &proposer, &view)
                .map_err(reg_err)?;
        let now = self.store.now_ms();
        let (record, completed_event) = lifecycle::install_completed(
            &candidate,
            &outcome,
            &attestations,
            None,
            &proposer,
            &view,
            opt_str(params, "approved").as_deref(),
            now,
        )
        .map_err(reg_err)?;
        let registrar = record.provenance.clone();
        let vref = self
            .registry
            .register(
                RegistryRecord::Extension(record),
                &registrar,
                opt_str(params, "trust_record_ref"),
            )
            .map_err(reg_err)?;
        self.flush_registry_events()?;
        let admission = self
            .registry
            .get(&vref.version_id)
            .map(|(env, _)| env.admission.as_str().to_string())
            .unwrap_or_else(|| "quarantined".to_string());
        Ok(Json::obj([
            ("version_id", Json::str(vref.version_id)),
            ("admission", Json::str(admission)),
            ("requires_approval", Json::Bool(plan.requires_approval)),
            ("install_completed_event", completed_event),
        ]))
    }

    /// `lab.extension.import_plugin` — `{format, document, source{…},
    /// source_uri, registrar, trust_record_ref?}` → the foreign-format import
    /// (quarantined plugin record + LossReport; AC-R-2.12.2-11).
    pub(crate) fn lab_extension_import_plugin(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let format = ForeignPluginFormat::parse(req_str(params, "format")?)
            .ok_or_else(|| bad("/format", "unknown foreign plugin format"))?;
        let document = req(params, "document")?;
        let source =
            declared_source_from_json(req(params, "source")?, "/source").map_err(reg_err)?;
        let source_uri = req_str(params, "source_uri")?.to_string();
        let registrar = registrar_of(params)?;
        let out = foreign_plugin::import_foreign_plugin(
            &mut self.registry,
            format,
            document,
            &source,
            &source_uri,
            &registrar,
            opt_str(params, "trust_record_ref"),
            self.store.now_ms(),
        )
        .map_err(reg_err)?;
        self.flush_registry_events()?;
        Ok(Json::obj([
            ("version_id", Json::str(out.version_id)),
            ("admission", Json::str(out.admission.as_str())),
            (
                "loss_report",
                hh_registry::schema::loss_report_json(&out.loss_report),
            ),
        ]))
    }

    /// `lab.extension.export_plugin` — `{version_id, format}` →
    /// `{document, loss_report}` — the foreign re-emission (the verbatim
    /// substrate + canonical identity overlay; AC-R-2.12.2-11's export leg).
    pub(crate) fn lab_extension_export_plugin(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let format = ForeignPluginFormat::parse(req_str(params, "format")?)
            .ok_or_else(|| bad("/format", "unknown foreign plugin format"))?;
        let record = extension_record(self, req_str(params, "version_id")?)?;
        let (doc, loss) =
            foreign_plugin::export_foreign_plugin(format, &record).map_err(reg_err)?;
        Ok(Json::obj([
            ("document", doc),
            ("loss_report", hh_registry::schema::loss_report_json(&loss)),
        ]))
    }

    /// `lab.permission.grant_approver` — `{run_id, grantee,
    /// capability_prefixes[], max_risk, expires_at?, registrar}` → mints the
    /// `ApproverGrant` (legitimacy: `principal`+ grantor or a live covering
    /// grant — delegation never widens) and appends the durable
    /// `security.permission.grant_issued` row on `run_id` (the run's
    /// `ApprovalState::project` fold reads it — the ledger is the one
    /// truth). Returns `{grant, issued}`.
    pub(crate) fn lab_permission_grant_approver(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let run_id = req_str(params, "run_id")?.to_string();
        let grantor = registrar_of(params)?;
        let grantee = req_str(params, "grantee")?.to_string();
        let capability_prefixes: Vec<String> = match params.get("capability_prefixes") {
            Some(Json::Arr(items)) => items
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        };
        let max_risk = hh_ontology::risk::RiskClass::from_json(req(params, "max_risk")?)
            .ok_or_else(|| bad("/max_risk", "malformed_risk_class"))?;
        let expires_at = params
            .get("expires_at")
            .and_then(Json::as_int)
            .map(|n| n.max(0) as u64);
        let now = self.store.now_ms();
        // The live grant set is the run's own fold — a delegate grantor's
        // coverage is what the *ledger* says, never a claim (CC1/CC2).
        let events = self
            .store
            .events(&run_id)
            .map_err(crate::service::ledger_err)?
            .to_vec();
        let head_seq = events.last().map(|e| e.seq).unwrap_or(0);
        let approvals = hh_monitor::approval::ApprovalState::project(&events, head_seq);
        let live: Vec<hh_monitor::approval::ApproverGrant> =
            approvals.grants.values().cloned().collect();
        let (grant, payload) = hh_monitor::approval::grant_approver(
            &grantor,
            &grantee,
            capability_prefixes,
            max_risk,
            expires_at,
            now,
            &live,
        )
        .map_err(|e| EmbedError::Refused {
            reason: format!("{e:?}"),
        })?;
        self.store
            .commit_kernel_row_for(
                "security.permission",
                &run_id,
                "security.permission.grant_issued",
                payload.clone(),
                Vec::new(),
                Vec::new(),
            )
            .map_err(crate::service::ledger_err)?;
        Ok(Json::obj([("grant", grant.to_json()), ("issued", payload)]))
    }

    /// `lab.permission.revoke_approver` — `{run_id, grant_ref, registrar}` →
    /// the `security.permission.grant_revoked` row on `run_id` (grantor or
    /// `principal`+ only).
    pub(crate) fn lab_permission_revoke_approver(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let run_id = req_str(params, "run_id")?.to_string();
        let grant_ref = req_str(params, "grant_ref")?.to_string();
        let revoker = registrar_of(params)?;
        let now = self.store.now_ms();
        let events = self
            .store
            .events(&run_id)
            .map_err(crate::service::ledger_err)?
            .to_vec();
        let head_seq = events.last().map(|e| e.seq).unwrap_or(0);
        let approvals = hh_monitor::approval::ApprovalState::project(&events, head_seq);
        let live: Vec<hh_monitor::approval::ApproverGrant> =
            approvals.grants.values().cloned().collect();
        let payload = hh_monitor::approval::revoke_approver(&grant_ref, &revoker, &live, now)
            .map_err(|e| EmbedError::Refused {
                reason: format!("{e:?}"),
            })?;
        self.store
            .commit_kernel_row_for(
                "security.permission",
                &run_id,
                "security.permission.grant_revoked",
                payload.clone(),
                Vec::new(),
                Vec::new(),
            )
            .map_err(crate::service::ledger_err)?;
        Ok(Json::obj([("revoked", payload)]))
    }
}
