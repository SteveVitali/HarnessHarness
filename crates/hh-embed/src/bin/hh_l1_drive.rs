//! `hh-l1-drive` — the Ws1 L1 host surface (R6.45q; spec §7.4
//! REQ-XD-103): drive the frozen HIR doc through an in-process
//! `EmbedService` against a declared model endpoint (the ws1 replay
//! session's listener) with the exec seam bound per leg:
//!
//! - `--mode obs_replay` (L1.1): after `open_session` arms the exec
//!   caps, every bound surface's executor is swapped to one
//!   `SharedObsReplayExecutor` serving O's recorded raw observations
//!   positionally (`rebind_exec_executor` re-runs the requirement
//!   bind — an insufficiency refuses typed, never a downgrade).
//! - `--mode live` (L1.2): the session's declared environment
//!   executes — the fresh HH cell the leg names.
//!
//! The run's model calls ride the `model_port` slot — the endpoint
//! reaches it through the slot's own `env:` param resolution (the
//! host exports `HH_L1_ENDPOINT`/`HH_L1_MODEL` before open) or a
//! `--set <ptr>=<json>` literal override into the document's
//! `assembly.values` mount / doc pointer space (the widening gate
//! stays armed). The replay proxy receipts every upstream-bound
//! request — those captures are the L1Drive comparison input; this
//! surface emits the run's durable event rows + terminal state for
//! the driver's fold.
//!
//! usage:
//!   hh-l1-drive --store-root D --workspace-root D --holder S \
//!     --definition F.json [--override F.json] \
//!     --environment <connection_info json> \
//!     --exec-surfaces s1,s2 --instruction F \
//!     --endpoint URL --model M \
//!     --mode live|obs_replay [--obs raws.json] \
//!     --max-submits N --out D

use std::path::PathBuf;

use hharness_embed::service::{EmbedService, ServiceConfig};
use hharness_embed_schema::errors::EmbedError;
use hharness_env::obs_replay::{RawObservation, SharedObsReplayExecutor};
use hharness_wire::json::{parse, Json};
use hharness_wire::jsonrpc::Request;

/// The bound `ModelProfile` coordinate the pack writes into the
/// authored doc's `native.profile` pin (`{profile_id}@{version}` —
/// resolved by the registry's catalog scan, never by content hash).
const MSWEA_PROFILE_ID: &str = "hh.ws1.mswea-mini";
const MSWEA_PROFILE_VERSION: &str = "1";

/// `mswea_profile(coord)` — the `ModelProfile/1` whose `prompt_layout`
/// rule produces O's two-section preamble (the committed `r6_35`
/// fixture templates carried on `ws1:sys_tmpl`/`ws1:inst_tmpl` nodes).
/// `content_hash` computes over the record minus the member — the
/// registered body's own identity, never authored.
fn mswea_profile() -> hharness_compiler::profile::ModelProfile {
    use hharness_compiler::profile::*;
    let debt = |rule_id: &str| ProfileDebtRecord {
        rule_id: rule_id.to_string(),
        hypothesis: "L1 re-proof bind — the authored HIR's profile pin \
                     resolves to this mswea-shaped surface layer"
            .to_string(),
        evidence_refs: vec![hharness_ontology::debt::EvidenceRef::legacy(
            "hh-l1-fold:o-corpus",
        )],
        owner: "hh-l1-drive".to_string(),
        reach_via: vec![],
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::Date,
            value: Some("2099-01-01".to_string()),
        },
        removal_test_ref: "ws1:l1-fold".to_string(),
        removal_test: Some(RemovalTest {
            kind: RemovalTestKind::Inspection,
            criteria: Some("strict replay parity on the O-recorded corpus".into()),
            ..RemovalTest::new(RemovalTestKind::Inspection)
        }),
        status: DebtStatus::Active,
        debt_class: None,
        hypothesis_typed: None,
        scope: None,
        expiry: None,
        runway_ms: None,
        revalidation: None,
        created_at: None,
        supersedes: None,
    };
    let layout_params = Json::obj([
        (
            "order",
            Json::Arr(vec![
                Json::str("system"),
                Json::str("task"),
                // `held` is the non-prompt sink: it declares the
                // `transcript` slot, so the head render skips it and
                // the prior-turn fold splices at its (terminal)
                // position — O's transcript placement, unchanged.
                Json::str("held"),
            ]),
        ),
        (
            "roles",
            Json::obj([("system", Json::str("system")), ("task", Json::str("user"))]),
        ),
        (
            "section_templates",
            Json::obj([
                ("system", Json::str("ws1:sys_tmpl")),
                ("task", Json::str("ws1:inst_tmpl")),
                // `ws1:bash`'s text leaves (`purpose` +
                // `description_template` = "Execute a bash command")
                // are prompt-layout text under `text_leaves` — left
                // unplaced they route by `role_map` into `task` and
                // prefix O's user message with the tool description
                // twice. Placing the node into `held` keeps the bytes
                // exactly where O carries them: `tools[].function.
                // description`, never a message section.
                ("held", Json::str("ws1:bash")),
            ]),
        ),
        // V9 — `role_map` is total over `AuthorityClass::ALL` and
        // monotone: every class routes into `task` (rank 1), so no
        // higher-authority class ever lands below a lower one. The doc
        // places its leaves explicitly via `section_templates`; this is
        // the unplaced-node fallback route.
        (
            "role_map",
            Json::obj([
                ("unverified", Json::str("task")),
                ("external", Json::str("task")),
                ("environment", Json::str("task")),
                ("delegate", Json::str("task")),
                ("principal", Json::str("task")),
                ("definition", Json::str("task")),
                ("kernel", Json::str("task")),
            ]),
        ),
        (
            "slots",
            Json::obj([
                (
                    "task",
                    Json::Arr(vec![
                        Json::str("task"),
                        Json::str("system"),
                        Json::str("release"),
                        Json::str("version"),
                        Json::str("machine"),
                    ]),
                ),
                // `held` declares the `transcript` fill — the
                // prior-turn fold's splice marker; its placed text
                // never renders into the request head.
                ("held", Json::Arr(vec![Json::str("transcript")])),
                // V9's six kernel-reserved context slot ids — declared
                // with no fills (this profile templates none of them;
                // the lowering skips undeclared sections).
                ("kernel", Json::Arr(vec![])),
                ("definition", Json::Arr(vec![])),
                ("principal", Json::Arr(vec![])),
                ("transcript", Json::Arr(vec![])),
                ("external", Json::Arr(vec![])),
                ("unverified", Json::Arr(vec![])),
            ]),
        ),
    ]);
    let rule = |rule_id: &str, kind: ProfileRuleKind, owned: &[&str], params: Json| ProfileRule {
        rule_id: rule_id.to_string(),
        kind,
        owned_fields: owned.iter().map(|f| f.to_string()).collect(),
        params,
        debt: debt(rule_id),
        scope: None,
        supersedes: None,
        compliance: Compliance {
            detector_class: ComplianceDetector::None,
            followed_predicate_ref: None,
        },
    };
    let mut p = ModelProfile {
        profile_id: MSWEA_PROFILE_ID.to_string(),
        version: MSWEA_PROFILE_VERSION.to_string(),
        content_hash: String::new(),
        selector: ProfileSelector {
            provider_api_family: "openai-compatible".to_string(),
            model_family: "*".to_string(),
            version_pattern: VersionPattern::Any,
            precedence: 0,
            successor_ref: None,
            retirement_at: None,
            roles_admitted: vec![ModelRole::Primary],
        },
        extends: None,
        capabilities: ProfileCapabilities {
            // V4 — `PromptLayout`'s `developer_role` dependency must be
            // established, not assumed: the profile declares the
            // system-role section it emits.
            developer_role: CapabilityState::Declared,
            ..ProfileCapabilities::default()
        },
        rules: vec![rule(
            "ws1.layout",
            ProfileRuleKind::PromptLayout,
            &["prompt_layout"],
            layout_params,
        )],
        ext: std::collections::BTreeMap::new(),
        expiry: debt("hh.ws1.mswea-mini.expiry"),
        compatibility: ProfileCompatibility {
            inventory_version: "1.0".to_string(),
            min_compiler_version: "0.0.0".to_string(),
        },
        tests: Json::obj([]),
    };
    p.content_hash = profile_identity(&p);
    p
}

