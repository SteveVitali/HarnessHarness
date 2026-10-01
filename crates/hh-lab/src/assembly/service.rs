//! `service` (§6.1 §2.1; R-2.10.1; ADR-0147…0150) — the assembly service: a
//! **semantics-free** orchestration layer over the kernel's one resolver —
//! `desugar → compose → resolve(snapshot) → validate_assembly` (`resolve`
//! seals). Every `SealedDefinition`, `ValidationReport` and `HirDiff` is
//! byte-identical to the kernel's output on the same inputs (S-3); the
//! service never pins a reference itself (S-2), never decides a widening gate
//! (S-6 forwards to `RegistryStore::publish`), and holds no state that is not
//! a registry record or a `derivation_key`-keyed cache.

use std::collections::HashSet;

use hh_assembly::catalog::ClassCatalog;
use hh_assembly::compose::compose;
use hh_assembly::diagnostics::{
    detail_text, diagnostic_json, report_json, AssemblyDiagnostic, Code, ReportStatus, Severity,
    Stage, StageOutcome, ValidationReport,
};
use hh_assembly::resolve::{resolve, ResolveEnv};
use hh_assembly::validate::{validate_assembly, Subject};
use hh_hir::document::SealedDefinition;
use hh_identity::names::ResolveMode;
use hh_identity::sameness::SamenessLevel;
use hh_provenance::{Origin, ProvenanceRecord};
use hh_registry::store::RegistryStore;
use hh_wire::json::Json;

use crate::assembly::desugar::{apply_experiment, derivation_key, desugar, lab_ext, LAB_EXT_KEY};
use crate::assembly::diff_view::{diff_json, project, AssemblyDiff};
use crate::assembly::drift::{adopt_admissible, drift_diagnostic, project_drift, DriftReport};
use crate::assembly::plan::{defaulted_paths, fragment_paths, layer_map, AssemblyPlan, PlanExit};
use crate::assembly::source::AssemblySource;

/// The assembler version entering `derivation_key` (`assembler.version`).
pub const ASSEMBLER_VERSION: &str = "hh-lab/assembly-service/1";

/// `assemble`'s mode (S-1: identical `report`, `plan`, `identity`,
/// `snapshot`; `seal` additionally retains the sealed document).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssembleMode {
    /// Full pipeline; the sealed document is computed but not retained.
    Plan,
    /// Full pipeline; the sealed document is retained.
    Seal,
}

/// The `assemble` result (§6.1 §2.1).
#[derive(Debug)]
pub struct AssemblyResult {
    /// `ok | error` (`error ⇔ report.status = fail` — V-3).
    pub status: &'static str,
    /// The sealed definition (seal mode, `status = ok` only).
    pub sealed: Option<SealedDefinition>,
    /// `{semantic_id, version_id}` (both modes — S-1).
    pub identity: Option<Json>,
    /// The complete report (desugar/compose/resolve diagnostics merged into
    /// `validate_assembly`'s output — never fail-fast).
    pub report: ValidationReport,
    /// The plan (identical across modes — S-1).
    pub plan: AssemblyPlan,
    /// The registry snapshot the resolve was confined to.
    pub snapshot: String,
    /// `H(idp ∥ "assemble" ∥ canonical(layers ∥ snapshot ∥ catalog ∥ assembler))`.
    pub derivation_key: String,
}

impl AssemblyResult {
    /// The canonical JSON wire form (`lab.assembly.assemble`).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("status", Json::str(self.status)),
            (
                "sealed",
                self.sealed
                    .as_ref()
                    .map(hh_hir::wire::sealed_definition_json)
                    .unwrap_or(Json::Null),
            ),
            ("identity", self.identity.clone().unwrap_or(Json::Null)),
            ("report", report_json(&self.report)),
            ("plan", self.plan.to_json()),
            ("snapshot", Json::str(self.snapshot.clone())),
            ("derivation_key", Json::str(self.derivation_key.clone())),
        ])
    }
}

/// `apply`'s publish parameters (`{namespace, name, label?, supersedes?}` —
/// the ADR-0037 `publish` arguments, verbatim).
#[derive(Debug, Clone)]
pub struct PublishSpec {
    /// The registry namespace.
    pub namespace: String,
    /// The name inside it.
    pub name: String,
    /// An optional version label.
    pub label: Option<String>,
    /// The superseded `version_id` (same name's line — the registry checks).
    pub supersedes: Option<String>,
}

