//! `hh-eval-oracle` — the out-of-process deterministic-oracle binary
//! (spec §5h.3.3; AC-R-2.9.2-11; S3.3).
//!
//! Reads one canonical-JSON `OracleRequest/1` on stdin, runs the same
//! `hh_eval::run_oracle` dispatch in-process callers use, and writes the
//! `OracleVerdict/1` (or an `oracle_failure` envelope) to stdout. No opaque
//! handles cross the boundary — the request is pure content, so the
//! subprocess verdict is byte-identical to the in-process verdict.

use std::io::Read;

use hh_eval::oracle::{run_oracle, OracleRequest};
use hh_wire::Json;

fn main() {
    let mut buf = String::new();
    let code = match std::io::stdin().read_to_string(&mut buf) {
        Ok(_) => run(&buf),
        Err(e) => fail(&format!("stdin: {e}")),
    };
    std::process::exit(code);
}

fn run(input: &str) -> i32 {
    let parsed = match hh_wire::parse(input) {
        Ok(j) => j,
        Err(e) => return fail(&format!("request parse: {e}")),
    };
    let req = match OracleRequest::from_json(&parsed) {
        Ok(r) => r,
        Err(e) => return fail(&format!("request decode: {e:?}")),
    };
    match run_oracle(&req) {
        Ok(v) => {
            println!("{}", v.to_json().to_canonical_string());
            0
        }
        Err(f) => {
            let env = Json::obj([
                ("schema", Json::str("oracle_failure/1")),
                ("failure", Json::str(f.to_string())),
            ]);
            println!("{}", env.to_canonical_string());
            2
        }
    }
}

fn fail(msg: &str) -> i32 {
    let env = Json::obj([
        ("schema", Json::str("oracle_failure/1")),
        ("failure", Json::str(format!("MalformedRequest: {msg}"))),
    ]);
    println!("{}", env.to_canonical_string());
    2
}
