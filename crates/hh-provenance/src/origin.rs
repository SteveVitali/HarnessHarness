//! [`Origin`] — the eight-variant sum answering *who produced this item* (§8.1 #3; ADR-0033 D1;
//! ADR-0048 CF-074) — and [`default_authority`]/[`default_text_authority`], the minting table
//! (§8.1 #2/#3; ADR-0033 D2/D6).
//!
//! An origin is **a fact, never an ordering**: the record carries who produced it, and
//! [`default_authority`] derives the class from that fact. Reference fields carry **identity
//! coordinates** (`version_id` / content-address strings): `hh-provenance` sits *below*
//! `hh-identity` in the build graph (`VersionedRef.provenance` is a `ProvenanceRecord`), so a
//! provenance record cannot embed the canonical `VersionedRef` without a recursive type; the
//! typed HIR `Ref<T>` projection lands at S1.4 above this layer (ADR-0230).

use crate::authority::{AuthorityClass, PersistenceScope, TaintTag};
use crate::record::Attestation;

/// The role a human origin plays (§8.1 #3 `human(author_ref, role)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HumanRole {
    /// The harness author.
    Author,
    /// The run's human principal.
    Principal,
    /// A human reviewer.
    Reviewer,
}

impl HumanRole {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            HumanRole::Author => "author",
            HumanRole::Principal => "principal",
            HumanRole::Reviewer => "reviewer",
        }
    }
}

/// `Origin` (§8.1 #3): WS-A3's sum plus `kernel` and `participant` (CF-074). Every field named
/// `*_ref`/`*_id` is an identity coordinate (a `version_id`/content address), not an embedded
/// record — see the module doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// `human(author_ref, role ∈ {author, principal, reviewer})`.
    Human {
        /// Identity coordinate of the human author/principal/reviewer.
        author_ref: String,
        /// The role played.
        role: HumanRole,
    },
    /// `model(model_ref, run_ref, response_id)` — an AgentProcess output.
    Model {
        /// Identity coordinate of the model (snapshot) that produced it.
        model_ref: String,
        /// The run the output was produced in.
        run_ref: String,
        /// The provider response id.
        response_id: String,
    },
    /// `tool(Ref<ToolCapability>, invocation_ref, inner_source?: TaintTag)` — a tool result.
    Tool {
        /// Identity coordinate of the `ToolCapability`.
        capability: String,
        /// The invocation that produced the result.
        invocation_ref: String,
        /// The tool's inner source (e.g. a page the tool fetched), when present.
        inner_source: Option<Box<TaintTag>>,
    },
    /// `evolution(candidate_id, hypothesis_ref)` — an evolution candidate.
    Evolution {
        /// The candidate's id.
        candidate_id: String,
        /// The hypothesis the candidate instantiates.
        hypothesis_ref: String,
    },
    /// `import(source_system, mapping_version)` — imported content.
    Import {
        /// The source system.
        source_system: String,
        /// The import mapping version.
        mapping_version: String,
    },
    /// `migration(from_dialect)` — migrated content.
    Migration {
        /// The dialect the content was migrated from.
        from_dialect: String,
    },
    /// `kernel(component_ref)` — the reference runtime itself (ledger facts, monitor
    /// decisions, deterministic validator verdicts).
    Kernel {
        /// The kernel component that produced the fact.
        component_ref: String,
    },
    /// `participant(participant_ref, hosting_mechanism)` — a hosted participant.
    Participant {
        /// Identity coordinate of the participant.
        participant_ref: String,
        /// The hosting mechanism (`hh.hosting/1` binding id).
        hosting_mechanism: String,
    },
}

impl Origin {
    /// Convenience: a human origin.
    pub fn human(author_ref: impl Into<String>, role: HumanRole) -> Origin {
        Origin::Human {
            author_ref: author_ref.into(),
            role,
        }
    }

