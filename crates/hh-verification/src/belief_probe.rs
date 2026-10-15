//! `BeliefProbeRecord` — the per-step belief-probe row
//! (`verification.belief.probe`; §5f R-2.7.2b Stage-5 — belief probes as
//! profile-owned `ProfileRule{kind: belief_probe}`s with
//! `AssumptionDebtRecord`s, the probe's elicitation compared against the
//! handle it named).
//!
//! Every `belief_divergence` reading is `provisional` (ADR-0047 D6's
//! `teacher_relative` precedent — single-origin until corroborated) and
//! `requires_observability = model_io`; the judged stratum never merges
//! with the deterministic one (AC-R-2.7.2b-6) — the fold keeps the two
//! detector strata as separate members, never an average across them.

use std::collections::BTreeMap;

use hh_identity::idp::idp_id;
use hh_wire::json::Json;

use crate::bind::RowView;

/// The event class the probe row lands under.
pub const BELIEF_PROBE_CLASS: &str = "verification.belief.probe";

/// `BeliefProbeRecord{probe_id, rule_id, conditioned_on?, step_ref,
/// belief_ref, observed_ref?, divergent, detector, provisional}` — the
/// per-step probe result. `provisional` is always `true` (the AC's "every
/// report" label rides the row itself, not just the render).
#[derive(Debug, Clone, PartialEq)]
pub struct BeliefProbeRecord {
    /// The probe's identity (`Ref<ProfileRule>`'s probe coordinate).
    pub probe_id: String,
    /// The `ProfileRule{kind: belief_probe}` that authored it.
    pub rule_id: String,
    /// The profile coordinate the rule is conditioned on.
    pub conditioned_on: Option<String>,
    /// The step/turn the elicitation ran at (a step ref or seq spelling).
    pub step_ref: String,
    /// The elicited belief's content address (the probe's answer).
    pub belief_ref: String,
    /// The observed handle the belief was compared against.
    pub observed_ref: Option<String>,
    /// Whether the comparison diverged.
    pub divergent: bool,
    /// The detector stratum (`deterministic | judged` — judged rows never
    /// fold into the deterministic stratum's counts).
    pub detector: String,
}

impl BeliefProbeRecord {
    /// The canonical payload.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("probe_id", Json::str(&self.probe_id)),
            ("rule_id", Json::str(&self.rule_id)),
            (
                "conditioned_on",
                self.conditioned_on
                    .as_deref()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            ("step_ref", Json::str(&self.step_ref)),
            ("belief_ref", Json::str(&self.belief_ref)),
            (
                "observed_ref",
                self.observed_ref
                    .as_deref()
                    .map(Json::str)
                    .unwrap_or(Json::Null),
            ),
            ("divergent", Json::Bool(self.divergent)),
            ("detector", Json::str(&self.detector)),
            ("provisional", Json::Bool(true)),
        ])
    }

    /// From canonical payload (`provisional` absent decodes `true` — the
    /// label is constitutive).
    pub fn from_json(j: &Json) -> Option<BeliefProbeRecord> {
        Some(BeliefProbeRecord {
            probe_id: j.get("probe_id")?.as_str()?.to_string(),
            rule_id: j.get("rule_id")?.as_str()?.to_string(),
            conditioned_on: j
                .get("conditioned_on")
                .and_then(Json::as_str)
                .map(str::to_string),
            step_ref: j.get("step_ref")?.as_str()?.to_string(),
            belief_ref: j.get("belief_ref")?.as_str()?.to_string(),
            observed_ref: j
                .get("observed_ref")
                .and_then(Json::as_str)
                .map(str::to_string),
            divergent: matches!(j.get("divergent"), Some(Json::Bool(true))),
            detector: j
                .get("detector")
                .and_then(Json::as_str)
                .unwrap_or("deterministic")
                .to_string(),
        })
    }
}

