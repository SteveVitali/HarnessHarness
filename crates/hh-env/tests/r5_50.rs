//! R5.50 — `hh-plugin-unselected/1`: the unselected
//! existing-component disposition (GATE-G15 PASSED —
//! **out-of-scope disposition**). The participation stage is
//! voided this round — the `hh-compact-evict-oldest` package is
//! NOT selected and no walkthrough executes (build/seal/load/
//! invoke/helper/bind/cohort/adverse runs all absent); the record
//! is a typed unselected packet over supplied facts. Machinery is
//! exercised where meaningful without participation: the
//! committed manifest template decodes strictly and admits
//! through the real `admit_plugin`, mutated decls produce the
//! verbatim refusal vocabulary, and the ten pinned inputs
//! byte-verify. Fails-without: the `plugin` module and this
//! battery do not exist at base `bc18d372` (compile-red
//! evidence).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hharness_env::plugin::{
    assemble, evaluate_claims, evaluate_decl, evaluate_gate, evaluate_pins, evaluate_vocabulary,
    evaluate_walkthrough, ClaimsFacts, Disposition, GateFacts, ManifestDeclFacts, PinsFacts,
    WalkthroughFacts, EXERCISES, GATE, LEGS, VERSION, WALKTHROUGH_MEMBERS,
};
use hharness_env::preflight::{check_pins, Leg, LegVerdict, PinnedArtifact};
use hharness_plugin::contract::EvalPoint;
use hharness_registry::corpus;
use hharness_registry::extension::plugin::{
    admit_plugin, manifest_from_json, AdmissionContext, PathOrLocator, PluginManifest, Requests,
};
use hharness_registry::store::RegistryStore;
use hharness_wire::json::{self, Json};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ── the ten pinned inputs (nine H8 artifacts + the committed
// manifest template — id path sha256, verbatim) ─────────────────
const ARTIFACTS: &str = "\
a-count crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-count.json 9f6d4f11695efff5671745015f7f7c87d9e4e95f049364e027e6a60fc4124ce6
a-echo crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-echo.json 9fdc327157877c3fdbb124a0a5f98b5a2df0d22a7889c4da9e11319559977b40
a-hello crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-hello.json 5bf206ca07f13659bc7bc69a71f7d7dd4b7348a1f635bfdb9a4dccc0912e0344
a-sum crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-sum.json e30c65b99203d604f9a843e72c31fd1a053bf31c8ab901bb295c2773b0f8e349
a-upper crates/hh-bench/fixtures/benchset/stage3_v1/suite.tb2/tasks/a-upper.json 3b489ba6cf9d225f926f34c027899583b00d0e6607342c987c593fe7f1fc87a4
ex-01 scripts/examples/ex-01.json 0d4cec8dea92168636d2224ecb392628c69b28bc73353674312183db472178e3
ex-02 scripts/examples/ex-02.json 8e35aac7c68aba68d3d2ff1c9abf23bf5ff7f6bff372f948eede02f1eea46285
ex-02-fixture scripts/examples/ex-02-fixture.json f21a340f966d2d21f72f424635461906dbff16e09cfb2f6d9844ad8e203300c8
fixture-definition clients/ts/fixture-definition.json 732af9f584a9f321d347f6f027ebd82a3376a33bb9580f63b2fcd6602865d0f8
plugin-manifest plugins/hh-compact-evict-oldest/plugin.manifest.json 80eeabb82b8550e146cc319499b69f0fa1ceb41733ba264ef4bd3035f0ad4a83";

fn artifact_pins() -> Vec<PinnedArtifact> {
    ARTIFACTS
        .lines()
        .map(|l| {
            let mut c = l.split_whitespace();
            PinnedArtifact {
                artifact_id: c.next().unwrap().to_string(),
                path: repo_root().join(c.next().unwrap()),
                sha256: c.next().unwrap().to_string(),
            }
        })
        .collect()
}

// ── leg 1 — gate_readout (GATE-G15 verbatim) ─────────────────────
fn gate_facts() -> GateFacts {
    GateFacts {
        decision: "PASSED — out-of-scope disposition".into(),
        operator_words: "accept, continue".into(),
        decided_at: "2026-11-04T14:32:29Z".into(),
        block_sha256_prefix: "0e5bd136858c".into(),
        df_h9_1: "open".into(),
    }
}

// ── leg 2 — pins_frozen ──────────────────────────────────────────
fn pins_facts() -> PinsFacts {
    let verdict = check_pins(&artifact_pins());
    PinsFacts {
        pins: artifact_pins().len() as u64,
        pins_verdict: verdict.name().to_string(),
        manifest_sha256: artifact_pins()
            .iter()
            .find(|a| a.artifact_id == "plugin-manifest")
            .unwrap()
            .sha256
            .clone(),
    }
}

// ── leg 3 — manifest_decl: the committed template decoded +
// admitted through the real machinery (pure read-side) ──────────
const MANIFEST_PATH: &str = "plugins/hh-compact-evict-oldest/plugin.manifest.json";

fn manifest_json() -> Json {
    let text = std::fs::read_to_string(repo_root().join(MANIFEST_PATH))
        .expect("committed manifest template readable");
    json::parse(&text).expect("committed manifest template parses")
}

fn committed_manifest() -> PluginManifest {
    manifest_from_json(&manifest_json(), MANIFEST_PATH).expect("committed template decodes")
}

fn store_at(d: &Path) -> RegistryStore {
    corpus::build(d).unwrap();
    RegistryStore::open(d, &corpus::kernel_registrar()).unwrap()
}

fn corpus_dir() -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("hh-env-r5_50-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ctx(first_party: bool) -> AdmissionContext {
    AdmissionContext {
        first_party,
        now: EvalPoint::kernel(),
        requests_cap: None,
    }
}

fn manifest_decl_facts() -> ManifestDeclFacts {
    let m = committed_manifest();
    let c = &m.contributions[0];
    let exe = c
        .executable
        .as_ref()
        .expect("variant contribution executable");
    let d = corpus_dir();
    let s = store_at(&d);
    let report = admit_plugin(&s, &m, &ctx(true));
    let (admit, admit_detail) = match &report {
        Ok(_) => ("admitted".to_string(), String::new()),
        Err(e) => {
            let detail = match e {
                hharness_registry::extension::plugin::ManifestError::ContractIncompatible {
                    failures,
                } => failures.first().map(|f| f.detail()).unwrap_or_default(),
                _ => String::new(),
            };
            (e.kind_str().to_string(), detail)
        }
    };
    ManifestDeclFacts {
        decoded: true,
        plugin_id: m.identity.id(),
        version_label: m.identity.version_label.clone().unwrap_or_default(),
        tier: m.tier.clone(),
        contributions: m.contributions.len() as u64,
        kind: c.kind.as_str().to_string(),
        class_id: c
            .declaration
            .get("class_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string(),
        contract_range: c.contract_range.clone().unwrap_or_default(),
        deterministic: matches!(c.declaration.get("deterministic"), Some(Json::Bool(true))),
        model_call: matches!(c.declaration.get("model_call"), Some(Json::Bool(true))),
        op_kinds: match c.declaration.get("op_kinds") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(Json::as_str)
                .collect::<Vec<_>>()
                .join(","),
            _ => String::new(),
        },
        isolation: exe.isolation.as_str().to_string(),
        placement_preference: exe
            .placement_preference
            .map(|p| p.as_str().to_string())
            .unwrap_or_default(),
        requests_empty: m.requests == Requests::default(),
        claims_empty: m.claims == Json::obj([]) && m.conformance_claims.is_empty(),
        code_pointer_zero: exe.code_pointer.digest.chars().all(|ch| ch == '0')
            && exe.code_pointer.size == 0,
        admit,
        admit_detail,
    }
}

// ── leg 4 — refusal_vocabulary: mutated decls through the real
// codec/admission (declared mutations only — nothing admitted) ──

/// `decoded` on success, else the `ManifestError::kind_str`
/// verbatim — the observed spelling the leg echoes.
fn decode_name(j: &Json) -> String {
    match manifest_from_json(j, MANIFEST_PATH) {
        Ok(_) => "decoded".to_string(),
        Err(e) => e.kind_str().to_string(),
    }
}

/// `admitted` on success, else the kind spelling — the observed
/// refusal the leg echoes.
fn admit_name(j: &Json, ctx: &AdmissionContext) -> String {
    let Ok(m) = manifest_from_json(j, MANIFEST_PATH) else {
        return manifest_from_json(j, MANIFEST_PATH)
            .unwrap_err()
            .kind_str()
            .to_string();
    };
    let d = corpus_dir();
    let s = store_at(&d);
    match admit_plugin(&s, &m, ctx) {
        Ok(_) => "admitted".to_string(),
        Err(e) => e.kind_str().to_string(),
    }
}

/// Normalize the template's declared `"1.0"` requires to `"1"`
/// (the kernel policy's supported version) so an admit-stage
/// exercise reaches its intended check in isolation — the raw
/// template's `ContractIncompatible` fires first otherwise.
fn admit_ready(j: &mut Json) {
    if let Json::Obj(m) = j {
        if let Json::Obj(r) = m.get_mut("requires").unwrap() {
            r.insert("hir_dialect".into(), Json::str("1"));
            r.insert("registry_dialect".into(), Json::str("1"));
            r.insert("plugin_abi".into(), Json::str("1"));
        }
    }
}

/// `exercise → observed refusal kind_str` — every mutation is a
/// one-member edit of the committed template driven through the
/// real codec/admission on a fresh corpus store.
fn exercise_vocabulary() -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    // An unknown top-level member — strict decode refuses.
    let mut j = manifest_json();
    if let Json::Obj(m) = &mut j {
        m.insert("evil".into(), Json::Null);
    }
    out.insert("unknown_member".into(), decode_name(&j));
    // A contribution kind outside the closed set.
    let mut j = manifest_json();
    if let Json::Obj(m) = &mut j {
        if let Json::Arr(c) = m.get_mut("contributions").unwrap() {
            if let Json::Obj(cm) = &mut c[0] {
                cm.insert("kind".into(), Json::str("gadget"));
            }
        }
    }
    out.insert("unknown_kind".into(), decode_name(&j));
    // A digest-less code_pointer — not a pin in sealed form.
    let mut j = manifest_json();
    if let Json::Obj(m) = &mut j {
        if let Json::Arr(c) = m.get_mut("contributions").unwrap() {
            if let Json::Obj(cm) = &mut c[0] {
                if let Json::Obj(e) = cm.get_mut("executable").unwrap() {
                    if let Json::Obj(cp) = e.get_mut("code_pointer").unwrap() {
                        cp.insert("digest".into(), Json::str(""));
                    }
                }
            }
        }
    }
    out.insert("empty_digest".into(), decode_name(&j));
    // A `local/` plugin preferring `in_process` — first-party only.
    let mut j = manifest_json();
    if let Json::Obj(m) = &mut j {
        if let Json::Obj(id) = m.get_mut("identity").unwrap() {
            id.insert("namespace".into(), Json::str("local"));
        }
        if let Json::Arr(c) = m.get_mut("contributions").unwrap() {
            if let Json::Obj(cm) = &mut c[0] {
                if let Json::Obj(e) = cm.get_mut("executable").unwrap() {
                    e.insert("placement_preference".into(), Json::str("in_process"));
                }
            }
        }
    }
    out.insert("local_in_process".into(), admit_name(&j, &ctx(true)));
    // Requests are claims — they never widen past the cap (the
    // requires normalized first so the cap stage is reached).
    let mut j = manifest_json();
    admit_ready(&mut j);
    if let Json::Obj(m) = &mut j {
        if let Json::Obj(r) = m.get_mut("requests").unwrap() {
            r.insert("env_keys".into(), Json::Arr(vec![Json::str("AWS_SECRET")]));
        }
    }
    let mut capped = ctx(true);
    capped.requests_cap = Some(Requests::default());
    out.insert("env_key_over_cap".into(), admit_name(&j, &capped));
    // A `requires.plugin_abi` outside the policy — the complete
    // incompatibility report, never a silent skip.
    let mut j = manifest_json();
    admit_ready(&mut j);
    if let Json::Obj(m) = &mut j {
        if let Json::Obj(r) = m.get_mut("requires").unwrap() {
            r.insert("plugin_abi".into(), Json::str(">=9"));
        }
    }
    out.insert("plugin_abi_nine".into(), admit_name(&j, &ctx(true)));
    // A namespace outside the Stage-1 {hh, local} set.
    let mut j = manifest_json();
    if let Json::Obj(m) = &mut j {
        if let Json::Obj(id) = m.get_mut("identity").unwrap() {
            id.insert("namespace".into(), Json::str("evil-corp"));
        }
    }
    out.insert("forbidden_namespace".into(), admit_name(&j, &ctx(true)));
    out
}