    /// Convenience: a model origin.
    pub fn model(
        model_ref: impl Into<String>,
        run_ref: impl Into<String>,
        response_id: impl Into<String>,
    ) -> Origin {
        Origin::Model {
            model_ref: model_ref.into(),
            run_ref: run_ref.into(),
            response_id: response_id.into(),
        }
    }

    /// Convenience: a tool origin.
    pub fn tool(capability: impl Into<String>, invocation_ref: impl Into<String>) -> Origin {
        Origin::Tool {
            capability: capability.into(),
            invocation_ref: invocation_ref.into(),
            inner_source: None,
        }
    }

    /// Convenience: a kernel origin.
    pub fn kernel(component_ref: impl Into<String>) -> Origin {
        Origin::Kernel {
            component_ref: component_ref.into(),
        }
    }

    /// Convenience: a hosted-participant origin.
    pub fn participant(
        participant_ref: impl Into<String>,
        hosting_mechanism: impl Into<String>,
    ) -> Origin {
        Origin::Participant {
            participant_ref: participant_ref.into(),
            hosting_mechanism: hosting_mechanism.into(),
        }
    }

    /// Convenience: an import origin.
    pub fn import(source_system: impl Into<String>, mapping_version: impl Into<String>) -> Origin {
        Origin::Import {
            source_system: source_system.into(),
            mapping_version: mapping_version.into(),
        }
    }

    /// Convenience: an evolution origin.
    pub fn evolution(candidate_id: impl Into<String>, hypothesis_ref: impl Into<String>) -> Origin {
        Origin::Evolution {
            candidate_id: candidate_id.into(),
            hypothesis_ref: hypothesis_ref.into(),
        }
    }

    /// Whether this origin is a `delegate`-class producer — `model`, `evolution` or
    /// `participant`. A `delegate`-origin endorser is **never valid** (§8.1 #2 endorse;
    /// "the model never endorses" — constitutional rule).
    pub fn is_delegate_class(&self) -> bool {
        matches!(
            self,
            Origin::Model { .. } | Origin::Evolution { .. } | Origin::Participant { .. }
        )
    }

    /// The variant tag — the `kind` spelling used in the canonical JSON and in lowered
    /// carriers that keep only the discriminant (P7).
    pub fn tag(&self) -> &'static str {
        match self {
            Origin::Human { .. } => "human",
            Origin::Model { .. } => "model",
            Origin::Tool { .. } => "tool",
            Origin::Evolution { .. } => "evolution",
            Origin::Import { .. } => "import",
            Origin::Migration { .. } => "migration",
            Origin::Kernel { .. } => "kernel",
            Origin::Participant { .. } => "participant",
        }
    }
}

/// The sealed-definition context [`default_authority_in`] consults (§8.1 #3 minting: the
/// `environment` class is conferred only for closed-schema structured values from
/// **closed-world tools declared in the sealed definition**). The context is conferred by the
/// runtime — it is the *sealed* definition's declaration, kernel-verified at `seal`, never the
/// record's own claim (CC2 holds: authority is still derived, never declared by the payload).
/// Empty context ⇒ no tool can mint `environment` (the pre-S1.4 floor).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MintingContext {
    /// Identity coordinates (`semantic_id`s) of `ToolCapability`s declared **closed-world** in
    /// the sealed definition (a tool whose declared effects are all `world = closed`, or
    /// `effects = pure` — computed by `hh-hir`'s `SealedDefinition`).
    pub closed_world_tools: std::collections::BTreeSet<String>,
}

