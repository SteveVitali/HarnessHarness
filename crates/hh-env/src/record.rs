//! `EnvironmentRecord` — the *definition* identity layer (§5a.5 §3 record row;
//! ADR-0136 §3). Identity is under `idp/1`'s `environment` domain with a
//! semantic projection (`environment_ref` = `semantic_id`); the record carries
//! the class, the pinned image digest / foreign claim, the provisioning recipe
//! (MUST-data — hooks are `Procedure` refs), the `ContainmentPolicy` ref, the
//! declared nondeterminism and the `unpinned[]` context `resolve` consults.
//!
//! The mutable-tag refusal is **`resolve`'s** job, not `identify`'s — an
//! `EnvironmentRecord` is a definition (not a sealed form), so its refs may be
//! selectors; it is `resolve()` — the provisioning-path entry — that refuses a
//! tag the `unpinned[]` context does not name (`UnresolvedRef`).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_containment::policy::IsolationClass;
use hh_identity::idp::ContentAddress;
use hh_identity::kinds::RecordKind;
use hh_identity::record::{identify, Identity, Record, Reference};
use hh_wire::json::Json;

use crate::errors::EnvError;

/// `EnvironmentClass` — the closed dialect sum (ADR-0136 §3). The class names
/// *where* the workload runs; the honest `isolation_class` on the handle is the
/// attach `ContainmentReport`'s, never the class's (CF-291 — a
/// `provider_hosted`/`remote_*` class claims `external` *isolation*, which is
/// the participant's word, never kernel-enforced).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnvironmentClass {
    /// The host process — `isolation_class = none`, no containment claims.
    LocalHost,
    /// An out-of-process helper under `process_sandbox` (the Stage-1 spike).
    LocalSandboxed,
    /// A local container (namespaces) — Stage 2.
    LocalContainer,
    /// A remote throwaway environment — Stage 3.
    RemoteEphemeral,
    /// A remote long-lived environment — Stage 3.
    RemotePersistent,
    /// A provider-hosted environment — the `external` claim class (Stage 3).
    ProviderHosted,
}

impl EnvironmentClass {
    /// The canonical spelling (`§5a.5` `environment.class`).
    pub fn as_str(self) -> &'static str {
        match self {
            EnvironmentClass::LocalHost => "local_host",
            EnvironmentClass::LocalSandboxed => "local_sandboxed",
            EnvironmentClass::LocalContainer => "local_container",
            EnvironmentClass::RemoteEphemeral => "remote_ephemeral",
            EnvironmentClass::RemotePersistent => "remote_persistent",
            EnvironmentClass::ProviderHosted => "provider_hosted",
        }
    }

    /// Parse the canonical spelling (`None` for an unknown tag).
    pub fn parse(s: &str) -> Option<EnvironmentClass> {
        Some(match s {
            "local_host" => EnvironmentClass::LocalHost,
            "local_sandboxed" => EnvironmentClass::LocalSandboxed,
            "local_container" => EnvironmentClass::LocalContainer,
            "remote_ephemeral" => EnvironmentClass::RemoteEphemeral,
            "remote_persistent" => EnvironmentClass::RemotePersistent,
            "provider_hosted" => EnvironmentClass::ProviderHosted,
            _ => return None,
        })
    }

    /// Whether the class is provisionable at Stage 1 (the C0 slice —
    /// `local_host` + `local_sandboxed`; the container/remote/provider classes
    /// are declared in the sum but unsupported until their stage).
    pub fn stage1_supported(self) -> bool {
        matches!(
            self,
            EnvironmentClass::LocalHost | EnvironmentClass::LocalSandboxed
        )
    }

    /// Whether the class is provisionable at Stage 2 (S2.1 — adds
    /// `local_container`, whose execs run inside a podman container through
    /// the helper's `container` backend; remote/provider classes remain
    /// Stage-3).
    pub fn provisionable(self) -> bool {
        self.stage1_supported() || matches!(self, EnvironmentClass::LocalContainer)
    }

    /// The `isolation_class` the *class name implies* before attach — the floor
    /// the handle reports when no `ContainmentReport` exists yet (e.g. a
    /// `declared`/`provisioning` handle, or `local_host` whose `none` is
    /// honest because nothing is enforced). Post-attach the handle reports the
    /// *report's* class verbatim; this is only the pre-attach claim.
    pub fn implied_isolation(self) -> IsolationClass {
        match self {
            EnvironmentClass::LocalHost => IsolationClass::None,
            EnvironmentClass::LocalSandboxed => IsolationClass::ProcessSandbox,
            EnvironmentClass::LocalContainer => IsolationClass::Namespaces,
            EnvironmentClass::RemoteEphemeral
            | EnvironmentClass::RemotePersistent
            | EnvironmentClass::ProviderHosted => IsolationClass::External,
        }
    }
}

