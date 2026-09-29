//! S4.15 hh-bench tests — the C1 measurement-depth legs (§5h.4;
//! AC-R-2.9.4-9/-10; ADR-0142 D3; ADR-0005):
//!
//! - `declare()` emits the full `AdapterDeclaration` (suite family,
//!   submission kind, isolations, export formats, hosting surfaces,
//!   parity state);
//! - `parity = none` admits `product`-granularity runs only (I-5);
//! - hosted surfaces are declared — an undeclared surface refuses
//!   `HostingSurfaceUnsupported`, never a silent native downgrade;
//! - stratum B (`suite.swe_fresh`) is headline-admissible only with its
//!   committed `SuiteValidityRecord` audit facts; G (`suite.harbor_index`)
//!   is the provisional smoke suite;
//! - `export(harbor_trial_dir)` lowers with a `LoweringLossReport` naming
//!   every dropped member;
//! - `import_result` lifts a `harness_bench_result/1` foreign row as a
//!   `product`-granularity `import`/`unverified` row (AC-R-2.9.4-10);
//! - a hosted run drives the same `TaskRecord`/`expose`/`grade` lifecycle
//!   as native (AC-R-2.9.4-9).

use std::io::Write;
use std::process::{Command, Stdio};

use hh_bench::adapter::{AdapterError, BenchmarkAdapter, ExportFormat, HostingSurface};
use hh_bench::adapters::FixtureAdapter;
use hh_bench::benchset::{Benchset, BenchsetError};
use hh_bench::foreign::{export, import_result, ForeignError};
use hh_bench::grade::GradeRequest;
use hh_ontology::lab::VerifierIsolation;
use hh_ontology::participant::Granularity;
use hh_wire::Json;

const BIN: &str = env!("CARGO_BIN_EXE_hh-bench-adapter");

/// One adapter RPC against the out-of-process binary (the reference
/// transport — identical bytes either side of the process boundary).
fn adapter_rpc(req: Json) -> (i32, Json) {
    let mut child = Command::new(BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn hh-bench-adapter");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(req.to_canonical_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        hh_wire::parse(&String::from_utf8_lossy(&out.stdout)).expect("response not canonical JSON"),
    )
}

fn tmp(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "hh-bench-s415-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Recursively copy the corpus so a member can be ablated.
fn copy_corpus(dst: &std::path::Path) {
    fn cp(src: &std::path::Path, dst: &std::path::Path) {
        std::fs::create_dir_all(dst).unwrap();
        for e in std::fs::read_dir(src).unwrap() {
            let e = e.unwrap();
            let d = dst.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                cp(&e.path(), &d);
            } else {
                std::fs::copy(e.path(), &d).unwrap();
            }
        }
    }
    cp(&hh_bench::benchset::default_dir(), dst);
}

/// The `declare()` record carries every C1 member — surfaces, formats,
/// parity — per adapter, with the declaration's own spellings.
#[test]
fn declarations_carry_the_c1_members() {
    let a = FixtureAdapter::adapter_a();
    let d = a.declare();
    assert_eq!(d.adapter_id, "adapter_a");
    assert!(d.parity.is_some(), "stratum A has a parity ref");
    assert!(d.export_formats.contains(&ExportFormat::HarborTrialDir));
    assert!(d
        .hosting_surfaces_supported
        .contains(&HostingSurface::ContainerInstalled));
    let j = d.to_json();
    assert_eq!(
        j.get("schema").and_then(Json::as_str),
        Some("adapter_declaration/1")
    );

    // Stratum B — the fresh-pool adapter hosts participants over all
    // three surfaces; its parity ref exists (the gated suite declares one).
    let b = FixtureAdapter::adapter_b();
    let db = b.declare();
    assert!(db.parity.is_some());
    assert!(db
        .hosting_surfaces_supported
        .contains(&HostingSurface::SessionAbi));

    // Stratum G — `parity = none`, native-participant surface only: a
    // hosted run over an unvalidated smoke adapter refuses.
    let g = FixtureAdapter::adapter_g();
    let dg = g.declare();
    assert!(dg.parity.is_none(), "G smoke declares parity = none");
    assert_eq!(
        dg.hosting_surfaces_supported,
        [HostingSurface::NativeParticipant].into_iter().collect()
    );
    // `parity` spells "none" on the record — never an absent member.
    assert_eq!(
        dg.to_json().get("parity").and_then(Json::as_str),
        Some("none")
    );
}

