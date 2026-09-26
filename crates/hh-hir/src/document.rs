//! The document model (§3.1.2): a `HirDocument` is a DAG of `Node`s and `Edge`s plus a
//! `root` `Ref<AgentProcess>` and an optional `assembly` section (§3.3 owns the grammar;
//! carried as a structured slot at Stage 1). A `SealedDefinition` is a sealed document with
//! the two facts `seal` computes: the definition's own `DefinitionVersionRef` and the
//! closed-world tool set (the minting context for `environment` authority — DF-S1.3-3).

use std::collections::BTreeMap;

use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::errors::HirError;
use crate::kinds::EntityKind;
use crate::leaves::Text;
use crate::records::{
    EdgeRecord, KindRecord, ObservationContent, ProcedureStep, SurfaceRecord, VersionRecord,
};
use crate::refs::Ref;

/// A HIR node — `{kind, provenance, semantic, surface?, ext, version}` (§3.1.2).
/// `home_plane` is **derived** — [`Node::home_plane`]; `dialect` lives on `version`.
/// `provenance` is mandatory by construction (DF-S1.3-1): the parse path fails
/// `MissingProvenance` when the member is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The entity kind.
    pub kind: EntityKind,
    /// The provenance record (mandatory — every node and edge; §3.1.1).
    pub provenance: ProvenanceRecord,
    /// The semantic record (one of the thirteen product types).
    pub semantic: KindRecord,
    /// The surface record (only on surface-carrying kinds — §3.1.5).
    pub surface: Option<SurfaceRecord>,
    /// The extension map — prefixed keys only; `hir/` is reserved (§3.1.12).
    pub ext: BTreeMap<String, Json>,
    /// The version record.
    pub version: VersionRecord,
}

impl Node {
    /// A fresh node: `provenance` is supplied by the author, never defaulted (CC2).
    pub fn new(kind: EntityKind, semantic: KindRecord, provenance: ProvenanceRecord) -> Node {
        debug_assert_eq!(kind, semantic.kind());
        Node {
            kind,
            provenance,
            semantic,
            surface: None,
            ext: BTreeMap::new(),
            version: VersionRecord::fresh(),
        }
    }

    /// `home_plane` — derived, never stored: `classify_home(kind)` (§3.1.2; the per-kind
    /// table is `hh-ontology`'s, DF-S1.1-1).
    pub fn home_plane(&self) -> hh_ontology::planes::Plane {
        self.kind.home()
    }

    /// The semantic id — `identity` sets it on `version`; before that, compute it.
    pub fn semantic_id(&self) -> String {
        match &self.version.semantic_id {
            Some(s) => s.clone(),
            None => crate::identity::semantic_id(self),
        }
    }

    /// The version id — `identity`/`seal` set it.
    pub fn version_id(&self) -> String {
        match &self.version.version_id {
            Some(v) => v.clone(),
            None => crate::identity::version_id(self),
        }
    }

    /// Every `Text` leaf reachable from this node (the in-memory prose bodies —
    /// the canonical form carries `content_hash` only, so a *content-level* scan
    /// such as R-2.8.3's `SecretValueInDefinition` needs this enumeration).
    /// Enumerated here, next to the record definitions, so a new `Text` member on
    /// a kind forces the walk to grow (CC7). `CompiledPayload` carries no bytes
    /// in memory — it is fully hash-addressed, so nothing here reaches it.
    pub fn text_leaves(&self) -> Vec<&Text> {
        let mut out: Vec<&Text> = Vec::new();
        match &self.semantic {
            KindRecord::Goal(g) => {
                out.push(&g.statement);
                if let Some(t) = &g.unverifiable_reason {
                    out.push(t);
                }
            }
            KindRecord::Observation(o) => {
                if let ObservationContent::Text(t) = &o.content {
                    out.push(t);
                }
            }
            KindRecord::Memory(m) => out.push(&m.content),
            KindRecord::Procedure(p) => collect_step_texts(&p.steps, &mut out),
            KindRecord::ToolCapability(t) => out.push(&t.purpose),
            _ => {}
        }
        if let Some(SurfaceRecord::Tool(ts)) = &self.surface {
            out.push(&ts.description_template);
        }
        out
    }