/// `image` — the environment image's identity member (ADR-0136 §3). Only the
/// `ContentAddress` arm is *identity*; `ForeignDigest` is a recorded claim
/// (N8), and `Tag` is a mutable selector `resolve` refuses unless unpinned.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageRef {
    /// An immutable `idp/1` content address — the only R2-backing form.
    ContentAddress(ContentAddress),
    /// A foreign digest (`{scheme, value, source}`) — a claim about the image's
    /// bytes, never identity; reproducibility claims cap at R0/R1/R3.
    ForeignDigest {
        /// The digest scheme (`sha256`, `oci`, …).
        scheme: String,
        /// The digest value.
        value: String,
        /// Where the claim came from (`registry`, `tool`, …).
        source: String,
    },
    /// A mutable tag (`myimage:latest`) — `resolve()` pins it only inside an
    /// `unpinned[]` context that names it; otherwise `UnresolvedRef`.
    Tag(String),
}

impl ImageRef {
    /// The canonical member form for the record's semantic core.
    pub fn to_json(&self) -> Json {
        match self {
            ImageRef::ContentAddress(ca) => Json::obj([
                ("kind", Json::str("content_address")),
                ("id", Json::str(ca.id())),
                ("media_type", Json::str(ca.media_type.clone())),
                ("size", Json::Int(ca.size as i64)),
            ]),
            ImageRef::ForeignDigest {
                scheme,
                value,
                source,
            } => Json::obj([
                ("kind", Json::str("foreign_digest")),
                ("scheme", Json::str(scheme.clone())),
                ("value", Json::str(value.clone())),
                ("source", Json::str(source.clone())),
            ]),
            ImageRef::Tag(t) => {
                Json::obj([("kind", Json::str("tag")), ("tag", Json::str(t.clone()))])
            }
        }
    }
}

/// What `resolve()` produces — the *resolved* image the manifest + the
/// `provisioned` event record. `Tag` survives only as `UnpinnedTag` (the
/// `unpinned[]` context explicitly admitted it); `ContentAddress`/`ForeignDigest`
/// carry their identity/claim through.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedImage {
    /// An immutable content address (R2-capable).
    Address(ContentAddress),
    /// A recorded foreign-digest claim (R0/R1/R3 only — never R2).
    Foreign {
        /// The scheme.
        scheme: String,
        /// The digest value.
        value: String,
        /// The claim source.
        source: String,
    },
    /// A mutable tag the `unpinned[]` context explicitly admitted (R0 only —
    /// the run records that it ran under an unpinned image).
    UnpinnedTag(String),
}

impl ResolvedImage {
    /// Whether the resolved image can back an R2 (or higher) reproducibility
    /// claim — only an immutable `ContentAddress` can.
    pub fn supports_r2(&self) -> bool {
        matches!(self, ResolvedImage::Address(_))
    }

    /// The `image` member of `action.environment.provisioned` (the manifest's
    /// `image` member is the same shape).
    pub fn to_json(&self) -> Json {
        match self {
            ResolvedImage::Address(ca) => Json::obj([
                ("content_address", Json::str(ca.id())),
                ("media_type", Json::str(ca.media_type.clone())),
            ]),
            ResolvedImage::Foreign {
                scheme,
                value,
                source,
            } => Json::obj([(
                "foreign_digest",
                Json::obj([
                    ("scheme", Json::str(scheme.clone())),
                    ("value", Json::str(value.clone())),
                    ("source", Json::str(source.clone())),
                ]),
            )]),
            ResolvedImage::UnpinnedTag(t) => Json::obj([("unpinned_tag", Json::str(t.clone()))]),
        }
    }
}

