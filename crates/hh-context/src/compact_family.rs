//! §5c.2 **C1 extension** — the compaction-strategy family (R-2.4.2, Stage 5;
//! ADR-0075/0076/0077 (e)): the `VariantDeclaration` every
//! [`CompactionStrategy`] now answers, the extended variant set
//! (`summarize_rolling`, `offload_then_summarize`, `structured_checkpoint`,
//! `fresh_window_with_notes`, `world_state_refresh`, `provider_compaction`),
//! the model-owned compaction boundary (`β(compact) = model` — ADR-0076 d7),
//! the conditioned `HarnessRule{action: set_compaction_policy}` /
//! `ProfileRule{compaction_reminder}` wiring (ADR-0076 d6's three homes), and
//! the soft-threshold re-arm rule (CF-169; ADR-0106).
//!
//! Registration (§5c.2 `declare()` row): a variant's
//! `model_conditioned_rules` carry complete `AssumptionDebtRecord`s and none
//! is conditioned on a literal model identity — the *same* shared check
//! `context_policy` registration runs
//! ([`crate::policy::check_conditioned_rules`]; T-LCD-01/T-LCD-05 — one
//! refusal vocabulary, CC7).

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::records::AssumptionDebtRecord;
use hh_wire::json::Json;

use crate::compact::{
    assess, execute, CompactError, CompactInput, CompactOutcome, CompactionOp, CompactionProposal,
    CompactionRecord, CompactionStatus, CompactionStrategy, InputReduction, ModelCallMembers,
    Placeholder, RestructureSpec, SummarizeItem,
};
use crate::events::EventSink;
use crate::plan::Candidate;
use crate::policy::{check_conditioned_rules, ConditionedRule, RegistrationError, RuleCondition};
use crate::vocab::{CandidateKind, CandidateState, Retention};

// ─────────────────────────────────────────────────────────────────────────────
// VariantDeclaration (§5c.2 `declare()` row)
// ─────────────────────────────────────────────────────────────────────────────

/// `control_boundary_compact ∈ {code, model}` (ADR-0076 d7): `code` — the
/// variant's ops are proposed by code and the kernel executes them; `model`
/// — the variant's ops arrive as model-owned effects (`offload_note`,
/// `new_window`, `fold`) the kernel re-checks through [`model_owned_ops`] +
/// [`model_owned_proposal`] + [`execute`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlBoundaryCompact {
    /// `code` — the variant proposes; the kernel executes.
    Code,
    /// `model` — β(compact) = model; the proposal is a model-owned effect.
    Model,
}

impl ControlBoundaryCompact {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ControlBoundaryCompact::Code => "code",
            ControlBoundaryCompact::Model => "model",
        }
    }
}

/// `capabilities{incremental_summary, split_turn, hard_reset,
/// handles_requests, reactive_overflow}` — the closed capability set a
/// `VariantDeclaration` carries (§5c.2 declare row).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactionCapabilities {
    /// The variant chains `previous_summary_ref` (rolling incremental).
    pub incremental_summary: bool,
    /// The variant's proposals may split a model turn (mid-pair cuts are
    /// still I-PAIR-checked — the capability declares the *intent*, the
    /// kernel still enforces).
    pub split_turn: bool,
    /// The variant can drop the whole optional set (`fresh_window`).
    pub hard_reset: bool,
    /// The variant answers `request_*` triggers.
    pub handles_requests: bool,
    /// The variant answers `overflow_reactive`.
    pub reactive_overflow: bool,
}

/// `VariantDeclaration{variant_id, op_kinds, deterministic,
/// control_boundary_compact, capabilities{…}, model_conditioned_rules[with
/// AssumptionDebtRecord], required_inputs ⊇ {ModelProfile, ResourceAccount},
/// fallback_variant?}` (§5c.2 `declare()` row; ADR-0075 d2 — `semantic_id`
/// is the variant identity, T-LCD-10).
#[derive(Debug, Clone)]
pub struct VariantDeclaration {
    /// The variant's identity coordinate.
    pub variant_id: String,
    /// The `CompactionOp` kinds the variant can propose.
    pub op_kinds: BTreeSet<String>,
    /// `deterministic` — no model call; admissible under
    /// `deterministic_replay` (I-DET).
    pub deterministic: bool,
    /// `control_boundary_compact`.
    pub control_boundary_compact: ControlBoundaryCompact,
    /// The closed capability record.
    pub capabilities: CompactionCapabilities,
    /// The conditioned rules + debt records.
    pub model_conditioned_rules: Vec<ConditionedRule>,
    /// `required_inputs ⊇ {ModelProfile, ResourceAccount}`.
    pub required_inputs: BTreeSet<String>,
    /// `fallback_variant?` — the I-FALLBACK rung after `proposal.fallback`.
    pub fallback_variant: Option<String>,
}

/// `required_inputs ⊇ {ModelProfile, ResourceAccount}` — the envelope every
/// `compaction_strategy` declaration must name (§5c.2 — the same envelope
/// [`crate::policy::REQUIRED_POLICY_INPUTS`] fixes for `context_policy`).
pub fn required_variant_inputs() -> BTreeSet<String> {
    crate::policy::REQUIRED_POLICY_INPUTS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl VariantDeclaration {
    /// The minimal C0 declaration every `CompactionStrategy` implementor
    /// inherits by default (deterministic, code boundary, `{evict, offload}`,
    /// no conditioned rules): the `declare()` trait default returns it.
    pub fn minimal(variant_id: &str) -> Self {
        VariantDeclaration {
            variant_id: variant_id.to_string(),
            op_kinds: ["evict".to_string(), "offload".to_string()]
                .into_iter()
                .collect(),
            deterministic: true,
            control_boundary_compact: ControlBoundaryCompact::Code,
            capabilities: CompactionCapabilities::default(),
            model_conditioned_rules: Vec::new(),
            required_inputs: required_variant_inputs(),
            fallback_variant: None,
        }
    }

    /// The canonical JSON (registry-facing).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("variant_id", Json::str(self.variant_id.clone())),
            (
                "op_kinds",
                Json::Arr(self.op_kinds.iter().map(|k| Json::str(k.clone())).collect()),
            ),
            ("deterministic", Json::Bool(self.deterministic)),
            (
                "control_boundary_compact",
                Json::str(self.control_boundary_compact.as_str()),
            ),
            (
                "capabilities",
                Json::obj([
                    (
                        "incremental_summary",
                        Json::Bool(self.capabilities.incremental_summary),
                    ),
                    ("split_turn", Json::Bool(self.capabilities.split_turn)),
                    ("hard_reset", Json::Bool(self.capabilities.hard_reset)),
                    (
                        "handles_requests",
                        Json::Bool(self.capabilities.handles_requests),
                    ),
                    (
                        "reactive_overflow",
                        Json::Bool(self.capabilities.reactive_overflow),
                    ),
                ]),
            ),
            (
                "model_conditioned_rules",
                Json::Arr(
                    self.model_conditioned_rules
                        .iter()
                        .map(|r| Json::str(r.rule_id.clone()))
                        .collect(),
                ),
            ),
            (
                "required_inputs",
                Json::Arr(
                    self.required_inputs
                        .iter()
                        .map(|i| Json::str(i.clone()))
                        .collect(),
                ),
            ),
            (
                "fallback_variant",
                self.fallback_variant
                    .as_ref()
                    .map(|f| Json::str(f.clone()))
                    .unwrap_or(Json::Null),
            ),
        ])
    }
}