    /// The mutable mirror of [`Node::text_leaves`] — `ablate`'s leaf-replacement
    /// path (§3.1.7). Same enumeration, same ordering.
    pub fn text_leaves_mut(&mut self) -> Vec<&mut Text> {
        let mut out: Vec<&mut Text> = Vec::new();
        match &mut self.semantic {
            KindRecord::Goal(g) => {
                out.push(&mut g.statement);
                if let Some(t) = &mut g.unverifiable_reason {
                    out.push(t);
                }
            }
            KindRecord::Observation(o) => {
                if let ObservationContent::Text(t) = &mut o.content {
                    out.push(t);
                }
            }
            KindRecord::Memory(m) => out.push(&mut m.content),
            KindRecord::Procedure(p) => collect_step_texts_mut(&mut p.steps, &mut out),
            KindRecord::ToolCapability(t) => out.push(&mut t.purpose),
            _ => {}
        }
        if let Some(SurfaceRecord::Tool(ts)) = &mut self.surface {
            out.push(&mut ts.description_template);
        }
        out
    }
}

/// The recursive step walk — `Instruction` is the only `Text`-bearing step kind;
/// `Branch`/`Loop` bodies recurse (the closed sum, §3.1.3).
fn collect_step_texts<'a>(steps: &'a [ProcedureStep], out: &mut Vec<&'a Text>) {
    for s in steps {
        match s {
            ProcedureStep::Instruction(t) => out.push(t),
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                collect_step_texts(then_body, out);
                collect_step_texts(else_body, out);
            }
            ProcedureStep::Loop { body, .. } => collect_step_texts(body, out),
            _ => {}
        }
    }
}

/// The mutable mirror of [`collect_step_texts`].
fn collect_step_texts_mut<'a>(steps: &'a mut [ProcedureStep], out: &mut Vec<&'a mut Text>) {
    for s in steps {
        match s {
            ProcedureStep::Instruction(t) => out.push(t),
            ProcedureStep::Branch {
                then_body,
                else_body,
                ..
            } => {
                collect_step_texts_mut(then_body, out);
                collect_step_texts_mut(else_body, out);
            }
            ProcedureStep::Loop { body, .. } => collect_step_texts_mut(body, out),
            _ => {}
        }
    }
}

/// A HIR edge — `{kind, from, to, provenance, fields}` plus the mandatory version record
/// (§3.1.1: "a provenance record and version record on every node and edge"). `from`/`to`
/// are semantic ids (refs-by-semantic_id; §3.1.2); for `derived-from`, `to` is the `from`
/// node itself — the edge's own record is the derivation (§3.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// The edge kind.
    pub kind: crate::kinds::EdgeKind,
    /// The `from` node's semantic id.
    pub from: String,
    /// The `to` node's semantic id.
    pub to: String,
    /// The provenance record (mandatory — DF-S1.3-1).
    pub provenance: ProvenanceRecord,
    /// The per-kind edge record.
    pub fields: EdgeRecord,
    /// The version record.
    pub version: VersionRecord,
}

impl Edge {
    /// A fresh edge; `provenance` is supplied, never defaulted.
    pub fn new(
        kind: crate::kinds::EdgeKind,
        from: impl Into<String>,
        to: impl Into<String>,
        fields: EdgeRecord,
        provenance: ProvenanceRecord,
    ) -> Edge {
        debug_assert_eq!(kind, fields.kind());
        Edge {
            kind,
            from: from.into(),
            to: to.into(),
            provenance,
            fields,
            version: VersionRecord::fresh(),
        }
    }

    /// The edge's version id.
    pub fn version_id(&self) -> String {
        match &self.version.version_id {
            Some(v) => v.clone(),
            None => crate::identity::edge_version_id(self),
        }
    }

    /// The edge's `Text` leaves (`derived-from.hypothesis` is the only
    /// `Text`-bearing edge record).
    pub fn text_leaves(&self) -> Vec<&Text> {
        match &self.fields {
            EdgeRecord::DerivedFrom { hypothesis, .. } => vec![hypothesis.as_ref()],
            _ => Vec::new(),
        }
    }

    /// The mutable mirror of [`Edge::text_leaves`].
    pub fn text_leaves_mut(&mut self) -> Vec<&mut Text> {
        match &mut self.fields {
            EdgeRecord::DerivedFrom { hypothesis, .. } => vec![hypothesis.as_mut()],
            _ => Vec::new(),
        }
    }
}

/// A HIR document (§3.1.2): `{hir_version, root: Ref<AgentProcess>, nodes[], edges[],
/// assembly?}`. The document's identity is the root's ids — a document is identified by its
/// sealed definition version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirDocument {
    /// `hir_version` — must be `HIR/1` (`DialectUnsupported` otherwise).
    pub hir_version: String,
    /// The root `Ref<AgentProcess>` (§3.1.2).
    pub root: Ref,
    /// The nodes.
    pub nodes: Vec<Node>,
    /// The edges.
    pub edges: Vec<Edge>,
    /// The assembly section — selector sets, unresolved refs (§3.3 grammar; an optional
    /// structured slot at Stage 1). **A document carrying `assembly` is not sealable** —
    /// only resolved documents are.
    pub assembly: Option<Json>,
}

