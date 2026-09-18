//! The `RegistryStore` — the C0/Stage-1 registry **store** (spec §6.2, R-2.10.2⁰;
//! ADR-0151 D6–D7, R1–R10; ADR-0239 for this slice's owned decisions).
//!
//! Persistence: **one canonical log** (`registry.json`) — a JSON array of append-only
//! lines (`record` / `name_history` / `supersedes` / `revocation` / `policy` /
//! `diagnostic`), rewritten atomically on each mutation. Records-in/records-out,
//! deterministic given `registry_snapshot_id` (R8). The store **reuses** the one
//! identity scheme: `NameIndex`/`Lineage`/`VersionedRef` from `hh-identity` (CC1 —
//! name histories and the version DAG are replayed into those structures on open,
//! never re-implemented), and never touches `implementation.content` (R3 — AC-3).
//!
//! `registered_at`/`published_at_seq` are transaction-time seqs (line indexes),
//! never wall clocks (§8.3 #2).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use hh_identity::kinds::RecordKind as IdentityKind;
use hh_identity::names::{
    NameHistoryEntry, NameIndex, NameStatus, Namespace, PublishError, ResolveMode, ResolveOutcome,
};
use hh_identity::refs::{NameSelector, VersionedRef};
use hh_identity::supersede::{
    Lineage, RevocationRecord, StaleEntry, SupersedeError, SupersedeReason,
};
use hh_provenance::{AuthorityClass, Origin, ProvenanceRecord};
use hh_wire::json::Json;

use crate::errors::RegistryError;
use crate::events::RegistryEvent;
use crate::identity;
use crate::kinds::{Admission, OwnerRef, Placement, PublishRule, RecordKind, RequireConformance};
use crate::records::{
    CapabilityRecord, NamespaceRecord, RegistryDiagnostic, RegistryEnvelope, RegistryPolicy,
    RegistryRecord, RegistrySnapshot, VariantRecord,
};
use crate::schema;

/// The file name of the canonical log inside the store directory.
const LOG_FILE: &str = "registry.json";

// ─────────────────────────────────────────────────────────────────────────────
// Request/response types
// ─────────────────────────────────────────────────────────────────────────────

/// What `resolve` is asked for — a pinned `version_id` or a `(namespace, name[,
/// label])` selector, optionally inside a `registry_snapshot_id` (R7/R8).
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveInput {
    /// A pinned `version_id` (resolves in every mode with status attached).
    Version(String),
    /// A name selector, optionally confined to a snapshot's pinned binding.
    Selector {
        /// `hh` or `local`.
        namespace: String,
        /// The logical name.
        name: String,
        /// A label pin, when asked.
        label: Option<String>,
        /// Resolve inside this snapshot's bindings (R8 determinism).
        snapshot_id: Option<String>,
    },
}

/// The `requirements` of `resolve`/`slot_choices` — the contract the resolved
/// record must satisfy (`class_id`, `contract_version`, `conformance_floor`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolveRequest {
    /// The required class id (a variant of another class fails
    /// `ContractIncompatible`).
    pub class_id: Option<String>,
    /// The required contract version (must fall inside `contract_range`).
    pub contract_version: Option<String>,
    /// The conformance floor to check beyond the policy default (`probed` reads
    /// admissible non-stale reports; `publisher_claim` never satisfies it).
    pub conformance_floor: Option<RequireConformance>,
}

/// The resolved record — the pinned `VersionedRef` plus the status axes and the
/// stale annotation (S4 — `depends_on_revoked` is data, never a silent exclusion).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRecord {
    /// The pinned reference (`kind = identity RegistryRecord` — CC1).
    pub versioned_ref: VersionedRef,
    /// The typed record body.
    pub record: RegistryRecord,
    /// The envelope (admission axis + registration metadata).
    pub envelope: RegistryEnvelope,
    /// The name-history entry when resolved through a name.
    pub name_entry: Option<NameHistoryEntry>,
    /// The *effective* admission — `revoked` derived from the lineage.
    pub admission: Admission,
    /// The publication status of the resolved name entry, when any.
    pub name_status: Option<NameStatus>,
    /// Stale-by-dependency entries (revoked members this record depends on).
    pub depends_on_revoked: Vec<StaleEntry>,
    /// The derived `variant_conformance_vector` (declared ∪ probed — ADR-0152 D6).
    pub conformance_vector: BTreeMap<String, crate::kinds::ConformanceVerdict>,
}

/// A `query` predicate — closed, quantifier-free, over declared fields only
/// (never relevance over free text). `UnknownField` on anything else.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryClause {
    /// The declared field (`kind`, `class_id`, `variant_id`, `semantic_id`,
    /// `admission`, `name`, `produced_by`).
    pub field: String,
    /// The operator (`eq`, `contains` — `contains` only on collection fields).
    pub op: QueryOp,
    /// The compared value (canonical spelling).
    pub value: String,
}

/// The closed `query` operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryOp {
    /// Equality.
    Eq,
    /// Collection membership.
    Contains,
}

/// The `query` predicate set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueryPredicate {
    /// The conjunctive clauses.
    pub clauses: Vec<QueryClause>,
    /// Confine to a snapshot's member set (`UnknownSnapshot` if unregistered).
    pub snapshot_id: Option<String>,
}

/// The `slot_choices` constraints (ADR-0151 D7 — minimal Stage-1 form: the
/// floor/vector semantics are Stage 2 — ADR-0239).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlotConstraints {
    /// Required declared capabilities (`supports[]` ⊆ the variant's declared
    /// truthy `capability_declaration` keys).
    pub supports: BTreeSet<String>,
    /// Required `contract_version` (inside `contract_range`).
    pub contract_version: Option<String>,
    /// The conformance floor (defaults to the policy's `require_conformance`).
    pub conformance_floor: Option<RequireConformance>,
    /// Confine to a snapshot's member set.
    pub snapshot_id: Option<String>,
}

/// The `verify()` report — every checked record and every divergence found.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyReport {
    /// Records checked.
    pub checked: usize,
    /// Name-history entries checked.
    pub name_entries_checked: usize,
    /// Divergences (`version_id` mismatches, dangling references, tampered log).
    pub errors: Vec<String>,
}

impl VerifyReport {
    /// Whether the store is intact.
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// The `lineage(version_id)` projection — the append-only identity/name/version
/// relationships around one record (spec §6.2 `lineage`).
#[derive(Debug, Clone, PartialEq)]
pub struct LineageView {
    /// The subject `version_id`.
    pub version_id: String,
    /// `supersedes` edges where the subject is `newer` (points to ancestors).
    pub supersedes: Vec<String>,
    /// Edges where the subject is `older` (its successors).
    pub superseded_by: Vec<String>,
    /// `RevocationRecord`s naming the subject.
    pub revocations: Vec<RevocationRecord>,
    /// Stale-by-dependency entries of the subject.
    pub stale: Vec<StaleEntry>,
    /// Name-history entries binding the subject (append-only).
    pub name_history: Vec<NameHistoryEntry>,
}

// ─────────────────────────────────────────────────────────────────────────────
// The store
// ─────────────────────────────────────────────────────────────────────────────

/// The Stage-1 registry store: records, name index, lineage, namespaces, policy,
/// diagnostics and pending audit events over one canonical log.
pub struct RegistryStore {
    dir: PathBuf,
    /// `version_id → (envelope, record)` — the registered records.
    records: BTreeMap<String, (RegistryEnvelope, RegistryRecord)>,
    /// The one name index (reused — CC1), replayed from `name_history` lines.
    names: NameIndex,
    /// The one version DAG + stale index (reused — CC1), replayed.
    lineage: Lineage,
    /// `namespace spelling → NamespaceRecord` (the latest registered version).
    namespaces: BTreeMap<String, NamespaceRecord>,
    /// The live `RegistryPolicy` (the latest `policy` line, or the Stage-1 default).
    policy: RegistryPolicy,
    /// `version_id → RegistrarDiagnostics` — every refused operation (R10).
    diagnostics: Vec<RegistryDiagnostic>,
    /// `RegistryEvent`s collected since the last [`Self::drain_events`].
    pending: Vec<RegistryEvent>,
    /// Persisted name-history lines — replay state for `NameIndex` (`widening`
    /// is data: recomputing it at replay would need both versions' bodies).
    name_lines: Vec<Json>,
    /// The transaction-time clock — the count of persisted log lines.
    seq: u64,
}

impl RegistryStore {
    /// `open(dir, kernel)` — replay the canonical log, or bootstrap an empty store
    /// with the two Stage-1 `NamespaceRecord`s (`hh/` kernel-owned; `local/`
    /// principal-writable) registered under `kernel` provenance (ADR-0153 D2).
    pub fn open(
        dir: impl AsRef<Path>,
        kernel: &ProvenanceRecord,
    ) -> Result<RegistryStore, RegistryError> {
        let dir = dir.as_ref().to_path_buf();
        let mut store = RegistryStore {
            dir,
            records: BTreeMap::new(),
            names: NameIndex::new(),
            lineage: Lineage::new(),
            namespaces: BTreeMap::new(),
            policy: RegistryPolicy::stage1_default(),
            diagnostics: Vec::new(),
            pending: Vec::new(),
            name_lines: Vec::new(),
            seq: 0,
        };
        let path = store.log_path();
        if path.exists() {
            let bytes = fs::read(&path).map_err(|e| RegistryError::SchemaViolation {
                path: "log".to_string(),
                detail: format!("unreadable log: {e}"),
            })?;
            store.replay(&bytes)?;
        } else {
            fs::create_dir_all(&store.dir).map_err(|e| RegistryError::SchemaViolation {
                path: "log".to_string(),
                detail: format!("cannot create store dir: {e}"),
            })?;
            store.bootstrap(kernel)?;
        }
        Ok(store)
    }

    fn log_path(&self) -> PathBuf {
        self.dir.join(LOG_FILE)
    }

