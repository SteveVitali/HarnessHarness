# `fp-w1-q-x55` egress-allowlist freeze — dated amendment, epoch 3 → epoch 4 (2026-11-16)

Evidence class: recorded execution — internal record; not outside-verified.

This is a **dated declared-data amendment** under REQ-XB-116 (§3.5): a
later change is "a dated amendment and a new epoch". It is **declared
data, never machinery** — no code path, codec, mediator or runner
changed by this record (the machinery half — R6.45k's D8–D14
mediated-egress repairs — landed separately on
`codex/harnessharness-r6.45k`). The frozen epoch-3 datum is preserved
verbatim: this amendment is a **versioned successor record** (same
`protocol_id`, new `epoch`), never an in-place rewrite.

## Authority (verbatim basis)

Operator ruling **R-6** (interactive, 2026-11-15 — LEDGER
`## GATE DECISIONS` `intervention-2026-11-15`): *"mint an epoch-4
freeze if, and only if, the scan check confirms a derivation gap"*.
Amendment row **A4** records the conditional authority; R6.45k's D16
scan check decides. Scope: **verify-phase additions only, never
agent-phase** (REQ-XB-116, X-06).

## The scan-check result (the confirmed derivation gap)

`scripts/egress-conformance/scan-verify.sh` runs the committed
`scan_hosts_in_text` (`crates/hh-env/src/egress_allowlist.rs`; through
`hh-w1 scan-hosts`, never a re-implementation) over every committed
`staging/tests/*` byte of the 14 re-proof members **and their
pre-re-proofed successors** (the mechanical set — 34 members, 610
files, `scripts/egress-conformance/derived/scan-inputs.txt`).

Scanned hosts: `astral.sh`, `download.pytorch.org`, `github.com`,
`repo.anaconda.com`, `s3-us-west-2.amazonaws.com`. Exactly **one**
scanned host is absent from the epoch-3 verify set:
**`download.pytorch.org`** — named verbatim by `pytorch-model-cli`'s
committed `tests/test.sh` (`uvx --index https://download.pytorch.org/whl/cpu`,
`docs/build/reports/compute/harvest/fp-wc7/run-root/trials/pytorch-model-cli/*/r*/staging/tests/test.sh`).

**Why the gap existed:** the epoch-3 freeze's `verify` derivation —
agent base ∪ `tests/*` scan ∪ oracle-observed — was derived over an
input set that **did not include the dev successor members' `tests/*`**
(pytorch-model-cli is a dev successor of an `oracle_fails_on_O` draw —
the epoch-3 `--verify-extra` hand-derivation predated/omitted the
successor set). It was a **derivation-input gap, never a scan bug**:
the committed scanner names the host over the same bytes today (the
`r6_45k` battery leg proves both). The derivation is now mechanical —
`scan-verify.sh` regenerates the input set from the committed tasklists
member names and fails on any scanned-but-ungranted host, so the input
set cannot silently drift again.

## Predecessor (frozen, unchanged)

- Record: `hh-egress-allowlist-freeze/1` — `{protocol_id: "fp-w1-q-x55",
  epoch: 3}`
- Digest: `sha256:787db9e1635a6ae8ebf5d50c88a1d4e33ba6fd3707f5a09dc20dee9398f6de21`
  (the 2026-11-14 epoch-2 → epoch-3 amendment's successor record —
  carried verbatim + verified live per-cell in Wc7: zero denied hosts)
- The epoch-3 and epoch-2 artifacts and their `.sha256` sidecars stay
  byte-identical — the committed recomputes still verify; the prereg's
  `allowlist_freeze` citation keeps naming the epoch-1 digest.

## Successor (this amendment)

- Artifact: `docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-4.json`
  (`hh-egress-allowlist-freeze/1`, canonical `Json::to_canonical_string`
  bytes; `.sha256` sidecar for the committed recompute)
- `protocol_id: "fp-w1-q-x55"` (unchanged), **`epoch: 4`**
- List digest: `sha256:785d709206243d4f2ba75f2a98cb0774050b8e42b1d0eea5f1269dc291b84203`
  (`idp/1` over the canonical list document — recomputed
  byte-identically by `scripts/check-fp-allowlist-amendment-epoch4.sh`)

## Delta — exactly one host, verify phase only, nothing else

| Host | Why |
|---|---|
| `download.pytorch.org` | `pytorch-model-cli`'s committed verifier bootstrap (`uvx --index …/whl/cpu`, torch 2.7.1 / torchvision 0.22.1 / numpy / opencv) — the D16-confirmed derivation gap |

Unchanged: `agent` (the Q-X55 no-`github.com`-in-agent-phase posture
verbatim — the grant is verify-phase only), `permanent_denies`,
`withheld` (empty), every epoch-3 grant (a widening amendment narrows
nothing). No other host is added — the scope is the single
scan-confirmed gap only.

## Lawfulness under the freeze rule

`AllowlistFreeze::check_candidate` refuses `allowlist_changed` only for
a changed digest under an **unchanged** `protocol_id` + `epoch`. The
successor carries `epoch: 4` — a different epoch is "a dated
amendment's new freeze, not this check's refusal"; the r6_15
acceptance legs cover both arms. The member's
`allowlist_gap{download.pytorch.org}` disposition is thereby resolved
on the declared-data side (S-3 stands as the mechanism).

## Consumption (recorded, not executed here)

`hh-w1`'s prove path reads the freeze **file** (`cfg.freeze_file`),
never re-derives it — the next window lands this committed artifact
verbatim as its `freeze.json`, exactly as Wc7 landed epoch-3. The
`egress-real-clients` conformance leg's S2 shape flips from a typed
`deny` naming `download.pytorch.org` (epoch-3) to success (epoch-4) —
both recorded under `hh-live-evidence/1`.

## Verification

`bash scripts/check-fp-allowlist-amendment-epoch4.sh` — offline, $0:
schema members; `epoch: 4` under the unchanged `protocol_id`; the list
digest recomputed through the `idp/1` framing; the `verify` diff vs the
epoch-3 set equals exactly `download.pytorch.org` and
`agent`/`permanent_denies`/`withheld` are byte-identical to epoch-3;
the mechanical member scan grants every scanned host
(`scripts/egress-conformance/scan-verify.sh`); the epoch-2 and epoch-3
artifacts still hash to their committed sidecars and the prereg's
`allowlist_freeze` citation still names the epoch-1 digest (all frozen
data untouched); the canonical bytes + sidecar reproduce.