/// `tmpl_node(id, content, seq)` — a `definition`-authority
/// `hh-tmpl/1` `Text` carrier (the `section_templates` placement
/// shape — the `system` role requires definition authority).
fn tmpl_node(id: &str, content: &str, seq: u64) -> hharness_hir::document::Node {
    use hharness_hir::kinds::EntityKind;
    use hharness_hir::leaves::Text;
    use hharness_hir::records::{KindRecord, ToolCapabilityRecord};
    use hharness_provenance::{AuthorityClass, HumanRole, Origin, PersistenceScope};
    let prov = hharness_provenance::ProvenanceRecord::minted(
        Origin::human("hh-l1-drive:mswea-pack", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    );
    let mut purpose = Text::new(content, "hh-l1-drive:mswea-pack", prov.clone());
    purpose.authority = AuthorityClass::Definition;
    purpose.language = Some("hh-tmpl/1".to_string());
    let mut n = hharness_hir::document::Node::new(
        EntityKind::ToolCapability,
        KindRecord::ToolCapability(ToolCapabilityRecord {
            exec_defaults: None,
            purpose,
            input_schema: Json::obj([]),
            output_schema: None,
            effects: hharness_hir::kinds::ToolEffects::Pure,
            preconditions: vec![],
            scope_bindings: hharness_hir::records::ScopeBindings::Unknown,
            resources: hharness_hir::records::Resources::NoneDeclared,
            observation_contract: Json::Null,
            cost_model: None,
            execution_requirement: Json::Null,
            source: Json::Null,
            exposure_hint: Json::Null,
            postconditions: vec![],
            flow_contract: None,
            action_patterns: Vec::new(),
        }),
        prov,
    );
    n.version.semantic_id = Some(id.to_string());
    n
}

/// `bash_node(id, seq)` — the `bash` exec surface: O's tool def
/// (`{name:bash, description:"Execute a bash command",
/// parameters:{command:string,required:[command]}}` — the r6_36
/// golden) plus the `json_head_tail@1` observation renderer that
/// produces O's `{returncode, output}` tool content (research 28 §A.5).
fn bash_node(id: &str, seq: u64) -> hharness_hir::document::Node {
    use hharness_hir::kinds::{EffectClass, EffectDomain, EntityKind, ToolEffects};
    use hharness_hir::leaves::Text;
    use hharness_hir::records::{KindRecord, SurfaceRecord, ToolCapabilityRecord, ToolSurface};
    use hharness_provenance::{HumanRole, Origin, PersistenceScope};
    let prov = hharness_provenance::ProvenanceRecord::minted(
        Origin::human("hh-l1-drive:mswea-pack", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    );
    let mut n = hharness_hir::document::Node::new(
        EntityKind::ToolCapability,
        KindRecord::ToolCapability(ToolCapabilityRecord {
            exec_defaults: None,
            purpose: Text::new(
                "Execute a bash command",
                "hh-l1-drive:mswea-pack",
                prov.clone(),
            ),
            input_schema: Json::obj([
                ("type", Json::str("object")),
                (
                    "properties",
                    Json::obj([(
                        "command",
                        Json::obj([
                            ("type", Json::str("string")),
                            ("description", Json::str("The bash command to execute")),
                        ]),
                    )]),
                ),
                ("required", Json::Arr(vec![Json::str("command")])),
            ]),
            output_schema: None,
            effects: ToolEffects::Declared(
                [EffectClass {
                    domain: EffectDomain::Exec,
                    // `irreversible ∧ workspace_local ∧ non_idempotent` —
                    // the honest class for arbitrary cell bash: `rm` and
                    // writes can destroy state and no compensation plan
                    // exists (`compensable` would fabricate the
                    // ADR-0032 §1 `compensation_plan_id` the ledger
                    // demands on `prepared`; `reversible` would
                    // fabricate an undo procedure). `never_auto` gates
                    // only non-Principal proposers, so the definition's
                    // `pre_authorize` + `auto_review{max_risk:UNKNOWN}`
                    // rules admit this class through the real chain.
                    attributes: Some(hharness_hir::kinds::EffectAttributes {
                        mutability: hharness_hir::kinds::Mutability::Destructive,
                        repeat_safety: hharness_hir::kinds::RepeatSafety::NonIdempotent,
                        world: hharness_hir::kinds::World::Closed,
                        reversibility: hharness_hir::kinds::Reversibility::Irreversible,
                    }),
                }]
                .into_iter()
                .collect(),
            ),
            preconditions: vec![],
            scope_bindings: hharness_hir::records::ScopeBindings::Unknown,
            resources: hharness_hir::records::Resources::NoneDeclared,
            observation_contract: Json::Null,
            cost_model: None,
            execution_requirement: Json::Null,
            source: Json::Null,
            exposure_hint: Json::Null,
            postconditions: vec![],
            flow_contract: None,
            action_patterns: Vec::new(),
        }),
        prov.clone(),
    );
    n.surface = Some(SurfaceRecord::Tool(Box::new(ToolSurface {
        name: "bash".to_string(),
        namespace: "hh.ws1".to_string(),
        description_template: Text::new("Execute a bash command", "hh-l1-drive:mswea-pack", prov),
        argument_order: vec!["command".to_string()],
        examples: Json::Null,
        error_format: Json::Null,
        result_renderer: Json::obj([
            ("mode", Json::str("full")),
            ("kind", Json::str("json_head_tail@1")),
            ("threshold", Json::Int(10_000)),
            ("head", Json::Int(5_000)),
            ("tail", Json::Int(5_000)),
            ("unit", Json::str("chars")),
            (
                "fields",
                Json::Arr(vec![
                    Json::str("returncode"),
                    Json::obj([
                        ("name", Json::str("output")),
                        ("truncate", Json::Bool(true)),
                        ("warning", Json::str("Output too long.")),
                    ]),
                    Json::str("exception_info?"),
                ]),
            ),
            ("json_style", Json::str("python_jinja_tojson")),
        ]),
        strictness: Json::Null,
        schema_dialect_narrowing: Json::Null,
        exposure_mode: Json::Null,
        display_title: None,
        icon_ref: None,
    })));
    n.version.semantic_id = Some(id.to_string());
    n
}

/// `rule_nodes(seq)` — the two `HarnessRule` declarations an unattended
/// exec lane needs (`pre_authorize` for the covering grant, `auto_review`
/// for the ask's non-human resolution):
///
/// - `pre_authorize{grants:[exec:*], scope:"run"}` — at open the kernel
///   mints a `policy_rule`-basis `AuthorityHandle` held by the doc's
///   `AgentProcess` (§5g.1 §9 Stage-2; ADR-0053 D5). Without it step 2 of
///   `authorize` denies `NoCoveringGrant` on every effect. The `*` grant
///   scope is forced honestly: the bash capability's `command` binds no
///   canonical resource (`ScopeBindings::Unknown`), and
///   `grant_scope_covers` requires `*` exactly for unscoped args —
///   nothing narrower can cover a free-form command.
/// - `auto_review{domains:[exec], max_risk: irreversible∧external,
///   verdict: allow}` — the sealed rule the reviewer chain's
///   `auto_reviewer` stage executes (I-P2): an exec ask resolves `allow`
///   with `basis = policy_rule`. `max_risk` is the top class honestly —
///   O's recorded commands run arbitrary shell pipelines, the
///   `command_parse` classifier raises metachar commands to
///   `UNKNOWN = {irreversible, non_idempotent, external}`, and that is
///   exactly what the recorded harness admits unconditionally (the
///   session's containment — no net, workspace-only fs — is the physical
///   boundary). The conditioned-rule discipline applies —
///   `conditioned_on` names the mswea profile the admission hypothesis
///   rests on and `assumption_debt` carries the complete T-LCD-05
///   record.
fn rule_nodes(profile_coord: &str, model_id: &str, seq: u64) -> Vec<hharness_hir::document::Node> {
    use hharness_hir::kinds::{EffectClass, EffectDomain, EntityKind};
    use hharness_hir::records::{
        AssumptionDebtRecord, Grant, GrantConstraints, HarnessRuleRecord, KindRecord, RuleAction,
    };
    use hharness_hir::refs::ProfileRef;
    use hharness_ontology::debt::{
        DebtClass, DebtScope, DebtStatus, EvidenceRef, ExpiryCondition, ExpiryKind, ModelSelector,
        OwnerRef, RemovalTest, RemovalTestKind,
    };
    use hharness_provenance::{HumanRole, Origin, PersistenceScope};
    let prov = hharness_provenance::ProvenanceRecord::minted(
        Origin::human("hh-l1-drive:mswea-pack", HumanRole::Author),
        PersistenceScope::Definition,
        seq,
    );
    let rule = |rule_id: &str,
                action: RuleAction,
                conditioned: bool,
                p: &hharness_provenance::ProvenanceRecord| {
        let mut n = hharness_hir::document::Node::new(
            EntityKind::HarnessRule,
            KindRecord::HarnessRule(HarnessRuleRecord {
                rule_id: rule_id.to_string(),
                trigger: Json::Null,
                action,
                scope: Json::Null,
                conditioned_on: None,
                assumption_debt: None,
            }),
            p.clone(),
        );
        if conditioned {
            if let KindRecord::HarnessRule(r) = &mut n.semantic {
                r.conditioned_on = Some(ProfileRef {
                    profile: profile_coord.to_string(),
                    pinned: true,
                });
                // The `harness_rule/assumption_debt` home's required set:
                // base ∪ {debt_class, removal_test} ∪ model_selectors
                // (model-conditioned home — §5h.6 §2 row 1).
                r.assumption_debt = Some(AssumptionDebtRecord {
                    rule_id: rule_id.to_string(),
                    hypothesis: hharness_hir::leaves::Text::new(
                        "the replayed corpus' exec calls stay inside the cell's \
                         containment under this model — the recorded harness \
                         admits them unconditionally and O's corpus is the \
                         evidence for that admission",
                        "hh-l1-drive:mswea-pack",
                        p.clone(),
                    ),
                    evidence_refs: vec![EvidenceRef::legacy("hh-l1-fold:o-corpus")],
                    owner: OwnerRef::principal("hh-l1-drive"),
                    expiry_condition: ExpiryCondition {
                        kind: ExpiryKind::EvidenceRefreshDue,
                        value: None,
                    },
                    removal_test_ref: "ws1:l1-fold".to_string(),
                    status: DebtStatus::Active,
                    debt_class: Some(DebtClass::Hypothesized),
                    hypothesis_typed: None,
                    scope: Some(DebtScope {
                        model_selectors: vec![ModelSelector::Exact {
                            model_id: model_id.to_string(),
                        }],
                        task_classes: vec![],
                        roles: vec![],
                    }),
                    expiry: None,
                    runway_ms: None,
                    revalidation: None,
                    removal_test: Some(RemovalTest {
                        criteria: Some("strict replay parity on the O-recorded corpus".into()),
                        ..RemovalTest::new(RemovalTestKind::Inspection)
                    }),
                    created_by: None,
                    created_at: None,
                    supersedes: None,
                });
            }
        }
        n.version.semantic_id = Some(rule_id.to_string());
        n
    };
    let grant = hharness_hir::grant_json(
        &Grant {
            effect: EffectClass::domain_only(EffectDomain::Exec),
            scope: "*".to_string(),
            constraints: GrantConstraints::default(),
            delegable: false,
        },
        false,
    );
    vec![
        rule(
            "ws1:rule.exec-preauth",
            RuleAction::PreAuthorize(Json::obj(vec![
                ("grants", Json::Arr(vec![grant])),
                ("scope", Json::str("run")),
            ])),
            false,
            &prov,
        ),
        rule(
            "ws1:rule.exec-auto-review",
            RuleAction::AutoReview(Json::obj(vec![
                ("domains", Json::Arr(vec![Json::str("exec")])),
                (
                    "max_risk",
                    Json::obj([
                        ("reversibility", Json::str("irreversible")),
                        ("repeat_safety", Json::str("non_idempotent")),
                        ("scope", Json::str("external")),
                    ]),
                ),
                ("verdict", Json::str("allow")),
            ])),
            true,
            &prov,
        ),
    ]
}

/// `node_json_with_text(n)` — `node_to_json` plus the parse-side
/// `content` admission: canonical leaves emit `content_hash` only, but
/// the compiled-surface lowering reads `Text.content`. The emitted JSON
/// carries each leaf's raw text beside its hash (the codec verifies
/// `identify_text(content) == content_hash` on parse — a mismatch is a
/// `SchemaViolation`, never a silent carry).
fn node_json_with_text(n: &hharness_hir::document::Node) -> Json {
    let mut j = hharness_hir::wire::node_to_json(n);
    let mut contents: std::collections::BTreeMap<String, String> = Default::default();
    for t in n.text_leaves() {
        if let Some(c) = &t.content {
            contents.insert(t.content_hash.clone(), c.clone());
        }
    }
    fn inject(j: &mut Json, m: &std::collections::BTreeMap<String, String>) {
        match j {
            Json::Obj(map) => {
                if let Some(ch) = map.get("content_hash").and_then(Json::as_str) {
                    if let Some(c) = m.get(ch) {
                        map.insert("content".to_string(), Json::str(c.clone()));
                    }
                }
                for v in map.values_mut() {
                    inject(v, m);
                }
            }
            Json::Arr(items) => items.iter_mut().for_each(|i| inject(i, m)),
            _ => {}
        }
    }
    inject(&mut j, &contents);
    j
}

/// `apply_mswea_pack(document, dir, model_id, pack_reasoning)` — inject the surface layer the
/// authored HIR's `sha256:profile` placeholder never carried:
/// template + bash nodes, the compiled-surface lane params, and the
/// profile pin rewrite. Returns `(doc, profile_body, profile)` — the
/// caller registers `profile_body` via `lab.registry.register` and the
/// `test_profile` report `mswea_report_body` produces before
/// `open_session`.
fn apply_mswea_pack(
    document: &Json,
    dir: &str,
    model_id: &str,
    pack_reasoning: &str,
) -> Result<(Json, Json, hharness_compiler::profile::ModelProfile, String), String> {
    let sys_tmpl = std::fs::read_to_string(PathBuf::from(dir).join("system_template.tmpl"))
        .map_err(|e| format!("{dir}/system_template.tmpl: {e}"))?;
    let inst_tmpl = std::fs::read_to_string(PathBuf::from(dir).join("instance_template.tmpl"))
        .map_err(|e| format!("{dir}/instance_template.tmpl: {e}"))?;
    let Json::Obj(mut doc) = document.clone() else {
        return Err("document is not an object".to_string());
    };
    let nodes = match doc.get_mut("nodes") {
        Some(Json::Arr(v)) => v,
        _ => return Err("document.nodes is not an array".to_string()),
    };
    let bash = bash_node("ws1:bash", 103);
    // The compiled call resolves its `bash` wire name to the binding's
    // `surface_id` (`crate::surface::surface_id` over the same record the
    // exec arm binds) — the session's `supplies.host_capabilities` must
    // name that exact address or G-INTERPRET reads the call
    // `unknown_surface`.
    let bash_surface_id = {
        use hharness_hir::records::{KindRecord, SurfaceRecord};
        let (rec, surface) = match (&bash.semantic, &bash.surface) {
            (KindRecord::ToolCapability(r), Some(SurfaceRecord::Tool(t))) => (r, t),
            _ => unreachable!("bash_node always mints ToolCapability + Tool surface"),
        };
        hharness_compiler::equiv::bind_surface(&bash, surface, &rec.input_schema).surface_id
    };
    let coord = format!("{MSWEA_PROFILE_ID}@{MSWEA_PROFILE_VERSION}");
    let mut added = vec![
        tmpl_node("ws1:sys_tmpl", &sys_tmpl, 101),
        tmpl_node("ws1:inst_tmpl", &inst_tmpl, 102),
        bash,
    ];
    added.extend(rule_nodes(&coord, model_id, 104));
    for n in added {
        nodes.push(node_json_with_text(&n));
    }
    fn obj_mut<'a>(j: &'a mut Json, k: &str) -> Option<&'a mut Json> {
        match j {
            Json::Obj(m) => m.get_mut(k),
            _ => None,
        }
    }
    fn patch_slots(slots: &mut Json, touched: &mut bool, pack_reasoning: &str) {
        let params = obj_mut(slots, "model_port")
            .and_then(|s| obj_mut(s, "one"))
            .and_then(|s| obj_mut(s, "params"));
        let Some(params) = params else { return };
        if let Json::Obj(pm) = params {
            pm.insert("model_surface".to_string(), Json::str("compiled"));
            // O's recording carries `reasoning_effort` — an extension
            // member `openai-compatible@1`'s plan_schema refuses by
            // design. `openai@1` is the admitted dialect that admits it
            // (same Chat-Completions envelope/framing/rules; wider
            // plan_schema + hosted error-code spellings). The lane's
            // member is `dialect` (the authored `dialect_ref` is inert —
            // decode defaults it); the bound doc records the
            // substitution.
            pm.insert("dialect".to_string(), Json::str("openai@1"));
            // O's recorded request carries no `stream` member — the
            // family default emits `stream:false` (REQ-XE-107's
            // declared member-presence choice removes it; `transport`
            // is already `non_stream`, so `omit` is admissible).
            pm.insert("stream_member".to_string(), Json::str("omit"));
            let pp = pm
                .entry("provider_params".to_string())
                .or_insert_with(|| Json::Obj(Default::default()));
            if let Json::Obj(ppm) = pp {
                ppm.insert("reasoning_effort".to_string(), Json::str("high"));
            }
            if let Some(Json::Obj(ar)) = pm
                .get_mut("transcript_render")
                .and_then(|tr| obj_mut(tr, "assistant_replay"))
            {
                // O's GLM transcript replay: every assistant turn
                // carries `provider_specific_fields{refusal:null[,
                // reasoning]}` and positional `index` members on its
                // `tool_calls[]` — the lane's declared echo/index
                // members (REQ-XE-108), never a verbatim `null`.
                // R6.52 — `--pack-reasoning`: O's baseline replay pins
                // `provider_specific_fields`; the V5 `rsn-drop` level's
                // sealed doc declares `drop` and the pack must not
                // silently flatten it back.
                ar.insert("reasoning".to_string(), Json::str(pack_reasoning));
                ar.insert("tool_call_index".to_string(), Json::str("emit"));
                ar.insert("provider_specific_fields".to_string(), Json::str("echo"));
            }
            *touched = true;
        }
    }
    let mut touched = false;
    if let Some(asm) = doc.get_mut("assembly") {
        if let Some(slots) = obj_mut(asm, "slots") {
            patch_slots(slots, &mut touched, pack_reasoning);
        }
    }
    if let Some(Json::Arr(items)) = doc.get_mut("nodes") {
        for n in items.iter_mut() {
            if n.get("kind").and_then(Json::as_str) != Some("AgentProcess") {
                continue;
            }
            let native = obj_mut(n, "semantic")
                .and_then(|s| obj_mut(s, "body"))
                .and_then(|b| obj_mut(b, "native"));
            if let Some(Json::Obj(native)) = native {
                if let Some(Json::Obj(p)) = native.get_mut("profile") {
                    p.insert("profile_ref".to_string(), Json::str(coord.clone()));
                }
                if let Some(slots) = native.get_mut("slots") {
                    patch_slots(slots, &mut touched, pack_reasoning);
                }
            }
        }
    }
    if !touched {
        return Err("no model_port slot binding found to arm".to_string());
    }
    let profile = mswea_profile();
    let body = Json::obj([
        ("kind", Json::str("model_profile")),
        (
            "profile",
            hharness_compiler::schema::profile_to_json(&profile),
        ),
    ]);
    Ok((Json::Obj(doc), body, profile, bash_surface_id))
}

