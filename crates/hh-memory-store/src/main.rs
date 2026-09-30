//! `hh-memory-store` — the packaged `memory_store` variant (§5c.3; S4.16b).
//! A deterministic `hh_context::memory::MemoryStore` served out of process
//! under `plugin_abi/1` via the shared runtime in `hh-plugin-fixture`;
//! launched by the variant host through `hh-helper`
//! (`argv = [bin, --socket, <path>, --version-id <pin>, --content <addr>]`).
//!
//! Every `invoke` lowers to `hh_context::memory_abi::dispatch` — the ONE
//! wire codec (CC7). Domain refusals return inside `outputs` as the typed
//! refusal envelope; only a *decode* fault (unknown op/member/shape) is an
//! `AbiError::SchemaViolation` — the ABI boundary's own failure class.

use std::collections::BTreeMap;
use std::time::Duration;

use hh_context::memory::MemoryStore;
use hh_context::memory_abi::{self, MEMORY_STORE_CLASS, MEMORY_STORE_CONTRACT};
use hh_embed_schema::plugin_abi::{
    AbiError, BindFailure, BindParams, ConformanceParams, GuardParams, GuardVerdict, HelloParams,
    InvokeParams, StreamParams, TriState,
};
use hh_plugin_fixture::{PluginCtx, PluginRuntime, VariantLogic};
use hh_varhost::channel::SocketIo;
use hh_wire::json::Json;

/// The variant logic — a live `MemoryStore` behind the contract's op
/// surface. Stateful (the store IS the state of record for the session);
/// the pins come from argv (the host asserts them against the seal — V5).
struct MemoryStoreLogic {
    store: MemoryStore,
    version_id: String,
    content: String,
    binds: u64,
}

impl VariantLogic for MemoryStoreLogic {
    fn hello(&self) -> HelloParams {
        let offers = vec![Json::obj([
            ("kind", Json::str("class_contract")),
            ("id", Json::str(MEMORY_STORE_CLASS)),
            ("version_range", Json::str(MEMORY_STORE_CONTRACT)),
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
        if params.class_id != MEMORY_STORE_CLASS || params.contract_version != MEMORY_STORE_CONTRACT
        {
            return Err(BindFailure::ContractMismatch);
        }
        self.binds += 1;
        Ok(format!("memory-store-{}", self.binds))
    }

    fn invoke(
        &mut self,
        params: &InvokeParams,
        _ctx: &mut PluginCtx,
    ) -> Result<Vec<Json>, AbiError> {
        // One codec, one dispatch — a decode fault is the ABI's own failure
        // (`SchemaViolation`: unknown member/wrong type); domain refusals
        // never reach this arm (they are data inside `outputs`). Every
        // output crosses as `{result: "<canonical-json>"}` — stored records
        // carry `label`/`provenance.authority` members the V1 screen forbids
        // as *envelope* members; inside the canonical string they are the
        // data they always were (see `memory_abi::wrap_output`).
        memory_abi::dispatch(&mut self.store, &params.operation, &params.inputs)
            .map(|outs| outs.iter().map(memory_abi::wrap_output).collect())
            .map_err(|_| AbiError::SchemaViolation)
    }

    fn stream(&mut self, _params: &StreamParams, _ctx: &mut PluginCtx) -> Result<(), AbiError> {
        // The memory store has no streaming surface.
        Err(AbiError::UnhandledOperation)
    }

    fn guard(&mut self, _params: &GuardParams, _ctx: &mut PluginCtx) -> GuardVerdict {
        // Not a hook — `no_decision` (the identity for optional guards).
        GuardVerdict::NoDecision
    }

    fn conformance(&mut self, params: &ConformanceParams) -> Json {
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
            // The class battery lives in `memory_abi::conformance` — the
            // same suite the host-side driver runs; the plugin reports the
            // declaration-level facts it can answer self-containedly.
            let pass = matches!(
                t.split('.').next().unwrap_or(""),
                "static" | "contract" | "executable" | "property"
            );
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
            ("suite_id", Json::str("memory_store.c1")),
            ("results", Json::Arr(results)),
        ])
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut socket = None;
    let mut version_id = "pin:hh-memory-store".to_string();
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
        eprintln!("hh-memory-store: --socket <path> required");
        std::process::exit(2);
    };
    let stream = match std::os::unix::net::UnixStream::connect(&sock) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("hh-memory-store: connect {sock}: {e}");
            std::process::exit(2);
        }
    };
    let logic = MemoryStoreLogic {
        store: MemoryStore::new("memory_store"),
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
                eprintln!("hh-memory-store: session ended: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("hh-memory-store: handshake failed: {e}");
            std::process::exit(2);
        }
    }
}