    /// The two Stage-1 namespaces as the store's first records.
    fn bootstrap(&mut self, kernel: &ProvenanceRecord) -> Result<(), RegistryError> {
        for ns in [
            NamespaceRecord {
                namespace: "hh".to_string(),
                owners: vec![OwnerRef::Principal("kernel".to_string())],
                who_may_publish: PublishRule::KernelOnly,
                who_may_deprecate: PublishRule::KernelOnly,
                who_may_yank: PublishRule::KernelOnly,
                who_may_revoke: PublishRule::KernelOnly,
                require_signature: false,
            },
            NamespaceRecord {
                namespace: "local".to_string(),
                owners: vec![OwnerRef::Principal("principal".to_string())],
                who_may_publish: PublishRule::OwnersOrPrincipal,
                who_may_deprecate: PublishRule::OwnersOrPrincipal,
                who_may_yank: PublishRule::OwnersOrPrincipal,
                who_may_revoke: PublishRule::OwnersOrPrincipal,
                require_signature: false,
            },
        ] {
            let record = RegistryRecord::Namespace(ns.clone());
            let version_id = identity::version_id(&record);
            let env = RegistryEnvelope {
                kind: RecordKind::Namespace,
                version_id: version_id.clone(),
                semantic_id: None,
                registered_at: self.seq,
                registrar: kernel.clone(),
                admission: Admission::Resolved,
                name_history_ref: None,
                trust_record_ref: None,
                dialect_range: "registry/1".to_string(),
                ext: BTreeMap::new(),
            };
            self.namespaces.insert(ns.namespace.clone(), ns);
            self.records.insert(version_id, (env, record));
            self.seq += 1;
        }
        self.flush()?;
        Ok(())
    }

    // ── persistence ──────────────────────────────────────────────────────

    /// Rewrite the canonical log atomically (tmp + rename — one canonical form;
    /// `verify()` re-checks the bytes against the in-memory state).
    fn flush(&self) -> Result<(), RegistryError> {
        let mut lines: Vec<Json> = Vec::new();
        for (env, rec) in self.records.values() {
            lines.push(Json::obj([
                ("type", Json::str("record")),
                ("seq", Json::Int(env.registered_at as i64)),
                ("envelope", schema::envelope_json(env)),
                ("body", schema::body_json(rec, false)),
            ]));
        }
        for l in &self.name_lines {
            lines.push(l.clone());
        }
        for e in self.lineage_edges_json() {
            lines.push(e);
        }
        for r in self.lineage_revocations_json() {
            lines.push(r);
        }
        if self.policy != RegistryPolicy::stage1_default() {
            lines.push(Json::obj([
                ("type", Json::str("policy")),
                ("policy", schema::policy_json(&self.policy)),
            ]));
        }
        for d in &self.diagnostics {
            lines.push(Json::obj([
                ("type", Json::str("diagnostic")),
                ("diagnostic", schema::diagnostic_json(d)),
            ]));
        }
        let bytes = Json::Arr(lines).to_canonical_string().into_bytes();
        let tmp = self.dir.join(".registry.json.tmp");
        fs::write(&tmp, &bytes).map_err(|e| RegistryError::SchemaViolation {
            path: "log".to_string(),
            detail: format!("cannot write log: {e}"),
        })?;
        fs::rename(&tmp, self.log_path()).map_err(|e| RegistryError::SchemaViolation {
            path: "log".to_string(),
            detail: format!("cannot rename log: {e}"),
        })?;
        Ok(())
    }

    fn replay(&mut self, bytes: &[u8]) -> Result<(), RegistryError> {
        let text = std::str::from_utf8(bytes).map_err(|e| RegistryError::SchemaViolation {
            path: "log".to_string(),
            detail: format!("{e}"),
        })?;
        let j = hh_wire::json::parse(text).map_err(|e| RegistryError::SchemaViolation {
            path: "log".to_string(),
            detail: format!("{e:?}"),
        })?;
        let lines = match j {
            Json::Arr(l) => l,
            _ => {
                return Err(RegistryError::SchemaViolation {
                    path: "log".to_string(),
                    detail: "expected array".to_string(),
                })
            }
        };
        let mut entry_by_version: BTreeMap<String, NameHistoryEntry> = BTreeMap::new();
        for line in &lines {
            let ty = line.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match ty {
                "record" => {
                    let env = schema::envelope_from_json(
                        line.get("envelope").unwrap_or(&Json::Null),
                        "envelope",
                    )?;
                    let rec = schema::record_from_json(
                        env.kind,
                        line.get("body").unwrap_or(&Json::Null),
                    )?;
                    if let RegistryRecord::Namespace(ns) = &rec {
                        self.namespaces.insert(ns.namespace.clone(), ns.clone());
                    }
                    for dep in rec.referenced_version_ids() {
                        self.lineage.declare_dependency(&env.version_id, &dep);
                    }
                    self.records.insert(env.version_id.clone(), (env, rec));
                }
                "name_history" => {
                    let entry = self.replay_name_entry(line, &entry_by_version)?;
                    entry_by_version.insert(entry.version_id.clone(), entry);
                    self.name_lines.push(line.clone());
                }
                "supersedes" => {
                    let newer = str_of(line, "newer");
                    let older = str_of(line, "older");
                    let reason = parse_reason(str_of(line, "reason"))?;
                    let aos = matches!(
                        line.get("ancestor_or_sibling"),
                        Some(Json::Bool(true)) | None
                    );
                    self.lineage
                        .supersede(newer, older, reason, aos)
                        .map_err(|e| RegistryError::SchemaViolation {
                            path: "supersedes".to_string(),
                            detail: format!("{e:?}"),
                        })?;
                }
                "revocation" => {
                    let revokes = str_of(line, "revokes");
                    let reason = parse_reason(str_of(line, "reason"))?;
                    let prov =
                        ProvenanceRecord::from_json(line.get("provenance").unwrap_or(&Json::Null))
                            .map_err(|e| RegistryError::SchemaViolation {
                                path: "revocation.provenance".to_string(),
                                detail: format!("{e:?}"),
                            })?;
                    let replacement = line
                        .get("replacement")
                        .and_then(|r| r.as_str())
                        .map(|s| s.to_string());
                    self.lineage.revoke(
                        revokes,
                        IdentityKind::RegistryRecord,
                        reason,
                        prov.origin,
                        replacement,
                    );
                }
                "policy" => {
                    self.policy = schema::policy_from_json(
                        line.get("policy").unwrap_or(&Json::Null),
                        "policy",
                    )?;
                }
                "diagnostic" => {
                    self.diagnostics.push(schema::diagnostic_from_json(
                        line.get("diagnostic").unwrap_or(&Json::Null),
                        "diagnostic",
                    )?);
                }
                other => {
                    return Err(RegistryError::SchemaViolation {
                        path: "log".to_string(),
                        detail: format!("unknown line type: {other}"),
                    })
                }
            }
        }
        self.seq = self
            .records
            .values()
            .map(|(e, _)| e.registered_at)
            .max()
            .unwrap_or(0)
            + 1;
        Ok(())
    }

    /// Rebuild one `NameHistoryEntry` by replaying `NameIndex::publish` — the one
    /// append path stays the only way entries exist (CC1).
    fn replay_name_entry(
        &mut self,
        line: &Json,
        by_version: &BTreeMap<String, NameHistoryEntry>,
    ) -> Result<NameHistoryEntry, RegistryError> {
        let namespace = str_of(line, "namespace");
        let name = str_of(line, "name");
        let version_id = str_of(line, "version_id");
        let label = line
            .get("label")
            .and_then(|l| l.as_str())
            .map(|s| s.to_string());
        let status = match str_of(line, "status") {
            "active" => NameStatus::Active,
            "deprecated" => NameStatus::Deprecated,
            "yanked" => NameStatus::Yanked,
            other => {
                return Err(RegistryError::SchemaViolation {
                    path: "name_history.status".to_string(),
                    detail: format!("closed vocabulary: {other}"),
                })
            }
        };
        let supersedes = line
            .get("supersedes")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let widening = matches!(line.get("widening"), Some(Json::Bool(true)));
        let publisher = ProvenanceRecord::from_json(line.get("publisher").unwrap_or(&Json::Null))
            .map_err(|e| RegistryError::SchemaViolation {
            path: "name_history.publisher".to_string(),
            detail: format!("{e:?}"),
        })?;
        let ns = Namespace::parse(namespace).ok_or_else(|| RegistryError::SchemaViolation {
            path: "name_history.namespace".to_string(),
            detail: namespace.to_string(),
        })?;
        let (_e, rec) =
            self.records
                .get(version_id)
                .ok_or_else(|| RegistryError::SchemaViolation {
                    path: "name_history.version_id".to_string(),
                    detail: format!("unregistered version: {version_id}"),
                })?;
        let vref = VersionedRef {
            kind: IdentityKind::RegistryRecord,
            version_id: version_id.to_string(),
            semantic_id: identity::semantic_id(rec),
            name: None,
            resolved_from: None,
            supersedes: supersedes.clone(),
            provenance: publisher.clone(),
            idp: identity::idp_profile(),
        };
        let sup_entry = supersedes.as_ref().and_then(|s| by_version.get(s));
        self.names
            .publish(
                ns, name, &vref, label, sup_entry, status, publisher, widening,
            )
            .map_err(|e| RegistryError::SchemaViolation {
                path: "name_history".to_string(),
                detail: format!("replay failed: {e:?}"),
            })
    }

    fn lineage_edges_json(&self) -> Vec<Json> {
        self.lineage
            .edges()
            .iter()
            .map(|e| {
                Json::obj([
                    ("type", Json::str("supersedes")),
                    ("newer", Json::str(e.newer.clone())),
                    ("older", Json::str(e.older.clone())),
                    ("reason", Json::str(reason_str(e.reason))),
                    ("ancestor_or_sibling", Json::Bool(true)),
                ])
            })
            .collect()
    }

    fn lineage_revocations_json(&self) -> Vec<Json> {
        self.lineage
            .revocations()
            .iter()
            .map(|r| {
                Json::obj([
                    ("type", Json::str("revocation")),
                    ("revokes", Json::str(r.revokes.clone())),
                    ("reason", Json::str(reason_str(r.reason))),
                    ("provenance", r.provenance.to_json()),
                    (
                        "replacement",
                        r.replacement.clone().map_or(Json::Null, Json::Str),
                    ),
                ])
            })
            .collect()
    }

