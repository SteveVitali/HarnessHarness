//! `opacity_dynamic` (spec §5h.2 §2.5; OQ-036's dynamic half; ADR-0045 D7;
//! S3.3).
//!
//! `opacity_dynamic(run)` is the distribution over model calls of
//! `untyped-text tokens ÷ context tokens` read from
//! `context.assembled{items[{kind, tokens}]}`, reported as median and P95
//! beside the static `opacity_ratio` (§3; CF-100). A call with no typed items
//! contributes `1_000_000` ppm (fully opaque); an item kind spelling of
//! `untyped`/`untyped_text`/`text_untyped` counts untyped. `n/a{class}` for
//! hosted rows is the *caller's* cell decision (the declaration's
//! `applies_to_classes` — this function only computes the distribution).

use crate::facts::LedgerFacts;
use crate::stats::{median, quantile};

/// Item-kind spellings that count as *typed* (everything else is untyped text
/// for this metric — conservative: an unknown kind counts untyped, never
/// silently typed).
pub fn is_typed_kind(kind: &str) -> bool {
    matches!(
        kind,
        "artefact" | "artifact_excerpt" | "tool_schema" | "typed" | "structured"
    )
}

/// The per-call opacity mass: `(untyped_ppm per call)` — one entry per
/// `context.assembled` row.
pub fn per_call_opacity(f: &LedgerFacts) -> Vec<i64> {
    f.assembled
        .iter()
        .map(|(_, items)| {
            let total: i64 = items.iter().map(|i| i.tokens.max(0)).sum();
            if total == 0 {
                // An empty plan is fully opaque — nothing typed was
                // delivered. (The call still counts — never dropped.)
                return 1_000_000;
            }
            let untyped: i64 = items
                .iter()
                .filter(|i| !is_typed_kind(&i.kind))
                .map(|i| i.tokens.max(0))
                .sum();
            (untyped as i128 * 1_000_000i128 / total as i128) as i64
        })
        .collect()
}

/// `opacity_dynamic(run)` → `{median_ppm, p95_ppm}` — `None` when the run
/// assembled no contexts (`n/a{not_run}` upstream, never 0).
pub fn opacity_dynamic(f: &LedgerFacts) -> Option<(i64, i64)> {
    let per_call = per_call_opacity(f);
    if per_call.is_empty() {
        return None;
    }
    Some((median(&per_call)?, quantile(&per_call, 950_000)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::AssembledItem;

    #[test]
    fn opacity_distribution() {
        let mut f = LedgerFacts::default();
        f.assembled.push((
            "c1".into(),
            vec![
                AssembledItem {
                    kind: "artefact".into(),
                    artefact_id: Some("a".into()),
                    tokens: 500,
                },
                AssembledItem {
                    kind: "untyped".into(),
                    artefact_id: None,
                    tokens: 500,
                },
            ],
        ));
        f.assembled.push((
            "c2".into(),
            vec![AssembledItem {
                kind: "untyped".into(),
                artefact_id: None,
                tokens: 100,
            }],
        ));
        let (med, p95) = opacity_dynamic(&f).unwrap();
        assert_eq!(med, 500_000); // nearest-rank median of [500k, 1M] → 500k
        assert_eq!(p95, 1_000_000);
    }

    #[test]
    fn no_calls_is_na() {
        assert_eq!(opacity_dynamic(&LedgerFacts::default()), None);
    }
}
