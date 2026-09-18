//! The compatibility surface Ψ_θ (spec §2.5.7; ADR-0012 decisions 7–8, ADR-0160).
//!
//! Ψ_θ : F → Dist(Δ) is a **derived view, never stored as truth** (§2.8); its only persisted
//! form is the [`FittedSurfaceReport`], which carries an expiry and a debt record. The
//! Stage-1 static half landed here: the report *shape* and the **no-interpolation** rule —
//! categorical axes are never interpolated; an unprobed level is [`FitError::UnknownLevel`]; a
//! cell below the minimum design is `unknown{insufficient, n}`, never dropped or estimated
//! (§2.5.7 "No interpolation"; T-LCD-07). Fitting from real rows is Stage 3.

/// The persisted, expiring form of Ψ_θ (§2.5.7 "Derived view") — carries an
/// expiry and a debt record (debt home 11; the `debt_record_ref` binding
/// landed at S1.24). Shape only at Stage 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FittedSurfaceReport {
    /// The metric fitted.
    pub metric: String,
    /// The factor axes.
    pub factors: Vec<String>,
    /// The probed levels per factor.
    pub levels: Vec<Vec<String>>,
    /// The design reference.
    pub design_ref: String,
    /// The model form used.
    pub model_form: ModelForm,
    /// The configuration ids the surface was fitted over (drives expiry).
    pub configuration_ids: Vec<String>,
    /// Cells that were below the minimum design (`unknown{insufficient, n}`), never estimated.
    pub unknown_cells: Vec<UnknownCell>,
    /// The report's expiry (§2.5.7 "Expiry" — the fitted surface expires with
    /// the configurations it was fitted over).
    pub expiry: Option<crate::debt::ExpiryCondition>,
    /// `debt_record_ref` — the `AssumptionDebtRecord` the fit carries (debt
    /// home 11; §2.5.7 "its only persisted form carries an expiry and a debt
    /// record").
    pub debt_record_ref: Option<String>,
    /// The report status.
    pub status: SurfaceStatus,
}

/// The admissible model forms (§2.5.7). `contrast` is the C1 default; `factorial_glmm` is C2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelForm {
    /// C1 default.
    Contrast,
    /// C2.
    FactorialGlmm,
    /// A curve along an ordered (budget-like) axis.
    CurveOnOrderedAxis,
}

/// A cell with insufficient data — reported, never dropped or estimated (§2.5.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownCell {
    /// The factor→level coordinate of the cell.
    pub coordinate: Vec<(String, String)>,
    /// The number of samples that were available (below the minimum design).
    pub n: usize,
}

/// The report status (§2.5.7 "Derived view"/"Expiry"). Never hidden; an expired surface is
/// annotated, not deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceStatus {
    /// Active.
    Active,
    /// Expiring.
    Expiring,
    /// Expired (annotated, never hidden — T-LCD-05).
    Expired,
}

/// Errors from surface fitting/lookup (§2.5.7; §2.9.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FitError {
    /// A categorical level that was not probed — never interpolated (§2.5.7; ADR-0160 D3).
    UnknownLevel {
        /// The factor whose level is unknown.
        factor: String,
        /// The requested, unprobed level.
        level: String,
    },
}

impl FittedSurfaceReport {
    /// The minimum design (§2.5.7): ≥ 2 levels, ≥ 3 replicates, ≥ 30 tasks.
    pub const MIN_LEVELS: usize = 2;
    pub const MIN_REPLICATES: usize = 3;
    pub const MIN_TASKS: usize = 30;

    /// Look up the effect at a categorical `(factor, level)` coordinate. A level that was not in
    /// the probed set is [`FitError::UnknownLevel`] — the no-interpolation rule. (Stage-1: this
    /// enforces the *rule*; the fitted estimate itself lands at Stage 3.)
    pub fn require_probed_level(&self, factor: &str, level: &str) -> Result<(), FitError> {
        let idx = self
            .factors
            .iter()
            .position(|f| f == factor)
            .ok_or_else(|| FitError::UnknownLevel {
                factor: factor.to_string(),
                level: level.to_string(),
            })?;
        if self.levels[idx].iter().any(|l| l == level) {
            Ok(())
        } else {
            Err(FitError::UnknownLevel {
                factor: factor.to_string(),
                level: level.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> FittedSurfaceReport {
        FittedSurfaceReport {
            metric: "success_rate".into(),
            factors: vec!["model".into(), "variant".into()],
            levels: vec![
                vec!["M1".into(), "M2".into()],
                vec!["withV".into(), "noV".into()],
            ],
            design_ref: "design/1".into(),
            model_form: ModelForm::Contrast,
            configuration_ids: vec!["cfg:a".into(), "cfg:b".into()],
            unknown_cells: vec![],
            expiry: None,
            debt_record_ref: Some("debt:surface-fit-1".into()),
            status: SurfaceStatus::Active,
        }
    }

    #[test]
    fn probed_level_is_ok() {
        assert!(report().require_probed_level("model", "M1").is_ok());
    }

    #[test]
    fn unprobed_categorical_level_is_unknown_never_interpolated() {
        // §2.5.7 / ADR-0160 D3: no interpolation across categorical axes.
        assert_eq!(
            report().require_probed_level("model", "M3"),
            Err(FitError::UnknownLevel {
                factor: "model".into(),
                level: "M3".into()
            })
        );
    }

    #[test]
    fn unknown_factor_is_unknown_level() {
        assert!(matches!(
            report().require_probed_level("hosting", "x"),
            Err(FitError::UnknownLevel { .. })
        ));
    }

    #[test]
    fn insufficient_cells_are_kept_not_dropped() {
        // §2.5.7: cells below the minimum design are reported, never dropped/estimated.
        let mut r = report();
        r.unknown_cells.push(UnknownCell {
            coordinate: vec![("model".into(), "M1".into())],
            n: 2,
        });
        assert_eq!(r.unknown_cells.len(), 1);
        assert!(r.unknown_cells[0].n < FittedSurfaceReport::MIN_REPLICATES);
    }
}