/// The `apply` result.
#[derive(Debug)]
pub struct ApplyResult {
    /// The assemble result (seal mode).
    pub result: AssemblyResult,
    /// The published `NameHistoryEntry`-shaped record (the registry's
    /// `VersionedRef` for the published name).
    pub published: Option<Json>,
    /// `true` when `plan.exit = error` refused the apply before any write
    /// (S-5 — the publish never happens).
    pub refused: bool,
}

impl ApplyResult {
    /// The canonical JSON wire form (`lab.assembly.apply`).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("result", self.result.to_json()),
            ("published", self.published.clone().unwrap_or(Json::Null)),
            ("refused", Json::Bool(self.refused)),
        ])
    }
}

/// A `validate_batch` point — a sealed definition or an authored source.
pub enum BatchPoint {
    /// An already-sealed definition (validated as a `Sealed` subject).
    Sealed(Box<SealedDefinition>),
    /// An authored source (desugared + resolved, then validated).
    Source(Box<AssemblySource>),
}

/// The assembly service. Borrows the registry mutably (resolve may cut a
/// snapshot) — the store is the only state (a crash loses only the
/// derivation-key cache).
pub struct AssemblyService<'a> {
    /// The one registry store (CC7).
    pub registry: &'a mut RegistryStore,
    /// The class catalog.
    pub catalog: &'a dyn ClassCatalog,
    /// The kernel provenance — mints diagnostic `detail` leaves.
    pub kernel: ProvenanceRecord,
    /// The registrar provenance — snapshot cuts, publish, hosted scaffold.
    pub registrar: ProvenanceRecord,
    /// `resolved_at` stamp (ms) for resolves.
    pub resolved_at: u64,
}

fn sdiag(
    code: Code,
    stage: Stage,
    path: &str,
    subject: &str,
    detail: &str,
    remedy: &str,
    kernel: &ProvenanceRecord,
) -> AssemblyDiagnostic {
    AssemblyDiagnostic {
        code,
        class: None,
        severity: Severity::Error,
        path: path.to_string(),
        source_layer: None,
        subject: subject.to_string(),
        stage,
        detail: detail_text(detail, kernel),
        remedy: remedy.to_string(),
        owner_adr: "ADR-0147".into(),
    }
}

/// A failed pipeline's report — every diagnostic gathered so far, `fail`.
fn failed_report(diags: Vec<AssemblyDiagnostic>, stages: Vec<StageOutcome>) -> ValidationReport {
    let mut r = ValidationReport {
        status: ReportStatus::Fail,
        diagnostics: diags,
        derived: Default::default(),
        stages,
    };
    r.finalize();
    r
}

