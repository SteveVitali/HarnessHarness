//! S0.3b — E1 reference emitter (throwaway, ADR-0050 R1–R6).
//!
//! Emits the two artefacts the cross-candidate arms consume:
//!   1. the SHARED CORPUS (`shared-corpus.json`) — the fixed `run-g1` corpus (l1-spike-spec §3.3:
//!      "a fixed corpus file with 200 canonical events per session, same bytes for every
//!      candidate") as a canonical JSON array of `{cost, kind, payload}` records. E2/E3 read the
//!      SAME bytes and build the chain with their OWN canonical serializer — that is what gate G1
//!      (canonical form is implementation-independent, ADR-0029 property 5) actually tests.
//!   2. the E1 REFERENCE (`reference.json`) — `run_id`, the E1 hash-chain head, the event count and
//!      the sha256 of the corpus bytes, so every candidate proves it hashed the same input
//!      (matched-budget, CC9).
//!
//! The E1 chain here reuses the LANDED `s1-kernel-spike` lib (`build_chain_in_memory`), which is
//! byte-identical to the durable `run_session` path (same envelope canonical form + LEAF_TAG +
//! SHA-256 chain) — so this reference is exactly the E1 kernel's chain, not a re-derivation.
use hh_wire::{sha256_hex, Json};
use std::fs;

const RUN_ID: &str = "run-g1";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let corpus_path = args.get(1).map(String::as_str).unwrap_or("shared-corpus.json");
    let ref_path = args.get(2).map(String::as_str).unwrap_or("reference.json");

    // 1. Shared corpus as a canonical JSON array (same bytes for every candidate).
    let corpus = s1_kernel_spike::session_corpus(RUN_ID);
    let records: Vec<Json> = corpus
        .iter()
        .map(|e| {
            Json::obj([
                ("cost", Json::Int(e.cost)),
                ("kind", Json::str(e.kind.clone())),
                ("payload", e.payload.clone()),
            ])
        })
        .collect();
    let corpus_json = Json::Arr(records).to_canonical_string();
    fs::write(corpus_path, corpus_json.as_bytes()).expect("write corpus");

    // 2. E1 reference head hash (byte-identical to the durable run_session chain).
    let (head, cost_total) = s1_kernel_spike::build_chain_in_memory(RUN_ID, || false);

    // Prove the in-memory reference IS the E1 kernel's real (durable) chain, not a re-derivation.
    let durable_root = std::env::temp_dir().join(format!("s03b-e1-ref-{}", std::process::id()));
    let _ = fs::remove_dir_all(&durable_root);
    let ledger = s1_kernel_spike::run_session(&durable_root, RUN_ID, 10).expect("durable run");
    let durable_head = ledger.visible().last().unwrap().hash.clone();
    let _ = fs::remove_dir_all(&durable_root);
    assert_eq!(
        head, durable_head,
        "in-memory reference head must equal the durable E1 kernel chain head"
    );
    let corpus_sha = sha256_hex(corpus_json.as_bytes());
    let reference = Json::obj([
        ("candidate", Json::str("E1")),
        ("corpus_sha256", Json::str(corpus_sha.clone())),
        ("cost_total", Json::Int(cost_total)),
        ("event_count", Json::Int(corpus.len() as i64)),
        ("head_hash", Json::str(head.clone())),
        ("leaf_tag", Json::str("hh-event\u{0}")),
        ("run_id", Json::str(RUN_ID)),
    ]);
    fs::write(ref_path, reference.to_canonical_string().as_bytes()).expect("write reference");

    eprintln!("E1 reference emitted:");
    eprintln!("  run_id        {RUN_ID}");
    eprintln!("  event_count   {}", corpus.len());
    eprintln!("  corpus_sha256 {corpus_sha}");
    eprintln!("  head_hash     {head}");
    eprintln!("  cost_total    {cost_total}");
}
