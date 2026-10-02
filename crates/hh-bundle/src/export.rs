//! `LedgerExport` assembly — the chunked canonical event tree over a
//! run's durable prefix (§5h.3 §3 layer P; CF-299). Pages are canonical
//! `Json` documents (`{page, seq_lo, seq_hi, events[]}`), each content-
//! addressed; the `tree` is `idp/1` over the ordered page-address list —
//! the one tree rule (R-ID-3), so Merkle partial verification and
//! header-first reads stay possible.

use std::collections::BTreeMap;

use hh_ledger::event::EventEnvelope;
use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::error::BundleError;
use crate::manifest::{BlobIndexEntry, LedgerExport};

/// Events per `LedgerExport` page (64 — header-first reads stay cheap;
/// the constant is part of the exporter's contract, not the schema's).
pub const LEDGER_PAGE_SIZE: u64 = 64;

/// A member the assembler produced: content-addressed bytes keyed by
/// their `idp/1` address.
pub type MemberBytes = BTreeMap<String, Vec<u8>>;

/// Content-address `bytes` and record them in `members`.
pub fn add_member(members: &mut MemberBytes, bytes: Vec<u8>, media_type: &str) -> (String, u64) {
    let addr = hh_identity::address(&bytes, media_type);
    let size = bytes.len() as u64;
    members.insert(addr.id(), bytes);
    (addr.id(), size)
}

/// `envelope → canonical page-document bytes`.
fn page_bytes(page_no: u64, events: &[EventEnvelope]) -> Vec<u8> {
    let doc = Json::obj([
        ("page", Json::Int(page_no as i64)),
        (
            "seq_lo",
            Json::Int(events.first().map(|e| e.seq).unwrap_or(0) as i64),
        ),
        (
            "seq_hi",
            Json::Int(events.last().map(|e| e.seq).unwrap_or(0) as i64),
        ),
        (
            "events",
            Json::Arr(events.iter().map(|e| e.to_json()).collect()),
        ),
    ]);
    doc.to_canonical_string().into_bytes()
}

/// Decode a page document back to envelopes (validation/import).
pub fn decode_page(bytes: &[u8]) -> Result<Vec<EventEnvelope>, BundleError> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| BundleError::Malformed {
        detail: "ledger page is not utf-8".into(),
    })?;
    let doc = hh_wire::json::parse(&text).map_err(|e| BundleError::Malformed {
        detail: format!("ledger page parse: {e}"),
    })?;
    match doc.get("events") {
        Some(Json::Arr(evs)) => evs
            .iter()
            .map(EventEnvelope::from_json)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| BundleError::Malformed {
                detail: format!("ledger page envelope: {e}"),
            }),
        _ => Err(BundleError::Malformed {
            detail: "ledger page has no events[]".into(),
        }),
    }
}

/// A ledger member's manifest-index row: `(role, member_ref, size)` —
/// the caller appends each as a `MemberRef` (`ledger_page:{run}:{i}`,
/// `ledger_tree:{run}`).
pub type LedgerMemberRow = (String, String, u64);

