# S0.3b — cross-candidate / MCP-ACP online spike (throwaway)

Ticket **S0.3b** (inserted). Closes the **machine-achievable** cells S0.3 deferred offline:
DF-S0.3-1 (official-SDK MCP/ACP round-trip + cross-candidate gate G1) and the machine cells of
DF-S0.3-3 (E2/E3 S1 scoring + the E5b/E5c splits). Operator released an **online** budget
(network + candidate toolchains + official SDKs against LOCAL reference peers; **no model spend**).

Throwaway (l1-spike-spec R1–R6): outside the Cargo workspace, not a kernel dependency, deletable at
the Stage-0 boundary. Candidate ids only here (CC4); the E2/E3 **candidate→toolchain binding** is in
`docs/adr/ADR-0225` (build ADR), never in this tree's committed prose or the sheet.

## Layout
- `e1-ref/` — Rust binary: emits the **shared corpus** (`corpus/shared-corpus.json`, same bytes for
  every candidate) and the **E1 reference** (`corpus/reference.json`: head hash + corpus sha, proven
  byte-identical to the durable `run_session` chain).
- `e2-python/` — E2 kernel (`e2_kernel.py`: own canonical serializer + hash chain + S1 measures) and
  the official-SDK round-trips (`mcp_roundtrip.py`, `acp_roundtrip.py`; `.venv` vendored).
- `e3-node/` — E3 kernel (`e3_kernel.mjs`) and official-SDK round-trips (`e3_mcp_roundtrip.mjs`,
  `e3_acp_roundtrip.mjs`; `node_modules` vendored).
- `e5b-e5c/` — boundary splits: E5b (E3 kernel + E2 lab, stdio JSON-RPC) and E5c (E2 kernel + E3
  surface, localhost HTTP); shared `e5_chain.mjs`.
- `run-sheet.py` — repeat-score every arm **N=5**, write `distributions.json`.
- `test-gates.sh` — executable gates (G1 byte-identity + a negative test, M-S1-7 verified, M-S2-7 100%).

## Reproduce
```bash
# 1. install the official SDKs (one-time, network)
python3 -m venv e2-python/.venv && e2-python/.venv/bin/pip install mcp agent-client-protocol
( cd e3-node && npm install @modelcontextprotocol/sdk @zed-industries/agent-client-protocol zod )
# 2. emit the shared corpus + E1 reference
( cd e1-ref && cargo build --release && ./target/release/s03b-e1-ref ../corpus/shared-corpus.json ../corpus/reference.json )
# 3. gates, then the full N=5 sheet
bash test-gates.sh
python3 run-sheet.py 5
```
Durable output: `docs/build/reports/S0.3-measurement-sheet.md` (S0.3b extension) + the ADR-0050
amendment-log block. No harness claim is made at Stage 0 (R5); these are internal tier-B numbers.