/// `ProbeView` — the fabricated-report `ProfileView` for the observed
/// compile. The `passing_for` report is fixture plumbing (the same
/// trick `hh-compiler`'s own test view uses): it admits the link gate so
/// the real lowering runs and yields the golden addresses. It never
/// registers — the record the drive publishes is the `test_profile`
/// suite's own verdict over real fixtures.
struct ProbeView {
    coord: String,
    profile: hharness_compiler::profile::ModelProfile,
    report: hharness_compiler::profile_test::ProfileTestReport,
}

impl hharness_compiler::profile::ProfileView for ProbeView {
    fn profile(&self, coordinate: &str) -> Option<hharness_compiler::profile::ModelProfile> {
        (coordinate == self.coord || coordinate == self.profile.content_hash)
            .then(|| self.profile.clone())
    }
    fn test_report(
        &self,
        coordinate: &str,
    ) -> Option<hharness_compiler::profile_test::ProfileTestReport> {
        (coordinate == self.coord || coordinate == self.profile.content_hash)
            .then(|| self.report.clone())
    }
}

/// `ProbeVariants` — the pinned-`version_id` `VariantView` over the
/// variant records the probe store's own resolution produced.
struct ProbeVariants(std::collections::BTreeMap<String, hharness_registry::records::VariantRecord>);