/// Build the `LedgerExport` for `run_id` — appending the page members to
/// `members` and returning the export record plus the page `MemberRef`
/// roles (the caller adds them to the manifest's member index).
pub fn build_ledger_export(
    store: &Store,
    run_id: &str,
    members: &mut MemberBytes,
) -> Result<(LedgerExport, Vec<LedgerMemberRow>), BundleError> {
    let events = store
        .envelopes(run_id)
        .map_err(|_| BundleError::RunNotFound {
            run_id: run_id.to_string(),
        })?;
    let mut pages = Vec::new();
    let mut roles = Vec::new();
    for (i, chunk) in events.chunks(LEDGER_PAGE_SIZE as usize).enumerate() {
        let bytes = page_bytes(i as u64, chunk);
        let (addr, size) = add_member(members, bytes, "application/vnd.hh.ledger-page+json");
        pages.push(addr.clone());
        roles.push((format!("ledger_page:{run_id}:{i}"), addr, size));
    }
    let tree = LedgerExport::tree_address(&pages);
    // The tree itself is a member — header-first readers pull it before
    // any page.
    let tree_doc = Json::obj([
        ("run_id", Json::str(run_id)),
        (
            "pages",
            Json::Arr(pages.iter().map(|p| Json::str(p.clone())).collect()),
        ),
    ]);
    let (tree_addr, tree_size) = add_member(
        members,
        tree_doc.to_canonical_string().into_bytes(),
        "application/vnd.hh.ledger-tree+json",
    );
    // Note: `tree_addr` (the tree *document's* content address) and
    // `tree` (the tree *coordinate* — `idp/1` over the bare
    // page-address list, R-ID-3) are different preimages by design; the
    // member index carries both spellings.
    roles.push((format!("ledger_tree:{run_id}"), tree_addr, tree_size));

    // Blob index — every `refs[]` address with its store status.
    let mut blob_index = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for ev in events {
        for r in &ev.refs {
            if seen.insert(r.id()) {
                let (status, size) = match store.get_blob(r) {
                    Ok(b) => ("present".to_string(), b.len() as u64),
                    Err(e) => {
                        let s = format!("{e:?}").to_lowercase();
                        (
                            if s.contains("redact") {
                                "redacted"
                            } else {
                                "gc"
                            }
                            .to_string(),
                            0u64,
                        )
                    }
                };
                blob_index.push(BlobIndexEntry {
                    address: r.id(),
                    status,
                    size,
                });
            }
        }
    }

    // Checkpoints — `security.audit.checkpoint` durable rows, as their
    // EventRef coordinates.
    let checkpoints: Vec<Json> = events
        .iter()
        .filter(|e| e.class == "security.audit.checkpoint")
        .map(|e| {
            Json::obj([
                ("run_id", Json::str(run_id)),
                ("event_id", Json::str(e.event_id.clone())),
                ("seq", Json::Int(e.seq as i64)),
            ])
        })
        .collect();

    // `audit_tree_head` — the newest *signed* `security.audit.checkpoint`
    // claim, ledgered as the bundle's attestation member (ADR-0038;
    // ADR-0067 D5; R-2.8.6¹): the bundle attests to the writer's own
    // signed claim — `checkpoint_ref` + `{tree_size, tree_head,
    // signatures}` verbatim — never a head it computed itself. A run with
    // no signed checkpoint carries `None`, an honest absence.
    let audit_tree_head = events
        .iter()
        .rev()
        .filter(|e| e.class == "security.audit.checkpoint")
        .find_map(|e| {
            let claim = hh_ledger::tree::parse_checkpoint(&e.payload)?;
            if claim.signatures.is_empty() {
                return None;
            }
            let checkpoint_ref = Json::obj([
                ("run_id", Json::str(run_id)),
                ("event_id", Json::str(e.event_id.clone())),
                ("seq", Json::Int(e.seq as i64)),
            ]);
            crate::manifest::AuditTreeHead::from_claim(checkpoint_ref, &claim)
        });

    let head = store.head(run_id).map_err(|_| BundleError::RunNotFound {
        run_id: run_id.to_string(),
    })?;
    let head_json = Json::obj([
        ("seq", Json::Int(head.seq as i64)),
        ("event_id", Json::str(head.event_id.clone())),
        ("hash", Json::str(head.hash.clone())),
    ]);

    // Lineage prefixes — the manifest's fork/continuation anchors.
    let manifest = store
        .manifest(run_id)
        .map_err(|_| BundleError::RunNotFound {
            run_id: run_id.to_string(),
        })?;
    let mut lineage_prefixes = Vec::new();
    for link in [&manifest.forked_from, &manifest.continued_from]
        .into_iter()
        .flatten()
    {
        lineage_prefixes.push(Json::obj([
            ("run_id", Json::str(link.run_id.clone())),
            ("up_to_seq", Json::Int(link.at_seq as i64)),
            ("head_hash", Json::str(link.head_hash.clone())),
        ]));
    }

    Ok((
        LedgerExport {
            run_id: run_id.to_string(),
            page_size: LEDGER_PAGE_SIZE,
            pages,
            tree,
            checkpoints,
            blob_index,
            head: head_json,
            lineage_prefixes,
            audit_tree_head,
        },
        roles,
    ))
}

// ── `export(bundle, target, policy)` — the publication lowering (§5h.3
// §2; ADR-0141 D4/D5; S4.2, AC-R-2.9.3-8/-10) ──────────────────────────

