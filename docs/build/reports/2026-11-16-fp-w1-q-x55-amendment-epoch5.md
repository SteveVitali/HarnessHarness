# `fp-w1-q-x55` egress-allowlist freeze — dated amendment, epoch 4 → epoch 5 (2026-11-16)

Evidence class: recorded execution — internal record; not outside-verified.

This is a **dated declared-data amendment** under REQ-XB-116 (§3.5): a
later change is "a dated amendment and a new epoch". It is **declared
data, never machinery** — no code path, codec, mediator or runner
changed by this record (the machinery half — R6.45k's D8–D14
mediated-egress repairs — stands verified live by smoke1's real CONNECT
byte counts and eof-only closes). The frozen epoch-4 datum is preserved
verbatim: this amendment is a **versioned successor record** (same
`protocol_id`, new `epoch`), never an in-place rewrite.

## Authority (honest scope note — exceeds R-6's scan-derived-only)

Epoch-4 minted under operator ruling **R-6**: *"mint an epoch-4 freeze
if, and only if, the scan check confirms a derivation gap"* — a
**scan-derived** authority. This epoch-5 amendment is **not**
scan-derived: the mechanical `scan_hosts_in_text` over the committed
`tests/*` bytes names `download.pytorch.org` (the *index* host) but
cannot name the hosts added here — they are **runtime-transitive**: the
verifier's legitimate dependencies surfaced only at smoke1's runtime as
deny rows (a 302 redirect target and two dataset fetches).

The authority recorded here is the **operator-delegated declared-data
scope call**: the operator delegated the declared-data decision to the
coordinator's judgment (verbatim basis: *"continue using your best
judgement"*, interactive, 2026-11-16) — mint epoch-5 with the
runtime-observed verify hosts, then run smoke2 under it. This record
names that delegation plainly rather than claiming the R-6 scan
conditional; a fuller derivation authority is the recorded follow-up
below. Scope stays inside R-6/X-06's letter: **verify-phase additions
only, never agent-phase** (REQ-XB-116, X-06).

## The runtime-observed derivation gap (smoke1's evidence)

smoke1 (R6.45l, `hh-smoke/1` non-evidence record
`docs/build/reports/smoke/2026-11-16-smoke1.json`) ran the hardened
runner under the carried epoch-4 freeze. Four of five declared cells
decided cleanly; `pytorch-model-cli`'s verify phase denied:

| cell | denied_hosts (verbatim from the cell-report, recorded in the smoke record) |
|---|---|
| `pytorch-model-cli/H-allow/r0` (epoch-4) | `["download-r2.pytorch.org"]` |
| `pytorch-model-cli/H-allow-diag/r0` (window-local diag freeze = epoch-4 verify + `download-r2.pytorch.org`, uncommitted) | `["ossci-datasets.s3.amazonaws.com", "yann.lecun.com"]` |