/// `declare()` registration check (§5c.2 row 4; AC-R-2.4.2-6's "a
/// model-identity branch fails registration"): the shared conditioned-rule
/// check — complete debt records, no model-identity condition, the
/// `{ModelProfile, ResourceAccount}` envelope — plus `op_kinds` naming only
/// the closed op grammar (an `ext` spelling is refused — the grammar closes
/// by dialect bump).
pub fn check_variant_declaration(d: &VariantDeclaration) -> Result<(), RegistrationError> {
    check_conditioned_rules(&d.model_conditioned_rules, &d.required_inputs)?;
    const OP_KINDS: &[&str] = &[
        "evict",
        "offload",
        "summarize",
        "restructure",
        "provider_compact",
    ];
    for k in &d.op_kinds {
        if !OP_KINDS.contains(&k.as_str()) {
            return Err(RegistrationError::UndeclaredNonDeterminism {
                rule_id: format!("{}/op:{k}", d.variant_id),
            });
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// The deterministic-extractor and provider ports
// ─────────────────────────────────────────────────────────────────────────────

/// `Restructure{extractor: deterministic{projection_ref}}`'s port (§5c.2 op
/// table; ADR-0076 d1): a deterministic projection over the consumed items'
/// bodies — the kernel `Projection` derivation (`authority = ⊔ inputs`,
/// `validity = valid` when the projection is total) producing an
/// `Artifact{kind: compaction_checkpoint}`. A `restructure` op naming a ref
/// the bound port does not answer is `UnsupportedOp` (never silently
/// skipped).
pub trait CheckpointExtractor {
    /// The projection's identity coordinate (the op's `extractor` member).
    fn extractor_ref(&self) -> &str;
    /// `extract(items, schema_ref, section_order) → {body, tokens}` — pure.
    fn extract(
        &self,
        items: &[SummarizeItem],
        schema_ref: &str,
        section_order: &[String],
    ) -> Result<CheckpointOutput, String>;
}

/// The checkpoint body a `CheckpointExtractor` returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointOutput {
    /// The `compaction_checkpoint` artifact body (each `section_order`
    /// member rendered — a `Text` leaf / typed value per section).
    pub body: String,
    /// The estimated tokens.
    pub tokens: u64,
}

/// `provider_compaction`'s port input (§5c.2 ADR-0076 d8): the consumed
/// blocks plus the profile-declared gateway capability the call delegates
/// through.
#[derive(Debug, Clone)]
pub struct ProviderCompactInput {
    /// The items the provider was asked to compact (bodies caller-joined —
    /// CC3, never a silent empty string).
    pub items: Vec<SummarizeItem>,
    /// The declared gateway capability ref.
    pub capability_ref: String,
}

/// The provider's answer: the summary plus the forgotten set it reports —
/// `None` ⇒ `forgotten = {range: all_prior}` and the loss lands in the
/// profile's lowering-loss report (T-LCD-11; `forgotten_range` on the
/// record).
#[derive(Debug, Clone)]
pub struct ProviderCompactOutcome {
    /// The provider's summary text (`delegate`-derived).
    pub summary_text: String,
    /// The summary's estimated tokens.
    pub summary_tokens: u64,
    /// The `context_item_id`s the provider reports compacted — `None` means
    /// it cannot report (the `all_prior` leg).
    pub forgotten_ids: Option<Vec<String>>,
    /// The provider's own usage record (`provider.notice{kind:
    /// compaction_applied}`'s `usage.raw` — posted as the provider's own
    /// charge, ADR-0039 R-ACC-3).
    pub usage: Json,
}

/// The provider-compaction port — `execute` delegates to it under the
/// profile-declared capability; the returned summary is `delegate`-derived
/// with `derived_from` = the compacted blocks (§5c.2 ADR-0076 d8).
pub trait ProviderCompaction {
    /// `provider_compact(input)` → the provider's summary + forgotten set.
    fn provider_compact(
        &self,
        input: &ProviderCompactInput,
    ) -> Result<ProviderCompactOutcome, String>;
}

// ─────────────────────────────────────────────────────────────────────────────
// The C1 family variants
// ─────────────────────────────────────────────────────────────────────────────

/// `hh/summarize-rolling@1`'s variant tag.
pub const SUMMARIZE_ROLLING_REF: &str = "hh/summarize-rolling@1";
/// `hh/offload-then-summarize@1`'s variant tag.
pub const OFFLOAD_THEN_SUMMARIZE_REF: &str = "hh/offload-then-summarize@1";
/// `hh/structured-checkpoint@1`'s variant tag.
pub const STRUCTURED_CHECKPOINT_REF: &str = "hh/structured-checkpoint@1";
/// `hh/fresh-window-with-notes@1`'s variant tag.
pub const FRESH_WINDOW_WITH_NOTES_REF: &str = "hh/fresh-window-with-notes@1";
/// `hh/world-state-refresh@1`'s variant tag.
pub const WORLD_STATE_REFRESH_REF: &str = "hh/world-state-refresh@1";
/// `hh/provider-compaction@1`'s variant tag.
pub const PROVIDER_COMPACTION_REF: &str = "hh/provider-compaction@1";
/// The `control_boundary_compact = model` proposal's variant tag
/// (`β(compact) = model` — the model's ops, the kernel's invariants).
pub const MODEL_OWNED_REF: &str = "hh/model-owned@1";

/// `summarize_rolling{keep_recent_tokens, reserve_tokens,
/// summarizer_profile, instructions_ref, incremental, split_turn,
/// retain_principal_tokens, max_summary_tokens, input_reduction}` (§5c.2 C1
/// family; ADR-0076 (e)). Keeps the newest `keep_recent_tokens` of the view,
/// protects the most recent `retain_principal_tokens` of principal-authored
/// items (I-REQ's pinning half — a rule *may* pin older principal items),
/// and summarizes the older optional tail. `incremental` chains
/// `previous_summary_ref`; `split_turn` declares the capability (cut legality
/// stays the kernel's — I-CUT/I-PAIR).
#[derive(Debug, Clone)]
pub struct SummarizeRolling {
    /// Tokens of the newest tail kept verbatim.
    pub keep_recent_tokens: u64,
    /// Headroom the compaction targets beyond the reclaim minimum.
    pub reserve_tokens: u64,
    /// `summarizer_profile` — the declared `ProfileRef` the summarize call
    /// is gated on (ADR-0012: a model-set member — the `compaction` role).
    pub summarizer_profile: String,
    /// The compaction-prompt `Text` ref.
    pub instructions_ref: Option<String>,
    /// Chain `previous_summary_ref`.
    pub incremental: bool,
    /// The declared `split_turn` capability.
    pub split_turn: bool,
    /// Extra principal-authored tokens the variant protects.
    pub retain_principal_tokens: u64,
    /// The summary-length bound.
    pub max_summary_tokens: Option<u64>,
    /// The declared `InputReduction`.
    pub input_reduction: Option<InputReduction>,
}

impl CompactionStrategy for SummarizeRolling {
    fn variant_ref(&self) -> &str {
        SUMMARIZE_ROLLING_REF
    }

    fn propose(
        &self,
        input: &CompactInput,
        assessment: &crate::compact::Assessment,
    ) -> Result<CompactionProposal, CompactError> {
        let flat = input.flattened();
        // The keep boundary — walk the newest tail until `keep_recent_tokens`
        // is covered; everything before it is the summarization candidate
        // set (optional only — I-REQ stays the kernel's check at
        // `check_proposal`, the variant pre-filters per d2's purity).
        let mut tail = 0u64;
        let mut keep_from = flat.len();
        for (i, f) in flat.iter().enumerate().rev() {
            tail += f.item.tokens;
            keep_from = i;
            if tail >= self.keep_recent_tokens + self.reserve_tokens {
                break;
            }
        }
        // `retain_principal_tokens` — the newest principal-authored items
        // the rule pinned stay out of the forgotten set even below the cut.
        let mut principal_kept = 0u64;
        let mut keep_ids: BTreeSet<usize> = BTreeSet::new();
        for (i, f) in flat.iter().enumerate().rev() {
            if principal_kept >= self.retain_principal_tokens {
                break;
            }
            if f.candidate
                .as_ref()
                .map(|c| c.label.authority)
                .is_some_and(|a| a == hh_provenance::AuthorityClass::Principal)
            {
                keep_ids.insert(i);
                principal_kept += f.item.tokens;
            }
        }
        let input_ids: Vec<String> = flat[..keep_from.min(flat.len())]
            .iter()
            .filter(|f| {
                f.candidate
                    .as_ref()
                    .is_some_and(|c| !matches!(c.retention, Retention::Required))
            })
            .filter(|f| !keep_ids.contains(&(f.flat_index as usize)))
            .map(|f| f.item.context_item_id.clone())
            .collect();
        if input_ids.is_empty() {
            return Ok(CompactionProposal::mint(
                SUMMARIZE_ROLLING_REF,
                vec![],
                None,
                0,
            ));
        }
        let insert_at = flat
            .iter()
            .find(|f| f.item.context_item_id == input_ids[0])
            .map(|f| f.flat_index)
            .unwrap_or(0);
        let freed: u64 = flat
            .iter()
            .filter(|f| input_ids.contains(&f.item.context_item_id))
            .map(|f| f.item.tokens)
            .sum();
        Ok(CompactionProposal::mint_with(
            SUMMARIZE_ROLLING_REF,
            vec![CompactionOp::Summarize {
                input_ids: input_ids.clone(),
                insert_at,
            }],
            Some(vec![CompactionOp::Evict {
                item_ids: input_ids,
                placeholder: Placeholder::KernelOmission,
            }]),
            freed.max(assessment.min_reclaim),
            Some(ModelCallMembers {
                summarizer_profile: Some(self.summarizer_profile.clone()),
                max_summary_tokens: self.max_summary_tokens,
                input_reduction: self.input_reduction.clone(),
                instructions_ref: self.instructions_ref.clone(),
                restructure: None,
            }),
        ))
    }

    fn declare(&self) -> VariantDeclaration {
        VariantDeclaration {
            variant_id: SUMMARIZE_ROLLING_REF.to_string(),
            op_kinds: ["evict".to_string(), "summarize".to_string()]
                .into_iter()
                .collect(),
            deterministic: false,
            control_boundary_compact: ControlBoundaryCompact::Code,
            capabilities: CompactionCapabilities {
                incremental_summary: self.incremental,
                split_turn: self.split_turn,
                hard_reset: false,
                handles_requests: true,
                reactive_overflow: true,
            },
            model_conditioned_rules: Vec::new(),
            required_inputs: required_variant_inputs(),
            fallback_variant: Some(crate::compact::EVICT_OLDEST_REF.to_string()),
        }
    }
}

/// `offload_then_summarize` (§5c.2 C1 family): every optional item carrying
/// a readable `OffloadHandle` is `Offload`ed first (the handle keeps a
/// content-addressed path back); the remaining optional tail — up to the
/// assessment's target — is `Summarize`d under the declared
/// `summarizer_profile`.
#[derive(Debug, Clone)]
pub struct OffloadThenSummarize {
    /// The declared summariser profile ref.
    pub summarizer_profile: String,
    /// The summary-length bound.
    pub max_summary_tokens: Option<u64>,
    /// The declared `InputReduction`.
    pub input_reduction: Option<InputReduction>,
}

impl CompactionStrategy for OffloadThenSummarize {
    fn variant_ref(&self) -> &str {
        OFFLOAD_THEN_SUMMARIZE_REF
    }

    fn propose(
        &self,
        input: &CompactInput,
        assessment: &crate::compact::Assessment,
    ) -> Result<CompactionProposal, CompactError> {
        let flat = input.flattened();
        let mut offloadable: Vec<String> = Vec::new();
        let mut rest: Vec<String> = Vec::new();
        for f in &flat {
            let Some(c) = &f.candidate else { continue };
            if matches!(c.retention, Retention::Required) {
                continue;
            }
            if c.handle
                .as_ref()
                .is_some_and(|h| !h.read_capability.is_empty())
            {
                offloadable.push(f.item.context_item_id.clone());
            } else {
                rest.push(f.item.context_item_id.clone());
            }
        }
        let mut ops = Vec::new();
        if !offloadable.is_empty() {
            ops.push(CompactionOp::Offload {
                item_ids: offloadable.clone(),
            });
        }
        // Summarize the remainder, oldest-first, until the reclaim target is
        // plausibly met (I-RECLAIM still measures the executed view).
        let offloaded_tokens: u64 = flat
            .iter()
            .filter(|f| offloadable.contains(&f.item.context_item_id))
            .map(|f| f.item.tokens)
            .sum();
        let mut summarize_ids = Vec::new();
        let mut acc = offloaded_tokens;
        for f in &flat {
            if acc >= assessment.target_reclaim.max(assessment.min_reclaim) {
                break;
            }
            if rest.contains(&f.item.context_item_id) {
                acc += f.item.tokens;
                summarize_ids.push(f.item.context_item_id.clone());
            }
        }
        let insert_at = flat
            .iter()
            .find(|f| f.item.context_item_id == summarize_ids.first().cloned().unwrap_or_default())
            .map(|f| f.flat_index)
            .unwrap_or(0);
        if !summarize_ids.is_empty() {
            ops.push(CompactionOp::Summarize {
                input_ids: summarize_ids,
                insert_at,
            });
        }
        Ok(CompactionProposal::mint_with(
            OFFLOAD_THEN_SUMMARIZE_REF,
            ops,
            None,
            acc,
            Some(ModelCallMembers {
                summarizer_profile: Some(self.summarizer_profile.clone()),
                max_summary_tokens: self.max_summary_tokens,
                input_reduction: self.input_reduction.clone(),
                instructions_ref: None,
                restructure: None,
            }),
        ))
    }

    fn declare(&self) -> VariantDeclaration {
        VariantDeclaration {
            variant_id: OFFLOAD_THEN_SUMMARIZE_REF.to_string(),
            op_kinds: [
                "evict".to_string(),
                "offload".to_string(),
                "summarize".to_string(),
            ]
            .into_iter()
            .collect(),
            deterministic: false,
            control_boundary_compact: ControlBoundaryCompact::Code,
            capabilities: CompactionCapabilities {
                handles_requests: true,
                ..CompactionCapabilities::default()
            },
            model_conditioned_rules: Vec::new(),
            required_inputs: required_variant_inputs(),
            fallback_variant: Some(crate::compact::EVICT_OLDEST_REF.to_string()),
        }
    }
}

/// `structured_checkpoint{schema_ref, section_order, projection_ref}`
/// (§5c.2 C1 family): a `Restructure` op under the declared *deterministic*
/// extractor — the kernel `Projection` derivation (`authority = ⊔ inputs`,
/// `validity = valid` when the projection is total) producing
/// `Artifact{kind: compaction_checkpoint}`.
#[derive(Debug, Clone)]
pub struct StructuredCheckpoint {
    /// The checkpoint schema's identity coordinate.
    pub schema_ref: String,
    /// The section order the extractor renders.
    pub section_order: Vec<String>,
    /// The deterministic extractor's `projection_ref`.
    pub projection_ref: String,
}

impl CompactionStrategy for StructuredCheckpoint {
    fn variant_ref(&self) -> &str {
        STRUCTURED_CHECKPOINT_REF
    }

    fn propose(
        &self,
        input: &CompactInput,
        _assessment: &crate::compact::Assessment,
    ) -> Result<CompactionProposal, CompactError> {
        let flat = input.flattened();
        let input_ids: Vec<String> = flat
            .iter()
            .filter(|f| {
                f.candidate
                    .as_ref()
                    .is_some_and(|c| !matches!(c.retention, Retention::Required))
            })
            .map(|f| f.item.context_item_id.clone())
            .collect();
        if input_ids.is_empty() {
            return Ok(CompactionProposal::mint(
                STRUCTURED_CHECKPOINT_REF,
                vec![],
                None,
                0,
            ));
        }
        let insert_at = flat
            .iter()
            .find(|f| f.item.context_item_id == input_ids[0])
            .map(|f| f.flat_index)
            .unwrap_or(0);
        let freed: u64 = flat
            .iter()
            .filter(|f| input_ids.contains(&f.item.context_item_id))
            .map(|f| f.item.tokens)
            .sum();
        Ok(CompactionProposal::mint_with(
            STRUCTURED_CHECKPOINT_REF,
            vec![CompactionOp::Restructure {
                input_ids: input_ids.clone(),
                extractor: self.projection_ref.clone(),
            }],
            Some(vec![CompactionOp::Evict {
                item_ids: input_ids,
                placeholder: Placeholder::KernelOmission,
            }]),
            freed,
            Some(ModelCallMembers {
                restructure: Some(RestructureSpec {
                    schema_ref: self.schema_ref.clone(),
                    section_order: self.section_order.clone(),
                    insert_at,
                }),
                ..ModelCallMembers::default()
            }),
        ))
    }

    fn declare(&self) -> VariantDeclaration {
        VariantDeclaration {
            variant_id: STRUCTURED_CHECKPOINT_REF.to_string(),
            op_kinds: ["evict".to_string(), "restructure".to_string()]
                .into_iter()
                .collect(),
            // The extractor is a declared *deterministic* projection — the
            // variant is admissible under `deterministic_replay` (I-DET).
            deterministic: true,
            control_boundary_compact: ControlBoundaryCompact::Code,
            capabilities: CompactionCapabilities::default(),
            model_conditioned_rules: Vec::new(),
            required_inputs: required_variant_inputs(),
            fallback_variant: Some(crate::compact::EVICT_OLDEST_REF.to_string()),
        }
    }
}

/// `world_state_refresh` (§5c.2 C1 family): evicts `environment_state`
/// items — they refresh by re-observation, so the deterministic answer to a
/// stale-world-state view is to forget the old state entirely (the
/// `kernel_omission` placeholder marks the range). `item_kinds` is the
/// caller-joined kind table (`CompactInput.item_kinds`); a view carrying no
/// `environment_state` items proposes nothing.
#[derive(Debug, Clone, Default)]
pub struct WorldStateRefresh;

impl CompactionStrategy for WorldStateRefresh {
    fn variant_ref(&self) -> &str {
        WORLD_STATE_REFRESH_REF
    }

    fn propose(
        &self,
        input: &CompactInput,
        _assessment: &crate::compact::Assessment,
    ) -> Result<CompactionProposal, CompactError> {
        let flat = input.flattened();
        let ids: Vec<String> = flat
            .iter()
            .filter(|f| {
                input
                    .item_kinds
                    .get(&f.item.context_item_id)
                    .map(|k| k.as_str())
                    == Some("environment_state")
            })
            .filter(|f| {
                f.candidate
                    .as_ref()
                    .is_some_and(|c| !matches!(c.retention, Retention::Required))
            })
            .map(|f| f.item.context_item_id.clone())
            .collect();
        if ids.is_empty() {
            return Ok(CompactionProposal::mint(
                WORLD_STATE_REFRESH_REF,
                vec![],
                None,
                0,
            ));
        }
        let freed: u64 = flat
            .iter()
            .filter(|f| ids.contains(&f.item.context_item_id))
            .map(|f| f.item.tokens)
            .sum();
        Ok(CompactionProposal::mint(
            WORLD_STATE_REFRESH_REF,
            vec![CompactionOp::Evict {
                item_ids: ids,
                placeholder: Placeholder::KernelOmission,
            }],
            None,
            freed,
        ))
    }

    fn declare(&self) -> VariantDeclaration {
        VariantDeclaration {
            variant_id: WORLD_STATE_REFRESH_REF.to_string(),
            op_kinds: ["evict".to_string()].into_iter().collect(),
            deterministic: true,
            control_boundary_compact: ControlBoundaryCompact::Code,
            capabilities: CompactionCapabilities {
                reactive_overflow: true,
                ..CompactionCapabilities::default()
            },
            model_conditioned_rules: Vec::new(),
            required_inputs: required_variant_inputs(),
            fallback_variant: Some(crate::compact::EVICT_OLDEST_REF.to_string()),
        }
    }
}

/// `provider_compaction` (§5c.2 C1 family; ADR-0076 d8): the variant whose
/// `execute` delegates to the gateway capability the profile declares —
/// `ProviderCompact{capability_ref}` over the whole optional set.
#[derive(Debug, Clone)]
pub struct ProviderCompactionVariant {
    /// The profile-declared gateway capability ref.
    pub capability_ref: String,
}

impl CompactionStrategy for ProviderCompactionVariant {
    fn variant_ref(&self) -> &str {
        PROVIDER_COMPACTION_REF
    }

    fn propose(
        &self,
        input: &CompactInput,
        _assessment: &crate::compact::Assessment,
    ) -> Result<CompactionProposal, CompactError> {
        let flat = input.flattened();
        let input_ids: Vec<String> = flat
            .iter()
            .filter(|f| {
                f.candidate
                    .as_ref()
                    .is_some_and(|c| !matches!(c.retention, Retention::Required))
            })
            .map(|f| f.item.context_item_id.clone())
            .collect();
        if input_ids.is_empty() {
            return Ok(CompactionProposal::mint(
                PROVIDER_COMPACTION_REF,
                vec![],
                None,
                0,
            ));
        }
        let insert_at = flat
            .iter()
            .find(|f| f.item.context_item_id == input_ids[0])
            .map(|f| f.flat_index)
            .unwrap_or(0);
        let freed: u64 = flat
            .iter()
            .filter(|f| input_ids.contains(&f.item.context_item_id))
            .map(|f| f.item.tokens)
            .sum();
        Ok(CompactionProposal::mint_with(
            PROVIDER_COMPACTION_REF,
            vec![CompactionOp::ProviderCompact {
                input_ids: input_ids.clone(),
                capability_ref: self.capability_ref.clone(),
                insert_at,
            }],
            Some(vec![CompactionOp::Evict {
                item_ids: input_ids,
                placeholder: Placeholder::KernelOmission,
            }]),
            freed,
            Some(ModelCallMembers::default()),
        ))
    }

    fn declare(&self) -> VariantDeclaration {
        VariantDeclaration {
            variant_id: PROVIDER_COMPACTION_REF.to_string(),
            op_kinds: ["evict".to_string(), "provider_compact".to_string()]
                .into_iter()
                .collect(),
            deterministic: false,
            control_boundary_compact: ControlBoundaryCompact::Code,
            capabilities: CompactionCapabilities {
                hard_reset: true,
                handles_requests: true,
                reactive_overflow: true,
                ..CompactionCapabilities::default()
            },
            model_conditioned_rules: Vec::new(),
            required_inputs: required_variant_inputs(),
            fallback_variant: Some(crate::compact::EVICT_OLDEST_REF.to_string()),
        }
    }
}

/// `fresh_window_with_notes{reminder_rule, note_capability,
/// history_capability, fallback_buffer_tokens}` (§5c.2 C1 family;
/// ADR-0076 d7): the **model-owned** variant — `control_boundary_compact =
/// model`, `hard_reset` capable. The model's `offload_note`/`new_window`/
/// `fold` calls arrive as monitor-evaluated effects through
/// [`compact_model_owned`]; `propose` itself returns only the deterministic
/// hard-reset fallback (evict the optional set except the newest
/// `fallback_buffer_tokens`) the ladder uses when the model's proposal is
/// refused or absent.
#[derive(Debug, Clone)]
pub struct FreshWindowWithNotes {
    /// The `compaction_reminder`-style rule id the fresh window carries.
    pub reminder_rule: String,
    /// The `offload_note` capability the model calls through.
    pub note_capability: String,
    /// The history/browse capability the new window reads back through.
    pub history_capability: String,
    /// The tokens the deterministic fallback keeps at the tail.
    pub fallback_buffer_tokens: u64,
}

impl CompactionStrategy for FreshWindowWithNotes {
    fn variant_ref(&self) -> &str {
        FRESH_WINDOW_WITH_NOTES_REF
    }

    fn propose(
        &self,
        input: &CompactInput,
        _assessment: &crate::compact::Assessment,
    ) -> Result<CompactionProposal, CompactError> {
        // The deterministic fallback — a hard reset to the newest
        // `fallback_buffer_tokens` of optional tail (I-CUT/I-REQ remain the
        // kernel's: `check_proposal` refuses a required or illegal cut).
        let flat = input.flattened();
        let mut tail = 0u64;
        let mut keep_from = flat.len();
        for (i, f) in flat.iter().enumerate().rev() {
            if tail >= self.fallback_buffer_tokens {
                break;
            }
            tail += f.item.tokens;
            keep_from = i;
        }
        let ids: Vec<String> = flat[..keep_from.min(flat.len())]
            .iter()
            .filter(|f| {
                f.candidate
                    .as_ref()
                    .is_some_and(|c| !matches!(c.retention, Retention::Required))
            })
            .map(|f| f.item.context_item_id.clone())
            .collect();
        if ids.is_empty() {
            return Ok(CompactionProposal::mint(
                FRESH_WINDOW_WITH_NOTES_REF,
                vec![],
                None,
                0,
            ));
        }
        let freed: u64 = flat
            .iter()
            .filter(|f| ids.contains(&f.item.context_item_id))
            .map(|f| f.item.tokens)
            .sum();
        Ok(CompactionProposal::mint(
            FRESH_WINDOW_WITH_NOTES_REF,
            vec![CompactionOp::Evict {
                item_ids: ids,
                placeholder: Placeholder::KernelOmission,
            }],
            None,
            freed,
        ))
    }

    fn declare(&self) -> VariantDeclaration {
        VariantDeclaration {
            variant_id: FRESH_WINDOW_WITH_NOTES_REF.to_string(),
            op_kinds: [
                "evict".to_string(),
                "offload".to_string(),
                "summarize".to_string(),
            ]
            .into_iter()
            .collect(),
            deterministic: false,
            // β(compact) = model — the boundary the `offload_note` /
            // `new_window` / `fold` capabilities flow through.
            control_boundary_compact: ControlBoundaryCompact::Model,
            capabilities: CompactionCapabilities {
                hard_reset: true,
                handles_requests: true,
                reactive_overflow: true,
                ..CompactionCapabilities::default()
            },
            model_conditioned_rules: Vec::new(),
            required_inputs: required_variant_inputs(),
            fallback_variant: Some(crate::compact::EVICT_OLDEST_REF.to_string()),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The model-owned boundary (§5c.2 ADR-0076 d7; AC-R-2.4.2-10)
// ─────────────────────────────────────────────────────────────────────────────

/// The model-owned compaction calls (`β(compact) = model`): each is an
/// ordinary `Effect` the reference monitor evaluates, parsed into the op
/// grammar the kernel then re-checks. `offload_note` → `Offload` (the note
/// is the handle's annotation); `new_window` → `Evict{all optional ∉
/// keep_ids}`; `fold` → `Summarize{input_ids, insert_at}`.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelCompactCall {
    /// `offload_note{item_ids}` — offload the named items (each must carry
    /// a readable handle — `NoHandle` stands).
    OffloadNote {
        /// The context item ids.
        item_ids: Vec<String>,
    },
    /// `new_window{keep_ids}` — forget every optional item outside the
    /// keep set.
    NewWindow {
        /// The ids the model keeps.
        keep_ids: Vec<String>,
    },
    /// `fold{item_ids, insert_at}` — fold the named items into one summary.
    Fold {
        /// The context item ids.
        item_ids: Vec<String>,
        /// The flattened index the summary occupies.
        insert_at: u64,
    },
}

/// Parse a `ModelCompactCall` into the op grammar (pure — the monitor has
/// already admitted the call as an effect; this fixes its kernel shape).
/// `new_window` computes "all optional ∉ keep_ids" from the view.
pub fn model_owned_ops(input: &CompactInput, call: &ModelCompactCall) -> Vec<CompactionOp> {
    match call {
        ModelCompactCall::OffloadNote { item_ids } => vec![CompactionOp::Offload {
            item_ids: item_ids.clone(),
        }],
        ModelCompactCall::NewWindow { keep_ids } => {
            let flat = input.flattened();
            let ids: Vec<String> = flat
                .iter()
                .filter(|f| !keep_ids.contains(&f.item.context_item_id))
                .filter(|f| {
                    f.candidate
                        .as_ref()
                        .is_some_and(|c| !matches!(c.retention, Retention::Required))
                })
                .map(|f| f.item.context_item_id.clone())
                .collect();
            if ids.is_empty() {
                vec![]
            } else {
                vec![CompactionOp::Evict {
                    item_ids: ids,
                    placeholder: Placeholder::KernelOmission,
                }]
            }
        }
        ModelCompactCall::Fold {
            item_ids,
            insert_at,
        } => vec![CompactionOp::Summarize {
            input_ids: item_ids.clone(),
            insert_at: *insert_at,
        }],
    }
}

/// `model_owned_proposal(input, ops)` — mint the model's proposal and apply
/// the kernel invariants to it (AC-R-2.4.2-10: I-REQ/I-NOWIDEN/I-CUT apply
/// to a model-owned proposal exactly as to a variant's; a violation is
/// refused with `PolicyViolation` and recorded). Every `check_proposal`
/// refusal class maps to `PolicyViolation` — the proposal's *provenance* is
/// the model, so a required-item or illegal-cut touch is the policy-class
/// refusal, never a silent acceptance.
pub fn model_owned_proposal(
    input: &CompactInput,
    ops: Vec<CompactionOp>,
) -> Result<CompactionProposal, CompactError> {
    let proposal = CompactionProposal::mint(MODEL_OWNED_REF, ops, None, 0);
    crate::compact::check_proposal(input, &proposal).map_err(|e| match e {
        CompactError::PolicyViolation { .. } => e,
        other => CompactError::PolicyViolation {
            detail: format!("model-owned proposal refused: {other:?}"),
        },
    })?;
    Ok(proposal)
}

/// `compact_model_owned(input, call, sink)` — the monitor-evaluated effect
/// path for `control_boundary_compact = model`: decode → propose →
/// `context.compaction.started` → kernel check (`PolicyViolation` recorded
/// through a `completed{status: failed}` row) → `execute` → I-RECLAIM →
/// `context.compaction.completed`.
pub fn compact_model_owned(
    input: &CompactInput,
    call: &ModelCompactCall,
    sink: &mut dyn EventSink,
) -> Result<CompactOutcome, CompactError> {
    let ops = model_owned_ops(input, call);
    let assessment = assess(
        &input.trigger,
        input.plan.occupancy_estimate,
        input.window_cap,
        input.needed,
        input.target_fraction_ppm,
    );
    sink.emit(
        "context.compaction.started",
        crate::events::compaction_started(
            &input.trigger,
            input.plan.occupancy_estimate,
            &assessment,
        ),
    );
    let proposal = match model_owned_proposal(input, ops) {
        Ok(p) => p,
        Err(e) => {
            // Record the refusal — a `completed{status: failed}` row naming
            // the variant and the violation; the view is unchanged.
            let unchanged = crate::compact::unchanged_view(input);
            let record = crate::compact::mint_record(
                input,
                &assessment,
                CompactionStatus::Failed,
                MODEL_OWNED_REF,
                &[],
                &unchanged,
                0,
                None,
            );
            crate::compact::emit_completed(input, sink, &record);
            return Err(e);
        }
    };
    let view = execute(input, &proposal)?;
    let ok = match assessment.requirement {
        crate::compact::Requirement::Hard => view.tokens_freed >= assessment.min_reclaim,
        _ => view.tokens_freed > 0 || proposal.ops.is_empty(),
    };
    let status = if ok {
        CompactionStatus::Applied
    } else {
        CompactionStatus::Ineffective
    };
    let record = crate::compact::mint_record(
        input,
        &assessment,
        status,
        MODEL_OWNED_REF,
        &proposal.ops,
        &view,
        0,
        None,
    );
    crate::compact::emit_completed(input, sink, &record);
    Ok(CompactOutcome {
        applied: status == CompactionStatus::Applied,
        record,
        view,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Conditioned rules — `HarnessRule{action: set_compaction_policy}` (home ii)
// ─────────────────────────────────────────────────────────────────────────────

/// `rule_condition_of(conditioned_on_json)` — decode a rule's
/// `conditioned_on` into the shared [`RuleCondition`] sum: `{"profile":
/// ref}` → `Profile`, `{"compliance": ref}` → `ComplianceMeasurement`,
/// `{"model" | "model_identity": id}` → `ModelIdentity` (registration
/// refuses it — T-LCD-01).
pub fn rule_condition_of(conditioned_on: &Json) -> Option<RuleCondition> {
    if let Some(p) = conditioned_on.get("profile").and_then(Json::as_str) {
        return Some(RuleCondition::Profile(p.to_string()));
    }
    if let Some(m) = conditioned_on
        .get("model_identity")
        .or_else(|| conditioned_on.get("model"))
        .and_then(Json::as_str)
    {
        return Some(RuleCondition::ModelIdentity(m.to_string()));
    }
    if let Some(c) = conditioned_on.get("compliance").and_then(Json::as_str) {
        return Some(RuleCondition::ComplianceMeasurement(c.to_string()));
    }
    conditioned_on
        .as_str()
        .map(|s| RuleCondition::Profile(s.to_string()))
}

/// A decoded `set_compaction_policy` rule (§5c.2 home (ii)): the action's
/// params verbatim plus its conditioning and debt record. `params` may name
/// `{variant_ref, param_overrides{…}, soft_threshold{occupancy_ppm},
/// schedule{every_turns}}` — each optional, all recorded.
#[derive(Debug, Clone)]
pub struct CompactionPolicyRule {
    /// The rule's id (its `occupancy_soft{rule_id}` / `schedule{rule_id}`
    /// trigger key).
    pub rule_id: String,
    /// The conditioning (`None` = unconditional).
    pub conditioned_on: Option<RuleCondition>,
    /// The `set_compaction_policy` params verbatim.
    pub params: Json,
    /// The `AssumptionDebtRecord` — mandatory when `conditioned_on` is set
    /// (the decode refuses otherwise — T-LCD-05).
    pub debt: Option<AssumptionDebtRecord>,
}

/// `decode_compaction_rule(rule_id, params, conditioned_on, debt)` — the
/// registration-time check on one `set_compaction_policy` rule: the shared
/// conditioned-rule check (`Profile` conditioning needs the debt record;
/// `ModelIdentity` refuses).
pub fn decode_compaction_rule(
    rule_id: &str,
    params: &Json,
    conditioned_on: Option<RuleCondition>,
    debt: Option<AssumptionDebtRecord>,
) -> Result<CompactionPolicyRule, RegistrationError> {
    if let Some(cond) = &conditioned_on {
        check_conditioned_rules(
            &[ConditionedRule {
                rule_id: rule_id.to_string(),
                conditioned_on: cond.clone(),
                debt: debt.clone(),
            }],
            &required_variant_inputs(),
        )?;
    }
    Ok(CompactionPolicyRule {
        rule_id: rule_id.to_string(),
        conditioned_on,
        params: params.clone(),
        debt,
    })
}

/// The resolved effective compaction policy for a bound profile — what
/// `select_variant`/`propose` read (the merged conditioned ruleset; rule
/// order is canonical — `rule_id` sort, deterministic).
#[derive(Debug, Clone, Default)]
pub struct ResolvedCompactionPolicy {
    /// The effective variant ref (the last firing rule's `variant_ref`,
    /// canonical order).
    pub variant_ref: Option<String>,
    /// The merged `param_overrides` (firing rules, canonical order —
    /// later rules win per key).
    pub param_overrides: BTreeMap<String, Json>,
    /// `occupancy_soft{rule_id}` thresholds to arm —
    /// `(rule_id, occupancy_ppm)`.
    pub soft_thresholds: Vec<(String, u64)>,
    /// `schedule{rule_id}` entries — `(rule_id, every_turns)`.
    pub schedules: Vec<(String, u64)>,
}

/// `resolve_compaction_policy(rules, bound_profile)` — the firing set is
/// rules whose `conditioned_on` is `Profile(bound_profile)` or
/// unconditional (`None`); a `ComplianceMeasurement` condition never fires
/// at link/plan time (the measurement arrives at run time — the rule is
/// inventoried, not applied). Deterministic: `rule_id` order.
pub fn resolve_compaction_policy(
    rules: &[CompactionPolicyRule],
    bound_profile: &str,
) -> ResolvedCompactionPolicy {
    let mut out = ResolvedCompactionPolicy::default();
    let mut firing: Vec<&CompactionPolicyRule> = rules
        .iter()
        .filter(|r| match &r.conditioned_on {
            None => true,
            Some(RuleCondition::Profile(p)) => p == bound_profile,
            Some(RuleCondition::ComplianceMeasurement(_)) => false,
            // `ModelIdentity` never reaches a resolved policy — decode refuses
            // it; an anyway-injected rule is inert, never applied.
            Some(RuleCondition::ModelIdentity(_)) => false,
        })
        .collect();
    firing.sort_by(|a, b| a.rule_id.cmp(&b.rule_id));
    for r in firing {
        if let Some(v) = r.params.get("variant_ref").and_then(Json::as_str) {
            out.variant_ref = Some(v.to_string());
        }
        if let Some(Json::Obj(m)) = r.params.get("param_overrides") {
            for (k, v) in m {
                out.param_overrides.insert(k.clone(), v.clone());
            }
        }
        if let Some(ppm) = r
            .params
            .get("soft_threshold")
            .and_then(|t| t.get("occupancy_ppm"))
            .and_then(Json::as_int)
        {
            if ppm > 0 {
                out.soft_thresholds.push((r.rule_id.clone(), ppm as u64));
            }
        }
        if let Some(n) = r
            .params
            .get("schedule")
            .and_then(|t| t.get("every_turns"))
            .and_then(Json::as_int)
        {
            if n > 0 {
                out.schedules.push((r.rule_id.clone(), n as u64));
            }
        }
    }
    out
}

/// CF-169/ADR-0106 re-arm: a soft threshold re-arms only on
/// `context.compaction.completed{status: applied \| fallback_applied}` —
/// `fallback_applied` is the record's `fallback_variant.is_some()` leg.
pub fn rearmable(record: &CompactionRecord) -> bool {
    record.status == CompactionStatus::Applied || record.fallback_variant.is_some()
}

/// The `occupancy_soft`/`schedule` trigger mints — the resolved rules'
/// `rule_id`s are the trigger coordinates (§5c.2 triggers row).
pub fn soft_triggers(
    resolved: &ResolvedCompactionPolicy,
) -> Vec<crate::compact::CompactionTrigger> {
    let mut out: Vec<crate::compact::CompactionTrigger> = resolved
        .soft_thresholds
        .iter()
        .map(
            |(rule_id, _)| crate::compact::CompactionTrigger::OccupancySoft {
                rule_id: rule_id.clone(),
            },
        )
        .collect();
    out.extend(resolved.schedules.iter().map(|(rule_id, _)| {
        crate::compact::CompactionTrigger::Schedule {
            rule_id: rule_id.clone(),
        }
    }));
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// `compaction_reminder` — profile-level surface rule (home iii; CF-271)
// ─────────────────────────────────────────────────────────────────────────────

/// `compaction_reminder{text_ref, placement}` — the decoded
/// `ProfileRule{kind: compaction_reminder}` params (§5c.2 home (iii)): the
/// reminder text the next assembly delivers at `placement` after an
/// `applied`/`fallback_applied` compaction (the "prior work" preamble is a
/// profile-owned `Text` leaf — ADR-0076 d3, never an authority).
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionReminder {
    /// The reminder `Text` leaf's ref (opacity-counted).
    pub text_ref: String,
    /// The slot the reminder lands in (`reminder_placement` — a
    /// `prompt_layout` member).
    pub placement: String,
}

/// Decode a `compaction_reminder` rule's params (`{text_ref, placement}` —
/// `text`/`text_ref` and `placement`/`slot` spellings both decode; a
/// missing text ref is a typed refusal, never a silent empty string).
pub fn decode_compaction_reminder(params: &Json) -> Result<CompactionReminder, CompactError> {
    let text_ref = params
        .get("text_ref")
        .or_else(|| params.get("text"))
        .and_then(Json::as_str)
        .ok_or_else(|| CompactError::PolicyViolation {
            detail: "compaction_reminder lacks text_ref".to_string(),
        })?;
    let placement = params
        .get("placement")
        .or_else(|| params.get("slot"))
        .and_then(Json::as_str)
        .unwrap_or("kernel")
        .to_string();
    Ok(CompactionReminder {
        text_ref: text_ref.to_string(),
        placement,
    })
}

/// `reminder_candidate(reminder, record, run_id, at_seq, estimator_ref)` —
/// mint the `kernel_notice` candidate the next `assemble` admits through
/// its advisories path (the kernel stamps `required` on advisories —
/// I-NOWIDEN). Returns `None` when the record is not `rearmable` (a failed
/// compaction carries no reminder — the model was never told to expect
/// one).
pub fn reminder_candidate(
    reminder: &CompactionReminder,
    record: &CompactionRecord,
    run_id: &str,
    at_seq: u64,
    estimator_ref: &str,
) -> Option<Candidate> {
    if !rearmable(record) {
        return None;
    }
    let cid = hh_identity::idp::idp_id(
        crate::compact::COMPACTION_IDP,
        format!("reminder:{run_id}:{}", record.compaction_id).as_bytes(),
    );
    Some(Candidate {
        candidate_id: cid.clone(),
        context_item_id: Some(cid),
        kind: CandidateKind::KernelNotice,
        state: CandidateState::Expanded,
        retention: Retention::Required,
        estimate: crate::plan::Estimate {
            tokens: 64,
            estimator_ref: estimator_ref.to_string(),
        },
        source_event: None,
        source_seq: at_seq,
        label: hh_provenance::label::Label::at(hh_provenance::AuthorityClass::Kernel),
        provenance: hh_provenance::record::ProvenanceRecord::kernel("kernel:compact", at_seq),
        validity: hh_hir::records::Validity::open_from(0),
        readers: None,
        slot_hint: Some(reminder.placement.clone()),
        volatile: false,
        paired_with: None,
        batch_id: None,
        artefact_id: Some(reminder.text_ref.clone()),
        handle: None,
    })
}
