//! R6.32 acceptance legs for the `hh-w1` window driver (manifest 290;
//! the W1 on-VM acts). Offline legs exercise the driver's record
//! emission paths — the freeze codec, the observe-leg recording, the
//! packet label gate and the index fold — through the real binary
//! (`CARGO_BIN_EXE_hh_w1`), never a re-implemented codec.

use hharness_runner::json_ext::JsonExt;
use std::path::PathBuf;
use std::process::Command;

fn dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("hh-w1-r632-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn w1(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_hh_w1"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn ac_freeze_mints_first_pass_list() {
    let d = dir("freeze");
    let out = d.join("freeze.json");
    let o = w1(&[
        "freeze",
        "--protocol",
        "fp-w1-q-x55",
        "--epoch",
        "1",
        "--family",
        "debian",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let j = hharness_wire::json::parse(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(
        j.get("schema").and_then(|v| v.as_str()),
        Some("hh-egress-allowlist-freeze/1")
    );
    let agent: Vec<&str> = j
        .get("list")
        .and_then(|l| l.get("agent"))
        .and_then(|a| JsonExt::as_arr(a))
        .unwrap()
        .iter()
        .filter_map(|h| h.as_str())
        .collect();
    for want in [
        "deb.debian.org",
        "pypi.org",
        "files.pythonhosted.org",
        "registry.npmjs.org",
        "crates.io",
    ] {
        assert!(agent.contains(&want), "agent list missing {want}");
    }
    // Q-X55: no github.com grant anywhere in the agent set.
    assert!(!agent.iter().any(|h| h.contains("github.com")));
    assert_eq!(
        j.get("protocol_id").and_then(|v| v.as_str()),
        Some("fp-w1-q-x55")
    );
    assert!(j.get("digest").and_then(|v| v.as_str()).is_some());
}

#[test]
fn ac_observe_records_upstream_bodies_per_turn() {
    let d = dir("obs");
    let inst = d.join("instruction.txt");
    std::fs::write(&inst, "turn the lights on").unwrap();
    // the stub agent posts twice to the recorded-response endpoint —
    // two upstream-bound request bodies must land in the recording.
    let stub = d.join("mini.sh");
    std::fs::write(
        &stub,
        "python3 -c '\nimport json,urllib.request,os\nfor i in range(2):\n r=urllib.request.Request(os.environ[\"OPENAI_BASE_URL\"].rstrip(\"/\")+\"/chat/completions\",data=json.dumps({\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}).encode(),headers={\"Content-Type\":\"application/json\"})\n urllib.request.urlopen(r).read()\n'\n",
    )
    .unwrap();
    let out = d.join("out");
    let o = w1(&[
        "observe",
        "--task",
        "t1",
        "--mini-argv",
        &format!("sh {}", stub.display()),
        "--instruction",
        inst.to_str().unwrap(),
        "--responses",
        "2",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let rec = hharness_wire::json::parse(
        &std::fs::read_to_string(out.join("t1.recording.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        rec.get("schema").and_then(|v| v.as_str()),
        Some("hh.model_proxy.recording/1")
    );
    let calls = rec.get("calls").and_then(|c| JsonExt::as_arr(c)).unwrap();
    assert_eq!(calls.len(), 2, "one upstream-bound body per turn");
    for c in calls {
        let body = c
            .get("upstream_request")
            .and_then(|u| u.get("body"))
            .and_then(|b| b.get("utf8"))
            .and_then(|s: &hharness_wire::json::Json| s.as_str());
        assert!(body.is_some(), "upstream-bound body captured");
    }
}

#[test]
fn ac_label_packet_refuses_absent_evidence() {
    let d = dir("lab");
    let packet = d.join("reproof-packet.json");
    std::fs::write(&packet, "{}").unwrap();
    let o = w1(&[
        "label-packet",
        "--packet",
        packet.to_str().unwrap(),
        "--ref",
        d.join("absent.json").to_str().unwrap(),
    ]);
    assert!(!o.status.success(), "absent evidence record refuses");
}

#[test]
fn ac_corpus_manifest_and_check() {
    let d = dir("corpus");
    let fx = d.join("fixtures");
    let recs = d.join("recordings");
    std::fs::create_dir_all(&fx).unwrap();
    std::fs::create_dir_all(&recs).unwrap();
    // The 19 committed branch fixtures are the corpus's declared
    // coverage; copy them verbatim so the manifest digests the
    // committed bytes.
    let committed = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/r6_28");
    for n in 1..=19 {
        let name = format!("bm{n:02}.json");
        std::fs::copy(committed.join(&name), fx.join(&name)).unwrap();
    }
    std::fs::write(recs.join("ws1-corpus-01.recording.json"), "{\"x\":1}").unwrap();
    let manifest = d.join("corpus.json");
    let o = w1(&[
        "corpus-manifest",
        "--fixtures-dir",
        fx.to_str().unwrap(),
        "--recordings-dir",
        recs.to_str().unwrap(),
        "--census-tasks",
        "t1,t2",
        "--out",
        manifest.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let stdout = String::from_utf8_lossy(&o.stdout);
    assert!(
        stdout.starts_with("sha256:"),
        "stdout carries the corpus digest"
    );
    let doc = hharness_wire::json::parse(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    assert_eq!(
        doc.get("schema").and_then(|v| v.as_str()),
        Some("hh-l1-corpus/1")
    );
    let entries = doc.get("entries").and_then(|c| JsonExt::as_arr(c)).unwrap();
    assert_eq!(entries.len(), 20);
    for e in entries {
        assert!(e
            .get("digest")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.starts_with("sha256:")));
        assert!(e.get("provenance").and_then(|v| v.as_str()).is_some());
    }
    // The sidecar binds the canonical manifest bytes.
    let side = std::fs::read_to_string(d.join("corpus.sha256")).unwrap();
    let digest = hharness_wire::sha256::sha256_hex(std::fs::read(&manifest).unwrap().as_slice());
    assert!(
        side.starts_with(&digest),
        "sidecar digest binds the manifest bytes"
    );
    // corpus-check emits the complete BM map + census coverage.
    let cov_out = d.join("coverage.json");
    let o = w1(&[
        "corpus-check",
        "--corpus",
        manifest.to_str().unwrap(),
        "--tasks",
        "t1,t2",
        "--out",
        cov_out.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let cov = hharness_wire::json::parse(&std::fs::read_to_string(&cov_out).unwrap()).unwrap();
    let obj = cov.as_obj().unwrap();
    assert_eq!(obj.len(), 19, "all BM branches covered");
    // A missing census task refuses.
    let o = w1(&[
        "corpus-check",
        "--corpus",
        manifest.to_str().unwrap(),
        "--tasks",
        "t1,t3",
    ]);
    assert!(!o.status.success(), "missing census task refuses");
    // An unnameable fixture member refuses.
    std::fs::write(fx.join("bmXX.json"), "{}").unwrap();
    let o = w1(&[
        "corpus-manifest",
        "--fixtures-dir",
        fx.to_str().unwrap(),
        "--recordings-dir",
        recs.to_str().unwrap(),
        "--census-tasks",
        "t1",
        "--out",
        manifest.to_str().unwrap(),
    ]);
    assert!(!o.status.success(), "unknown fixture stem refuses");
}
