# scripts/egress-conformance — the real-client egress conformance harness (R6.45k)

The `egress-real-clients` live leg (insert spec §3.2; operator ruling R-4; ADR-0363,
**$0, no secrets, never per-PR CI**). It runs the 14 re-proof members' **and their
pre-re-proofed successors'** committed `tests/test.sh` bootstrap shapes against the
real mediated-egress path: real apt mirrors, astral.sh → GitHub release assets,
uv's managed CPython, PyPI wheels, npm — through `hh-cell-relay` →
`hh-egress-mediator` → the internet.

## Topology

One `ubuntu:noble` container (the task images' family; `Dockerfile`):

- the **subject** tree's `hh-egress-mediator` + `hh-cell-relay`, built inside the
  image for its arch (`x86_64` on GitHub Actions — the GCE arch — `aarch64`
  locally under Docker Desktop);
- the mediator started with the **same argv shape** `hh-w1 h-run` uses
  (`--store-root --socket --policy --env-handle --effect-id --participant --run-id`);
- `--policy` receives the phase layer rendered by **`hh-w1 render-policy`** from
  the committed freeze — the same code path (`admit_allowlist_mediated` +
  `kernel_default(0)` + `phase_schedule`), never re-derived. A phase swap is a
  `policy.json` rewrite, exactly like `DriveCell`'s `policy_dst` write;
- `hh-cell-relay --pair egress=3128=/run/hh/egress.sock` binds `127.0.0.1:3128`;
- `http_proxy`/`https_proxy`/`HTTP(S)_PROXY`/`no_proxy`/`NO_PROXY` set exactly as
  `crates/hh-runner/src/render.rs::closed_env`; `PIP_BREAK_SYSTEM_PACKAGES=1` is
  the declared noble-compat switch for verbatim `pip install` lines written for
  the members' own images.

## Shapes (mechanical — `extract.sh` + `check-derived.sh`)

`extract.sh` derives `derived/` from the committed member `tests/*` (latest
fp tasklists members ∪ fp-w1 trial members): unique `apt-get install`, `pip
install`, `uvx`, curl and env lines — the list cannot drift
(`check-derived.sh` regenerates + diffs; the D16 scan set rides
`scan-inputs.txt` through `hh-w1 scan-hosts` = the committed
`scan_hosts_in_text`).

- **S1** apt update → `apt-get install -y curl` → the astral uv installer → every
  unique non-pytorch `uvx` line, verbatim except the pytest tail → `pytest --version`.
- **S2** the pytorch-index `uvx` line verbatim → expected typed `deny` naming
  `download.pytorch.org` under a freeze that withholds it; success under epoch-4.
- **S3** every unique `pip install` line verbatim + `python -m pytest --version`.
- **S4** every unique `apt-get install` line (the heavier sets).
- **S5** agent-phase: the agent layer policy + apt/`pip`/`npm` + a `github.com`
  deny probe (Q-X55: github.com is never in the agent list).

## Assertions (`walcheck.py` → `verdict.json`, `hh-egress-conformance/1`)

- every step's exit code matches its declared expectation;
- every `security.egress.requested` has a `security.egress.decided`
  (`request_ref` pairing over the committed WAL);
- zero mediator 5xx (decided dispositions, probe status lines, mediator.err);
- every `security.egress.tunnel.closed` carries `close_cause == "eof"` + true
  byte counts;
- the expected denies land as decided `deny` rows naming the host;
- per-step walls + peak concurrent `:3128` connections recorded (the D8 proof).

## Usage

```sh
bash scripts/egress-conformance/extract.sh          # regen derived/
bash scripts/egress-conformance/check-derived.sh    # drift guard
bash scripts/egress-conformance/scan-verify.sh      # the D16 scan check
bash scripts/egress-conformance/runtime-verify.sh <derivation.json> [freeze.json]
                                                    # the R6.51 runtime-evidence
                                                    # check: a committed
                                                    # hh-verify-host-derivation/1
                                                    # candidate set ⊆ freeze verify
bash scripts/egress-conformance/run.sh --subject HEAD --out out/
bash scripts/egress-conformance/run.sh --subject <pre-fix-sha> --shapes s1 --out out-red/
```

The live leg: `bash scripts/live-verify.sh egress-real-clients --budget 0`
(docker required; `HH_EGRESS_SUBJECT_REF` selects the code under test,
`HH_EGRESS_SHAPES` narrows the set). On GitHub Actions it rides the
`live-verify` workflow's manual dispatch (`subject_ref` input), per the
"Local + manual workflow" ruling.