// ── legs 5+6 — walkthrough_absent / claims_withheld ──────────────
fn walkthrough_facts() -> WalkthroughFacts {
    WalkthroughFacts {
        counts: WALKTHROUGH_MEMBERS
            .iter()
            .map(|m| (m.to_string(), 0))
            .collect(),
        selection_attempted: false,
        reseal_attempted: false,
    }
}

fn claims_facts() -> ClaimsFacts {
    ClaimsFacts {
        df_h9_1: "open".into(),
        participant_predicates: "withheld".into(),
        outreach: "none".into(),
        claim_awarded: false,
        third_arm_claim: false,
        provider_efficacy_claim: false,
        publication_claim: false,
        capability_claim: false,
        adoption_claim: false,
    }
}

fn leg(name: &str, verdict: LegVerdict, facts: &[(&str, String)]) -> Leg {
    Leg {
        name: name.into(),
        verdict,
        facts: facts
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    }
}

fn s(v: impl ToString) -> String {
    v.to_string()
}

/// The six legs driven on real inputs (pins recomputed, the
/// committed manifest decoded/admitted, the refusal vocabulary
/// exercised, the absent counts and withheld claims folded) — the
/// packet's honest record.
fn run_packet() -> Vec<Leg> {
    let g = gate_facts();
    let pf = pins_facts();
    let m = manifest_decl_facts();
    let vocabulary = exercise_vocabulary();
    let w = walkthrough_facts();
    let c = claims_facts();
    vec![
        leg(
            "gate_readout",
            evaluate_gate(&g),
            &[
                ("decision", s(&g.decision)),
                ("operator_words", s(&g.operator_words)),
                ("decided_at", s(&g.decided_at)),
                ("block_sha256_prefix", s(&g.block_sha256_prefix)),
                ("df_h9_1", s(&g.df_h9_1)),
            ],
        ),
        leg(
            "pins_frozen",
            evaluate_pins(&pf),
            &[
                ("pins", s(pf.pins)),
                ("pins_verdict", s(&pf.pins_verdict)),
                ("manifest_sha256", s(&pf.manifest_sha256)),
            ],
        ),
        leg(
            "manifest_decl",
            evaluate_decl(&m),
            &[
                ("decoded", s(m.decoded)),
                ("plugin_id", s(&m.plugin_id)),
                ("version_label", s(&m.version_label)),
                ("tier", s(&m.tier)),
                ("contributions", s(m.contributions)),
                ("kind", s(&m.kind)),
                ("class_id", s(&m.class_id)),
                ("contract_range", s(&m.contract_range)),
                ("deterministic", s(m.deterministic)),
                ("model_call", s(m.model_call)),
                ("op_kinds", s(&m.op_kinds)),
                ("isolation", s(&m.isolation)),
                ("placement_preference", s(&m.placement_preference)),
                ("requests_empty", s(m.requests_empty)),
                ("claims_empty", s(m.claims_empty)),
                ("code_pointer_zero", s(m.code_pointer_zero)),
                ("admit", s(&m.admit)),
                ("admit_detail", s(&m.admit_detail)),
            ],
        ),
        leg(
            "refusal_vocabulary",
            evaluate_vocabulary(&vocabulary),
            &vocabulary
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect::<Vec<_>>(),
        ),
        leg(
            "walkthrough_absent",
            evaluate_walkthrough(&w),
            &w.counts
                .iter()
                .map(|(k, v)| (k.as_str(), s(v)))
                .chain([
                    ("selection_attempted", s(w.selection_attempted)),
                    ("reseal_attempted", s(w.reseal_attempted)),
                ])
                .collect::<Vec<_>>(),
        ),
        leg(
            "claims_withheld",
            evaluate_claims(&c),
            &[
                ("df_h9_1", s(&c.df_h9_1)),
                ("participant_predicates", s(&c.participant_predicates)),
                ("outreach", s(&c.outreach)),
                ("claim_awarded", s(c.claim_awarded)),
                ("third_arm_claim", s(c.third_arm_claim)),
                ("provider_efficacy_claim", s(c.provider_efficacy_claim)),
                ("publication_claim", s(c.publication_claim)),
                ("capability_claim", s(c.capability_claim)),
                ("adoption_claim", s(c.adoption_claim)),
            ],
        ),
    ]
}