impl hharness_compiler::link::VariantView for ProbeVariants {
    fn variant(&self, version_id: &str) -> Option<hharness_registry::records::VariantRecord> {
        self.0.get(version_id).cloned()
    }
}

/// `mswea_report_body(probe_root, doc, profile)` — ADR-0125's link
/// gate admits a bound profile only beside a registry-carried
/// `ProfileTestReport` whose V1–V9 all pass. The honest inputs:
///
///   1. a throwaway `EmbedService` seeds a scratch registry (the same
///      seed set `open` builds), then `assembly::resolve` seals the
///      derived doc **in memory** — the `hh-tmpl/1` leaves keep their
///      content, which canonical bytes never emit;
///   2. a real `compile` (twice) yields the observed golden surfaces
///      and two determinism witnesses (`CompiledBundle.bundle_id` —
///      `idp/1` over canonical outputs);
///   3. `test_profile(profile, fixtures)` computes the real V1–V9
///      verdicts — V5's `recorded` golden is this publish's observed
///      addresses (first registration — nothing earlier to drift from).
///
/// Returns the `profile_test_report` registry body; the caller refuses
/// the run (never fabricates a pass) when any validity section fails.
fn mswea_report_body(
    probe_root: &PathBuf,
    doc_json: &Json,
    profile: &hharness_compiler::profile::ModelProfile,
) -> Result<Json, String> {
    use hharness_compiler::profile::profile_coordinate;
    use hharness_compiler::profile_test::{
        test_profile, test_report_json, GoldenSurfaces, ProfileTestFixtures, ProfileTestReport,
    };

    let coord = profile_coordinate(profile);
    let kernel = hharness_provenance::ProvenanceRecord::kernel("hharness-embed", 0);
    let catalog = hharness_assembly::catalog::Stage1Catalog::stage1();

    // ── 1. seed the scratch registry (EmbedService::open's own seeds),
    //      then resolve the derived doc in memory — the sealed doc the
    //      real open produces, with its template text intact ──────────
    let _ = std::fs::remove_dir_all(probe_root);
    let probe = EmbedService::open(ServiceConfig {
        store_root: probe_root.clone(),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: probe_root.clone(),
        holder: "hh-l1-drive:probe".into(),
    })
    .map_err(|e| format!("probe service: {e:?}"))?;
    drop(probe);
    let mut probe_registry =
        hharness_registry::store::RegistryStore::open(probe_root.join("registry"), &kernel)
            .map_err(|e| format!("probe registry: {e:?}"))?;
    // R7.3 (manifest 329) — the sealed P4 level docs pin the sweep
    // store's variant version_ids (`hh/compactor` foremost); the
    // Stage-1 seeds alone never carried them. Register the lab's
    // pass-2 sweep records (one constructor home, CC1) so the probe
    // resolve admits the sealed pins — no republish, the sealed docs
    // resolve by version_id, never by name selector.
    hharness_lab::p4::sweep_seed_into_p4(&mut probe_registry);
    let doc = hharness_hir::wire::document_from_json(doc_json)
        .map_err(|e| format!("derived doc parse: {e:?}"))?;
    let sealed = {
        let mut env = hharness_assembly::resolve::ResolveEnv {
            registry: &mut probe_registry,
            catalog: &catalog,
            snapshot_id: None,
            mode: hharness_identity::names::ResolveMode::Audit,
            registrar: kernel.clone(),
            resolved_at: 0,
            notices: None,
        };
        hharness_assembly::resolve(&doc, &mut env).map_err(|d| format!("probe resolve: {d:?}"))?
    };

    // ── 3. the resolved variant set the sealed doc pins ──────────────
    let mut pinned: std::collections::BTreeSet<String> = Default::default();
    fn collect_variants(j: &Json, out: &mut std::collections::BTreeSet<String>) {
        match j {
            Json::Obj(m) => {
                if m.contains_key("variant_id") {
                    if let Some(v) = m.get("version_id").and_then(Json::as_str) {
                        out.insert(v.to_string());
                    }
                }
                for v in m.values() {
                    collect_variants(v, out);
                }
            }
            Json::Arr(items) => items.iter().for_each(|i| collect_variants(i, out)),
            _ => {}
        }
    }
    for n in &sealed.document.nodes {
        collect_variants(&hharness_hir::wire::node_to_json(n), &mut pinned);
    }
    if let Json::Obj(m) = &doc_slots_json(&sealed) {
        collect_variants(&Json::Obj(m.clone()), &mut pinned);
    }
    // The service's registry lives under `store_root/registry` — the
    // same layout `EmbedService::build` opens and seeds.
    let probe_registry =
        hharness_registry::store::RegistryStore::open(probe_root.join("registry"), &kernel)
            .map_err(|e| format!("probe registry: {e:?}"))?;
    let mut variants = ProbeVariants(Default::default());
    for vid in &pinned {
        match probe_registry.get(vid).map(|(_, r)| r) {
            Some(hharness_registry::records::RegistryRecord::Variant(v)) => {
                variants.0.insert(vid.clone(), v.clone());
            }
            _ => {
                return Err(format!(
                    "sealed pin {vid} resolves no variant in probe store"
                ))
            }
        }
    }

    // ── 4. the observed compile (twice — V6's witnesses) ─────────────
    let view = ProbeView {
        coord: coord.clone(),
        profile: profile.clone(),
        report: ProfileTestReport::passing_for(&coord, &profile.content_hash),
    };
    let compile_once = |variants: &ProbeVariants| {
        hharness_compiler::compile(
            &hharness_compiler::CompileInputs {
                sealed: sealed.clone(),
                profile_refs: vec![coord.clone()],
                fallback_profile: None,
                targets: vec![],
                compile_for_expired: false,
                intent_ref: None,
            },
            &view,
            variants,
            &catalog,
            &kernel,
        )
    };
    let bundle1 = compile_once(&variants).map_err(|e| format!("probe compile: {e:?}"))?;
    let bundle2 = compile_once(&variants).map_err(|e| format!("probe compile 2: {e:?}"))?;
    let hharness_compiler::seal::ModelSurfaceState::Lowered(surface) = &bundle1.model_surface
    else {
        return Err("probe compile produced no lowered surface".to_string());
    };
    let observed_surfaces: std::collections::BTreeMap<String, String> = surface
        .tools
        .iter()
        .map(|t| {
            (
                format!("{}∥{}", t.binding.hir_node_id, t.binding.surface_name),
                hharness_identity::idp::idp_id(
                    "golden.surface",
                    format!(
                        "{}∥{}∥{}∥{}",
                        t.binding.surface_id,
                        t.binding.surface_name,
                        t.schema.to_canonical_string(),
                        t.description
                    )
                    .as_bytes(),
                ),
            )
        })
        .collect();
    let layout_hash = hharness_identity::idp::idp_id(
        "golden.layout",
        Json::Arr(
            surface
                .layout
                .iter()
                .map(|s| Json::str(s.section_id.clone()))
                .collect(),
        )
        .to_canonical_string()
        .as_bytes(),
    );
    let renderer_hash = hharness_identity::idp::idp_id(
        "golden.renderer",
        surface.transcript_renderer.to_canonical_string().as_bytes(),
    );

    // ── 5. the real suite over real fixtures ─────────────────────────
    let mut fixtures = ProfileTestFixtures {
        observed_surfaces: observed_surfaces.clone(),
        observed_layout_hash: layout_hash.clone(),
        observed_renderer_spec_hash: renderer_hash.clone(),
        determinism_witnesses: vec![bundle1.bundle_id.clone(), bundle2.bundle_id.clone()],
        ..ProfileTestFixtures::default()
    };
    fixtures.recorded = GoldenSurfaces {
        surfaces: observed_surfaces,
        layout_hash,
        renderer_spec_hash: renderer_hash,
    };
    let produced_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let report = test_profile(profile, &fixtures, produced_at);
    if !report.validity_ok() {
        let failed: Vec<String> = report
            .validity
            .iter()
            .filter(|s| {
                !matches!(
                    s.verdict,
                    hharness_compiler::profile_test::ValidityVerdict::Pass
                )
            })
            .map(|s| format!("{}:{:?}", s.id.name(), s.verdict))
            .collect();
        return Err(format!("profile test suite fails: {}", failed.join(", ")));
    }

    Ok(Json::obj([
        ("kind", Json::str("profile_test_report")),
        ("target", Json::obj([("profile_ref", Json::str(coord))])),
        ("report", test_report_json(&report)),
    ]))
}

