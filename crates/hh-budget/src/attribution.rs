//! `Attribution` and `charged_to` (§8.2 §3 `Attribution` row; ADR-0039 D3 as amended
//! CF-174/307; ADR-0181 consequences for R-ACC-4).
//!
//! **R-ACC-3 (subject vs instrument):** producers bound in the run's sealed Harness
//! Definition — its own validators, judges, compaction calls, embedder and
//! consolidation calls — charge `subject`. The experiment's grader, conformance
//! probes, Lab-side activation/following detectors, hosting-adapter calls,
//! `registry_ci` runs, user simulators and Lab search charge `instrument`. Both are
//! always recorded; efficiency metrics default to `subject` and report `instrument`
//! beside it.
//!
//! **R-ACC-4:** out-of-process binding cost is attributed to the subject by default
//! with `component_variant_ref` and `binding_locality` set (stratifiable).

use hh_identity::IrRef;
use hh_ledger::manifest::EventRef;
use hh_wire::json::Json;
use std::collections::BTreeMap;

/// `charged_to ∈ {subject, instrument}` (R-ACC-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChargedTo {
    /// The run's own workload — producers bound in the sealed Harness Definition
    /// (including its validators, judges, compaction, embedder, consolidation calls).
    Subject,
    /// The measurement instrument — the experiment's grader, conformance probes,
    /// Lab-side detectors, hosting-adapter calls, `registry_ci`, user simulators,
    /// Lab search.
    Instrument,
}

impl ChargedTo {
    pub const ALL: [ChargedTo; 2] = [ChargedTo::Subject, ChargedTo::Instrument];

    pub fn as_str(self) -> &'static str {
        match self {
            ChargedTo::Subject => "subject",
            ChargedTo::Instrument => "instrument",
        }
    }

    pub fn parse(s: &str) -> Option<ChargedTo> {
        match s {
            "subject" => Some(ChargedTo::Subject),
            "instrument" => Some(ChargedTo::Instrument),
            _ => None,
        }
    }
}

/// `model_ref{profile_ref, provider_model_id, serving_route, effort?}` — the model the
/// usage was metered against. Priced at the *served* model (`serving_route`), never the
/// requested one (ADR-0039; §05b.1's `served_model`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModelRef {
    /// The Model Profile ref (semantic/version id).
    pub profile_ref: String,
    /// The provider's model id.
    pub provider_model_id: String,
    /// The serving route actually used.
    pub serving_route: String,
    /// Effort/reasoning tier, when the profile declares one.
    pub effort: Option<String>,
}

impl ModelRef {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("profile_ref".to_string(), Json::str(&self.profile_ref));
        m.insert(
            "provider_model_id".to_string(),
            Json::str(&self.provider_model_id),
        );
        m.insert("serving_route".to_string(), Json::str(&self.serving_route));
        if let Some(e) = &self.effort {
            m.insert("effort".to_string(), Json::str(e));
        }
        Json::Obj(m)
    }

    pub fn from_json(j: &Json) -> Option<ModelRef> {
        Some(ModelRef {
            profile_ref: j.get("profile_ref")?.as_str()?.to_string(),
            provider_model_id: j.get("provider_model_id")?.as_str()?.to_string(),
            serving_route: j.get("serving_route")?.as_str()?.to_string(),
            effort: j.get("effort").and_then(Json::as_str).map(str::to_string),
        })
    }

    /// The pricing key — the served model (`provider_model_id`@`serving_route`).
    /// `PricingTable` rows key on this string.
    pub fn pricing_key(&self) -> String {
        format!("{}@{}", self.provider_model_id, self.serving_route)
    }
}

/// `cache{hit, kind}` — cache-attribution member (§8.2 `Attribution.cache?{hit}`;
/// ADR-0039 P2: cache hits post zero-amount charges with `cache.hit = true` and an
/// avoided-cost estimate beside, never subtracted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheAttribution {
    /// The charge is for a cache hit (amount is typically 0 — the hit avoided spend).
    pub hit: bool,
    /// The cache kind (`K1` provider prefix … `K6` semantic — §05b.4's taxonomy).
    pub kind: Option<String>,
    /// The avoided-cost estimate's event ref (a `measurement.cost.attributed` row with
    /// `provenance = estimated_from_pricing`, `confidence = estimate`) — *beside* the
    /// hit charge, never subtracted (ADR-0128).
    pub avoided_ref: Option<EventRef>,
}