    /// Append a `RegistryDiagnostic` + refused-admission event (R10 — failures are
    /// records, never silent) and return the error to the caller.
    fn fail(
        &mut self,
        operation: &str,
        subject: Option<String>,
        registrar: Option<&ProvenanceRecord>,
        e: RegistryError,
    ) -> RegistryError {
        let registrar_prov = registrar
            .cloned()
            .unwrap_or_else(|| ProvenanceRecord::kernel("registry", self.seq));
        self.diagnostics.push(RegistryDiagnostic {
            operation: operation.to_string(),
            reason: e.reason().to_string(),
            subject: subject.clone(),
            registrar: registrar_prov,
            seq: self.seq,
        });
        self.pending.push(RegistryEvent::refused(
            operation,
            e.reason(),
            subject.as_deref(),
        ));
        let _ = self.flush();
        e
    }

    fn ok_event(&mut self, ev: RegistryEvent) {
        self.pending.push(ev);
    }

    /// The pending audit rows, drained — append them through the run's single
    /// fenced writer with [`crate::events::emit`].
    pub fn drain_events(&mut self) -> Vec<RegistryEvent> {
        std::mem::take(&mut self.pending)
    }

    /// The recorded diagnostics (R10 — every refused operation is a record).
    pub fn diagnostics(&self) -> &[RegistryDiagnostic] {
        &self.diagnostics
    }

    /// The live `RegistryPolicy`.
    pub fn policy(&self) -> &RegistryPolicy {
        &self.policy
    }

    /// Replace the `RegistryPolicy` (MUST-data — layered; persisted as a `policy`
    /// line and pinned into every subsequent snapshot).
    pub fn set_policy(&mut self, policy: RegistryPolicy) -> Result<(), RegistryError> {
        self.policy = policy;
        self.flush()
    }

    /// The registered namespaces (`hh`, `local` at Stage 1).
    pub fn namespaces(&self) -> &BTreeMap<String, NamespaceRecord> {
        &self.namespaces
    }

    /// A record by `version_id`.
    pub fn get(&self, version_id: &str) -> Option<&(RegistryEnvelope, RegistryRecord)> {
        self.records.get(version_id)
    }

    /// `lookup(version_id)` — the direct-read verb (§5d.1): the stored
    /// envelope + body for an exact `version_id`, `Unresolved` for an unknown
    /// id (fail-closed — never an empty result). Reads are *not* resolution:
    /// a quarantined record is returned with its admission attached (a caller
    /// needing the governed path uses `resolve`). Never loads the
    /// implementation body (R3).
    pub fn lookup(&self, version_id: &str) -> Result<ResolvedRecord, RegistryError> {
        let (env, rec) = self
            .records
            .get(version_id)
            .ok_or_else(|| RegistryError::Unresolved {
                detail: format!("unregistered version {version_id}"),
            })?;
        // An `audit`-grade read — the same projection `resolve` uses, minus
        // the execute-mode admission gates.
        self.project(
            env,
            rec,
            None,
            ResolveMode::Audit,
            &ResolveRequest::default(),
        )
    }

    /// All registered `version_id`s.
    pub fn version_ids(&self) -> impl Iterator<Item = &String> {
        self.records.keys()
    }

    // ── register ─────────────────────────────────────────────────────────

    /// `register(record, registrar, trust_record_ref?) → VersionedRef` (spec §6.2).
    /// Validates the kind schema, variant-vs-`declaration_schema`, contract-range
    /// intersection, conditioned-rule completeness, the pinned implementation,
    /// the trust record where required and placement admissibility; idempotent on
    /// equal canonical bytes; **never loads `implementation.content`** (R3).
    pub fn register(
        &mut self,
        record: RegistryRecord,
        registrar: &ProvenanceRecord,
        trust_record_ref: Option<String>,
    ) -> Result<VersionedRef, RegistryError> {
        let op = "register";
        macro_rules! bail {
            ($e:expr) => {
                return Err(self.fail(op, None, Some(registrar), $e))
            };
        }
        // Registrar provenance must validate (R9 — ext never carries authority).
        if let Err(pe) = registrar.validate(None) {
            bail!(RegistryError::SchemaViolation {
                path: "registrar".to_string(),
                detail: format!("{pe:?}"),
            });
        }
        let kind = record.kind();
        if matches!(
            kind,
            RecordKind::RegistrySnapshot | RecordKind::NameHistoryEntry
        ) {
            bail!(RegistryError::SchemaViolation {
                path: "kind".to_string(),
                detail: format!(
                    "kind {} is produced by an operation, never registered",
                    kind.as_str()
                ),
            });
        }
        if !kind.has_stage1_schema() {
            bail!(RegistryError::SchemaViolation {
                path: "kind".to_string(),
                detail: format!("kind {} has no Stage-1 record schema", kind.as_str()),
            });
        }
        // Per-kind admission checks.
        match &record {
            RegistryRecord::Variant(v) => {
                if let Err(e) = self.check_variant(v, registrar, op) {
                    bail!(e);
                }
            }
            RegistryRecord::Suite(s) => {
                match self.records.get(&s.class_ref) {
                    Some((_, RegistryRecord::Class(c))) => {
                        if c.contract_version != s.contract_version {
                            bail!(RegistryError::ContractIncompatible {
                                detail: format!(
                                    "suite pinned to {} but class is {}",
                                    s.contract_version, c.contract_version
                                ),
                            });
                        }
                    }
                    _ => {
                        bail!(RegistryError::UnknownClass {
                            class_ref: s.class_ref.clone(),
                        });
                    }
                }
                let ids: BTreeSet<&String> = s.tests.iter().map(|t| &t.test_id).collect();
                if ids.len() != s.tests.len()
                    || !s.required_for_status.iter().all(|r| ids.contains(r))
                {
                    bail!(RegistryError::SchemaViolation {
                        path: "tests".to_string(),
                        detail: "duplicate test_id or required_for_status outside tests"
                            .to_string(),
                    });
                }
            }
            RegistryRecord::Report(r) => {
                for (field, id) in [("subject_ref", &r.subject_ref), ("suite_ref", &r.suite_ref)] {
                    if !self.records.contains_key(id) {
                        bail!(RegistryError::SchemaViolation {
                            path: field.to_string(),
                            detail: format!("unregistered pin: {id}"),
                        });
                    }
                }
            }
            RegistryRecord::Namespace(ns) => {
                if !matches!(ns.namespace.as_str(), "hh" | "local") {
                    bail!(RegistryError::SchemaViolation {
                        path: "namespace".to_string(),
                        detail: format!(
                            "Stage-1 namespaces are hh/local (exp/ and shared are later): {}",
                            ns.namespace
                        ),
                    });
                }
            }
            RegistryRecord::Class(c) => {
                if !c.required_inputs.contains("ModelProfile")
                    || !c.required_inputs.contains("ResourceAccount")
                {
                    bail!(RegistryError::SchemaViolation {
                        path: "required_inputs".to_string(),
                        detail: "must declare at least ModelProfile and ResourceAccount"
                            .to_string(),
                    });
                }
            }
            RegistryRecord::ForeignImport(_) | RegistryRecord::Snapshot(_) => {}
            RegistryRecord::Capability(c) => {
                if let Err(e) = check_capability(c) {
                    bail!(e);
                }
            }
            RegistryRecord::MetricDeclaration(m) => {
                // The declaration's own schema checks are the admission gate —
                // a metric admitting no detector/oracle, or a headline metric
                // admitting a non-deterministic oracle, never registers
                // (AC-R-2.9.2-13; ADR-0047 D2).
                if let Err(e) = m.validate() {
                    bail!(RegistryError::SchemaViolation {
                        path: "metric_declaration".to_string(),
                        detail: format!("{e:?}"),
                    });
                }
            }
            RegistryRecord::Validator(o) => {
                // ADR-0047's Stage-1 enforceable obligations (`judge ⇒
                // instrument`, `judge ⇒ ¬deterministic`, `reference_relative ⇒
                // three_valued`) refuse at registration, never at first use.
                if let Err(e) = o.validate() {
                    bail!(RegistryError::SchemaViolation {
                        path: "validator".to_string(),
                        detail: format!("{e:?}"),
                    });
                }
            }
            RegistryRecord::Extension(e) => {
                // §5g.5 L1–L3 admission gate: minted `text_authority` equality
                // (LocationElevation), credential-free locator, verified-status
                // consistency, claims-never-grants (LegCrossing).
                if let Err(e) = crate::extension::validate_extension_record(e) {
                    bail!(e);
                }
            }
        }
        // Trust record: mandatory for non-kernel/definition registrars.
        let first_party = matches!(
            registrar.authority,
            AuthorityClass::Kernel | AuthorityClass::Definition
        );
        if !first_party && trust_record_ref.is_none() {
            bail!(RegistryError::TrustRecordRequired);
        }
        // Identity + idempotence on equal canonical bytes (R6).
        let version_id = identity::version_id(&record);
        if let Some((existing_env, existing_rec)) = self.records.get(&version_id) {
            let same_body =
                schema::body_json(existing_rec, false) == schema::body_json(&record, false);
            if same_body && existing_env.kind == kind {
                return Ok(self.to_versioned_ref(existing_env, existing_rec));
            }
            bail!(RegistryError::SchemaViolation {
                path: "version_id".to_string(),
                detail: format!("conflicting body for {version_id}"),
            });
        }
        // Admission: never authored — assigned from the registrar's conferred
        // authority + policy (ADR-0063; `revoked` stays derived).
        let admission = if matches!(record, RegistryRecord::ForeignImport(_)) {
            self.policy.foreign_import_default_admission
        } else if let RegistryRecord::Capability(c) = &record {
            // ADR-0088 amendment / CF-210: a lifted source (`mcp_listing`,
            // `participant_supplied`) or a declaration carrying `effects` with
            // undeclared attribute vectors (`unknown_domain`) registers
            // `quarantined` — never model-visible until `seal`.
            if capability_needs_quarantine(c)
                || self.policy.require_signature_for_kinds.contains(&kind)
            {
                Admission::Quarantined
            } else if first_party {
                Admission::Resolved
            } else {
                Admission::Quarantined
            }
        } else if self.policy.require_signature_for_kinds.contains(&kind) {
            Admission::Quarantined
        } else if first_party {
            Admission::Resolved
        } else {
            Admission::Quarantined
        };
        let env = RegistryEnvelope {
            kind,
            version_id: version_id.clone(),
            semantic_id: identity::semantic_id(&record),
            registered_at: self.seq,
            registrar: registrar.clone(),
            admission,
            name_history_ref: None,
            trust_record_ref,
            dialect_range: match &record {
                RegistryRecord::Variant(v) => v.dialect_range.clone(),
                _ => "registry/1".to_string(),
            },
            ext: BTreeMap::new(),
        };
        if !dialect_covers(&env.dialect_range) {
            bail!(RegistryError::DialectIncompatible {
                dialect_range: env.dialect_range.clone(),
            });
        }
        for dep in record.referenced_version_ids() {
            self.lineage.declare_dependency(&version_id, &dep);
        }
        self.records
            .insert(version_id.clone(), (env.clone(), record.clone()));
        self.seq += 1;
        if let RegistryRecord::Capability(c) = &record {
            // ADR-0088 D8 / CF-327: capability registration emits the
            // kind-specific `lifecycle.capability.registered` class.
            let source_kind = match &c.node.semantic {
                hh_hir::records::KindRecord::ToolCapability(t) => {
                    hh_hir::tools::source_kind(&t.source).unwrap_or_default()
                }
                _ => String::new(),
            };
            self.ok_event(RegistryEvent::capability_registered(
                &version_id,
                env.semantic_id.as_deref(),
                &source_kind,
                registrar.origin.tag(),
            ));
        } else {
            self.ok_event(RegistryEvent::registered(
                kind.as_str(),
                &version_id,
                env.semantic_id.as_deref(),
                admission.as_str(),
                registrar.origin.tag(),
            ));
        }
        if let RegistryRecord::Report(r) = &record {
            self.ok_event(RegistryEvent::conformance(
                &r.report_id,
                &r.subject_ref,
                &r.suite_ref,
                r.produced_by.as_str(),
            ));
        }
        let (e, rec) = self.records.get(&version_id).unwrap();
        let vref = self.to_versioned_ref(e, rec);
        self.flush()?;
        Ok(vref)
    }

