//! `closure_walk` / `closure_report` — the owner-typed recipient
//! closure interface (spec §6.2; PADR-R5-3∗ C.1 items 3–4; R5.22).
//!
//! The walk traverses claim/procedure roots through native
//! heads/manifests/pages/causes/lineage, ordinary refs, IR refs and
//! owner-typed payload refs. The **owning codecs enumerate
//! references** — the manifest member index, `section_member_refs`,
//! the decoded `RunManifest`'s `manifest_refs` projection, each
//! `LedgerExport`'s tree/pages/`blob_index`/head/lineage, the decoded
//! `EventEnvelope`'s `causes`/`refs`/`ir_refs` and the class table's
//! declared `content_refs` — never a hash regex over payload bytes,
//! never a second class table (C.10). Every enumerated edge carries
//! `{parent, role, field, media, size}` provenance plus its owner;
//! visited dedup keys on exactly that tuple, and explicit
//! [`ClosureLimits`] bound edges, manifest-nesting depth and decoded
//! events — an exhausted limit fails closure, never passes
//! partially.
//!
//! Every required edge resolves to verified member bytes or an
//! explicit `fetch`/`redacted`/`gc`: `present` requires bytes in the
//! recipient's own `Decoded.members` recomputing to the address —
//! a producer assertion is never evidence (§6.2). `fetch` members
//! materialize through the landed [`crate::fetch::fetch`] only when
//! the caller supplies a [`FetchPermit`] — exact identity, admitted
//! locator/trust/expiry, verified arrival, never executed — else
//! they withhold; `redacted`/`gc` withhold as recorded losses.
//! There is no producer home/store/cache/path/credential fallback:
//! the walk's whole world is `&Decoded` plus the caller-supplied
//! fetch seams (§6.2, C.5).
//!
//! The report is a versioned sidecar-class record
//! (`closure-report/1`): pure over bundle bytes plus permitted fetch
//! and never joined to `max_supported_level` — a sidecar cannot
//! raise a level (C.1 item 4).

use std::collections::{BTreeMap, BTreeSet};

use hharness_ledger::classes;
use hharness_ledger::event::EventEnvelope;
use hharness_wire::json::Json;

use crate::codec::Decoded;
use crate::export::MemberBytes;
use crate::fetch::{FetchOutcome, Fetcher, LocationFetcher};
use crate::manifest::{LedgerExport, MemberStatus};
use crate::profile::{self, EvidenceProfile};
use crate::trust::FetchTrustPolicy;

/// The report schema id (C.1 item 4's versioned sidecar report).
pub const CLOSURE_REPORT_SCHEMA: &str = "closure-report/1";

/// The closed failure-code set a report's `failures[]`/unresolved
/// edges carry — §6.2's typed failure vocabulary plus the landed
/// fetch arms (`fetch`'s own `failed[]` reasons ride through
/// verbatim).
pub mod code {
    /// A required ref names no member, blob-index row, carried bytes,
    /// decoded event or included run.
    pub const MISSING_MEMBER: &str = "missing_member";
    /// `present` declared but the recipient carries no bytes.
    pub const MISSING_BYTES: &str = "missing_bytes";
    /// Carried bytes do not recompute to the address.
    pub const DIGEST_MISMATCH: &str = "digest_mismatch";
    /// Carried bytes disagree with the declared member size.
    pub const BAD_SIZE: &str = "bad_size";
    /// A run coordinate names no included trace.
    pub const MISSING_RUN: &str = "missing_run";
    /// An event/seq/hash coordinate cannot be verified against the
    /// decoded prefix.
    pub const UNRESOLVED_COORD: &str = "unresolved_coordinate";
    /// A declared head/tree/hash disagrees with the decoded record.
    pub const HEAD_MISMATCH: &str = "head_mismatch";
    /// A member required for traversal does not decode (page,
    /// manifest, profile member).
    pub const MALFORMED: &str = "malformed_member";
    /// A decoded event's class is not in the registered class table —
    /// its payload-ref partition is unknowable.
    pub const UNKNOWN_CLASS: &str = "unknown_class";
    /// A required record names a schema/major this codec does not
    /// admit (unknown required profile version, `hh-bundle/2`…).
    pub const UNSUPPORTED_SCHEMA: &str = "unsupported_schema";
    /// An explicit `ClosureLimits` bound was exhausted.
    pub const TRAVERSAL_LIMIT: &str = "traversal_limit";
    /// An edge re-enters a node still on the expansion stack.
    pub const CYCLE: &str = "cycle";
}

/// The explicit traversal bounds — a finite walk, never ambient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosureLimits {
    /// Dedup'd edges enumerated before `traversal_limit` fails.
    pub max_edges: usize,
    /// `contains:` manifest-nesting depth before `traversal_limit`.
    pub max_depth: usize,
    /// Ledger events decoded across all traces before
    /// `traversal_limit`.
    pub max_events: usize,
}

impl Default for ClosureLimits {
    fn default() -> Self {
        ClosureLimits {
            max_edges: 8192,
            max_depth: 8,
            max_events: 1 << 20,
        }
    }
}

/// The caller's materialization permit — `None` means every `fetch`
/// member withholds (`fetch_declared`), never silently fetched.
pub struct FetchPermit<'a> {
    /// The blob-pool fetcher (pool/local `locations[]`).
    pub pool: Option<&'a Fetcher<'a>>,
    /// The declared remote-admission policy.
    pub trust: Option<&'a FetchTrustPolicy>,
    /// The remote transport (`https:` TLS GET seam).
    pub remote: Option<&'a mut LocationFetcher<'a>>,
    /// The evaluation clock for `expires`/signature arms (ms).
    pub now_ms: u64,
}