// ── The runtime emitter (R2.15; ADR-0316's revisit point) ────────────────────
//
// The `belief_probe` `ProfileRule` *is* the emitter: the driver decodes the
// sealed profile's `rules[]` at `open`/`resume` and, per recorded
// `model.call.completed`, fires each deterministic rule over the step's
// declared belief fields (§5a.3.5's optional per-step belief-probe members —
// they ride the recorded `calls[].args_raw`, so `requires_observability =
// model_io` is satisfied only when the response's parsed calls are ledgered).

/// `ProbeRule` — the decoded `ProfileRule{kind = belief_probe}` projection the
/// emitter fires (`{rule_id, conditioned_on, field, detector}`).
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeRule {
    /// The stable rule id (`rule_id` on the profile rule).
    pub rule_id: String,
    /// The profile coordinate the rule is conditioned on
    /// (`profile_id@version` — `conditioned_on` on the emitted record).
    pub conditioned_on: String,
    /// The per-step elicitation member on each parsed call's args
    /// (`params.field`, default `beliefs`).
    pub field: String,
    /// The record's `detector` stratum (`params.detector`, default
    /// `deterministic`). A `judged` probe rule is *declared but never
    /// fired* by this emitter — its comparison needs the critic leg
    /// (DF-S1.21-3 stays OPEN under `offline-only`); the fold's judged
    /// stratum honestly reports `n = 0` rather than a deterministic
    /// comparison masquerading as a judged one.
    pub detector: String,
}

/// A malformed `belief_probe` profile rule — a typed refusal at `open`,
/// never a silently-dropped conditioned rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeRuleError {
    /// A `belief_probe` rule without its mandatory assumption-debt record
    /// (profile-owned rules carry debt records — §5f T-LCD-01/-05).
    DebtMissing {
        /// The offending rule.
        rule_id: String,
    },
}

impl std::fmt::Display for ProbeRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeRuleError::DebtMissing { rule_id } => {
                write!(f, "belief_probe rule {rule_id} carries no debt record")
            }
        }
    }
}

impl std::error::Error for ProbeRuleError {}