impl<'a> AssemblyService<'a> {
    /// `assemble(source, snapshot, mode)` (§6.1 §2.1) — sequences
    /// `desugar → compose → resolve → validate_assembly`; `seal` mode
    /// retains the sealed document. `snapshot` is a `registry_snapshot_id`
    /// or `"head"` (a fresh snapshot, cut through the registry's own verb).
    pub fn assemble(
        &mut self,
        source: &AssemblySource,
        snapshot: Option<&str>,
        mode: AssembleMode,
    ) -> AssemblyResult {
        let kernel = self.kernel.clone();
        let mut diags: Vec<AssemblyDiagnostic> = Vec::new();

        // The snapshot — `"head"` (None) cuts a fresh one through the
        // registry's own verb so the id is known even when resolve fails.
        let snapshot_id = match snapshot {
            Some(s) => s.to_string(),
            None => match self.registry.snapshot(&self.registrar) {
                Ok(s) => s.snapshot_id,
                Err(e) => {
                    diags.push(sdiag(
                        Code::IntUncodedRejection,
                        Stage::Resolve,
                        "/snapshot",
                        "head",
                        &format!("snapshot cut failed: {e:?}"),
                        "the registry's snapshot verb owns the failure — this is a defect",
                        &kernel,
                    ));
                    String::new()
                }
            },
        };

        // desugar (D1–D5).
        let desugared = desugar(
            source,
            self.registry,
            Some(snapshot_id.as_str()).filter(|s| !s.is_empty()),
            self.catalog,
            &kernel,
            &self.registrar,
        );
        let dkey = derivation_key(
            &desugared.layers,
            desugared.experiment.as_ref(),
            &snapshot_id,
            self.catalog,
            ASSEMBLER_VERSION,
        );
        diags.extend(desugared.diags.iter().cloned());

        // The plan skeleton — authored/defaulted paths and layer_map are
        // computable before compose finishes (S-1: identical across modes).
        let mut authored: Vec<String> = desugared
            .layers
            .iter()
            .flat_map(|l| fragment_paths(&l.fragment))
            .collect();
        authored.sort();
        authored.dedup();
        let lmap = layer_map(&desugared);

        let (mut doc, experiment) = (desugared.doc, desugared.experiment);
        let desugared_layers = desugared.layers;

        // compose.
        let mut composed = match compose(&desugared_layers, &kernel) {
            Ok(a) => a,
            Err(cs) => {
                diags.extend(cs);
                let mut plan = AssemblyPlan::error(diags.clone());
                plan.authored_paths = authored;
                plan.layer_map = lmap;
                return AssemblyResult {
                    status: "error",
                    sealed: None,
                    identity: None,
                    report: failed_report(diags, Vec::new()),
                    plan,
                    snapshot: snapshot_id,
                    derivation_key: dkey,
                };
            }
        };

        // The experiment layer's effect (D2) applies post-compose.
        if let Some(exp) = &experiment {
            apply_experiment(&mut composed, exp, &kernel, &mut diags);
        }
        // The desugared layer stack + derivation key ride the sealed
        // definition's `ext` (S-7 explain material — non-semantic).
        composed.ext.insert(
            LAB_EXT_KEY.into(),
            lab_ext(&desugared_layers, experiment.as_ref(), &dkey),
        );
        let def_paths = defaulted_paths(&composed, &authored);

        doc.assembly = Some(composed.to_json());

        // resolve(snapshot) — pins + seals (the kernel's one resolver).
        let mut env = ResolveEnv {
            registry: self.registry,
            catalog: self.catalog,
            snapshot_id: Some(snapshot_id.clone()),
            mode: ResolveMode::Audit,
            registrar: self.registrar.clone(),
            resolved_at: self.resolved_at,
            notices: None,
        };
        // Non-error resolve diagnostics (info/warning notices — e.g.
        // `C-REF-5 DenyListNoop`) surface in the report (V-3).
        let mut notices: Vec<AssemblyDiagnostic> = Vec::new();
        env.notices = Some(&mut notices);
        let (sealed, subject_diags_stage) = match resolve(&doc, &mut env) {
            Ok(s) => (Some(s), None),
            Err(rs) => (None, Some(rs)),
        };

        // validate_assembly — the sealed subject when resolve succeeded (the
        // sealed form is what stages 2–7 must see); the authored scaffold
        // otherwise, so the report stays complete (V-3).
        let mut report = match &sealed {
            Some(s) => validate_assembly(Subject::Sealed(s), self.catalog, None, &kernel),
            None => validate_assembly(Subject::Authored(&doc), self.catalog, None, &kernel),
        };
        if let Some(rs) = subject_diags_stage {
            report.diagnostics.extend(rs);
        }
        report.diagnostics.extend(notices.iter().cloned());
        report.diagnostics.extend(diags.iter().cloned());
        report.finalize();

        let status = if report.status == ReportStatus::Fail {
            "error"
        } else {
            "ok"
        };
        let identity = sealed.as_ref().map(|s| {
            Json::obj([
                (
                    "semantic_id",
                    Json::str(s.definition_ref.semantic_id.clone()),
                ),
                ("version_id", Json::str(s.definition_ref.version_id.clone())),
            ])
        });

        let mut plan = AssemblyPlan {
            diff: None,
            defaulted_paths: def_paths,
            authored_paths: authored,
            layer_map: lmap,
            drift: None,
            exit: if status == "error" {
                PlanExit::Error
            } else {
                PlanExit::Changes
            },
            sameness: None,
            diagnostics: diags,
        };
        // `exit = no_changes` when the assembled definition equals its base.
        if plan.exit == PlanExit::Changes {
            if let (Some(base), Some(sealed)) = (&source.base, &sealed) {
                if let Ok(b) = crate::assembly::desugar::base_definition(
                    base,
                    self.registry,
                    Some(snapshot_id.as_str()),
                ) {
                    let (ops, class) = hh_hir::diff::ops_between(&b.document, &sealed.document);
                    let va = versioned_of(&b, &self.kernel);
                    let vb = versioned_of(sealed, &self.kernel);
                    let same = hh_identity::sameness::sameness(
                        &va,
                        &vb,
                        true,
                        Some(&hh_identity::sameness::DiffClassification {
                            semantic_ops_nonempty: class.semantic_ops > 0,
                            authority_delta: delta_from_authority(&class.authority_delta),
                            budget_delta: delta_from(&class.budget_delta),
                            dialect_change: b.document.hir_version != sealed.document.hir_version,
                        }),
                    );
                    plan.sameness = Some(same.level);
                    if same.level == SamenessLevel::L0 || ops.is_empty() {
                        plan.exit = PlanExit::NoChanges;
                    } else {
                        plan.diff = Some(project(ops, class, Some(same.level)));
                    }
                }
            }
        }

        AssemblyResult {
            status,
            sealed: match mode {
                AssembleMode::Seal if status == "ok" => sealed,
                _ => None,
            },
            identity,
            report,
            plan,
            snapshot: snapshot_id,
            derivation_key: dkey,
        }
    }