/// `PublicationPolicy{readers, content_classes ⊆ {accounting,
/// structural, content, diagnostic}, redaction, payload_protection ∈
/// {none, restricted_store}, loss_report}` (§5h.3 §2 publication note).
/// `content_classes` empty = every class ships; a listed subset withholds
/// the rest (each withheld member lands in the loss report, never
/// silently dropped — CC3).
#[derive(Debug, Clone, PartialEq)]
pub struct PublicationPolicy {
    /// The audience the export is cut for.
    pub readers: Vec<String>,
    /// The content classes admitted (empty = all).
    pub content_classes: Vec<String>,
    /// A declared redaction policy ref.
    pub redaction: Option<String>,
    /// `none | restricted_store` — `restricted_store` withholds every
    /// `content`-class member (L2 defaults under a contamination policy).
    pub payload_protection: String,
}

impl Default for PublicationPolicy {
    fn default() -> Self {
        PublicationPolicy {
            readers: vec![],
            content_classes: vec![],
            redaction: None,
            payload_protection: "none".to_string(),
        }
    }
}

impl PublicationPolicy {
    /// The per-suite **public** default (R-2.10.5 C2; §6.5 §4): a
    /// `public` read set, `restricted_store` payload protection and the
    /// `content` class withheld — the exported view carries accounting,
    /// structural and diagnostic members; every withheld member lands in
    /// the loss report (never silently dropped — CC3).
    pub fn public() -> Self {
        PublicationPolicy {
            readers: vec!["public".to_string()],
            content_classes: vec![
                "accounting".to_string(),
                "structural".to_string(),
                "diagnostic".to_string(),
            ],
            redaction: None,
            payload_protection: "restricted_store".to_string(),
        }
    }
}

impl PublicationPolicy {
    /// Decode from boundary params (`policy` member of `kernel.export`).
    pub fn from_json(j: Option<&Json>) -> Result<PublicationPolicy, BundleError> {
        let mut p = PublicationPolicy::default();
        let Some(j) = j else { return Ok(p) };
        let bad = |d: &str| BundleError::Malformed {
            detail: format!("PublicationPolicy: {d}"),
        };
        if let Some(Json::Arr(rs)) = j.get("readers") {
            p.readers = rs
                .iter()
                .filter_map(|r| r.as_str().map(String::from))
                .collect();
        }
        if let Some(Json::Arr(cs)) = j.get("content_classes") {
            for c in cs {
                let s = c.as_str().ok_or(bad("content_classes"))?;
                match s {
                    "accounting" | "structural" | "content" | "diagnostic" => {
                        p.content_classes.push(s.to_string())
                    }
                    other => return Err(bad(&format!("content_class {other}"))),
                }
            }
        }
        p.redaction = j.get("redaction").and_then(Json::as_str).map(String::from);
        if let Some(pp) = j.get("payload_protection").and_then(Json::as_str) {
            match pp {
                "none" | "restricted_store" => p.payload_protection = pp.to_string(),
                other => return Err(bad(&format!("payload_protection {other}"))),
            }
        }
        Ok(p)
    }

    /// Whether a member's content class ships under this policy.
    pub fn admits(&self, class: &str) -> bool {
        if self.payload_protection == "restricted_store" && class == "content" {
            return false;
        }
        self.content_classes.is_empty() || self.content_classes.iter().any(|c| c == class)
    }
}

/// The `export` artefact: the produced file tree (the caller writes or
/// ships it), the policy-applied manifest + member set (the reader-side
/// bundle), the `LoweringLossReport`, and the artefact's content
/// address.
pub struct ExportOutcome {
    /// The artefact's content address (`idp/1` over the file tree digest).
    pub artefact: String,
    /// The artefact's file tree (`path → bytes`) — `directory` layout for
    /// `ledger_native`, the foreign layout otherwise.
    pub files: BTreeMap<String, Vec<u8>>,
    /// The policy-applied manifest (withheld members `redacted`, the
    /// `derived.redaction`/`loss` records attached) — the reader's bundle.
    pub manifest: crate::manifest::BundleManifest,
    /// The policy-applied member bytes.
    pub members: MemberBytes,
    /// `LoweringLossReport{entries[{member, class}], withheld[]}`.
    pub loss_report: Json,
    /// The export's granularity ceiling (`run` for `ledger_native`,
    /// `product` for the foreign targets — OQ-030's interchange
    /// deferral, ADR-0213).
    pub granularity_ceiling: String,
    /// The subject runs a `measurement.export.delivered` row lands on.
    pub delivered_runs: Vec<String>,
}