    /// Variant admission checks (register step — T-LCD-05, T-LCD-12, R7, CF-378).
    fn check_variant(
        &self,
        v: &VariantRecord,
        registrar: &ProvenanceRecord,
        _op: &str,
    ) -> Result<(), RegistryError> {
        let class = match self.records.get(&v.class_ref) {
            Some((_, RegistryRecord::Class(c))) => c,
            _ => {
                return Err(RegistryError::UnknownClass {
                    class_ref: v.class_ref.clone(),
                })
            }
        };
        if !range_covers(&v.contract_range, &class.contract_version) {
            return Err(RegistryError::ContractIncompatible {
                detail: format!(
                    "contract_range {} ∩ class.contract_version {} = ∅",
                    v.contract_range, class.contract_version
                ),
            });
        }
        if !dialect_covers(&v.dialect_range) {
            return Err(RegistryError::DialectIncompatible {
                dialect_range: v.dialect_range.clone(),
            });
        }
        // declaration_schema: the declared-field contract (required ⊆ declared;
        // additionalProperties=false refuses undeclared fields — R2's field axis).
        check_declaration(&class.declaration_schema, &v.capability_declaration)?;
        // Conditioned rules carry complete debt records (T-LCD-05).
        for (rule_id, debt) in &v.conditioned_rules {
            if rule_id.is_empty()
                || debt.rule_id.is_empty()
                || debt
                    .hypothesis
                    .content
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                || debt.owner.is_empty()
                || debt.expiry_condition.is_empty()
                || debt.removal_test_ref.is_empty()
            {
                return Err(RegistryError::ConditionedRuleIncomplete {
                    rule_id: rule_id.clone(),
                });
            }
        }
        // Placement admissibility (ADR-0153 D1): kernel/definition → all but the
        // reserved component_model; every other registrar is confined.
        let placement = v.implementation.placement;
        if placement == Placement::ComponentModel {
            return Err(RegistryError::LocalityInadmissible {
                origin: registrar.origin.tag().to_string(),
                placement: placement.as_str().to_string(),
            });
        }
        let first_party = matches!(
            registrar.authority,
            AuthorityClass::Kernel | AuthorityClass::Definition
        );
        if !first_party
            && !matches!(
                placement,
                Placement::SubprocessConfined | Placement::Container | Placement::Remote
            )
        {
            return Err(RegistryError::LocalityInadmissible {
                origin: registrar.origin.tag().to_string(),
                placement: placement.as_str().to_string(),
            });
        }
        Ok(())
    }

    fn to_versioned_ref(&self, env: &RegistryEnvelope, _rec: &RegistryRecord) -> VersionedRef {
        VersionedRef {
            kind: IdentityKind::RegistryRecord,
            version_id: env.version_id.clone(),
            semantic_id: env.semantic_id.clone(),
            name: None,
            resolved_from: None,
            supersedes: None,
            provenance: env.registrar.clone(),
            idp: identity::idp_profile(),
        }
    }

    // ── publish ──────────────────────────────────────────────────────────

    /// `publish(namespace, name, version_id, label?, supersedes?, registrar)` —
    /// appends a `name_history` entry through the one `NameIndex` (CC1). Enforces
    /// namespace ownership (`hh/` kernel-only, `local/` owners-or-principal),
    /// `(namespace, name)` ownership (a second publisher's name is
    /// `NameCollision`), label uniqueness, kind match and the widening rule; the
    /// Stage-1 `require_conformance` floor applies at publish (ADR-0152 D6).
    pub fn publish(
        &mut self,
        namespace: &str,
        name: &str,
        version_id: &str,
        label: Option<String>,
        supersedes: Option<String>,
        registrar: &ProvenanceRecord,
    ) -> Result<VersionedRef, RegistryError> {
        let op = "publish";
        let subject = Some(format!("{namespace}/{name}"));
        macro_rules! bail {
            ($e:expr) => {
                return Err(self.fail(op, subject.clone(), Some(registrar), $e))
            };
        }
        if let Err(pe) = registrar.validate(None) {
            bail!(RegistryError::SchemaViolation {
                path: "registrar".to_string(),
                detail: format!("{pe:?}"),
            });
        }
        let ns = match Namespace::parse(namespace) {
            Some(ns) => ns,
            None => bail!(RegistryError::NamespaceForbidden {
                namespace: namespace.to_string(),
            }),
        };
        let ns_record = match self.namespaces.get(ns.as_str()) {
            Some(n) => n.clone(),
            None => bail!(RegistryError::NamespaceForbidden {
                namespace: namespace.to_string(),
            }),
        };
        let (_e, rec) = match self.records.get(version_id) {
            Some(x) => x,
            None => bail!(RegistryError::UnknownVersion {
                version_id: version_id.to_string(),
            }),
        };
        let rec = rec.clone();
        // Namespace authority — `hh/` is kernel-owned (AC-8); `local/` admits the
        // namespace owner or a principal-authority registrar.
        let may = match ns_record.who_may_publish {
            PublishRule::KernelOnly => registrar.authority == AuthorityClass::Kernel,
            PublishRule::OwnersOrPrincipal => {
                matches!(
                    registrar.authority,
                    AuthorityClass::Kernel | AuthorityClass::Principal
                ) || is_namespace_owner(&ns_record, registrar)
            }
        };
        if !may {
            bail!(RegistryError::NamespaceForbidden {
                namespace: namespace.to_string(),
            });
        }
        // `(namespace, name)` ownership: an existing history belongs to its first
        // publisher — a different principal publishing under it is NameCollision
        // (AC-8: "a publish em nome de outra entidade falha").
        let history = self.names.history(ns, name);
        if let Some(first) = history.first() {
            let same_owner = publisher_key(&first.publisher) == publisher_key(registrar);
            if !same_owner && registrar.authority != AuthorityClass::Kernel {
                bail!(RegistryError::NameCollision {
                    namespace: namespace.to_string(),
                    name: name.to_string(),
                });
            }
        }
        // The supersedes pin must be a registered version of the same name's line.
        let sup_entry = match &supersedes {
            Some(s) => match history.iter().find(|e| e.version_id == *s) {
                Some(e) => Some((*e).clone()),
                None => bail!(RegistryError::UnknownVersion {
                    version_id: s.clone(),
                }),
            },
            None => None,
        };
        // The Stage-1 conformance floor at publish (ADR-0152 D6).
        if let RegistryRecord::Variant(v) = &rec {
            if let Err(e) =
                self.check_conformance_floor(v, version_id, self.policy.require_conformance)
            {
                bail!(e);
            }
        }
        // Widening successor: placement grows more privileged vs the superseded
        // version → requires `principal` + attestation (the NameIndex enforces).
        let widening = match (&supersedes, &rec) {
            (Some(s), RegistryRecord::Variant(v)) => match self.records.get(s) {
                Some((_, RegistryRecord::Variant(old))) => {
                    placement_rank(v.implementation.placement)
                        > placement_rank(old.implementation.placement)
                }
                _ => false,
            },
            _ => false,
        };
        let vref = VersionedRef {
            kind: IdentityKind::RegistryRecord,
            version_id: version_id.to_string(),
            semantic_id: identity::semantic_id(&rec),
            name: None,
            resolved_from: None,
            supersedes: supersedes.clone(),
            provenance: registrar.clone(),
            idp: identity::idp_profile(),
        };
        // The supersedes edge lands BEFORE the name append: a name binding must
        // never exist for a version whose succession edge is invalid (the edge
        // without the entry still tells the truth — the reverse is a lie).
        if let Some(s) = &supersedes {
            if s != version_id {
                if let Err(e) = self
                    .lineage
                    .supersede(version_id, s, SupersedeReason::Edit, true)
                {
                    bail!(match e {
                        SupersedeError::CycleDetected { newer, older } => {
                            RegistryError::CycleDetected { newer, older }
                        }
                        SupersedeError::NotAncestorOrSibling => {
                            RegistryError::NotAncestorOrSibling
                        }
                    });
                }
            }
        }
        let entry = match self.names.publish(
            ns,
            name,
            &vref,
            label.clone(),
            sup_entry.as_ref(),
            NameStatus::Active,
            registrar.clone(),
            widening,
        ) {
            Ok(e) => e,
            Err(PublishError::LabelReused { name, label }) => {
                bail!(RegistryError::LabelReused { name, label });
            }
            Err(PublishError::KindMismatch { expected, got }) => {
                bail!(RegistryError::KindMismatch {
                    detail: format!("expected {expected:?}, got {got:?}"),
                });
            }
            Err(PublishError::NamespaceForbidden { namespace }) => {
                bail!(RegistryError::NamespaceForbidden {
                    namespace: namespace.as_str().to_string(),
                });
            }
            Err(PublishError::AuthorityWideningRequiresHuman) => {
                bail!(RegistryError::AuthorityWideningRequiresHuman);
            }
        };
        // Persist the replayable line and update the envelope's name ref.
        self.name_lines.push(Json::obj([
            ("type", Json::str("name_history")),
            ("namespace", Json::str(ns.as_str())),
            ("name", Json::str(name)),
            ("version_id", Json::str(version_id)),
            ("label", label.clone().map_or(Json::Null, Json::Str)),
            ("status", Json::str("active")),
            (
                "supersedes",
                supersedes.clone().map_or(Json::Null, Json::Str),
            ),
            ("widening", Json::Bool(widening)),
            ("publisher", registrar.to_json()),
        ]));
        if let Some((env, _)) = self.records.get_mut(version_id) {
            env.name_history_ref = Some(format!("{ns_str}/{name}", ns_str = ns.as_str()));
        }
        self.ok_event(RegistryEvent::published(
            ns.as_str(),
            name,
            version_id,
            label.as_deref(),
            supersedes.as_deref(),
            registrar.origin.tag(),
        ));
        self.seq += 1;
        self.flush()?;
        Ok(self.to_versioned_ref_with_name(&entry))
    }