/// `doc_slots_json` — the sealed doc's `assembly.slots` member, for the
/// pinned-variant scan (the resolver materialises it beside
/// `native.slots`; both may carry pins).
fn doc_slots_json(sealed: &hharness_hir::document::SealedDefinition) -> Json {
    sealed
        .document
        .assembly
        .as_ref()
        .and_then(|a| a.get("slots").cloned())
        .unwrap_or(Json::Null)
}

fn usage() -> ! {
    eprintln!("{}", std::env::args().next().unwrap());
    eprintln!(
        "usage: hh-l1-drive --store-root D --workspace-root D --holder S \
         --definition F [--override F] --environment J \
         --exec-surfaces CSV --instruction F --endpoint URL --model M \
         --mode live|obs_replay [--obs F] [--max-submits N] --out D"
    );
    std::process::exit(2)
}

fn req_arg(args: &[String], name: &str) -> String {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| {
            eprintln!("missing {name}");
            usage()
        })
}

fn opt_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn read_json_file(p: &str) -> Result<Json, String> {
    let b = std::fs::read(p).map_err(|e| format!("{p}: {e}"))?;
    parse(&String::from_utf8_lossy(&b)).map_err(|e| format!("{p}: {e:?}"))
}

fn call(svc: &mut EmbedService, method: &str, params: Json) -> Result<Json, EmbedError> {
    let r = svc.handle(&Request {
        id: Json::str(format!("l1-{method}")),
        method: method.to_string(),
        params,
    });
    if let Some(e) = r.get("error") {
        return Err(EmbedError::Refused {
            reason: e.to_canonical_string(),
        });
    }
    Ok(r.get("result").cloned().unwrap_or(Json::Null))
}

