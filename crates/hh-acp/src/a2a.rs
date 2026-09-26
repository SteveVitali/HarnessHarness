//! The C2 A2A edge (§5d.4 C2 — record shapes only; the spawn/drive
//! seam is S4.6's). `AgentCard` is the attestable descriptor —
//! `version` is the deployable `bundle_id` (binding (b), never the
//! protocol or session version); `signatures[]` carries
//! `hh-ledger` audit-machinery signatures verified under a
//! `TrustRootPolicy` fixture — unsigned/foreign *content* stays
//! declared-not-evidence, and an unverifiable signature is never
//! "passed" (CC4).

use hh_compiler::acp::{project_task_state, TaskState};
use hh_identity::idp::idp_id;
use hh_ledger::audit::{parse_sig, render_sig, AuditKeyResolver, AuditSigner};
use hh_wire::json::Json;
use hh_wire::sha256::hmac_sha256;

/// `AgentCard` — the A2A descriptor record.
#[derive(Debug, Clone)]
pub struct AgentCard {
    /// The agent's display name.
    pub name: String,
    /// What the agent does (the artefact's declared surface).
    pub description: String,
    /// The endpoint the card advertises.
    pub url: String,
    /// `version` — the deployable `bundle_id` (binding (b): the
    /// version is the sealed bundle, never the session/protocol
    /// version — a peer pinning the card pins the *code*).
    pub version: String,
    /// The advertised capability names (`skills[].id` echo).
    pub capabilities: Vec<String>,
    /// `skills[]` — `{id, name, description}` records.
    pub skills: Json,
    /// `signatures[]` — `{alg, key_id, sig}` over the card's content
    /// address (empty until [`sign_card`] runs).
    pub signatures: Vec<Json>,
}

impl AgentCard {
    /// The unsigned card body (the signature preimage's subject —
    /// `signatures` excluded so sign/verify agree).
    pub fn unsigned_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("name".to_string(), Json::str(self.name.clone()));
        m.insert(
            "description".to_string(),
            Json::str(self.description.clone()),
        );
        m.insert("url".to_string(), Json::str(self.url.clone()));
        m.insert("version".to_string(), Json::str(self.version.clone()));
        m.insert(
            "capabilities".to_string(),
            Json::Arr(
                self.capabilities
                    .iter()
                    .map(|c| Json::str(c.clone()))
                    .collect(),
            ),
        );
        m.insert("skills".to_string(), self.skills.clone());
        Json::Obj(m)
    }

    /// The full card (unsigned body + `signatures[]`).
    pub fn to_json(&self) -> Json {
        let Json::Obj(mut m) = self.unsigned_json() else {
            return Json::obj([]);
        };
        m.insert("signatures".to_string(), Json::Arr(self.signatures.clone()));
        Json::Obj(m)
    }

    /// The card's content address (the derivation coordinate —
    /// `sig` covers this string's bytes).
    pub fn content_address(&self) -> String {
        idp_id(
            "a2a.agent_card",
            self.unsigned_json().to_canonical_string().as_bytes(),
        )
    }
}

/// `card(name, description, url, bundle_id, skills)` — mint the
/// unsigned card (`version = bundle_id` — binding (b)).
pub fn card(name: &str, description: &str, url: &str, bundle_id: &str, skills: Json) -> AgentCard {
    let capabilities = match &skills {
        Json::Arr(a) => a
            .iter()
            .filter_map(|s| s.get("id").and_then(Json::as_str).map(String::from))
            .collect(),
        _ => Vec::new(),
    };
    AgentCard {
        name: name.to_string(),
        description: description.to_string(),
        url: url.to_string(),
        version: bundle_id.to_string(),
        capabilities,
        skills,
        signatures: Vec::new(),
    }
}