/// The walk's options.
pub struct WalkOpts<'a> {
    /// The recipient procedure label the report records (`inspect`,
    /// `verify`, …) — a label, never a dispatch.
    pub procedure: &'a str,
    /// The recipient class the verdict is scoped to.
    pub recipient_class: &'a str,
    /// The recipient-side timestamp the report records.
    pub checked_at: &'a str,
    /// Traversal bounds.
    pub limits: ClosureLimits,
    /// Materialization permit; `None` withholds all `fetch` members.
    pub fetch: Option<FetchPermit<'a>>,
}

/// One enumerated reference — `{parent, role, field, media, size}`
/// provenance plus owner, kind, target and its resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosureEdge {
    /// The node that declared the edge (`manifest`, `profile`,
    /// `trace:<run>`, `event:<run>:<event_id>`, `contains:<id>`).
    pub parent: String,
    /// The member role / edge role (`ledger_page:<run>:<i>`,
    /// `definition`, `cause`, `ir_ref`, …).
    pub role: String,
    /// The declaring field (`members[3]`, `traces.<run>.pages[0]`,
    /// `claims[0].native_refs[1]`, `ev.causes[2]`).
    pub field: String,
    /// The member media type when the index declares one.
    pub media: String,
    /// The declared byte size when the index carries one.
    pub size: Option<u64>,
    /// The enumerating owner (`hh-bundle.manifest`,
    /// `evidence-package/1`, `hh-ledger.envelope:<class>`,
    /// `hh-ledger.class_table:<class>`, `hh-ledger.blob_index`,
    /// `hh-bundle.run_refs`).
    pub owner: String,
    /// The edge class (`member`, `section`, `ordinary`, `manifest`,
    /// `trace`, `tree`, `page`, `blob`, `head`, `checkpoint`,
    /// `lineage`, `cause`, `payload`, `ir`, `declared`).
    pub kind: &'static str,
    /// The reference target (content address, `run_id:event_id`,
    /// `run_id@seq`, or a declared ref string).
    pub target: String,
    /// The expected coordinate payload for tree/head/lineage edges,
    /// and the declaring index's blob status for `blob` edges.
    pub expect: Option<Json>,
    /// The declaring member index's status when the edge was
    /// enumerated from a `MemberRef` row — the resolution consults it
    /// before any outer-index lookup so a `contains:` manifest's own
    /// statuses withhold truthfully.
    pub declared_status: Option<MemberStatus>,
    /// The outcome.
    pub resolution: Resolution,
}

/// An edge's truthful outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// Verified: `bytes` — recipient member bytes recompute to the
    /// address; `fetch` — materialized + verified through
    /// `fetch::fetch`; `coordinate` — the event/run/seq/hash exists
    /// in the decoded prefix.
    Resolved { via: &'static str },
    /// An explicit loss the manifest records: `fetch` (declared, no
    /// permit), `redacted`, `gc` — withheld, never fabricated.
    Withheld { status: &'static str },
    /// No truthful resolution — the closed [`code`] set or a landed
    /// `fetch::fetch` failure reason carried verbatim.
    Unresolved { code: String },
}

/// `closure_report/1` — `{bundle_id, profile_version, procedure,
/// recipient_class, checked_at, required, resolved, withheld,
/// unresolved, verdict, failures[]}` plus the edge enumeration and
/// the verbatim remote-attempt records (C.1 item 4).
#[derive(Debug, Clone, PartialEq)]
pub struct ClosureReport {
    /// The bundle's `version_id`.
    pub bundle_id: String,
    /// The decoded `evidence-package/<n>` major — `None` when the
    /// manifest carries no profile.
    pub profile_version: Option<u64>,
    /// The recorded procedure label.
    pub procedure: String,
    /// The recorded recipient class.
    pub recipient_class: String,
    /// The recorded recipient-side timestamp.
    pub checked_at: String,
    /// Dedup'd edges enumerated.
    pub required: usize,
    /// Edges resolved to verified bytes or coordinates.
    pub resolved: usize,
    /// Edges resolved as explicit `fetch`/`redacted`/`gc` losses.
    pub withheld: usize,
    /// Edges with no truthful resolution.
    pub unresolved: usize,
    /// `closed` — every edge byte/coordinate-verified; `withheld` —
    /// no unresolved, explicit losses present; `failed` — an
    /// unresolved edge or a traversal failure (limit, cycle,
    /// unknown schema/class, malformed required member).
    pub verdict: &'static str,
    /// Traversal-level failure rows (`{code, detail}`).
    pub failures: Vec<Json>,
    /// The dedup'd edge enumeration with resolutions.
    pub edges: Vec<ClosureEdge>,
    /// The `bundle.member.fetched` payload rows remote attempts
    /// produced — verbatim; the caller lands them durable before the
    /// member reads `present` (`require_ledgered_arrival`).
    pub fetched: Vec<Json>,
}

impl ClosureReport {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let res = |r: &Resolution| match r {
            Resolution::Resolved { via } => {
                Json::obj([("state", Json::str("resolved")), ("via", Json::str(*via))])
            }
            Resolution::Withheld { status } => Json::obj([
                ("state", Json::str("withheld")),
                ("status", Json::str(*status)),
            ]),
            Resolution::Unresolved { code } => Json::obj([
                ("state", Json::str("unresolved")),
                ("code", Json::str(code.clone())),
            ]),
        };
        let edge = |e: &ClosureEdge| {
            let mut m = vec![
                ("parent", Json::str(e.parent.clone())),
                ("role", Json::str(e.role.clone())),
                ("field", Json::str(e.field.clone())),
                ("media", Json::str(e.media.clone())),
                ("owner", Json::str(e.owner.clone())),
                ("kind", Json::str(e.kind)),
                ("target", Json::str(e.target.clone())),
                ("resolution", res(&e.resolution)),
            ];
            if let Some(s) = e.size {
                m.push(("size", Json::Int(s as i64)));
            }
            Json::obj(m)
        };
        Json::obj([
            ("schema", Json::str(CLOSURE_REPORT_SCHEMA)),
            ("bundle_id", Json::str(self.bundle_id.clone())),
            (
                "profile_version",
                self.profile_version
                    .map(|v| Json::Int(v as i64))
                    .unwrap_or(Json::Null),
            ),
            ("procedure", Json::str(self.procedure.clone())),
            ("recipient_class", Json::str(self.recipient_class.clone())),
            ("checked_at", Json::str(self.checked_at.clone())),
            ("required", Json::Int(self.required as i64)),
            ("resolved", Json::Int(self.resolved as i64)),
            ("withheld", Json::Int(self.withheld as i64)),
            ("unresolved", Json::Int(self.unresolved as i64)),
            ("verdict", Json::str(self.verdict)),
            ("failures", Json::Arr(self.failures.clone())),
            ("edges", Json::Arr(self.edges.iter().map(edge).collect())),
            ("fetched", Json::Arr(self.fetched.clone())),
        ])
    }
}

