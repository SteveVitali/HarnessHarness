//! The Hosting ABI's shared constants (spec §6.6; S4.5a; ADR-0166 D4–D5).
//!
//! The probe catalogue lives in `hh-hosting`; the quarantine act lives in
//! `hh-registry`; neither may depend on the other (`hosting_edges = []`
//! keeps the hosting tier removable). The **P0 quarantine set** is §6.6
//! data both sides consume — it homes here, next to the `lifecycle.hosted.*`
//! class registrations and the `hosted_lowering` column (the ledger is the
//! one crate every tier already shares; CC7 — one schema source).

/// The P0 probe dimensions — a DRIFT on one of these sets the participant
/// record's admission to `quarantined` (§6.6 §2.5 "DRIFT on a P0 dimension
/// sets the `participant` record's admission to `quarantined` for
/// experiments until a new declaration is registered"; probes
/// P-01/P-03/P-04/P-06/P-09/P-16). Non-P0 drift is a stratum on every
/// results row — annotated, never silently quarantined.
pub const P0_DIMENSIONS: &[&str] = &[
    "basic_turn",
    "interrupt",
    "tool_calling_supplied",
    "policy_deny",
    "coordinate_model",
    "end_state",
];

/// `true` for a P0 dimension spelling.
pub fn is_p0(dimension: &str) -> bool {
    P0_DIMENSIONS.contains(&dimension)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_p0_set_is_closed_and_sorted() {
        assert_eq!(P0_DIMENSIONS.len(), 6);
        for d in P0_DIMENSIONS {
            assert!(is_p0(d));
        }
        assert!(!is_p0("streaming"));
        assert!(!is_p0("usage_reporting"));
    }
}