/// I-5 — `parity = none` admits `product`-granularity runs only; finer
/// granularities refuse `ParityAbsentProductOnly`.
#[test]
fn no_parity_admits_product_only() {
    let g = FixtureAdapter::adapter_g().declare();
    assert!(g.admits_granularity(Granularity::ProductLevel).is_ok());
    for finer in [Granularity::ConfigurationLevel, Granularity::ComponentLevel] {
        assert!(matches!(
            g.admits_granularity(finer),
            Err(AdapterError::ParityAbsentProductOnly { .. })
        ));
    }
    // A parity-carrying adapter imposes no granularity restriction.
    let a = FixtureAdapter::adapter_a().declare();
    assert!(a.admits_granularity(Granularity::ComponentLevel).is_ok());
}

/// The participant-surface gate — a hosted run over an undeclared
/// surface refuses, never silently downgrades to native.
#[test]
fn undeclared_hosted_surface_refuses() {
    for id in ["adapter_c", "adapter_d", "adapter_e", "adapter_g"] {
        let d = FixtureAdapter::by_id(id).unwrap().declare();
        assert_eq!(
            d.admits_surface(HostingSurface::ContainerInstalled),
            Err(AdapterError::HostingSurfaceUnsupported {
                surface: "container_installed".into()
            }),
            "{id} must refuse container_installed"
        );
    }
    // The declared surfaces admit.
    let a = FixtureAdapter::adapter_a().declare();
    for s in [
        HostingSurface::NativeParticipant,
        HostingSurface::ContainerInstalled,
        HostingSurface::SessionAbi,
    ] {
        assert!(a.admits_surface(s).is_ok());
    }
}

/// An undeclared export format refuses `ExportFormatUnsupported` —
/// `export` never silently lowers.
#[test]
fn export_refuses_undeclared_format() {
    // adapter_b declares `adapter_export` only.
    let b = FixtureAdapter::adapter_b();
    let decl = b.declare();
    assert!(decl
        .admits_export_format(ExportFormat::AdapterExport)
        .is_ok());
    assert_eq!(
        decl.admits_export_format(ExportFormat::HarborTrialDir),
        Err(AdapterError::ExportFormatUnsupported {
            format: "harbor_trial_dir".into()
        })
    );
}

/// Stratum-B headline gating: the committed corpus's `suite.swe_fresh`
/// carries a dated audit (admissible); the same corpus with B's `audit`
/// block removed refuses at load — a suite whose `SuiteValidityRecord`
/// does not exist cannot land; an undated audit (`audited_at = null`)
/// loads but never headlines.
#[test]
fn stratum_b_validity_gate() {
    let bs = Benchset::load_default().unwrap();
    let b = bs.suite("b").unwrap();
    assert!(b.headline_admissible(), "committed B audit is dated");
    // The flawed member is recorded, never dropped.
    assert_eq!(
        b.manifest.validity.flawed_task_ids.len(),
        1,
        "the audit names the flawed task"
    );
    assert!(b
        .manifest
        .tasks
        .contains(&b.manifest.validity.flawed_task_ids[0]));

    // No `audit` block → the SuiteValidityRecord does not exist → the
    // corpus refuses at load (B lands *once the record exists*).
    let dir = tmp("b-no-audit");
    copy_corpus(&dir);
    let suite_path = dir.join("suite.swe_fresh").join("suite.json");
    let j = hh_wire::parse(&std::fs::read_to_string(&suite_path).unwrap()).unwrap();
    let Json::Obj(mut m) = j else {
        panic!("suite.json is an object")
    };
    m.remove("audit");
    std::fs::write(&suite_path, Json::Obj(m).to_canonical_string()).unwrap();
    assert!(matches!(
        Benchset::load(&dir),
        Err(BenchsetError::Malformed { .. })
    ));

    // `audited_at = null` — the record exists but the audit is undated:
    // the suite loads, `headline_admissible` stays false.
    let dir = tmp("b-null-audit");
    copy_corpus(&dir);
    let suite_path = dir.join("suite.swe_fresh").join("suite.json");
    let j = hh_wire::parse(&std::fs::read_to_string(&suite_path).unwrap()).unwrap();
    let Json::Obj(mut m) = j else {
        panic!("suite.json is an object")
    };
    let Some(Json::Obj(am)) = m.get_mut("audit") else {
        panic!("audit block is an object")
    };
    am.insert("audited_at".into(), Json::Null);
    std::fs::write(&suite_path, Json::Obj(m).to_canonical_string()).unwrap();
    let bs = Benchset::load(&dir).expect("an undated audit still loads");
    assert!(!bs.suite("b").unwrap().headline_admissible());
}