// ── the walk ───────────────────────────────────────────────────────

struct Walk<'a> {
    d: &'a Decoded,
    lim: &'a ClosureLimits,
    edges: Vec<ClosureEdge>,
    seen: BTreeSet<(String, String, String, String, u64)>,
    runs_expanded: BTreeSet<String>,
    manifests_expanded: BTreeSet<String>,
    visiting: Vec<String>,
    events: BTreeMap<String, Vec<EventEnvelope>>,
    n_events: usize,
    failures: Vec<Json>,
    over: bool,
}

impl<'a> Walk<'a> {
    fn fail(&mut self, code: &'static str, detail: impl Into<String>) {
        self.failures.push(Json::obj([
            ("code", Json::str(code)),
            ("detail", Json::str(detail.into())),
        ]));
    }

    #[allow(clippy::too_many_arguments)]
    fn edge(
        &mut self,
        parent: &str,
        role: impl Into<String>,
        field: impl Into<String>,
        media: &str,
        size: Option<u64>,
        owner: &str,
        kind: &'static str,
        target: impl Into<String>,
        expect: Option<Json>,
    ) {
        self.edge_status(
            parent, role, field, media, size, owner, kind, target, expect, None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn edge_status(
        &mut self,
        parent: &str,
        role: impl Into<String>,
        field: impl Into<String>,
        media: &str,
        size: Option<u64>,
        owner: &str,
        kind: &'static str,
        target: impl Into<String>,
        expect: Option<Json>,
        declared_status: Option<MemberStatus>,
    ) {
        if self.over {
            return;
        }
        if self.edges.len() >= self.lim.max_edges {
            self.over = true;
            self.fail(
                code::TRAVERSAL_LIMIT,
                format!("edge bound {} exhausted", self.lim.max_edges),
            );
            return;
        }
        let (role, field, target) = (role.into(), field.into(), target.into());
        let key = (
            parent.to_string(),
            role.clone(),
            field.clone(),
            media.to_string(),
            size.unwrap_or(0),
        );
        if self.seen.insert(key) {
            self.edges.push(ClosureEdge {
                parent: parent.to_string(),
                role,
                field,
                media: media.to_string(),
                size,
                owner: owner.to_string(),
                kind,
                target,
                expect,
                declared_status,
                resolution: Resolution::Unresolved {
                    code: code::MISSING_MEMBER.into(),
                },
            });
        }
    }

    /// Decode every materialized page of `export` into `events[run]`
    /// (bounded by `max_events`); page members that cannot decode are
    /// closure failures — the traversal into them cannot complete.
    fn decode_pages(&mut self, export: &LedgerExport) {
        let run = export.run_id.clone();
        let mut out = self.events.remove(&run).unwrap_or_default();
        for (i, addr) in export.pages.iter().enumerate() {
            let Some(bytes) = self.d.members.get(addr) else {
                continue; // the page member edge reports the loss.
            };
            match crate::export::decode_page(bytes) {
                Ok(evs) => {
                    self.n_events += evs.len();
                    if self.n_events > self.lim.max_events {
                        self.over = true;
                        self.fail(
                            code::TRAVERSAL_LIMIT,
                            format!("event bound {} exhausted", self.lim.max_events),
                        );
                        return;
                    }
                    out.extend(evs);
                }
                Err(e) => self.fail(
                    code::MALFORMED,
                    format!("traces.{run}.pages[{i}] does not decode: {e}"),
                ),
            }
        }
        self.events.insert(run, out);
    }

    /// `traces.<run>` — the manifest/page/cause/lineage root class.
    /// `index` is the *declaring* manifest's member index (the outer
    /// manifest's for a top-level run, the contained manifest's for a
    /// nested trace): member statuses resolve against their own
    /// index, never the parent's.
    fn expand_export(
        &mut self,
        ex: &LedgerExport,
        index: &[crate::manifest::MemberRef],
        _depth: usize,
    ) {
        let run = ex.run_id.clone();
        let node = format!("trace:{run}");
        if self.visiting.contains(&node) {
            self.fail(code::CYCLE, format!("{node} re-enters the expansion stack"));
            return;
        }
        if !self.runs_expanded.insert(run.clone()) {
            return;
        }
        self.visiting.push(node.clone());
        {
            self.edge_status(
                &format!("trace:{run}"),
                format!("ledger_tree:{run}"),
                format!("traces.{run}.tree"),
                "",
                None,
                "hh-bundle.manifest",
                "tree",
                ex.tree.clone(),
                Some(Json::Arr(
                    ex.pages.iter().map(|p| Json::str(p.clone())).collect(),
                )),
                index
                    .iter()
                    .find(|mm| mm.address == ex.tree)
                    .map(|mm| mm.status),
            );
            for (i, p) in ex.pages.iter().enumerate() {
                let row = index.iter().find(|mm| mm.address == *p);
                let (role, media) = row
                    .map(|mm| (mm.role.clone(), mm.media_type.clone()))
                    .unwrap_or_else(|| (format!("ledger_page:{run}:{i}"), String::new()));
                self.edge_status(
                    &format!("trace:{run}"),
                    role,
                    format!("traces.{run}.pages[{i}]"),
                    &media,
                    row.map(|mm| mm.size),
                    "hh-bundle.manifest",
                    "page",
                    p.clone(),
                    None,
                    row.map(|mm| mm.status),
                );
            }
            for (i, b) in ex.blob_index.iter().enumerate() {
                self.edge(
                    &format!("trace:{run}"),
                    format!("blob:{run}"),
                    format!("traces.{run}.blob_index[{i}]"),
                    "",
                    Some(b.size),
                    "hh-ledger.blob_index",
                    "blob",
                    b.address.clone(),
                    Some(Json::obj([("status", Json::str(b.status.clone()))])),
                );
            }
            self.edge(
                &format!("trace:{run}"),
                format!("head:{run}"),
                format!("traces.{run}.head"),
                "",
                None,
                "hh-bundle.manifest",
                "head",
                run.to_string(),
                Some(ex.head.clone()),
            );
            for (i, c) in ex.checkpoints.iter().enumerate() {
                self.edge(
                    &format!("trace:{run}"),
                    "checkpoint",
                    format!("traces.{run}.checkpoints[{i}]"),
                    "",
                    None,
                    "hh-bundle.manifest",
                    "checkpoint",
                    event_ref_target(c),
                    Some(c.clone()),
                );
            }
            if let Some(h) = &ex.audit_tree_head {
                self.edge(
                    &format!("trace:{run}"),
                    "audit_checkpoint",
                    format!("traces.{run}.audit_tree_head"),
                    "",
                    None,
                    "hh-bundle.manifest",
                    "checkpoint",
                    event_ref_target(&h.checkpoint_ref),
                    Some(h.checkpoint_ref.clone()),
                );
            }
            self.decode_pages(ex);
            for (i, l) in ex.lineage_prefixes.iter().enumerate() {
                let (lrun, seq, hash) = lineage_ref(l);
                self.edge(
                    &format!("trace:{run}"),
                    format!("lineage:{lrun}"),
                    format!("traces.{run}.lineage_prefixes[{i}]"),
                    "",
                    None,
                    "hh-bundle.manifest",
                    "lineage",
                    format!("{lrun}@{seq}"),
                    Some(Json::obj([
                        ("run_id", Json::str(lrun.clone())),
                        ("up_to_seq", Json::Int(seq as i64)),
                        ("head_hash", Json::str(hash)),
                    ])),
                );
                if let Some(lex) = self.d.manifest.traces.get(&lrun).cloned() {
                    if !self.over {
                        self.expand_export(&lex, &self.d.manifest.members, _depth + 1);
                    }
                }
            }
        }
        self.visiting.pop();
    }

    /// A decoded `contains:<id>` member — manifest roots recurse.
    fn expand_manifest(&mut self, addr: &str, bytes: &[u8], depth: usize) {
        if depth > self.lim.max_depth {
            self.over = true;
            self.fail(
                code::TRAVERSAL_LIMIT,
                format!("manifest depth bound {} exhausted", self.lim.max_depth),
            );
            return;
        }
        let node = format!("manifest:{addr}");
        if self.visiting.contains(&node) {
            self.fail(code::CYCLE, format!("{node} re-enters the expansion stack"));
            return;
        }
        if !self.manifests_expanded.insert(addr.to_string()) {
            return;
        }
        let parsed = std::str::from_utf8(bytes)
            .ok()
            .and_then(|t| hharness_wire::json::parse(t).ok());
        let Some(doc) = parsed else {
            self.fail(
                code::MALFORMED,
                format!("contains member {addr} does not parse"),
            );
            return;
        };
        if doc.get("schema").and_then(Json::as_str) != Some(crate::manifest::BUNDLE_SCHEMA) {
            self.fail(
                code::UNSUPPORTED_SCHEMA,
                format!("contains member {addr} is not hh-bundle/1"),
            );
            return;
        }
        let Ok(child) = crate::manifest::BundleManifest::from_json(&doc) else {
            self.fail(
                code::MALFORMED,
                format!("contains member {addr} does not decode as a manifest"),
            );
            return;
        };
        self.visiting.push(node.clone());
        for (i, mm) in child.members.iter().enumerate() {
            self.edge_status(
                &node,
                mm.role.clone(),
                format!("contains.members[{i}]"),
                &mm.media_type,
                Some(mm.size),
                "hh-bundle.manifest",
                "member",
                mm.address.clone(),
                None,
                Some(mm.status),
            );
            if mm.role.starts_with("contains:") && !self.over {
                if let Some(b) = self.d.members.get(&mm.address) {
                    if self.visiting.contains(&format!("manifest:{}", mm.address)) {
                        self.fail(
                            code::CYCLE,
                            format!("manifest:{} re-enters the expansion stack", mm.address),
                        );
                    } else {
                        let b = b.clone();
                        self.expand_manifest(&mm.address, &b, depth + 1);
                    }
                }
            }
        }
        // The contained manifest's own traces/pages/blob index walk
        // against the shared member pool — each declares its status
        // in the *child's* index.
        for ex in child.traces.values().cloned().collect::<Vec<_>>() {
            if !self.over {
                self.expand_export(&ex, &child.members, depth + 1);
            }
        }
        self.visiting.pop();
    }
}

fn member_row<'m>(
    m: &'m crate::manifest::BundleManifest,
    addr: &str,
) -> Option<&'m crate::manifest::MemberRef> {
    m.members.iter().find(|mm| mm.address == addr)
}

