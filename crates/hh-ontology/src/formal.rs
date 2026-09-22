//! The formal model (spec §2.5; ADR-0012 decision 5). The formalism's only duty is to fix
//! *what is held constant and what is varied*; the runtime never computes it (§2.5, §2.5.3).
//!
//! AC-A2-2: "the formal model of §2.5 is present; every symbol used elsewhere (κ, B, β, Δ, Ψ,
//! θ, M, J) resolves to §2." [`resolve`] is total over [`Symbol`] and every variant carries the
//! §2 definition and the sub-section it resolves to. The symbol-resolution ruling of CF-474 is
//! honored: **Δ is the harness effect** (§2.5.4) and the closed decision-point set over which β
//! ranges is written **𝒟** (§2.5.5), so Δ keeps a single meaning.

/// Every symbol the rest of the spec resolves against §2.5 (§2.0 "Symbol resolution" rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Symbol {
    /// θ — harness parameterization.
    Theta,
    /// M — model snapshot.
    M,
    /// E — environment.
    E,
    /// D — task distribution.
    D,
    /// B — budget vector.
    B,
    /// κ — configuration (one factorial point).
    Kappa,
    /// β — the control boundary.
    Beta,
    /// Δ — the harness effect (paired, matched-B).
    Delta,
    /// Ψ — the compatibility surface Ψ_θ.
    Psi,
    /// J — the harness objective.
    J,
    /// π_M — the model-induced action distribution.
    PiM,
    /// π_system — the closed-loop policy stack.
    PiSystem,
    /// 𝒟 — the closed decision-point set over which β ranges (CF-474; distinct from Δ).
    DecisionPointSet,
}

/// A resolved symbol definition: the object, its §2 definition, and the resolving sub-section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDef {
    /// The symbol.
    pub symbol: Symbol,
    /// The rendered glyph (`θ`, `M`, …).
    pub glyph: &'static str,
    /// The object name.
    pub object: &'static str,
    /// The §2 definition (condensed from §2.5).
    pub definition: &'static str,
    /// The sub-section the symbol resolves to.
    pub section: &'static str,
}

impl Symbol {
    /// Every symbol, including the AC-A2-2 required set {κ, B, β, Δ, Ψ, θ, M, J} plus E, D,
    /// π_M, π_system and the 𝒟 clarification.
    pub const ALL: [Symbol; 13] = [
        Symbol::Theta,
        Symbol::M,
        Symbol::E,
        Symbol::D,
        Symbol::B,
        Symbol::Kappa,
        Symbol::Beta,
        Symbol::Delta,
        Symbol::Psi,
        Symbol::J,
        Symbol::PiM,
        Symbol::PiSystem,
        Symbol::DecisionPointSet,
    ];

    /// The exact set AC-A2-2 names as "every symbol used elsewhere".
    pub const AC_A2_2_REQUIRED: [Symbol; 8] = [
        Symbol::Kappa,
        Symbol::B,
        Symbol::Beta,
        Symbol::Delta,
        Symbol::Psi,
        Symbol::Theta,
        Symbol::M,
        Symbol::J,
    ];
}