/// AC-R-2.9.4-9 — hosted participants over the same adapter consume the
/// same `TaskRecord` and `expose` output and grade through the same
/// kernel-invoked verifier; the admission gate is the declaration's
/// `hosting_surfaces_supported`.
#[test]
fn hosted_and_native_share_the_lifecycle() {
    let bs = Benchset::load_default().unwrap();
    let suite = bs.suite("a").unwrap();
    let adapter = &suite.adapter;
    let task = suite.task_named("a-hello").unwrap();
    let decl = adapter.declare();

    let run_leg = |tag: &str, surface: HostingSurface| {
        // Run admission: the participant's surface must be declared.
        decl.admits_surface(surface).unwrap();
        let root = tmp(tag);
        let participant = adapter
            .materialize(&task.record().task_id, root.to_str().unwrap(), false)
            .unwrap();
        let _verifier = adapter
            .materialize(&task.record().task_id, root.to_str().unwrap(), true)
            .unwrap();
        let exposed = adapter.expose(&participant, "profile:ref").unwrap();
        // The participant's only write path — the environment.
        std::fs::write(
            std::path::Path::new(&participant.root).join("submission.json"),
            task.expected(),
        )
        .unwrap();
        let sub = adapter.collect_submission(&participant).unwrap();
        let res = adapter
            .grade(&GradeRequest {
                task: task.record().clone(),
                submission: sub,
                isolation: VerifierIsolation::Separate,
            })
            .unwrap();
        (exposed, res.reward_ppm)
    };

    // The native leg and the hosted (container-installed) leg see the
    // same visible surface and the same graded reward.
    let (native_exposed, native_reward) = run_leg("native", HostingSurface::NativeParticipant);
    let (hosted_exposed, hosted_reward) = run_leg("hosted", HostingSurface::ContainerInstalled);
    assert_eq!(native_exposed.task_id, hosted_exposed.task_id);
    assert_eq!(native_exposed.instruction, hosted_exposed.instruction);
    assert_eq!(native_exposed.attachments, hosted_exposed.attachments);
    assert_eq!(native_reward, hosted_reward);
    assert_eq!(native_reward, 1_000_000);
}