fn event_ref_target(c: &Json) -> String {
    let run = c.get("run_id").and_then(Json::as_str).unwrap_or("");
    let ev = c.get("event_id").and_then(Json::as_str).unwrap_or("");
    format!("{run}:{ev}")
}

fn lineage_ref(l: &Json) -> (String, u64, String) {
    (
        l.get("run_id").and_then(Json::as_str).unwrap_or("").into(),
        l.get("up_to_seq").and_then(Json::as_int).unwrap_or(0) as u64,
        l.get("head_hash")
            .and_then(Json::as_str)
            .unwrap_or("")
            .into(),
    )
}

/// `closure_walk(decoded, opts) → closure_report` — enumerate the
/// owner-typed roots, then resolve every dedup'd edge against the
/// recipient's own member bytes (plus a permitted `fetch::fetch`).
pub fn closure_walk(decoded: &Decoded, opts: WalkOpts<'_>) -> ClosureReport {
    let mut w = Walk {
        d: decoded,
        lim: &opts.limits,
        edges: Vec::new(),
        seen: BTreeSet::new(),
        runs_expanded: BTreeSet::new(),
        manifests_expanded: BTreeSet::new(),
        visiting: Vec::new(),
        events: BTreeMap::new(),
        n_events: 0,
        failures: Vec::new(),
        over: false,
    };
    let m = &decoded.manifest;

    // ── member + ordinary (section/run-manifest) roots ─────────────
    for (i, mm) in m.members.iter().enumerate() {
        w.edge(
            "manifest",
            mm.role.clone(),
            format!("members[{i}]"),
            &mm.media_type,
            Some(mm.size),
            "hh-bundle.manifest",
            "member",
            mm.address.clone(),
            None,
        );
    }
    for (i, r) in crate::levels::section_member_refs(m).iter().enumerate() {
        w.edge(
            "manifest",
            "section",
            format!("section_member_refs[{i}]"),
            "",
            None,
            "hh-bundle.manifest",
            "section",
            r.clone(),
            None,
        );
    }
    if let Some(Json::Arr(cs)) = m.composition.get("contains") {
        for (i, c) in cs.iter().enumerate() {
            if let Some(a) = c.get("member").and_then(Json::as_str) {
                w.edge(
                    "composition",
                    format!(
                        "contains:{}",
                        c.get("bundle_id").and_then(Json::as_str).unwrap_or("")
                    ),
                    format!("composition.contains[{i}]"),
                    "",
                    None,
                    "hh-bundle.manifest",
                    "manifest",
                    a.to_string(),
                    None,
                );
            }
        }
    }
    if let Some(df) = m.composition.get("derived_from") {
        if let Some(b) = df.get("bundle_id").and_then(Json::as_str) {
            w.edge(
                "composition",
                "derived_from",
                "composition.derived_from",
                "",
                None,
                "hh-bundle.manifest",
                "ordinary",
                b.to_string(),
                None,
            );
        }
    }
    for (i, l) in m.subject.lineage.iter().enumerate() {
        let (lrun, seq, hash) = lineage_ref(l);
        w.edge(
            "manifest",
            format!("lineage:{lrun}"),
            format!("subject.lineage[{i}]"),
            "",
            None,
            "hh-bundle.manifest",
            "lineage",
            format!("{lrun}@{seq}"),
            Some(Json::obj([
                ("run_id", Json::str(lrun)),
                ("up_to_seq", Json::Int(seq as i64)),
                ("head_hash", Json::str(hash)),
            ])),
        );
    }

    // ── claim/procedure roots (evidence-package/1) ─────────────────
    let mut profile_version = None;
    if m.evidence_profile != Json::Null {
        match m.evidence_profile.get("schema").and_then(Json::as_str) {
            Some(profile::PROFILE_SCHEMA) => {}
            Some(other) if other.starts_with("evidence-package/") => w.fail(
                code::UNSUPPORTED_SCHEMA,
                format!("evidence_profile declares {other}"),
            ),
            _ => w.fail(
                code::MALFORMED,
                "evidence_profile schema is not evidence-package/<n>",
            ),
        }
        if let Some(addr) = m.evidence_profile.get("member").and_then(Json::as_str) {
            let parsed = decoded
                .members
                .get(addr)
                .and_then(|b| std::str::from_utf8(b).ok())
                .and_then(|t| hharness_wire::json::parse(t).ok());
            match parsed.as_ref().map(EvidenceProfile::from_json) {
                Some(Ok(p)) => {
                    profile_version = Some(p.profile_version);
                    profile_roots(&mut w, &p);
                }
                Some(Err(crate::error::BundleError::ProfileRefused { code: c, detail })) => {
                    w.fail(c, detail);
                }
                Some(Err(e)) => w.fail(code::MALFORMED, format!("profile member: {e}")),
                None => {
                    // The member edge reports the byte-level loss; an
                    // undecodable member is a traversal failure.
                    if decoded.members.contains_key(addr) {
                        w.fail(code::MALFORMED, "profile member does not parse");
                    }
                }
            }
        }
    }

    // ── trace roots: every subject run + every trace the manifest
    //    carries (a trace for a non-subject run still closes). ──────
    let mut runs: BTreeSet<String> = m.subject.run_ids.iter().cloned().collect();
    runs.extend(m.traces.keys().cloned());
    for run in runs {
        w.edge(
            "manifest",
            format!("traces:{run}"),
            format!("traces.{run}"),
            "",
            None,
            "hh-bundle.manifest",
            "trace",
            run.clone(),
            None,
        );
        if let Some(ex) = w.d.manifest.traces.get(&run).cloned() {
            w.expand_export(&ex, &w.d.manifest.members, 0);
        }
    }

    // ── owner-typed payload refs: sweep the decoded events — the
    //    envelope's causes/refs/ir_refs and the class table's
    //    declared content_refs members. An unregistered class is an
    //    unknown relevant schema — it fails closure. ───────────────
    let events: Vec<EventEnvelope> = w.events.values().flat_map(|v| v.iter().cloned()).collect();
    for ev in &events {
        let parent = format!("event:{}:{}", ev.run_id, ev.event_id);
        let owner = format!("hh-ledger.envelope:{}", ev.class);
        match classes::lookup(&ev.class) {
            Some(spec) => {
                for cr in spec.content_refs {
                    if let Some(v) = ev.payload.get(cr).and_then(Json::as_str) {
                        w.edge(
                            &parent,
                            format!("payload:{cr}"),
                            format!("ev.{}.{}", ev.event_id, cr),
                            "",
                            None,
                            &format!("hh-ledger.class_table:{}", ev.class),
                            "payload",
                            v.to_string(),
                            None,
                        );
                    }
                }
            }
            None => w.fail(
                code::UNKNOWN_CLASS,
                format!("event {} class {} is not registered", ev.event_id, ev.class),
            ),
        }
        for (i, c) in ev.causes.iter().enumerate() {
            w.edge(
                &parent,
                "cause",
                format!("ev.causes[{i}]"),
                "",
                None,
                &owner,
                "cause",
                format!("{}:{}", c.run_id, c.event_id),
                None,
            );
        }
        for (i, r) in ev.refs.iter().enumerate() {
            w.edge(
                &parent,
                "payload_ref",
                format!("ev.refs[{i}]"),
                "",
                Some(r.size),
                &owner,
                "payload",
                r.id(),
                None,
            );
        }
        for (i, r) in ev.ir_refs.iter().enumerate() {
            w.edge(
                &parent,
                "ir_ref",
                format!("ev.ir_refs[{i}]"),
                "",
                None,
                &owner,
                "ir",
                r.version_id.clone(),
                None,
            );
        }
    }

    // ── ordinary refs: the decoded RunManifest's declared refs ─────
    for ev in &events {
        if ev.class != "lifecycle.run.created" {
            continue;
        }
        if let Ok(rm) = hharness_ledger::manifest::RunManifest::from_json(&ev.payload) {
            if let Json::Obj(refs) = crate::runrefs::manifest_refs(&rm) {
                for (field, v) in refs {
                    if let Some(t) = v.as_str() {
                        w.edge(
                            &format!("event:{}:{}", ev.run_id, ev.event_id),
                            "run_ref",
                            format!("run_refs.{}.{}", ev.run_id, field),
                            "",
                            None,
                            "hh-bundle.run_refs",
                            "ordinary",
                            t.to_string(),
                            None,
                        );
                    }
                }
            }
        }
    }

    // ── expand the materialized `contains:` manifests ──────────────
    let manifest_edges: Vec<(String, String)> = w
        .edges
        .iter()
        .filter(|e| e.kind == "manifest")
        .map(|e| (e.target.clone(), e.parent.clone()))
        .collect();
    for (addr, _parent) in manifest_edges {
        if w.over {
            break;
        }
        if let Some(b) = decoded.members.get(&addr).cloned() {
            w.expand_manifest(&addr, &b, 1);
        }
    }

    // ── materialization: batch the `fetch`-status targets through
    //    the landed fetch machinery (verified arrival, never
    //    executed). ─────────────────────────────────────────────────
    let mut fetched = MemberBytes::new();
    let mut fetch_failed: BTreeMap<String, String> = BTreeMap::new();
    let mut records = Vec::new();
    if let Some(permit) = opts.fetch {
        let addrs: Vec<String> = w
            .edges
            .iter()
            .filter(|e| {
                member_row(m, &e.target)
                    .map(|mm| mm.status == MemberStatus::Fetch)
                    .unwrap_or(false)
                    && !decoded.members.contains_key(&e.target)
            })
            .map(|e| e.target.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !addrs.is_empty() {
            let FetchOutcome {
                materialized,
                failed,
                bytes,
                records: recs,
            } = crate::fetch::fetch(
                decoded,
                &addrs,
                permit.pool,
                permit.trust,
                permit.remote,
                permit.now_ms,
            );
            records = recs;
            for a in &materialized {
                if let Some(b) = bytes.get(a) {
                    fetched.insert(a.clone(), b.clone());
                }
            }
            for f in &failed {
                if let (Some(a), Some(r)) = (
                    f.get("address").and_then(Json::as_str),
                    f.get("reason").and_then(Json::as_str),
                ) {
                    fetch_failed.insert(a.to_string(), r.to_string());
                }
            }
        }
    }

    // ── resolve every dedup'd edge truthfully ──────────────────────
    let members_all: BTreeMap<String, &[u8]> = decoded
        .members
        .iter()
        .chain(fetched.iter())
        .map(|(a, b)| (a.clone(), b.as_slice()))
        .collect();
    let mut resolved = 0usize;
    let mut withheld = 0usize;
    let mut unresolved = 0usize;
    for e in &mut w.edges {
        e.resolution = resolve(decoded, &members_all, &fetch_failed, &w.events, e);
        match e.resolution {
            Resolution::Resolved { .. } => resolved += 1,
            Resolution::Withheld { .. } => withheld += 1,
            Resolution::Unresolved { ref code } => {
                unresolved += 1;
                w.failures.push(Json::obj([
                    ("code", Json::str(code.clone())),
                    (
                        "detail",
                        Json::str(format!("{} {} → {}", e.kind, e.field, e.target)),
                    ),
                ]));
            }
        }
    }
    let verdict = if !w.failures.is_empty() || unresolved > 0 {
        "failed"
    } else if withheld > 0 {
        "withheld"
    } else {
        "closed"
    };
    ClosureReport {
        bundle_id: decoded.manifest.version_id.clone(),
        profile_version,
        procedure: opts.procedure.to_string(),
        recipient_class: opts.recipient_class.to_string(),
        checked_at: opts.checked_at.to_string(),
        required: w.edges.len(),
        resolved,
        withheld,
        unresolved,
        verdict,
        failures: w.failures,
        edges: w.edges,
        fetched: records,
    }
}

/// The `evidence-package/1` claim/procedure roots — claim
/// `native_refs`/`report_refs`, declared coordinate refs (subject
/// run, launch/settlement/instrument/selection refs, sealed digests,
/// oracle refs, export coordinates, native heads, watermarks, legacy
/// members). Null/absent optional members declare no edge.
fn profile_roots(w: &mut Walk<'_>, p: &EvidenceProfile) {
    let decl = |w: &mut Walk<'_>, field: String, v: Option<&str>| {
        if let Some(t) = v {
            w.edge(
                "profile",
                "profile_ref",
                field,
                "",
                None,
                "evidence-package/1",
                "declared",
                t.to_string(),
                None,
            );
        }
    };
    for (i, c) in p.claims.iter().enumerate() {
        for kind in ["native_refs", "report_refs"] {
            if let Some(Json::Arr(rs)) = c.get(kind) {
                for (j, r) in rs.iter().enumerate() {
                    decl(w, format!("claims[{i}].{kind}[{j}]"), r.as_str());
                }
            }
        }
    }
    for (i, a) in p.attempts.iter().enumerate() {
        decl(
            w,
            format!("attempts[{i}].subject_run"),
            a.subject_run.as_deref(),
        );
        decl(
            w,
            format!("attempts[{i}].launch_binding"),
            a.launch_binding.as_deref(),
        );
        decl(
            w,
            format!("attempts[{i}].settlement"),
            a.settlement.as_deref(),
        );
    }
    for (i, g) in p.grade_attempts.iter().enumerate() {
        for (k, v) in [
            ("instrument_event", g.instrument_event.as_deref()),
            ("sealed_subject_digest", g.sealed_subject_digest.as_deref()),
            (
                "sealed_submission_digest",
                g.sealed_submission_digest.as_deref(),
            ),
            ("oracle", g.oracle.as_deref()),
            ("oracle_version", g.oracle_version.as_deref()),
            ("selection", g.selection.as_deref()),
        ] {
            decl(w, format!("grade_attempts[{i}].{k}"), v);
        }
        for (j, s) in g.supersedes.iter().enumerate() {
            decl(w, format!("grade_attempts[{i}].supersedes[{j}]"), Some(s));
        }
    }
    for (i, e) in p.exports.iter().enumerate() {
        decl(w, format!("exports[{i}].run_id"), Some(&e.run_id));
        w.edge(
            "profile",
            "profile_coord",
            format!("exports[{i}].last_event"),
            "",
            None,
            "evidence-package/1",
            "checkpoint",
            format!("{}:{}", e.run_id, e.last_event.event_id),
            Some(e.last_event.to_json()),
        );
    }
    for (i, h) in p.native_heads.iter().enumerate() {
        decl(w, format!("native_heads[{i}].run_id"), Some(&h.run_id));
    }
    for (i, wm) in p.watermarks.iter().enumerate() {
        decl(w, format!("watermarks[{i}].run_id"), Some(&wm.run_id));
    }
    for (i, l) in p.legacy_refs.iter().enumerate() {
        decl(
            w,
            format!("legacy_refs[{i}].member"),
            l.get("member").and_then(Json::as_str),
        );
    }
}