/// `exec_arm_decl` — the `open_session.exec` declaration's surface
/// list. The pack's own `ws1:bash` is the exec surface its calls
/// resolve to — an un-armed pack leaves `exec_caps` empty: the uname
/// probe mints `probe_unbacked` and the first request's `{{system}}`
/// fill refuses `template_slot_unfilled` before any model dispatch.
/// An explicit `--exec-surfaces` overrides verbatim; no pack and no
/// flag stays `None` (byte-identical legacy).
fn exec_arm_decl(surfaces_csv: &str, bash_surface_id: &Option<String>) -> Option<Json> {
    let surfaces: Vec<Json> = if surfaces_csv.is_empty() {
        if bash_surface_id.is_some() {
            vec![Json::str("ws1:bash")]
        } else {
            vec![]
        }
    } else {
        surfaces_csv
            .split(',')
            .filter(|s| !s.is_empty())
            .map(Json::str)
            .collect()
    };
    if surfaces.is_empty() {
        None
    } else {
        Some(Json::obj([
            ("env", Json::str("session")),
            ("surfaces", Json::Arr(surfaces)),
        ]))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let store_root = PathBuf::from(req_arg(&args, "--store-root"));
    let ws_root = PathBuf::from(req_arg(&args, "--workspace-root"));
    let holder = req_arg(&args, "--holder");
    let definition_path = req_arg(&args, "--definition");
    let environment = opt_arg(&args, "--environment").unwrap_or_else(|| {
        "{\"kind\":\"connection_info\",\"connection_info\":{\"class\":\"local_host\"}}".to_string()
    });
    let surfaces_csv = opt_arg(&args, "--exec-surfaces").unwrap_or_default();
    let instruction_path = req_arg(&args, "--instruction");
    let endpoint = req_arg(&args, "--endpoint");
    let model = req_arg(&args, "--model");
    let mode = req_arg(&args, "--mode");
    let max_submits: usize = opt_arg(&args, "--max-submits")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let out_dir = PathBuf::from(req_arg(&args, "--out"));
    let overrides_path = opt_arg(&args, "--override");
    let mswea_pack = opt_arg(&args, "--mswea-pack");
    // R6.52 — the pack's assistant-replay `reasoning` spelling: O's
    // baseline is `provider_specific_fields`; the V5 `rsn-drop` level
    // drives with `--pack-reasoning drop` (the sealed level's own
    // param the pack must not flatten).
    let pack_reasoning = opt_arg(&args, "--pack-reasoning")
        .unwrap_or_else(|| "provider_specific_fields".to_string());

    if !["live", "obs_replay"].contains(&mode.as_str()) {
        eprintln!("mode must be live|obs_replay");
        std::process::exit(2);
    }
    let raws: Option<Vec<RawObservation>> = if mode == "obs_replay" {
        let p = req_arg(&args, "--obs");
        match read_json_file(&p) {
            Ok(Json::Arr(items)) => Some(
                items
                    .iter()
                    .map(|j| RawObservation {
                        output: j
                            .get("output")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        returncode: j
                            .get("returncode")
                            .and_then(Json::as_int)
                            .unwrap_or_default(),
                        exception_info: j.get("exception_info").cloned(),
                    })
                    .collect(),
            ),
            other => {
                eprintln!("--obs must be a JSON array ({other:?})");
                std::process::exit(2)
            }
        }
    } else {
        None
    };

    // The `model_port` slot's `env:` params resolve at arm from the
    // host environment — export the realized endpoint + model (the
    // slot binds `env:HH_L1_ENDPOINT`/`env:HH_L1_MODEL` spellings; a
    // doc that binds literals instead needs `--override` values or a
    // pointer write — the widening gate stays armed either way).
    // SAFETY: single-threaded argument setup before the service opens.
    unsafe {
        std::env::set_var("HH_L1_ENDPOINT", &endpoint);
        std::env::set_var("HH_L1_MODEL", &model);
    }

    let document = match read_json_file(&definition_path) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("definition: {e}");
            std::process::exit(2)
        }
    };
    // `--mswea-pack D` — the authored HIR's `sha256:profile` pin is a
    // placeholder; the pack injects O's surface layer (template +
    // `bash` nodes, `model_surface:"compiled"`, the profile record
    // itself) and rewrites the pin to the registered coordinate.
    let (document, mswea_profile_body, mswea_profile, bash_surface_id) = match &mswea_pack {
        Some(dir) => match apply_mswea_pack(&document, dir, &model, &pack_reasoning) {
            Ok((d, b, p, s)) => (d, Some(b), Some(p), Some(s)),
            Err(e) => {
                eprintln!("mswea-pack: {e}");
                std::process::exit(2)
            }
        },
        None => (document, None, None, None),
    };
    let overrides = match &overrides_path {
        Some(p) => match read_json_file(p) {
            Ok(Json::Arr(a)) => a,
            _ => {
                eprintln!("--override must be a JSON array of Override docs");
                std::process::exit(2)
            }
        },
        None => vec![],
    };
    let instruction = match std::fs::read_to_string(&instruction_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("instruction: {e}");
            std::process::exit(2)
        }
    };
    let env_json = match parse(&environment) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("environment json: {e:?}");
            std::process::exit(2)
        }
    };

    // The probe compile re-parses the derived doc (the text-content
    // admission round-trips it) — keep a copy before it moves into
    // `spec`.
    let doc_for_probe = document.clone();
    let spec_members = vec![
        ("kind".to_string(), Json::str("new")),
        (
            "definition".to_string(),
            Json::obj([("kind", Json::str("document")), ("document", document)]),
        ),
        ("overrides".to_string(), Json::Arr(overrides)),
        ("environment".to_string(), env_json),
        // The pack replays a headless corpus — `unattended` is the
        // honest attendance (no human answers asks), and
        // `approval_mode:"auto_review"` lowers to Π-12's `auto_review`
        // member: the sealed `HarnessRule{auto_review}` the pack injects
        // endorses the asks the recorded harness took unconditionally
        // (R-2.8.7 I-P2). An un-packed drive keeps `async`/unset.
        (
            "attendance".to_string(),
            Json::obj([
                (
                    "value",
                    Json::str(if mswea_pack.is_some() {
                        "unattended"
                    } else {
                        "async"
                    }),
                ),
                ("source", Json::str("declared")),
            ]),
        ),
    ];
    let mut spec_members = spec_members;
    if mswea_pack.is_some() {
        spec_members.push(("approval_mode".to_string(), Json::str("auto_review")));
    }
    if let Some(sid) = &bash_surface_id {
        // `supplies.host_capabilities` feeds `driver_surfaces` — the
        // `delivered_surfaces` set G-INTERPRET validates the emitted call
        // against. The armed exec cap alone does not deliver the surface
        // to the interpret check; without this entry the compiled call
        // rejects `unknown_surface`. `irreversible`/`workspace_local` is
        // the honest class for a cell bash under a non-approval supply —
        // the same class the capability declares (no compensation plan
        // exists; ADR-0032 §1).
        spec_members.push((
            "supplies".to_string(),
            Json::obj([
                ("context", Json::Arr(vec![])),
                (
                    "host_capabilities",
                    Json::Arr(vec![Json::obj([
                        ("capability_id", Json::str("ws1:bash")),
                        ("surface_id", Json::str(sid.clone())),
                        ("requires_approval", Json::Bool(false)),
                        (
                            "risk_class",
                            Json::obj([
                                ("reversibility", Json::str("irreversible")),
                                ("repeat_safety", Json::str("non_idempotent")),
                                ("scope", Json::str("workspace_local")),
                            ]),
                        ),
                    ])]),
                ),
                ("mcp_servers", Json::Arr(vec![])),
                ("procedures", Json::Arr(vec![])),
            ]),
        ));
    }
    let spec = Json::Obj(spec_members.into_iter().collect());
    // `exec` rides `open_session`'s params (not `spec`) — R6.38's arm
    // declaration for the definition's named ToolCapability surfaces.
    let exec_decl = exec_arm_decl(&surfaces_csv, &bash_surface_id);

    // R7.3 (manifest 329) — pre-seed the session store's registry with
    // the lab's pass-2 sweep records *before* `EmbedService::open`
    // installs the Stage-1 seeds (idempotent on both sides — the check
    // is the deterministic version_id). The sealed P4 level docs pin
    // sweep version_ids the Stage-1 set never carried; session
    // resolve/open would otherwise surface `RefUnresolved` for
    // `hh/compactor` exactly as the probe did.
    {
        let seed_kernel = hharness_provenance::ProvenanceRecord::kernel("hh-l1-drive", 0);
        let mut session_registry = hharness_registry::store::RegistryStore::open(
            store_root.join("registry"),
            &seed_kernel,
        )
        .expect("session registry opens for the p4 sweep seed");
        hharness_lab::p4::sweep_seed_into_p4(&mut session_registry);
    }

    let mut svc = match EmbedService::open(ServiceConfig {
        store_root: store_root.clone(),
        kernel_version_id: "hh-kernel/0.1.0".into(),
        workspace_root: ws_root,
        holder,
    }) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("service open: {e:?}");
            std::process::exit(1)
        }
    };

    // The boundary refuses every op `NotInitialized` until `hello`
    // completes the negotiation (§7.4 §2.3) — the drive is a client
    // like any other and must handshake before `open_session`.
    let hello_params = Json::obj([
        ("contract_major", Json::Int(1)),
        (
            "client",
            Json::obj([
                ("name", Json::str("hh-l1-drive")),
                ("version", Json::str(env!("CARGO_PKG_VERSION"))),
                ("kind", Json::str("lab")),
            ]),
        ),
        // `experimental` + `serves_measurement` admit the
        // `lab.registry.*` ops — the pack's profile registration
        // needs them; the surface arm itself is stable-tier.
        (
            "capabilities",
            Json::obj([
                ("experimental", Json::Bool(true)),
                ("serves_measurement", Json::Bool(true)),
            ]),
        ),
    ]);
    match call(&mut svc, "hello", hello_params.clone()) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("hello: {e:?}");
            std::process::exit(1)
        }
    }

    // The pack's `ModelProfile/1` registers before open — the doc's
    // rewritten `native.profile` pin resolves it by coordinate through
    // the same catalog scan `pinned_profile_refs` feeds (CC1 — the
    // registry's own admission, never a second binding path).
    if let Some(body) = &mswea_profile_body {
        let registrar = hharness_provenance::ProvenanceRecord::kernel("hh-l1-drive", 0);
        match call(
            &mut svc,
            "lab.registry.register",
            Json::obj([
                ("kind", Json::str("model_profile")),
                ("body", body.clone()),
                ("registrar", registrar.to_json()),
            ]),
        ) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("lab.registry.register(model_profile): {e:?}");
                std::process::exit(1)
            }
        }
    }

    let mut open_params = Json::obj([("spec", spec), ("idempotency_key", Json::str("l1-drive"))]);
    if let (Some(e), Json::Obj(m)) = (exec_decl, &mut open_params) {
        m.insert("exec".into(), e);
    }

    // ADR-0125's link gate admits the bound profile only beside a
    // registry-carried `ProfileTestReport` — the pack's probe compile
    // produces the honest observed golden + witnesses, `test_profile`
    // the real verdicts; a failed section refuses the run.
    if let Some(profile) = &mswea_profile {
        let probe_root = store_root.with_file_name(format!(
            "{}-probe",
            store_root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("store")
        ));
        let report_body = match mswea_report_body(&probe_root, &doc_for_probe, profile) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("mswea profile test report: {e}");
                std::process::exit(1)
            }
        };
        let registrar = hharness_provenance::ProvenanceRecord::kernel("hh-l1-drive", 0);
        match call(
            &mut svc,
            "lab.registry.register",
            Json::obj([
                ("kind", Json::str("profile_test_report")),
                ("body", report_body),
                ("registrar", registrar.to_json()),
            ]),
        ) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("lab.registry.register(profile_test_report): {e:?}");
                std::process::exit(1)
            }
        }
    }

    let opened = match call(&mut svc, "open_session", open_params) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("open_session: {e:?}");
            std::process::exit(1)
        }
    };
    let sess_id = opened
        .get("session_id")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    let run_id = opened
        .get("run_id")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();

    // L1.1 — the observation-replay executor double: one shared queue
    // across every armed exec surface (spec §7.4: "an
    // observation-replay executor fed O's raw observations").
    if let Some(raws) = raws {
        let shared = SharedObsReplayExecutor::new(raws);
        for sid in svc.exec_surface_ids(&sess_id) {
            if let Err(e) = svc.rebind_exec_executor(&sess_id, &sid, Box::new(shared.clone())) {
                eprintln!("rebind {sid}: {e:?}");
                std::process::exit(1)
            }
        }
    }

    let mut finished = false;
    let mut submits = 0;
    while submits < max_submits && !finished {
        submits += 1;
        match call(
            &mut svc,
            "submit",
            Json::obj([
                ("session_id", Json::str(&sess_id)),
                (
                    "input",
                    Json::Arr(vec![Json::obj([
                        ("kind", Json::str("text")),
                        ("text", Json::str(&instruction)),
                    ])]),
                ),
                ("idempotency_key", Json::str(format!("l1-submit-{submits}"))),
            ]),
        ) {
            Ok(_) => {}
            Err(EmbedError::Draining) => {
                finished = true;
            }
            Err(e) => {
                eprintln!("submit {submits}: {e:?}");
                std::process::exit(1)
            }
        }
        if let Some((fin, active)) = svc.session_state(&sess_id) {
            finished = finished || fin;
            // A parked turn awaiting input means the drive went off
            // the recorded trajectory — stop honestly, the diff is
            // the fold's.
            if !finished && !active {
                break;
            }
        }
    }

    std::fs::create_dir_all(&out_dir).ok();
    let events = svc
        .surface_events(&run_id)
        .unwrap_or_default()
        .iter()
        .map(|e| {
            Json::obj([
                ("class", Json::str(&e.class)),
                ("payload", e.payload.clone()),
            ])
        })
        .collect::<Vec<_>>();
    let events_j = Json::Arr(events);
    let _ = std::fs::write(out_dir.join("events.json"), events_j.to_canonical_string());
    let report = Json::obj([
        ("schema", Json::str("hh-l1-drive-report/1")),
        ("run_id", Json::str(&run_id)),
        ("session_id", Json::str(&sess_id)),
        ("mode", Json::str(&mode)),
        ("endpoint", Json::str(&endpoint)),
        ("submits", Json::Int(submits as i64)),
        ("finished", Json::Bool(finished)),
    ]);
    let _ = std::fs::write(out_dir.join("drive.json"), report.to_canonical_string());
    println!(
        "{}",
        Json::obj([
            ("run_id", Json::str(&run_id)),
            ("finished", Json::Bool(finished)),
            ("submits", Json::Int(submits as i64)),
        ])
        .to_canonical_string()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_arm_decl_pack_defaults_to_bash_surface() {
        let decl = exec_arm_decl("", &Some("sha256:abc".to_string()))
            .expect("pack present arms its own bash surface");
        assert_eq!(
            decl.to_canonical_string(),
            "{\"env\":\"session\",\"surfaces\":[\"ws1:bash\"]}"
        );
    }

    #[test]
    fn exec_arm_decl_explicit_csv_overrides() {
        let decl = exec_arm_decl("s1,s2", &Some("sha256:abc".to_string()))
            .expect("explicit surfaces arm verbatim");
        assert_eq!(
            decl.to_canonical_string(),
            "{\"env\":\"session\",\"surfaces\":[\"s1\",\"s2\"]}"
        );
    }

    #[test]
    fn exec_arm_decl_no_pack_no_flag_stays_unarmed() {
        assert!(exec_arm_decl("", &None).is_none());
    }

    // R6.52 — the V5 `rsn-drop` level: `--pack-reasoning drop` keeps the
    // sealed level's own assistant-replay param; the default still pins
    // O's `provider_specific_fields` replay.
    #[test]
    fn mswea_pack_reasoning_flag_respects_the_level() {
        let dir = std::env::temp_dir().join(format!("l1-pack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("system_template.tmpl"), "sys {{instruction}}").unwrap();
        std::fs::write(dir.join("instance_template.tmpl"), "inst {{task}}").unwrap();
        let port = |r: &str| {
            format!(
                "{{\"model_port\":{{\"one\":{{\"params\":{{\"transcript_render\":{{\"assistant_replay\":{{\"reasoning\":\"{r}\"}}}}}}}}}}}}"
            )
        };
        let doc = parse(&format!(
            "{{\"nodes\":[],\"assembly\":{{\"slots\":{}}}}}",
            port("verbatim")
        ))
        .unwrap();
        let got = |reasoning: &str| {
            let (d, _, _, _) =
                apply_mswea_pack(&doc, dir.to_str().unwrap(), "m-test", reasoning).unwrap();
            d.get("assembly")
                .and_then(|a| a.get("slots"))
                .and_then(|s| s.get("model_port"))
                .and_then(|p| p.get("one"))
                .and_then(|p| p.get("params"))
                .and_then(|p| p.get("transcript_render"))
                .and_then(|t| t.get("assistant_replay"))
                .and_then(|a| a.get("reasoning"))
                .and_then(Json::as_str)
                .map(str::to_string)
        };
        assert_eq!(
            got("provider_specific_fields").as_deref(),
            Some("provider_specific_fields")
        );
        assert_eq!(got("drop").as_deref(), Some("drop"));
    }
}