    /// `plan(source, snapshot)` — `assemble(…, plan)`; `plan(a, b)` on two
    /// sealed definitions is `project(diff(a, b))` (T-2: the service diffs
    /// sealed definitions only).
    pub fn plan(&mut self, source: &AssemblySource, snapshot: Option<&str>) -> AssemblyPlan {
        self.assemble(source, snapshot, AssembleMode::Plan).plan
    }

    /// `plan`/`diff` on two sealed definitions (T-2 — authored sources are
    /// diffed by desugaring and resolving both sides under one snapshot;
    /// textual diffs of authored files are never produced).
    pub fn diff_sealed(&self, a: &SealedDefinition, b: &SealedDefinition) -> AssemblyDiff {
        let (ops, class) = hh_hir::diff::ops_between(&a.document, &b.document);
        let sam = self.registry_sameness(a, b);
        project(ops, class, sam.map(|s| s.level))
    }

    /// The registry's sameness verdict between two sealed definitions (by
    /// `version_id` — registry records).
    fn registry_sameness(
        &self,
        a: &SealedDefinition,
        b: &SealedDefinition,
    ) -> Option<hh_identity::sameness::Sameness> {
        let va = &a.definition_ref.version_id;
        let vb = &b.definition_ref.version_id;
        self.registry.sameness(va, vb).ok().or_else(|| {
            // Not registered — classify_pair reading (same_levelage unknown →
            // the semantic_id/lineage rungs decide what they can).
            let class_a = hh_hir::diff::classify_pair(&a.document, &b.document);
            let class_b = hh_hir::diff::classify_pair(&b.document, &a.document);
            let class = hh_identity::sameness::DiffClassification {
                semantic_ops_nonempty: class_a.semantic_ops > 0 || class_b.semantic_ops > 0,
                authority_delta: worse(
                    delta_from_authority(&class_a.authority_delta),
                    delta_from_authority(&class_b.authority_delta),
                ),
                budget_delta: worse(
                    delta_from(&class_a.budget_delta),
                    delta_from(&class_b.budget_delta),
                ),
                dialect_change: a.document.hir_version != b.document.hir_version,
            };
            Some(hh_identity::sameness::sameness(
                &versioned_of(a, &self.kernel),
                &versioned_of(b, &self.kernel),
                false,
                Some(&class),
            ))
        })
    }