/// `sign_card(card, signer)` — append `{alg:"hmac-sha256",
/// key_id, sig}` over the card content-address bytes (the hh-ledger
/// audit machinery — key material stays in the `AuditSigner`
/// wrapper; the card carries `key_id` + `sig`, never bytes).
pub fn sign_card(card: &mut AgentCard, signer: &mut dyn AuditSigner) -> Result<(), String> {
    let subject = card.content_address();
    let sig = signer.sign(subject.as_bytes())?;
    card.signatures.push(Json::obj([
        ("alg", Json::str("hmac-sha256")),
        ("key_id", Json::str(signer.key_id().to_string())),
        ("sig", Json::str(render_sig(&sig))),
        ("subject", Json::str(subject)),
    ]));
    Ok(())
}

/// `verify_card(card, resolver)` — fail-closed verification: every
/// signature must resolve its `key_id` under the resolver (the
/// `TrustRootPolicy` fixture — `None` → `SignerUnavailable`, never
/// ok) and match `hmac-sha256` over the content address.
pub fn verify_card(card: &AgentCard, resolver: &dyn AuditKeyResolver) -> Result<(), String> {
    if card.signatures.is_empty() {
        return Err("card_unsigned".to_string());
    }
    let subject = card.content_address();
    for sig in &card.signatures {
        if sig.get("subject").and_then(Json::as_str) != Some(subject.as_str()) {
            return Err("card_signature_subject_mismatch".to_string());
        }
        let key_id = sig
            .get("key_id")
            .and_then(Json::as_str)
            .ok_or_else(|| "card_signature_missing_key_id".to_string())?;
        let key = resolver
            .verify_key(key_id)
            .ok_or_else(|| "card_signer_unavailable".to_string())?;
        let sig_spelling = sig
            .get("sig")
            .and_then(Json::as_str)
            .ok_or_else(|| "card_signature_missing_sig".to_string())?;
        let bytes =
            parse_sig(sig_spelling).ok_or_else(|| "card_signature_bad_spelling".to_string())?;
        let expect = hmac_sha256(&key, subject.as_bytes());
        if bytes != expect.to_vec() {
            return Err("card_signature_bad".to_string());
        }
    }
    Ok(())
}

/// `delegate_task(goal, budget_slice, attenuated_grants, descriptor)`
/// — the `control.subagent.spawned`-shaped record (records-out:
/// the spawn/drive seam is S4.6's; this is the delegation envelope
/// the spawned `control.subagent.*` rows reference). `descriptor`
/// carries the peer's `AcpBinding`/card ref.
pub fn delegate_task(
    goal: &Json,
    budget_slice: &Json,
    attenuated_grants: &Json,
    descriptor: &Json,
) -> Json {
    let mut m = std::collections::BTreeMap::new();
    m.insert(
        "task_id".to_string(),
        Json::str(idp_id(
            "a2a.task",
            format!(
                "{}|{}|{}",
                goal.to_canonical_string(),
                budget_slice.to_canonical_string(),
                descriptor.to_canonical_string()
            )
            .as_bytes(),
        )),
    );
    m.insert("state".to_string(), Json::str("submitted"));
    m.insert("goal".to_string(), goal.clone());
    m.insert("budget_slice".to_string(), budget_slice.clone());
    m.insert("attenuated_grants".to_string(), attenuated_grants.clone());
    m.insert("delegated_to".to_string(), descriptor.clone());
    Json::Obj(m)
}

/// `task_state(phase, outcome, paused, auth_pending)` — the
/// exhaustive `TaskState` projection over the child's kernel
/// lifecycle coordinates (the single-source map —
/// `hh_compiler::acp::project_task_state`; `INPUT_REQUIRED` is
/// non-terminal on the wire, terminal-classified for the parent).
pub fn task_state(
    phase: &str,
    outcome: Option<&str>,
    paused: bool,
    auth_pending: bool,
) -> TaskState {
    project_task_state(phase, outcome, paused, auth_pending)
}

/// `resume_task(task_id, request_state)` — the `INPUT_REQUIRED`
/// resume record: `requestState` echoes **verbatim** (D2 — the
/// adapter treats it as opaque state, never re-serializes).
pub fn resume_task(task_id: &str, request_state: &Json) -> Json {
    Json::obj([
        ("task_id", Json::str(task_id.to_string())),
        ("state", Json::str("working")),
        ("resume", Json::Bool(true)),
        ("requestState", request_state.clone()),
    ])
}