/// The class a member role belongs to (one scheme — `fetch::content_class`).
fn class_of(role: &str) -> &'static str {
    crate::fetch::content_class(role)
}

/// Apply a `PublicationPolicy` to a manifest + member set: withheld
/// members flip `status → redacted` on the *export copy* (the source
/// manifest is never mutated — I1), their bytes leave the tree, and each
/// earns a `no_slot` loss row.
fn apply_policy(
    manifest: &crate::manifest::BundleManifest,
    members: &MemberBytes,
    policy: &PublicationPolicy,
) -> (crate::manifest::BundleManifest, MemberBytes, Vec<Json>) {
    let mut out_manifest = manifest.clone();
    let mut out_members = members.clone();
    let mut loss: Vec<Json> = Vec::new();
    for m in out_manifest.members.iter_mut() {
        let class = class_of(&m.role);
        if policy.admits(class) {
            continue;
        }
        if m.status == crate::manifest::MemberStatus::Present {
            m.status = crate::manifest::MemberStatus::Redacted;
            out_members.remove(&m.address);
        }
        loss.push(Json::obj([
            ("member", Json::str(m.role.clone())),
            ("address", Json::str(m.address.clone())),
            ("class", Json::str("no_slot")),
            (
                "detail",
                Json::str(format!("withheld by policy (class {class})")),
            ),
        ]));
    }
    // The withholding is recorded on the export copy (the `redacted`
    // statuses + `derived.loss_report` — never silently absent).
    if !loss.is_empty() {
        let mut comp = match out_manifest.composition.clone() {
            Json::Obj(m) => m,
            _ => BTreeMap::new(),
        };
        comp.insert("loss_report".into(), Json::Arr(loss.clone()));
        out_manifest.composition = Json::Obj(comp);
        out_manifest.version_id = String::new();
        out_manifest.version_id = out_manifest.compute_id();
    }
    (out_manifest, out_members, loss)
}