/// Per-edge truthful resolution — member status → bytes → blob
/// status → unindexed bytes → decoded event/run coordinate.
fn resolve(
    decoded: &Decoded,
    members: &BTreeMap<String, &[u8]>,
    fetch_failed: &BTreeMap<String, String>,
    events: &BTreeMap<String, Vec<EventEnvelope>>,
    e: &ClosureEdge,
) -> Resolution {
    let verify = |addr: &str, via: &'static str, size: Option<u64>| match members.get(addr) {
        Some(b) if hharness_identity::idp_id("blob", b) != addr => Resolution::Unresolved {
            code: code::DIGEST_MISMATCH.into(),
        },
        Some(b) if size.map(|s| s > 0).unwrap_or(false) && b.len() as u64 != size.unwrap() => {
            Resolution::Unresolved {
                code: code::BAD_SIZE.into(),
            }
        }
        Some(_) => Resolution::Resolved { via },
        None => Resolution::Unresolved {
            code: code::MISSING_BYTES.into(),
        },
    };
    let blob_row = |addr: &str| {
        decoded
            .manifest
            .traces
            .values()
            .flat_map(|t| t.blob_index.iter())
            .find(|b| b.address == addr)
            .map(|b| b.status.clone())
    };
    match e.kind {
        "trace" => {
            if decoded.manifest.traces.contains_key(&e.target) {
                Resolution::Resolved { via: "coordinate" }
            } else {
                Resolution::Unresolved {
                    code: code::MISSING_RUN.into(),
                }
            }
        }
        "tree" => match decoded
            .manifest
            .traces
            .values()
            .find(|t| t.tree == e.target)
        {
            Some(t) if LedgerExport::tree_address(&t.pages) == e.target => {
                Resolution::Resolved { via: "coordinate" }
            }
            _ => Resolution::Unresolved {
                code: code::HEAD_MISMATCH.into(),
            },
        },
        "head" => {
            let expected = e.expect.clone().unwrap_or(Json::Null);
            let ok = events.get(&e.target).and_then(|evs| evs.last()).map(|l| {
                l.seq as i64 == expected.get("seq").and_then(Json::as_int).unwrap_or(-1)
                    && l.event_id
                        == expected
                            .get("event_id")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                    && l.hash == expected.get("hash").and_then(Json::as_str).unwrap_or("")
            });
            if ok.unwrap_or(false) {
                Resolution::Resolved { via: "coordinate" }
            } else {
                Resolution::Unresolved {
                    code: code::HEAD_MISMATCH.into(),
                }
            }
        }
        "checkpoint" | "cause" => {
            let mut it = e.target.splitn(2, ':');
            let (run, ev) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            if events
                .get(run)
                .map(|evs| evs.iter().any(|x| x.event_id == ev))
                .unwrap_or(false)
            {
                Resolution::Resolved { via: "coordinate" }
            } else {
                Resolution::Unresolved {
                    code: code::UNRESOLVED_COORD.into(),
                }
            }
        }
        "lineage" => {
            let mut it = e.target.splitn(2, '@');
            let (run, seq) = (
                it.next().unwrap_or(""),
                it.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
            );
            let hash = e
                .expect
                .as_ref()
                .and_then(|x| x.get("head_hash"))
                .and_then(Json::as_str)
                .unwrap_or("");
            match events.get(run) {
                Some(evs) => match evs.iter().find(|x| x.seq == seq) {
                    Some(x) if x.hash == hash => Resolution::Resolved { via: "coordinate" },
                    _ => Resolution::Unresolved {
                        code: code::HEAD_MISMATCH.into(),
                    },
                },
                None => Resolution::Unresolved {
                    code: code::MISSING_RUN.into(),
                },
            }
        }
        _ => {
            let status = e
                .declared_status
                .or_else(|| member_row(&decoded.manifest, &e.target).map(|mm| mm.status));
            let size = e
                .size
                .or_else(|| member_row(&decoded.manifest, &e.target).map(|mm| mm.size))
                .filter(|s| *s > 0);
            if let Some(st) = status {
                match st {
                    MemberStatus::Present => verify(&e.target, "bytes", size),
                    MemberStatus::Fetch => {
                        if let Some(r) = fetch_failed.get(&e.target) {
                            Resolution::Unresolved { code: r.clone() }
                        } else if members.contains_key(&e.target) {
                            verify(&e.target, "fetch", size)
                        } else {
                            Resolution::Withheld { status: "fetch" }
                        }
                    }
                    MemberStatus::Redacted => Resolution::Withheld { status: "redacted" },
                    MemberStatus::Gc => Resolution::Withheld { status: "gc" },
                }
            } else if let Some(status) = blob_row(&e.target).or_else(|| {
                e.expect
                    .as_ref()
                    .and_then(|x| x.get("status"))
                    .and_then(Json::as_str)
                    .map(String::from)
            }) {
                match status.as_str() {
                    "present" => verify(&e.target, "bytes", None),
                    "redacted" => Resolution::Withheld { status: "redacted" },
                    "gc" => Resolution::Withheld { status: "gc" },
                    _ => Resolution::Unresolved {
                        code: code::MALFORMED.into(),
                    },
                }
            } else if members.contains_key(&e.target) {
                verify(&e.target, "bytes", e.size)
            } else if decoded.manifest.traces.values().any(|t| t.tree == e.target) {
                // The tree *coordinate* (`idp/1` over the page-address
                // list) is self-verifying, not a member.
                match decoded
                    .manifest
                    .traces
                    .values()
                    .find(|t| t.tree == e.target)
                {
                    Some(t) if LedgerExport::tree_address(&t.pages) == e.target => {
                        Resolution::Resolved { via: "coordinate" }
                    }
                    _ => Resolution::Unresolved {
                        code: code::HEAD_MISMATCH.into(),
                    },
                }
            } else if events.contains_key(&e.target)
                || events.values().flatten().any(|x| {
                    x.event_id == e.target
                        || format!("{}:{}", x.class, x.event_id) == e.target
                        || format!("{}:{}", x.run_id, x.event_id) == e.target
                })
            {
                Resolution::Resolved { via: "coordinate" }
            } else {
                Resolution::Unresolved {
                    code: code::MISSING_MEMBER.into(),
                }
            }
        }
    }
}
