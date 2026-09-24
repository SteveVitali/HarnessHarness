//! The deterministic `followed` detector table (spec §02 "Detectors per
//! kind" + `verification.artefact.followed`; AC-R-2.7.1-9 / AC-G1-9; S3.10).
//!
//! Following is deterministic iff the artefact references a `schema` or
//! `trace_predicate` validator evaluable from `events`/`end_state` alone.
//! Prose artefacts — an `instruction` or `memory` with no `validates` edge,
//! a `procedure` whose body is not typed — stay judged-only at this stage:
//! their followed evaluation renders `n/a{no_detector}`, never a proxy
//! (ADR-0014; CF-483). The per-profile share of deliveries whose followed
//! verdict is `detector = deterministic` is a reported metric
//! ([`crate::evalfold`]'s `deterministic_detector_share`).

use hh_wire::Json;

/// The deterministic detector spellings a `followed` verdict's
/// `detector_ref` carries.
pub mod detector {
    /// `tool_surface` — the call's args conformed to the declared schema
    /// and the effect class is as declared (the G-INTERPRET validation
    /// row is the evidence).
    pub const ARGS_CONFORM: &str = "tool_surface:args_conform";
    /// Typed `procedure` — the invoked capability is in the activated
    /// procedure's `allowed_capabilities` and the index `delivery_id` is
    /// in the call's causes (hh-context's `detect_followed`).
    pub const PROCEDURE_INVOKED: &str = "procedure:procedure_invoked";
    /// Kernel-executed / soft-budget `rule` — the postcondition held in
    /// the ledger (the nudge's admitted decision is the evidence).
    pub const RULE_POSTCONDITION: &str = "rule:postcondition";
    /// Closed-schema / `recovery` `memory` — citation-in-action /
    /// first-try canonical-args match.
    pub const CITATION_IN_ACTION: &str = "memory:citation_in_action";
    /// `instruction` — only via a `validates` edge to a deterministic
    /// validator.
    pub const VALIDATES_EDGE: &str = "instruction:validates_edge";
}

/// The `n/a` spelling a followed evaluation renders when the artefact's
/// kind carries no deterministic detector (ADR-0014 — never a proxy).
pub const NO_DETECTOR: &str = "n/a{no_detector}";

/// What the detector table resolves for one delivered artefact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowedResolution {
    /// A deterministic detector exists — the string is the `detector_ref`
    /// spelling the `verification.artefact.followed` row carries.
    Deterministic(&'static str),
    /// No deterministic detector — the verdict renders
    /// [`NO_DETECTOR`] (judged-only at this stage).
    NoDetector,
}

/// `deterministic_detector(kind, references_validator)` — the detector
/// table. `references_validator` is the kind's structural precondition:
/// a `procedure`'s typed body, a `memory`'s closed schema / `recovery`
/// class, an `instruction`'s `validates` edge. `tool_surface` and the
/// kernel-executed rule kinds are unconditional — the surface schema /
/// the ledger postcondition *is* the referenced validator.
pub fn deterministic_detector(
    kind: &str,
    references_validator: bool,
) -> FollowedResolution {
    match kind {
        "tool_surface" => FollowedResolution::Deterministic(detector::ARGS_CONFORM),
        "procedure" | "procedure_index" if references_validator => {
            FollowedResolution::Deterministic(detector::PROCEDURE_INVOKED)
        }
        "rule" | "kernel_rule" | "soft_budget_rule" | "loop_nudge"
        | "continue_nudge" => {
            FollowedResolution::Deterministic(detector::RULE_POSTCONDITION)
        }
        "memory" | "memory_index" | "recovery" if references_validator => {
            FollowedResolution::Deterministic(detector::CITATION_IN_ACTION)
        }
        "instruction" if references_validator => {
            FollowedResolution::Deterministic(detector::VALIDATES_EDGE)
        }
        _ => FollowedResolution::NoDetector,
    }
}

/// The `verification.artefact.followed` payload (§02's member set:
/// `{artefact_id, delivery_id, detector, detector_ref, verdict,
///   confidence, evidence_ref}` + `kind`).
pub fn followed_payload(
    artefact_id: &str,
    delivery_id: &str,
    detector_ref: &str,
    verdict: bool,
    evidence_ref: &str,
    kind: &str,
) -> Json {
    Json::obj([
        ("artefact_id", Json::str(artefact_id)),
        ("delivery_id", Json::str(delivery_id)),
        ("detector", Json::str("deterministic")),
        ("detector_ref", Json::str(detector_ref)),
        ("verdict", Json::Bool(verdict)),
        ("confidence_ppm", Json::Int(1_000_000)),
        ("evidence_ref", Json::str(evidence_ref)),
        ("kind", Json::str(kind)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_table_covers_the_ac_kinds() {
        // AC-R-2.7.1-9 — deterministic for tool_surface, typed procedure,
        // kernel + soft-budget rules.
        assert_eq!(
            deterministic_detector("tool_surface", false),
            FollowedResolution::Deterministic(detector::ARGS_CONFORM)
        );
        assert_eq!(
            deterministic_detector("procedure_index", true),
            FollowedResolution::Deterministic(detector::PROCEDURE_INVOKED)
        );
        assert_eq!(
            deterministic_detector("soft_budget_rule", false),
            FollowedResolution::Deterministic(detector::RULE_POSTCONDITION)
        );
    }

    #[test]
    fn prose_renders_no_detector() {
        // AC-R-2.7.1-9 — prose instructions/memories absent a `validates`
        // edge are judged-only.
        assert_eq!(
            deterministic_detector("instruction", false),
            FollowedResolution::NoDetector
        );
        assert_eq!(
            deterministic_detector("memory", false),
            FollowedResolution::NoDetector
        );
        // …and a `validates`/closed-schema edge restores determinism.
        assert_eq!(
            deterministic_detector("instruction", true),
            FollowedResolution::Deterministic(detector::VALIDATES_EDGE)
        );
        assert_eq!(
            deterministic_detector("memory", true),
            FollowedResolution::Deterministic(detector::CITATION_IN_ACTION)
        );
        // An untyped procedure is prose-class.
        assert_eq!(
            deterministic_detector("procedure", false),
            FollowedResolution::NoDetector
        );
    }
}