The progression: `download.pytorch.org` (epoch-4's D16 fix) is the
*index* host; the `uvx` wheel fetch 302-redirects to
**`download-r2.pytorch.org`** (denied). The in-window diagnostic —
window-local freeze, uncommitted, labelled `diagnostic` — confirmed that
deny was the sole blocker at that layer, then surfaced the next depth:
**`ossci-datasets.s3.amazonaws.com`** + **`yann.lecun.com`**, the task's
MNIST/dataset fetches. Both denies carried clean `close_cause: eof`
tunnels and zero 5xx — the mediator machinery behaved correctly; the
gap is declared data only.

**Why the gap exists (the honest class):** epoch-4's verify derivation —
agent base ∪ `tests/*` scan ∪ oracle-observed — has no arm that names a
host appearing only at *runtime*: redirect targets behind a granted
index host and dataset URLs fetched by verifier code paths. The
committed scan correctly finds `download.pytorch.org` in
`tests/test.sh`; nothing in the committed text names the three hosts
below. A complete freeze derivation therefore needs an
**oracle-observed / runtime arm for transitive hosts** — recorded as
deferral row **DF-R6.45l-1** (`docs/tickets/DEFERRALS.md`): derive the
transitive verify set from a captured runtime trace (the diag pattern)
rather than another blind epoch.

## Predecessor (frozen, unchanged)

- Record: `hh-egress-allowlist-freeze/1` — `{protocol_id: "fp-w1-q-x55",
  epoch: 4}`
- Digest: `sha256:785d709206243d4f2ba75f2a98cb0774050b8e42b1d0eea5f1269dc291b84203`
  (the epoch-3 → epoch-4 amendment's successor record — carried verbatim
  through smoke1: 4/5 declared cells decided reward 1, zero 5xx,
  eof-only closes)
- The epoch-4, epoch-3 and epoch-2 artifacts and their `.sha256`
  sidecars stay byte-identical — the committed recomputes still verify;
  the prereg's `allowlist_freeze` citation keeps naming the epoch-1
  digest.

## Successor (this amendment)

- Artifact: `docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-5.json`
  (`hh-egress-allowlist-freeze/1`, canonical `Json::to_canonical_string`
  bytes; `.sha256` sidecar for the committed recompute)
- `protocol_id: "fp-w1-q-x55"` (unchanged), **`epoch: 5`**
- List digest: `sha256:e61a6ed9771d506e0606dd7a953ebe49a56dc96d8427d9eb410ae492e7f3b328`
  (`idp/1` over the canonical list document — recomputed
  byte-identically by `scripts/check-fp-allowlist-amendment-epoch5.sh`)

## Delta — exactly three hosts, verify phase only, nothing else

| Host | Why |
|---|---|
| `download-r2.pytorch.org` | `pytorch-model-cli` verifier's wheel CDN — the 302 target of the granted index `download.pytorch.org`; smoke1 H-allow `denied_hosts` verbatim |
| `ossci-datasets.s3.amazonaws.com` | the task's dataset fetch (MNIST-class); smoke1 diag `denied_hosts` verbatim |
| `yann.lecun.com` | the task's dataset mirror (MNIST index pages); smoke1 diag `denied_hosts` verbatim |

Unchanged: `agent` (the Q-X55 no-`github.com`-in-agent-phase posture
verbatim — the grants are verify-phase only), `permanent_denies`,
`withheld` (empty), every epoch-4 grant (a widening amendment narrows
nothing). No other host is added — the scope is exactly the
runtime-observed deny set.

## Lawfulness under the freeze rule

`AllowlistFreeze::check_candidate` refuses `allowlist_changed` only for
a changed digest under an **unchanged** `protocol_id` + `epoch`. The
successor carries `epoch: 5` — a different epoch is "a dated
amendment's new freeze, not this check's refusal". The member's
`allowlist_gap{download-r2.pytorch.org, ossci-datasets.s3.amazonaws.com,
yann.lecun.com}` disposition is thereby resolved on the declared-data
side if smoke2 observes no deeper host; a *new* runtime-transitive deny
under epoch-5 is recorded honestly (the diag pattern) and feeds
DF-R6.45l-1's derivation arm — never another blind epoch.

## Consumption (recorded, not executed here)

`hh-w1`'s prove path reads the freeze **file** (`cfg.freeze_file`),
never re-derives it — smoke2 lands this committed artifact verbatim as
its `freeze.json` (the declaration `fp-smoke2.json`, quote
`pq-30c1a03d1dce50ea`, ceiling 830,480 µ$ under GATE-G28i's envelope —
the second non-evidence window). Smoke results are labelled per A1 —
non-evidence, never re-proof evidence.

## Verification

`bash scripts/check-fp-allowlist-amendment-epoch5.sh` — offline, $0:
schema members; `epoch: 5` under the unchanged `protocol_id`; the list
digest recomputed through the `idp/1` framing; the `verify` diff vs the
epoch-4 set equals exactly the three runtime-observed hosts and
`agent`/`permanent_denies`/`withheld` are byte-identical to epoch-4; the
mechanical member scan still grants every scanned host
(`scripts/egress-conformance/scan-verify.sh` — the scan does **not**
name the added hosts: they are runtime-observed, not scan-derived, and
the check asserts that provenance honestly); the epoch-2/3/4 artifacts
still hash to their committed sidecars and the prereg's
`allowlist_freeze` citation still names the epoch-1 digest (all frozen
data untouched); the committed smoke1 record carries the deny evidence
this amendment cites; the canonical bytes + sidecar reproduce.