/// AC-R-2.9.4-10 — a native run exported to `harbor_trial_dir` and a
/// foreign `harness_bench_result` import round-trip `task_success`:
/// the exported artifact + loss report, then the imported row at
/// `product` granularity, `origin = import`, `authority = unverified`.
#[test]
fn foreign_export_and_harness_bench_import_round_trip() {
    let bs = Benchset::load_default().unwrap();
    let suite = bs.suite("a").unwrap();
    let adapter = &suite.adapter;
    let task = suite.task_named("a-hello").unwrap();
    let root = tmp("foreign");

    // A native leg: materialize → expose → submit → grade.
    let participant = adapter
        .materialize(&task.record().task_id, root.to_str().unwrap(), false)
        .unwrap();
    let _verifier = adapter
        .materialize(&task.record().task_id, root.to_str().unwrap(), true)
        .unwrap();
    adapter.expose(&participant, "profile:ref").unwrap();
    std::fs::write(
        std::path::Path::new(&participant.root).join("submission.json"),
        task.expected(),
    )
    .unwrap();
    let sub = adapter.collect_submission(&participant).unwrap();
    let res = adapter
        .grade(&GradeRequest {
            task: task.record().clone(),
            submission: sub.clone(),
            isolation: VerifierIsolation::Separate,
        })
        .unwrap();
    let verdict = Json::obj([
        ("reward_ppm", Json::Int(res.reward_ppm)),
        ("detector", Json::str("deterministic")),
    ]);

    // Export — the harbor trial directory lowers the run; the loss
    // report names every member with no foreign slot.
    let (artifact, loss) = export(
        adapter,
        ExportFormat::HarborTrialDir,
        task.record(),
        &sub,
        &verdict,
        res.reward_ppm,
    )
    .unwrap();
    assert!(!loss.lossless);
    let fields: Vec<&str> = loss.entries.iter().map(|e| e.field.as_str()).collect();
    assert!(fields.contains(&"submission.submission_id"));
    assert!(fields.contains(&"submission.artifact_refs"));
    assert!(fields.contains(&"task.provenance"));
    assert!(fields.contains(&"verdict"));
    assert_eq!(
        artifact
            .body
            .get("verifier_result")
            .and_then(|v| v.get("reward_ppm"))
            .and_then(Json::as_int),
        Some(1_000_000)
    );

    // Import — the external plane's result row. The native run's
    // `task_success` equals the imported row's (the round-trip arm).
    let foreign_row = Json::obj([
        ("schema", Json::str("harness_bench_result/1")),
        ("suite", Json::str("harbor_index")),
        ("task", Json::str(task.record().foreign.name.clone())),
        ("run_id", Json::str("hb-trial-1")),
        ("reward_ppm", Json::Int(res.reward_ppm)),
        (
            "observability_level",
            Json::Arr(vec![Json::str("end_state")]),
        ),
        // A foreign member with no typed home — preserved verbatim.
        ("agent_extra", Json::obj([("harness", Json::str("hb/2.1"))])),
    ]);
    let (imported, iloss) = import_result(&foreign_row).unwrap();
    assert_eq!(imported.reward_ppm, res.reward_ppm);
    assert_eq!(imported.granularity, Granularity::ProductLevel);
    assert_eq!(imported.observability_level, vec!["end_state"]);
    // The unmapped member is preserved *and* named on the loss report.
    assert!(imported.foreign_preserved.contains_key("agent_extra"));
    assert_eq!(iloss.entries.len(), 1);
    assert_eq!(iloss.entries[0].field, "agent_extra");
    let row = imported.to_json();
    assert_eq!(row.get("origin").and_then(Json::as_str), Some("import"));
    assert_eq!(
        row.get("authority").and_then(Json::as_str),
        Some("unverified")
    );
}

/// Import refusals are typed — a foreign row malformed or of an unknown
/// schema never coerces.
#[test]
fn import_refuses_typed() {
    assert_eq!(
        import_result(&Json::obj([("schema", Json::str("other/1"))])),
        Err(ForeignError::UnknownSchema {
            schema: "other/1".into()
        })
    );
    assert!(matches!(
        import_result(&Json::obj([(
            "schema",
            Json::str("harness_bench_result/1")
        )])),
        Err(ForeignError::Malformed { .. })
    ));
    // `reward_ppm` outside [0, 1e6] refuses — never clamps.
    assert!(matches!(
        import_result(&Json::obj([
            ("schema", Json::str("harness_bench_result/1")),
            ("suite", Json::str("s")),
            ("task", Json::str("t")),
            ("run_id", Json::str("r")),
            ("reward_ppm", Json::Int(2_000_000)),
        ])),
        Err(ForeignError::Malformed { .. })
    ));
}