// ── the battery ──────────────────────────────────────────────────

#[test]
fn p50_gate_and_pins_verified() {
    assert_eq!(evaluate_gate(&gate_facts()), LegVerdict::Verified);
    // A procedures-admitted decision, a missing verbatim member or
    // a closed DF-H9-1 refuses — never relabelled.
    for f in [
        GateFacts {
            decision: "PASSED — procedures admitted".into(),
            ..gate_facts()
        },
        GateFacts {
            df_h9_1: "closed".into(),
            ..gate_facts()
        },
        GateFacts {
            block_sha256_prefix: "not-hex!!".into(),
            ..gate_facts()
        },
    ] {
        assert!(matches!(evaluate_gate(&f), LegVerdict::Refused(_)));
    }
    // The ten pinned inputs re-verify byte-for-byte; drift refuses,
    // unreadable is inconclusive.
    assert_eq!(check_pins(&artifact_pins()), LegVerdict::Verified);
    assert_eq!(evaluate_pins(&pins_facts()), LegVerdict::Verified);
    let mut drifted = artifact_pins();
    drifted[0].sha256 = "0".repeat(64);
    assert!(matches!(check_pins(&drifted), LegVerdict::Refused(_)));
    let mut unreadable = artifact_pins();
    unreadable[0].path = repo_root().join("does-not-exist.json");
    assert!(matches!(
        check_pins(&unreadable),
        LegVerdict::Inconclusive(_)
    ));
    for f in [
        PinsFacts {
            pins: 9,
            ..pins_facts()
        },
        PinsFacts {
            pins_verdict: "refused".into(),
            ..pins_facts()
        },
    ] {
        assert!(matches!(evaluate_pins(&f), LegVerdict::Refused(_)));
    }
    let mut f = pins_facts();
    f.manifest_sha256.clear();
    assert!(matches!(evaluate_pins(&f), LegVerdict::Inconclusive(_)));
}