    fn to_versioned_ref_with_name(&self, e: &NameHistoryEntry) -> VersionedRef {
        VersionedRef {
            kind: e.kind,
            version_id: e.version_id.clone(),
            semantic_id: e.semantic_id.clone(),
            name: Some(hh_identity::refs::Name {
                namespace: e.namespace.as_str().to_string(),
                name: e.name.clone(),
                label: e.label.clone(),
            }),
            resolved_from: None,
            supersedes: e.supersedes_entry.clone(),
            provenance: e.publisher.clone(),
            idp: identity::idp_profile(),
        }
    }

    // ── resolve ──────────────────────────────────────────────────────────

    /// `resolve(selector|version_id, mode, requirements, registry_snapshot_id)` —
    /// deterministic for the same selector + snapshot + mode + requirements (R8);
    /// a pinned `version_id` resolves with its status attached in every mode;
    /// `execute` never returns a yanked head, a revoked version, a quarantined
    /// record or one below the conformance floor. **Never loads** the
    /// implementation body (R3).
    pub fn resolve(
        &self,
        input: &ResolveInput,
        mode: ResolveMode,
        requirements: &ResolveRequest,
    ) -> Result<ResolvedRecord, RegistryError> {
        let (version_id, name_entry) = match input {
            ResolveInput::Version(v) => (v.clone(), None),
            ResolveInput::Selector {
                namespace,
                name,
                label,
                snapshot_id,
            } => {
                if let Some(snap_id) = snapshot_id {
                    let snap = self.snapshot_by_id(snap_id)?;
                    let key = (namespace.clone(), name.clone());
                    let pinned =
                        snap.name_bindings
                            .get(&key)
                            .ok_or_else(|| RegistryError::Unresolved {
                                detail: format!("{namespace}/{name} outside snapshot {snap_id}"),
                            })?;
                    if !snap.members.contains(pinned) {
                        return Err(RegistryError::Unresolved {
                            detail: format!("binding {pinned} outside snapshot members"),
                        });
                    }
                    (pinned.clone(), None)
                } else {
                    let selector = NameSelector {
                        namespace: namespace.clone(),
                        name: name.clone(),
                        label: label.clone(),
                    };
                    match self.names.resolve(&selector, mode) {
                        ResolveOutcome::Resolved(vref) => {
                            // The entry the resolver actually landed on — the
                            // LATEST entry binding that version (an earlier
                            // active entry and a later yanked one can share a
                            // version_id; the status axis lives on the entry).
                            let entry = self
                                .names
                                .history(
                                    Namespace::parse(namespace).unwrap_or(Namespace::Local),
                                    name,
                                )
                                .iter()
                                .rev()
                                .find(|e| e.version_id == vref.version_id)
                                .map(|e| (*e).clone());
                            (vref.version_id.clone(), entry)
                        }
                        ResolveOutcome::Unresolved => {
                            return Err(RegistryError::Unresolved {
                                detail: format!("{namespace}/{name}"),
                            })
                        }
                        ResolveOutcome::Ambiguous { candidates } => {
                            return Err(RegistryError::Ambiguous { candidates })
                        }
                        ResolveOutcome::Revoked { version_id, reason } => {
                            return Err(RegistryError::Revoked { version_id, reason })
                        }
                    }
                }
            }
        };
        let (env, rec) =
            self.records
                .get(&version_id)
                .ok_or_else(|| RegistryError::Unresolved {
                    detail: format!("unregistered version {version_id}"),
                })?;
        self.project(env, rec, name_entry, mode, requirements)
    }

    /// The status/rules projection over a resolved record (the three axes +
    /// requirements + stale annotation).
    fn project(
        &self,
        env: &RegistryEnvelope,
        rec: &RegistryRecord,
        name_entry: Option<NameHistoryEntry>,
        mode: ResolveMode,
        requirements: &ResolveRequest,
    ) -> Result<ResolvedRecord, RegistryError> {
        let revoked = self.lineage.is_revoked(&env.version_id);
        let admission = if revoked {
            Admission::Revoked
        } else {
            env.admission
        };
        let stale = self.lineage.stale_for(&env.version_id).to_vec();
        if mode == ResolveMode::Execute {
            if revoked {
                let reason = self
                    .lineage
                    .revocations()
                    .iter()
                    .find(|r| r.revokes == env.version_id)
                    .map(|r| reason_str(r.reason).to_string())
                    .unwrap_or_default();
                return Err(RegistryError::Revoked {
                    version_id: env.version_id.clone(),
                    reason,
                });
            }
            if admission != Admission::Resolved && admission != Admission::Sealed {
                return Err(RegistryError::Unresolved {
                    detail: format!("admission {} is not executable", admission.as_str()),
                });
            }
            if !stale.is_empty() {
                return Err(RegistryError::Stale {
                    depends_on_revoked: stale.iter().map(|s| s.revoked_member.clone()).collect(),
                    detail: "depends on revoked members".to_string(),
                });
            }
        }
        // Requirements — class + contract version + conformance floor.
        if let RegistryRecord::Variant(v) = rec {
            if let Some(cid) = &requirements.class_id {
                let class_ok = self
                    .records
                    .get(&v.class_ref)
                    .map(|(_, r)| matches!(r, RegistryRecord::Class(c) if c.class_id == *cid))
                    .unwrap_or(false);
                if !class_ok {
                    return Err(RegistryError::ContractIncompatible {
                        detail: format!("variant is not of class {cid}"),
                    });
                }
            }
            if let Some(cv) = &requirements.contract_version {
                if !range_covers(&v.contract_range, cv) {
                    return Err(RegistryError::ContractIncompatible {
                        detail: format!(
                            "contract_version {cv} outside contract_range {}",
                            v.contract_range
                        ),
                    });
                }
            }
            let floor = requirements
                .conformance_floor
                .unwrap_or(self.policy.require_conformance);
            if mode == ResolveMode::Execute || floor == RequireConformance::Probed {
                self.check_conformance_floor(v, &env.version_id, floor)
                    .map_err(|e| match e {
                        RegistryError::ConformanceRequired { .. } => {
                            RegistryError::ConformanceBelowFloor {
                                vector: self.conformance_vector(v, &env.version_id),
                            }
                        }
                        other => other,
                    })?;
            }
        }
        let mut vref = self.to_versioned_ref(env, rec);
        if let Some(e) = &name_entry {
            vref = self.to_versioned_ref_with_name(e);
            vref.resolved_from = Some(NameSelector {
                namespace: e.namespace.as_str().to_string(),
                name: e.name.clone(),
                label: e.label.clone(),
            });
        }
        Ok(ResolvedRecord {
            versioned_ref: vref,
            record: rec.clone(),
            envelope: env.clone(),
            name_status: name_entry.as_ref().map(|e| e.status),
            name_entry,
            admission,
            depends_on_revoked: stale,
            conformance_vector: match rec {
                RegistryRecord::Variant(v) => self.conformance_vector(v, &env.version_id),
                _ => BTreeMap::new(),
            },
        })
    }

    /// The Stage-1 conformance floor (ADR-0152 D6): `declared` = a schema-valid
    /// `capability_declaration` exists (checked at `register`); `probed` = an
    /// admissible **non-stale** report covers the declaration (`publisher_claim`
    /// never satisfies; a stale report satisfies nothing → `SuiteStale`).
    fn check_conformance_floor(
        &self,
        v: &VariantRecord,
        version_id: &str,
        floor: RequireConformance,
    ) -> Result<(), RegistryError> {
        match floor {
            RequireConformance::None => Ok(()),
            RequireConformance::Declared => {
                if v.capability_declaration.is_empty() {
                    return Err(RegistryError::ConformanceRequired {
                        floor: "declared".to_string(),
                    });
                }
                Ok(())
            }
            RequireConformance::Probed => {
                let mut saw_stale_only = false;
                for (_env, rec) in self.records.values() {
                    let RegistryRecord::Report(r) = rec else {
                        continue;
                    };
                    if r.subject_ref != *version_id {
                        continue;
                    }
                    if !self
                        .policy
                        .admissible_report_producers
                        .contains(&r.produced_by)
                    {
                        continue; // publisher_claim never counts
                    }
                    if r.stale {
                        saw_stale_only = true;
                        continue;
                    }
                    return Ok(());
                }
                if saw_stale_only {
                    return Err(RegistryError::SuiteStale);
                }
                Err(RegistryError::ConformanceRequired {
                    floor: "probed".to_string(),
                })
            }
        }
    }