/// The smoke suite runs at `product` granularity through the normal
/// lifecycle — its declaration restricts granularity, never the task.
#[test]
fn g_smoke_suite_runs_product() {
    let bs = Benchset::load_default().unwrap();
    let suite = bs.suite("g").unwrap();
    let adapter = &suite.adapter;
    let decl = adapter.declare();
    decl.admits_granularity(Granularity::ProductLevel).unwrap();
    decl.admits_surface(HostingSurface::NativeParticipant)
        .unwrap();
    let task = suite.task_named("g-smoke-1").unwrap();
    let root = tmp("g-smoke");
    let participant = adapter
        .materialize(&task.record().task_id, root.to_str().unwrap(), false)
        .unwrap();
    adapter.expose(&participant, "profile:ref").unwrap();
    std::fs::write(
        std::path::Path::new(&participant.root).join("submission.json"),
        task.expected(),
    )
    .unwrap();
    let sub = adapter.collect_submission(&participant).unwrap();
    let res = adapter
        .grade(&GradeRequest {
            task: task.record().clone(),
            submission: sub,
            isolation: VerifierIsolation::Separate,
        })
        .unwrap();
    assert_eq!(res.reward_ppm, 1_000_000);
}

/// The transport — `declare` and `export` ride the out-of-process binary
/// identically: `declare` emits the full `adapter_declaration/1`;
/// `export{format = harbor_trial_dir}` returns the artifact, its content
/// ref and the loss report in one response.
#[test]
fn declare_and_export_over_the_binary() {
    let (code, resp) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_g")),
        ("op", Json::str("declare")),
    ]));
    assert_eq!(code, 0);
    let decl = resp.get("declaration").unwrap();
    assert_eq!(
        decl.get("parity").and_then(Json::as_str),
        Some("none"),
        "the smoke adapter declares parity = none out of process"
    );

    // A native leg through the binary, then `export` to
    // harbor_trial_dir — the loss report lands beside the artifact. The
    // binary serves the adapter's own fixture suite — `discover` names
    // the task ids it admits.
    let (code, resp) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_a")),
        ("op", Json::str("discover")),
    ]));
    assert_eq!(code, 0);
    let Some(Json::Arr(ids)) = resp.get("task_ids") else {
        panic!("discover returns task_ids[]")
    };
    let tid = ids[0].as_str().unwrap().to_string();
    let root = tmp("bin-export");
    let (code, resp) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_a")),
        ("op", Json::str("materialize")),
        ("task_id", Json::str(&tid)),
        ("root", Json::str(root.to_str().unwrap())),
        ("verifier", Json::Bool(false)),
    ]));
    assert_eq!(code, 0, "materialize out of process");
    let handle = resp.get("handle").unwrap().clone();
    std::fs::write(
        std::path::Path::new(handle.get("root").and_then(Json::as_str).unwrap())
            .join("submission.json"),
        "{\"answer\":\"hello\"}",
    )
    .unwrap();
    let (code, resp) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_a")),
        ("op", Json::str("export")),
        ("format", Json::str("harbor_trial_dir")),
        ("handle", handle),
        ("reward_ppm", Json::Int(1_000_000)),
    ]));
    assert_eq!(code, 0, "export out of process");
    assert_eq!(
        resp.get("artifact")
            .and_then(|a| a.get("format"))
            .and_then(Json::as_str),
        Some("harbor_trial_dir")
    );
    let loss = resp.get("loss_report").unwrap();
    assert_eq!(loss.get("lossless"), Some(&Json::Bool(false)));
    let Some(Json::Arr(entries)) = loss.get("entries") else {
        panic!("loss_report.entries is an array")
    };
    assert!(entries
        .iter()
        .any(|e| e.get("field").and_then(Json::as_str) == Some("submission.submission_id")));

    // An undeclared format refuses over the wire, typed.
    let (code, resp) = adapter_rpc(Json::obj([
        ("adapter", Json::str("adapter_b")),
        ("op", Json::str("export")),
        ("format", Json::str("harbor_trial_dir")),
        (
            "handle",
            Json::obj([
                ("handle_id", Json::str("h")),
                ("task_id", Json::str("t")),
                ("family", Json::str("coding_terminal")),
                ("root", Json::str("/tmp/nonexistent-s415")),
                ("network_mode", Json::str("none")),
                ("surface", Json::str("search")),
            ]),
        ),
    ]));
    assert_eq!(code, 2);
    let kind = resp.get("kind").and_then(Json::as_str).unwrap_or("");
    assert!(
        kind.starts_with("ExportFormatUnsupported"),
        "unexpected refusal kind {kind}"
    );
}