    /// `drift(source, snap_old, snap_new)` (T-3) — two resolves, pin-only
    /// projection, `C-REF-6` at the sameness-driven severity. `freeze` is the
    /// default: this call never mutates authored pins.
    pub fn drift(
        &mut self,
        source: &AssemblySource,
        snap_old: &str,
        snap_new: &str,
    ) -> DriftReport {
        let kernel = self.kernel.clone();
        let mut diags = Vec::new();
        let old = self.assemble(source, Some(snap_old), AssembleMode::Seal);
        let new = self.assemble(source, Some(snap_new), AssembleMode::Seal);
        diags.extend(old.report.diagnostics.iter().cloned());
        diags.extend(new.report.diagnostics.iter().cloned());
        match (old.sealed, new.sealed) {
            (Some(a), Some(b)) => {
                let (ops, class) = hh_hir::diff::ops_between(&a.document, &b.document);
                let sam = self.registry_sameness(&a, &b).map(|s| s.level);
                let d = project_drift(ops, class, sam);
                if let Some(level) = sam {
                    let canon = Json::Arr(
                        crate::assembly::diff_view::flatten(&d)
                            .iter()
                            .map(hh_hir::diff::op_json)
                            .collect(),
                    )
                    .to_canonical_string();
                    let diff_ref = hh_identity::idp::idp_id("drift", canon.as_bytes());
                    if let Some(dg) =
                        drift_diagnostic(level, &diff_ref, snap_old, snap_new, &kernel)
                    {
                        diags.push(dg);
                    }
                }
                DriftReport {
                    diff: Some(d),
                    sameness: sam,
                    diagnostics: diags,
                }
            }
            _ => DriftReport {
                diff: None,
                sameness: None,
                diagnostics: diags,
            },
        }
    }

    /// `adopt` — re-resolve the *source* under the new snapshot and produce
    /// the adoption `HirDiff` (T-3: explicit; `derived-from{hypothesis:
    /// "snapshot adoption"}`; `origin = evolution` may never adopt L3).
    /// `freeze` needs no call — it is the absence of `adopt`.
    pub fn adopt(
        &mut self,
        source: &AssemblySource,
        snap_old: &str,
        snap_new: &str,
        caller: &ProvenanceRecord,
    ) -> Result<hh_hir::diff::HirDiff, Vec<AssemblyDiagnostic>> {
        let kernel = self.kernel.clone();
        let old = self.assemble(source, Some(snap_old), AssembleMode::Seal);
        let new = self.assemble(source, Some(snap_new), AssembleMode::Seal);
        let (Some(a), Some(b)) = (old.sealed, new.sealed) else {
            let mut diags = old.report.diagnostics;
            diags.extend(new.report.diagnostics);
            return Err(diags);
        };
        let sam = self.registry_sameness(&a, &b).map(|s| s.level);
        if let Some(level) = sam {
            let evolution = matches!(caller.origin, Origin::Evolution { .. });
            if let Err(e) = adopt_admissible(level, evolution) {
                return Err(vec![sdiag(
                    Code::PlanRefusal,
                    Stage::Resolve,
                    "/snapshot",
                    snap_new,
                    &e,
                    "an L3 adoption needs a human origin with attestation",
                    &kernel,
                )]);
            }
        }
        hh_hir::diff::diff(
            &a.document,
            &b.document,
            caller.clone(),
            hh_hir::diff::DiffDerivation {
                hypothesis: Some(hh_hir::leaves::Text::new(
                    crate::assembly::drift::ADOPT_HYPOTHESIS,
                    "en",
                    caller.clone(),
                )),
                trajectories: Vec::new(),
                candidate_id: None,
            },
        )
        .map_err(|errs| {
            errs.iter()
                .map(|e| {
                    sdiag(
                        hh_assembly::diagnostics::kern_code(e),
                        Stage::Resolve,
                        "/snapshot",
                        snap_new,
                        &format!("{e:?}"),
                        "the adoption diff's gates refused — see the kernel error",
                        &kernel,
                    )
                })
                .collect()
        })
    }