/// `default_authority(origin, scope, attestation?) → AuthorityClass` (§8.1 #2): total, pure,
/// the minting table of §8.1 #3. Returns `unverified` for anything unmatched; **never reads a
/// payload's own claim** — authority is derived, never declared (CC2; ADR-0033 D2/D6).
///
/// - `scope` is **never read** — `default_authority` reads no locator/scope/directory fact, so
///   identical bytes in different locations mint the same class (`LocationElevation` refused;
///   ADR-0063 L1; AC-R-2.1.5-9's pure-function half).
/// - `attestation` can never *raise* the minted class (endorsement is the only raiser); a
///   structurally inconsistent attestation (a self-claim with no anchor/verifier — the
///   attestation-failed case) mints `unverified`. Trust-root verification itself is
///   [`crate::record::verify_attestation`]; `pin` consults it (ADR-0035 D2).
/// - `tool` mints `external` without a minting context, and `environment` only when the
///   capability is declared **closed-world in the sealed definition** ([`default_authority_in`]
///   with a non-empty [`MintingContext`]). Free text never rises: text values mint through
///   [`default_text_authority`], which caps at `external` regardless (R-TEXT). The
///   `validator`-basis endorsement path (`external → environment`, kernel endorser only)
///   remains for content minted before the sealed-definition declaration existed.
///   DF-S1.3-3 **closed** at S1.4.
/// - `participant` mints `unverified`: the `≤ delegate` claim class applies only to content the
///   Hosting ABI vouches for, and no Hosting ABI exists at Stage 1 (T-LCD-06/07; ADR-0033 D2).
///   Tracked: DF-S1.3-2.
pub fn default_authority(
    origin: &Origin,
    scope: PersistenceScope,
    attestation: Option<&Attestation>,
) -> AuthorityClass {
    default_authority_in(origin, scope, attestation, &MintingContext::default())
}

/// `default_authority` with a sealed-definition [`MintingContext`] (§8.1 #3; CC8 additive —
/// the two-arg form delegates with an empty context). The only behavioural difference: a
/// `tool` origin whose capability is declared closed-world in the sealed definition mints
/// `environment` instead of `external`. Callers minting a *free-text* (`Text` leaf) record use
/// [`default_text_authority`] regardless — R-TEXT caps text at `external` either way, so a
/// closed-world declaration can never lift prose above `external`.
pub fn default_authority_in(
    origin: &Origin,
    scope: PersistenceScope,
    attestation: Option<&Attestation>,
    ctx: &MintingContext,
) -> AuthorityClass {
    let _ = scope; // never read: location may not elevate authority (ADR-0063 L1).
    if let Some(att) = attestation {
        if !att.self_consistent() {
            return AuthorityClass::Unverified;
        }
    }
    match origin {
        Origin::Kernel { .. } => AuthorityClass::Kernel,
        Origin::Human { .. } => AuthorityClass::Principal,
        Origin::Model { .. } | Origin::Evolution { .. } => AuthorityClass::Delegate,
        Origin::Tool { capability, .. } => {
            if ctx.closed_world_tools.contains(capability) {
                AuthorityClass::Environment
            } else {
                AuthorityClass::External
            }
        }
        Origin::Import { .. } | Origin::Migration { .. } | Origin::Participant { .. } => {
            AuthorityClass::Unverified
        }
    }
}

