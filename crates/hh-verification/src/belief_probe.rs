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
        let judged = v
            .get("by_detector")
            .and_then(|m| m.get("judged"))
            .unwrap();
        assert_eq!(judged.get("probes").and_then(Json::as_int), Some(1));
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
