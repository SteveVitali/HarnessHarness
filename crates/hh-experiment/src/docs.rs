//! `LabDocs` — the Lab's durable document store (§6.3; S3.4a). Registered
//! `ExperimentSpec`s and expanded `CellPlan`s are content-addressed canonical
//! JSON under `<store_root>/lab_docs/<kind>/<id>.json` — durable, immutable,
//! idempotent (`put` of the same body is the same file). The
//! `index/experiment.json` map binds `experiment_id → {plan_id, run_id}` so
//! the lifecycle ops address an experiment by its content id (the `hh` CLI's
//! `<experiment-id>` argument) rather than the engine's run id.
//!
//! CC3: one canonical codec (`Json::to_canonical_string`), one identity scheme
//! (`idp/1` under the `lab_doc.<kind>` domain). This store holds *documents*;
//! the experiment's mutable scheduling state lives only in the experiment
//! run's ledger (S-1 — no memory-only state).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use hh_identity::idp::idp_id;
use hh_lab::experiment::{CellPlan, ExperimentSpec};
use hh_wire::json::{parse as parse_json, Json};

use crate::errors::ExperimentError;

/// The document kinds the store holds.
pub mod kind {
    /// A registered `hh-experiment/1` spec.
    pub const SPEC: &str = "experiment";
    /// An expanded `CellPlan`.
    pub const PLAN: &str = "plan";
    /// A pinned `BudgetSpec` body (the `eval_budget`/`search_budget`/
    /// `budgets.experiment` refs resolve here when no richer resolver is
    /// wired).
    pub const BUDGET: &str = "budget";
    /// A persisted `AnalysisRecord` (§6.4/§6.5 — written by the estimator
    /// kernel's `analyze_and_record`; S3.4c).
    pub const ANALYSIS: &str = "analysis";
    /// A persisted `analysis_report_body/1` — the full report payload an
    /// `AnalysisReport.result_ref` addresses (S3.4c).
    pub const ANALYSIS_REPORT: &str = "analysis_report";
}

/// The durable Lab document store rooted at `<store_root>/lab_docs`.
#[derive(Debug, Clone)]
pub struct LabDocs {
    root: PathBuf,
}

/// The `index/experiment.json` row for one experiment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExperimentIndexEntry {
    /// The expanded `CellPlan` id (`None` until `expand` runs).
    pub plan_id: Option<String>,
    /// The experiment run id (`None` until `open_experiment` runs).
    pub run_id: Option<String>,
}

fn err(detail: impl Into<String>) -> ExperimentError {
    ExperimentError::Store {
        detail: detail.into(),
    }
}

/// Sanitize a content id into a file name (`:` is legal but awkward).
fn file_name(id: &str) -> String {
    id.replace(':', "_") + ".json"
}

impl LabDocs {
    /// Open (creating) the store under `store_root`.
    pub fn open(store_root: &Path) -> Result<LabDocs, ExperimentError> {
        let root = store_root.join("lab_docs");
        fs::create_dir_all(root.join("index")).map_err(|e| err(format!("create: {e}")))?;
        Ok(LabDocs { root })
    }

