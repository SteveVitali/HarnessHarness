//! R6.24 — first-pass harness renderers (§5.3/§5.4; REQ-XC-307/308/309/
//! 310/320, REQ-XC-327, REQ-XD-082, REQ-XD-121, REQ-XE-001). Offline,
//! deterministic; goldens reviewed against the pinned Harbor adapters.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hharness_runner::admission::{self, ValidationContext, CLAUDE_CODE, MINI_SWE_AGENT};
use hharness_runner::json_ext::JsonExt;
use hharness_runner::render::{
    self, check_cell_listing, check_recording_key, precheck_instruction, receipt_effort, render,
    rendered_config_hash, rendered_files_digest, rendered_record, sm_pos_assert, RenderUse,
    SlotInputs, ELIDED,
};
use hharness_wire::json::Json;

const TOKEN: &str = "slot-token-7f3aa1c9d24b4e01";
const PORT: u16 = 8080;
const INSTRUCTION: &str = "write `ok` to /app/hh_smoke.txt";

fn fixture(name: &str) -> Json {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/r6_22")
        .join(name);
    hharness_wire::json::parse(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn slot() -> SlotInputs {
    SlotInputs {
        slot_token: TOKEN.to_string(),
        model_port: PORT,
        instruction: INSTRUCTION.as_bytes().to_vec(),
        home: "/root".to_string(),
        user: "root".to_string(),
        agent_wall_timeout_s: 3600,
        proxy_budget_ceiling_usd_micros: 5_000_000,
        slot_id: "slot-golden-1".to_string(),
    }
}

fn at<'a>(j: &'a Json, path: &[&str]) -> &'a Json {
    let mut cur = j;
    for p in path {
        cur = cur
            .get(p)
            .unwrap_or_else(|| panic!("missing member {path:?}"));
    }
    cur
}

fn secret_free(rc: &render::RenderedConfig) {
    let rec = render::elided_form(rc, TOKEN).to_canonical_string();
    assert!(
        !rec.contains(TOKEN),
        "recorded form carries the token: {rec}"
    );
    assert!(rec.contains(ELIDED));
}

fn files_have_no_secret(rc: &render::RenderedConfig) {
    for f in &rc.files {
        assert!(
            !String::from_utf8_lossy(&f.bytes).contains(TOKEN),
            "rendered file {} carries the token",
            f.path
        );
    }
}

fn only_token_credential(env: &BTreeMap<String, String>, carriers: &[&str]) {
    for (k, v) in env {
        assert_eq!(
            v.contains(TOKEN),
            carriers.contains(&k.as_str()),
            "env {k} credential membership wrong"
        );
    }
}

// ── AC-1 (AC-5.3-2; REQ-XC-307) — golden {argv, env, files} per harness ──