    /// `apply(source, snapshot, publish?)` — `assemble(…, seal)` then the
    /// registry's `publish` (ADR-0037 verbatim — the widening/attestation
    /// gate is the registry's, S-6). `plan.exit = error` refuses before any
    /// write (S-5).
    pub fn apply(
        &mut self,
        source: &AssemblySource,
        snapshot: Option<&str>,
        publish: Option<&PublishSpec>,
    ) -> ApplyResult {
        let kernel = self.kernel.clone();
        let mut result = self.assemble(source, snapshot, AssembleMode::Seal);
        if result.plan.exit == PlanExit::Error {
            return ApplyResult {
                result,
                published: None,
                refused: true,
            };
        }
        let mut published = None;
        if let (Some(spec), Some(sealed)) = (publish, &result.sealed) {
            match self.registry.publish(
                &spec.namespace,
                &spec.name,
                &sealed.definition_ref.version_id,
                spec.label.clone(),
                spec.supersedes.clone(),
                &self.registrar,
            ) {
                Ok(vr) => {
                    published = Some(Json::obj([
                        (
                            "name",
                            vr.name
                                .as_ref()
                                .map(|n| {
                                    Json::obj([
                                        ("namespace", Json::str(n.namespace.clone())),
                                        ("name", Json::str(n.name.clone())),
                                        (
                                            "label",
                                            n.label.clone().map(Json::str).unwrap_or(Json::Null),
                                        ),
                                    ])
                                })
                                .unwrap_or(Json::Null),
                        ),
                        ("version_id", Json::str(vr.version_id.clone())),
                    ]));
                }
                Err(e) => {
                    result.report.diagnostics.push(sdiag(
                        Code::PlanRefusal,
                        Stage::Resolve,
                        "/apply/publish",
                        &format!("{}/{}", spec.namespace, spec.name),
                        &format!("registry publish refused: {e:?}"),
                        "the publish gate (ADR-0037) is the registry's — fix the record or the registrar",
                        &kernel,
                    ));
                    result.report.finalize();
                    result.status = "error";
                    result.plan.exit = PlanExit::Error;
                    result.plan.diagnostics = result.report.diagnostics.clone();
                }
            }
        }
        ApplyResult {
            result,
            published,
            refused: false,
        }
    }

    /// `validate_batch(points, snapshot)` — the pre-spend gate (ADR-0148 D4):
    /// reports are independent per point; identical points (by
    /// `derivation_key`) validate once.
    pub fn validate_batch(
        &mut self,
        points: &[BatchPoint],
        snapshot: Option<&str>,
    ) -> Vec<(String, ValidationReport)> {
        // One snapshot per batch — `"head"` cuts once so identical points
        // (by derivation_key) dedupe (ADR-0148 D4).
        let owned_snap;
        let snap = match snapshot {
            Some(s) => s,
            None => {
                owned_snap = self
                    .registry
                    .snapshot(&self.registrar)
                    .map(|s| s.snapshot_id)
                    .unwrap_or_default();
                owned_snap.as_str()
            }
        };
        let mut seen: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        for p in points {
            match p {
                BatchPoint::Sealed(s) => {
                    let key = format!("sealed:{}", s.definition_ref.version_id);
                    if !seen.insert(key.clone()) {
                        continue;
                    }
                    let r = validate_assembly(Subject::Sealed(s), self.catalog, None, &self.kernel);
                    out.push((key, r));
                }
                BatchPoint::Source(src) => {
                    let r = self.assemble(src, Some(snap), AssembleMode::Plan);
                    if !seen.insert(r.derivation_key.clone()) {
                        continue;
                    }
                    out.push((r.derivation_key, r.report));
                }
            }
        }
        out
    }

    /// `explain` — the desugared layer stack, layer_map, plan and report of a
    /// source (or the recorded `hh.lab/assembly` ext of a sealed definition).
    /// The output is a JSON record, never a narrative (M-2: the pipeline
    /// makes the explanation *checkable*).
    pub fn explain(&mut self, source: &AssemblySource, snapshot: Option<&str>) -> Json {
        let result = self.assemble(source, snapshot, AssembleMode::Plan);
        Json::obj([
            ("derivation_key", Json::str(result.derivation_key.clone())),
            ("snapshot", Json::str(result.snapshot.clone())),
            ("status", Json::str(result.status)),
            ("plan", result.plan.to_json()),
            ("report", report_json(&result.report)),
            (
                "layers",
                result
                    .sealed
                    .as_ref()
                    .and_then(|s| s.document.assembly.as_ref())
                    .and_then(|a| a.get("ext"))
                    .and_then(|e| e.get(LAB_EXT_KEY))
                    .and_then(|x| x.get("layers"))
                    .cloned()
                    .unwrap_or(Json::Null),
            ),
        ])
    }