/// Resolve a symbol to its §2 definition. **Total** over [`Symbol`] (AC-A2-2).
pub fn resolve(symbol: Symbol) -> SymbolDef {
    match symbol {
        Symbol::Theta => SymbolDef {
            symbol,
            glyph: "θ",
            object: "harness parameterization",
            definition: "θ = (h, p, β, π_pol): a Harness Definition h in HIR, a Model Profile p, \
                         a control boundary β, and the policies h references.",
            section: "§2.5.1",
        },
        Symbol::M => SymbolDef {
            symbol,
            glyph: "M",
            object: "model snapshot",
            definition: "Provider, version identity and sampling parameters; induces π_M(a|c) \
                         over rendered contexts c. Served-model drift is a ledger fact, never an \
                         assumption.",
            section: "§2.5.1",
        },
        Symbol::E => SymbolDef {
            symbol,
            glyph: "E",
            object: "environment",
            definition:
                "The external world with state s and an effect semantics, reached only \
                         through the environment boundary; identified by an EnvironmentRecord in κ.",
            section: "§2.5.1",
        },
        Symbol::D => SymbolDef {
            symbol,
            glyph: "D",
            object: "task distribution",
            definition: "The distribution of goals; every result row carries a task with suite id \
                         and split label.",
            section: "§2.5.1",
        },
        Symbol::B => SymbolDef {
            symbol,
            glyph: "B",
            object: "budget vector",
            definition: "A vector over the kernel resource dimensions (counters and gauges); \
                         every Goal and AgentProcess references one.",
            section: "§2.5.1",
        },
        Symbol::Kappa => SymbolDef {
            symbol,
            glyph: "κ",
            object: "configuration",
            definition: "κ = (M-set, h, p, E, B, seed): one factorial point; results keyed by \
                         configuration id.",
            section: "§2.5.6",
        },
        Symbol::Beta => SymbolDef {
            symbol,
            glyph: "β",
            object: "control boundary",
            definition: "β : 𝒟 → {code, model, human} with per-decision-point guards; a typed \
                         record on AgentProcess.native (ADR-0012 writes the domain Δ; 𝒟 here per \
                         CF-474).",
            section: "§2.5.5",
        },
        Symbol::Delta => SymbolDef {
            symbol,
            glyph: "Δ",
            object: "harness effect",
            definition: "Δ(θ, θ₀ | M, E, D, B) = J(θ) − J(θ₀): always paired, always at matched \
                         B, never an absolute score (T-LCD-14).",
            section: "§2.5.4",
        },
        Symbol::Psi => SymbolDef {
            symbol,
            glyph: "Ψ",
            object: "compatibility surface",
            definition: "Ψ_θ : F → Dist(Δ) maps the factor space F to the distribution of the \
                         paired harness effect; a derived view, never stored as truth.",
            section: "§2.5.7",
        },
        Symbol::J => SymbolDef {
            symbol,
            glyph: "J",
            object: "harness objective",
            definition: "J(θ; M, E, D, B) = E_{τ∼π_system}[U(success, quality) − λ_c C(τ) − λ_l \
                         L(τ) − λ_r R(τ)] subject to c(τ) ≤ B and the safety/permission \
                         constraints.",
            section: "§2.5.4",
        },
        Symbol::PiM => SymbolDef {
            symbol,
            glyph: "π_M",
            object: "model policy",
            definition: "The action distribution π_M(a|c) induced by a model snapshot M over \
                         rendered contexts; nonstationary, hence snapshot with drift detection.",
            section: "§2.5.1/§2.5.3",
        },
        Symbol::PiSystem => SymbolDef {
            symbol,
            glyph: "π_system",
            object: "policy stack",
            definition:
                "π_system = H_{θ,E,B}[π_M]: the closed-loop policy over environment states \
                         induced by iterating the harness step; a description, never computed.",
            section: "§2.5.3",
        },
        Symbol::DecisionPointSet => SymbolDef {
            symbol,
            glyph: "𝒟",
            object: "decision-point set",
            definition: "The closed set of HIR/1 DecisionPoints over which β ranges (CF-474), so \
                         Δ keeps its single meaning as the harness effect.",
            section: "§2.5.5",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_symbol_resolves_to_section_2() {
        // AC-A2-2: resolve is total and every symbol resolves to a §2 sub-section with a
        // non-empty definition and the right symbol echoed back.
        for s in Symbol::ALL {
            let def = resolve(s);
            assert_eq!(def.symbol, s);
            assert!(def.section.starts_with("§2"), "{:?} not homed in §2", s);
            assert!(!def.definition.is_empty());
            assert!(!def.glyph.is_empty());
        }
    }

    #[test]
    fn ac_a2_2_required_symbols_all_resolve() {
        // The exact set the AC names: {κ, B, β, Δ, Ψ, θ, M, J}.
        assert_eq!(Symbol::AC_A2_2_REQUIRED.len(), 8);
        for s in Symbol::AC_A2_2_REQUIRED {
            let def = resolve(s);
            assert!(!def.definition.is_empty());
        }
    }

    #[test]
    fn delta_is_the_harness_effect_and_decision_set_is_distinct() {
        // CF-474: Δ is the harness effect (§2.5.4); the decision-point set is 𝒟 (§2.5.5).
        assert_eq!(resolve(Symbol::Delta).object, "harness effect");
        assert_eq!(resolve(Symbol::Delta).section, "§2.5.4");
        assert_eq!(resolve(Symbol::DecisionPointSet).glyph, "𝒟");
        assert_ne!(
            resolve(Symbol::Delta).glyph,
            resolve(Symbol::DecisionPointSet).glyph
        );
    }

    #[test]
    fn glyphs_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in Symbol::ALL {
            assert!(seen.insert(resolve(s).glyph), "duplicate glyph for {:?}", s);
        }
    }
}