/// `export(bundle, target, policy)` — produce the artefact for `target`
/// (`ledger_native | harbor_job_dir | interchange_trajectory`; the
/// remaining targets land with their owning tickets — `FormatUnknown`).
/// Every target is a lowering; only `ledger_native` is lossless. The
/// artefact digest is `idp/1` over the canonical file-tree document.
pub fn export_target(
    decoded: &crate::codec::Decoded,
    target: &str,
    policy: &PublicationPolicy,
) -> Result<ExportOutcome, BundleError> {
    let (manifest, members, mut loss) = apply_policy(&decoded.manifest, &decoded.members, policy);
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    match target {
        "ledger_native" => {
            // Lossless modulo the policy — the artefact is the encoded
            // container bytes plus the manifest document.
            files.insert(
                "bundle.hhb1".into(),
                crate::codec::encode_container(&manifest, &members),
            );
            files.insert(
                "bundle.json".into(),
                manifest.to_json().to_canonical_string().into_bytes(),
            );
        }
        "harbor_job_dir" => {
            // Harbor shape: `job.json` + `trajectories/<run>.jsonl` (the
            // ledger events as canonical JSONL) + `manifest.json`. Every
            // member the format cannot express lists a loss row.
            let mut runs = Vec::new();
            for (run, export) in &manifest.traces {
                let mut lines = String::new();
                for page_addr in &export.pages {
                    if let Some(bytes) = members.get(page_addr) {
                        for ev in decode_page(bytes)? {
                            lines.push_str(&ev.to_json().to_canonical_string());
                            lines.push('\n');
                        }
                    }
                }
                let path = format!("trajectories/{run}.jsonl");
                files.insert(path.clone(), lines.into_bytes());
                runs.push(Json::obj([
                    ("run_id", Json::str(run.clone())),
                    ("trajectory", Json::str(path)),
                    // Lineage-aware export (R-2.12.1¹): fork/continuation
                    // prefixes and the signed audit head ride the foreign
                    // row — ancestry is data, never a dropped member.
                    (
                        "lineage_prefixes",
                        Json::Arr(export.lineage_prefixes.clone()),
                    ),
                    (
                        "audit_tree_head",
                        export
                            .audit_tree_head
                            .as_ref()
                            .map(|h| h.to_json())
                            .unwrap_or(Json::Null),
                    ),
                ]));
            }
            let job = Json::obj([
                ("schema", Json::str("harbor_job/1")),
                ("job_id", Json::str(manifest.version_id.clone())),
                ("runs", Json::Arr(runs)),
                ("subject_lineage", Json::Arr(manifest.subject.lineage.clone())),
            ]);
            files.insert(
                "job.json".into(),
                job.to_canonical_string().into_bytes(),
            );
            files.insert(
                "manifest.json".into(),
                manifest.to_json().to_canonical_string().into_bytes(),
            );
            // Foreign digest claims — the bytes' own digests, so an
            // importer can verify (AC-R-2.9.3-8's "foreign digests verify
            // against the exported bytes").
            let digests = crate::fetch::digest_index(&files);
            files.insert(
                "digests.json".into(),
                Json::Obj(
                    digests
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v.clone())))
                        .collect(),
                )
                .to_canonical_string()
                .into_bytes(),
            );
            // Loss rows for members the job format has no slot for
            // (definition/compiled/models — the trajectory is the only
            // native-shaped payload Harbor carries).
            for m in &manifest.members {
                let already = loss.iter().any(|e| {
                    e.get("member").and_then(Json::as_str) == Some(m.role.as_str())
                });
                if already {
                    continue;
                }
                let class = match m.role.as_str() {
                    r if r.starts_with("ledger_page:") || r.starts_with("ledger_tree:") => {
                        continue; // expressed as the trajectory.
                    }
                    "subject" | "model" => "hint_only",
                    _ => "untyped_slot",
                };
                loss.push(Json::obj([
                    ("member", Json::str(m.role.clone())),
                    ("address", Json::str(m.address.clone())),
                    ("class", Json::str(class)),
                    (
                        "detail",
                        Json::str("no harbor_job_dir slot — the manifest carries the ref"),
                    ),
                ]));
            }
        }
        "interchange_trajectory" => {
            // OQ-030 undecided (ADR-0213 deferral): the interchange
            // degrades to `product` granularity with every non-trajectory
            // member listed in the loss report.
            let mut lines = String::new();
            let mut run_events: BTreeMap<String, Vec<EventEnvelope>> = BTreeMap::new();
            for (run, export) in &manifest.traces {
                for page_addr in &export.pages {
                    if let Some(bytes) = members.get(page_addr) {
                        for ev in decode_page(bytes)? {
                            let row = Json::obj([
                                ("run_id", Json::str(run.clone())),
                                ("event", ev.to_json()),
                            ]);
                            lines.push_str(&row.to_canonical_string());
                            lines.push('\n');
                            run_events.entry(run.clone()).or_default().push(ev);
                        }
                    }
                }
            }
            files.insert("trajectory.jsonl".into(), lines.into_bytes());
            // `cost_view` fields in the header (R-2.9.1¹ — "interchange
            // export of `cost_view` fields with unit conversion declared
            // lossy"): the projection's exact per-currency micro-unit
            // totals stay verbatim; `total_spend` is the *converted* whole-
            // unit figure the interchange schema carries — truncation is
            // declared per run in the loss report, never silent (CC3).
            let mut cost_views = BTreeMap::new();
            for (run, events) in &run_events {
                let cv = hh_telemetry::views::cost_view(run, run, events, None);
                let micro = cv
                    .payload
                    .get("total_spend_micro")
                    .cloned()
                    .unwrap_or(Json::obj([]));
                let mut units = BTreeMap::new();
                if let Json::Obj(m) = &micro {
                    for (ccy, v) in m {
                        if let Some(micro_units) = v.as_int() {
                            units.insert(
                                ccy.clone(),
                                Json::Int(micro_units / 1_000_000),
                            );
                        }
                    }
                }
                cost_views.insert(
                    run.clone(),
                    Json::obj([
                        ("total_spend_micro", micro),
                        ("total_spend", Json::Obj(units)),
                        (
                            "total_rows",
                            cv.payload
                                .get("total_rows")
                                .cloned()
                                .unwrap_or(Json::Int(0)),
                        ),
                        (
                            "watermark_seq",
                            cv.derived_from_seq
                                .map(|w| Json::Int(w as i64))
                                .unwrap_or(Json::Null),
                        ),
                    ]),
                );
                loss.push(Json::obj([
                    ("member", Json::str(format!("cost_view:{run}"))),
                    ("address", Json::str("")),
                    ("class", Json::str("unit_conversion")),
                    (
                        "detail",
                        Json::str(
                            "total_spend converts micro-units → units by truncation; \
                             total_spend_micro is the exact source",
                        ),
                    ),
                ]));
            }
            // Lineage-aware export (R-2.12.1¹): the subject lineage and each
            // run's ledger lineage prefixes ride the header — a forked or
            // continued run's ancestry is part of the product row, never
            // dropped by the lowering.
            let mut lineage_by_run = BTreeMap::new();
            for (run, export) in &manifest.traces {
                if !export.lineage_prefixes.is_empty() {
                    lineage_by_run.insert(
                        run.clone(),
                        Json::Arr(export.lineage_prefixes.clone()),
                    );
                }
            }
            let header = Json::obj([
                ("schema", Json::str("hh-interchange-trajectory/1")),
                ("bundle_id", Json::str(manifest.version_id.clone())),
                ("granularity", Json::str("product")),
                (
                    "subject_runs",
                    Json::Arr(manifest.subject.run_ids.iter().map(Json::str).collect()),
                ),
                ("subject_lineage", Json::Arr(manifest.subject.lineage.clone())),
                ("lineage_prefixes", Json::Obj(lineage_by_run)),
                ("cost_views", Json::Obj(cost_views)),
            ]);
            files.insert(
                "interchange.json".into(),
                header.to_canonical_string().into_bytes(),
            );
            for m in &manifest.members {
                let already = loss.iter().any(|e| {
                    e.get("member").and_then(Json::as_str) == Some(m.role.as_str())
                });
                if already || m.role.starts_with("ledger_page:") {
                    continue;
                }
                loss.push(Json::obj([
                    ("member", Json::str(m.role.clone())),
                    ("address", Json::str(m.address.clone())),
                    ("class", Json::str("untyped_slot")),
                    (
                        "detail",
                        Json::str("OQ-030: interchange carries the product-granularity minimum set only"),
                    ),
                ]));
            }
        }
        other => {
            return Err(BundleError::FormatUnknown {
                detail: format!(
                    "export target `{other}` — supported: ledger_native, harbor_job_dir, interchange_trajectory"
                ),
            })
        }
    }
    let artefact = hh_identity::idp_id(
        "bundle.export",
        Json::Obj(
            crate::fetch::digest_index(&files)
                .into_iter()
                .map(|(k, v)| (k, Json::str(v)))
                .collect(),
        )
        .to_canonical_string()
        .as_bytes(),
    );
    let loss_report = Json::obj([
        ("schema", Json::str("hh-lowering-loss/1")),
        ("bundle_id", Json::str(decoded.manifest.version_id.clone())),
        ("target", Json::str(target)),
        ("entries", Json::Arr(loss)),
    ]);
    Ok(ExportOutcome {
        artefact,
        files,
        manifest,
        members,
        loss_report,
        granularity_ceiling: match target {
            "ledger_native" => "run",
            _ => "product",
        }
        .to_string(),
        delivered_runs: decoded.manifest.subject.run_ids.clone(),
    })
}