/// `ProvisioningRecipe` — MUST-data (a recipe is data; its `hooks` are
/// `Procedure` refs the provisioning path runs, keyed by the lifecycle
/// transition they hook).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProvisioningRecipe {
    /// `transition → [procedure_ref]` — e.g. `"post_provision" → [proc-id]`.
    /// Empty at Stage 1 for the two local classes (no hooks).
    pub hooks: BTreeMap<String, Vec<String>>,
}

/// `NondeterminismDeclaration` — a declared source of run-to-run variance the
/// record owns (`clock`, `entropy`, `scheduler`, `fs_state`, …) with the
/// mitigation the run applies. Declared, never inferred (CC2).
#[derive(Debug, Clone, PartialEq)]
pub struct NondeterminismDeclaration {
    /// The variance source (closed at Stage 1 — `clock|entropy|fs_state|
    /// network|scheduler|other`).
    pub source: String,
    /// The mitigation, when the run pins one (`fixed_clock`, `seeded_rng`, …).
    pub mitigation: Option<String>,
}

/// `EnvironmentRecord` — the definition record (ADR-0136 §3). `identify()`
/// yields `version_id` + `semantic_id` (the `environment_ref` a `RunManifest`
/// or `AgentProcessRecord` pins). A mutable `image.tag` is *recordable* here —
/// it is `resolve()` (the provisioning path) that refuses it outside an
/// `unpinned[]` context, so the record of *what was requested* is honest.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentRecord {
    /// The environment class.
    pub class: EnvironmentClass,
    /// The image identity/claim.
    pub image: ImageRef,
    /// A build context (content-addressed) the image was built from, if any.
    pub build_context: Option<ContentAddress>,
    /// The target platform triple (`linux/x86_64`, …) — declared, recorded.
    pub platform: String,
    /// The provisioning recipe.
    pub provisioning: ProvisioningRecipe,
    /// The `ContainmentPolicy` semantic+version ref the handle's
    /// `containment` slot pins (`PolicySlot::ResolvedRef`) — or the policy
    /// inline. Stored as the resolved pair for the record's identity.
    pub containment_policy_ref: (String, String),
    /// The declared resource bounds (the policy's `resources` member —
    /// re-recorded here so the definition's claim is self-contained).
    pub limits: hh_containment::policy::ResourceLimits,
    /// The declared nondeterminism.
    pub nondeterminism: Vec<NondeterminismDeclaration>,
    /// The `unpinned[]` context — the member names a `tag` selector the run
    /// deliberately admits un-pinned (records `unpinned_tag` on provision).
    pub unpinned: BTreeSet<String>,
    /// Extension members (surface — never identity-bearing).
    pub ext: BTreeMap<String, Json>,
}