    /// The derived `variant_conformance_vector` — per declared field:
    /// `SUPPORTED` when an admissible non-stale report probed it so, `UNKNOWN`
    /// when only `declared` (never coerced — T-LCD-07).
    fn conformance_vector(
        &self,
        v: &VariantRecord,
        version_id: &str,
    ) -> BTreeMap<String, crate::kinds::ConformanceVerdict> {
        use crate::kinds::ConformanceVerdict as V;
        let mut out = BTreeMap::new();
        for field in v.capability_declaration.keys() {
            out.insert(field.clone(), V::Unknown);
        }
        for (_e, rec) in self.records.values() {
            let RegistryRecord::Report(r) = rec else {
                continue;
            };
            if r.subject_ref != *version_id
                || r.stale
                || !self
                    .policy
                    .admissible_report_producers
                    .contains(&r.produced_by)
            {
                continue;
            }
            for (field, verdict) in &r.probed_declaration {
                out.insert(field.clone(), *verdict);
            }
        }
        out
    }

    // ── deprecate / yank / revoke ────────────────────────────────────────

    /// `deprecate`/`yank` — append a status entry to the name history (the
    /// namespace policy's `who_may_*` rules; head fallback is `resolve`'s).
    pub fn set_name_status(
        &mut self,
        namespace: &str,
        name: &str,
        status: NameStatus,
        registrar: &ProvenanceRecord,
    ) -> Result<(), RegistryError> {
        let op = match status {
            NameStatus::Deprecated => "deprecate",
            NameStatus::Yanked => "yank",
            NameStatus::Active => "publish",
        };
        let subject = Some(format!("{namespace}/{name}"));
        macro_rules! bail {
            ($e:expr) => {
                return Err(self.fail(op, subject.clone(), Some(registrar), $e))
            };
        }
        let ns = Namespace::parse(namespace).ok_or(RegistryError::NamespaceForbidden {
            namespace: namespace.to_string(),
        })?;
        let ns_record = match self.namespaces.get(ns.as_str()) {
            Some(n) => n.clone(),
            None => bail!(RegistryError::NamespaceForbidden {
                namespace: namespace.to_string(),
            }),
        };
        let rule = match status {
            NameStatus::Deprecated => ns_record.who_may_deprecate,
            NameStatus::Yanked => ns_record.who_may_yank,
            NameStatus::Active => ns_record.who_may_publish,
        };
        let may = match rule {
            PublishRule::KernelOnly => registrar.authority == AuthorityClass::Kernel,
            PublishRule::OwnersOrPrincipal => {
                matches!(
                    registrar.authority,
                    AuthorityClass::Kernel | AuthorityClass::Principal
                ) || is_namespace_owner(&ns_record, registrar)
            }
        };
        if !may {
            bail!(RegistryError::AuthorityInsufficient {
                operation: op.to_string(),
            });
        }
        let history = self.names.history(ns, name);
        let head = match history.last() {
            Some(h) => (*h).clone(),
            None => bail!(RegistryError::Unresolved {
                detail: format!("{namespace}/{name}"),
            }),
        };
        let (_e, rec) = match self.records.get(&head.version_id) {
            Some(x) => x,
            None => bail!(RegistryError::UnknownVersion {
                version_id: head.version_id.clone(),
            }),
        };
        let rec = rec.clone();
        let vref = VersionedRef {
            kind: IdentityKind::RegistryRecord,
            version_id: head.version_id.clone(),
            semantic_id: identity::semantic_id(&rec),
            name: None,
            resolved_from: None,
            supersedes: None,
            provenance: registrar.clone(),
            idp: identity::idp_profile(),
        };
        self.names
            .publish(
                ns,
                name,
                &vref,
                None,
                Some(&head),
                status,
                registrar.clone(),
                false,
            )
            .map_err(|e| {
                self.fail(
                    op,
                    subject.clone(),
                    Some(registrar),
                    match e {
                        PublishError::LabelReused { name, label } => {
                            RegistryError::LabelReused { name, label }
                        }
                        _ => RegistryError::SchemaViolation {
                            path: "name_history".to_string(),
                            detail: format!("{e:?}"),
                        },
                    },
                )
            })?;
        self.name_lines.push(Json::obj([
            ("type", Json::str("name_history")),
            ("namespace", Json::str(ns.as_str())),
            ("name", Json::str(name)),
            ("version_id", Json::str(head.version_id.clone())),
            ("label", Json::Null),
            (
                "status",
                Json::str(match status {
                    NameStatus::Active => "active",
                    NameStatus::Deprecated => "deprecated",
                    NameStatus::Yanked => "yanked",
                }),
            ),
            ("supersedes", Json::Null),
            ("widening", Json::Bool(false)),
            ("publisher", registrar.to_json()),
        ]));
        self.ok_event(RegistryEvent::name_status(
            match status {
                NameStatus::Deprecated => crate::events::NAME_DEPRECATED,
                NameStatus::Yanked => crate::events::NAME_YANKED,
                NameStatus::Active => crate::events::PUBLISHED,
            },
            ns.as_str(),
            name,
            registrar.origin.tag(),
        ));
        self.seq += 1;
        self.flush()
    }

    /// `revoke(version_id, reason, registrar, replacement?)` — a `RevocationRecord`
    /// on the lineage, never an in-place flag (S1). `who_may_revoke` of the
    /// namespaces naming the version applies; revoking another publisher's
    /// version needs ≥ `principal` (AC-8).
    pub fn revoke(
        &mut self,
        version_id: &str,
        reason: SupersedeReason,
        registrar: &ProvenanceRecord,
        replacement: Option<String>,
    ) -> Result<(), RegistryError> {
        let op = "revoke";
        let subject = Some(version_id.to_string());
        macro_rules! bail {
            ($e:expr) => {
                return Err(self.fail(op, subject.clone(), Some(registrar), $e))
            };
        }
        let (env, _rec) = match self.records.get(version_id) {
            Some(x) => x.clone(),
            None => bail!(RegistryError::UnknownVersion {
                version_id: version_id.to_string(),
            }),
        };
        // The version's namespaces' revoke rules — hh/ is kernel-only; elsewhere
        // the registrar of the record or a ≥principal authority may revoke.
        let mut allowed = matches!(
            registrar.authority,
            AuthorityClass::Kernel | AuthorityClass::Principal
        );
        if !allowed {
            // Own record: same publisher key as the record's registrar or a name
            // publisher of it.
            allowed = publisher_key(&env.registrar) == publisher_key(registrar)
                || self.name_lines.iter().any(|l| {
                    l.get("version_id").and_then(|v| v.as_str()) == Some(version_id)
                        && l.get("publisher")
                            .and_then(|p| ProvenanceRecord::from_json(p).ok())
                            .map(|p| publisher_key(&p) == publisher_key(registrar))
                            .unwrap_or(false)
                });
        }
        if !allowed {
            bail!(RegistryError::AuthorityInsufficient {
                operation: op.to_string(),
            });
        }
        if self.lineage.is_revoked(version_id) {
            bail!(RegistryError::Revoked {
                version_id: version_id.to_string(),
                reason: reason_str(reason).to_string(),
            });
        }
        self.lineage.revoke(
            version_id,
            IdentityKind::RegistryRecord,
            reason,
            registrar.origin.clone(),
            replacement,
        );
        self.ok_event(RegistryEvent::revoked(
            version_id,
            reason_str(reason),
            registrar.origin.tag(),
        ));
        self.seq += 1;
        self.flush()
    }

    // ── snapshot ─────────────────────────────────────────────────────────

    /// `snapshot() → registry_snapshot_id` — an ADR-0038 closure: the member
    /// `version_id`s, the `(namespace, name) → version_id` bindings and the policy
    /// digest, minted `idp("registry.snapshot", …)` and stored as a record (R8;
    /// AC-1/AC-8 — the same store state always mints the same snapshot id).
    pub fn snapshot(
        &mut self,
        registrar: &ProvenanceRecord,
    ) -> Result<RegistrySnapshot, RegistryError> {
        let members: BTreeSet<String> = self.records.keys().cloned().collect();
        let mut name_bindings = BTreeMap::new();
        for l in &self.name_lines {
            let ns = l.get("namespace").and_then(|v| v.as_str()).unwrap_or("");
            let name = l.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if let ResolveOutcome::Resolved(vref) = self.names.resolve(
                &NameSelector {
                    namespace: ns.to_string(),
                    name: name.to_string(),
                    label: None,
                },
                ResolveMode::Audit,
            ) {
                name_bindings.insert((ns.to_string(), name.to_string()), vref.version_id);
            }
        }
        let policy_digest = hh_identity::idp::idp_id(
            "registry.policy",
            schema::policy_json(&self.policy)
                .to_canonical_string()
                .as_bytes(),
        );
        let snap = {
            let member_strs: Vec<String> = members.iter().cloned().collect();
            let mut probe = RegistrySnapshot {
                snapshot_id: String::new(),
                members,
                name_bindings,
                policy_digest,
                created_seq: self.seq,
            };
            probe.snapshot_id = identity::snapshot_id(
                &member_strs,
                &identity::binding_spellings(&probe),
                &probe.policy_digest,
                probe.created_seq,
            );
            probe
        };
        let snapshot_id = snap.snapshot_id.clone();
        // Idempotent: the same state mints the same snapshot — register once.
        if !self.records.contains_key(&snapshot_id) {
            let env = RegistryEnvelope {
                kind: RecordKind::RegistrySnapshot,
                version_id: snapshot_id.clone(),
                semantic_id: None,
                registered_at: self.seq,
                registrar: registrar.clone(),
                admission: Admission::Resolved,
                name_history_ref: None,
                trust_record_ref: None,
                dialect_range: "registry/1".to_string(),
                ext: BTreeMap::new(),
            };
            self.records.insert(
                snapshot_id.clone(),
                (env, RegistryRecord::Snapshot(snap.clone())),
            );
            self.ok_event(RegistryEvent::snapshotted(
                &snapshot_id,
                snap.members.len(),
                self.seq,
            ));
            self.seq += 1;
            self.flush()?;
        }
        Ok(snap)
    }

    fn snapshot_by_id(&self, snapshot_id: &str) -> Result<&RegistrySnapshot, RegistryError> {
        match self.records.get(snapshot_id) {
            Some((_, RegistryRecord::Snapshot(s))) => Ok(s),
            _ => Err(RegistryError::UnknownSnapshot {
                snapshot_id: snapshot_id.to_string(),
            }),
        }
    }

