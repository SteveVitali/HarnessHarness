//! Reproducibility levels **R0–R3** and the [`InstrumentRecord`] (§8.3 #3; ADR-0038 D1).
//!
//! A bundle declares a `claimed_level`; a claim exceeding its pins is `ReproClaimUnsupported`.
//! This Stage-1 slice lands the levels, the [`InstrumentRecord`] and the cap rules
//! (`pinned = false ⇒ claimed ∈ {R0, R1, R3}`; `dirty ⇒ R0`); the executable `reproduce` at
//! R0/R1/R3 with a `ReproReport` is the Stage-3 slice (§8.3 #9).

/// The four reproducibility levels (§8.3 #3), ordered R0 < R1 < R2 < R3 by *strength of claim*
/// (note R1 is ordered above R2 for *derivation* claims — §8.3 #5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReproLevel {
    /// Recorded: inputs content-identified or explicitly `unpinned[]`; raw traces; completeness ok.
    R0,
    /// Bit-exact derivation: every derived object re-derives to equal hashes.
    R1,
    /// Semantic re-execution: equal validator verdicts and end-state class (requires pinned model).
    R2,
    /// Statistical: distributions over n ≥ k seeds within a pre-registered margin under matched budget.
    R3,
}

/// The `InstrumentRecord{version, source_commit, dirty, component_versions, idp}` — a bundle
/// member (§8.3 #3). `dirty = true` caps every claim at R0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstrumentRecord {
    pub version: String,
    pub source_commit: String,
    pub dirty: bool,
    pub component_versions: Vec<(String, String)>,
    pub idp: &'static str,
}

impl InstrumentRecord {
    pub fn new(
        version: impl Into<String>,
        source_commit: impl Into<String>,
        dirty: bool,
    ) -> InstrumentRecord {
        InstrumentRecord {
            version: version.into(),
            source_commit: source_commit.into(),
            dirty,
            component_versions: Vec::new(),
            idp: crate::idp::IDP_1.idp_id,
        }
    }
}

/// A claim exceeding the pins (`ReproClaimUnsupported`; §8.3 #5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReproClaimUnsupported {
    pub claimed: ReproLevel,
    pub max_supported: ReproLevel,
    pub reason: &'static str,
}

/// The maximum level supportable given the pins (§8.3 #3/#5). `dirty ⇒ R0`; an unpinned model
/// can never reach R2 (semantic re-execution) but may claim R0/R1/R3.
pub fn max_supported_level(model_pinned: bool, instrument_dirty: bool) -> ReproLevel {
    if instrument_dirty {
        return ReproLevel::R0;
    }
    if model_pinned {
        ReproLevel::R3
    } else {
        // Unpinned model: R2 is unreachable, but R3 (distributional) is (model named with
        // observed_fingerprint). We express the cap as "not R2"; the check below enforces it.
        ReproLevel::R3
    }
}

/// Check a declared `claimed_level` against the pins. Enforces `dirty ⇒ R0` and
/// `pinned = false ⇒ claimed ∈ {R0, R1, R3}` (an unpinned model claiming R2 is unsupported).
pub fn check_claim(
    claimed: ReproLevel,
    model_pinned: bool,
    instrument_dirty: bool,
) -> Result<(), ReproClaimUnsupported> {
    if instrument_dirty && claimed != ReproLevel::R0 {
        return Err(ReproClaimUnsupported {
            claimed,
            max_supported: ReproLevel::R0,
            reason: "dirty instrument caps the claim at R0",
        });
    }
    if !model_pinned && claimed == ReproLevel::R2 {
        return Err(ReproClaimUnsupported {
            claimed,
            max_supported: ReproLevel::R1,
            reason: "R2 semantic re-execution requires ModelSnapshotRecord.pinned = true",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_instrument_caps_at_r0() {
        // §8.3 #7: dirty = true caps at R0.
        assert!(check_claim(ReproLevel::R1, true, true).is_err());
        assert!(check_claim(ReproLevel::R0, true, true).is_ok());
        assert_eq!(max_supported_level(true, true), ReproLevel::R0);
    }

    #[test]
    fn unpinned_model_cannot_claim_r2() {
        let err = check_claim(ReproLevel::R2, false, false).unwrap_err();
        assert_eq!(err.max_supported, ReproLevel::R1);
        // …but R0/R1/R3 remain claimable with an unpinned model.
        assert!(check_claim(ReproLevel::R0, false, false).is_ok());
        assert!(check_claim(ReproLevel::R1, false, false).is_ok());
        assert!(check_claim(ReproLevel::R3, false, false).is_ok());
    }

    #[test]
    fn pinned_clean_supports_up_to_r3() {
        assert!(check_claim(ReproLevel::R2, true, false).is_ok());
        assert!(check_claim(ReproLevel::R3, true, false).is_ok());
        assert_eq!(max_supported_level(true, false), ReproLevel::R3);
    }

    #[test]
    fn instrument_record_pins_idp() {
        let ir = InstrumentRecord::new("0.0.1", "abc123", false);
        assert_eq!(ir.idp, "idp/1");
        assert!(!ir.dirty);
    }
}