#[test]
fn p50_manifest_decl_and_vocabulary() {
    // The committed template decodes strictly and its declared
    // vocabulary is the §8.2 package: one variant binding
    // `compaction_strategy`@`1.0`, deterministic, no model call,
    // `evict` only, subprocess-confined, empty requests/claims,
    // the zero code_pointer placeholder (unsealed template —
    // sealing a principal act).
    let m = committed_manifest();
    assert_eq!(m.identity.id(), "hh/hh-compact-evict-oldest");
    assert_eq!(m.tier, "C0");
    assert_eq!(m.contributions.len(), 1);
    let c = &m.contributions[0];
    assert!(matches!(c.path_or_locator, PathOrLocator::PackagePath(_)));
    let exe = c.executable.as_ref().unwrap();
    assert_eq!(exe.isolation.as_str(), "subprocess_confined");
    assert_eq!(m.requests, Requests::default());
    assert!(exe.code_pointer.digest.chars().all(|ch| ch == '0'));
    let f = manifest_decl_facts();
    // The honest admission outcome on the raw template:
    // `ContractIncompatible` — declared `"1.0"` requires vs the
    // kernel policies' supported `["1"]` — the machinery stands,
    // never an `admitted` relabel.
    assert_eq!(f.admit, "ContractIncompatible");
    assert_eq!(f.admit_detail, "dialect:hir/1 offered 1.0 vs supported [1]");
    assert_eq!(evaluate_decl(&f), LegVerdict::Verified);
    // Drifted vocabulary refuses; an absent manifest is
    // inconclusive, never verified.
    for f in [
        ManifestDeclFacts {
            plugin_id: "hh/other".into(),
            ..manifest_decl_facts()
        },
        ManifestDeclFacts {
            class_id: "control_strategy".into(),
            ..manifest_decl_facts()
        },
        ManifestDeclFacts {
            admit: "admitted".into(),
            ..manifest_decl_facts()
        },
        ManifestDeclFacts {
            admit_detail: "registry/1 offered 1.0".into(),
            ..manifest_decl_facts()
        },
        ManifestDeclFacts {
            code_pointer_zero: false,
            ..manifest_decl_facts()
        },
        ManifestDeclFacts {
            requests_empty: false,
            ..manifest_decl_facts()
        },
        ManifestDeclFacts {
            placement_preference: "in_process".into(),
            ..manifest_decl_facts()
        },
    ] {
        assert!(matches!(evaluate_decl(&f), LegVerdict::Refused(_)));
    }
    let mut f = manifest_decl_facts();
    f.decoded = false;
    f.plugin_id.clear();
    assert!(matches!(evaluate_decl(&f), LegVerdict::Inconclusive(_)));
    // The landed machinery stands — every exercise produces its
    // typed refusal on a mutated decl; nothing is admitted or
    // decoded.
    let observed = exercise_vocabulary();
    for (ex, expected) in EXERCISES {
        assert_eq!(observed.get(ex).map(String::as_str), Some(expected), "{ex}");
    }
    assert_eq!(evaluate_vocabulary(&observed), LegVerdict::Verified);
    // `decoded`/`admitted` is the machinery broken; wrong/missing →
    // refused/inconclusive, never verified.
    for (ex, o) in [
        ("unknown_member", "decoded"),
        ("local_in_process", "admitted"),
        ("empty_digest", "SchemaViolation"),
    ] {
        let mut bad = observed.clone();
        bad.insert(ex.into(), o.into());
        assert!(matches!(evaluate_vocabulary(&bad), LegVerdict::Refused(_)));
    }
    let mut missing = observed;
    missing.remove("empty_digest");
    assert!(matches!(
        evaluate_vocabulary(&missing),
        LegVerdict::Inconclusive(_)
    ));
}

