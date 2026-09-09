# SIG go-live & productionization — design and implementation specification

**Document:** `~/MetaHarness/sig-golive-spec.md` · **Version:** 0.2.0 · **Status:** Ratified 2026-09-09 (operator delegated the Appendix-B open questions to Devin's best judgement — see §0.1; workers may implement).
**Date:** 2026-09-09 · **Author:** Devin (for Steve Vitali)
**Repo / worktree:** `~/Eleutheria` (SIG — Surveillance Infrastructure Graph)
**Derived from:** the `sig-postbuild` build (PRs #47–#68, `projectStatus: DONE`); the build ledger's `RETURN PASS`, `GATE DECISIONS`, and `OPEN FINDINGS`; `docs/build/OPERATIONAL_READINESS.md`; `docs/build/INTEGRATION_PLAN.md`; `docs/build/BACKLOG.csv`; `docs/2_canonical_design_spec.md` §§ cited per ticket.
**Requirement IDs:** `GL-<AREA>-<nn>`, append-only. Areas: `GATE`, `REL`, `GOV`, `LEGAL`, `ACCT`, `RIGHTS`, `LIVE`, `INFRA`, `CONTRIB`, `SOURCES`, `DEPLOY`, `SCHED`, `OBS`, `CI`, `META`, `JURIS`, `CCOPS`.
**Normative language:** MUST / SHOULD / MAY per RFC 2119. Prose without a keyword is rationale.
**Precondition:** ticket `EL.1` (build-memory-v2 migration) has landed, so this repo is in **build-memory v2** (`docs/build/README.md` carries `<!-- build-memory: v2 -->`, `docs/build/LEDGER.md` is the machine state, `docs/tickets/DEFERRALS.md` exists and is seeded from the RETURN PASS items). This spec is instantiated into the v2 manifest by `decompose-spec mode=extend`, **not** hand-written into the legacy chain.

---

# Part 0 — How to use this document

## The core idea: three lanes, three homes

The sig-postbuild build produced a functionally complete system that runs composed-green **over fixtures, to local staging** — nothing is live, merged, or deployed. The remaining work to reach a live, public Oklahoma City (and then scale) splits into three lanes that must **not** all be forced into `implement-spec` tickets:

| Lane | Work | Home | New contract? |
|---|---|---|---|
| **A — Human gates** | legal home, operating governance, counsel sign-off, accounts/tokens, hosting/budget, go-public | `docs/tickets/NN[a-z]_HUMAN-H<k>__*.md` / `NN[a-z]_GATE-G<k>__*.md` marker docs + the gate register (§2); readouts in `docs/build/readouts/` | No — markers, signed by the operator |
| **B — Return-pass re-runs** | re-run P21.1/P21.3/P21.4/P21.5/P21.7/P21.8/P21.9 once their gate opens | `docs/tickets/DEFERRALS.md` rows (seeded by EL.1) + the **existing** ticket files, re-run verbatim | No — re-run the same file after ticking its gate |
| **C — New code** | real deployment, scheduling, observability, CI-composed, 2nd jurisdiction, CCOPS connector, the 3 OKC document connectors | new `implement-spec` contracts (Part II Round 4 + LIVE.1's sub-work) | **Yes** — decomposed into the manifest |

**Policy (GL-META-00, MUST):** a return-pass item never gets a new ticket contract; it is a DEFERRALS row whose "re-run line" is the original ticket's `Run:` line. A human prerequisite never gets an `implement-spec` contract; it is a HUMAN/GATE marker. Only genuinely-new build work gets a new contract. This keeps the plan honest (no duplicated contracts) and matches how the sig-postbuild build actually recorded its gate-skipped tickets.

## Instantiation (after EL.1)

```
decompose-spec mode=extend spec=~/MetaHarness/sig-golive-spec.md tail=minimal
```
`extend` reads the v2 `docs/build/LEDGER.md`, `DEFERRALS.md`, and `BACKLOG.csv`, then:
1. emits the Round-3 HUMAN/GATE marker files (Lane A) and appends the gate register rows;
2. reconciles the Round-3 return-pass entries (Lane B) against the DEFERRALS rows EL.1 already seeded — adding nothing new, only cross-referencing this spec's `GL-*` ids;
3. emits full ticket contracts for Lane C (Round 4 + LIVE.1's document-connector sub-work) with `NN_<ID>__slug.md` filenames continuing the sequence after EL.1's `P23.*` tail;
4. appends `tail=minimal` closeout rows for the go-live round (a `CAP`/`REC` delta is optional — see D6);
5. writes the manifest rows, runs `check-build-memory.sh`, prints the manual floor.

Rounds are **ratified independently**: ratify Round 3 when the legal/governance path is real; ratify Round 4 when OKC is live. Do not autonomously execute an unratified round.

## §0.1 Operator decisions (delegated 2026-09-09) — baked into GATE DECISIONS at decompose time

The operator delegated the Appendix-B gates to Devin's judgement ("publish everything we want"; "flip all sources, testing the first few end-to-end first"; "use my GCP project for the host"). These are recorded as **pre-answered gate decisions** so `orchestrate-build` does not stall, with honest provenance:

- **GL-GATE-01 (HG-01 legal home) — interim posture, MUST record as interim.** SIG operates as an independent open-source project under maintainer stewardship pending a formal legal-home designation. This unblocks the artifacts; a **real legal home remains a human action before the actual public cutover** (Go-public). Not a substitute for legal counsel.
- **GL-GATE-02 (HG-02 counsel) — engineering disposition, publish-permitting, NOT a legal opinion.** Publication of the SIG graph, the OSM-derived compartment (ODbL, separate + attribution + share-alike), and the OKC dossiers is permitted, resting on the *structural* safeguards already built and tested: Part VIII (no plate/trip/per-person storage; officer-naming gate; sensitivity tiers + coordinate rules), per-compartment licences, publication tiers, honest-rendering. **Labelled in every artifact as "operator/engineering disposition pending counsel; counsel review recommended before real public exposure."**
- **GL-GATE-03 (HG-03/04 flips) — flip all, phased.** RIGHTS.1 flips the **OKC critical subset first** (`okc_procurement`, `okc_council`, `okcpd_policy`, `ok_statute`, `osm_overpass`, `deflock`), records reviewer = "maintainer (delegated)" + date + the rights basis quoted from each packet; LIVE.1→LIVE.2 run end-to-end on that subset as the test; SOURCES.1 then flips the remainder (news sources stay LINK-only per their packets; FR/BE stay design-gated false). Every flip cites its packet's redistributable/derivative/SPDX basis; no fabricated basis.
- **GL-GATE-04 (HG-12 host) — GCP project `zeta-medley-508121-u7` (name `eleutheria`).** Zero/low-cost design (SIG-STORE-003): static site + `sig-exports` output + deposits on **GCS**; API on **Cloud Run** (scales to zero); Postgres+PostGIS on the smallest **Cloud SQL** tier *or* a single `e2-micro` GCE running the compose stack (DEPLOY.1 picks one in its ADR, keeps the other documented). Infra-as-code (gcloud + a Terraform/`ops/gcp/` module) is **written and validated** by DEPLOY.1; the actual `apply` is **gated on operator `gcloud` auth** (Application Default Credentials in the run shell) and never executed by an isolated subagent.
- **GL-GATE-05 (Go-public) — stays a deliberate human action.** The chain drives to "OKC ingested + published to a GCP **staging/private** target"; the DNS/public cutover (and the `v0.2.0` "first public jurisdiction" tag) is the operator's explicit final step.

**Credentialed live actions** (real fetches HG-09, GCP `apply`, Zenodo HG-07, MapRoulette HG-08, usability study HG-10) are executed by whatever run shell holds the credentials. In an unattended subagent chain they run in **prepare + gate-pending** mode (code + config landed, RETURN PASS row written) — they are the short list of operator re-runs, not blocks.

## Reconciling the P23.x migration tail (decompose-spec instruction)

EL.1 auto-instantiated a generic migration-closeout tail `docs/tickets/P23.1…P23.7` (rows 67–73). It is redundant — PR #68's capstone already verified the composed build and the migration (PR #69) added no product code. `decompose-spec mode=extend` **removes those seven placeholder rows + files** and replaces the tail with this spec's Round 3 + Round 4 chain, ending with one minimal closeout (`tail=minimal`). `CURRENT STATE.nextTicket` is set to the first go-live ticket (REL.1 or, if the operator tags separately, RIGHTS.1's predecessor GOV.1). Historical rows 47–66 and all committed artifacts are untouched.

## Operator runbook (the sessions, in order)

0. **Ratify Round 3** (Appendix B): settle the open questions, flip Status to `Ratified <date>`.
1. **`REL.1`** (operator, any time): re-run `merge_dryrun.sh` over the open stack, merge bottom-up per `INTEGRATION_PLAN.md §(d)`, `make check` on `main`, tag `v0.1.0`, `make sbom` + release. *(May precede or follow EL.1; the stack stays valid either way.)*
2. **Lane A human work** — `GOV.1`, `LEGAL.1`, `ACCT.1`: real-world actions; record answers in `docs/build/LEDGER.md` GATE DECISIONS and sign the HUMAN/GATE markers. Secrets go into the operator's secret manager / the worker's shell env, never into any file (validator greps token shapes).
3. **Go-live re-runs** — as each gate opens, tick it and re-run the named ticket file (Lane B). Order: `RIGHTS.1` (P21.1) → `LIVE.1` (P21.3 + doc connectors) → `LIVE.2` (P21.4 → publish → Go-public → `v0.2.0`) → `INFRA.1` (P21.5) → `CONTRIB.1` (P21.7) → `SOURCES.1` (P21.8/9).
4. **Ratify Round 4** once OKC is live, then run `DEPLOY.1`, `SCHED.1`, `OBS.1`, `CI.1`, `META.1`, then `JURIS.2` and `CCOPS.1`.

Each Lane B/C session is a single `implement-spec` run on the manual floor (or `orchestrate-build` if you want the loop). `gh` authenticated, write perms for the worker.

---

# Part I — Design

## 1. Problem, goals, non-goals

**Problem.** The system is built and proven on fixtures but is inert: 0 sources flipped, no real fetch, no publication, no deployment, and ~10 human/legal/ops gates unsatisfied. The build deliberately stopped at "the system runs; going live is a human decision." This spec turns that decision into an ordered, checkable path.

**Goals.**
1. A single gate register that names every human prerequisite, who owns it, what unblocks it, and what it blocks.
2. Go OKC live end-to-end (real fetch → resolve → publish) behind the existing gates, using the tickets already built — re-run, not rewritten.
3. Productionize: real hosted infra, re-ingest cadence, observability, CI that runs the composed stack, backups + restore.
4. Prove the federation design with a second jurisdiction, and close the one genuinely-missing connector class (CCOPS).
5. Zero fabricated readiness: fixture-backed stays fixture-backed until a gate opens; secrets never enter a file; nothing publishes without the two-reviewer + counsel gates.

**Non-goals.** No change to the built pipeline's contracts (append-only, wire names, schema). No new reconciliation/inference logic. No automated OSM edits (human-mediated only). No merging/tagging by any worker (operator, per `INTEGRATION_PLAN.md`). No third jurisdiction here (JURIS.3+ is a later round).

## 2. Gate register (Lane A)

Each row becomes a `HUMAN-H<k>` or `GATE-G<k>` marker; the operator signs it; its readout lands in `docs/build/readouts/`. `blockedOn` is never set for a pending gate — it is a DEFERRALS row.

| Gate | Requirement | What unblocks it | Owner | Blocks (GL ticket) |
|---|---|---|---|---|
| HG-05 | Integration & release | merge #47–#68 bottom-up, tag `v0.1.0` | operator | (nothing downstream strictly; milestone) |
| HG-01 | Legal home named (SIG-GOV-012) | operator names the legal entity in `docs/governance/governance-and-code-of-conduct.md` | operator/legal | LIVE.2 publish |
| HG-11 | Operating governance | two reviewer **roles** + written concurrence workflow (SIG-PUB-008) + live takedown/corrections contact | governance | LIVE.2 publish |
| HG-02 | Counsel sign-off | counsel opinion recorded for ODbL 4.4(b) (RISK-P0-01), officer-naming gate, publication tiers, Part VIII | counsel | INFRA.1 OSM export, LIVE.2 publish |
| HG-03 | Per-source rights flips | reviewer reads the 27 packets in `docs/build/rights/`, sets `ingestion_permitted=true` + review metadata | reviewer | RIGHTS.1, LIVE.1, SOURCES.1 |
| HG-04 | Stage-0 outreach | perform + record outreach to the 19 compact projects (SIG-CONTRIB-012/013) | operator | RIGHTS.1, SOURCES.1 |
| HG-07 | Deposit/object-store accounts | Zenodo (sandbox+prod), S3-compatible store, optional SWH token | operator | INFRA.1 |
| HG-08 | Contribution accounts | MapRoulette API key + registered OSM Organised-Editing page | operator | CONTRIB.1 |
| HG-09 | API tokens | `SIG_MUCKROCK_TOKEN`, `SIG_DATA_GOV_KEY`, `SIG_OVERPASS_ENDPOINT`, `SIG_CIVICCLERK_BASE` | operator | LIVE.1 |
| HG-10 | Usability participants | ≥5 naïve participants scheduled (roles only) | operator | CONTRIB.1 study |
| HG-12 | Hosting/budget | a real (zero/low-cost) host + object store + PG target beyond local | operator | INFRA.1, DEPLOY.1 |
| Go-public | DNS/host cutover | operator decision after HG-01/HG-11 | operator | LIVE.2 → `v0.2.0` |

## 3. Decisions (first principles)

- **D1 — Re-run, don't rewrite (GL-META-00).** The seven gate-skipped tickets are complete contracts; going live is ticking their gate and re-running the same file. This spec adds their `GL-*` id and points at the DEFERRALS row EL.1 seeded; it authors no duplicate contract.
- **D2 — Human gates are markers, signed, with readouts.** A gate is a pause, never a block. The register (§2) is the source of truth; markers carry the sign-off; secrets are `provided: yes/no` only.
- **D3 — Secrets via a manager/env, never a file.** `ACCT.1` provisions credentials into the operator's secret manager (or the worker's shell env at run time). `check-build-memory.sh` fails on token shapes in any committed file. The `.env*` gitignore + no-token-literal test from P21.3/P21.5 stand.
- **D4 — Zero-cost posture is the default (SIG-STORE-003).** `DEPLOY.1`/`INFRA.1` target a single small host + object store + static host; CDN/egress are templates with a live alarm; the `.torrent` + Software Heritage mirrors remain the free succession path; degraded mode + keepalive stay the $0 floor.
- **D5 — Live is proven by the same composed E2E, on real data.** `LIVE.2` re-runs `run_okc.sh` in `--mode live` for green sources; the acceptance queries (J-1, Q-1…Q-13) run against the running stack; the 299-vs-190 contradiction must survive to the published page. No metric is a single whole-set number.
- **D6 — The go-live round gets a minimal tail, not a full capstone.** EL.1 already stands up the Round-2 delta capstone (`P23.*`) over the migration. Round 3 closes with a short `REC`-style readiness delta + `GATE-ACCEPT` (operator re-signs the accepted-deviations list if it changed); a full `CAP.1–CAP.3` is only warranted if Round 4's new code is large (decide at `mode=extend` time via `tail=minimal|full`).
- **D7 — The second jurisdiction is the design's proof, not a copy.** `JURIS.2` reuses `run_okc.sh` as a template but rights-reviews its own sources and exercises the jurisdiction-adapter framework (P18.1) — it is the first real test that the federation design generalizes.

## 4. Critical path

```
                (EL.1 lands: v2 layout + DEFERRALS seeded)
                              │
        REL.1 (merge/tag v0.1.0) ──────────────┐ (milestone, non-blocking)
                              │
   GOV.1 ─┐  LEGAL.1 ─┐  ACCT.1 ─┐             │
          ▼           ▼          ▼             │
   HG-01/11        HG-02      HG-03/04/07/08/09/10/12
          └─────┬─────┴───────────┬────────────┘
                ▼                  ▼
        RIGHTS.1 (P21.1 re-run: flips + outreach)
                ▼
        LIVE.1 (P21.3 re-run + 3 OKC doc connectors) ── needs HG-03/09
                ▼
        LIVE.2 (P21.4 re-run → publish → Go-public → v0.2.0) ── needs HG-01/11/02
                ▼
   INFRA.1 (P21.5) · CONTRIB.1 (P21.7) · SOURCES.1 (P21.8/9)   [parallel once gated]
                ▼
   ───────────── ROUND 4 (ratify after OKC live) ─────────────
   DEPLOY.1 → SCHED.1 → OBS.1 → CI.1 → META.1 → JURIS.2 → CCOPS.1
```

---

# Part II — Tickets

> Round 3 IDs map to the gate register; Lane B tickets carry a **Re-run** line (the original ticket's `Run:`), not a fresh contract. Lane A/C carry full contracts. `decompose-spec mode=extend` assigns `NN_<ID>__slug.md` filenames after EL.1's `P23.*` tail.

## Round 3 — Go live for Oklahoma City

### REL.1 — Integrate & release v0.1.0 (GL-REL-01)
- **Kind:** operator action + verify · **Gate:** HG-05 · **Depends:** the open stack (#47–#68); EL.1 optional-before/after.
- **Goal:** land the whole tested machine as `v0.1.0` without changing behaviour.
- **Steps:** re-run `docs/build/tools/merge_dryrun.sh` (expect 0 conflicts); merge #47–#68 bottom-up per `INTEGRATION_PLAN.md §(d)`, retargeting each next base to `main`; `git checkout main && git pull`; `make check` green on `main`; `git tag -a v0.1.0`, `make sbom`, `gh release create v0.1.0 … sbom.cdx.json`; verify CI green on `main`.
- **Acceptance:** `git tag -l` contains `v0.1.0`; `main` CI green; `docs/build/CHANGELOG.md` `0.1.0` dated; every PR #47–#68 merged (or the delta recorded).
- **Out of scope:** any code change beyond conflict resolution.

### GOV.1 — Legal home & operating governance (GL-GOV-01, HUMAN-H_a + GATE)
- **Kind:** human + small doc · **Gate:** HG-01, HG-11.
- **Deliverables:** name the legal home in `docs/governance/governance-and-code-of-conduct.md` (SIG-GOV-012); define **two reviewer roles** + the written-concurrence workflow (`ReviewerConcurrence`, SIG-PUB-008); stand up a live takedown/corrections contact (org channel only, no personal data). Sign the marker; readout to `docs/build/readouts/`.
- **Acceptance:** governance doc names the home + the two roles + the contact; `docs/build/PUBLICATION_CHECKLIST.md` HG-01/HG-11 rows tick with evidence links.

### LEGAL.1 — Counsel sign-off (GL-LEGAL-01, HUMAN-H_b)
- **Kind:** human · **Gate:** HG-02.
- **Deliverables:** recorded counsel opinions for: ODbL 4.4(b) disposition (supersede the operator's interim disposition with a real one, RISK-P0-01); officer-naming gate; publication tiers + sensitive-coordinate rules; Part VIII compliance of the published surface. Record in a governance doc + the GATE DECISIONS table (opinion summary, not the full privileged text).
- **Acceptance:** each item has a dated counsel disposition; if any tightens the interim posture, the affected `COVERAGE_MATRIX`/ADR note is updated (append-only) and INFRA.1/LIVE.2 consume it.

### ACCT.1 — Provision accounts, credentials, hosting (GL-ACCT-01, HUMAN-H_c)
- **Kind:** human/ops · **Gate:** HG-07, HG-08, HG-09, HG-12.
- **Deliverables:** create/record (as `provided: yes/no`, never values): Zenodo (sandbox + prod), S3-compatible object store + optional SWH token, MapRoulette key + registered OSM OE page, MuckRock/data.gov/Overpass/CivicClerk tokens, and a real host target. Store secrets in the operator's secret manager; document the env names each re-run needs.
- **Acceptance:** every `SIG_*` env name the re-runs need has a `provided: yes` row; `check-build-memory.sh` finds no token literal in any file.

### RIGHTS.1 — Rights review + flips + outreach (GL-RIGHTS-01, re-run P21.1)
- **Kind:** return-pass · **Gate:** HG-03, HG-04 · **Depends:** GOV.1 (reviewer roles exist).
- **Re-run:** `implement-spec spec=docs/tickets/P21.1__rights-review-and-registry-completion.md live_verification=false` after ticking HG-03/HG-04 in GATE DECISIONS.
- **Work:** reviewer decides which of the 27 packets flip to `ingestion_permitted=true` (reviewer role + date + metadata); record Stage-0 outreach outcomes into `STAGE0_OUTREACH_RECORD.md`.
- **Acceptance:** `sig-connectors review-status` shows the flipped sources fully green; `sig-connectors validate` `loadable now ≥ 1`; DEFERRALS `D-P21.1-*` closed.

### LIVE.1 — First real fetches + OKC document connectors (GL-LIVE-01, re-run P21.3 + new code)
- **Kind:** return-pass + **new code** · **Gate:** HG-03, HG-09 · **Depends:** RIGHTS.1, ACCT.1.
- **Re-run:** `implement-spec spec=docs/tickets/P21.3__live-connector-wiring.md live_verification=true` with `SIG_*` env set.
- **New code (the one real build item in Round 3):** implement the three document-connector modules P21.3 could not exercise — `okc_procurement`, `okcpd_policy`, `ok_statute` (fetch the cited PDFs/HTML from `okc_sources.json`, capture to OCFL, parse via `sig-parsing`, emit the same claims the fixtures encode; `shadow_replay` diff = 0 against fixtures). Close BL-023/024/026.
- **Acceptance:** for each green source, one live run into PG with a fetch record; captures in OCFL; idempotent re-run; the three doc connectors pass `shadow` byte-identity + a live smoke; DEFERRALS `D-P21.3-*` closed.

### LIVE.2 — OKC live ingest → publish → go public (GL-LIVE-02, re-run P21.4)
- **Kind:** return-pass · **Gate:** HG-01, HG-11, HG-02, Go-public · **Depends:** LIVE.1, GOV.1, LEGAL.1.
- **Re-run:** `implement-spec spec=docs/tickets/P21.4__first-jurisdiction-ingest-and-publish.md live_verification=true`.
- **Work:** run `run_okc.sh` in live mode over the green OKC sources; build exports + web from the export; J-1 + Q-1…Q-13 against the running stack; hostile-reader review on live pages; complete `PUBLICATION_CHECKLIST.md`; on Go-public, cut over and bump `0.2.0` "first public jurisdiction".
- **Acceptance:** the public OKC dossier serves the 299-vs-190 contradiction with both sources + dates; `/terms` + `robots.txt` served; officer-naming + publication tiers verified on live data; DEFERRALS `D-P21.4-*` closed.

### INFRA.1 — Real deposit, object store, tiles, mirrors (GL-INFRA-01, re-run P21.5)
- **Kind:** return-pass · **Gate:** HG-07, HG-12, HG-02 (ODbL export) · **Depends:** ACCT.1, LEGAL.1.
- **Re-run:** `implement-spec spec=docs/tickets/P21.5__infra-deposit-and-tiles.md live_verification=true`.
- **Work:** real Zenodo concept DOI; object-store push + CDN + **live egress alarm**; SWH save; mirror manifest. Keep the ODbL compartment separate per LEGAL.1.
- **Acceptance:** `DEPOSITS.md` carries a real (non-sandbox) DOI; egress-report reads live usage; DEFERRALS `D-P21.5-*` closed.

### CONTRIB.1 — Contribution-back live + usability study (GL-CONTRIB-01, re-run P21.7)
- **Kind:** return-pass · **Gate:** HG-08, HG-10 · **Depends:** ACCT.1, LIVE.2 (real leverage data).
- **Re-run:** `implement-spec spec=docs/tickets/P21.7__contribution-back-live.md live_verification=true`.
- **Work:** register the OE page (`registered=true`); real MapRoulette challenge + OSM changeset feed → LeverageLedger; run the ≥5-participant usability study; record results + onboarding fixes.
- **Acceptance:** a live challenge exists; the §7 metric page shows real leverage; `USABILITY_STUDY.md` reports median ≤10 min (or the finding); no OSM usernames stored; DEFERRALS `D-P21.7-*` closed.

### SOURCES.1 — Flip + fetch reviewed ecosystem/pathway sources (GL-SOURCES-01, re-run P21.8/9)
- **Kind:** return-pass · **Gate:** HG-03/HG-04 per source · **Depends:** RIGHTS.1.
- **Re-run:** `implement-spec spec=docs/tickets/P21.8__data-driven-and-coarse-international.md live_verification=true` and `…P21.9__stage5-pathway-connectors.md live_verification=true`.
- **Acceptance:** each flipped source has a live run + fetch record; INGEST-043*/pathway ids move to MET-with-live-evidence; DEFERRALS `D-P21.8-*`/`D-P21.9-*` closed.

## Round 4 — Productionize & scale (ratify after OKC is live)

### DEPLOY.1 — Real hosted deployment on GCP + backups (GL-DEPLOY-01)
- **Target:** GCP project **`zeta-medley-508121-u7`** (name `eleutheria`). Zero/low-cost design (SIG-STORE-003): **GCS** bucket(s) for the static site + `sig-exports` output + deposits/mirrors (public-read on the published compartment only); **Cloud Run** for the API (min-instances 0, scales to zero); Postgres+PostGIS on the smallest **Cloud SQL** tier **or** a single **`e2-micro` GCE** running the existing `ops/docker-compose.yml` (pick one in ADR-`DEPLOY`, keep the other documented). TLS via the managed cert / Cloud Run default; secrets from **Secret Manager** (never in a file).
- **Deliverables:** `ops/gcp/` infra-as-code (a Terraform module *or* idempotent `gcloud` scripts) parameterised by project id/region; a `sig-ops deploy --target gcp` path that builds + pushes the API image (Artifact Registry) and syncs `web/dist` + exports to GCS; **automated backups** (Cloud SQL automated backups or `pg_dump` + OCFL sync to a GCS backup bucket) and a **documented + tested restore drill** (rebuild the graph from backup + the Zenodo deposit); a cost note vs the GCP free tier.
- **Gate:** the infra-as-code is **written + validated** (`terraform validate` / `bash -n` / a dry-run plan) autonomously; the real `apply`/deploy is **gated on operator `gcloud` ADC** in the run shell (HG-12) — an isolated subagent lands it in prepare mode.
- **Acceptance:** `terraform validate`/plan (or the gcloud dry-run) is green; with ADC present, `sig-ops deploy --target gcp` brings up API + static + PG and the OKC dossier serves from the GCS/Cloud Run URL; a restore drill reproduces the graph; monthly cost documented against the free tier.

### SCHED.1 — Re-ingest cadence & orchestration (GL-SCHED-01)
- **Goal:** the deferred scheduling seam (P21.3 backlog): cadence-driven `sig-connectors run` per source (respecting `PoliteFetcher`/Overpass etiquette), freshness tracking, and the disappearance-detection cadence, via a minimal scheduler (cron/GitHub Actions first; Prefect/Dagster only if warranted). ADR + `RISK` for cadence vs source etiquette.
- **Acceptance:** a scheduled run re-ingests a source, produces new dated claims (never overwrites), and records freshness; a source going dark triggers a disappearance record.

### OBS.1 — Observability & alerting (GL-OBS-01)
- **Goal:** metrics/logs/alerting for the live stack; wire the egress-budget alarm (INFRA.1) to a real notifier; keepalive verification; uptime + error budgets; log retention within the zero-cost posture.
- **Acceptance:** an egress-threshold breach and a keepalive failure each fire a recorded alert; a dashboard/readout exists; no secrets in logs.

### CI.1 — CI hardening (GL-CI-01)
- **Goal:** a CI job that runs the **composed `tests/e2e` for real** (Node + Docker present, so S8/LD-V08 runs in CI, closing the P20.4 gap honestly); nightly composed run; dependency/license/secret scanning; enforce `make docs-check` + `check-build-memory.sh` on PRs.
- **Acceptance:** CI runs `SIG_REQUIRE_DB_TESTS=1` composed suite green with the web build present; a seeded secret/license violation fails CI; nightly run reports.

### META.1 — Housekeeping backlog (GL-META-01)
- **Goal:** `pyproject.toml` descriptions for the 5 packages still marked "skeleton" (BL-052); remaining docs-drift; triage the P22+ `BACKLOG.csv` rows into DEFERRALS/backlog with real landings.
- **Acceptance:** no package description says "skeleton"; `check_backlog.py` green; BL-052 closed.

### JURIS.2 — Second jurisdiction (GL-JURIS-01)
- **Goal:** the federation proof — pick the next jurisdiction, rights-review its sources (new packets, HG-03/04 per source), and run its `run_<juris>.sh` (templated from `run_okc.sh`) end-to-end through the jurisdiction-adapter framework (P18.1). Surfaces any OKC-specific assumptions.
- **Acceptance:** the second jurisdiction ingests → resolves → publishes with its own acceptance queries green; the adapter framework needed no per-jurisdiction hack (or the hacks are recorded as backlog).

### CCOPS.1 — Government-mandated-disclosure connector (GL-CCOPS-01)
- **Goal:** the one genuinely-missing connector class (SIG-INGEST-049*, the P17-FLIP deferral): a `government_mandated_disclosure` connector for municipal surveillance-ordinance (CCOPS) disclosures, through the eight-stage framework + loader gate, per-agency aggregate rows only (Part VIII), with rights packets + review.
- **Acceptance:** INGEST-049* move from PARTIAL to MET with test evidence; a CCOPS disclosure fixture ingests to typed claims; `procured≠deployed` enforced.

---

# Appendix A — Mapping to existing artifacts

| GL ticket | Existing ticket / DEFERRALS | BACKLOG / matrix ids |
|---|---|---|
| RIGHTS.1 | P21.1 · `D-P21.1-*` | packets in `docs/build/rights/` |
| LIVE.1 | P21.3 · `D-P21.3-*` · BL-023/024/026 | doc-connector ids |
| LIVE.2 | P21.4 · `D-P21.4-*` | SIG-PUB-*, SIG-UI-* |
| INFRA.1 | P21.5 · `D-P21.5-*` | SIG-STORE-003/004/005, SIG-GOV-022/023/024 |
| CONTRIB.1 | P21.7 · `D-P21.7-*` · BL-041 (asset-promotion, if folded) | SIG-CONTRIB-014.. |
| SOURCES.1 | P21.8/P21.9 · `D-P21.8-*`/`D-P21.9-*` | SIG-INGEST-043* |
| META.1 | BL-052 | pyproject descriptions |
| CCOPS.1 | P17-FLIP-01 (OPEN FINDING) | SIG-INGEST-049* |

# Appendix B — Ratification answers (2026-09-09, operator-delegated)

1. **Merge timing (REL.1):** operator's call; the chain does not depend on it. Recommendation stands: tag `v0.1.0` at REL.1 (tested milestone), `v0.2.0` on first-public. **REL.1 is left as an operator step, not an autonomous ticket** (merging is an operator action per `INTEGRATION_PLAN.md`).
2. **EL.1 vs REL.1 order:** EL.1 landed first (PR #69); merge whenever. **Answered.**
3. **Legal home + counsel:** no counsel engaged; publish everything. → interim legal-home posture + engineering disposition permitting publication, both honestly labelled (see §0.1 GL-GATE-01/02). Real legal home + counsel = pre-public-cutover human action.
4. **First flips (HG-03):** yes — the four government-records sources + `osm_overpass`/`deflock` first, end-to-end, then the rest; news → LINK-only (see §0.1 GL-GATE-03). **Answered.**
5. **Host (HG-12):** GCP project `zeta-medley-508121-u7` (see §0.1 GL-GATE-04 + DEPLOY.1). **Answered.**
6. **Second jurisdiction (JURIS.2):** deferred to the operator at JURIS.2 time (candidate selection is a research step); the ticket ships the templating + adapter exercise regardless. **Deferred, non-blocking.**
7. **Round 4 tail:** `tail=minimal` (a readiness delta, not a fresh full capstone — PR #68 already capstoned the composed build). **Answered.**
