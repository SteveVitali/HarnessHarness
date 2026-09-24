//! Group L — `lab.registry.*` dispatch (S2.12; AC-R-2.12.2). The boundary
//! adapter over the one `RegistryStore`: canonical Json params in, canonical
//! Json results out (records-in/records-out — §6.2 R3/R8; contract-only reach,
//! CC5). Mutating ops drain the store's `lifecycle.registry.*` audit rows into
//! a kernel-internal ledger run (`registry_events`) through the one fenced
//! writer (`hh_registry::events::emit`) — the rows are ledgered, never dropped.
//!
//! `registrar` is the caller-supplied canonical `ProvenanceRecord` — the store
//! re-derives its authority class from the origin (CC2; `validate` inside
//! every op), so a client claiming `kernel` authority without a kernel origin
//! is refused by the store's own gates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::names::{NameStatus, ResolveMode};
use hh_identity::refs::VersionedRef;
use hh_identity::supersede::SupersedeReason;
use hh_ledger::manifest::{RunKind, RunManifest};
use hh_ledger::store::Lease;
use hh_provenance::ProvenanceRecord;
use hh_registry::kinds::{ProducedBy, RecordKind, RequireConformance};
use hh_registry::records::RegistryRecord;
use hh_registry::schema;
use hh_registry::store::{
    CatalogEntry, QueryClause, QueryOp, QueryPredicate, ResolveInput, ResolveRequest,
    ResolvedRecord, SlotConstraints,
};
use hh_registry::RegistryError;
use hh_wire::json::Json;

use crate::service::ledger_err;
use crate::service::{EmbedService, LEASE_TTL_MS};
use hh_embed_schema::errors::EmbedError;

/// The internal ledger run's name — registry lifecycle rows append here.
const REGISTRY_RUN_LABEL: &str = "registry-events";

fn reg_err(e: RegistryError) -> EmbedError {
    EmbedError::Refused {
        reason: format!("{e}"),
    }
}

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

/// The caller's `registrar` provenance — canonical decode, then the store
/// validates (origin ⇒ authority class; a claimed class is never read).
fn registrar_of(params: &Json) -> Result<ProvenanceRecord, EmbedError> {
    ProvenanceRecord::from_json(req(params, "registrar")?)
        .map_err(|e| bad("/registrar", &format!("{e:?}")))
}

fn parse_mode(j: &Json) -> Result<ResolveMode, EmbedError> {
    match j.get("mode").and_then(|m| m.as_str()).unwrap_or("execute") {
        "execute" => Ok(ResolveMode::Execute),
        "audit" => Ok(ResolveMode::Audit),
        "reproduce" => Ok(ResolveMode::Reproduce),
        other => Err(bad("/mode", &format!("unknown mode {other}"))),
    }
}

fn parse_reason(s: &str) -> Result<SupersedeReason, EmbedError> {
    Ok(match s {
        "edit" => SupersedeReason::Edit,
        "revocation" => SupersedeReason::Revocation,
        "expiry" => SupersedeReason::Expiry,
        "migration" => SupersedeReason::Migration,
        "consolidation" => SupersedeReason::Consolidation,
        "fork" => SupersedeReason::Fork,
        _ => return Err(bad("/reason", &format!("closed vocabulary: {s}"))),
    })
}

fn parse_kind(s: &str) -> Result<RecordKind, EmbedError> {
    RecordKind::ALL
        .iter()
        .find(|k| k.as_str() == s)
        .copied()
        .ok_or_else(|| bad("/kind", &format!("unknown record kind {s}")))
}

fn parse_floor(j: &Json, k: &str) -> Result<Option<RequireConformance>, EmbedError> {
    match j.get(k).and_then(|v| v.as_str()) {
        None => Ok(None),
        Some("none") => Ok(Some(RequireConformance::None)),
        Some("declared") => Ok(Some(RequireConformance::Declared)),
        Some("probed") => Ok(Some(RequireConformance::Probed)),
        Some(other) => Err(bad(
            &format!("/{k}"),
            &format!("closed vocabulary: {other}"),
        )),
    }
}