    // ── verify ───────────────────────────────────────────────────────────

    /// `verify()` — canonical content + snapshot integrity: every `version_id`
    /// re-mints from the body (idp verify), every `semantic_id` matches the
    /// declared projection, name histories bind registered versions, snapshot
    /// members resolve, and the log on disk matches the in-memory canonical form.
    pub fn verify(&self) -> VerifyReport {
        let mut errors = Vec::new();
        for (vid, (env, rec)) in &self.records {
            if env.version_id != *vid {
                errors.push(format!("envelope key != version_id for {vid}"));
            }
            if let Err(e) = identity::verify_body(rec, vid) {
                errors.push(format!("{vid}: {e}"));
            }
            if env.semantic_id != identity::semantic_id(rec) {
                errors.push(format!("{vid}: semantic_id divergence"));
            }
            if env.kind != rec.kind() {
                errors.push(format!("{vid}: envelope kind != body kind"));
            }
            if let RegistryRecord::Snapshot(s) = rec {
                for m in &s.members {
                    if !self.records.contains_key(m) && m != vid {
                        errors.push(format!("snapshot {vid}: member {m} unregistered"));
                    }
                }
            }
        }
        let mut name_entries_checked = 0;
        for l in &self.name_lines {
            name_entries_checked += 1;
            let vid = l.get("version_id").and_then(|v| v.as_str()).unwrap_or("");
            if !self.records.contains_key(vid) {
                errors.push(format!("name_history binds unregistered {vid}"));
            }
        }
        // Tamper check: the on-disk log must equal the canonical in-memory form.
        if let Ok(bytes) = fs::read(self.log_path()) {
            let mut fresh = RegistryStore {
                dir: self.dir.clone(),
                records: BTreeMap::new(),
                names: NameIndex::new(),
                lineage: Lineage::new(),
                namespaces: BTreeMap::new(),
                policy: RegistryPolicy::stage1_default(),
                diagnostics: Vec::new(),
                pending: Vec::new(),
                name_lines: Vec::new(),
                seq: 0,
            };
            match fresh.replay(&bytes) {
                Ok(()) => {
                    if fresh.records.len() != self.records.len() {
                        errors.push(format!(
                            "log record count {} != memory {}",
                            fresh.records.len(),
                            self.records.len()
                        ));
                    }
                    for (vid, (env, rec)) in &self.records {
                        match fresh.records.get(vid) {
                            Some((e2, r2))
                                if schema::envelope_json(e2) == schema::envelope_json(env)
                                    && schema::body_json(r2, false)
                                        == schema::body_json(rec, false) => {}
                            _ => errors.push(format!("{vid}: replayed bytes diverge from memory")),
                        }
                    }
                }
                Err(e) => errors.push(format!("log does not replay: {e}")),
            }
        }
        VerifyReport {
            checked: self.records.len(),
            name_entries_checked,
            errors,
        }
    }

    // ── lineage ──────────────────────────────────────────────────────────

    /// `lineage(version_id)` — the append-only identity/name/version
    /// relationships around one record: supersedes edges both ways, revocations,
    /// stale-by-dependency and the name-history entries that bound it.
    pub fn lineage(&self, version_id: &str) -> Result<LineageView, RegistryError> {
        if !self.records.contains_key(version_id) {
            return Err(RegistryError::UnknownVersion {
                version_id: version_id.to_string(),
            });
        }
        let mut supersedes = Vec::new();
        let mut superseded_by = Vec::new();
        for e in self.lineage.edges() {
            if e.newer == version_id {
                supersedes.push(e.older.clone());
            }
            if e.older == version_id {
                superseded_by.push(e.newer.clone());
            }
        }
        let revocations = self
            .lineage
            .revocations()
            .iter()
            .filter(|r| r.revokes == version_id)
            .cloned()
            .collect();
        let mut name_history = Vec::new();
        for ns in [Namespace::Hh, Namespace::Local] {
            for l in &self.name_lines {
                let lns = l.get("namespace").and_then(|v| v.as_str()).unwrap_or("");
                if lns != ns.as_str() {
                    continue;
                }
                let name = l.get("name").and_then(|v| v.as_str()).unwrap_or("");
                for e in self.names.history(ns, name) {
                    if e.version_id == version_id && !name_history.contains(e) {
                        name_history.push(e.clone());
                    }
                }
            }
        }
        Ok(LineageView {
            version_id: version_id.to_string(),
            supersedes,
            superseded_by,
            revocations,
            stale: self.lineage.stale_for(version_id).to_vec(),
            name_history,
        })
    }

    // ── query / slot_choices / substitutable (AC-1 projections) ─────────

    /// `query(predicate, registry_snapshot_id)` — the closed predicate over
    /// declared fields; results sorted by `version_id` (deterministic — R8).
    /// No free text, no relevance (the Stage-1 form AC-1 names).
    pub fn query(&self, predicate: &QueryPredicate) -> Result<Vec<ResolvedRecord>, RegistryError> {
        const FIELDS: [&str; 7] = [
            "kind",
            "class_id",
            "variant_id",
            "semantic_id",
            "admission",
            "name",
            "produced_by",
        ];
        for c in &predicate.clauses {
            if !FIELDS.contains(&c.field.as_str()) {
                return Err(RegistryError::UnknownField {
                    field: c.field.clone(),
                });
            }
        }
        let member_filter: Option<BTreeSet<String>> = match &predicate.snapshot_id {
            Some(id) => Some(self.snapshot_by_id(id)?.members.clone()),
            None => None,
        };
        let mut out = Vec::new();
        for (vid, (env, rec)) in &self.records {
            if let Some(m) = &member_filter {
                if !m.contains(vid) {
                    continue;
                }
            }
            if self.matches(env, rec, &predicate.clauses) {
                let revoked = self.lineage.is_revoked(vid);
                out.push(ResolvedRecord {
                    versioned_ref: self.to_versioned_ref(env, rec),
                    record: rec.clone(),
                    envelope: env.clone(),
                    name_entry: self.names.by_version_id(vid).cloned(),
                    admission: if revoked {
                        Admission::Revoked
                    } else {
                        env.admission
                    },
                    name_status: self.names.by_version_id(vid).map(|e| e.status),
                    depends_on_revoked: self.lineage.stale_for(vid).to_vec(),
                    conformance_vector: match rec {
                        RegistryRecord::Variant(v) => self.conformance_vector(v, vid),
                        _ => BTreeMap::new(),
                    },
                });
            }
        }
        out.sort_by(|a, b| a.envelope.version_id.cmp(&b.envelope.version_id));
        Ok(out)
    }

    fn matches(
        &self,
        env: &RegistryEnvelope,
        rec: &RegistryRecord,
        clauses: &[QueryClause],
    ) -> bool {
        clauses.iter().all(|c| {
            let value_matches = |candidate: &str| candidate == c.value;
            match (c.field.as_str(), c.op) {
                ("kind", QueryOp::Eq) => value_matches(rec.kind().as_str()),
                ("admission", QueryOp::Eq) => {
                    let eff = if self.lineage.is_revoked(&env.version_id) {
                        Admission::Revoked
                    } else {
                        env.admission
                    };
                    value_matches(eff.as_str())
                }
                ("semantic_id", QueryOp::Eq) => env
                    .semantic_id
                    .as_deref()
                    .map(value_matches)
                    .unwrap_or(false),
                ("name", QueryOp::Eq) => self
                    .names
                    .by_version_id(&env.version_id)
                    .map(|e| value_matches(&e.name))
                    .unwrap_or(false),
                ("class_id", QueryOp::Eq) => match rec {
                    RegistryRecord::Class(c) => value_matches(&c.class_id),
                    RegistryRecord::Variant(v) => self
                        .records
                        .get(&v.class_ref)
                        .map(
                            |(_, r)| matches!(r, RegistryRecord::Class(k) if k.class_id == c.value),
                        )
                        .unwrap_or(false),
                    _ => false,
                },
                ("variant_id", QueryOp::Eq) => match rec {
                    RegistryRecord::Variant(v) => value_matches(&v.variant_id),
                    _ => false,
                },
                ("produced_by", QueryOp::Eq) => match rec {
                    RegistryRecord::Report(r) => value_matches(r.produced_by.as_str()),
                    _ => false,
                },
                ("class_id", QueryOp::Contains) => match rec {
                    RegistryRecord::Variant(v) => self
                        .records
                        .get(&v.class_ref)
                        .map(
                            |(_, r)| matches!(r, RegistryRecord::Class(k) if k.class_id == c.value),
                        )
                        .unwrap_or(false),
                    _ => false,
                },
                _ => false,
            }
        })
    }