    /// The store root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, kind: &str, id: &str) -> PathBuf {
        self.root.join(kind).join(file_name(id))
    }

    /// The document id for `body` under `kind`. Registered kinds carry their
    /// own identity rule (CC1 — one identity scheme per record kind): a spec
    /// is addressed by `experiment_id = H(canonical(spec minus
    /// experiment_id))` under `experiment`, a plan by `plan_id =
    /// H(canonical(plan minus plan_id))` under `cell_plan`. Unknown kinds
    /// fall back to the store's own `lab_doc.<kind>` address over the body.
    fn doc_id(kind: &str, body: &Json) -> Result<String, ExperimentError> {
        match kind {
            k if k == kind::SPEC => Ok(ExperimentSpec::from_json(body)
                .map_err(|e| err(format!("spec decode: {e:?}")))?
                .experiment_id()),
            k if k == kind::PLAN => Ok(CellPlan::from_json(body)
                .map_err(|e| err(format!("plan decode: {e:?}")))?
                .plan_id()),
            _ => Ok(idp_id(
                &format!("lab_doc.{kind}"),
                body.to_canonical_string().as_bytes(),
            )),
        }
    }

    /// `put(kind, body) → id` — the kind's identity rule ([`LabDocs::doc_id`]),
    /// durable-before-return, idempotent.
    pub fn put(&self, kind: &str, body: &Json) -> Result<String, ExperimentError> {
        let id = Self::doc_id(kind, body)?;
        let dir = self.root.join(kind);
        fs::create_dir_all(&dir).map_err(|e| err(format!("create {kind}: {e}")))?;
        let path = dir.join(file_name(&id));
        if !path.exists() {
            // Write-then-rename so a torn write never leaves a half doc under
            // the address (durable-before-visible, mirrored from the WAL).
            let tmp = dir.join(format!(".{}.tmp", file_name(&id)));
            fs::write(&tmp, body.to_canonical_string().as_bytes())
                .map_err(|e| err(format!("write: {e}")))?;
            fs::rename(&tmp, &path).map_err(|e| err(format!("rename: {e}")))?;
        }
        Ok(id)
    }

    /// `get(kind, id)` — the body, verified against its address (a doc whose
    /// bytes no longer hash to `id` is corruption, not a miss).
    pub fn get(&self, kind: &str, id: &str) -> Result<Option<Json>, ExperimentError> {
        let path = self.path(kind, id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|e| err(format!("read: {e}")))?;
        let text = String::from_utf8(bytes).map_err(|e| err(format!("utf8: {e}")))?;
        let body = parse_json(&text).map_err(|e| err(format!("parse: {e:?}")))?;
        if Self::doc_id(kind, &body)? != id {
            return Err(err(format!("doc {id} failed its address check")));
        }
        Ok(Some(body))
    }

    /// Register a spec (idempotent — the content address is the key).
    pub fn put_spec(&self, spec: &ExperimentSpec) -> Result<String, ExperimentError> {
        let id = self.put(kind::SPEC, &spec.to_json())?;
        if id != spec.experiment_id {
            // The spec's declared id must be its content address — a mismatch
            // is a schema violation, not a re-keying.
            return Err(ExperimentError::Refusal(
                hh_lab::experiment::ExperimentRefusal::Schema(hh_lab::json_util::SchemaError::v(
                    "experiment_id",
                    "spec.experiment_id is not the content address of the spec",
                )),
            ));
        }
        Ok(id)
    }

    /// The registered spec (`None` = unregistered id).
    pub fn spec(&self, experiment_id: &str) -> Result<Option<ExperimentSpec>, ExperimentError> {
        match self.get(kind::SPEC, experiment_id)? {
            Some(j) => {
                let spec = ExperimentSpec::from_json(&j)
                    .map_err(|e| err(format!("spec {experiment_id} decode: {e:?}")))?;
                Ok(Some(spec))
            }
            None => Ok(None),
        }
    }

    /// Store an expanded plan and bind `experiment_id → plan_id` in the index.
    pub fn put_plan(
        &self,
        experiment_id: &str,
        plan: &CellPlan,
    ) -> Result<String, ExperimentError> {
        let id = self.put(kind::PLAN, &plan.to_json())?;
        let mut entry = self.index_entry(experiment_id)?.unwrap_or_default();
        entry.plan_id = Some(id.clone());
        self.set_index_entry(experiment_id, entry)?;
        Ok(id)
    }

    /// A stored plan.
    pub fn plan(&self, plan_id: &str) -> Result<Option<CellPlan>, ExperimentError> {
        match self.get(kind::PLAN, plan_id)? {
            Some(j) => {
                Ok(Some(CellPlan::from_json(&j).map_err(|e| {
                    err(format!("plan {plan_id} decode: {e:?}"))
                })?))
            }
            None => Ok(None),
        }
    }

    /// The index row for an experiment id.
    pub fn index_entry(
        &self,
        experiment_id: &str,
    ) -> Result<Option<ExperimentIndexEntry>, ExperimentError> {
        let idx = self.read_index()?;
        Ok(idx.get(experiment_id).cloned())
    }

    /// Merge one experiment's index row.
    pub fn set_index_entry(
        &self,
        experiment_id: &str,
        entry: ExperimentIndexEntry,
    ) -> Result<(), ExperimentError> {
        let mut idx = self.read_index()?;
        idx.insert(experiment_id.to_string(), entry);
        self.write_index(&idx)
    }

    /// `put_named(kind, name, body)` — a *named* deposit under
    /// `named/<kind>/<sanitized>` (the boundary's records-in resolver
    /// surface: a `budgets{ref → body}` map keyed by the spec's ref
    /// spelling). Named docs are not content addresses — no address check —
    /// but they are durable and idempotent (same name + same body = the same
    /// file; a *different* body under the same name is a conflict error,
    /// never a silent overwrite).
    pub fn put_named(&self, kind: &str, name: &str, body: &Json) -> Result<(), ExperimentError> {
        let dir = self.root.join("named").join(kind);
        fs::create_dir_all(&dir).map_err(|e| err(format!("create named/{kind}: {e}")))?;
        let path = dir.join(file_name(name));
        let row = Json::obj([("name", Json::str(name)), ("body", body.clone())]);
        let bytes = row.to_canonical_string();
        if path.exists() {
            let existing = fs::read_to_string(&path).map_err(|e| err(format!("read: {e}")))?;
            if existing != bytes {
                return Err(err(format!(
                    "named doc {kind}/{name} already exists with a different body"
                )));
            }
            return Ok(());
        }
        let tmp = dir.join(format!(".{}.tmp", file_name(name)));
        fs::write(&tmp, bytes.as_bytes()).map_err(|e| err(format!("write: {e}")))?;
        fs::rename(&tmp, &path).map_err(|e| err(format!("rename: {e}")))?;
        Ok(())
    }

    /// `get_named(kind, name)` — the body deposited under `name`.
    pub fn get_named(&self, kind: &str, name: &str) -> Result<Option<Json>, ExperimentError> {
        let path = self.root.join("named").join(kind).join(file_name(name));
        if !path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|e| err(format!("read: {e}")))?;
        let row = parse_json(&text).map_err(|e| err(format!("parse: {e:?}")))?;
        Ok(row.get("body").cloned())
    }

    /// The experiment a run id belongs to (reverse index lookup).
    pub fn experiment_for_run(&self, run_id: &str) -> Result<Option<String>, ExperimentError> {
        let idx = self.read_index()?;
        Ok(idx.iter().find_map(|(eid, e)| {
            if e.run_id.as_deref() == Some(run_id) {
                Some(eid.clone())
            } else {
                None
            }
        }))
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("index").join("experiment.json")
    }

    fn read_index(&self) -> Result<BTreeMap<String, ExperimentIndexEntry>, ExperimentError> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = fs::read_to_string(&path).map_err(|e| err(format!("index read: {e}")))?;
        let j = parse_json(&text).map_err(|e| err(format!("index parse: {e:?}")))?;
        let mut out = BTreeMap::new();
        if let Json::Obj(m) = j {
            for (eid, v) in m {
                let plan_id = v.get("plan_id").and_then(Json::as_str).map(str::to_string);
                let run_id = v.get("run_id").and_then(Json::as_str).map(str::to_string);
                out.insert(eid, ExperimentIndexEntry { plan_id, run_id });
            }
        }
        Ok(out)
    }

    fn write_index(
        &self,
        idx: &BTreeMap<String, ExperimentIndexEntry>,
    ) -> Result<(), ExperimentError> {
        let mut m = BTreeMap::new();
        for (eid, e) in idx {
            let mut row = BTreeMap::new();
            if let Some(p) = &e.plan_id {
                row.insert("plan_id".to_string(), Json::str(p));
            }
            if let Some(r) = &e.run_id {
                row.insert("run_id".to_string(), Json::str(r));
            }
            m.insert(eid.clone(), Json::Obj(row));
        }
        let path = self.index_path();
        let tmp = self.root.join("index").join(".experiment.json.tmp");
        fs::write(&tmp, Json::Obj(m).to_canonical_string().as_bytes())
            .map_err(|e| err(format!("index write: {e}")))?;
        fs::rename(&tmp, &path).map_err(|e| err(format!("index rename: {e}")))?;
        Ok(())
    }
}