impl HirDocument {
    /// A fresh document with the given root ref.
    pub fn new(root: Ref) -> HirDocument {
        HirDocument {
            hir_version: "HIR/1".into(),
            root,
            nodes: Vec::new(),
            edges: Vec::new(),
            assembly: None,
        }
    }

    /// Find a node by semantic id.
    pub fn node(&self, semantic_id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.semantic_id() == semantic_id)
    }

    /// Find a node by semantic id (mutable).
    pub fn node_mut(&mut self, semantic_id: &str) -> Option<&mut Node> {
        self.nodes
            .iter_mut()
            .find(|n| n.semantic_id() == semantic_id)
    }

    /// Edges out of a node (by semantic id).
    pub fn edges_from<'a>(&'a self, semantic_id: &'a str) -> impl Iterator<Item = &'a Edge> + 'a {
        self.edges.iter().filter(move |e| e.from == semantic_id)
    }

    /// Edges into a node.
    pub fn edges_to<'a>(&'a self, semantic_id: &'a str) -> impl Iterator<Item = &'a Edge> + 'a {
        self.edges.iter().filter(move |e| e.to == semantic_id)
    }

    /// The canonical encoding of the document — `canonicalize` (§3.1.7).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.to_json().to_canonical_string().into_bytes()
    }

    /// Every `Text` leaf in the document (nodes + edges) — the in-memory prose
    /// a content-level scan must cover (the canonical form carries hashes only).
    pub fn text_leaves(&self) -> Vec<&Text> {
        self.nodes
            .iter()
            .flat_map(|n| n.text_leaves())
            .chain(self.edges.iter().flat_map(|e| e.text_leaves()))
            .collect()
    }

    /// The mutable mirror of [`HirDocument::text_leaves`] (`ablate`, §3.1.7).
    pub fn text_leaves_mut(&mut self) -> Vec<&mut Text> {
        self.nodes
            .iter_mut()
            .flat_map(|n| n.text_leaves_mut())
            .chain(self.edges.iter_mut().flat_map(|e| e.text_leaves_mut()))
            .collect()
    }
}

/// A `DefinitionVersionRef` — `{semantic_id, version_id}` of a definition (the ref form of
/// `diff`'s endpoints, §3.1.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionVersionRef {
    /// The definition's semantic id (the root's).
    pub semantic_id: String,
    /// The definition's version id (the root's).
    pub version_id: String,
}

/// A sealed definition (§3.1.6): a document whose refs are all pinned, whose nodes carry
/// `sealed = true`, and whose every member carries the `seal`-conferred `definition`
/// authority. `seal` also computes the closed-world tool set — the minting context
/// `environment` authority is conferred through (DF-S1.3-3).
#[derive(Debug, Clone, PartialEq)]
pub struct SealedDefinition {
    /// The sealed document.
    pub document: HirDocument,
    /// The definition's own identity (`RecordKind::SealedDefinition` over the canonical
    /// sealed document — CC1 idp/1).
    pub definition_ref: DefinitionVersionRef,
    /// The semantic ids of `ToolCapability`s declared **closed-world** in this definition —
    /// `effects = pure`, or every declared effect is `world = closed` (§8.1 #3: the
    /// `environment` mint requires a closed-schema structured value from a closed-world
    /// tool declared in the sealed definition).
    pub closed_world_tools: std::collections::BTreeSet<String>,
}

impl SealedDefinition {
    /// The minting context this definition confers — plug into
    /// `hh_provenance::default_authority_in` when minting records produced *inside* this
    /// sealed definition (DF-S1.3-3).
    pub fn minting_context(&self) -> hh_provenance::MintingContext {
        hh_provenance::MintingContext {
            closed_world_tools: self.closed_world_tools.clone(),
            // A sealed definition vouches for no participant — the vouch set
            // is conferred by the Hosting-ABI boundary, never by a document
            // (CC2). S4.5a / DF-S1.3-2.
            vouched_participants: std::collections::BTreeSet::new(),
        }
    }

    /// The canonical bytes of the sealed document.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.document.canonical_bytes()
    }
}

/// The parse entry: `canonical bytes → HirDocument` (the out-of-process seam — AC-IR-10).
/// `NonCanonicalInput` for malformed encodings; `MissingProvenance` for members without the
/// mandatory record; `UnknownKind` for unregistered kinds.
pub fn parse_document(bytes: &[u8]) -> Result<HirDocument, HirError> {
    let json =
        hh_wire::canonical::parse_canonical(bytes).map_err(|e| HirError::NonCanonicalInput {
            detail: format!("document: {e}"),
        })?;
    crate::schema::document_from_json(&json)
}