#[test]
fn ac1_mini_golden_mirrors_the_adapter() {
    let rec = fixture("mini-swe-agent.json");
    let rc = render(&rec, &slot(), RenderUse::HostedArm).unwrap();
    assert_eq!(
        rc.argv,
        vec![
            "bash".to_string(),
            "-c".to_string(),
            "set -o pipefail; if [ -f \"$HOME/.local/bin/env\" ]; then . \"$HOME/.local/bin/env\"; else export PATH=\"$HOME/.local/bin:$PATH\"; fi; mini-swe-agent --yolo --model=openai/glm-5.3-flash --task='write `ok` to /app/hh_smoke.txt' --output=/logs/agent/mini-swe-agent.trajectory.json --cost-limit 0 -c mini -c model.model_kwargs.timeout=3600 -c model.model_kwargs.reasoning_effort=high -c \"model.model_kwargs.api_key=$MINI_SWE_AGENT_MODEL_API_KEY_JSON\" -c model.model_kwargs.custom_llm_provider=openai -c model.model_class=litellm -c model.model_kwargs.api_base=http://127.0.0.1:8080/v1 --exit-immediately 2>&1 </dev/null | tee /logs/agent/mini-swe-agent.txt".to_string(),
        ]
    );
    // The closed env (REQ-XC-308): harness members + hints + no_proxy +
    // HOME/USER — nothing else.
    let want_env: BTreeMap<String, String> = [
        ("OPENAI_API_KEY", TOKEN),
        ("OPENAI_BASE_URL", "http://127.0.0.1:8080/v1"),
        (
            "MINI_SWE_AGENT_MODEL_API_KEY_JSON",
            "\"slot-token-7f3aa1c9d24b4e01\"",
        ),
        ("MSWEA_CONFIGURED", "true"),
        ("MSWEA_COST_TRACKING", "ignore_errors"),
        ("LITELLM_LOCAL_MODEL_COST_MAP", "True"),
        ("http_proxy", "http://127.0.0.1:3128"),
        ("https_proxy", "http://127.0.0.1:3128"),
        ("HTTP_PROXY", "http://127.0.0.1:3128"),
        ("HTTPS_PROXY", "http://127.0.0.1:3128"),
        ("no_proxy", "127.0.0.1,localhost"),
        ("NO_PROXY", "127.0.0.1,localhost"),
        ("HOME", "/root"),
        ("USER", "root"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    assert_eq!(rc.env, want_env);
    assert_eq!(rc.files.len(), 1);
    assert_eq!(
        rc.files[0].path,
        "/opt/hh-harness/mini-swe-agent-2.4.6/.hh-config.json"
    );
    let manifest = String::from_utf8(rc.files[0].bytes.clone()).unwrap();
    assert!(manifest.contains("\"api_base\":\"http://127.0.0.1:8080/v1\""));
    assert!(manifest.contains("\"model\":\"openai/glm-5.3-flash\""));
    secret_free(&rc);
    files_have_no_secret(&rc);
    only_token_credential(
        &rc.env,
        &["OPENAI_API_KEY", "MINI_SWE_AGENT_MODEL_API_KEY_JSON"],
    );
    // A changed input changes the config hash.
    let mut s2 = slot();
    s2.instruction = b"different task".to_vec();
    let rc2 = render(&rec, &s2, RenderUse::HostedArm).unwrap();
    assert_ne!(
        rendered_config_hash(&rc, TOKEN),
        rendered_config_hash(&rc2, TOKEN)
    );
    let mut s3 = slot();
    s3.model_port = 9090;
    let rc3 = render(&rec, &s3, RenderUse::HostedArm).unwrap();
    assert_ne!(
        rendered_config_hash(&rc, TOKEN),
        rendered_config_hash(&rc3, TOKEN)
    );
}

#[test]
fn ac1_pi_golden_mirrors_the_adapter() {
    let rec = fixture("pi.json");
    let rc = render(&rec, &slot(), RenderUse::HostedArm).unwrap();
    assert_eq!(
        rc.argv,
        vec![
            "bash".to_string(),
            "-c".to_string(),
            "set -o pipefail; . ~/.nvm/nvm.sh; mkdir -p /logs/agent/pi/sessions && PI_CODING_AGENT_DIR=/tmp/harbor-pi-agent pi --print --mode json --session-dir /logs/agent/pi/sessions --provider harbor-endpoint --model glm-5.3-flash --thinking high 'write `ok` to /app/hh_smoke.txt' 2>&1 </dev/null | grep -v '\"type\":\"message_update\"' | stdbuf -oL tee /logs/agent/pi.txt".to_string(),
        ]
    );
    for k in ["OPENAI_API_KEY", "http_proxy", "no_proxy", "HOME", "USER"] {
        assert!(rc.env.contains_key(k), "env missing {k}");
    }
    assert!(!rc.env.contains_key("OPENAI_BASE_URL")); // Pi's spec disables the base-url env destination.
    only_token_credential(&rc.env, &["OPENAI_API_KEY"]);
    // models.json — Python json.dumps(indent=2)+"\n" byte for byte.
    let models = String::from_utf8(rc.files[0].bytes.clone()).unwrap();
    assert_eq!(
        models,
        "{\n  \"providers\": {\n    \"harbor-endpoint\": {\n      \"baseUrl\": \"http://127.0.0.1:8080/v1\",\n      \"apiKey\": \"$OPENAI_API_KEY\",\n      \"api\": \"openai-completions\",\n      \"models\": [\n        {\n          \"id\": \"glm-5.3-flash\",\n          \"reasoning\": true\n        }\n      ]\n    }\n  }\n}\n"
    );
    assert_eq!(rc.files[0].path, "/tmp/harbor-pi-agent/models.json");
    secret_free(&rc);
    files_have_no_secret(&rc);
}

#[test]
fn ac1_claude_golden_mirrors_the_adapter() {
    let rec = fixture("claude-code.json");
    let rc = render(&rec, &slot(), RenderUse::HostedArm).unwrap();
    let suffix = hharness_wire::sha256::sha256_hex(b"slot-golden-1")[..32].to_string();
    let shell_var = format!("claude_code_instruction_{suffix}");
    let env_var = shell_var.to_uppercase();
    assert_eq!(
        rc.argv,
        vec![
            "bash".to_string(),
            "-c".to_string(),
            format!(
                "set -o pipefail; export PATH=\"$HOME/.local/bin:$PATH\"; {shell_var}=\"${env_var}\"; unset {env_var}; printf \"%s\" \"${shell_var}\" | claude --verbose --output-format=stream-json --effort high --disallowedTools WebSearch,WebFetch --permission-mode=bypassPermissions --print 2>&1 | tee /logs/agent/claude-code.txt"
            ),
        ]
    );
    // Setup exec mirrors the adapter's config-dir prep.
    let setup = rc.setup_argv.as_ref().unwrap();
    assert!(setup[2].contains("mkdir -p $CLAUDE_CONFIG_DIR/debug"));
    // The Q-X26 env (REQ-XC-327): token only in AUTH_TOKEN; empties present.
    assert_eq!(rc.env["ANTHROPIC_AUTH_TOKEN"], TOKEN);
    assert_eq!(rc.env["ANTHROPIC_API_KEY"], "");
    assert_eq!(rc.env["CLAUDE_CODE_OAUTH_TOKEN"], "");
    assert_eq!(rc.env["ANTHROPIC_BASE_URL"], "http://127.0.0.1:8080");
    for k in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_FABLE_MODEL",
        "CLAUDE_CODE_SUBAGENT_MODEL",
    ] {
        assert_eq!(rc.env[k], "glm-5.3-flash", "alias {k}");
    }
    assert_eq!(rc.env["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"], "1");
    assert_eq!(rc.env["IS_SANDBOX"], "1");
    assert_eq!(rc.env["CLAUDE_CONFIG_DIR"], "/logs/agent/sessions");
    assert_eq!(rc.env[&env_var], INSTRUCTION);
    only_token_credential(&rc.env, &["ANTHROPIC_AUTH_TOKEN"]);
    assert_eq!(rc.files.len(), 2);
    assert_eq!(
        String::from_utf8(rc.files[0].bytes.clone()).unwrap(),
        "{\"hasCompletedOnboarding\":true}"
    );
    let hh_env = String::from_utf8(rc.files[1].bytes.clone()).unwrap();
    assert!(hh_env.contains(&format!("ANTHROPIC_AUTH_TOKEN={ELIDED}")));
    assert!(!hh_env.contains(TOKEN));
    assert!(!hh_env.contains("CLAUDE_CODE_INSTRUCTION"));
    secret_free(&rc);
    files_have_no_secret(&rc);
}

// ── AC-2 (AC-5.3-3; REQ-XC-308) — closed env + filesystem listing ────────────

#[test]
fn ac2_closed_env_and_cell_listing() {
    for name in ["mini-swe-agent.json", "pi.json", "claude-code.json"] {
        let rec = fixture(name);
        let rc = render(&rec, &slot(), RenderUse::HostedArm).unwrap();
        // The env is exactly the declared closed set — no extras.
        let mut allowed = 0;
        for k in rc.env.keys() {
            assert!(
                [
                    "OPENAI_API_KEY",
                    "OPENAI_BASE_URL",
                    "MINI_SWE_AGENT_MODEL_API_KEY_JSON",
                    "MSWEA_CONFIGURED",
                    "MSWEA_COST_TRACKING",
                    "LITELLM_LOCAL_MODEL_COST_MAP",
                    "ANTHROPIC_AUTH_TOKEN",
                    "ANTHROPIC_API_KEY",
                    "CLAUDE_CODE_OAUTH_TOKEN",
                    "ANTHROPIC_BASE_URL",
                    "ANTHROPIC_MODEL",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                    "ANTHROPIC_DEFAULT_FABLE_MODEL",
                    "CLAUDE_CODE_SUBAGENT_MODEL",
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
                    "IS_SANDBOX",
                    "CLAUDE_CONFIG_DIR",
                    "http_proxy",
                    "https_proxy",
                    "HTTP_PROXY",
                    "HTTPS_PROXY",
                    "no_proxy",
                    "NO_PROXY",
                    "HOME",
                    "USER",
                ]
                .contains(&k.as_str())
                    || k.starts_with("CLAUDE_CODE_INSTRUCTION_"),
                "env carries undeclared member {k}"
            );
            allowed += 1;
        }
        assert_eq!(allowed, rc.env.len());
        assert_eq!(rc.env["no_proxy"], "127.0.0.1,localhost");
        // The shared cell guard accepts the projection (no kernel token).
        hharness_env::egress_allowlist::validate_cell_env(&rc.env).unwrap();
    }
    // The filesystem-listing fixture (REQ-XC-308): agent + installed-agent
    // present; tests/solution absent.
    let good: Vec<String> = [
        "/app",
        "/logs",
        "/logs/agent",
        "/installed-agent",
        "/opt",
        "/work",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    check_cell_listing(&good).unwrap();
    for bad in ["/tests", "/solution"] {
        let mut l = good.clone();
        l.push(bad.to_string());
        assert!(check_cell_listing(&l).is_err(), "{bad} present");
    }
    for missing in ["/logs/agent", "/installed-agent"] {
        let l: Vec<String> = good
            .iter()
            .filter(|p| p.as_str() != missing)
            .cloned()
            .collect();
        assert!(check_cell_listing(&l).is_err(), "{missing} absent");
    }
}

// ── AC-3/AC-4 (AC-5.3-4, AC-6.10-1; REQ-XC-309/XD-082) — effort pins ─────────

#[test]
fn ac3_native_effort_at_the_pin() {
    let mini = render(
        &fixture("mini-swe-agent.json"),
        &slot(),
        RenderUse::HostedArm,
    )
    .unwrap();
    assert!(mini.argv[2].contains("-c model.model_kwargs.reasoning_effort=high"));
    let pi = render(&fixture("pi.json"), &slot(), RenderUse::HostedArm).unwrap();
    assert!(pi.argv[2].contains("--thinking high"));
    let claude = render(&fixture("claude-code.json"), &slot(), RenderUse::HostedArm).unwrap();
    assert!(claude.argv[2].contains("--effort high"));
    // A mismatched declared mechanism refuses.
    let mut src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/r6_22/pi.json"),
    )
    .unwrap();
    src = src.replace(
        "\"mechanism\": \"--thinking\"",
        "\"mechanism\": \"--effort\"",
    );
    let bad = hharness_wire::json::parse(&src).unwrap();
    assert!(render(&bad, &slot(), RenderUse::HostedArm).is_err());
}

#[test]
fn ac4_receipt_records_requested_and_applied() {
    // R6.10's proxy pin fold — a harness sending `medium` is receipted
    // requested: medium, applied: high (the records-level read).
    let receipt = Json::obj([
        ("schema", Json::str("hh.model_io/1")),
        (
            "mutations",
            Json::obj([(
                "effort",
                Json::obj([
                    ("requested", Json::str("medium")),
                    ("applied", Json::str("high")),
                    ("path", Json::str("reasoning_effort")),
                ]),
            )]),
        ),
    ]);
    let (req, app) = receipt_effort(&receipt).unwrap();
    assert_eq!(req.as_deref(), Some("medium"));
    assert_eq!(app.as_deref(), Some("high"));
}

// ── AC-5 (AC-5.3-5; REQ-XC-310) — LiteLLM local cost map ─────────────────────

#[test]
fn ac5_litellm_local_cost_map_declared() {
    let rec = fixture("mini-swe-agent.json");
    let switches = at(&rec, &["config", "disable_switches"]).as_arr().unwrap();
    assert!(switches.iter().any(|s| {
        s.get("switch").and_then(Json::as_str) == Some("LITELLM_LOCAL_MODEL_COST_MAP=True")
    }));
    let rc = render(&rec, &slot(), RenderUse::HostedArm).unwrap();
    assert_eq!(rc.env["LITELLM_LOCAL_MODEL_COST_MAP"], "True");
}

// ── AC-6 (AC-5.3-9; REQ-XC-320) — instruction pre-check + byte parity ────────

#[test]
fn ac6_precheck_flags_pi_leading_dash() {
    let rec = fixture("pi.json");
    let hazards = precheck_instruction("pi", b"-rm -rf /");
    assert!(hazards.iter().any(|h| h.code == "leading_dash"));
    let mut s = slot();
    s.instruction = b"-danger".to_vec();
    let e = render(&rec, &s, RenderUse::HostedArm).unwrap_err();
    assert_eq!(e.rule, "instruction_hazard");
    // NUL and non-UTF-8 are universal hazards.
    assert!(precheck_instruction(MINI_SWE_AGENT, b"a\0b")
        .iter()
        .any(|h| h.code == "nul_byte"));
    assert!(precheck_instruction(CLAUDE_CODE, &[0xff])
        .iter()
        .any(|h| h.code == "non_utf8"));
    // Benign instructions pass and their bytes travel verbatim — argv
    // for mini/Pi, the env-var → stdin channel for Claude Code (F13).
    assert!(precheck_instruction("pi", b"benign task").is_empty());
    for name in ["mini-swe-agent.json", "pi.json"] {
        let r = render(&fixture(name), &slot(), RenderUse::HostedArm).unwrap();
        assert_eq!(r.instruction, INSTRUCTION.as_bytes());
        assert!(r.argv[2].contains(INSTRUCTION), "{name} instruction bytes");
    }
    let r = render(&fixture("claude-code.json"), &slot(), RenderUse::HostedArm).unwrap();
    assert_eq!(r.instruction, INSTRUCTION.as_bytes());
    assert!(r
        .env
        .iter()
        .any(|(k, v)| k.starts_with("CLAUDE_CODE_INSTRUCTION_") && v == INSTRUCTION));
}

// ── AC-7 (AC-7.1-1; REQ-XE-001 + REQ-XD-122) — mini participant record ───────

#[test]
fn ac7_mini_participant_identity_and_limits() {
    let rec = fixture("mini-swe-agent.json");
    let rc = render(&rec, &slot(), RenderUse::HostedArm).unwrap();
    let rr = rendered_record(&rc, &rec, &slot()).unwrap();
    let p = at(&rr, &["participant"]).as_obj().unwrap();
    assert_eq!(
        p["harbor_commit"].as_str().unwrap(),
        hharness_runner::harbor::HARBOR_PIN
    );
    assert_eq!(p["mini_version"].as_str().unwrap(), "2.4.6");
    assert_eq!(
        p["mini_yaml_sha256"].as_str().unwrap(),
        render::MINI_YAML_SHA256
    );
    let specs: Vec<&str> = p["c_spec_list"]
        .as_arr()
        .unwrap()
        .iter()
        .map(|j| j.as_str().unwrap())
        .collect();
    assert_eq!(
        specs,
        vec![
            "mini",
            "model.model_kwargs.timeout=3600",
            "model.model_kwargs.reasoning_effort=high",
            "model.model_kwargs.api_key=$MINI_SWE_AGENT_MODEL_API_KEY_JSON",
            "model.model_kwargs.custom_llm_provider=openai",
            "model.model_class=litellm",
            "model.model_kwargs.api_base=http://127.0.0.1:8080/v1",
        ]
    );
    // The declared external caps ride the record; the ceiling binding is
    // labelled infra-uncertain (REQ-XD-122 alias).
    let lim = at(&rr, &["limits"]).as_obj().unwrap();
    assert_eq!(lim["agent_wall_timeout_s"].as_int().unwrap(), 3600);
    assert_eq!(
        lim["proxy_budget_ceiling_usd_micros"].as_int().unwrap(),
        5_000_000
    );
    assert_eq!(lim["ceiling_binding"].as_str().unwrap(), "infra_uncertain");
    // The config record matches the record's files digest + config hash.
    assert_eq!(
        at(&rr, &["files_digest"]).as_str().unwrap(),
        rendered_files_digest(&rc.files)
    );
}

// ── AC-8 (AC-7.5-10; REQ-XD-121) — litellm identity + recording keys ─────────

#[test]
fn ac8_litellm_version_and_recording_key() {
    let mini = fixture("mini-swe-agent.json");
    let rc = render(&mini, &slot(), RenderUse::HostedArm).unwrap();
    let rr = rendered_record(&rc, &mini, &slot()).unwrap();
    assert_eq!(at(&rr, &["litellm_version"]).as_str().unwrap(), "1.103.5");
    let img = at(&rr, &["participant", "image"]).as_obj().unwrap();
    assert!(img["digest"].as_str().unwrap().starts_with("sha256:"));
    assert!(img["lock"].as_str().unwrap().starts_with("sha256:"));
    // Recordings keyed by litellm_version refuse a mismatched key.
    let good = Json::obj([("litellm_version", Json::str("1.103.5"))]);
    check_recording_key(&good, &mini).unwrap();
    let bad = Json::obj([("litellm_version", Json::str("1.999.0"))]);
    assert!(check_recording_key(&bad, &mini).is_err());
    let missing = Json::obj([("other", Json::str("x"))]);
    assert!(check_recording_key(&missing, &mini).is_err());
    // A keyed recording against a non-LiteLLM harness refuses; an unkeyed
    // one admits.
    let pi = fixture("pi.json");
    assert!(check_recording_key(&good, &pi).is_err());
    check_recording_key(&missing, &pi).unwrap();
}

// ── AC-9 (AC-5.4-1; REQ-XC-327) — Claude Code Q-X26 + records assertion ──────

#[test]
fn ac9_claude_qx26_terms_and_sm_pos_assert() {
    let rec = fixture("claude-code.json");
    admission::validate(&rec, &ValidationContext::first_pass()).unwrap();
    // Black-box arm only: an HIR-target use refuses typed.
    let e = render(&rec, &slot(), RenderUse::HirTarget).unwrap_err();
    assert_eq!(e.rule, "qx26_black_box");
    render(
        &fixture("mini-swe-agent.json"),
        &slot(),
        RenderUse::HirTarget,
    )
    .unwrap();
    // The records-level assertion (mediator rows + receipts).
    let denied =
        |host: &str| Json::obj([("host", Json::str(host)), ("decision", Json::str("denied"))]);
    let receipt = |path: &str, disp: &str| {
        Json::obj([
            ("downstream", Json::obj([("path", Json::str(path))])),
            ("disposition", Json::str(disp)),
        ])
    };
    let conforming_rows = vec![
        denied("api.anthropic.com"),
        denied("statsig.anthropic.com"),
        Json::obj([
            ("host", Json::str("registry.npmjs.org")),
            ("decision", Json::str("denied")),
        ]),
    ];
    let conforming_receipts = vec![
        receipt("/v1/messages", "forwarded"),
        receipt("/v1/messages/count_tokens", "refused_route"),
    ];
    sm_pos_assert(&rec, &conforming_rows, &conforming_receipts).unwrap();
    // An allowed Anthropic-host attempt fails.
    let mut bad_rows = conforming_rows.clone();
    bad_rows.push(Json::obj([
        ("host", Json::str("api.anthropic.com")),
        ("decision", Json::str("allow")),
    ]));
    assert_eq!(
        sm_pos_assert(&rec, &bad_rows, &conforming_receipts)
            .unwrap_err()
            .rule,
        "anthropic_attempt"
    );
    // A denied Anthropic host outside expected_denied fails.
    let mut uncovered = conforming_rows.clone();
    uncovered.push(denied("fivetran.anthropic.com"));
    assert_eq!(
        sm_pos_assert(&rec, &uncovered, &conforming_receipts)
            .unwrap_err()
            .rule,
        "expected_denied"
    );
    // A non-/v1/messages receipt fails.
    let mut bad_receipts = conforming_receipts.clone();
    bad_receipts.push(receipt("/v1/chat/completions", "forwarded"));
    assert_eq!(
        sm_pos_assert(&rec, &conforming_rows, &bad_receipts)
            .unwrap_err()
            .rule,
        "receipt_route"
    );
    // A refused_route at an undeclared path also fails.
    let mut bad2 = conforming_receipts.clone();
    bad2.push(receipt("/v1/models", "refused_route"));
    assert!(sm_pos_assert(&rec, &conforming_rows, &bad2).is_err());
}

// ── template_hash ↔ rendered files digest (R6.22's ST2 composite) ────────────

#[test]
fn files_digest_equals_declared_template_hash() {
    for name in ["mini-swe-agent.json", "pi.json", "claude-code.json"] {
        let rec = fixture(name);
        let rc = render(&rec, &slot(), RenderUse::HostedArm).unwrap();
        eprintln!(
            "{name} rendered_files_digest = {}",
            rendered_files_digest(&rc.files)
        );
        assert_eq!(
            rendered_files_digest(&rc.files),
            at(&rec, &["config", "template_hash"]).as_str().unwrap(),
            "{name} template_hash must equal the rendered files composite"
        );
    }
}
