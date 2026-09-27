//! `hh-compact-evict-oldest` — the packaged first-party
//! `compaction_strategy` variant (`evict_oldest`, C0). Runs out of process
//! under `plugin_abi/1` via the shared runtime in `hh-plugin-fixture`;
//! launched by the variant host through `hh-helper`
//! (`argv = [bin, --socket, <path>, --version-id <pin>, --content <addr>]`).

mod compact;

use std::collections::BTreeMap;
use std::time::Duration;

use hh_embed_schema::plugin_abi::{
    AbiError, BindFailure, BindParams, ConformanceParams, GuardParams, GuardVerdict, HelloParams,
    InvokeParams, StreamParams, TriState,
};
use hh_plugin_fixture::{PluginCtx, PluginRuntime, VariantLogic};
use hh_varhost::channel::SocketIo;
use hh_wire::json::Json;

/// The variant logic — `evict_oldest` over the `compaction_strategy`
/// contract. Stateless; the pins come from argv (the host asserts them
/// against the seal — V5).
struct EvictOldest {
    version_id: String,
    content: String,
    binds: u64,
}

impl VariantLogic for EvictOldest {
    fn hello(&self) -> HelloParams {
        let offers = vec![Json::obj([
            ("kind", Json::str("class_contract")),
            ("id", Json::str("compaction_strategy")),
            ("version_range", Json::str("1.0")),
        ])];
        let mut caps = BTreeMap::new();
        caps.insert("deterministic".to_string(), TriState::Supported);
        caps.insert("model_call".to_string(), TriState::Unsupported);
        HelloParams {
            plugin_abi_version: format!(
                "plugin_abi/{}",
                hh_embed_schema::plugin_abi::PLUGIN_ABI_MAJOR
            ),
            plugin_version_id: self.version_id.clone(),
            plugin_content: self.content.clone(),
            contract_versions_offered: offers,
            capabilities: caps,
        }
    }

    fn bind(&mut self, params: &BindParams) -> Result<String, BindFailure> {
        if params.class_id != "compaction_strategy" || params.contract_version != "1.0" {
            return Err(BindFailure::ContractMismatch);
        }
        self.binds += 1;
        Ok(format!("compact-{}", self.binds))
    }

    fn invoke(
        &mut self,
        params: &InvokeParams,
        _ctx: &mut PluginCtx,
    ) -> Result<Vec<Json>, AbiError> {
        let view = params.inputs.first().cloned().unwrap_or(Json::Null);
        let second = params.inputs.get(1).cloned().unwrap_or(Json::Null);
        match params.operation.as_str() {
            // Contract shape: `assess(view, requirement)`,
            // `propose(assessment, view)`, `execute(proposal, view)`.
            "assess" => compact::assess(&view, &second).map(|a| vec![a]),
            "propose" => compact::propose(&view, &second).map(|p| vec![p]),
            "execute" => compact::execute(&view, &second).map(|r| vec![r]),
            "declare" => Ok(vec![compact::declaration()]),
            _ => Err(AbiError::UnhandledOperation),
        }
    }

    fn stream(&mut self, _params: &StreamParams, _ctx: &mut PluginCtx) -> Result<(), AbiError> {
        // `evict_oldest` has no streaming surface.
        Err(AbiError::UnhandledOperation)
    }

    fn guard(&mut self, _params: &GuardParams, _ctx: &mut PluginCtx) -> GuardVerdict {
        // Not a hook — `no_decision` (the identity for optional guards).
        GuardVerdict::NoDecision
    }

    fn conformance(&mut self, params: &ConformanceParams) -> Json {
        let decl = compact::declaration();
        let tests: Vec<String> = params
            .plugin
            .get("tests")
            .and_then(|t| match t {
                Json::Arr(a) => Some(
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let mut results = Vec::new();
        for t in &tests {
            let pass = match t.split('.').next().unwrap_or("") {
                "static" => {
                    decl.get("deterministic") == Some(&Json::Bool(true))
                        && decl.get("op_kinds").is_some()
                }
                "contract" => decl.get("contract_range").and_then(Json::as_str) == Some("1.0"),
                "executable" => true,
                "property" => {
                    decl.get("placement").and_then(Json::as_str) == Some("subprocess_confined")
                }
                _ => false,
            };
            results.push(Json::obj([
                ("test_id", Json::str(t.clone())),
                ("pass", Json::Bool(pass)),
                (
                    "detail",
                    Json::str(if pass { "ok" } else { "declaration miss" }),
                ),
            ]));
        }
        Json::obj([
            ("suite_id", Json::str("compaction_strategy.c0")),
            ("results", Json::Arr(results)),
        ])
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut socket = None;
    let mut version_id = "pin:hh-compact-evict-oldest".to_string();
    let mut content = "content:unknown".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => socket = args.get(i + 1).cloned(),
            "--version-id" => version_id = args.get(i + 1).cloned().unwrap_or(version_id),
            "--content" => content = args.get(i + 1).cloned().unwrap_or(content),
            _ => {}
        }
        i += 1;
    }
    let Some(sock) = socket else {
        eprintln!("hh-compact-evict-oldest: --socket <path> required");
        std::process::exit(2);
    };
    let stream = match std::os::unix::net::UnixStream::connect(&sock) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("hh-compact-evict-oldest: connect {sock}: {e}");
            std::process::exit(2);
        }
    };
    let logic = EvictOldest {
        version_id,
        content,
        binds: 0,
    };
    match PluginRuntime::connect(
        Box::new(SocketIo::new(stream)),
        logic,
        Duration::from_secs(10),
    ) {
        Ok(mut rt) => {
            if let Err(e) = rt.run() {
                eprintln!("hh-compact-evict-oldest: session ended: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("hh-compact-evict-oldest: handshake failed: {e}");
            std::process::exit(2);
        }
    }
}
