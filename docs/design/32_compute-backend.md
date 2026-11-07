# 32_compute-backend — Compute backend recipe, teardown and cost accounting
Stream: B
Researched: 2026-11-06
Author: design worker for ledger row B3 (Claude Code subagent, claude-opus-5-5), dispatched by the experiments-program coordinator
Ledger rows covered: B3
Repository baseline: main 09fe8523 (R5.59 merged), read in worktree `claude/experiments-spec` at a8a45660. Relative to main, that branch adds only the program's charter, ledger and notes; `git diff --stat 09fe8523 HEAD -- crates scripts MAINTAINING.md docs/adr` is empty.
Confidence overall: high for the round-5 facts (recorded execution, inspected) and for GCE prices and features (primary pages, retrieved today); medium for the sizing and wall-time estimates (inferred from declared timeouts and other notes' priors, not measured); medium for the topology recommendation (a design proposal that no execution has tested)

## Scope, sources and method

Row B3 asks for five things:

- a committed, re-runnable GCE recipe;
- sizing and quotes against B1's resource census;
- an unskippable teardown;
- a structured compute-cost record, reconciled with model spend;
- the security and operations rules, the HUMAN/GATE rows and the ticket shape that follow from them.

This note owns compute provisioning, teardown, the instance-hour, disk and egress cost fields, and compute-spend reconciliation. B2 (`design/31_experiment-runner.md`) owns the runner and the slot-record envelope. C2 (`design/33_model-mediation.md`) owns the model proxy and the model-spend fields. Neither note existed when this one was written, so the interfaces below are stated as proposals for them to consume.

**Nothing was executed.** No `gcloud`, `podman`, `harbor` or provisioning command ran, and nothing was spent. The deferrals-first check `bash scripts/deferrals-open.sh` returned empty output with rc 0, which means zero open rows (2026-11-06T23:20Z).

**Sources read:**
- **Program files.** Brief §2–§3 (A-3, A-10); ledger row B3, §Q (Q-X3, Q-X11, Q-X17, Q-X30, Q-X31, Q-X32), the Execution contract and the outline trace (OL-08, OL-16, OL-17).
- **Program notes.** `research/25_tb21-harbor.md` (B1: resource census, F8–F12, F15, F19, Appendix A); `research/24_executed-record-audit.md` (A1: F11 and the driver inventory); `research/26_ollama-glm.md` (C1: F9, F11); `research/27_harness-survey.md` (C3: the pilot-size table and the concurrency finding).
- **Round-5 records.** `~/tb-work/gce-backend.md`, the operator-local, uncommitted backend record; `docs/build/runs/R5.59.md` read at `09fe8523`; `docs/build/runs/S4-COMPLETION.md` and `S4-30SLOT-REPAIRS.md`.
- **Repository code and rules.** `crates/hh-helper/src/podman.rs`; `crates/hh-env/src/study.rs` (`CostFacts`, `evaluate_costs`); `crates/hh-eval/src/exposure_quote.rs` (vocabularies and §9.2 ceilings); ADR-0363 and ADR-0420; `docs/build/reports/live/pricing.json` and `SPEND.jsonl`; `scripts/live-verify.sh` (leg registry and `budget_gate`); `MAINTAINING.md` (Evidence classes, Write integrity); `docs/build/tools/README.md`; the `scripts/` listing.

**Web sources (primary, Google Cloud), all retrieved 2026-11-06:**
- `https://cloud.google.com/products/compute/pricing/general-purpose` (23:19:22Z; raw HTML, static price tables);
- `https://cloud.google.com/compute/vm-instance-pricing` (23:18:40Z; billing-model prose only, because its tables are script-rendered);
- `https://cloud.google.com/compute/disks-image-pricing` (23:18:40Z);
- `https://cloud.google.com/vpc/network-pricing` (23:20:01Z);
- `https://docs.cloud.google.com/compute/docs/instances/limit-vm-runtime`, `…/billing/docs/how-to/budgets`, `…/iap/docs/using-tcp-forwarding` and `…/sdk/gcloud/reference/compute/instances/create`, all read 23:21–23:23Z through a summarizing fetcher. These pages carried no last-updated date in the fetched content.

The pricing pages' region selector lists Iowa (us-central1) first. The prices I quote are the static default table. I infer that table is us-central1 from the selector order and from its exact match with the round-5 operator figure ($0.268/h for e2-standard-8); I did not prove it.

## The round-5 backend as it actually ran (baseline)

| aspect | round-5 fact | source |
|---|---|---|
| project / VM | dedicated project `harnessharness-evidence-8d7a99`; VM `podman-s4-backend`, us-central1-a | `gce-backend.md`; R5.59 §DEPROVISION |
| shape | e2-standard-8 (8 vCPU / 32 GiB), on-demand; 200 GB `pd-balanced`; debian-12; external IP; labels `purpose=s4-completion`, `ttl=deprovision-on-done` | `gce-backend.md` |
| container runtime | podman 4.3.1 **rootful**; systemd `podman-remote.service` (`Restart=always`); socket `/tmp/podman.sock` **mode 0666** | `gce-backend.md` |
| runner topology | runner, store, ledger, observer and sentinel ran **on the laptop**. The local podman CLI reached the VM through a `gcloud compute ssh -N -L` forward (`CONTAINER_HOST=unix:///tmp/podman-gce/podman.sock`, plus `DOCKER_HOST=tcp://localhost:23750`) | `S4-COMPLETION.md` 33–63 |
| workspace | the cell's `/work` bind mount names a *host* path, which on remote podman lives on the VM. Each slot's workspace was pushed and pulled as tar streams over `gcloud compute ssh`: 130 syncs in the completion epoch, 120 in repairs | `S4-COMPLETION.md` 55–63, 203–205; repairs fixture `remote_ws_syncs_total: 120` |
| scheduling | cells ran sequentially. Slot wall was 1,124,199 ms (completion) plus 2,668,051 ms (repairs), about 63 min in total | `S4-COMPLETION.md` 216; `S4-30SLOT-REPAIRS.md` 189 |
| cost record | prose only: "~2.4 h … ≈$0.72" and "~4.7 h … ≈$1.41", labelled "billed" | same, plus R5.59 |
| teardown | R5.59 AC-3: forwards killed, then `instances delete` and `projects delete`, plus post-state commands, at 2026-11-06T22:15:40Z | R5.59 §DEPROVISION |

## Findings

### F1 — HH's podman backend has no remote awareness; round 5 bridged it with an SSH socket forward and a per-dispatch tar mirror of `/work`
Claim: `hh-helper` drives podman by spawning the local `podman` CLI and bind-mounting a host path at `/work`. A remote podman therefore needs a workspace mirror on the VM. Round 5 supplied one in scratch drivers: about 4 SSH tar syncs per cell, outside committed code.
Status: VERIFIED
Evidence kind: source inspection + recorded execution (inspected)
Evidence:
- `crates/hh-helper/src/podman.rs` 1–30 and 71–80: `Command::new("podman")` with no host, URL or connection argument; `is_available` runs `podman info`.
- 101–126: `create_args` emits `-v {workspace}:/work:rw`.
- `rg 'remote_ws|CONTAINER_HOST|remote_host' crates/` matches only the r5_49 test fixture's recorded strings, not code.
- `S4-COMPLETION.md` 55–63 ("only the container-visible `/work` tree is mirrored"); A1 inventory row `s4-cell/` ("`gcloud compute ssh` sync").
Retrieved: 2026-11-06T23:17Z
Implication for the spec: Any committed design that keeps the runner off the VM must either commit the mirror (new machinery, about 4 SSH round-trips per slot) or teach `hh-helper` about remote volumes. Running the runner **on** the VM removes the problem: the host path and the container host are the same machine (F10).
Outline delta: EXTENDS OL-17 — topology is part of the recipe, not an operator convenience.
Limitations: I did not read the archived scratch `s4-cell` source. The sync count comes from the records.

### F2 — No provisioning recipe is committed; the backend exists only as an operator-local record
Claim: The round-5 VM was created by hand. Its recipe, shape, socket setup and costs live in `~/tb-work/gce-backend.md`, which is not in the repository. No script under `scripts/` or `docs/build/tools/` provisions or tears down compute.
Status: VERIFIED
Evidence kind: source inspection
Evidence:
- The `scripts/` listing (40 entries) contains no gcloud or backend script; `rg -l 'gcloud'` over `scripts docs/build/tools` finds nothing.
- `docs/build/tools/README.md` scopes that directory to the supplement guard ("It never writes build state").
- `gce-backend.md` records the bring-up only as results, not as commands.
Retrieved: 2026-11-06T23:18Z
Implication for the spec: A-10's "committed recipe" is new work. Its home is `scripts/`, which holds operational and live scripts such as `live-verify.sh`. `docs/build/tools/` is the guard's read-only home and should not host a mutating tool.
Outline delta: CONFIRMS OL-17 (re-provisioning must be a recorded act); EXTENDS — the recipe is a ticket deliverable.
Limitations: The operator's shell history was not inspected, so the exact creation flags are unknown, for example whether a service account was attached.

### F3 — The round-5 compute figure understates the VM's lifetime and mislabels an estimate as billed
Claim: The VM existed from 16:32Z (creation as recorded) to 22:15:40Z (delete), about 5.73 h. The recorded "≈4.7 h … ≈$1.41" stops at the repairs run's end and omits the closure hour, the external-IPv4 charge and any billing read.
- At this note's rate card (e2-standard-8 + 200 GB `pd-balanced` + one in-use IPv4), 5.73 h ≈ **$1.72**.
- Utilization was about 18 % (63 min of slot wall over 344 min of VM existence).
Status: VERIFIED (timestamps and arithmetic from records); the dollar figure is a rate-card estimate, not billing
Evidence kind: recorded execution (inspected) + inference
Evidence:
- `S4-30SLOT-REPAIRS.md` 189–194 ("instance created 2026-11-06T16:32Z … ~4.7 h billed … ≈ **$1.41**"), and 16:32Z + 4.7 h = 21:14Z.
- R5.59 §DEPROVISION: "UTC `2026-11-06T22:15:40Z`" and "**Cumulative billed ≈ $1.41**".
- The rates are in F5.
- The creation time is said to be "recorded in `~/tb-work/gce-backend.md`", but that file as read at 23:16Z has no creation timestamp.
- A1 F11 independently found the figure prose-only.
Retrieved: 2026-11-06T23:24Z
Implication for the spec:
- Compute cost must come from resource-lifecycle timestamps (GCE `creationTimestamp` and the delete operation's end), not from run windows (F15).
- "Billed" is reserved for a billing read.
- A2's methodology record should state round-5 compute as "rate-card estimate ≈ $1.4–1.7, unbilled read". A dated correction is a candidate below (REQ-XB-222).
- Idle time is the dominant compute cost when runs are sequential or gated.
Outline delta: CORRECTS the brief §2 "~$1.41 GCE instance-hours"; EXTENDS OL-17.
Limitations: The true creation time may be earlier than 16:32Z. Billing data was not read, so free-tier credits for the IPv4 hour and egress could lower the billed figure.

### F4 — The R5.59 post-state check proves less than it appears to
Claim: R5.59's post-state `gcloud compute instances list` ran **after** `projects delete`. It returned "This API method requires billing to be enabled … Listed 0 items", which is a failed request, not an empty inventory. The real evidence of deletion is the `instances delete` output itself. Disks, addresses, snapshots, images and firewall rules were never inventoried.
Status: VERIFIED
Evidence kind: recorded execution (inspected)
Evidence: R5.59 lines 24–43: commands and outputs verbatim, including the WARNING line and `lifecycleState: DELETE_REQUESTED`.
Retrieved: 2026-11-06T23:17Z
Implication for the spec: Teardown must inventory every billable resource class **while the project and its billing still exist**, and only then delete the project (F14). "Listed 0 items" under a WARNING must be classified as `inconclusive`, not empty.
Outline delta: CORRECTS OL-17's precedent — the receipt form is right, but its post-state check needs strengthening.
Limitations: Project deletion does stop all resources' billing per the record's own note ("VM deletion stops all billing"), so the round-5 outcome was very likely clean. The defect is in the evidence, not necessarily in the state.

### F5 — Rate card for us-central1 (on-demand, USD)
Claim: The billable components of a round-5-shaped backend have the rates below. At round-5 scale, compute is about $0.30–0.31/h, about $7.2–7.5 per day.

| component | rate | source line |
|---|---|---|
| e2-standard-4 (4 vCPU/16 GiB) | $0.13402284 /h | general-purpose page, E2 standard table, "Default" column |
| **e2-standard-8 (8/32)** | **$0.26804568 /h** | same |
| e2-standard-16 (16/64) | $0.53609136 /h | same |
| e2-highmem-8 (8/64) | $0.36159864 /h | E2 highmem table |
| n2-standard-8 / n2d-standard-8 | $0.388472 / $0.337968 /h | N2, N2D tables |
| `pd-balanced` provisioned space | $0.000136986 /GiB-h (≈ $0.10/GiB-month) | disk page, "Balanced provisioned space" |
| in-use external IPv4 (standard VM) | $0.005 /h (free tier 1 h/month/account) | network page, IP address table |
| internet egress, Premium Tier, to North America | first 1 GiB/month free, then $0.12/GiB up to 1 TiB | network page |
| ingress | "No charge for inbound data transfer" | network page |
| Cloud NAT (≤32 VMs) | $0.0014/h per VM, plus $0.045/GiB processed, plus $0.005/h per NAT IP | network page, Cloud NAT table |

Billing granularity: "Compute Engine bills for a minimum of 1 minute … After 1 minute, your instance is billed on a per-second basis" (VM pricing page).
Status: VERIFIED (page text); PARTIALLY VERIFIED that the static table is us-central1 (inferred from selector order and the match with round 5)
Evidence kind: source inspection (web)
Evidence: The pages listed under Sources. The extracted strings are those quoted in the table (scratch extraction of the raw HTML; tables are static for these pages).
Retrieved: 2026-11-06T23:18:40Z – 23:20:50Z
Implication for the spec:
- The quote and cost record pin these rates in a committed rate table with URL and retrieval time (REQ-XB-213/215).
- Compute is small next to the $60 model pilot. The financial risk is unbounded idle time, not the hourly rate: a forgotten VM costs about $225/month (F14).
- Sustained-use discounts apply only after 25 % of a month. Estimates should therefore use the on-demand rate and treat any SUD as a billed-below-estimate reconciliation delta.
Outline delta: EXTENDS OL-08 (compute side of the funding model).
Limitations:
- Spot prices were not read; Spot is rejected for other reasons (F11).
- Prices change; the table is dated.
- The Cloud Billing Catalog API, the machine-readable authority, was not queried.

### F6 — GCP budgets alert but do not cap, and billing data lags by hours
Claim: Budgets "don't automatically cap … usage or spending". Default thresholds are 50/90/100 % of actual spend. The first notification "may take several hours". Monthly budgets keep tracking late usage for two days into the next month. Automated responses (for example disabling project billing) need Pub/Sub plumbing.
Status: VERIFIED
Evidence kind: source inspection (web)
Evidence: `docs.cloud.google.com/billing/docs/how-to/budgets` (quotations above).
Retrieved: 2026-11-06T23:22Z
Implication for the spec:
- A budget alert is a third, after-the-fact layer, never the guarantee. The guarantee is an enforced maximum run duration on the VM itself (F7).
- Reconciliation must wait for billing data. A cost record therefore carries `billed: pending` at teardown and is completed by a later append (REQ-XB-216).
- The alert is a billing-account action, i.e. a HUMAN row.
Outline delta: EXTENDS OL-17, OL-08.
Limitations: The exact lag is unstated. The Pub/Sub disable-billing pattern was not designed here (it is rejected as new infrastructure).

### F7 — GCE can enforce a hard VM lifetime: `--max-run-duration` with STOP or DELETE
Claim: The GA `gcloud compute instances create` accepts `--max-run-duration` (relative to the last start) or `--termination-time`, with a termination action of `STOP` or `DELETE`.
- Standard VMs require the action.
- The range is 30 s to 120 days, and "automatic termination might take up to 30 seconds longer".
- The timestamp is cleared when the VM is stopped or suspended, and recalculated on restart for duration-based limits.
- Boot disks auto-delete with their instance by default.
Status: VERIFIED
Evidence kind: source inspection (web)
Evidence:
- `limit-vm-runtime` page: flags, limits, caveats, logged operations `compute.instances.deferredStop`/`deferredDelete`.
- `gcloud … instances create` reference: "--boot-disk-auto-delete … Enabled by default"; `--max-run-duration` is documented on the GA page.
Retrieved: 2026-11-06T23:21–23:23Z
Implication for the spec:
- Every provision sets a dead-man duration `D` from the authorized quote (REQ-XB-209), which makes instance-hours a hard bound of `D`.
- **STOP** is recommended over DELETE. STOP stops vCPU/memory billing and keeps the disk, and with it the unharvested results, at about $0.03–0.04/h. DELETE would destroy evidence that teardown must harvest.
- An in-guest timer (`systemd-run --on-active=D shutdown -h now`) is a second, independent layer.
- Because the stop clears the timestamp, a restart after a dead-man stop must be a new authorization.
Outline delta: EXTENDS OL-17 — "unskippable" gets a physical enforcement, not just a ticket AC.
Limitations:
- I did not verify whether the limit can be changed on a running instance, so an extension is designed as re-authorization plus a documented command to be verified in the recipe ticket.
- I did not verify whether the ephemeral external IPv4 is released, and so unbilled, while stopped.

### F8 — IAP TCP forwarding gives SSH without open ingress, but it is not a bulk or long-lived data path
Claim: IAP TCP forwarding needs no external IP. Ingress is allowed only from `35.235.240.0/20`, and the caller needs `roles/iap.tunnelResourceAccessor` (SSH also needs instance admin). Usage is `gcloud compute ssh --tunnel-through-iap`. The page states "IAP's TCP forwarding feature isn't intended for bulk transfer of data", may rate-limit, and "automatically disconnects sessions after one hour of inactivity".
Status: VERIFIED
Evidence kind: source inspection (web)
Evidence: `docs.cloud.google.com/iap/docs/using-tcp-forwarding` (quotations above).
Retrieved: 2026-11-06T23:23Z
Implication for the spec:
- IAP suits operator control sessions: start, status, harvest.
- It is unsuitable as the container-API path for 12–70 h runs: an idle-disconnect or a laptop sleep would sever every in-flight cell.
- This, F1 and laptop-sleep risk together decide the topology (F10).
- Firewall policy: one ingress rule, tcp:22 from `35.235.240.0/20`, nothing from `0.0.0.0/0` (REQ-XB-203).
Outline delta: EXTENDS OL-17.
Limitations: No throughput figure is published. Harvest volume (records plus trajectories, likely low GiB) over IAP is assumed acceptable.

### F9 — "No public IP" costs more than an IAP-restricted ephemeral IPv4, because Cloud NAT meters inbound image pulls
Claim: A VM without an external IP needs Cloud NAT for registry, apt, PyPI and model egress. Cloud NAT charges $0.045/GiB processed in **both** directions. Pulling tens of GiB of task images and harness layers through NAT therefore costs dollars per window. An ephemeral IPv4 costs $0.005/h, and inbound transfer is free.
Status: VERIFIED (rates); inference (volume)
Evidence kind: source inspection (web) + inference
Evidence:
- Network pricing page: "No charge for inbound data transfer. However there may be a charge for resource that processes data. Services that process inbound data are … Cloud NAT"; the NAT table ($0.0014/h, $0.045/GiB, $0.005/h NAT IP).
- B1 census: 89 prebuilt images, sizes unmeasured.
Retrieved: 2026-11-06T23:20:50Z
Implication for the spec: The default is an **ephemeral external IPv4 with IAP-only ingress**: no inbound reachability, but direct outbound egress. "No public IP + Cloud NAT" is a declared alternative for an operator who prefers it, quoted with its per-GiB line.
Outline delta: EXTENDS OL-17.
Limitations: The image volume is unmeasured (B1 Q-B1-6). A 100 GiB pull through NAT would be about $4.50, which is still small, so this is a cost-and-simplicity preference, not a blocker.

### F10 — Run the runner on the VM; keep the laptop a thin, interruptible controller
Claim: The phase-2–4 runs should co-locate on the VM: the runner, `hh-helper`, Harbor (the O surface), the HH store, ledger and records, and preferably C2's model proxy. Rootful podman is then used through its **local** socket, never exposed off-host. The operator laptop only starts, observes, harvests and tears down over `gcloud compute ssh`. Losing the laptop session must not affect a running window.
Status: UNVERIFIED
Evidence kind: design proposal
Evidence (inputs, each VERIFIED elsewhere in this note or in B1/C1/C3):

| criterion | SSH-forwarded socket (round 5) | runner on VM (proposed) |
|---|---|---|
| `/work` locality | mirror needed: about 4 SSH tar syncs per slot, uncommitted (F1) | native bind mounts |
| Harbor O surface | Harbor's podman backend needs `podman` plus a compose frontend and drives `docker cp`-style copies; over a forwarded socket this is untested. Terminus-2's loop runs in the Harbor host process (B1 F12, F15) | Harbor runs where podman runs, which is its documented mode |
| run length | the pilot plans about 16 h, a tranche 70 h or more (F12). Laptop sleep or a network change kills the forward and every in-flight cell, giving correlated infra missingness (D1's concern) | a laptop outage only pauses observation |
| IAP path | not for bulk; 1 h idle disconnect (F8) | used only for control and harvest |
| durable before visible | the ledger is local while the containers are remote, so the state is split across machines | store and cells co-located; records harvested by a sha256 manifest at teardown |
| model proxy (C2) | proxy on the laptop: every model call crosses the tunnel, and its availability is tied to the laptop | proxy on the VM: cells reach it on a local bridge; the key must live on the VM (F16) |
| latency | about 4 extra round-trips per slot plus per-call tunnel latency | none added |
| socket security | round 5 used a 0666 `/tmp` socket; any VM user is root-equivalent | socket stays local, group-restricted, no forward |

Retrieved: 2026-11-06T23:25Z (synthesis)
Implication for the spec:
- B2's runner must be buildable and runnable on the VM from a pinned commit; the recipe builds `hh`, `hh-helper` and the runner there, or ships release binaries (B2's choice).
- Records are harvested by a recipe step that writes a sha256 manifest and verifies it locally before teardown proceeds (REQ-XB-210).
- The committed records are the harvested copies; the VM store is never authoritative after teardown.
Outline delta: EXTENDS OL-17; affects B2 (runner placement) and C2 (proxy placement) — flagged as a cross-note interface.
Limitations:
- Nothing here was executed.
- Building Rust on an e2-standard-8 adds roughly tens of minutes per window (unmeasured).
- If C2 keeps the proxy laptop-side to keep the key off the VM, the sleep-risk advantage shrinks for model traffic only.

### F11 — Sizing: agent phases are bound by model concurrency, oracle phases by memory and CPU; e2-standard-8 suffices for both at Ollama Pro concurrency
Claim:
- **Oracle phases.** The VM must memory-pack Σ`memory_mb` ≤ about 28 GiB and should keep CPU oversubscription low (CPU limits do not reserve). That allows about 4 concurrent trials on e2-standard-8: Harbor's default `-n 4` is safe even with four 8-GiB tasks (B1 F9).
- **Agent phases.** Useful concurrency is ≈ K/f, where K = 3 in-flight requests on Ollama Pro and f ≈ 0.78 is the model-wait duty cycle. That gives about 3.85, so about 4 concurrent trials (C1 F11, C3).
- **Bigger machines.** More vCPUs buy nothing until K rises (Ollama Max: K = 10, so about 12 trials). Then e2-standard-16 or e2-highmem-8 becomes worthwhile.
- **Spot** is rejected: preemption would inject time-correlated infra failures into long trials.
Status: UNVERIFIED (design proposal resting on VERIFIED inputs)
Evidence kind: design proposal
Evidence:
- B1 census: 1 CPU ×83, 2 ×3, 4 ×3; 2 GiB ×68, 4 ×13, 8 ×8; no GPU; storage not enforced.
- B1 F9 ("about 3 concurrent 8-GiB trials, or about 10 concurrent 2-GiB trials").
- C1 F11 ("about 3/*f* trials can run usefully in parallel").
- C3 ("At 3 concurrent requests, about 3–4 trials run at once. A 184-trial pilot then needs about 10–12 h"; duty cycle 0.78).
- F5 prices.
Retrieved: 2026-11-06T23:25Z
Implication for the spec:
- **The scheduler's admitted concurrency** C = min(⌊(host_mem − reserve)/max running `memory_mb`⌋ by packing, vCPU budget, ⌈K/f⌉ for agent phases). C is recorded per window (REQ-XB-214; it composes with B1's REQ-XB-010).
- **Queueing.** Running more trials than ⌈K/f⌉ moves waiting into C2's proxy queue *inside the agent's timeout*. That penalizes agents for infrastructure, so excess trials must wait in the scheduler instead, before their clock starts.
- **Default shape.** e2-standard-8, 300 GB `pd-balanced`, us-central1-a, standard provisioning. The 100 GB increase over round 5 costs $0.0137/h and absorbs harness-derived layers (C4).
- **Disk policy.** Pre-pull only the window's admitted images, by digest. Prune each derived harness layer after its task's slots, and stop admitting new slots below 20 GiB free.
Outline delta: EXTENDS OL-17, OL-03 (the wall-time side of the quote).
Limitations:
- Image sizes and architectures are unmeasured (B1a), and so are peak memory, real oracle durations and Docker Hub anonymous pull limits.
- The QEMU tasks (excluded per B1 F11) were not sized. Nested virtualization on E2 was not checked.

### F12 — Quote template and per-phase quotes
Claim: A compute quote is a pure function of declared inputs. It yields three figures:
- a **planning** estimate;
- a **timeout bound** (every trial runs to its declared timeouts);
- the **authorization ceiling** at the dead-man duration `D`. Because `D` is enforced by GCE (F7), this ceiling is a hard bound on instance cost.

At the default shape, every planned phase-2 window's compute is under $12 at its authorization ceiling.
Status: UNVERIFIED (design proposal; arithmetic checked)
Evidence kind: design proposal
Evidence: F5 rates; the B1 census (agent timeouts sum to 42.2 h over 89 tasks, mean 1,707 s; verifier timeouts sum to 41.4 h, mean 1,676 s); C1 and C3 concurrency; C3 pilot size (about 184 trials) and HF agent time 611–974 s per trial; Q-X31 (start with the cheapest stratum: 48 tasks have a 900 s agent timeout).
Retrieved: 2026-11-06T23:24:34Z (arithmetic)
Implication for the spec: The template below is what the provisioning gate presents (REQ-XB-213).

**Template (all money in integer USD micros, each line rounded up):**

```text
inputs:  machine_type, r_vm ($/h); disk_gib S, r_disk ($/GiB-h); r_ip ($/h, 0 if no IP; NAT lines if chosen)
         trials N; surfaces s (1 or 2); per-trial planning seconds t_p; per-trial bound seconds
         t_b = agent_timeout + verifier_timeout + setup_bound (per task, from task.toml, × declared multiplier)
         concurrency C (REQ-XB-214); overhead hours H_o (provision + build + pre-pull + smoke + harvest + teardown)
         egress bound E_gib; dead-man D (h); post-stop disk residual horizon h_r (h, until teardown deletes)
hourly   r = r_vm + S·r_disk + r_ip
planning W_p = H_o + Σ t_p /(3600·C)            $_p = r·W_p + 0.12·max(0, E_est − 1)
bound    W_b = H_o + Σ t_b /(3600·C)            $_b = r·W_b + 0.12·E_gib
ceiling  $_D = r·D + S·r_disk·h_r + 0.12·E_gib  (authorization amount; D ≥ W_p, ideally ≥ W_b)
```

**Quotes at the default shape** (e2-standard-8 + 300 GB `pd-balanced` + IPv4: r = $0.31414/h; egress under $1 in each case and omitted):

| window | trials | C | planning W_p | $ planning | timeout bound W_b | $ bound | proposed D | $ ceiling at D |
|---|---|---|---|---|---|---|---|---|
| Phase 2a — $0 oracle re-proof, 30 tasks × O+H, R=1 | 60 | 4 | 4.5 h (1.5 setup/pre-pull + 2.5 run at about 10 min/trial + 0.5 teardown) | $1.41 | 16.1 h (mean 3,383 s/trial) | $5.06 | 12 h | $3.77 |
| Phase 2a′ — same, R=3 | 180 | 4 | 9.5 h | $2.98 | 44.3 h | $13.92 | 24 h | $7.54 |
| Phase 2b — $60 pilot (C3: P0 24 + P1 160 trials), cheapest stratum, H surface | 184 | 4 | 16.3 h (13.3 run at about 1,040 s/trial + 3 setup) | $5.12 | 29.8 h (900 s + 900 s + 300 s per trial) | $9.36 | 36 h | $11.31 |
| Phase 2c — illustrative tranche, 30 tasks × 6 harnesses × 5 attempts | 900 | 4 | 69 h | $21.68 | about 234 h | about $73 | 120 h | $37.70 |
| Phase 2c on Ollama Max (K=10), e2-standard-16 + 300 GB (r = $0.58219/h) | 900 | 12 | 25.7 h | $14.96 | about 81 h | about $47 | 48 h | $27.94 |

Reading the table:
- `D` between the planning figure and the bound is a deliberate trade. A dead-man stop leaves in-flight trials `inconclusive{backend_terminated}`, which is an honest but costly outcome. The gate may choose `D ≥ W_b` instead.
- Phases 3 and 4 use the same template with their own N, s and t (D2/D3). Each phase re-provisions (Q-X17).
- Each provisioning window costs about 1–1.5 h of overhead (about $0.31–0.47) for rebuild and pre-pull. That is the price of tearing down across gate waits (REQ-XB-212), against about $7.5 per idle day.

Outline delta: EXTENDS OL-03, OL-08, OL-17.
Limitations:
- The planning seconds per trial (10 min oracle; 1,040 s agent, from HF's 611–974 s plus setup and verifier) are priors to be replaced by pilot measurements (C1 REQ-XC-015).
- Oracle durations are unknown.
- The timeout bound assumes a subset whose timeouts match the population mean; the actual draw changes it.
- Rust build time and image pulls are folded into `H_o` unmeasured.

### F13 — The landed exposure-quote ceilings (round-5 §9.2) cannot express TB 2.1 compute
Claim: `hh-eval::exposure_quote` already prices resource lines in currency micros and refuses unpriced lines, pooled capacity and top-ups, so it is the right home for the compute quote (CC1). But its frozen §9.2 ceilings would refuse every planned TB 2.1 window:
- `wall_minutes_per_subject` 30 and `verifier_minutes` 15 conflict with 900–12,000 s timeouts;
- `concurrent_subjects` 2 conflicts with C = 4;
- the pilot `cpu_core_hours` 200 conflicts with 8 vCPU × 36 h = 288 *provisioned* core-hours.

Its components also measure consumption (`cpu_core_hours`, `ram_gib_hours`), whereas GCE bills **provisioned** shape × wall time.
Status: VERIFIED (code); inference (the conflict)
Evidence kind: source inspection
Evidence:
- `crates/hh-eval/src/exposure_quote.rs` 97–121 (`COMPONENTS`, incl. `cpu_core_hours`, `ram_gib_hours`, `wall_minutes`, `storage_gib`, `egress_gib`).
- 124–136 (`ENVELOPE_COMPONENTS`).
- 139–157 (`PER_SUBJECT_CEILINGS`, `PILOT_TRANCHE`, `CONFIRM_TRANCHE`).
- 176–190 (`ceilings`).
- 258–278 (`ResourceLine.unit_price_micros`, `pool_from`, `top_up`).
- ADR-0420 Decision 1–5.
- Q-X32 (per-task declared timeouts admitted via ADR).
Retrieved: 2026-11-06T23:23Z
Implication for the spec:
- **Quote vehicle.** The experiments spec should quote compute through `exposure_quote` with provisioned quantities: `cpu_core_hours = vCPU × D`, `ram_gib_hours = GiB × D`, `storage_gib`, `egress_gib`, each priced at its GCE per-unit rate.
- **New ceilings.** It needs its own stage ceilings (per-task declared deadlines; C from REQ-XB-214). They are admitted by ADR alongside Q-X32's timeout ADR, never by widening the frozen round-5 constants in place.
- **Ownership.** This is H1's amendment register item, with D1 owning the stage vocabulary.
- **Interim alternative.** Until then, the shell recipe computes the template in F12 and records it, labelled "pre-quote", and the gate reads that.
Outline delta: EXTENDS OL-16 (the amendment protocol), OL-08.
Limitations:
- E2's per-vCPU and per-GiB unit rates were not extracted; only machine-type totals were.
- Pricing a machine type as one line (`environment_image`-style) would need a component the vocabulary lacks.

### F14 — Teardown must be unskippable in three independent layers, with a receipt-checked index
Claim: A ticket AC alone (the R5.59 precedent) depends on the chain reaching that ticket. Three layers make a forgotten VM bounded and visible:
- **Physical.** A GCE max-run-duration plus an in-guest timer, both set at provision (F7).
- **Procedural.** A teardown AC in the window's closing ticket, plus a read-only `compute-open` index over committed receipts that round or phase closure must find empty, in the style of `deferrals-open.sh`.
- **Financial.** A billing-account budget alert (F6, HUMAN).
Status: UNVERIFIED
Evidence kind: design proposal
Evidence: R5.59 §DEPROVISION (AC-3 as a mandatory AC); F4; F6; F7; `scripts/deferrals-open.sh` (the empty-output-means-zero convention, AGENTS gotcha 1).
Retrieved: 2026-11-06T23:25Z
Implication for the spec: The teardown procedure is fixed (REQ-XB-210):
1. Stop new admissions, and drain or record in-flight slots as `inconclusive{teardown}`.
2. Harvest the run root with a sha256 manifest, verified locally.
3. Capture serial output and final usage counters.
4. Inventory every billable class in the project: instances, disks, addresses, snapshots, custom images, forwarding rules and routers/NAT, plus firewall rules.
5. Delete the instance (boot disk auto-deletes, F7) and any others.
6. Re-inventory and require each class empty, or exactly the declared non-billable residue (network, firewall).
7. At phase close, delete the project, then `projects describe` must show `DELETE_REQUESTED`.
8. Kill local helper processes.
9. Write the teardown receipt and the cost record (estimate, `billed: pending`).

If harvest fails, the only admitted deviation is *stopped, disk retained*, with a dated `DF-*` row, owner and deadline. A running VM is never an admitted end state.
Outline delta: CONFIRMS and EXTENDS OL-17.
Limitations: An index can only check records, not live state. The physical layer covers that gap.

### F15 — One compute-cost record shape, one phase spend ledger, and the compute side of the `evaluate_costs` amendment
Claim:
- **Today's evaluator.** `evaluate_costs` refuses any non-zero `exposure_micros` and hard-codes 14 packets, 12 and 90 subjects. No packet has a compute field (A1 F11).
- **Precedent.** The repository's only spend ledger, ADR-0363's `SPEND.jsonl` plus `pricing.json` (estimate; provider console = authority; operator reconciles at a gate), is the precedent to extend rather than duplicate (CC1).
- **Proposal.** A `hh-compute-cost/1` record per provisioning window; compute rows in one phase spend ledger with `kind`/`authority`; and compute predicates in the amended evaluator.
Status: VERIFIED (current code and ADR); UNVERIFIED (the proposal)
Evidence kind: source inspection + design proposal
Evidence:
- `crates/hh-env/src/study.rs` 1466–1508 (`CostFacts{per_packet, subjects_first, gated_subjects}`, `evaluate_costs` returning `Refused("exposure:…")` on non-zero, `Inconclusive("packets:…")` unless 14).
- ADR-0363 D2 (`cost_usd`, `cost_pricing{table, source, retrieved}`), D4 ("`SPEND.jsonl` is an *estimate* … the provider consoles' billing is the authority"), D6 (`hh-secret-scan` single detector set), D9 (validators are scripts on a `ci-mode: both` step because crates may not read `docs/`).
- `pricing.json` (`schema: hh-live-pricing/1`, `cap_usd`, `allocations_usd`, `authority`).
- `live-verify.sh` 37–41 (bash-3.2 leg registry), 287 (`budget_gate`).
Retrieved: 2026-11-06T23:22Z
Implication for the spec: See §Cost record schema below and REQ-XB-215…218. Ownership is split:
- C2 proposes the model half of the amendment and the ledger ADR text.
- This note fixes the compute half.
- H1 registers one ADR covering `hh-live-pricing/2` (compute rate rows) and ledger rows with `kind`.
Outline delta: EXTENDS OL-08, OL-15, OL-16.
Limitations:
- Whether billing reports remain readable at billing-account scope for a deleted project was not verified (it is the reconciliation input).
- If they are not, the read must happen before project deletion, or `billed: unavailable{project_deleted}` is recorded honestly.

### F16 — Security and operations posture
Claim: The backend can be operated with no credential in any record and no GCP API authority on the VM. If C2 places the proxy on the VM, the only secret that must reach it is `OLLAMA_API_KEY`. It is delivered over SSH stdin into a tmpfs file, never via metadata, the startup script or argv, and it is destroyed with the VM.
Status: UNVERIFIED (design proposal); the round-5 socket and permission facts are VERIFIED
Evidence kind: design proposal + source inspection
Evidence:
- `gce-backend.md` (rootful podman; socket `/tmp/podman.sock` mode 0666; "No credentials stored anywhere").
- ADR-0363 D2 (`provided` names-only), D3 (redaction allowlist), D5 (names-only sourcing, `set +x`), D6 (`hh-secret-scan` leak sweep).
- The brief's A-4 (key `provided: yes`, value never recorded).
- `gcloud … create` reference (`--no-service-account`, `--no-scopes`, `--shielded-secure-boot`, `--metadata-from-file`).
- IAP page (role requirements).
Retrieved: 2026-11-06T23:23Z
Implication for the spec (REQ-XB-203, 204, 219, 220):
- **Credentials.**
  - `gcloud` auth and SSH keys stay operator-local; records carry `provided: {gcloud_auth: yes}` only.
  - Billing-account identifiers stay out of committed records; the project id suffices for attribution.
  - The VM gets `--no-service-account --no-scopes`, so a compromised cell cannot reach GCP APIs.
  - Instance metadata (readable from inside the VM and by project viewers) never carries a secret.
- **Socket.** The rootful podman socket is group-restricted (0660, a dedicated `hh` group) and is never forwarded or bound to TCP.
- **Patching.**
  - Boot from an exact image name, not a family, recorded in the receipt.
  - Install pinned podman, crun, conmon, netavark and the compose frontend (versions recorded via `dpkg-query`).
  - Disable unattended upgrades during the window, so podman never restarts mid-run.
  - The VM's bounded lifetime plus IAP-only ingress is the compensating control. There is no in-window patching.
- **Logging.**
  - The runner logs to files in the run root and to journald.
  - Harvest includes both, plus `get-serial-port-output`.
  - Cloud Logging is not used: it would need a service account.
  - All harvested text passes `hh-secret-scan` with a mask built from the key before commit (ADR-0363 D6).
Outline delta: EXTENDS OL-15 (honesty scaffolding), OL-17.
Limitations:
- The GCE default-network firewall contents and org-policy defaults for new projects were not read. The recipe therefore creates its own network and verifies the rule set instead of assuming defaults.
- Key rotation after a phase is an operator choice, not designed here.

### F17 — The podman/compose versions on Debian 12 must be verified against Harbor's preflight
Claim: Harbor's podman backend requires either `podman compose` (with the API socket) or podman-compose 1.x (B1 F12). Round 5 ran Debian 12's podman 4.3.1. I did not verify that 4.3.1 provides a `podman compose` subcommand. The recipe must pin and verify a working compose frontend, or choose an image whose podman satisfies Harbor.
Status: PARTIALLY VERIFIED (Harbor's requirement via B1; the round-5 version via its record); UNVERIFIED (compatibility)
Evidence kind: source inspection (via B1) + inference
Evidence: B1 Harbor table, "podman backend" row (`environments/podman.py`: "Preflight requires `podman` on PATH and a compose frontend"); `S4-COMPLETION.md` 35–38 (podman 4.3.1 rootful, debian-12).
Retrieved: 2026-11-06T23:17Z
Implication for the spec:
- Provision-time verification includes the following, and refuses (typed `Unsupported{backend.compose_frontend}`) rather than proceeding:
  - `podman version --format json`;
  - `curl --unix-socket <sock> http://d/_ping` → `OK`;
  - `podman compose version` or `podman-compose --version`;
  - Harbor's own preflight (`harbor` dry check, B2/C4).
- The image choice (Debian 12 vs a newer distribution) is a recipe input fixed at the recipe ticket after a $0 smoke.
Outline delta: EXTENDS OL-17.
Limitations: Version facts about podman releases were not read from primary sources here.

## Recipe design (design proposal)

**Artifact.**
- `scripts/gce-backend.sh`: bash-3.2-safe like `live-verify.sh`, using only `gcloud`, `ssh` and `jq`-free POSIX tools or `python3 -I` for JSON.
- `scripts/gce-backend/startup.sh`: the in-guest bootstrap, passed with `--metadata-from-file startup-script=`; contains no secrets; its sha256 is recorded.
- `scripts/gce-backend/declarations/<phase>-<window>.json`: the committed inputs.
- Terraform is rejected: it would be a new external tool and dependency for one VM and four resource kinds. `gcloud` is already the operator's tool and the round-5 precedent. A missing `gcloud` exits like `live-verify.sh`'s "required tool absent" (code 7).

Subcommands:

| subcommand | effect | writes |
|---|---|---|
| `quote <decl>` | computes F12's template from the declaration and the pinned rate table; no network | `…-quote.json` (pre-quote, or an `exposure_quote` input once F13's ADR lands) |
| `provision <decl> --authorization <gate-record>` | refuses without a recorded gate authorization whose quote id and ceiling match. Then creates or reuses the phase project, links billing (operator-local billing id), enables compute and IAP, creates network `hh-net` and its single firewall rule, and runs `instances create` | provision receipt |
| `verify <decl>` | podman `_ping`, version, compose frontend, `nproc`, `free`, `df`, dpkg pins, firewall-rule set == declared, SA == none, max-run-duration set | appended to the provision receipt |
| `prepull <decl>` | serial pulls of the window's admitted images by digest with bounded retry; records digest, size and arch; diskwatch threshold | pre-pull receipt (the B1a measurement leg, folded in) |
| `deliver-secret <decl> NAME` | only if C2 places the proxy on the VM. Streams the named value from the operator's names-only env file over `gcloud compute ssh` stdin into `/run/hh/secrets/NAME` (tmpfs, 0400, owner `hh`); never argv, never echoed | `provided: {NAME: yes}` |
| `status <decl>` | instance state, uptime vs D, free disk, runner liveness | stdout only |
| `harvest <decl>` | tars the run root on the VM with a sha256 manifest; copies it over `gcloud compute scp` (IAP); verifies locally | harvest manifest |
| `teardown <decl>` | the F14 sequence | teardown receipt plus `hh-compute-cost/1` |
| `inventory <decl>` | read-only, all billable classes | stdout / receipt section |

**Declaration fields.**
- `schema: hh-compute-decl/1`, `phase`, `window`, `ticket`;
- `project_id` (fresh per phase: `hh-<phase>-<6 hex>`), `region` (`us-central1`), `zone` (`us-central1-a`);
- `machine_type`, `provisioning_model: STANDARD`, `boot_disk {type: pd-balanced, size_gib: 300}`, `image {project, name}` (exact);
- `packages {podman, crun, conmon, netavark, compose_frontend}` (exact versions);
- `network {external_ipv4: ephemeral|none, nat: false|true, ingress: [{proto: tcp, port: 22, source: 35.235.240.0/20}]}`;
- `service_account: none`, `labels {hh-phase, hh-window, hh-ticket, ttl}`;
- `dead_man {max_run_duration: <D>, action: STOP, in_guest_timer: <D + 15m>}`;
- `runner {commit, build: source|release}`, `proxy_on_vm: bool`;
- `images [{task, ref, digest}]` from B2's admitted manifest; `disk_floor_gib: 20`; `concurrency` (REQ-XB-214 inputs);
- `rate_table {path, sha256}`.

**Receipt (`hh-compute-receipt/1`).**
- `schema`, `kind` (`provision|verify|prepull|harvest|teardown`), `recorded_at` (`date -u +%FT%TZ`), `ticket`, `phase`, `window`;
- `recipe {path, sha256, git_head}`, `declaration {path, sha256}`, `authorization {gate, readout_path, verbatim_sha256, quote_id, ceiling_micros}`;
- `commands [{argv_redacted, started_at, ended_at, exit, stdout_sha256, stdout_excerpt≤4 KiB, stderr_excerpt≤4 KiB}]`;
- `resources {project_id, project_number, instance_name, instance_id, zone, machine_type, creation_timestamp, deletion_operation_end, disks [{name, type, size_gib, created, deleted}], external_ipv4: present|absent}` (the address value itself is not needed and is omitted);
- `verification {podman_ping, podman_version, compose_frontend, kernel, image, dpkg_sha256, vcpus, mem_gib, disk_free_gib, firewall_rules, service_account, max_run_duration}`;
- `post_state {class → count | inconclusive{reason}}` (teardown only);
- `provided {gcloud_auth: yes|no, OLLAMA_API_KEY: yes|no}`.

All excerpts are redacted through `hh-secret-scan` and leak-swept before write (ADR-0363 D6); a hit refuses the write. Receipts land under `docs/build/reports/compute/<date>-<phase>-<window>-<kind>.json`, an allowed `reports/` root as ADR-0363 D2 uses for `live/`.

## Cost record schema sketch (`hh-compute-cost/1`, one per provisioning window)

```json
{
  "schema": "hh-compute-cost/1",
  "phase": "p2", "window": "w1-oracle", "ticket": "<id>", "recorded_at": "<UTC>",
  "provider": "gce", "project_id": "<id>", "zone": "us-central1-a",
  "receipts": {"provision": {"path": "…", "sha256": "…"}, "teardown": {"path": "…", "sha256": "…"}},
  "quote": {"id": "<quote_id>", "ceiling_micros": 11310000, "dead_man_h": 36},
  "rate_table": {"schema": "hh-live-pricing/2", "path": "…", "sha256": "…",
                 "rows": [{"sku": "e2-standard-8 on-demand", "unit": "hour", "price_micros": 268046,
                           "source": "https://cloud.google.com/products/compute/pricing/general-purpose",
                           "retrieved": "2026-11-06T23:19:22Z"}, "…"]},
  "usage": [
    {"line": "instance", "machine_type": "e2-standard-8", "vcpu": 8, "mem_gib": 32,
     "intervals": [{"state": "RUNNING", "from": "<creationTimestamp>", "to": "<stop|delete end>"}],
     "billable_seconds": 0, "source": "gcloud describe + operation log"},
    {"line": "disk", "type": "pd-balanced", "size_gib": 300, "from": "…", "to": "…", "gib_seconds": 0},
    {"line": "ipv4", "kind": "ephemeral", "seconds": 0},
    {"line": "egress", "gib_measured": 0.0, "method": "guest NIC tx counters, start→harvest", "gib_bound": 5},
    {"line": "nat", "present": false}
  ],
  "estimate": {"lines_micros": {"instance": 0, "disk": 0, "ipv4": 0, "egress": 0},
               "total_micros": 0, "rounding": "ceil per line", "free_tier_applied": false},
  "attribution": {"slot_wall_ms": 0, "utilization_ppm": 0, "idle_seconds": 0},
  "billed": {"status": "pending | reconciled | unavailable", "reason": null,
             "amount_micros": null, "source": "Cloud Billing report, project filter, read by operator",
             "read_at": null, "record": null},
  "reconciliation": {"delta_micros": null, "tolerance_micros": null, "verdict": "pending | within | outside"},
  "provided": {"gcloud_auth": "yes"}
}
```

Rules:
- **Intervals.** They come from resource lifecycle timestamps, never run windows (F3). Stop/start periods split the instance line.
- **Billable seconds.** max(60, s) per running interval, matching per-second billing with a one-minute minimum (F5).
- **Estimate.** Conservative: on-demand rates, no free tier, no sustained-use discount.
- **Append-only.** The record is never rewritten. A reconciliation is a new dated record that references the first: `hh-compute-cost/1` with `supersedes_estimate_of`, or an appended ledger row.
- **Tolerance (proposal).** `within` iff |billed − estimate| ≤ max($0.50, 10 % of estimate). Billed below estimate by free tier or SUD is expected and within tolerance. Outside tolerance requires a written explanation in the reconciliation record, and the phase total uses the billed figure.
- **Attribution.** `utilization_ppm` (Σ slot wall / instance wall, in HH's ppm convention) is reported so idle cost is visible. It does not allocate cost to slots.

**One phase spend ledger, no double counting (proposal, coordinated with C2).** Extend ADR-0363's row shape `{ts, ticket, leg, cost_usd, cumulative_usd, record}` with:
- `kind ∈ {model, compute}`;
- `authority ∈ {ollama_usage, gcp_billing}`;
- `basis ∈ {estimate, billed}`;
- `phase`, `window`.

The rules:
1. Each cost line has exactly one authority. All GCP-billed items are `compute`, including the VM time that hosts the model proxy and the VM egress that carries model traffic. Only Ollama's metered tokens are `model`.
2. A billed row for a window supersedes its estimate row by reference. Totals take the billed row when present, else the estimate, never both.
3. The phase total is Σ model + Σ compute, reported separately and combined.
4. Authorization ceilings are separate per kind. The $60 credit never pays compute (Q-X11: "$60 is a pilot only"), and compute never consumes model allocation.

This needs one ADR (`hh-live-pricing/2` plus the ledger row extension). It is not a second ledger implementation (CC1).

**`evaluate_costs` amendment — compute side (proposal; C2 owns the model side).**
1. **Round-5 era.** Existing packets carry no compute member. They fold as `compute: unrecorded{pre_schema}`, with the prose estimate cited and never coerced to $0 (honest refusal of a fabricated zero). Their model predicate (`exposure_micros == 0`) is untouched, so the R5.59 study packet's refold stays byte-stable.
2. **New-phase packets.** Each window must reference a `hh-compute-cost/1` record whose receipts exist and whose `post_state` is empty.
   - Missing record → `Inconclusive("compute_record_missing:<window>")`.
   - Teardown receipt missing, or post-state non-empty → `Refused("teardown:<window>")`.
3. **Recompute.** The evaluator recomputes `estimate.total_micros` from `usage` × `rate_table`; a mismatch → `Refused("compute_recompute:<window>")`.
4. **Authorization.** Σ compute per phase (billed, or else estimate) must not exceed the authorized ceiling; over the ceiling → `Refused("compute_over_authorization:<phase>")`.
5. **Billing status.** `billed.status = pending` folds as `Inconclusive("compute_billing_pending")` for publication purposes but does not block phase closure. `outside` tolerance with an explanation is `Verified` with the delta echoed.
6. **Packet counts.** The hard-coded counts (14 / 12 / 90) become declared expectations per study fold (C2/B2).

Docs-reading validation (records → facts) runs in a script on a `ci-mode: both` step per ADR-0363 D9. The crate evaluator stays records-in, through facts passed by the caller.

## Answer and completion assessment

**Answer.**
- **Recipe.** It is a committed bash-3.2 `gcloud` script under `scripts/` with a committed startup script and per-window declarations; no Terraform. It provisions a fresh project per phase and a VM per provisioning window. The default shape is e2-standard-8, 300 GB `pd-balanced`, an exact Debian image with pinned podman packages, no service account, and one ingress rule (IAP SSH). It carries a GCE-enforced dead-man (`--max-run-duration`, STOP) and an in-guest timer.
- **Topology.** The runner, Harbor, store and preferably the model proxy run **on the VM**. The laptop only controls and harvests (F10).
- **Quotes.** Every planned phase-2 window costs under $12 at its authorization ceiling at Ollama Pro concurrency (F12). Compute is small; idle time and forgotten VMs are the risk.
- **Teardown.** It is unskippable in three layers (F14) and verified by a pre-project-deletion inventory, which corrects R5.59's weak post-state check (F4).
- **Cost accounting.** A `hh-compute-cost/1` record per window from lifecycle timestamps; reconciliation against GCP billing with a tolerance and a pending state; one phase ledger extended from ADR-0363 with single-authority rows; and compute predicates for the `evaluate_costs` amendment (F15).

**Settled:** rates and billing granularity (F5), budget semantics (F6), dead-man mechanics (F7), IAP properties (F8), the NAT cost asymmetry (F9), the round-5 facts and their two record defects (F3, F4).

**Acceptable uncertainty for this row:** image sizes, oracle durations, Rust build time on the VM, podman/compose compatibility (F17), whether a run's max-run-duration can be extended in place, and billing visibility for deleted projects. The recipe ticket's $0 smoke and the first window's receipts resolve each of these.

**Technical blockers:** none for synthesis. One amendment item is needed before paid quotes can flow through `exposure_quote`: the TB 2.1 compute ceilings (F13). The shell pre-quote covers the interim.

## Open questions

1. **Proxy placement and the key on the VM.** Should the HH model proxy run on the VM, with `OLLAMA_API_KEY` delivered over SSH stdin into tmpfs and destroyed with the VM, or stay laptop-side behind a reverse tunnel, which keeps the key off GCP but ties model traffic to the laptop? Owner: C2 (design), operator (decision). Affects C2, B2, C4, H1. **Operator decision: yes.**
2. **Dead-man action and margin.** STOP (recommended: keeps evidence, disk residual about $1/day) or DELETE (no residual, evidence loss)? And should `D` sit at the planning×2.5 level, as in the F12 table, or at the timeout bound? Owner: B3/H1. **Operator decision: yes**, at the first provisioning gate.
3. **Provisioning windows within a phase.** Tear down across every operator gate wait (recommended; about $0.3–0.5 of re-provision overhead vs $7.5 per idle day), or keep one VM per phase as Q-X17's literal reading? Owner: H1. Operator decision: confirm at ratification. The recommendation is consistent with Q-X17's "fresh VM per phase".
4. **Billing budget and reconciliation channel.** The budget amount and recipients (for example a $50/month compute budget at 50/90/100 %). And should reconciliation be an operator console read per window (recommended, the ADR-0363 D4 pattern) or a BigQuery billing export in a persistent project (new infrastructure)? Owner: operator (HUMAN). **Operator decision: yes.**
5. **Network posture.** An IAP-only ephemeral IPv4 (recommended) or no IP plus Cloud NAT (about $0.045/GiB, including pulls)? Owner: B3. Operator decision: no, unless the operator prefers no public address.
6. **Round-5 compute correction.** Record a dated correction of "≈$1.41 billed" to "rate-card ≈$1.72 for 16:32Z–22:15:40Z; billing not read" (F3)? Owner: A2/H1. Operator decision: no (a records-honesty correction).
7. **Image and pull-limit measurement (B1 Q-B1-6).** Fold it into the first window's `prepull` receipt? Owner: B2/B3. Operator decision: no.

## Spec requirements emitted

| id | requirement (candidate) | rationale / source | OL | acceptance |
|---|---|---|---|---|
| REQ-XB-201 | The compute backend SHALL be provisioned only by a committed, bash-3.2-safe `scripts/gce-backend.sh`, with subcommands quote/provision/verify/prepull/deliver-secret/status/harvest/teardown/inventory, a committed startup script and per-window declarations. No Terraform or other new tool without an ADR. | F2, F10 | OL-17, OL-16 | Fake-`gcloud` shim test battery (REQ-XB-223) passes; `rg gcloud scripts/` shows a single provisioning home |
| REQ-XB-202 | A provisioning declaration (`hh-compute-decl/1`) SHALL declare every input in §Recipe design. The recipe SHALL refuse a missing or unknown field, an image family instead of an exact image, or unpinned packages. | F2, F16, F17 | OL-17 | Shim tests: each omission refuses with a typed reason |
| REQ-XB-203 | The VM SHALL accept inbound traffic only on tcp:22 from `35.235.240.0/20` (IAP). The default is an ephemeral IPv4 without NAT; no-IP plus Cloud NAT is a declared alternative quoted with its per-GiB line. `verify` SHALL refuse if any other ingress rule exists. | F8, F9 | OL-17 | Shim test with an extra `0.0.0.0/0` rule refuses; the provision receipt lists the rule set |
| REQ-XB-204 | The VM SHALL be created with `--no-service-account --no-scopes` and Shielded secure boot. No instance metadata key SHALL carry a secret. | F16 | OL-17, OL-15 | Shim argv assertion; the leak sweep over the declaration and startup script |
| REQ-XB-205 | Paid-phase windows SHALL run the runner, `hh-helper`, Harbor and the HH store on the VM against the local rootful podman socket (group-restricted, never forwarded or TCP-bound). The laptop session SHALL be non-essential to a running window. | F1, F8, F10 | OL-17 | Receipt `verification.socket_mode`; a recorded window survives a deliberate controller disconnect (first-window AC) |
| REQ-XB-206 | `verify` SHALL record podman `_ping` = OK, the podman version, the compose frontend version, the kernel, the image, dpkg pins, vCPU, memory and disk. A mismatch with the declaration or a missing compose frontend SHALL refuse `Unsupported{backend.<field>}`. | F17 | OL-17 | Shim tests per field; the first live receipt carries all fields |
| REQ-XB-207 | `prepull` SHALL pull only the window's admitted images, by digest, recording digest, size and architecture. The runner SHALL stop admitting slots below the declared free-disk floor (default 20 GiB) and prune derived harness layers after their task's slots. | F11, B1 F9 | OL-17 | Pre-pull receipt; diskwatch unit test in B2's runner |
| REQ-XB-208 | Every recipe step that mutates GCP SHALL write a `hh-compute-receipt/1` with the fields in §Recipe design: UTC stamps, argv, exit codes and output hashes, resource identities, verification and post-state, and `provided` names only. All text SHALL pass `hh-secret-scan` before write. | F3, F4, F16; ADR-0363 D6 | OL-17, OL-16 | Validator script on a `ci-mode: both` step parses all receipts; a seeded secret refuses the write |
| REQ-XB-209 | `provision` SHALL set `--max-run-duration=D` with `--instance-termination-action=STOP` (or the declared action) and an in-guest shutdown timer at D + 15 min, where D comes from the authorized quote. A declaration without D SHALL refuse. Restarting a dead-man-stopped VM SHALL require a new authorization. | F7 | OL-17 | Shim argv assertion; receipt `verification.max_run_duration` equals the declared D |
| REQ-XB-210 | `teardown` SHALL run the F14 sequence. The post-delete inventory runs before project deletion and requires every billable class to be empty. A WARNING or failed list is `inconclusive`, never empty. The only admitted deviation is "stopped, disk retained" with a dated `DF-*` row. | F4, F14 | OL-17 | Shim tests: a failed list → inconclusive; a leftover disk → refusal; receipt post_state complete |
| REQ-XB-211 | Each provisioning window's closing ticket SHALL carry teardown as a mandatory AC. A read-only `scripts/compute-open.sh` SHALL list provision receipts without a matching teardown receipt (empty output = zero open). Phase and round closure SHALL require it empty. | F14; R5.59 AC-3 | OL-17, OL-16 | Script fixture battery; closure checklist cites its empty output |
| REQ-XB-212 | Each phase SHALL use a fresh project and VM, never reused across phases (Q-X17). Within a phase, a VM SHALL be torn down before any operator gate whose wait is not bounded within the window's D. | Q-X17, F3, F12 | OL-17 | Receipts show distinct project ids per phase; no window spans a GATE row |
| REQ-XB-213 | The provisioning gate SHALL present a compute quote computed from the F12 template, the declared inputs and a pinned rate table (`source` URL + `retrieved` UTC per row), in integer micros rounded up per line, giving planning, timeout-bound and dead-man ceilings. Authorization SHALL name the ceiling verbatim. | F5, F12 | OL-03, OL-08, OL-17 | Quote recompute test; the gate readout carries the quote id and ceiling |
| REQ-XB-214 | Admitted trial concurrency SHALL be C = min(memory packing on host_mem − reserve, vCPU budget, ⌈K/f⌉ for agent phases), with K the model account's in-flight limit and f the measured or prior model-wait fraction. Excess trials SHALL wait before their timeout clock starts. C and its inputs are recorded per window. | F11; C1 F11; B1 F9 | OL-03, OL-17 | Scheduler test: no trial's clock starts while C trials run; window record carries C inputs |
| REQ-XB-215 | Every provisioning window SHALL produce one `hh-compute-cost/1` record whose usage intervals derive from resource lifecycle timestamps, with estimate lines recomputable from the pinned rate table and utilization reported. | F3, F15; A1 REQ-XA-012 | OL-08, OL-17 | Validator recomputes `total_micros`; intervals match receipt timestamps |
| REQ-XB-216 | Compute reconciliation SHALL compare the estimate to a GCP billing read (operator, HUMAN), append-only. `pending` until read; `unavailable{reason}` when unreadable; `within` iff \|Δ\| ≤ max($0.50, 10 %); `outside` requires a written explanation, and totals then use billed. | F6, F15; ADR-0363 D4 | OL-08 | Fixture records for each status fold correctly |
| REQ-XB-217 | Model and compute spend SHALL share one phase spend ledger (ADR-0363 row shape plus `kind`/`authority`/`basis`/`phase`/`window`, admitted by ADR). Each cost line has exactly one authority; a billed row supersedes its estimate by reference; ceilings are per kind; the model credit never pays compute. | F15; Q-X11 | OL-08, OL-15 | Ledger validator: no window counted twice; per-kind cumulative ≤ ceiling |
| REQ-XB-218 | The amended `evaluate_costs` SHALL fold round-5 packets as `compute: unrecorded{pre_schema}` (never $0). For new-phase packets it SHALL refuse a missing teardown or non-empty post-state, a recompute mismatch, or compute over authorization; and report billing-pending as inconclusive for publication. | F15; `study.rs` 1466–1508 | OL-08, OL-15, OL-16 | `hh-env` unit tests per predicate; the R5.59 study refold is byte-stable |
| REQ-XB-219 | GCP credentials, SSH keys and billing-account ids SHALL never enter a committed record (`provided: yes/no` only). A model key needed on the VM SHALL be delivered only by `deliver-secret` (stdin → tmpfs 0400), never via metadata, argv or disk, and is destroyed with the VM. | F16; ADR-0363 D2/D5 | OL-15, OL-17 | Shim test asserts no secret in argv; leak sweep over harvested artifacts |
| REQ-XB-220 | The VM SHALL boot an exact image name, record dpkg versions, disable unattended upgrades for the window, and harvest runner logs, journald and serial output with the run root under a verified sha256 manifest before teardown deletes anything. | F16, F14 | OL-17 | Harvest manifest verify step; receipt fields present |
| REQ-XB-221 | Paid-phase compute quotes SHALL flow through `exposure_quote` with provisioned quantities (`cpu_core_hours = vCPU × D`, etc.) once an ADR admits TB 2.1 stage ceilings. The frozen round-5 §9.2 constants SHALL NOT be widened in place; until the ADR lands, the recipe's pre-quote is labelled as such. | F13; ADR-0420; Q-X32 | OL-16, OL-08 | ADR exists; quote packets cite it; a test shows the old ceilings unchanged |
| REQ-XB-222 | The phase-1 methodology record SHALL state round-5 compute as a rate-card estimate (≈$1.72 over 16:32Z–22:15:40Z at F5 rates; billing not read), recorded as a dated correction that keeps the original "$1.41" text. | F3 | OL-17, OL-15 | A2's record text; `check-position.sh` claims check if applicable |
| REQ-XB-223 | The recipe SHALL ship a deterministic test battery that runs it against a fake `gcloud`/`ssh` shim on PATH. The shim records argv and returns canned JSON. The battery covers every refusal above, receipt shape, the teardown ordering and `compute-open.sh`. It runs offline in CI with no credentials. | F2, F14 | OL-16 | `bash scripts/check-gce-backend.sh` passes in CI; mutation cases fail as expected |

## Handoff

- **Dependencies resolved.**
  - B1 supplied the census, packing rule, timeout sums and image facts.
  - C1 supplied the concurrency of 3 and the cost prior; C3 the pilot size and wall-time.
  - A1 F11 supplied the missing compute field.
  - R5.59 is the teardown precedent.
- **Interfaces offered to other rows:**
  - **B2:** runner-on-VM placement (REQ-XB-205); the C rule (REQ-XB-214), which composes with REQ-XB-010; the diskwatch (REQ-XB-207); harvest of the run root (REQ-XB-220). The slot record should carry `window` and `compute_cost` references, never per-slot dollars.
  - **C2:** the proxy-placement question (open question 1); `deliver-secret` as the only key path to the VM; the ledger `kind/authority` extension and the joint ADR; the model half of `evaluate_costs`.
  - **D1:** dead-man stops classify in-flight trials as `inconclusive{backend_terminated}`, an infra-missingness class; Spot is excluded to avoid time-correlated preemption.
  - **H1:** the ADRs (`hh-live-pricing/2` plus ledger rows; TB 2.1 quote ceilings, F13; recipe admission of `gcloud` as an operator tool); the GATE and HUMAN rows below; the round-5 correction (REQ-XB-222, with A2).
- **Implied GATE and HUMAN rows:**
  - `GATE-PROV-<phase>-<window>`: provisioning authorization, presenting the REQ-XB-213 quote. Operator words are recorded verbatim with the ceiling; an extension beyond D is a new gate.
  - `HUMAN-BILLING-ALERT`: once per billing account before the first paid window; budget amount and thresholds recorded, recipients `provided: yes`.
  - `HUMAN-RECONCILE-<window>`: at least 24 h after teardown; billing read recorded per REQ-XB-216.
  - Optional `HUMAN-KEY-ROTATE` after a phase if the key was on a VM (operator choice).
- **Ticket shaping hints** (each one fresh-context `implement-spec` run):
  1. *Recipe and receipts tooling* ($0, `fixture-verified`): `gce-backend.sh`, startup script, declaration schema, receipt writer with the `hh-secret-scan` sweep, `compute-open.sh`, fake-shim battery (REQ-XB-201–204, 206, 208–211, 219, 220, 223). Possibly split into a provision/verify half and a teardown/inventory half if over the size trigger.
  2. *Cost record, rate table, reconciliation and ledger ADR* ($0): `hh-compute-cost/1`, `hh-live-pricing/2` compute rows, ledger row extension, records validator script, the compute half of `evaluate_costs` (REQ-XB-213, 215–218). Joint with C2's model half, or sequenced after it.
  3. *Quote-ceiling ADR* (docs/ADR, $0; REQ-XB-221), with Q-X32's timeout ADR.
  4. *Per-window live tickets* (operator-gated, spend): `PROV-<window>` (provision, verify, prepull, runner build; the first one also does the $0 compose/podman smoke and B1a's measurement), then B2's run tickets, then `TEARDOWN-<window>` (harvest, teardown, cost record; mandatory AC), then `HUMAN-RECONCILE-<window>`. Each run ticket's stop conditions include "on any abort, the next row is TEARDOWN"; the guard ordering makes it unskippable.
- **Conflicts with other notes and the charter:**
  - The brief §2's "~$1.41 GCE instance-hours … billed" (F3).
  - R5.59's post-state evidence (F4).
  - The round-5 §9.2 quote ceilings vs TB 2.1 (F13).
  - A possible conflict with B2 or C2 if either assumes a laptop-side runner or proxy (F10).
- **Capability and portfolio dispositions.** None changed. Nothing is claimed `live-executed`. The recipe would be `fixture-verified` (shim) until a first receipted window exists.