/// `default_text_authority(origin, attestation?)` — **R-TEXT** (§8.1 #3): *all free text* not
/// from kernel/definition/principal mints `external`. Free text is capped at `external` —
/// `min(default_authority(origin), external)` — except the three text authorities that pass
/// through (`kernel`, `principal`; `definition` text is reached only via `seal`, not minted
/// from an origin). Imported/migrated/unvouched free text stays `unverified` — the distinct
/// bottom, never coerced into `external` (T-LCD-07).
pub fn default_text_authority(
    origin: &Origin,
    attestation: Option<&Attestation>,
) -> AuthorityClass {
    match origin {
        Origin::Kernel { .. } | Origin::Human { .. } => {
            default_authority(origin, PersistenceScope::Run, attestation)
        }
        _ => {
            let a = default_authority(origin, PersistenceScope::Run, attestation);
            a.min(AuthorityClass::External)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn human() -> Origin {
        Origin::human("human:alice", HumanRole::Principal)
    }

    #[test]
    fn minting_table_is_total_and_matches_the_spec() {
        let cases: Vec<(Origin, AuthorityClass)> = vec![
            (Origin::kernel("kernel:ledger"), AuthorityClass::Kernel),
            (human(), AuthorityClass::Principal),
            (
                Origin::model("model:m1", "run:r1", "resp:1"),
                AuthorityClass::Delegate,
            ),
            (
                Origin::evolution("cand:c1", "hyp:h1"),
                AuthorityClass::Delegate,
            ),
            (
                Origin::tool("tool:shell", "inv:1"),
                AuthorityClass::External,
            ),
            (Origin::import("jira", "v1"), AuthorityClass::Unverified),
            (
                Origin::Migration {
                    from_dialect: "hir/0".into(),
                },
                AuthorityClass::Unverified,
            ),
            (
                Origin::participant("p:1", "hh.hosting/1"),
                AuthorityClass::Unverified,
            ),
        ];
        for (o, want) in cases {
            assert_eq!(
                default_authority(&o, PersistenceScope::Run, None),
                want,
                "origin {o:?}"
            );
        }
    }

    #[test]
    fn scope_is_never_read_location_neutrality() {
        // AC-R-2.1.5-9's pure half / ADR-0063 L1: identical origin in every scope mints the
        // same class — a "trusted directory" cannot elevate.
        let o = Origin::import("marketplace", "v1");
        for s in [
            PersistenceScope::Definition,
            PersistenceScope::User,
            PersistenceScope::Project,
            PersistenceScope::Session,
            PersistenceScope::Run,
            PersistenceScope::Turn,
        ] {
            assert_eq!(
                default_authority(&o, s, None),
                AuthorityClass::Unverified,
                "scope {s:?} must not change the minted class"
            );
        }
    }

    #[test]
    fn r_text_caps_free_text_at_external() {
        // Model free text is external (not delegate); human/kernel text keeps its class;
        // imported text stays unverified (never coerced to external — T-LCD-07).
        assert_eq!(
            default_text_authority(&Origin::model("m", "r", "resp"), None),
            AuthorityClass::External
        );
        assert_eq!(
            default_text_authority(&Origin::tool("t", "i"), None),
            AuthorityClass::External
        );
        assert_eq!(
            default_text_authority(&human(), None),
            AuthorityClass::Principal
        );
        assert_eq!(
            default_text_authority(&Origin::kernel("k"), None),
            AuthorityClass::Kernel
        );
        assert_eq!(
            default_text_authority(&Origin::import("s", "v"), None),
            AuthorityClass::Unverified
        );
    }

    #[test]
    fn closed_world_tool_mints_environment_only_with_the_context() {
        // DF-S1.3-3: `tool` mints `external` with no sealed-definition context, and
        // `environment` when the capability is declared closed-world in the sealed definition.
        let tool = Origin::tool("tool:closed-read", "inv:1");
        assert_eq!(
            default_authority(&tool, PersistenceScope::Run, None),
            AuthorityClass::External
        );
        let mut ctx = MintingContext::default();
        ctx.closed_world_tools
            .insert("tool:closed-read".to_string());
        assert_eq!(
            default_authority_in(&tool, PersistenceScope::Run, None, &ctx),
            AuthorityClass::Environment
        );
        // A tool NOT declared closed-world still mints external under the same context.
        let open_tool = Origin::tool("tool:web", "inv:2");
        assert_eq!(
            default_authority_in(&open_tool, PersistenceScope::Run, None, &ctx),
            AuthorityClass::External
        );
        // R-TEXT: free text from a closed-world tool still caps at external — a closed-world
        // declaration never lifts prose (the mint applies to closed-schema structured values).
        assert_eq!(
            default_text_authority(&tool, None),
            AuthorityClass::External
        );
    }

    #[test]
    fn a_delegate_origin_is_marked() {
        assert!(Origin::model("m", "r", "x").is_delegate_class());
        assert!(Origin::evolution("c", "h").is_delegate_class());
        assert!(Origin::participant("p", "m").is_delegate_class());
        assert!(!human().is_delegate_class());
        assert!(!Origin::kernel("k").is_delegate_class());
    }
}