#[test]
fn p50_walkthrough_absent_and_claims() {
    assert_eq!(
        evaluate_walkthrough(&walkthrough_facts()),
        LegVerdict::Verified
    );
    assert_eq!(evaluate_claims(&claims_facts()), LegVerdict::Verified);
    // Any nonzero walkthrough fact is a real walkthrough's
    // evidence — refused here, never a fabricated absence.
    for member in WALKTHROUGH_MEMBERS {
        let mut w = walkthrough_facts();
        w.counts.insert(member.into(), 1);
        assert!(matches!(evaluate_walkthrough(&w), LegVerdict::Refused(_)));
    }
    let mut missing = walkthrough_facts();
    missing.counts.remove("built");
    assert!(matches!(
        evaluate_walkthrough(&missing),
        LegVerdict::Inconclusive(_)
    ));
    for w in [
        WalkthroughFacts {
            selection_attempted: true,
            ..walkthrough_facts()
        },
        WalkthroughFacts {
            reseal_attempted: true,
            ..walkthrough_facts()
        },
    ] {
        assert!(matches!(evaluate_walkthrough(&w), LegVerdict::Refused(_)));
    }
    // A claim of any sign — or a discharged deferral — refuses,
    // never relabelled.
    for f in [
        ClaimsFacts {
            df_h9_1: "closed".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            participant_predicates: "admitted".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            outreach: "issue_comment".into(),
            ..claims_facts()
        },
        ClaimsFacts {
            claim_awarded: true,
            ..claims_facts()
        },
        ClaimsFacts {
            third_arm_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            provider_efficacy_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            publication_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            capability_claim: true,
            ..claims_facts()
        },
        ClaimsFacts {
            adoption_claim: true,
            ..claims_facts()
        },
    ] {
        assert!(matches!(evaluate_claims(&f), LegVerdict::Refused(_)));
    }
}

