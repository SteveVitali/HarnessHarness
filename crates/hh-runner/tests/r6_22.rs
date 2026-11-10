//! R6.22 acceptance battery (manifest 275; ADMIT-CORE a+b; AR-18/ADR-0436):
//! AC-2 the validator truth table (REQ-XC-302 + the REQ-XB-014/REQ-XC-201/
//! REQ-XD-080 alias predicates); AC-3 legal posture (REQ-XC-202); AC-4 the
//! AG9 O-vs-H rule (REQ-XC-214); AC-5 version pins (REQ-XD-081); AC-6 the
//! `container_installed` declaration (REQ-XC-301); AC-7 golden bundle
//! recipes (REQ-XC-303); AC-8 golden Containerfiles + stub-podman argv +
//! digest-only cell refs (REQ-XC-304 + REQ-XC-204 alias); AC-9 ST2
//! verify-only + `n/a{install:<step>}` (REQ-XC-305); AC-10 the ignored
//! live build leg. Offline — no podman, no network, USD $0.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_runner::admission::{
    admission_id, admit_slot_placement, harness_declaration, ovsh_eligible, validate,
    version_is_unpinned, ValidationContext, CLAUDE_CODE, SIDE_CHANNEL_MODEL_ROUTE,
};
use hharness_runner::admission_build::{
    artifact_upload, build_once_guard, build_record, check_cell_image_ref, layer_for_task,
    manifest_ref, package_delta_argv, parse_package_delta, run_build, st2_offline, st2_plan,
    st2_verify, BuildPlan, BundleRecipe, InstallStep, LayerSpec, LockSpec, St2Observation,
};
use hharness_runner::disposition::{AdmissionReason, WithheldReason};
use hharness_runner::facts::{map_slot_facts, SlotFacts};
use hharness_runner::harbor::HARBOR_PIN;
use hharness_runner::json_ext::JsonExt;
use hharness_wire::json::{self, Json};

const PIN: &str = HARBOR_PIN;
const TASK_A: &str =
    "docker.io/alexgshaw/task-base-a@sha256:37bbbbed5fd87cbc8c1b2084b1c48b52170cfe0d9d0ac70f0ebe26ef3d835c9b";
const TASK_B: &str =
    "docker.io/alexgshaw/task-base-b@sha256:b977da3c7aa32912c9718c37e6bc5830f89543921c9427bdaf3f6c8397210114";

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-runner-r622-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn ctx() -> ValidationContext {
    ValidationContext::first_pass()
}