impl CacheAttribution {
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("hit".to_string(), Json::Bool(self.hit));
        if let Some(k) = &self.kind {
            m.insert("kind".to_string(), Json::str(k));
        }
        if let Some(r) = &self.avoided_ref {
            m.insert(
                "avoided_ref".to_string(),
                Json::obj([
                    ("run_id", Json::str(&r.run_id)),
                    ("event_id", Json::str(&r.event_id)),
                ]),
            );
        }
        Json::Obj(m)
    }

    pub fn from_json(j: &Json) -> Option<CacheAttribution> {
        let hit = match j.get("hit") {
            Some(Json::Bool(b)) => *b,
            _ => return None,
        };
        let avoided_ref = match j.get("avoided_ref") {
            Some(r) => Some(EventRef {
                run_id: r.get("run_id")?.as_str()?.to_string(),
                event_id: r.get("event_id")?.as_str()?.to_string(),
            }),
            None => None,
        };
        Some(CacheAttribution {
            hit,
            kind: j.get("kind").and_then(Json::as_str).map(str::to_string),
            avoided_ref,
        })
    }
}

/// `Attribution{run_id, participant_ref, charged_to, budget_id, component_class?,
/// component_variant_ref?, binding_locality?, model_ref?, ir_refs[], cache?{hit}}`
/// (§8.2 §3 — verbatim).
#[derive(Debug, Clone, PartialEq)]
pub struct Attribution {
    /// The run the charge belongs to.
    pub run_id: String,
    /// The participant the producing component ran as.
    pub participant_ref: String,
    /// `subject` | `instrument` (R-ACC-3).
    pub charged_to: ChargedTo,
    /// The budget node charged.
    pub budget_id: String,
    /// The producing component's class (e.g. `model`, `executor`, `compaction`).
    pub component_class: Option<String>,
    /// The component variant ref (content address / version id) — set for
    /// out-of-process bindings (R-ACC-4).
    pub component_variant_ref: Option<String>,
    /// The binding locality (e.g. `in_process`, `plugin`, `hosted`) — set for
    /// out-of-process bindings (R-ACC-4).
    pub binding_locality: Option<String>,
    /// The model the usage was metered against (token/spend charges).
    pub model_ref: Option<ModelRef>,
    /// The `{semantic_id, version_id}` pairs the producing config resolves to.
    pub ir_refs: Vec<IrRef>,
    /// Cache attribution (hit charges are zero-amount).
    pub cache: Option<CacheAttribution>,
    /// Charged beyond the consumed reservation — flagged, never refused after the fact.
    pub over_reservation: bool,
}

impl Attribution {
    /// The minimal charge attribution — subject-side, in-process, no model.
    pub fn subject(
        run_id: impl Into<String>,
        budget_id: impl Into<String>,
        participant_ref: impl Into<String>,
    ) -> Attribution {
        Attribution {
            run_id: run_id.into(),
            participant_ref: participant_ref.into(),
            charged_to: ChargedTo::Subject,
            budget_id: budget_id.into(),
            component_class: None,
            component_variant_ref: None,
            binding_locality: None,
            model_ref: None,
            ir_refs: vec![],
            cache: None,
            over_reservation: false,
        }
    }

