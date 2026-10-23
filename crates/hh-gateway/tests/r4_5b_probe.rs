// quick probe — dbg! the transport error against real ollama
use hharness_gateway::gateway::Transport;
#[test]
#[ignore]
fn probe() {
    let d = hharness_gateway::dialects::openai_compatible();
    let mut t = hharness_gateway::http_transport::HttpModelTransport::new(d.clone());
    let tools = vec![hharness_gateway::openai::tool_entry(
        "hh_submit",
        "hh.submit/1",
        hharness_wire::json::Json::obj([
            ("type", hharness_wire::json::Json::str("object")),
            (
                "properties",
                hharness_wire::json::Json::obj([(
                    "text",
                    hharness_wire::json::Json::obj([(
                        "type",
                        hharness_wire::json::Json::str("string"),
                    )]),
                )]),
            ),
            (
                "required",
                hharness_wire::json::Json::Arr(vec![hharness_wire::json::Json::str("text")]),
            ),
        ]),
    )];
    let mut params = std::collections::BTreeMap::new();
    params.insert(
        "stream_options".to_string(),
        hharness_wire::json::Json::obj([("include_usage", hharness_wire::json::Json::Bool(true))]),
    );
    let body = hharness_gateway::openai::chat_request_body(
        &std::env::var("HH_LOCAL_MODEL").unwrap(),
        &[hharness_gateway::openai::text_msg(
            "user",
            "Call the hh_submit tool with arguments {\"text\":\"done\"}.",
        )],
        &tools,
        true,
        &params,
    );
    let p = hharness_gateway::plan::ProviderRequestPlan {
        dialect_id: d.dialect_id.clone(),
        dialect_version: d.version.clone(),
        endpoint_ref: std::env::var("HH_LOCAL_BASE_URL").unwrap(),
        model_ref: "x".into(),
        body,
        provider_params: std::collections::BTreeMap::new(),
        capabilities: hharness_gateway::plan::Capabilities {
            streaming: true,
            tool_use: false,
            vision: None,
            deferred: None,
        },
        cache: Default::default(),
        budget_reservation_id: None,
        canonical_len: 0,
    };
    let bytes = hharness_gateway::codec::serialize(&p);
    match t.send(
        &bytes,
        &hharness_gateway::gateway::CredentialHandle {
            binding_id: None,
            provided: false,
        },
        &std::env::var("HH_LOCAL_BASE_URL").unwrap(),
    ) {
        Ok(fr) => {
            eprintln!("OK {} frames", fr.len());
            for (_, f) in &fr {
                eprintln!("  {}", f.kind);
            }
        }
        Err(e) => eprintln!(
            "ERR class={:?} msg={} code={:?}",
            e.class, e, e.provider_code
        ),
    }
}