// ── training_export/1 (S6.4; §5h.8 §2.1; ADR-0325) ────────────────────────────

/// A `hh_lab::coevolution::CoEvolutionError` → the boundary's typed
/// refusal (`CoevolutionRefused` carries the closed code verbatim —
/// `HeldOutInExport`, `InadmissibleRewardSource`, `ReaderViolation`,
/// `BundleNotValid`, …).
fn coe_err(e: hh_lab::coevolution::CoEvolutionError) -> BundleError {
    BundleError::CoevolutionRefused {
        reason: e.code(),
    }
}

/// `export_training(bundle, TrainingExportPolicy, ctx) → ExportOutcome`
/// — the `training_export/1` lowering target (§5h.8 §2.1; S6.4;
/// ADR-0325). A *projection* of the bundle's ledger exports — the same
/// bundle + policy + ctx ⇒ the byte-identical artefact (E-1).
///
/// - **E-0/E-2** — a ledger page the manifest names but the member set
///   lacks is `BundleNotValid`; a task the ctx's `task_splits` labels
///   `held_out`/`private` refuses `HeldOutInExport` before any sample
///   projects.
/// - **E-4** — `policy.readers ⊄ ctx.corpus_readers` refuses
///   `ReaderViolation`.
/// - **E-5/E-7** — every unavailable/narrowed member is a typed loss
///   row in `loss_report` (and the export record's own `loss[]`).
/// - **`files`** — `training_export.json` (the record), `samples.jsonl`
///   (canonical one-sample-per-line), `exposure_record.json` (the
///   `TrainingExposureRecord`), `manifest.json`, `loss_report.json`.
///
/// `granularity_ceiling = "model_call"` — the projection reads
/// `model.call.*`/`tool.call.*`/oracle rows; token-level data is never
/// fabricated (E-6). `delivered_runs` are the manifest's subject runs
/// (the caller mints `measurement.export.delivered` per subject run).
pub fn export_training(
    decoded: &crate::codec::Decoded,
    policy: &hh_lab::coevolution::TrainingExportPolicy,
    ctx: &hh_lab::coevolution::TrainingExportCtx,
) -> Result<ExportOutcome, BundleError> {
    use hh_lab::coevolution as coe;

    let mut samples = Vec::new();
    let mut loss: Vec<coe::ExportLossEntry> = Vec::new();
    let mut data_refs = Vec::new();
    let mut reward_prov = Vec::new();
    for (run, export) in &decoded.manifest.traces {
        let mut envelopes = Vec::new();
        for page_addr in &export.pages {
            let bytes =
                decoded
                    .members
                    .get(page_addr)
                    .ok_or_else(|| BundleError::CoevolutionRefused {
                        reason: format!("BundleNotValid: ledger page {page_addr} absent"),
                    })?;
            for ev in decode_page(bytes)? {
                envelopes.push(ev.to_json());
            }
        }
        let (exp, l) = coe::project_training_export(run, &envelopes, policy, ctx)
            .map_err(coe_err)?;
        samples.extend(exp.samples);
        loss.extend(l);
        data_refs.extend(exp.exposure.data_refs);
        reward_prov.extend(exp.exposure.reward_provenance);
    }
    let mut record = coe::TrainingExport {
        export_id: String::new(),
        policy: policy.clone(),
        samples,
        exposure: coe::TrainingExposureRecord {
            export_id: String::new(),
            data_refs,
            reward_provenance: reward_prov,
            provenance: ctx.provenance.clone(),
        },
        loss: loss.clone(),
        harness_constraints: ctx.harness_constraints.clone(),
        provenance: ctx.provenance.clone(),
    };
    record.seal();

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert(
        "training_export.json".into(),
        record.to_json().to_canonical_string().into_bytes(),
    );
    let mut lines = String::new();
    for s in &record.samples {
        lines.push_str(&s.to_json().to_canonical_string());
        lines.push('\n');
    }
    files.insert("samples.jsonl".into(), lines.into_bytes());
    files.insert(
        "exposure_record.json".into(),
        record
            .exposure
            .to_json()
            .to_canonical_string()
            .into_bytes(),
    );
    files.insert(
        "manifest.json".into(),
        decoded
            .manifest
            .to_json()
            .to_canonical_string()
            .into_bytes(),
    );
    let loss_json: Vec<Json> = loss.iter().map(coe::ExportLossEntry::to_json).collect();
    let loss_report = Json::obj([
        ("schema", Json::str("hh-lowering-loss/1")),
        ("bundle_id", Json::str(decoded.manifest.version_id.clone())),
        ("target", Json::str("training_export")),
        ("entries", Json::Arr(loss_json)),
    ]);
    files.insert(
        "loss_report.json".into(),
        loss_report.to_canonical_string().into_bytes(),
    );
    let artefact = hh_identity::idp_id(
        "bundle.export",
        Json::Obj(
            crate::fetch::digest_index(&files)
                .into_iter()
                .map(|(k, v)| (k, Json::str(v)))
                .collect(),
        )
        .to_canonical_string()
        .as_bytes(),
    );
    Ok(ExportOutcome {
        artefact,
        files,
        manifest: decoded.manifest.clone(),
        members: decoded.members.clone(),
        loss_report,
        granularity_ceiling: "model_call".to_string(),
        delivered_runs: decoded.manifest.subject.run_ids.clone(),
    })
}
