//! The loss-report export (spec §5h.2 §2.1; R-2.9.2 AC-2; S3.3).
//!
//! The compiler's `LoweringLossReport` (the `hh-compiler::lcd` record) is
//! exported verbatim into the eval plane as `loss_report_export/1` — every
//! `target`, `target_version`, `entries[{hir_node_id, field, class,
//! severity, detail, debt_ref}]` and the `granularity_ceiling` is preserved;
//! nothing is summarised away (CC3 — nothing silently lost).

use hh_compiler::lcd::LoweringLossReport;
use hh_wire::Json;

/// `loss_report_export/1` — one exported lowering-loss report plus its
/// content address (`loss_report_ref` — the member `CompiledBundle` names).
pub fn export_loss_report(report: &LoweringLossReport) -> Json {
    let body = Json::obj([
        ("schema", Json::str("loss_report_export/1")),
        ("target", Json::str(&report.target)),
        ("target_version", Json::str(&report.target_version)),
        (
            "entries",
            Json::Arr(
                report
                    .entries
                    .iter()
                    .map(|e| {
                        let mut m = std::collections::BTreeMap::new();
                        m.insert("hir_node_id".to_string(), Json::str(&e.hir_node_id));
                        m.insert("field".to_string(), Json::str(&e.field));
                        m.insert("class".to_string(), Json::str(e.class.name()));
                        m.insert("severity".to_string(), Json::str(e.severity.name()));
                        m.insert("detail".to_string(), Json::str(&e.detail));
                        if let Some(d) = &e.debt_ref {
                            m.insert("debt_ref".to_string(), Json::str(d));
                        }
                        Json::Obj(m)
                    })
                    .collect(),
            ),
        ),
        (
            "granularity_ceiling",
            Json::str(report.granularity_ceiling.name()),
        ),
    ]);
    let loss_ref = hh_identity::idp_id("eval.loss_report", body.to_canonical_string().as_bytes());
    let mut m = match body {
        Json::Obj(m) => m,
        _ => unreachable!(),
    };
    m.insert("loss_report_ref".to_string(), Json::str(&loss_ref));
    Json::Obj(m)
}

/// Export every per-target report for a bundle (`bundle.lowering_loss` →
/// one export row per target).
pub fn export_bundle_losses(reports: &[LoweringLossReport]) -> Json {
    Json::obj([
        ("schema", Json::str("loss_report_export_set/1")),
        (
            "reports",
            Json::Arr(reports.iter().map(export_loss_report).collect()),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_compiler::lcd::{
        GranularityCeiling, LossEntry, LossKind, LossSeverity, LoweringLossReport,
    };

    #[test]
    fn export_preserves_every_member() {
        let r = LoweringLossReport {
            target: "mcp".into(),
            target_version: "v1".into(),
            entries: vec![LossEntry {
                hir_node_id: "n1".into(),
                field: "allowed_capabilities".into(),
                class: LossKind::NoSlot,
                severity: LossSeverity::Lost,
                detail: "target has no capability slot".into(),
                debt_ref: Some("debt-1".into()),
            }],
            granularity_ceiling: GranularityCeiling::Product,
        };
        let j = export_loss_report(&r);
        assert_eq!(j.get("target").and_then(Json::as_str), Some("mcp"));
        let entries = j.get("entries").unwrap();
        let e = &entries.get("").unwrap_or(entries); // array
        let _ = e;
        let Json::Arr(es) = entries else {
            panic!("entries not an array")
        };
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].get("class").and_then(Json::as_str), Some("no_slot"));
        assert_eq!(es[0].get("debt_ref").and_then(Json::as_str), Some("debt-1"));
        assert_eq!(
            j.get("granularity_ceiling").and_then(Json::as_str),
            Some("product")
        );
        assert!(j.get("loss_report_ref").and_then(Json::as_str).is_some());
    }
}