    /// The instrument-side variant.
    pub fn instrument(
        run_id: impl Into<String>,
        budget_id: impl Into<String>,
        participant_ref: impl Into<String>,
    ) -> Attribution {
        Attribution {
            charged_to: ChargedTo::Instrument,
            ..Attribution::subject(run_id, budget_id, participant_ref)
        }
    }

    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("run_id".to_string(), Json::str(&self.run_id));
        m.insert(
            "participant_ref".to_string(),
            Json::str(&self.participant_ref),
        );
        m.insert(
            "charged_to".to_string(),
            Json::str(self.charged_to.as_str()),
        );
        m.insert("budget_id".to_string(), Json::str(&self.budget_id));
        if let Some(c) = &self.component_class {
            m.insert("component_class".to_string(), Json::str(c));
        }
        if let Some(c) = &self.component_variant_ref {
            m.insert("component_variant_ref".to_string(), Json::str(c));
        }
        if let Some(l) = &self.binding_locality {
            m.insert("binding_locality".to_string(), Json::str(l));
        }
        if let Some(r) = &self.model_ref {
            m.insert("model_ref".to_string(), r.to_json());
        }
        m.insert(
            "ir_refs".to_string(),
            Json::Arr(
                self.ir_refs
                    .iter()
                    .map(|r| {
                        let mut o = BTreeMap::new();
                        if let Some(s) = &r.semantic_id {
                            o.insert("semantic_id".to_string(), Json::str(s));
                        }
                        o.insert("version_id".to_string(), Json::str(&r.version_id));
                        Json::Obj(o)
                    })
                    .collect(),
            ),
        );
        if let Some(c) = &self.cache {
            m.insert("cache".to_string(), c.to_json());
        }
        if self.over_reservation {
            m.insert("over_reservation".to_string(), Json::Bool(true));
        }
        Json::Obj(m)
    }

    pub fn from_json(j: &Json) -> Option<Attribution> {
        let ir_refs = match j.get("ir_refs") {
            Some(Json::Arr(rs)) => rs
                .iter()
                .map(|r| {
                    Some(IrRef {
                        semantic_id: r
                            .get("semantic_id")
                            .and_then(Json::as_str)
                            .map(str::to_string),
                        version_id: r.get("version_id")?.as_str()?.to_string(),
                    })
                })
                .collect::<Option<Vec<_>>>()?,
            _ => vec![],
        };
        Some(Attribution {
            run_id: j.get("run_id")?.as_str()?.to_string(),
            participant_ref: j.get("participant_ref")?.as_str()?.to_string(),
            charged_to: ChargedTo::parse(j.get("charged_to")?.as_str()?)?,
            budget_id: j.get("budget_id")?.as_str()?.to_string(),
            component_class: j
                .get("component_class")
                .and_then(Json::as_str)
                .map(str::to_string),
            component_variant_ref: j
                .get("component_variant_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            binding_locality: j
                .get("binding_locality")
                .and_then(Json::as_str)
                .map(str::to_string),
            model_ref: j.get("model_ref").and_then(ModelRef::from_json),
            ir_refs,
            cache: j.get("cache").and_then(CacheAttribution::from_json),
            over_reservation: matches!(j.get("over_reservation"), Some(Json::Bool(true))),
        })
    }
}

/// R-ACC-3's producer table — `charged_to` by event class. The run's own planes
/// (`model.*`, `action.*`, `context.*`, `lifecycle.*`, `control.*`) charge `subject`;
/// measurement classes that name the experiment's instruments (`verification.*`,
/// `measurement.*` emitted by Lab-side detectors/graders) charge `instrument`.
///
/// Callers that *know* better (a kernel-produced `measurement.cost.attributed` for a
/// subject-side cache-avoidance estimate) pass an explicit `charged_to`; this table is
/// the default the account uses when attribution is derived rather than supplied.
pub fn default_charged_to(class: &str) -> ChargedTo {
    if class.starts_with("verification.") || class.starts_with("measurement.") {
        ChargedTo::Instrument
    } else {
        ChargedTo::Subject
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribution_round_trip() {
        let mut a = Attribution::subject("run-1", "budget-1", "participant-0");
        a.model_ref = Some(ModelRef {
            profile_ref: "sha256:aa".into(),
            provider_model_id: "claude-x".into(),
            serving_route: "anthropic-prod".into(),
            effort: Some("high".into()),
        });
        a.cache = Some(CacheAttribution {
            hit: true,
            kind: Some("K1".into()),
            avoided_ref: None,
        });
        a.over_reservation = true;
        let j = a.to_json();
        assert_eq!(Attribution::from_json(&j), Some(a));
    }

    #[test]
    fn r_acc_3_default_table() {
        assert_eq!(
            default_charged_to("model.call.completed"),
            ChargedTo::Subject
        );
        assert_eq!(
            default_charged_to("action.tool.completed"),
            ChargedTo::Subject
        );
        assert_eq!(
            default_charged_to("lifecycle.component.invoked"),
            ChargedTo::Subject
        );
        assert_eq!(
            default_charged_to("verification.validator.invoked"),
            ChargedTo::Instrument
        );
        assert_eq!(
            default_charged_to("measurement.experiment.bound"),
            ChargedTo::Instrument
        );
    }
}
