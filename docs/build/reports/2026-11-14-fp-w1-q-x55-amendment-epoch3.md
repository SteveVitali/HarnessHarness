# `fp-w1-q-x55` egress-allowlist freeze — dated amendment, epoch 2 → epoch 3 (2026-11-14)

Evidence class: recorded execution — internal record; not outside-verified.

This is a **dated declared-data amendment** under REQ-XB-116 (§3.5):
"the list SHALL be frozen with its digest before the first paid slot; a
later change is a dated amendment and a new epoch." It is **declared
data, never machinery** — no code path, codec, mediator, runner or
acceptance battery changed by this record (the machinery half — the
duplicate-`Host` repair and the `VERIFY_MIRROR_HOSTS` generator
extension — landed separately at `b8310b68`). The frozen epoch-2 datum
is preserved verbatim: this amendment is a **versioned successor
record** (same `protocol_id`, new `epoch`), never an in-place rewrite.

## Scope decision (verbatim)

Operator scope-decision **"H-allow duplicate-Host repair + Wc7 under
the epoch-3 allowlist"** — land the sixth machinery repair (mediated
forward drops hop-by-hop/authority headers), widen the frozen
`fp-w1-q-x55` allowlist for the verify phase's uv-install 302 target,
then re-run the re-proof in a Wc7 window at the Wc6-proven D = 8 h.
The Wc7 window itself is a later gate's scope; this record is the data
half only.

## Why (the declared-data finding)

Wc6 (R6.45f, PR #382 `4b39dd99`) ran the re-proof to **completion**
inside D = 8 h — held 30/30 + dev 8/8 surfaces, both
`hh-reproof-packet/1`s landed, the Wc5 window-bound deficit discharged —
yet **0/14 covering verdicts**: every member's H-allow cell produced
`reward.txt = 0` (rolled up by the committed classifier as
`exclude:runner_infeasible:allowlist_gap{unknown}` — `denied_hosts`
empty because every request was *allowed*). Diagnosed live on the real
mediator (`b8310b68`): `LocalHttpTransport::forward` and the TLS twin in
`crates/hh-env/src/egress.rs` injected their own `Host: <host>:<port>`
then relayed the client's headers **verbatim** — every absolute-form
request that carried a `Host` (apt's index + `.deb` fetches, every proxy
client) reached the origin with **two `Host` headers** and was answered
`400 Bad Request` per RFC 7230 §5.4. The verifier's apt bootstrap could
never install curl, so uv/uvx never landed, `test.sh` fell through to
`echo 0 > reward.txt`, pytest never ran — a **sixth machinery defect**,
repaired at `b8310b68` (forwarding now filters the closed
hop-by-hop/authority set).

That same diagnosis exposed a **second, declared-data gap**: the
verifier's `uv` `install.sh` 302-redirects from `astral.sh` to
**`release-assets.githubusercontent.com`** — GitHub's release-asset
host — which is **absent from the epoch-2 verify allowlist** (observed
denied live in the stub-mediator reproduction; unreachable by any
`tests/*` scan and the next denial behind the 400s). The committed
`VERIFY_MIRROR_HOSTS` generator extension (`b8310b68`) now carries it
in `Allowlist::first_pass`, but the frozen declared list needs this
new-epoch amendment — a declared-data property (REQ-XB-125), never a
silent list swap.

## Predecessor (frozen, unchanged)

- Record: `hh-egress-allowlist-freeze/1` — `{protocol_id: "fp-w1-q-x55",
  epoch: 2}`
- Digest: `sha256:86def07e2e8402c3aa0d2332d97abaed2b5d189c9ba3c1562afc1daf10712f98`
  (the 2026-11-13 epoch-1 → epoch-2 amendment's successor record —
  landed `28617cb0`, carried verbatim + verified live per-cell in Wc5
  and Wc6: zero denied hosts, real `reward.txt` production on every
  completed H-allow cell)
- The epoch-2 artifact and its `.sha256` sidecar stay byte-identical —
  the committed recompute still verifies.
- The W1–Wc4 epoch-1 datum (`sha256:b62762ae…`) and the prereg's
  `allowlist_freeze` citation stay verbatim — history preserved.

## Successor (this amendment)

- Artifact: `docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.json`
  (`hh-egress-allowlist-freeze/1`, canonical `Json::to_canonical_string`
  bytes; `.sha256` sidecar for the committed recompute)
- `protocol_id: "fp-w1-q-x55"` (unchanged), **`epoch: 3`**
- List digest: `sha256:787db9e1635a6ae8ebf5d50c88a1d4e33ba6fd3707f5a09dc20dee9398f6de21`
  (`idp/1` over the canonical list document — recomputed
  byte-identically by `scripts/check-fp-allowlist-amendment-epoch3.sh`)

## Delta — exactly one host, verify phase only, nothing else

Added to the **`verify` set only** (the verifier's uv-install 302
target; the agent phase does not need it — the agent list is
byte-identical to epoch-2):

| Host | Why |
|---|---|
| `release-assets.githubusercontent.com` | GitHub's release-asset host — `astral.sh`'s `install.sh` 302s here; the verify phase's uv/uvx bootstrap cannot complete without it |

Unchanged: `agent` (the Q-X55 no-`github.com`-in-agent-phase posture
verbatim — the mirror grant is verify-phase only, exactly as the
committed `VERIFY_MIRROR_HOSTS` chains it into verify and never agent),
`permanent_denies` (`hub.harborframework.com`, `tbench.ai` verbatim),
`withheld` (empty), every epoch-2 grant (a widening amendment narrows
nothing), and `github.com`'s own verify grant. No other host is added —
the scope is the uv-install 302 target only.

## Lawfulness under the freeze rule

`AllowlistFreeze::check_candidate` refuses `allowlist_changed` only for
a changed digest under an **unchanged** `protocol_id` + `epoch`. The
successor carries `epoch: 3` — a different epoch is "a dated
amendment's new freeze, not this check's refusal"
(`crates/hh-env/src/egress_allowlist.rs`); the r6_15 acceptance legs
cover both arms.

## Wc7 consumption (recorded, not executed here)

`hh-w1`'s prove path reads the freeze **file** (`cfg.freeze_file` →
`/opt/wc*/freeze.json`), never re-derives it — so the Wc7 window lands
this committed artifact verbatim as `/opt/wc7/freeze.json`, the same way
Wc5/Wc6 landed the epoch-2 record. Note for that gate (carried from the
epoch-2 amendment): `hh-w1 freeze` mints a single `ImageFamily`'s list,
so the in-window "regenerate and compare digest" leg cannot reproduce
the union — Wc7 carries the committed artifact (the recorded basis
here). The committed `VERIFY_MIRROR_HOSTS` now produces this host in
every fresh verify derivation, keeping the generator and the freeze in
agreement.

## Verification

`bash scripts/check-fp-allowlist-amendment-epoch3.sh` — offline, $0:
schema members; `epoch: 3` under the unchanged `protocol_id`; the list
digest recomputed through the `idp/1` framing; the `verify` diff vs the
epoch-2 set equals exactly `release-assets.githubusercontent.com` and
`agent`/`permanent_denies`/`withheld` are byte-identical to epoch-2; the
committed `VERIFY_MIRROR_HOSTS` names the host (generator/freeze
agreement); the epoch-2 artifact still hashes to its committed sidecar
and the prereg's `allowlist_freeze` citation still names the epoch-1
digest (both frozen data untouched); the canonical bytes + sidecar
reproduce.