    /// `explain` over a sealed definition — reads the recorded
    /// `hh.lab/assembly` ext (S-7: the sealed definition carries its layer
    /// stack; a definition assembled outside the service carries none — the
    /// `explain` is then the diff/identity surface only).
    pub fn explain_sealed(&self, sealed: &SealedDefinition) -> Json {
        let ext = sealed
            .document
            .assembly
            .as_ref()
            .and_then(|a| a.get("ext"))
            .and_then(|e| e.get(LAB_EXT_KEY))
            .cloned()
            .unwrap_or(Json::Null);
        Json::obj([
            (
                "identity",
                Json::obj([
                    (
                        "semantic_id",
                        Json::str(sealed.definition_ref.semantic_id.clone()),
                    ),
                    (
                        "version_id",
                        Json::str(sealed.definition_ref.version_id.clone()),
                    ),
                ]),
            ),
            ("assembly_ext", ext),
        ])
    }

    /// `compile` — delegates to the compiler with the registry as the
    /// variant view and the service's catalog/kernel (CC7: the service adds
    /// no compile semantics; a `plan`-mode caller supplies `inputs` whose
    /// `sealed` is the `assemble` output).
    pub fn compile(
        &self,
        inputs: &hh_compiler::compiler::CompileInputs,
        profiles: &dyn hh_compiler::profile::ProfileView,
    ) -> Result<hh_compiler::seal::CompiledBundle, hh_compiler::errors::CompileError> {
        hh_compiler::compiler::compile(inputs, profiles, self.registry, self.catalog, &self.kernel)
    }
}

fn delta_from(d: &hh_hir::diff::Delta) -> hh_identity::sameness::Delta {
    match d {
        hh_hir::diff::Delta::Loosening => hh_identity::sameness::Delta::Loosening,
        hh_hir::diff::Delta::Tightening => hh_identity::sameness::Delta::Tightening,
        hh_hir::diff::Delta::None => hh_identity::sameness::Delta::None,
    }
}

fn delta_from_authority(d: &hh_hir::diff::AuthorityDelta) -> hh_identity::sameness::Delta {
    match d {
        hh_hir::diff::AuthorityDelta::Widening => hh_identity::sameness::Delta::Widening,
        hh_hir::diff::AuthorityDelta::Narrowing => hh_identity::sameness::Delta::Tightening,
        hh_hir::diff::AuthorityDelta::None => hh_identity::sameness::Delta::None,
    }
}

/// `M-1`/`M-2` — the sweep-engine gates as pure predicates over a result:
/// M-1 a refused point is any report with `status = fail`; M-2 the
/// derivation key is the point's identity (identical desugared layers under
/// one snapshot are one point).
pub fn point_refused(r: &AssemblyResult) -> bool {
    r.report.status == ReportStatus::Fail
}

/// The `VersionedRef` a sealed definition presents for sameness (the
/// registry's own view — `RecordKind::SealedDefinition` over the canonical
/// sealed document).
fn versioned_of(
    s: &SealedDefinition,
    kernel: &ProvenanceRecord,
) -> hh_identity::refs::VersionedRef {
    hh_identity::refs::VersionedRef::pinned(
        hh_identity::kinds::RecordKind::SealedDefinition,
        s.definition_ref.version_id.clone(),
        kernel.clone(),
    )
    .with_semantic(s.definition_ref.semantic_id.clone())
}

/// The worse of two deltas (for the symmetric classify_pair reading).
fn worse(
    a: hh_identity::sameness::Delta,
    b: hh_identity::sameness::Delta,
) -> hh_identity::sameness::Delta {
    use hh_identity::sameness::Delta;
    let rank = |d: Delta| match d {
        Delta::Widening | Delta::Loosening => 2,
        Delta::Tightening | Delta::Narrowing => 1,
        Delta::None => 0,
    };
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
}

/// The diagnostic list as canonical JSON (wire helper).
pub fn diagnostics_json(diags: &[AssemblyDiagnostic]) -> Json {
    Json::Arr(diags.iter().map(diagnostic_json).collect())
}

/// A `BTreeMap<String, Json>` of an `AssemblyDiff`'s wire form plus the
/// flattened ops' canonical encodings (for `hh-embed` result shaping).
pub fn diff_wire(d: &AssemblyDiff) -> Json {
    diff_json(d)
}