#[test]
fn p50_packet_unselected_honest() {
    let legs = run_packet();
    for l in &legs {
        assert!(l.verdict.is_verified(), "{}: {:?}", l.name, l.verdict);
    }
    let report = assemble(legs).unwrap();
    assert_eq!(report.version, VERSION);
    assert_eq!(report.gate, GATE);
    assert!(report.unselected && report.stage_voided && report.df_h9_1_open);
    assert_eq!(report.phases_completed, 0);
    assert_eq!(report.admission, "unselected_stage_voided");
    assert_eq!(report.disposition, Disposition::Unselected);
    let j = report.to_json().to_canonical_string();
    assert!(j.contains("\"disposition\":\"unselected\""));
    assert!(report.packet_id.chars().all(|c| c.is_ascii_hexdigit()));
    // Emit the unselected record when asked (the committed
    // artifact is produced by this exact path — never
    // hand-written).
    if let Ok(out) = std::env::var("HH_PLUGIN_UNSELECTED_EMIT") {
        let out = PathBuf::from(&out);
        let out = if out.is_absolute() {
            out
        } else {
            repo_root().join(out)
        };
        std::fs::write(&out, format!("{j}\n")).unwrap();
        eprintln!("emitted {} disposition=unselected", out.display());
    }
}

#[test]
fn p50_fold_honest_and_legs_closed() {
    let mk = |v: LegVerdict| -> Vec<Leg> { LEGS.iter().map(|n| leg(n, v.clone(), &[])).collect() };
    assert_eq!(
        assemble(mk(LegVerdict::Verified)).unwrap().disposition,
        Disposition::Unselected
    );
    assert_eq!(
        assemble(mk(LegVerdict::Inconclusive("x".into())))
            .unwrap()
            .disposition,
        Disposition::Inconclusive
    );
    // A definitive failure dominates an undetermined leg.
    let mut legs = mk(LegVerdict::Inconclusive("x".into()));
    legs[3].verdict = LegVerdict::Refused("z".into());
    assert_eq!(assemble(legs).unwrap().disposition, Disposition::Failed);
    // Malformed leg sets refuse — never a partial packet.
    assert!(assemble(mk(LegVerdict::Verified)[..5].to_vec()).is_err());
    let mut bad = mk(LegVerdict::Verified);
    bad[0].name = "not_a_leg".into();
    assert!(assemble(bad).is_err());
    // A refused leg never folds to unselected.
    let mut legs = run_packet();
    legs[4].verdict = LegVerdict::Refused("walkthrough_present".into());
    assert_eq!(assemble(legs).unwrap().disposition, Disposition::Failed);
}