/// `probe_rules(profile)` — decode the profile projection's `rules[]` for
/// `kind = belief_probe` members. Rules of other kinds are ignored; a
/// `belief_probe` rule missing its debt record is a typed refusal.
pub fn probe_rules(profile: &Json) -> Result<Vec<ProbeRule>, ProbeRuleError> {
    let conditioned_on = match (
        profile.get("profile_id").and_then(Json::as_str),
        profile.get("version").and_then(Json::as_str),
    ) {
        (Some(id), Some(v)) => format!("{id}@{v}"),
        (Some(id), None) => id.to_string(),
        _ => "profile/unbound".to_string(),
    };
    let mut out = Vec::new();
    let Some(Json::Arr(rules)) = profile.get("rules") else {
        return Ok(out);
    };
    for r in rules {
        if r.get("kind").and_then(Json::as_str) != Some("belief_probe") {
            continue;
        }
        let rule_id = r
            .get("rule_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        // The debt record is load-bearing (§5f: "profile-owned `ProfileRule`s
        // with assumption-debt records") — absent ⇒ typed refusal.
        match r.get("debt") {
            Some(Json::Obj(m)) if !m.is_empty() => {}
            _ => return Err(ProbeRuleError::DebtMissing { rule_id }),
        }
        let params = r.get("params").cloned().unwrap_or(Json::Null);
        let field = params
            .get("field")
            .and_then(Json::as_str)
            .unwrap_or("beliefs")
            .to_string();
        let detector = params
            .get("detector")
            .and_then(Json::as_str)
            .unwrap_or("deterministic")
            .to_string();
        out.push(ProbeRule {
            rule_id,
            conditioned_on: conditioned_on.clone(),
            field,
            detector,
        });
    }
    Ok(out)
}

/// `BeliefElicitation` — one parsed per-step belief item
/// `{subject, member, value}`: `subject` names the handle the belief is
/// asserted against (the closed spelling set is the emitter's — `effect:<id>`
/// on an `action.effect.observed` row, `verdict:<id>` on a
/// `verification.validator.verdict` row); `member` is the observed payload
/// member the `value` is compared against.
#[derive(Debug, Clone, PartialEq)]
pub struct BeliefElicitation {
    /// The handle spelling (`effect:<id>` | `verdict:<id>`).
    pub subject: String,
    /// The payload member of the resolved row to compare.
    pub member: String,
    /// The elicited value.
    pub value: Json,
}

/// `elicit(args, field)` — read the per-step belief-probe items off a parsed
/// call's `args` (the recorded model_io). Absent/non-array ⇒ no items — the
/// surface is optional; a malformed item (missing `subject`/`member`/`value`)
/// yields no record — model-claimed text is never kernel evidence (F7).
pub fn elicit(args: &Json, field: &str) -> Vec<BeliefElicitation> {
    let mut out = Vec::new();
    let Some(Json::Arr(items)) = args.get(field) else {
        return out;
    };
    for item in items {
        let (Some(subject), Some(member), Some(value)) = (
            item.get("subject").and_then(Json::as_str),
            item.get("member").and_then(Json::as_str),
            item.get("value"),
        ) else {
            continue;
        };
        out.push(BeliefElicitation {
            subject: subject.to_string(),
            member: member.to_string(),
            value: value.clone(),
        });
    }
    out
}

/// The deterministic probe id (`probe:<rule_id>:<step>:<index>`).
pub fn probe_id(rule_id: &str, step_ref: &str, index: usize) -> String {
    format!("probe:{rule_id}:{step_ref}:{index}")
}

/// `fire` — mint the `BeliefProbeRecord` for one elicitation. `belief_ref`
/// content-addresses the elicited value (`idp/1` over the canonical
/// encoding — the answer is addressed, never inlined); `observed` is the
/// resolver's `(member_value, handle_ref)` when the subject resolved against
/// the durable prefix, `None` otherwise (an unresolved handle is *not* a
/// divergence — `observed_ref` stays absent rather than fabricating a
/// comparison).
pub fn fire(
    rule: &ProbeRule,
    step_ref: &str,
    index: usize,
    item: &BeliefElicitation,
    observed: Option<(Json, String)>,
) -> BeliefProbeRecord {
    let belief_ref = idp_id("belief", item.value.to_canonical_string().as_bytes());
    let (divergent, observed_ref) = match observed {
        Some((value, handle_ref)) => (value != item.value, Some(handle_ref)),
        None => (false, None),
    };
    BeliefProbeRecord {
        probe_id: probe_id(&rule.rule_id, step_ref, index),
        rule_id: rule.rule_id.clone(),
        conditioned_on: Some(rule.conditioned_on.clone()),
        step_ref: step_ref.to_string(),
        belief_ref,
        observed_ref,
        divergent,
        detector: rule.detector.clone(),
    }
}

/// `belief_divergence(events)` — the provisional divergence vector
/// (§5f's "arrival/growth terms"): `{probes, divergent, arrival_ppm,
/// growth_ppm, by_detector{deterministic|judged}}`. `arrival_ppm` is the
/// share of probes that diverged; `growth_ppm` is the second-half minus
/// first-half divergence-rate delta (signed ppm — a decaying belief
/// drift reads negative). Detector strata are reported side by side and
/// never merged (AC-R-2.7.2b-6); a run with no probes folds `n = 0`.
pub fn belief_divergence(rows: &[RowView<'_>]) -> Json {
    let mut strata: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut seqs: Vec<(u64, bool)> = Vec::new();
    for e in rows {
        if e.class != BELIEF_PROBE_CLASS {
            continue;
        }
        let Some(r) = BeliefProbeRecord::from_json(e.payload) else {
            continue;
        };
        let (n, d) = strata.entry(r.detector.clone()).or_default();
        *n += 1;
        *d += r.divergent as i64;
        seqs.push((e.seq, r.divergent));
    }
    let probes = seqs.len() as i64;
    let divergent: i64 = seqs.iter().filter(|(_, d)| *d).count() as i64;
    let ppm = |d: i64, n: i64| -> i64 {
        if n == 0 {
            0
        } else {
            d * 1_000_000 / n
        }
    };
    // Growth: the second-half divergence rate minus the first's (the
    // arrival/growth vector — deterministic over the seq-ordered fold).
    let half = seqs.len() / 2;
    let growth_ppm = if probes >= 2 {
        let (a, b) = seqs.split_at(half);
        ppm(b.iter().filter(|(_, d)| *d).count() as i64, b.len() as i64)
            - ppm(a.iter().filter(|(_, d)| *d).count() as i64, a.len() as i64)
    } else {
        0
    };
    Json::obj([
        ("probes", Json::Int(probes)),
        ("divergent", Json::Int(divergent)),
        ("arrival_ppm", Json::Int(ppm(divergent, probes))),
        ("growth_ppm", Json::Int(growth_ppm)),
        (
            "by_detector",
            Json::Obj(
                strata
                    .iter()
                    .map(|(det, (n, d))| {
                        (
                            det.clone(),
                            Json::obj([
                                ("probes", Json::Int(*n)),
                                ("divergent", Json::Int(*d)),
                                ("arrival_ppm", Json::Int(ppm(*d, *n))),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
        ("provisional", Json::Bool(true)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_provenance::authority::AuthorityClass;

    fn probe(seq: u64, detector: &str, divergent: bool) -> (u64, String, Json) {
        (
            seq,
            BELIEF_PROBE_CLASS.to_string(),
            BeliefProbeRecord {
                probe_id: "probe/x".into(),
                rule_id: "rule/x".into(),
                conditioned_on: Some("profile/p".into()),
                step_ref: format!("step/{seq}"),
                belief_ref: "sha256:b".into(),
                observed_ref: Some("sha256:o".into()),
                divergent,
                detector: detector.into(),
            }
            .to_json(),
        )
    }

    fn views(rows: &[(u64, String, Json)]) -> Vec<RowView<'_>> {
        rows.iter()
            .map(|(seq, class, payload)| RowView {
                seq: *seq,
                class,
                payload,
                authority: AuthorityClass::Kernel,
                scope_effect_id: None,
            })
            .collect()
    }

    #[test]
    fn divergence_folds_per_detector_never_merged() {
        let rows = vec![
            probe(1, "deterministic", true),
            probe(2, "deterministic", false),
            probe(3, "judged", false),
        ];
        let v = belief_divergence(&views(&rows));
        assert_eq!(v.get("probes").and_then(Json::as_int), Some(3));
        assert_eq!(v.get("divergent").and_then(Json::as_int), Some(1));
        assert_eq!(
            v.get("provisional"),
            Some(&Json::Bool(true)),
            "provisional rides every reading"
        );
        let det = v
            .get("by_detector")
            .and_then(|m| m.get("deterministic"))
            .unwrap();
        assert_eq!(det.get("arrival_ppm").and_then(Json::as_int), Some(500_000));
        let judged = v.get("by_detector").and_then(|m| m.get("judged")).unwrap();
        assert_eq!(judged.get("probes").and_then(Json::as_int), Some(1));
    }

    // ── R2.15 — the runtime emitter's decode/elicit/fire legs ────────────

    fn profile_with_rules(rules: Json) -> Json {
        Json::obj([
            ("profile_id", Json::str("profile/test")),
            ("version", Json::str("1")),
            ("rules", rules),
        ])
    }

    fn probe_rule(detector: &str) -> Json {
        Json::obj([
            ("kind", Json::str("belief_probe")),
            ("rule_id", Json::str("rule/bp-1")),
            ("debt", Json::obj([("id", Json::str("debt/bp-1"))])),
            (
                "params",
                Json::obj([
                    ("field", Json::str("beliefs")),
                    ("detector", Json::str(detector)),
                ]),
            ),
        ])
    }

    #[test]
    fn probe_rules_decodes_belief_probe_members_only() {
        let p = profile_with_rules(Json::Arr(vec![
            probe_rule("deterministic"),
            // A non-belief_probe rule is ignored, never decoded.
            Json::obj([
                ("kind", Json::str("loop_nudge")),
                ("rule_id", Json::str("rule/other")),
            ]),
        ]));
        let rules = probe_rules(&p).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule_id, "rule/bp-1");
        assert_eq!(rules[0].conditioned_on, "profile/test@1");
        assert_eq!(rules[0].field, "beliefs");
        assert_eq!(rules[0].detector, "deterministic");
    }

    #[test]
    fn probe_rules_refuses_a_debtless_rule_typed() {
        let p = profile_with_rules(Json::Arr(vec![Json::obj([
            ("kind", Json::str("belief_probe")),
            ("rule_id", Json::str("rule/naked")),
        ])]));
        assert_eq!(
            probe_rules(&p),
            Err(ProbeRuleError::DebtMissing {
                rule_id: "rule/naked".into()
            }),
            "a conditioned rule without its assumption-debt record is a typed refusal, never a silent drop"
        );
    }

    #[test]
    fn elicit_reads_declared_items_and_skips_malformed() {
        let args = Json::obj([(
            "beliefs",
            Json::Arr(vec![
                Json::obj([
                    ("subject", Json::str("effect:ef-1")),
                    ("member", Json::str("outcome")),
                    ("value", Json::str("ok")),
                ]),
                // Malformed — model-claimed text is never kernel evidence;
                // the item is dropped, not guessed.
                Json::obj([("subject", Json::str("effect:ef-2"))]),
            ]),
        )]);
        let items = elicit(&args, "beliefs");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].subject, "effect:ef-1");
        assert_eq!(items[0].member, "outcome");
        // A different field name reads nothing.
        assert!(elicit(&args, "priors").is_empty());
    }

    #[test]
    fn fire_marks_divergence_only_on_a_resolved_mismatch() {
        let rule = ProbeRule {
            rule_id: "rule/bp-1".into(),
            conditioned_on: "profile/test@1".into(),
            field: "beliefs".into(),
            detector: "deterministic".into(),
        };
        let item = BeliefElicitation {
            subject: "effect:ef-1".into(),
            member: "outcome".into(),
            value: Json::str("ok"),
        };
        // Resolved and matching — not divergent.
        let same = fire(
            &rule,
            "mc-1",
            0,
            &item,
            Some((Json::str("ok"), "row:ev-9".into())),
        );
        assert!(!same.divergent);
        assert_eq!(same.observed_ref.as_deref(), Some("row:ev-9"));
        // Resolved and mismatched — divergent, citing the durable row.
        let diff = fire(
            &rule,
            "mc-1",
            1,
            &item,
            Some((Json::str("error"), "row:ev-9".into())),
        );
        assert!(diff.divergent);
        // Unresolved — no observed_ref, never a fabricated divergence.
        let unresolved = fire(&rule, "mc-1", 2, &item, None);
        assert!(!unresolved.divergent);
        assert!(unresolved.observed_ref.is_none());
        assert_eq!(unresolved.probe_id, "probe:rule/bp-1:mc-1:2");
        // `provisional` is constitutive on the emitted record.
        assert_eq!(
            unresolved.to_json().get("provisional"),
            Some(&Json::Bool(true))
        );
    }

    #[test]
    fn record_round_trips_and_provisional_is_constitutive() {
        let r = BeliefProbeRecord {
            probe_id: "probe/a".into(),
            rule_id: "rule/a".into(),
            conditioned_on: None,
            step_ref: "step/7".into(),
            belief_ref: "sha256:x".into(),
            observed_ref: None,
            divergent: true,
            detector: "judged".into(),
        };
        let j = r.to_json();
        assert_eq!(j.get("provisional"), Some(&Json::Bool(true)));
        assert_eq!(BeliefProbeRecord::from_json(&j), Some(r));
    }
}