    /// `slot_choices(class_id, constraints, registry_snapshot_id)` — the resolved
    /// variants of a class satisfying the constraints, as `ComponentVariantRef`s
    /// sorted by `(variant_id, version_id)` (deterministic — R8; T-LCD-09/-14's
    /// sweepable/budget flags ride the projection).
    pub fn slot_choices(
        &self,
        class_id: &str,
        constraints: &SlotConstraints,
    ) -> Result<Vec<crate::records::ComponentVariantRef>, RegistryError> {
        let member_filter: Option<BTreeSet<String>> = match &constraints.snapshot_id {
            Some(id) => Some(self.snapshot_by_id(id)?.members.clone()),
            None => None,
        };
        let floor = constraints
            .conformance_floor
            .unwrap_or(self.policy.require_conformance);
        // Class without suite + a floor above none → SuiteMissing (ADR-0152).
        if floor != RequireConformance::None {
            let class_has_suite = self.records.values().any(|(_, r)| {
                matches!(r, RegistryRecord::Class(c)
                    if c.class_id == class_id && c.conformance_suite_ref.is_some())
            });
            if !class_has_suite {
                return Err(RegistryError::SuiteMissing {
                    class_id: class_id.to_string(),
                });
            }
        }
        let mut out = Vec::new();
        for (vid, (env, rec)) in &self.records {
            let RegistryRecord::Variant(v) = rec else {
                continue;
            };
            if let Some(m) = &member_filter {
                if !m.contains(vid) {
                    continue;
                }
            }
            let belongs = self
                .records
                .get(&v.class_ref)
                .map(|(_, r)| matches!(r, RegistryRecord::Class(c) if c.class_id == class_id))
                .unwrap_or(false);
            if !belongs {
                continue;
            }
            // Slot candidates must be executable: resolved admission, not revoked,
            // not stale.
            if env.admission != Admission::Resolved && env.admission != Admission::Sealed {
                continue;
            }
            if self.lineage.is_revoked(vid) || self.lineage.depends_on_revoked(vid) {
                continue;
            }
            if let Some(cv) = &constraints.contract_version {
                if !range_covers(&v.contract_range, cv) {
                    continue;
                }
            }
            let declared: BTreeSet<&String> = v
                .capability_declaration
                .iter()
                .filter(|(_, val)| **val != Json::Bool(false) && **val != Json::Null)
                .map(|(k, _)| k)
                .collect();
            if !constraints.supports.iter().all(|s| declared.contains(s)) {
                continue;
            }
            if self.check_conformance_floor(v, vid, floor).is_err() {
                continue;
            }
            let class_id_of = self
                .records
                .get(&v.class_ref)
                .and_then(|(_, r)| match r {
                    RegistryRecord::Class(c) => Some(c.class_id.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            let sweepable = v
                .param_schema
                .iter()
                .filter(|(_, p)| p.sweepable)
                .map(|(k, p)| (k.clone(), p.clone()))
                .collect();
            out.push(crate::records::ComponentVariantRef {
                variant_id: v.variant_id.clone(),
                class_id: class_id_of,
                version_id: vid.clone(),
                semantic_id: env.semantic_id.clone(),
                sweepable_params: sweepable,
            });
        }
        out.sort_by(|a, b| {
            (a.variant_id.clone(), a.version_id.clone())
                .cmp(&(b.variant_id.clone(), b.version_id.clone()))
        });
        Ok(out)
    }

    /// `substitutable(a, b)` — the minimal Stage-1 form: same class and `b`'s
    /// declared capabilities cover `a`'s (the vector/floor semantics are Stage 2 —
    /// ADR-0239). Pinned ids only.
    pub fn substitutable(&self, a: &str, b: &str) -> Result<bool, RegistryError> {
        let va = match self.records.get(a) {
            Some((_, RegistryRecord::Variant(v))) => v,
            _ => {
                return Err(RegistryError::UnknownVersion {
                    version_id: a.to_string(),
                })
            }
        };
        let vb = match self.records.get(b) {
            Some((_, RegistryRecord::Variant(v))) => v,
            _ => {
                return Err(RegistryError::UnknownVersion {
                    version_id: b.to_string(),
                })
            }
        };
        if va.class_ref != vb.class_ref {
            return Ok(false);
        }
        let covers = va
            .capability_declaration
            .iter()
            .all(|(k, val)| vb.capability_declaration.get(k) == Some(val));
        Ok(covers)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Free helpers
// ─────────────────────────────────────────────────────────────────────────────

fn str_of<'a>(j: &'a Json, k: &str) -> &'a str {
    j.get(k).and_then(|v| v.as_str()).unwrap_or("")
}

fn reason_str(r: SupersedeReason) -> &'static str {
    match r {
        SupersedeReason::Edit => "edit",
        SupersedeReason::Revocation => "revocation",
        SupersedeReason::Expiry => "expiry",
        SupersedeReason::Migration => "migration",
        SupersedeReason::Consolidation => "consolidation",
        SupersedeReason::Fork => "fork",
    }
}

fn parse_reason(s: &str) -> Result<SupersedeReason, RegistryError> {
    Ok(match s {
        "edit" => SupersedeReason::Edit,
        "revocation" => SupersedeReason::Revocation,
        "expiry" => SupersedeReason::Expiry,
        "migration" => SupersedeReason::Migration,
        "consolidation" => SupersedeReason::Consolidation,
        "fork" => SupersedeReason::Fork,
        other => {
            return Err(RegistryError::SchemaViolation {
                path: "reason".to_string(),
                detail: format!("closed vocabulary: {other}"),
            })
        }
    })
}

/// The stable identity of a publisher for the name-ownership check — the
/// canonical `origin` projection (author_ref+role for humans, component_ref for
/// kernel), never the whole provenance record (seq/scope are per-act).
fn publisher_key(p: &ProvenanceRecord) -> String {
    let j = p.to_json();
    let origin = j.get("origin").cloned().unwrap_or(Json::Null);
    origin.to_canonical_string()
}

fn is_namespace_owner(ns: &NamespaceRecord, registrar: &ProvenanceRecord) -> bool {
    let key = registrar_origin_tagged(registrar);
    ns.owners.iter().any(|o| match o {
        crate::kinds::OwnerRef::Principal(p) => *p == key,
        crate::kinds::OwnerRef::Signer(s) => *s == key,
    })
}

/// `author_ref`/`component_ref`-style coordinate for the ownership check.
fn registrar_origin_tagged(p: &ProvenanceRecord) -> String {
    match &p.origin {
        Origin::Human { author_ref, .. } => author_ref.clone(),
        Origin::Kernel { component_ref } => component_ref.clone(),
        Origin::Participant {
            participant_ref, ..
        } => participant_ref.clone(),
        _ => p.origin.tag().to_string(),
    }
}

/// Version/range containment: `*` covers all; `v` exact; `a-b` inclusive bounds;
/// `>=v` / `<=v` open bounds. Versions compare as numeric tuples (unknown shapes
/// compare lexically — deterministic).
pub(crate) fn range_covers(range: &str, version: &str) -> bool {
    let range = range.trim();
    if range == "*" || range.is_empty() {
        return true;
    }
    if let Some(v) = range.strip_prefix(">=") {
        return version_cmp(version, v.trim()) != std::cmp::Ordering::Less;
    }
    if let Some(v) = range.strip_prefix("<=") {
        return version_cmp(version, v.trim()) != std::cmp::Ordering::Greater;
    }
    if let Some((a, b)) = range.split_once('-') {
        return version_cmp(version, a.trim()) != std::cmp::Ordering::Less
            && version_cmp(version, b.trim()) != std::cmp::Ordering::Greater;
    }
    version_cmp(version, range) == std::cmp::Ordering::Equal
}

fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v')
            .split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    parse(a).cmp(&parse(b))
}

/// Whether a `dialect_range` admits `registry/1`.
fn dialect_covers(range: &str) -> bool {
    let normalized = range.trim().replace("registry/", "");
    range_covers(&normalized, "1")
}

/// The `declaration_schema` check — the declared-field contract
/// (`{required: [f…], properties: {f: …}, additionalProperties: bool}`):
/// required ⊆ declared keys; `additionalProperties: false` refuses undeclared
/// fields (the R2 closed-vocabulary rule on the field axis).
fn check_declaration(
    schema: &Json,
    declaration: &BTreeMap<String, Json>,
) -> Result<(), RegistryError> {
    if let Json::Obj(s) = schema {
        if let Some(Json::Arr(req)) = s.get("required") {
            for r in req {
                if let Json::Str(f) = r {
                    if !declaration.contains_key(f) {
                        return Err(RegistryError::SchemaViolation {
                            path: format!("capability_declaration.{f}"),
                            detail: "required by class declaration_schema".to_string(),
                        });
                    }
                }
            }
        }
        if s.get("additionalProperties") == Some(&Json::Bool(false)) {
            let allowed: BTreeSet<&String> = match s.get("properties") {
                Some(Json::Obj(p)) => p.keys().collect(),
                _ => BTreeSet::new(),
            };
            for k in declaration.keys() {
                if !allowed.contains(k) {
                    return Err(RegistryError::SchemaViolation {
                        path: format!("capability_declaration.{k}"),
                        detail: "not in class declaration_schema".to_string(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Placement privilege rank for the widening-successor check — more local is
/// more privileged (`in_process` is the top of the lattice).
/// The capability admission checks (§5d.1 §3 — V-E1 at `register`, ADR-0088
/// D1's `ValidationErrors`): the node must be a `tool_capability` whose own
/// provenance validates, and `hh_hir::tools::validate_capability` must be
/// clean. Failures are collected, never fail-fast.
fn check_capability(c: &CapabilityRecord) -> Result<(), RegistryError> {
    if c.node.kind != hh_hir::kinds::EntityKind::ToolCapability {
        return Err(RegistryError::SchemaViolation {
            path: "kind".to_string(),
            detail: format!(
                "capability record must be a tool_capability node, not {}",
                c.node.kind.name()
            ),
        });
    }
    if let Err(pe) = c.node.provenance.validate(None) {
        return Err(RegistryError::SchemaViolation {
            path: "record.provenance".to_string(),
            detail: format!("{pe:?}"),
        });
    }
    let hh_hir::records::KindRecord::ToolCapability(t) = &c.node.semantic else {
        return Err(RegistryError::SchemaViolation {
            path: "semantic".to_string(),
            detail: "node kind/semantic mismatch".to_string(),
        });
    };
    let violations = hh_hir::tools::validate_capability(t, c.node.provenance.authority);
    if !violations.is_empty() {
        return Err(RegistryError::CapabilityValidation { violations });
    }
    Ok(())
}

/// Whether a capability record must register `quarantined` (ADR-0088
/// amendment; CF-210): a lifted source (`mcp_listing`/`participant_supplied`),
/// or a `declared` effect set carrying members with no attribute vector (the
/// `unknown_domain` honesty form — `effects = unknown` at the record level).
fn capability_needs_quarantine(c: &CapabilityRecord) -> bool {
    let hh_hir::records::KindRecord::ToolCapability(t) = &c.node.semantic else {
        return false;
    };
    if hh_hir::tools::lifted_source_kind(&t.source).is_some() {
        return true;
    }
    if let hh_hir::kinds::ToolEffects::Declared(set) = &t.effects {
        if set.iter().any(|e| e.attributes.is_none()) {
            return true;
        }
    }
    false
}

fn placement_rank(p: Placement) -> u8 {
    match p {
        Placement::Remote => 0,
        Placement::Container => 1,
        Placement::SubprocessConfined => 2,
        Placement::InProcess => 3,
        Placement::ComponentModel => 4,
    }
}