/// The `lab.registry.import` result — refs, the D6 name map, the
/// recovered/declared-unverified split, the exact loss list and the
/// preserved foreign `_meta` members.
fn import_outcome_json(out: &hh_registry::import::ImportOutcome) -> Json {
    Json::obj([
        ("refs", Json::Arr(out.refs.iter().map(vref_json).collect())),
        (
            "name_map",
            Json::Obj(
                out.name_map
                    .iter()
                    .map(|(name, (sref, sid))| {
                        (
                            name.clone(),
                            Json::obj([
                                ("server_ref", Json::str(sref.clone())),
                                ("semantic_id", Json::str(sid.clone())),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
        (
            "recovered",
            Json::Arr(out.recovered.iter().map(|s| Json::str(s.clone())).collect()),
        ),
        (
            "declared_unverified",
            Json::Arr(
                out.declared_unverified
                    .iter()
                    .map(|s| Json::str(s.clone()))
                    .collect(),
            ),
        ),
        (
            "losses",
            Json::Arr(out.losses.iter().map(|s| Json::str(s.clone())).collect()),
        ),
        ("ext_meta", Json::Arr(out.ext_meta.clone())),
        ("listing_hash", Json::str(out.listing_hash.clone())),
    ])
}

fn vref_json(v: &VersionedRef) -> Json {
    Json::obj([
        ("version_id", Json::str(v.version_id.clone())),
        (
            "semantic_id",
            v.semantic_id.clone().map_or(Json::Null, Json::Str),
        ),
        (
            "name",
            v.name.as_ref().map_or(Json::Null, |n| {
                Json::obj([
                    ("namespace", Json::str(n.namespace.clone())),
                    ("name", Json::str(n.name.clone())),
                    ("label", n.label.clone().map_or(Json::Null, Json::Str)),
                ])
            }),
        ),
        (
            "supersedes",
            v.supersedes.clone().map_or(Json::Null, Json::Str),
        ),
        ("provenance", v.provenance.to_json()),
    ])
}

fn resolved_json(r: &ResolvedRecord, update: Option<Json>) -> Json {
    Json::obj([
        ("versioned_ref", vref_json(&r.versioned_ref)),
        ("record", schema::body_json(&r.record, false)),
        ("envelope", schema::envelope_json(&r.envelope)),
        ("admission", Json::str(r.admission.as_str())),
        (
            "name_status",
            r.name_status
                .map(|s| Json::str(schema::name_status_str(s)))
                .unwrap_or(Json::Null),
        ),
        (
            "depends_on_revoked",
            Json::Arr(
                r.depends_on_revoked
                    .iter()
                    .map(|s| {
                        Json::obj([
                            ("revoked_member", Json::str(s.revoked_member.clone())),
                            ("since_seq", Json::Int(s.since_seq as i64)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "conformance_vector",
            Json::Obj(
                r.conformance_vector
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v.as_str())))
                    .collect::<BTreeMap<_, _>>(),
            ),
        ),
        ("update_available", update.unwrap_or(Json::Null)),
    ])
}

fn catalog_entry_json(e: &CatalogEntry) -> Json {
    Json::obj([
        ("envelope", schema::envelope_json(&e.envelope)),
        ("record", schema::body_json(&e.record, false)),
        (
            "name",
            e.name.as_ref().map_or(Json::Null, |(ns, n)| {
                Json::obj([
                    ("namespace", Json::str(ns.clone())),
                    ("name", Json::str(n.clone())),
                ])
            }),
        ),
        (
            "name_status",
            e.name_status
                .map(|s| Json::str(schema::name_status_str(s)))
                .unwrap_or(Json::Null),
        ),
        ("revoked", Json::Bool(e.revoked)),
        (
            "stale",
            Json::Arr(
                e.stale
                    .iter()
                    .map(|s| Json::str(s.revoked_member.clone()))
                    .collect(),
            ),
        ),
    ])
}

fn clauses_of(j: &Json) -> Result<Vec<QueryClause>, EmbedError> {
    let mut out = Vec::new();
    if let Some(Json::Arr(list)) = j.get("clauses") {
        for c in list {
            out.push(QueryClause {
                field: req_str(c, "field")?.to_string(),
                op: match req_str(c, "op")? {
                    "eq" => QueryOp::Eq,
                    "contains" => QueryOp::Contains,
                    other => return Err(bad("/op", &format!("closed vocabulary: {other}"))),
                },
                value: req_str(c, "value")?.to_string(),
            });
        }
    }
    Ok(out)
}

fn predicate_of(j: &Json) -> Result<QueryPredicate, EmbedError> {
    Ok(QueryPredicate {
        clauses: clauses_of(j)?,
        snapshot_id: opt_str(j, "snapshot_id"),
    })
}

impl EmbedService {
    /// The internal ledger run that carries `lifecycle.registry.*` rows —
    /// created lazily, re-leased when the writer lease expires (the one fenced
    /// writer for registry events — ADR-0151 (e)).
    fn ensure_registry_run(&mut self) -> Result<(String, Lease), EmbedError> {
        if let Some((run_id, lease)) = &self.registry_run {
            // Renew the live lease; on expiry re-acquire (the holder is ours —
            // an expired record fences silently, never blocks).
            match self.store.renew(lease) {
                Ok(l) => {
                    let out = (run_id.clone(), l.clone());
                    self.registry_run = Some(out.clone());
                    return Ok(out);
                }
                Err(_) => {
                    let l = self
                        .store
                        .acquire_writer(&self.holder, run_id, LEASE_TTL_MS)
                        .map_err(ledger_err)?;
                    let out = (run_id.clone(), l);
                    self.registry_run = Some(out.clone());
                    return Ok(out);
                }
            }
        }
        let mut manifest = RunManifest::minimal(RunKind::Fleet);
        manifest.configuration_id = None;
        manifest.configuration_version_id = None;
        manifest
            .extra
            .insert("purpose".to_string(), Json::str(REGISTRY_RUN_LABEL));
        let (run_id, lease) = self
            .store
            .open_run(manifest, &self.holder)
            .map_err(ledger_err)?;
        let out = (run_id, lease);
        self.registry_run = Some(out.clone());
        Ok(out)
    }

    /// Drain pending registry events into the internal run through the one
    /// fenced writer. Event persistence is best-effort *inside* the ledger —
    /// the store's own canonical log is already authoritative; a failed append
    /// is surfaced, never swallowed.
    fn flush_registry_events(&mut self) -> Result<(), EmbedError> {
        let events = self.registry.drain_events();
        if events.is_empty() {
            return Ok(());
        }
        let (run_id, lease) = self.ensure_registry_run()?;
        let kernel = self.kernel_prov.clone();
        hh_registry::events::emit(&mut self.store, &run_id, &lease, &kernel, events)
            .map_err(ledger_err)
    }

    /// `lab.registry.register` — `{kind, body, registrar, trust_record_ref?}`.
    pub(crate) fn lab_registry_register(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let kind = parse_kind(req_str(params, "kind")?)?;
        let body = req(params, "body")?;
        let record = schema::record_from_json(kind, body).map_err(reg_err)?;
        let registrar = registrar_of(params)?;
        let v = self
            .registry
            .register(record, &registrar, opt_str(params, "trust_record_ref"))
            .map_err(reg_err)?;
        self.flush_registry_events()?;
        Ok(vref_json(&v))
    }

    /// `lab.registry.publish` — `{namespace, name, version_id, label?,
    /// supersedes?, registrar}`.
    pub(crate) fn lab_registry_publish(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let v = self.registry.publish(
            req_str(params, "namespace")?,
            req_str(params, "name")?,
            req_str(params, "version_id")?,
            opt_str(params, "label"),
            opt_str(params, "supersedes"),
            &registrar_of(params)?,
        );
        match v {
            Ok(v) => {
                self.flush_registry_events()?;
                Ok(vref_json(&v))
            }
            Err(e) => {
                self.flush_registry_events()?;
                Err(reg_err(e))
            }
        }
    }

    /// `lab.registry.resolve` — `{version_id | namespace+name(+label,
    /// snapshot_id), mode?, requirements?}`.
    pub(crate) fn lab_registry_resolve(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let mode = parse_mode(params)?;
        let input = if let Some(vid) = opt_str(params, "version_id") {
            ResolveInput::Version(vid)
        } else {
            ResolveInput::Selector {
                namespace: req_str(params, "namespace")?.to_string(),
                name: req_str(params, "name")?.to_string(),
                label: opt_str(params, "label"),
                snapshot_id: opt_str(params, "snapshot_id"),
            }
        };
        let mut req_ = ResolveRequest::default();
        if let Some(r) = params.get("requirements") {
            req_ = ResolveRequest {
                class_id: opt_str(r, "class_id"),
                contract_version: opt_str(r, "contract_version"),
                conformance_floor: parse_floor(r, "conformance_floor")?,
            };
        }
        let resolved = self
            .registry
            .resolve(&input, mode, &req_)
            .map_err(reg_err)?;
        // AC-R-2.12.2-13: a resolve answer carries `update_available` when a
        // newer published version exists on the same name — notification only.
        let update = self
            .registry
            .update_available(&resolved.envelope.version_id)
            .ok()
            .flatten()
            .map(|u| {
                Json::obj([
                    ("version_id", Json::str(u.version_id.clone())),
                    ("label", u.label.clone().map_or(Json::Null, Json::Str)),
                    ("sameness", Json::str(format!("{:?}", u.sameness.level))),
                ])
            });
        Ok(resolved_json(&resolved, update))
    }

    /// `lab.registry.query` — `{clauses?, snapshot_id?}`.
    pub(crate) fn lab_registry_query(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let results = self
            .registry
            .query(&predicate_of(params)?)
            .map_err(reg_err)?;
        Ok(Json::obj([(
            "results",
            Json::Arr(results.iter().map(|r| resolved_json(r, None)).collect()),
        )]))
    }

    /// `lab.registry.catalog` — `{clauses?, snapshot_id?}`: every record and
    /// name binding in scope, tombstoned names included.
    pub(crate) fn lab_registry_catalog(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let pred = predicate_of(params)?;
        let entries = self.registry.catalog(Some(&pred)).map_err(reg_err)?;
        Ok(Json::obj([(
            "entries",
            Json::Arr(entries.iter().map(catalog_entry_json).collect()),
        )]))
    }

    /// `lab.registry.slot_choices` — `{class_id, supports?, contract_version?,
    /// conformance_floor?, snapshot_id?}`.
    pub(crate) fn lab_registry_slot_choices(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let supports: BTreeSet<String> = params
            .get("supports")
            .and_then(|s| match s {
                Json::Arr(l) => Some(
                    l.iter()
                        .filter_map(|v| v.as_str().map(|x| x.to_string()))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let constraints = SlotConstraints {
            supports,
            contract_version: opt_str(params, "contract_version"),
            conformance_floor: parse_floor(params, "conformance_floor")?,
            snapshot_id: opt_str(params, "snapshot_id"),
        };
        let choices = self
            .registry
            .slot_choices(req_str(params, "class_id")?, &constraints)
            .map_err(reg_err)?;
        Ok(Json::obj([(
            "choices",
            Json::Arr(
                choices
                    .iter()
                    .map(|c| {
                        Json::obj([
                            ("variant_id", Json::str(c.variant_id.clone())),
                            ("class_id", Json::str(c.class_id.clone())),
                            ("version_id", Json::str(c.version_id.clone())),
                            (
                                "semantic_id",
                                c.semantic_id.clone().map_or(Json::Null, Json::Str),
                            ),
                        ])
                    })
                    .collect(),
            ),
        )]))
    }

    /// `lab.registry.substitutable` — `{a, b}`.
    pub(crate) fn lab_registry_substitutable(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let yes = self
            .registry
            .substitutable(req_str(params, "a")?, req_str(params, "b")?)
            .map_err(reg_err)?;
        Ok(Json::obj([("substitutable", Json::Bool(yes))]))
    }

    /// `lab.registry.snapshot` — `{registrar}`.
    pub(crate) fn lab_registry_snapshot(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let snap = self
            .registry
            .snapshot(&registrar_of(params)?)
            .map_err(reg_err)?;
        self.flush_registry_events()?;
        Ok(Json::obj([
            ("snapshot_id", Json::str(snap.snapshot_id.clone())),
            (
                "members",
                Json::Arr(snap.members.iter().map(|s| Json::str(s.clone())).collect()),
            ),
            (
                "name_bindings",
                Json::Arr(
                    snap.name_bindings
                        .iter()
                        .map(|((ns, n), v)| {
                            Json::obj([
                                ("namespace", Json::str(ns.clone())),
                                ("name", Json::str(n.clone())),
                                ("version_id", Json::str(v.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("policy_digest", Json::str(snap.policy_digest.clone())),
            ("created_seq", Json::Int(snap.created_seq as i64)),
        ]))
    }

    /// `lab.registry.record_conformance` — `{report, registrar}`. A
    /// `registry_ci`/`lab` report's `run_id` must name a *finished* ledger run
    /// — the ledger is the durability oracle; the boundary marks it durable
    /// in the store and the store enforces (`RunNotDurable`).
    pub(crate) fn lab_registry_record_conformance(
        &mut self,
        params: &Json,
    ) -> Result<Json, EmbedError> {
        let body = req(params, "report")?;
        let record =
            schema::record_from_json(RecordKind::ConformanceReport, body).map_err(reg_err)?;
        let RegistryRecord::Report(report) = record else {
            return Err(bad("/report", "kind_mismatch"));
        };
        if matches!(report.produced_by, ProducedBy::RegistryCi | ProducedBy::Lab) {
            // Durability: the run must exist and carry `lifecycle.run.finished`.
            let durable = self
                .store
                .events(&report.run_id)
                .map(|evs| evs.iter().any(|e| e.class == "lifecycle.run.finished"))
                .unwrap_or(false);
            if durable {
                self.registry
                    .mark_run_durable(&report.run_id, &self.kernel_prov.clone())
                    .map_err(reg_err)?;
            }
        }
        let registrar = registrar_of(params)?;
        let v = self
            .registry
            .record_conformance(report, &registrar)
            .map_err(reg_err)?;
        self.flush_registry_events()?;
        Ok(vref_json(&v))
    }

    /// `lab.registry.deprecate` / `lab.registry.yank` — `{namespace, name,
    /// registrar}`.
    pub(crate) fn lab_registry_name_status(
        &mut self,
        params: &Json,
        status: NameStatus,
    ) -> Result<Json, EmbedError> {
        let r = self.registry.set_name_status(
            req_str(params, "namespace")?,
            req_str(params, "name")?,
            status,
            &registrar_of(params)?,
        );
        self.flush_registry_events()?;
        r.map_err(reg_err)?;
        Ok(Json::obj([("ok", Json::Bool(true))]))
    }

    /// `lab.registry.revoke` — `{version_id, reason, registrar, replacement?}`.
    pub(crate) fn lab_registry_revoke(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let reason = parse_reason(req_str(params, "reason")?)?;
        let r = self.registry.revoke(
            req_str(params, "version_id")?,
            reason,
            &registrar_of(params)?,
            opt_str(params, "replacement"),
        );
        self.flush_registry_events()?;
        r.map_err(reg_err)?;
        Ok(Json::obj([("ok", Json::Bool(true))]))
    }

    /// `lab.registry.lineage` — `{version_id}`.
    pub(crate) fn lab_registry_lineage(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let view = self
            .registry
            .lineage(req_str(params, "version_id")?)
            .map_err(reg_err)?;
        Ok(Json::obj([
            ("version_id", Json::str(view.version_id.clone())),
            (
                "supersedes",
                Json::Arr(
                    view.supersedes
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            ),
            (
                "superseded_by",
                Json::Arr(
                    view.superseded_by
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            ),
            (
                "revocations",
                Json::Arr(
                    view.revocations
                        .iter()
                        .map(|r| {
                            Json::obj([
                                ("revokes", Json::str(r.revokes.clone())),
                                ("reason", Json::str(format!("{:?}", r.reason))),
                                (
                                    "replacement",
                                    r.replacement.clone().map_or(Json::Null, Json::Str),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "stale",
                Json::Arr(
                    view.stale
                        .iter()
                        .map(|s| Json::str(s.revoked_member.clone()))
                        .collect(),
                ),
            ),
            (
                "name_history",
                Json::Arr(
                    view.name_history
                        .iter()
                        .map(|e| {
                            Json::obj([
                                ("namespace", Json::str(e.namespace.as_str())),
                                ("name", Json::str(e.name.clone())),
                                ("version_id", Json::str(e.version_id.clone())),
                                ("label", e.label.clone().map_or(Json::Null, Json::Str)),
                                ("status", Json::str(schema::name_status_str(e.status))),
                                ("diff_ref", e.diff_ref.clone().map_or(Json::Null, Json::Str)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]))
    }

    /// `lab.registry.sameness` — `{a, b}`.
    pub(crate) fn lab_registry_sameness(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let s = self
            .registry
            .sameness(req_str(params, "a")?, req_str(params, "b")?)
            .map_err(reg_err)?;
        Ok(Json::obj([
            ("level", Json::str(format!("{:?}", s.level))),
            ("used_classification", Json::Bool(s.used_classification)),
        ]))
    }

    /// `lab.registry.import` — `{listing, registrar}` (S3.9; §5d.1 §2;
    /// AC-R-2.5.1-11's live boundary half): a `hh-mcp-listing/1` document
    /// is lifted and registered — one `ToolCapabilityRecord` per wire
    /// tool, `unverified` + `quarantined` for `mcp_listing` sources,
    /// published under `local/mcp/{server_ref}/{name}`.
    pub(crate) fn lab_registry_import(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let listing = req(params, "listing")?;
        let registrar = registrar_of(params)?;
        let out = hh_registry::import::import_listing(&mut self.registry, listing, &registrar)
            .map_err(reg_err)?;
        self.flush_registry_events()?;
        Ok(import_outcome_json(&out))
    }

    /// `lab.registry.refresh` — `{listing, registrar}` (S3.9): re-lift and
    /// diff name-keyed — unchanged `listing_hash` is a no-op, a changed
    /// tool registers a new version published `supersedes{edit}` with a
    /// `surface_only | semantic` classification, a removed name's
    /// versions stay addressable.
    pub(crate) fn lab_registry_refresh(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let listing = req(params, "listing")?;
        let registrar = registrar_of(params)?;
        let out = hh_registry::import::refresh(&mut self.registry, listing, &registrar)
            .map_err(reg_err)?;
        self.flush_registry_events()?;
        Ok(Json::obj([
            (
                "unchanged",
                Json::Arr(out.unchanged.iter().map(|s| Json::str(s.clone())).collect()),
            ),
            (
                "added",
                Json::Arr(out.added.iter().map(|s| Json::str(s.clone())).collect()),
            ),
            (
                "removed",
                Json::Arr(out.removed.iter().map(|s| Json::str(s.clone())).collect()),
            ),
            (
                "superseded",
                Json::Arr(
                    out.superseded
                        .iter()
                        .map(|s| {
                            Json::obj([
                                ("name", Json::str(s.name.clone())),
                                ("prev_version_id", Json::str(s.prev_version_id.clone())),
                                ("version_id", Json::str(s.version_id.clone())),
                                ("classification", Json::str(s.classification.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]))
    }

    /// `lab.registry.verify` — the content/snapshot/log integrity check.
    pub(crate) fn lab_registry_verify(&mut self, _params: &Json) -> Result<Json, EmbedError> {
        let r = self.registry.verify();
        Ok(Json::obj([
            ("ok", Json::Bool(r.ok())),
            ("checked", Json::Int(r.checked as i64)),
            (
                "name_entries_checked",
                Json::Int(r.name_entries_checked as i64),
            ),
            (
                "errors",
                Json::Arr(r.errors.iter().map(|s| Json::str(s.clone())).collect()),
            ),
        ]))
    }
}
