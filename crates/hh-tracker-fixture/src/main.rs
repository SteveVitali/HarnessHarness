//! `hh-tracker-fixture` — the packaged `work_source_adapter` reference
//! variant (§5i.1; S5.6). A `FixtureTracker` (the pinned fixture
//! document's logic) served out of process under `plugin_abi/1` via the
//! shared runtime in `hh-plugin-fixture`; launched by the variant host
//! through `hh-helper` (`argv = [bin, --socket, <path>, --version-id
//! <pin>, --content <addr>]`).
//!
//! The fixture document arrives in `BindParams.parameters` under
//! `fixture_doc` — records-in (the bind carries the doc; the process
//! never reads ambient state). Every `invoke` lowers to
//! `hh_fleet_adapter::tracker::dispatch` — the ONE wire codec (CC7).
//! A decode fault is `AbiError::SchemaViolation`; domain data never
//! leaves the typed envelope.

use std::collections::BTreeMap;
use std::time::Duration;

use hh_embed_schema::plugin_abi::{
    AbiError, BindFailure, BindParams, ConformanceParams, GuardParams, GuardVerdict, HelloParams,
    InvokeParams, StreamParams, TriState,
};
use hh_fleet_adapter::tracker::{self, FixtureTracker};
use hh_fleet_adapter::WORK_SOURCE_CLASS;
use hh_fleet_adapter::WORK_SOURCE_CONTRACT;
use hh_plugin_fixture::{PluginCtx, PluginRuntime, VariantLogic};
use hh_varhost::channel::SocketIo;
use hh_wire::json::Json;

/// The variant logic — a `TrackerLogic` behind the contract's op
/// surface. The fixture document is bind-supplied (`records-in`); the
/// pins come from argv (the host asserts them against the seal — V5).
struct TrackerFixtureLogic {
    logic: Option<FixtureTracker>,
    version_id: String,
    content: String,
    binds: u64,
}

impl VariantLogic for TrackerFixtureLogic {
    fn hello(&self) -> HelloParams {
        let offers = vec![Json::obj([
            ("kind", Json::str("class_contract")),
            ("id", Json::str(WORK_SOURCE_CLASS)),
            ("version_range", Json::str(WORK_SOURCE_CONTRACT)),
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
        if params.class_id != WORK_SOURCE_CLASS || params.contract_version != WORK_SOURCE_CONTRACT {
            return Err(BindFailure::ContractMismatch);
        }
        // The fixture document is a bind parameter — `fixture_doc` is the
        // `hh.fleet.fixture/1` record the logic serves (records-in). An
        // undecodable document is a bind refusal, never a fault.
        let doc = params
            .params
            .get("fixture_doc")
            .cloned()
            .unwrap_or(Json::Null);
        self.logic =
            Some(FixtureTracker::from_doc(doc).map_err(|_| BindFailure::ContractMismatch)?);
        self.binds += 1;
        Ok(format!("tracker-fixture-{}", self.binds))
    }

    fn invoke(
        &mut self,
        params: &InvokeParams,
        _ctx: &mut PluginCtx,
    ) -> Result<Vec<Json>, AbiError> {
        let Some(logic) = self.logic.as_mut() else {
            return Err(AbiError::SchemaViolation);
        };
        // One codec, one dispatch — a decode fault is the ABI's own
        // failure (`SchemaViolation`: unknown op/member/shape); domain
        // refusals never reach this arm (they are data inside `outputs`).
        tracker::dispatch(logic, &params.operation, &params.inputs)
            .map_err(|_| AbiError::SchemaViolation)
    }

    fn stream(&mut self, _params: &StreamParams, _ctx: &mut PluginCtx) -> Result<(), AbiError> {
        // The work-source contract has no streaming surface (push
        // deliveries arrive through the ingress lane, not streams).
        Err(AbiError::UnhandledOperation)
    }

    fn guard(&mut self, _params: &GuardParams, _ctx: &mut PluginCtx) -> GuardVerdict {
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
            ("suite_id", Json::str("work_source_adapter.c1")),
            ("results", Json::Arr(results)),
        ])
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut socket = None;
    let mut version_id = "pin:hh-tracker-fixture".to_string();
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
        eprintln!("hh-tracker-fixture: --socket <path> required");
        std::process::exit(2);
    };
    let stream = match std::os::unix::net::UnixStream::connect(&sock) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("hh-tracker-fixture: connect {sock}: {e}");
            std::process::exit(2);
        }
    };
    let logic = TrackerFixtureLogic {
        logic: None,
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
                eprintln!("hh-tracker-fixture: session ended: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("hh-tracker-fixture: handshake failed: {e}");
            std::process::exit(2);
        }
    }
}