fn load(name: &str) -> Json {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/r6_22")
        .join(name);
    json::parse(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn mini() -> Json {
    load("mini-swe-agent.json")
}
fn pi() -> Json {
    load("pi.json")
}
fn claude() -> Json {
    load("claude-code.json")
}

fn obj_mut(j: &mut Json) -> &mut BTreeMap<String, Json> {
    match j {
        Json::Obj(m) => m,
        _ => panic!("expected object"),
    }
}

/// Deep-set a member path on a Json object.
fn set(j: &mut Json, path: &[&str], v: Json) {
    let mut cur = j;
    for (i, k) in path.iter().enumerate() {
        let m = obj_mut(cur);
        if i == path.len() - 1 {
            m.insert(k.to_string(), v);
            return;
        }
        cur = m.get_mut(*k).unwrap();
    }
}
fn del(j: &mut Json, path: &[&str]) {
    let mut cur = j;
    for k in &path[..path.len() - 1] {
        cur = obj_mut(cur).get_mut(*k).unwrap();
    }
    obj_mut(cur).remove(*path.last().unwrap());
}

fn refuses(rec: &Json, rule: &str) {
    let e = validate(rec, &ctx()).unwrap_err();
    assert_eq!(e.rule, rule, "{e}");
}

// ── AC-2 — the REQ-XC-302 truth table ───────────────────────────────────────

#[test]
fn ac2_complete_records_admit_for_all_three_harnesses() {
    for rec in [mini(), pi(), claude()] {
        validate(&rec, &ctx()).unwrap_or_else(|e| panic!("{e}"));
    }
}

#[test]
fn ac2_each_refusal_fails_its_fixture() {
    // missing AG<n> per role
    for n in 1..=8u8 {
        let mut r = mini();
        del(&mut r, &["gates", &format!("AG{n}")]);
        refuses(&r, "gate_missing");
    }
    let mut r = mini();
    del(&mut r, &["gates", "AG9"]);
    refuses(&r, "gate_missing");
    // a failed gate
    let mut r = mini();
    set(&mut r, &["gates", "AG5", "result"], Json::str("fail"));
    refuses(&r, "gate_failed");
    // floating/@latest/prerelease/unlocked — REQ-XD-081 + XB-014 alias
    for v in [
        "@latest",
        "latest",
        "stable",
        "2.4.6-rc1",
        "^2.4.6",
        "2.4.x",
        "",
    ] {
        let mut r = pi();
        set(&mut r, &["harness", "version"], Json::str(v));
        let e = validate(&r, &ctx()).unwrap_err();
        assert!(
            e.rule == "version_unpinned" || e.rule == "mini_swe_version",
            "{v} → {e}"
        );
    }
    // missing lock / resolution cutoff
    let mut r = mini();
    set(&mut r, &["harness", "freeze_at"], Json::str(""));
    refuses(&r, "lock_missing");
    // missing bundle / layer digest
    let mut r = mini();
    del(&mut r, &["install", "bundle"]);
    refuses(&r, "digest_missing");
    let mut r = mini();
    set(
        &mut r,
        &["install", "bundle", "digest"],
        Json::str("sha256:abc"),
    );
    refuses(&r, "digest_missing");
    let mut r = mini();
    set(&mut r, &["install", "layers"], Json::Obj(BTreeMap::new()));
    refuses(&r, "layer_digest");
    // config template hash
    let mut r = mini();
    del(&mut r, &["config", "template_hash"]);
    refuses(&r, "digest_missing");
    // expected_denied
    let mut r = mini();
    set(&mut r, &["egress", "expected_denied"], Json::Arr(vec![]));
    refuses(&r, "expected_denied");
    // legal posture record
    let mut r = mini();
    del(&mut r, &["harness", "legal"]);
    refuses(&r, "member_missing");
    // A1–A7
    for a in [
        "a1_wire_format",
        "a2_cached_share",
        "a3_trajectory",
        "a4_install",
        "a5_known_issues",
        "a6_priors",
        "a7_maintenance",
    ] {
        let mut r = mini();
        del(&mut r, &["attributes", a]);
        refuses(&r, "member_missing");
    }
    // outcome-blind admission timing
    let mut r = mini();
    set(&mut r, &["admission_timing"], Json::str("post_outcome"));
    refuses(&r, "admission_timing");
    // effort-pin mechanism + value
    let mut r = mini();
    set(&mut r, &["config", "effort", "value"], Json::str("low"));
    refuses(&r, "effort_pin");
    let mut r = mini();
    set(&mut r, &["config", "effort", "mechanism"], Json::str(""));
    refuses(&r, "effort_pin");
    // role
    let mut r = mini();
    set(&mut r, &["harness", "role"], Json::str("arm"));
    refuses(&r, "role");
    // configuration-limit set
    let mut r = mini();
    set(&mut r, &["config", "limits_ref"], Json::str(""));
    refuses(&r, "member_missing");
    // recipe attribution to another commit refuses
    let mut r = mini();
    set(
        &mut r,
        &["install", "recipe_ref", "derived_from", "harbor_commit"],
        Json::str("deadbeef"),
    );
    refuses(&r, "attribution_pin");
    // a non-openai/anthropic surface refuses
    let mut r = mini();
    set(&mut r, &["config", "surface"], Json::str("google.genai"));
    refuses(&r, "surface");
    // must_reach widened refuses
    let mut r = mini();
    set(
        &mut r,
        &["egress", "must_reach"],
        Json::Arr(vec![Json::str("model_relay"), Json::str("pypi.org")]),
    );
    refuses(&r, "must_reach");
    // REQ-XC-201/REQ-XD-080 alias predicates: an `n/a` AG9 result on an
    // O-vs-H record, and a gate without evidence, refuse.
    let mut r = mini();
    set(&mut r, &["gates", "AG9", "result"], Json::str("n/a"));
    refuses(&r, "gate_failed");
    let mut r = mini();
    set(&mut r, &["gates", "AG2", "evidence_ref"], Json::str(""));
    refuses(&r, "gate_missing");
}

// ── AC-3 — legal posture (REQ-XC-202) ───────────────────────────────────────

#[test]
fn ac3_proprietary_without_terms_or_ack_refuses() {
    for (k, want) in [
        ("terms_quote", "legal_terms"),
        ("terms_source_ref", "legal_terms"),
        ("operator_ack_ref", "legal_terms"),
    ] {
        let mut r = claude();
        del(&mut r, &["harness", "legal", k]);
        refuses(&r, want);
    }
    let mut r = claude();
    set(
        &mut r,
        &["harness", "legal", "branding"],
        Json::str("logo_permitted"),
    );
    refuses(&r, "legal_branding");
    // Q-X26 verbatim: any dropped term refuses.
    for k in [
        "unmodified_pinned_binary",
        "no_claude_account",
        "black_box_arm_only",
        "name_without_logo",
        "unsupported_statement_disclosed",
    ] {
        let mut r = claude();
        set(&mut r, &["harness", "legal", "qx26", k], Json::Bool(false));
        refuses(&r, "qx26_terms");
    }
    let mut r = claude();
    set(
        &mut r,
        &["harness", "legal", "qx26", "disclosure"],
        Json::str("redacted"),
    );
    refuses(&r, "qx26_terms");
}

// ── AC-4 — AG9 parity evidence (REQ-XC-214) ─────────────────────────────────

#[test]
fn ac4_ovsh_requires_ag9_evidence() {
    for rec in [mini(), pi(), claude()] {
        assert!(ovsh_eligible(&rec, &ctx()).unwrap());
    }
    // AG9 evidence on the wrong commit refuses.
    let mut r = pi();
    set(
        &mut r,
        &["gates", "AG9", "evidence", "harbor_commit"],
        Json::str("fdba28fc411e47e8765420a1a063b4918fce3def"),
    );
    refuses(&r, "ag9_evidence");
    let mut r = pi();
    set(
        &mut r,
        &["gates", "AG9", "evidence", "same_image"],
        Json::Bool(false),
    );
    refuses(&r, "ag9_evidence");
    let mut r = pi();
    set(
        &mut r,
        &["gates", "AG9", "evidence", "harbor_adapter"],
        Json::str("other"),
    );
    refuses(&r, "ag9_evidence");
    // a record without a harbor adapter is never O-vs-H eligible.
    let mut r = mini();
    set(&mut r, &["harness", "harbor_agent"], Json::Null);
    set(
        &mut r,
        &["gates", "AG9"],
        Json::obj([("result", Json::str("n/a"))]),
    );
    assert!(!ovsh_eligible(&r, &ctx()).unwrap());
}

// ── AC-5 — version pins (REQ-XD-081) ────────────────────────────────────────

#[test]
fn ac5_version_predicates() {
    for v in [
        "@latest",
        "latest",
        "stable",
        "nightly",
        "main",
        "1.0.0-rc1",
        "0.52.0-next.3",
        "^1.2.3",
        "~1.2",
        ">=1.0",
        "1.x",
        "*",
        "x.y.z",
        "",
    ] {
        assert!(version_is_unpinned(v), "{v}");
    }
    for v in ["2.4.6", "0.52.3", "2.1.50", "v0.25.0"] {
        assert!(!version_is_unpinned(v), "{v}");
    }
    // mini-swe-agent ≠ 2.4.6 refuses.
    let mut r = mini();
    set(&mut r, &["harness", "version"], Json::str("2.4.7"));
    refuses(&r, "mini_swe_version");
    // an unpinned LiteLLM refuses.
    let mut r = mini();
    set(
        &mut r,
        &["runtime", "litellm", "version"],
        Json::str(">=1.92.0"),
    );
    refuses(&r, "version_unpinned");
    let mut r = mini();
    del(&mut r, &["runtime", "litellm"]);
    refuses(&r, "litellm_unpinned");
    // a post-freeze version change is a new participant identity.
    let a = admission_id(&mini()).unwrap();
    let mut r2 = mini();
    set(&mut r2, &["harness", "version"], Json::str("2.4.7"));
    set(
        &mut r2,
        &["harness", "freeze_at"],
        Json::str("2026-11-15T00:00:00Z"),
    );
    assert_ne!(a, admission_id(&r2).unwrap());
    // …while an identical record derives an identical identity.
    assert_eq!(a, admission_id(&mini()).unwrap());
}

// ── AC-6 — the container_installed declaration (REQ-XC-301) ────────────────

#[test]
fn ac6_declaration_shape_and_lab_host_refusal() {
    for rec in [mini(), pi(), claude()] {
        let d = harness_declaration(&rec).unwrap();
        let dd = d.get("descriptor").unwrap().as_obj().unwrap();
        assert_eq!(
            dd.get("hosting_mechanism").and_then(Json::as_str),
            Some("container_installed")
        );
        let ext = d
            .get("ext")
            .and_then(|e| e.get("hh.hosting/1"))
            .and_then(Json::as_obj)
            .unwrap();
        assert_eq!(
            ext.get("model_io_intercept").and_then(Json::as_str),
            Some("base_url")
        );
        assert_eq!(
            ext.get("process_placement").and_then(Json::as_str),
            Some("in_environment")
        );
        // model_io is earned from proxy receipts only — declared verbatim.
        assert_eq!(
            d.get("model_io_evidence").and_then(Json::as_str),
            Some("proxy_receipts_only")
        );
    }
    // lab_host declared only — a slot refuses typed until AR-19.
    let mut r = mini();
    set(&mut r, &["posture", "placement"], Json::str("lab_host"));
    validate(&r, &ctx()).unwrap(); // the declaration validates
    let e = admit_slot_placement(&r).unwrap_err();
    assert_eq!(e.rule, "placement_unsupported");
    // the in_environment side-channel residual is mandatory.
    let mut r = mini();
    set(&mut r, &["posture", "residuals"], Json::Arr(vec![]));
    refuses(&r, "posture_residual");
    let _ = SIDE_CHANNEL_MODEL_ROUTE;
}

// ── AC-7 — golden bundle recipes (REQ-XC-303) ───────────────────────────────

fn mini_recipe() -> BundleRecipe {
    BundleRecipe {
        harness: "mini-swe-agent".into(),
        version: "2.4.6".into(),
        harbor_agent: "mini_swe_agent".into(),
        harbor_commit: PIN.into(),
        builder_base:
            "docker.io/library/debian@sha256:4864455f8be6a549425df071d539eb2f261d1ae03046196639203c52bd00fe10"
                .into(),
        lock: LockSpec::UvExcludeNewer("2026-11-08T00:00:00Z".into()),
        steps: vec![
            "curl -fsSL https://astral.sh/uv/install.sh | sh".into(),
            "$HOME/.local/bin/uv tool install mini-swe-agent==2.4.6 --with 'litellm[mcp]==1.103.5' --python 3.12".into(),
            "cp -r \"$HOME/.local/share/uv/tools/mini-swe-agent/\"* \"$HH_BUNDLE_ROOT/\"".into(),
        ],
        deviations: vec![
            "litellm pinned 1.103.5 — Harbor installs unpinned".into(),
            "UV_EXCLUDE_NEWER cutoff — exact resolution lock".into(),
        ],
    }
}

#[test]
fn ac7_golden_recipes_carry_attribution_deviations_and_digests() {
    let r = mini_recipe().render();
    assert!(r.contains(&format!(
        "# derived from harbor@{PIN} installed/mini_swe_agent.py::install()"
    )));
    assert!(r.contains("# attribution: Harbor install templates (Apache-2.0)"));
    assert!(r.contains("FROM docker.io/library/debian@sha256:4864455f"));
    assert!(r.contains("ENV UV_EXCLUDE_NEWER=2026-11-08T00:00:00Z"));
    assert!(r.contains("mini-swe-agent==2.4.6"));
    assert!(r.contains("litellm[mcp]==1.103.5"));
    assert!(r.contains("#   - litellm pinned 1.103.5 — Harbor installs unpinned"));
    assert!(r.contains("ENV HH_BUNDLE_ROOT=/opt/hh-harness/mini-swe-agent-2.4.6"));

    let cc = BundleRecipe {
        harness: CLAUDE_CODE.into(),
        version: "2.1.50".into(),
        harbor_agent: "claude_code".into(),
        harbor_commit: PIN.into(),
        builder_base: "docker.io/library/debian@sha256:311f930add713cc26a7bab2ab848904230a587103b41286360ec21623b8f523c".into(),
        lock: LockSpec::NpmLockfile("lock/npm-2.1.50.json".into()),
        steps: vec!["npm install -g @anthropic-ai/claude-code@2.1.50".into()],
        deviations: vec!["npm lockfile path — bootstrap.sh unused".into()],
    };
    let cc_r = cc.render();
    assert!(cc_r.contains("installed/claude_code.py::install()"));
    assert!(cc_r.contains("COPY lock/npm-2.1.50.json package-lock.json"));
    assert!(cc_r.contains("claude-code@2.1.50"));
    assert!(cc_r.contains("/opt/hh-harness/claude-code-2.1.50"));

    // the manifest lists every package@version — the fixture manifest
    // digest is the bundle's resolved_manifest_ref.
    let manifest = parse_package_delta(
        "mini-swe-agent==2.4.6\nlitellm==1.103.5\nmcp==1.12.4\n",
        "bundle",
    )
    .unwrap();
    let pkgs = manifest.get("packages").unwrap().as_arr().unwrap();
    assert_eq!(pkgs.len(), 3);
    assert!(pkgs
        .iter()
        .all(|p| p.get("name").and_then(Json::as_str).is_some()
            && p.get("version").and_then(Json::as_str).is_some()));
    assert!(manifest_ref(&manifest).starts_with("sha256:"));
}

// ── AC-8 — golden Containerfiles + stub-podman argv + digest-only (REQ-XC-304) ──

fn layer(h: &str, v: &str, task: &str) -> LayerSpec {
    LayerSpec {
        task_image: task.into(),
        harness: h.into(),
        version: v.into(),
        system_deps: vec![
            "apt-get update && apt-get install -y --no-install-recommends curl git ca-certificates"
                .into(),
        ],
        adapter_paths: vec!["/opt/hh-harness-link".into()],
    }
}

#[test]
fn ac8_golden_containerfiles_two_harnesses_two_bases() {
    for (h, v) in [("mini-swe-agent", "2.4.6"), (CLAUDE_CODE, "2.1.50")] {
        for t in [TASK_A, TASK_B] {
            let cf = layer(h, v, t).render().unwrap();
            assert!(cf.contains(&format!("FROM {t}")), "{cf}");
            assert!(cf.contains("RUN apt-get update && apt-get install -y --no-install-recommends curl git ca-certificates"));
            assert!(cf.contains(&format!("COPY bundle/ /opt/hh-harness/{h}-{v}/")));
            assert!(cf.contains(&format!(
                "printf '%s\\n' '{v}' > /opt/hh-harness/{h}-{v}/.hh-version"
            )));
            assert!(cf.contains(&format!(
                "ln -sfn '/opt/hh-harness/{h}-{v}' '/opt/hh-harness-link'"
            )));
            // REQ-XC-204 alias: the layer names its base by digest only —
            // O and H share it through skip_install.
            assert!(!cf.contains(":latest"));
        }
    }
    // a tag-pinned task image refuses at render.
    assert!(
        layer("mini-swe-agent", "2.4.6", "docker.io/alexgshaw/task:latest")
            .render()
            .is_err()
    );
}

/// A `podman` stub on PATH: logs every argv to `$HH_PODMAN_LOG` and
/// writes a deterministic digest to any `--iidfile` argument.
fn podman_stub(dir: &Path, digest: &str, fail: bool) -> PathBuf {
    let bin = dir.join("podman");
    let rc = if fail { "exit 7" } else { "exit 0" };
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\necho \"$@\" >> \"$HH_PODMAN_LOG\"\nprev=\"\"\nfor a in \"$@\"; do if [ \"$prev\" = \"--iidfile\" ]; then printf '%s' '{digest}' > \"$a\"; fi; prev=\"$a\"; done\nif [ \"$1\" = \"run\" ]; then printf 'curl=8.10.1\\ngit=2.43.0\\nripgrep=14.1.0\\n'; fi\n{rc}\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

fn with_stub_path(dir: &Path, body: impl FnOnce()) {
    let key = "PATH";
    let old = std::env::var(key).unwrap_or_default();
    // serialized — the battery mutates PATH nowhere else.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap();
    std::env::set_var(key, format!("{}:{}", dir.display(), old));
    body();
    std::env::set_var(key, old);
}

#[test]
fn ac8_stub_podman_pins_argv_and_captures_digest() {
    let d = tmp("build-argv");
    let log = d.join("podman.log");
    let digest = "sha256:5386e86c9450fdebe68309b491d51e61d751c3add85fed469a8692745cccfb7e";
    let _bin = podman_stub(&d, digest, false);
    let plan = BuildPlan {
        kind: InstallStep::Layer,
        containerfile: d.join("Containerfile").display().to_string(),
        context: d.display().to_string(),
        tag: "hh-admit/claude-code-2.1.50".into(),
        iidfile: d.join("iid").display().to_string(),
    };
    let mut rec = Json::Null;
    with_stub_path(&d, || {
        std::env::set_var("HH_PODMAN_LOG", &log);
        rec = run_build(&plan, &d.join("build.log")).unwrap();
        std::env::remove_var("HH_PODMAN_LOG");
    });
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(
        text.contains(&format!(
            "build -f {} --iidfile {} -t hh-admit/claude-code-2.1.50 {}",
            plan.containerfile, plan.iidfile, plan.context
        )),
        "{text}"
    );
    assert_eq!(rec.get("digest").and_then(Json::as_str), Some(digest));
    assert_eq!(
        rec.get("schema").and_then(Json::as_str),
        Some("hh-harness-build/1")
    );
    // package-delta argv + manifest capture.
    let argv = package_delta_argv(rec.get("digest").and_then(Json::as_str).unwrap());
    assert_eq!(argv[..3], ["run", "--rm", digest]);
    let manifest =
        parse_package_delta("curl=8.10.1\ngit=2.43.0\nripgrep=14.1.0\n", digest).unwrap();
    assert_eq!(manifest.get("packages").unwrap().as_arr().unwrap().len(), 3);
    // a failed build step is n/a{install:<step>}, never a retry.
    let d2 = tmp("build-fail");
    let _b2 = podman_stub(&d2, digest, true);
    let mut fail = None;
    with_stub_path(&d2, || {
        std::env::set_var("HH_PODMAN_LOG", d2.join("podman.log"));
        fail = Some(run_build(&plan, &d2.join("build.log")).unwrap_err());
        std::env::remove_var("HH_PODMAN_LOG");
    });
    assert_eq!(fail.unwrap().na(), "n/a{install:layer}");
    // cells run the layer by digest only — a tag ref refuses.
    assert!(check_cell_image_ref("hh-admit/mini:2.4.6").is_err());
    assert!(check_cell_image_ref("hh-admit/mini").is_err());
    // a locally built image has no RepoDigest — bare `sha256:<id>` is the
    // digest-pinned, podman-resolvable ref (admitted since R6.32b's
    // image-export repair; still not a tag or a name).
    check_cell_image_ref(rec.get("digest").and_then(Json::as_str).unwrap()).unwrap();
    check_cell_image_ref("hh-admit/mini-swe-agent-2.4.6@sha256:4538051c1b972bd7c7ca680818488a5e655a041150b228762d3eff661303ecb1").unwrap();
    // build-once: a second layer for the same task image refuses.
    assert!(build_once_guard(&mini(), TASK_A).is_err());
    let mut r = mini();
    del(&mut r, &["install", "layers", TASK_A]);
    build_once_guard(&r, TASK_A).unwrap();
    // digest identity: layer_for_task returns the recorded digest.
    assert_eq!(
        layer_for_task(&mini(), TASK_A).unwrap(),
        "hh-admit/mini-swe-agent-2.4.6@sha256:4538051c1b972bd7c7ca680818488a5e655a041150b228762d3eff661303ecb1"
    );
    // REQ-XC-330: a proprietary bundle is digest_only, never uploaded.
    assert_eq!(artifact_upload("proprietary"), "digest_only");
    assert_eq!(artifact_upload("source_available"), "digest_only");
    assert_eq!(artifact_upload("open"), "archived");
    let rec2 = build_record(
        &BuildPlan {
            kind: InstallStep::Bundle,
            containerfile: "cf".into(),
            context: "ctx".into(),
            tag: "t".into(),
            iidfile: "iid".into(),
        },
        digest,
        Some("sha256:16bc29f96009cb137eb49c6c513eb9e22ddb4af3428f974003f00ebc3ecb65a8"),
        "sha256:609d5910010f99d433024dbd96969e3f2eb0dec54f6dbf95aba063d952c25566",
    );
    assert_eq!(rec2.get("kind").and_then(Json::as_str), Some("bundle"));
    assert_eq!(
        rec2.get("delta_manifest_ref").and_then(Json::as_str),
        Some("sha256:16bc29f96009cb137eb49c6c513eb9e22ddb4af3428f974003f00ebc3ecb65a8")
    );
}

// ── AC-9 — ST2 verify-only (REQ-XC-305) ─────────────────────────────────────

fn at<'a>(j: &'a Json, path: &[&str]) -> Option<&'a Json> {
    let mut cur = j;
    for k in path {
        cur = cur.get(k)?;
    }
    Some(cur)
}

fn obs_match(rec: &Json) -> St2Observation {
    St2Observation {
        version_stdout: format!(
            "{} {}",
            at(rec, &["harness", "name"])
                .and_then(Json::as_str)
                .unwrap(),
            at(rec, &["harness", "version"])
                .and_then(Json::as_str)
                .unwrap()
        ),
        bundle_tree_sha256: at(rec, &["install", "bundle", "digest"])
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        config_sha256: at(rec, &["config", "template_hash"])
            .and_then(Json::as_str)
            .unwrap()
            .to_string(),
        binary_sha256: at(rec, &["harness", "legal", "qx26", "binary_sha256"])
            .and_then(Json::as_str)
            .map(str::to_string),
    }
}

#[test]
fn ac9_st2_verify_only_and_mismatch_withheld() {
    for rec in [mini(), pi(), claude()] {
        let plan = st2_plan(&rec).unwrap();
        // the plan is verify-only: version, tree hash, config hash — and
        // the Q-X26 binary probe where declared. No network leg.
        assert!(st2_offline(&plan), "{:?}", plan.probes);
        assert!(plan.probes.len() >= 3);
        st2_verify(&rec, &obs_match(&rec)).unwrap();
    }
    // claude-code carries the binary probe.
    assert_eq!(st2_plan(&claude()).unwrap().probes.len(), 4);
    // every mismatch class → withheld{admission:harness_layer_mismatch},
    // refused, no cell (OutcomeClass::Refused, replans=false).
    for (name, rec, mutate) in [
        (
            "version",
            mini(),
            Box::new(|o: &mut St2Observation| o.version_stdout = "mini-swe-agent 2.4.5".into())
                as Box<dyn Fn(&mut St2Observation)>,
        ),
        (
            "bundle",
            mini(),
            Box::new(|o: &mut St2Observation| {
                o.bundle_tree_sha256 = format!("sha256:{}", "0".repeat(64))
            }),
        ),
        (
            "config",
            mini(),
            Box::new(|o: &mut St2Observation| {
                o.config_sha256 = format!("sha256:{}", "1".repeat(64))
            }),
        ),
        (
            "binary",
            claude(),
            Box::new(|o: &mut St2Observation| {
                o.binary_sha256 = Some(format!("sha256:{}", "2".repeat(64)))
            }),
        ),
    ] {
        let mut o = obs_match(&rec);
        mutate(&mut o);
        let reason = st2_verify(&rec, &o).unwrap_err();
        assert_eq!(reason, AdmissionReason::HarnessLayerMismatch, "{name}");
        let facts = SlotFacts {
            admission_refusal: Some(reason),
            ..Default::default()
        };
        let m = map_slot_facts(&facts);
        assert_eq!(
            m.disposition,
            hharness_runner::disposition::Disposition::Withheld {
                reason: WithheldReason::Admission(AdmissionReason::HarnessLayerMismatch)
            },
            "{name}"
        );
        assert_eq!(
            m.disposition.as_str(),
            "withheld{admission:harness_layer_mismatch}"
        );
        assert_eq!(
            m.outcome_class,
            hharness_ontology::control::OutcomeClass::Refused
        );
        assert!(!m.replans);
    }
    // a version-probe plan for a missing layer task refuses at lookup.
    assert!(layer_for_task(&pi(), TASK_B).is_err());
}

// ── AC-10 — the live build leg (ADR-0363; W1/R6.32) ─────────────────────────

#[test]
#[ignore = "live — ADR-0363: scripts/live-verify.sh admission-build — on the provisioned phase VM the $0 leg builds one real bundle + layer per first-pass harness and records the digest (W1, R6.32)"]
fn live_admission_build_leg() {}