impl EnvironmentRecord {
    /// The `idp/1` `Record` (semantic core = identity-bearing members; `ext` is
    /// surface). The pinned image address joins `refs` so `identify`'s
    /// sealed-form check sees a pinned ref when the record is sealed *into* a
    /// bundle.
    pub fn to_record(&self) -> Record {
        let mut r = Record::new(RecordKind::EnvironmentRecord)
            .semantic("class", Json::str(self.class.as_str()))
            .semantic("image", self.image.to_json())
            .semantic("platform", Json::str(self.platform.clone()))
            .semantic(
                "provisioning",
                Json::obj([(
                    "hooks",
                    Json::Obj(
                        self.provisioning
                            .hooks
                            .iter()
                            .map(|(k, v)| {
                                (
                                    k.clone(),
                                    Json::Arr(v.iter().map(|p| Json::str(p.clone())).collect()),
                                )
                            })
                            .collect(),
                    ),
                )]),
            )
            .semantic(
                "containment_policy",
                Json::obj([
                    (
                        "semantic_id",
                        Json::str(self.containment_policy_ref.0.clone()),
                    ),
                    (
                        "version_id",
                        Json::str(self.containment_policy_ref.1.clone()),
                    ),
                ]),
            )
            .semantic(
                "nondeterminism",
                Json::Arr(
                    self.nondeterminism
                        .iter()
                        .map(|n| {
                            Json::obj([
                                ("source", Json::str(n.source.clone())),
                                (
                                    "mitigation",
                                    match &n.mitigation {
                                        Some(m) => Json::str(m.clone()),
                                        None => Json::Null,
                                    },
                                ),
                            ])
                        })
                        .collect(),
                ),
            )
            .semantic(
                "unpinned",
                Json::Arr(self.unpinned.iter().map(|t| Json::str(t.clone())).collect()),
            );
        if let Some(bc) = &self.build_context {
            r = r.semantic("build_context", Json::str(bc.id()));
        }
        // The limits member is the policy's own canonical form (one schema —
        // the ResourceLimits member spelling is hh-containment's).
        r = r.semantic("limits", self.limits_json());
        // The pinned image address is a `refs` member (content-addressed —
        // the ref-by-content form) so a bundle sealing the record pins it.
        if let ImageRef::ContentAddress(ca) = &self.image {
            r = r.reference(Reference::Pinned(ca.id()));
        }
        for (k, v) in &self.ext {
            r = r.surface(k, v.clone());
        }
        r
    }

    /// `identify()` — `version_id` + `semantic_id` (the `environment_ref`).
    pub fn identify(&self) -> Result<Identity, EnvError> {
        identify(&self.to_record()).map_err(|e| EnvError::Identity(format!("{e:?}")))
    }

    /// `resolve()` — pin the image for provisioning. `ContentAddress` and
    /// `ForeignDigest` carry through; a `Tag` resolves to `UnpinnedTag` only
    /// when `unpinned[]` names it, else `UnresolvedRef` (ADR-0136 §3 — a
    /// mutable tag is never silently resolved to a moving target).
    pub fn resolve(&self) -> Result<ResolvedImage, EnvError> {
        match &self.image {
            ImageRef::ContentAddress(ca) => Ok(ResolvedImage::Address(ca.clone())),
            ImageRef::ForeignDigest {
                scheme,
                value,
                source,
            } => Ok(ResolvedImage::Foreign {
                scheme: scheme.clone(),
                value: value.clone(),
                source: source.clone(),
            }),
            ImageRef::Tag(t) => {
                if self.unpinned.contains(t) {
                    Ok(ResolvedImage::UnpinnedTag(t.clone()))
                } else {
                    Err(EnvError::UnresolvedRef { tag: t.clone() })
                }
            }
        }
    }

    /// Whether this record can back an R2 (or higher) reproducibility claim —
    /// `resolve()` must yield `Address`. A `foreign_digest` or unpinned tag
    /// caps the claim at R0/R1/R3 (N8).
    pub fn check_repro(&self, claim: &str) -> Result<(), EnvError> {
        let resolved = self.resolve()?;
        if matches!(claim, "R2") && !resolved.supports_r2() {
            return Err(EnvError::ReproClaimUnsupported {
                claim: claim.to_string(),
                image_kind: match resolved {
                    ResolvedImage::Address(_) => "content_address",
                    ResolvedImage::Foreign { .. } => "foreign_digest",
                    ResolvedImage::UnpinnedTag(_) => "unpinned_tag",
                },
            });
        }
        Ok(())
    }

    /// The `limits` member in the record's canonical form — a flat object of
    /// the set bounds (the `ResourceLimits` canonical projection — `set_fields`
    /// names them `resources.*`; here the member is already `limits`).
    fn limits_json(&self) -> Json {
        let mut m = BTreeMap::new();
        for (name, v) in self.limits.set_fields() {
            // `set_fields` spells `resources.x` — re-key to the record's local
            // member names (`x`) so the semantic core is self-describing.
            let short = name.strip_prefix("resources.").unwrap_or(name);
            m.insert(short.to_string(), Json::Int(v as i64));
        }
        Json::Obj(m)
    }
}
