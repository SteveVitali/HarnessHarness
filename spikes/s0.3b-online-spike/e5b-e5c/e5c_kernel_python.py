#!/usr/bin/env python3
"""S0.3b — E5c KERNEL (E2 ecosystem) driven by an E3 surface over the kernel<->surfaces boundary
(ADR-0050 M3b: localhost HTTP + generated client). Modes:
  native <corpus> <n>   build n sessions in-process, print internal build wall (baseline arm)
  serve <corpus> <port> answer POST /run_session {run_id} -> {head,cost,events} on localhost
Throwaway; reuses the E2 kernel's build_ledger (chain reproduces the E1 reference head, gate G1)."""
import json
import os
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "e2-python"))
import e2_kernel  # noqa: E402


def run_result(corpus, run_id):
    ledger = e2_kernel.build_ledger(corpus, run_id)
    return {
        "head": ledger[-1]["hash"],
        "cost": sum(e["cost"] for e in ledger),
        "events": [{"seq": e["seq"], "hash": e["hash"]} for e in ledger],
    }


def main():
    mode = sys.argv[1]
    corpus_path = sys.argv[2]
    with open(corpus_path, "rb") as f:
        corpus = json.loads(f.read().decode("utf-8"))

    if mode == "native":
        n = int(sys.argv[3])
        t0 = time.perf_counter()
        for i in range(n):
            run_result(corpus, "run-%d" % i)
        print("native_build_wall_us %d" % int((time.perf_counter() - t0) * 1e6))
        return

    if mode == "serve":
        port = int(sys.argv[3])

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_POST(self):
                length = int(self.headers.get("Content-Length", 0))
                body = json.loads(self.rfile.read(length).decode("utf-8"))
                if self.path == "/hello":
                    result = {"contract": "hh-embed/1", "kernel_candidate": "E2", "protocol": "http-localhost"}
                elif self.path == "/run_session":
                    result = run_result(corpus, body["run_id"])
                else:
                    self.send_response(404)
                    self.end_headers()
                    return
                payload = json.dumps(result).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

        httpd = ThreadingHTTPServer(("127.0.0.1", port), Handler)
        sys.stderr.write("LISTENING %d\n" % httpd.server_address[1])
        sys.stderr.flush()
        httpd.serve_forever()


if __name__ == "__main__":
    main()
