#!/usr/bin/env bash
# check-gce-backend.sh — the deterministic fake-shim battery for the
# compute recipe (manifest 266/R6.13; spec §3.9/§3.11; REQ-XB-223).
# A fake `gcloud`/`ssh` shim goes on PATH: argv is recorded to a log,
# canned JSON comes from a state dir, and live resources mutate the way
# GCE's would. Covers every refusal, receipt shape, teardown ordering and
# compute-open.sh — fully offline, no credential, no GCP call. Mutation
# cases fail as expected; a pass here is fixture-verified, never live.
set -euo pipefail
cd "$(dirname "$0")/.."
REPO="$(pwd)"

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
BIN="$T/bin"; STATE="$T/state"; LIVE="$STATE/live"; REPORTS="$T/reports"
DECLS="$T/decls"; ENV="$T/operator.env"
mkdir -p "$BIN" "$STATE/ssh" "$STATE/scp_src" "$LIVE" "$REPORTS" "$DECLS" \
         "$STATE/fail" "$STATE/warn" "$STATE/ssh_rc"
export HH_PHASE_SPEND="$T/PHASE_SPEND.jsonl"

# the pinned hh-live-pricing/2 rate table fixture (R6.14 priced quotes —
# B3 default rows summing to r = 314140 µ$/h = $0.31414/h)
RATES="$T/pricing.json"
cat > "$RATES" <<'EOF'
{"schema":"hh-live-pricing/2","compute":{"currency":"USD","rows":[
 {"sku":"e2-standard-8","unit":"hour","price_micros":268040,
  "source":"https://cloud.google.com/compute/vm-instance-pricing","retrieved":"2026-11-07"},
 {"sku":"pd-balanced","unit":"gib_hour","price_micros":137,
  "source":"https://cloud.google.com/compute/disks-image-pricing","retrieved":"2026-11-07"},
 {"sku":"ipv4","unit":"hour","price_micros":5000,
  "source":"https://cloud.google.com/vpc/network-pricing","retrieved":"2026-11-07"},
 {"sku":"egress","unit":"gib","price_micros":120000,
  "source":"https://cloud.google.com/vpc/network-pricing","retrieved":"2026-11-07"},
 {"sku":"nat_gateway","unit":"hour","price_micros":44000,
  "source":"https://cloud.google.com/nat/pricing","retrieved":"2026-11-07"}]}}
EOF
LOG="$T/argv.log"; : > "$LOG"
export HH_SHIM_LOG="$LOG" HH_SHIM_STATE="$STATE" HH_SHIM_LIVE="$LIVE"
export HH_COMPUTE_REPORTS_DIR="$REPORTS"
export HH_OPERATOR_ENV_FILE="$ENV"
export HH_BILLING_ACCOUNT="fixture-billing-0000"
printf 'OLLAMA_API_KEY=fix-ollama-fake-secret-9f8e7d6c\nOTHER=nothing\n' > "$ENV"

# ── fake gcloud ─────────────────────────────────────────────────────────
cat > "$BIN/gcloud" <<'SHIM'
#!/usr/bin/env bash
set -u
printf 'gcloud' >> "$HH_SHIM_LOG"
for a in "$@"; do printf ' [%s]' "$a" >> "$HH_SHIM_LOG"; done
printf '\n' >> "$HH_SHIM_LOG"
S="$HH_SHIM_STATE"; L="$HH_SHIM_LIVE"
norm() { printf '%s' "$*" | tr -cs 'A-Za-z0-9' '_' | sed 's/_$//'; }
key="$(norm "$@")"
for f in "$S"/fail/*; do
  [ -e "$f" ] || continue
  case "$key" in *"$(basename "$f")"*) exit "$(cat "$f")" ;; esac
done
for f in "$S"/warn/*; do
  [ -e "$f" ] || continue
  case "$key" in *"$(basename "$f")"*) printf 'WARNING: injected warning\n' >&2 ;; esac
done
# ssh: capture stdin only when armed (deliver-secret streams the key over
# stdin) and serve the canned remote output for the --command value.
case "$*" in
  *"compute ssh "*)
    [ -f "$S/want_stdin" ] && cat > "$S/stdin.capture" || true
    rcmd=""; prev=""
    for a in "$@"; do
      [ "$prev" = "--command" ] && rcmd="$a"; prev="$a"
    done
    nk="$(norm "$rcmd")"
    [ -f "$S/ssh_rc/$nk" ] && exit "$(cat "$S/ssh_rc/$nk")"
    [ -f "$S/ssh/$nk" ] && cat "$S/ssh/$nk"
    exit 0 ;;
  *"compute scp "*)
    dest=""; src=""
    for a in "$@"; do dest="$a"; case "$a" in *:/*) src="${a##*:}";; esac; done
    cp "$S/scp_src/$(basename "$src")" "$dest" 2>/dev/null || exit 1
    exit 0 ;;
esac
case "$*" in
  *"auth list"*) printf 'fake-operator@example.com\n' ;;
  *"billing projects link"*) : ;;
  *"services enable"*) : ;;
  *"projects create"*)
    printf '{"lifecycleState":"ACTIVE"}\n' > "$L/projects_describe.json" ;;
  *"projects delete"*)
    printf '{"lifecycleState":"DELETE_REQUESTED"}\n' > "$L/projects_describe.json" ;;
  *"projects describe"*)
    if [ -f "$L/projects_describe.json" ]; then
      cat "$L/projects_describe.json"
    else echo "not found" >&2; exit 1; fi ;;
  *"firewall-rules describe"*)
    if [ -f "$L/fw_describe.json" ]; then cat "$L/fw_describe.json"
    else echo "not found" >&2; exit 1; fi ;;
  *"firewall-rules create"*) printf '{}\n' > "$L/fw_describe.json" ;;
  *"firewall-rules list"*)
    [ -f "$L/fw_list.json" ] && cat "$L/fw_list.json" || printf '[]\n' ;;
  *"networks describe"*)
    if [ -f "$L/net_describe.json" ]; then cat "$L/net_describe.json"
    else echo "not found" >&2; exit 1; fi ;;
  *"networks create"*) printf '{}\n' > "$L/net_describe.json" ;;
  *"routers nats list"*)
    [ -f "$L/nats_list.json" ] && cat "$L/nats_list.json" || printf '[]\n' ;;
  *"routers describe"*)
    if [ -f "$L/router_describe.json" ]; then cat "$L/router_describe.json"
    else echo "not found" >&2; exit 1; fi ;;
  *"routers nats create"*) : ;;
  *"routers create"*) printf '{}\n' > "$L/router_describe.json" ;;
  *"routers list"*)
    [ -f "$L/routers_list.json" ] && cat "$L/routers_list.json" || printf '[]\n' ;;
  *"instances describe"*)
    if [ -f "$L/instance_describe.json" ]; then
      cat "$L/instance_describe.json"
    else echo "not found" >&2; exit 1; fi ;;
  *"instances list"*)
    [ -f "$L/instances_list.json" ] && cat "$L/instances_list.json" || printf '[]\n' ;;
  *"instances create"*)
    cp "$S/new_instance_describe.json" "$L/instance_describe.json"
    printf '[{"name":"hh-p1-w1x","status":"RUNNING"}]\n' > "$L/instances_list.json"
    printf '[{"name":"hh-p1-w1x","type":"pd-balanced","sizeGb":"300"}]\n' \
      > "$L/disks_list.json" ;;
  *"instances start"*)
    [ -f "$L/start_describe.json" ] && \
      cp "$L/start_describe.json" "$L/instance_describe.json" ;;
  *"instances delete"*)
    if [ -f "$L/after_delete_instances.json" ]; then
      cp "$L/after_delete_instances.json" "$L/instances_list.json"
    else printf '[]\n' > "$L/instances_list.json"; fi
    if [ -f "$L/after_delete_disks.json" ]; then
      cp "$L/after_delete_disks.json" "$L/disks_list.json"
    else printf '[]\n' > "$L/disks_list.json"; fi
    if [ -f "$L/after_delete_describe.json" ]; then
      cp "$L/after_delete_describe.json" "$L/instance_describe.json"
    else rm -f "$L/instance_describe.json"; fi ;;
  *"disks list"*)
    [ -f "$L/disks_list.json" ] && cat "$L/disks_list.json" || printf '[]\n' ;;
  *"addresses list"*)   cat "$L/addresses_list.json"   2>/dev/null || printf '[]\n' ;;
  *"snapshots list"*)   cat "$L/snapshots_list.json"   2>/dev/null || printf '[]\n' ;;
  *"images list"*)      cat "$L/images_list.json"      2>/dev/null || printf '[]\n' ;;
  *"forwarding-rules list"*) cat "$L/forwarding_list.json" 2>/dev/null || printf '[]\n' ;;
  *"get-serial-port-output"*) cat "$S/serial.txt" 2>/dev/null || : ;;
  *) echo "fake-gcloud: unhandled call: $*" >&2; exit 9 ;;
esac
exit 0
SHIM
cat > "$BIN/ssh" <<'SHIM'
#!/usr/bin/env bash
printf 'ssh' >> "$HH_SHIM_LOG"
for a in "$@"; do printf ' [%s]' "$a" >> "$HH_SHIM_LOG"; done
printf '\n' >> "$HH_SHIM_LOG"
exit 0
SHIM
chmod +x "$BIN/gcloud" "$BIN/ssh"
export PATH="$BIN:$PATH"

# ── declarations ────────────────────────────────────────────────────────
# $1 out-file, $2 python mutation over `d` ("" = none)
mkdecl() {
  python3 -I - "$1" "$2" "$RATES" <<'PY'
import json, sys
d = {
 "schema":"hh-compute-decl/1","mode":"full","phase":"p1","window":"w1x",
 "ticket":"R6.13","project_id":"hh-p1-a1b2c3","region":"us-central1",
 "zone":"us-central1-a","machine_type":"e2-standard-8",
 "provisioning_model":"STANDARD",
 "boot_disk":{"type":"pd-balanced","size_gib":300},
 "image":{"project":"debian-cloud","name":"debian-12-bookworm-v20261014"},
 "packages":{"podman":"4.3.1+ds1-8+deb12u1","crun":"1.8.1-1+deb12u1",
   "conmon":"2.1.6+ds1-1+deb12u1","netavark":"1.4.0-4",
   "compose_frontend":{"name":"podman-compose","version":"1.0.6-2"}},
 "network":{"external_ipv4":"ephemeral","nat":False,
   "ingress":[{"proto":"tcp","port":22,"source":"35.235.240.0/20"}]},
 "service_account":"none",
 "labels":{"hh-phase":"p1","hh-window":"w1x","hh-ticket":"r6-13","ttl":"window"},
 "dead_man":{"max_run_duration_hours":12,"action":"STOP",
   "in_guest_timer_minutes":735},
 "restart":{"preauthorized_hours":1},"h_r_hours":6,
 "runner":{"commit":"a"*40,"build":"source"},"proxy_on_vm":True,
 "model_line":"authorized",
 "images":[{"task":"t1","ref":"reg.invalid/hh/cell","digest":"sha256:"+"1"*64}],
 "disk_floor_gib":20,
 "concurrency":{"memory_gib_available":26,"memory_per_trial_gib":6,
   "vcpu_budget":8,"model_in_flight_limit":4,"model_wait_fraction":0.5},
 "rate_table":{"path":sys.argv[3],"sha256":"0"*64},
 "authorization":{"gate":"PROV-W1","readout_path":"build/readouts/PROV-W1.md",
   "verbatim_sha256":"2"*64,"quote_id":"TO_FILL","ceiling_micros":0,
   "model_line":"authorized"},
 "quote_inputs":{"trials":20,"surfaces":1,"per_trial_planning_seconds":600,
   "per_trial_bound_seconds":1200,"overhead_hours":1.5,"egress_gib_estimate":6,
   "retry_trials":2,"load_hours":0.25,"admission_cutoff_hours":0.65,
   "export_size_gib":4.0,
   "work_items":["observe_pass","rust_build","admission_build",
     "layer_export","census","smoke_battery"]},
 "phase_close":False,"last_model_call_utc":"2026-11-07T14:23:00Z",
}
if sys.argv[2]:
    exec(sys.argv[2])
json.dump(d, open(sys.argv[1], "w"))
PY
}
set_qid() { # fill rate_table sha + quote_id + ceiling from the priced quote
  python3 -I - "$1" <<'PY'
import hashlib, json, os, sys
sys.path.insert(0, "scripts/gce-backend")
import importlib.util
spec = importlib.util.spec_from_file_location(
    "receipt_lib", "scripts/gce-backend/receipt_lib.py")
lib = importlib.util.module_from_spec(spec); spec.loader.exec_module(lib)
d = json.load(open(sys.argv[1]))
d["rate_table"]["sha256"] = hashlib.sha256(
    open(d["rate_table"]["path"], "rb").read()).hexdigest()
q = lib.quote_doc(d, os.getcwd())
d["authorization"]["quote_id"] = "pq-" + lib.sha256_text(lib.canon(q))[:16]
d["authorization"]["ceiling_micros"] = q["pricing"]["ceiling_micros"]
json.dump(d, open(sys.argv[1], "w"))
PY
}
mkdecl "$DECLS/base.json" ""
set_qid "$DECLS/base.json"
mkauth() { python3 -I - "$1" "$2" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
json.dump(dict(d["authorization"]), open(sys.argv[2], "w"))
PY
}
mkauth "$DECLS/base.json" "$T/auth.json"
mkdecl "$DECLS/restart.json" 'd["mode"]="restart-only"; d["model_line"]="none"; d["authorization"]["model_line"]="none"; d["restart"]={"max_hours":1}; d["images"]=[]'
set_qid "$DECLS/restart.json"
mkdecl "$DECLS/none.json" 'd["model_line"]="none"; d["authorization"]["model_line"]="none"'
set_qid "$DECLS/none.json"
mkdecl "$DECLS/residual.json" 'd["residual"]={"admit_stopped_disk":{"df_row":"DF-R6.13-1","owner":"operator","deadline":"2026-11-08"}}'
set_qid "$DECLS/residual.json"
mkdecl "$DECLS/late.json" 'd["h_r_hours"]=0.001'
set_qid "$DECLS/late.json"
mkdecl "$DECLS/close.json" 'd["phase_close"]=True'
set_qid "$DECLS/close.json"

# ── canned state ────────────────────────────────────────────────────────
cat > "$STATE/new_instance_describe.json" <<'EOF'
{"name":"hh-p1-w1x","id":"1234567890123456","status":"RUNNING",
 "creationTimestamp":"2026-11-07T10:00:00.000Z",
 "lastStopTimestamp":"2026-11-07T20:00:00.000Z",
 "scheduling":{"maxRunDuration":{"seconds":"43200"}},
 "serviceAccounts":[],
 "networkInterfaces":[{"accessConfigs":[{"natIP":"34.0.0.1"}]}]}
EOF
printf 'serial-line-1 serial-line-2\n' > "$STATE/serial.txt"
printf '{"lifecycleState":"ACTIVE"}\n' > "$LIVE/projects_describe.json"
printf '[{"name":"hh-iap-ssh","allowed":[{"IPProtocol":"tcp","ports":["22"]}],"sourceRanges":["35.235.240.0/20"],"network":"hh-net"}]\n' > "$LIVE/fw_list.json"
cp "$STATE/new_instance_describe.json" "$LIVE/instance_describe.json"
printf '[{"name":"hh-p1-w1x","status":"RUNNING"}]\n' > "$LIVE/instances_list.json"

snorm() { printf '%s' "$1" | tr -cs 'A-Za-z0-9' '_' | sed 's/_$//'; }
sshf() { printf '%s' "$2" > "$STATE/ssh/$(snorm "$1")"; }
sshf 'curl -fs --unix-socket /run/podman/podman.sock http://d/_ping' 'OK'
sshf 'podman version --format json' '{"Client":{"Version":"4.3.1+ds1-8+deb12u1"}}'
sshf 'podman compose version --format json 2>/dev/null || podman-compose --version' 'podman-compose version 1.0.6'
sshf 'uname -r' '6.1.0-26-amd64'
sshf 'dpkg-query -W podman crun conmon netavark podman-compose' \
     $'podman\t4.3.1+ds1-8+deb12u1\ncrun\t1.8.1-1+deb12u1\nconmon\t2.1.6+ds1-1+deb12u1\nnetavark\t1.4.0-4\npodman-compose\t1.0.6-2'
sshf 'nproc' '8'
sshf "free -b | awk 'NR==2{print \$2}'" '34359738368'
sshf 'df -B1 --output=avail / | tail -1' '240000000000'
sshf 'df -B1 --output=avail /var | tail -1' '240000000000'
sshf 'stat -c %a:%U:%G /run/podman/podman.sock' '660:root:hh'
sshf 'sh -c "ss -tln 2>/dev/null | grep -c podman || true"' '0'
sshf "hh-runner drain --window 'w1x' --inconclusive teardown" ''
sshf 'journalctl --no-pager -o short-iso' 'journal-line'
sshf 'cat /proc/uptime; df -BG --output=avail / | tail -1' 'uptime'
DIG="sha256:$(printf '1%.0s' $(seq 64))"
sshf "podman pull 'reg.invalid/hh/cell@$DIG'" 'pulled'
sshf "podman image inspect '$DIG' --format json" '[{"Size":123456,"Architecture":"amd64"}]'
# harvest fixture: a real run root + matching manifest + tar
mkdir -p "$T/runroot"
printf 'slot-1 receipt\n' > "$T/runroot/slot-1.txt"
printf 'runner log\n'      > "$T/runroot/runner.log"
( cd "$T/runroot" && { sha256sum slot-1.txt runner.log 2>/dev/null || shasum -a 256 slot-1.txt runner.log; } > "$STATE/scp_src/hh-manifest.sha256" )
( cd "$T/runroot" && tar czf "$STATE/scp_src/hh-harvest.tar.gz" . )
HREMOTE="mkdir -p '/var/lib/hh/runs/run-root-w1x' && cd '/var/lib/hh/runs/run-root-w1x' && find . -type f -print0 | sort -z | xargs -0 --no-run-if-empty sha256sum > /tmp/hh-manifest.sha256 && tar czf /tmp/hh-harvest.tar.gz . && sha256sum /tmp/hh-manifest.sha256 /tmp/hh-harvest.tar.gz"
sshf "$HREMOTE" "$(cd "$STATE/scp_src" && { sha256sum hh-manifest.sha256 hh-harvest.tar.gz 2>/dev/null || shasum -a 256 hh-manifest.sha256 hh-harvest.tar.gz; })"

SCAN="$REPO/target/debug/hh-secret-scan"
[ -x "$SCAN" ] || (cd "$REPO" && cargo build -q -p hharness-secrets --bins)
export HH_SECRET_SCAN="$SCAN"

# ── test harness ────────────────────────────────────────────────────────
PASS=0; FAILED=0
ok()   { PASS=$((PASS + 1)); printf 'ok %d — %s\n' "$PASS" "$1"; }
bad()  { FAILED=$((FAILED + 1)); printf 'NOT OK — %s\n' "$1" >&2; }
expect_rc() { # $1 want-rc $2 label; rest cmd
  local want="$1" label="$2"; shift 2
  local out rc
  out="$("$@" 2>&1)" && rc=0 || rc=$?
  if [ "$rc" -eq "$want" ]; then ok "$label"; else
    bad "$label — rc=$rc want $want :: ${out:0:160}"
  fi
}
has()  { grep -qFe "$1" "$LOG" && ok "$2" || bad "$2 — missing in argv log: $1"; }
lacks(){ grep -qFe "$1" "$LOG" && bad "$2 — forbidden in argv log: $1" || ok "$2"; }
lnof() { grep -nFe "$1" "$LOG" | head -1 | cut -d: -f1; }
G() { bash scripts/gce-backend.sh "$@"; }

# ── 1 · quote — the priced §3.10 quote in integer µ$, no network (AC-9) ─
: > "$LOG"
QOUT="$(G quote "$DECLS/base.json")"
printf '%s' "$QOUT" | grep -q '"label": "quote"' \
  && ok "quote labelled quote" || bad "quote label"
MISSING=0
for q in machine_type disk_gib external_ipv4 nat trials surfaces \
         per_trial_planning_seconds per_trial_bound_seconds concurrency \
         overhead_hours egress_gib dead_man_hours residual_disk_hours \
         restart_hours H_rs h_r E_gib; do
  printf '%s' "$QOUT" | grep -q "\"$q\"" || { bad "quote missing $q"; MISSING=1; }
done
[ "$MISSING" -eq 0 ] && ok "quote carries every §3.10 input quantity"
printf '%s' "$QOUT" | grep -q '"label": "estimated"' \
  && ok "egress labelled estimated" || bad "egress estimate label"
# REQ-XB-213 — planning/bound/ceiling in integer µ$, every ceiling term
printf '%s' "$QOUT" | grep -q '"hourly_micros": 314140' \
  && ok "golden: r = 314140 µ$/h (B3 default shape)" || bad "hourly µ$"
for t in '"ceiling_micros"' '"planning_micros"' '"bound_micros"' \
         '"restart_hours"' '"residual_disk_gib_hours"' '"egress_micros"'; do
  printf '%s' "$QOUT" | grep -q "$t" || { bad "pricing missing $t"; MISSING=1; }
done
[ "$MISSING" -eq 0 ] && ok "quote prices every ceiling term"
[ -s "$LOG" ] && bad "quote touched gcloud" || ok "quote made no network call"

# ── 2 · missing gcloud exits 7 (AC-1) ────────────────────────────────────
expect_rc 7 "missing gcloud exits 7" env PATH="/usr/bin:/bin" \
  HH_GCLOUD_BIN="$T/no-such-gcloud" \
  bash "$REPO/scripts/gce-backend.sh" provision "$DECLS/base.json" \
  --authorization "$T/auth.json"

# ── 3 · declaration validator refusals (AC-2) ───────────────────────────
for m in \
  'del d["dead_man"]' \
  'd["bogus_field"] = 1' \
  'd["image"] = {"project":"debian-cloud","family":"debian-12"}' \
  'd["packages"]["podman"] = ">=4.0"' \
  'del d["dead_man"]["max_run_duration_hours"]' \
  'd["project_id"] = "hh-p2-a1b2c3"' \
  'd["network"]["ingress"] = [{"proto":"tcp","port":22,"source":"0.0.0.0/0"}]' \
  'd["service_account"] = "sa@x"' \
  'd["provisioning_model"] = "SPOT"' \
  'd["mode"]="restart-only"; d["model_line"]="none"; d["authorization"]["model_line"]="none"; d["restart"]={"max_hours":2}; d["images"]=[]' \
  'd["images"] = [{"task":"t","ref":"r"}]' \
  'del d["rate_table"]["sha256"]'; do
  mkdecl "$T/mut.json" "$m"
  if python3 -I scripts/gce-backend/validate_decl.py "$T/mut.json" 2>"$T/e"; then
    bad "declaration mutation accepted: $m"
  else
    grep -q 'Refused{decl\.' "$T/e" && ok "decl refuses: $m" \
      || bad "decl mutation wrong refusal: $m :: $(cat "$T/e")"
  fi
done

# ── 4 · provision happy path (AC-3/4/7) ─────────────────────────────────
: > "$LOG"; rm -rf "$REPORTS"/*
rm -f "$LIVE/instance_describe.json"
G provision "$DECLS/base.json" --authorization "$T/auth.json" >/dev/null \
  || bad "provision failed"
has '--no-service-account' 'argv: no service account'
has '--no-scopes' 'argv: no scopes'
has '--shielded-secure-boot' 'argv: shielded secure boot'
has '--max-run-duration=43200s' 'argv: max-run-duration = D'
has '--instance-termination-action' 'argv: termination action'
has 'startup-script' 'argv: startup script metadata'
has 'tcp:22' 'argv: iap firewall allow'
has '35.235.240.0/20' 'argv: iap source range'
PR="$REPORTS/$(date -u +%F)-p1-w1x-provision.json"
[ -f "$PR" ] && ok "provision receipt written" || bad "no provision receipt"
python3 -I scripts/gce-backend/validate_receipt.py "$PR" \
  && ok "provision receipt validates" || bad "provision receipt invalid"
grep -q '"firewall_rules"' "$PR" \
  && ok "provision receipt lists the rule set" || bad "receipt lacks rule set"

# authorization mismatch (AC-7)
python3 -c "
import json
a = json.load(open('$T/auth.json')); a['quote_id'] = 'pq-deadbeefdeadbeef'
json.dump(a, open('$T/auth.json', 'w'))"
expect_rc 2 "wrong quote_id refuses" G provision "$DECLS/base.json" --authorization "$T/auth.json"
mkauth "$DECLS/base.json" "$T/auth.json"
python3 -c "
import json
a = json.load(open('$T/auth.json')); a['ceiling_micros'] = 1
json.dump(a, open('$T/auth.json', 'w'))"
expect_rc 2 "wrong ceiling refuses" G provision "$DECLS/base.json" --authorization "$T/auth.json"
mkauth "$DECLS/base.json" "$T/auth.json"
expect_rc 2 "absent authorization refuses" G provision "$DECLS/base.json"

# ── 5 · verify happy + field refusals (AC-5) ─────────────────────────────
: > "$LOG"
G verify "$DECLS/base.json" >/dev/null || bad "verify failed"
VR="$REPORTS/$(date -u +%F)-p1-w1x-verify.json"
[ -f "$VR" ] && ok "verify receipt written" || bad "no verify receipt"
python3 -I scripts/gce-backend/validate_receipt.py "$VR" \
  && ok "verify receipt validates" || bad "verify receipt invalid"
grep -q '"socket_mode": "local_rootful_group_restricted"' "$VR" \
  && ok "socket_mode recorded" || bad "socket_mode missing"
grep -q '"max_run_duration": 12' "$VR" \
  && ok "receipt max_run_duration == D" || bad "max_run_duration mismatch"
# per-field mutations → Unsupported{backend.<field>} (exit 4)
vfield() { sshf "$1" "$2"; }
vfield 'podman version --format json' '{"Client":{"Version":"9.9.9"}}'
expect_rc 4 "podman version mismatch" G verify "$DECLS/base.json"
vfield 'podman version --format json' '{"Client":{"Version":"4.3.1+ds1-8+deb12u1"}}'
vfield 'podman compose version --format json 2>/dev/null || podman-compose --version' ''
expect_rc 4 "missing compose frontend" G verify "$DECLS/base.json"
vfield 'podman compose version --format json 2>/dev/null || podman-compose --version' 'podman-compose version 1.0.6'
vfield 'stat -c %a:%U:%G /run/podman/podman.sock' '666:root:root'
expect_rc 4 "socket mode refusal" G verify "$DECLS/base.json"
vfield 'stat -c %a:%U:%G /run/podman/podman.sock' '660:root:hh'
# extra 0.0.0.0/0 ingress rule → refuse (AC-3)
printf '[{"name":"x","allowed":[{"IPProtocol":"tcp","ports":["22"]}],"sourceRanges":["35.235.240.0/20"]},{"name":"bad","allowed":[{"IPProtocol":"tcp","ports":["22"]}],"sourceRanges":["0.0.0.0/0"]}]\n' > "$LIVE/fw_list.json"
expect_rc 4 "extra ingress rule refuses" G verify "$DECLS/base.json"
printf '[{"name":"hh-iap-ssh","allowed":[{"IPProtocol":"tcp","ports":["22"]}],"sourceRanges":["35.235.240.0/20"],"network":"hh-net"}]\n' > "$LIVE/fw_list.json"
# max-run-duration mismatch
python3 -c "
import json
d = json.load(open('$LIVE/instance_describe.json'))
d['scheduling']['maxRunDuration']['seconds'] = '9999'
json.dump(d, open('$LIVE/instance_describe.json', 'w'))"
expect_rc 4 "max_run_duration mismatch" G verify "$DECLS/base.json"
cp "$STATE/new_instance_describe.json" "$LIVE/instance_describe.json"

# ── 6 · prepull (AC-11) ──────────────────────────────────────────────────
: > "$LOG"
G prepull "$DECLS/base.json" >/dev/null || bad "prepull failed"
PPR="$REPORTS/$(date -u +%F)-p1-w1x-prepull.json"
python3 -I scripts/gce-backend/validate_receipt.py "$PPR" \
  && ok "prepull receipt validates" || bad "prepull receipt invalid"
grep -q '"size_bytes": 123456' "$PPR" && grep -q '"arch": "amd64"' "$PPR" \
  && ok "prepull records digest/size/arch" || bad "prepull receipt fields"
expect_rc 2 "restart-only prepull refuses" G prepull "$DECLS/restart.json"

# ── 7 · deliver-secret (AC-15) ───────────────────────────────────────────
: > "$LOG"; rm -f "$STATE/stdin.capture"; touch "$STATE/want_stdin"
G deliver-secret "$DECLS/base.json" OLLAMA_API_KEY >/dev/null || bad "deliver-secret failed"
rm -f "$STATE/want_stdin"
grep -q 'fix-ollama-fake-secret-9f8e7d6c' "$STATE/stdin.capture" \
  && ok "secret streamed over ssh stdin" || bad "secret not on stdin"
lacks 'fix-ollama-fake-secret-9f8e7d6c' "secret never in argv"
expect_rc 2 "deliver-secret in \$0 window refuses" G deliver-secret "$DECLS/none.json" OLLAMA_API_KEY
expect_rc 2 "deliver-secret restart-only refuses" G deliver-secret "$DECLS/restart.json" OLLAMA_API_KEY
expect_rc 2 "deliver-secret wrong name refuses" G deliver-secret "$DECLS/base.json" HF_TOKEN

# ── 8 · provision on stopped VM refuses (AC-7) ──────────────────────────
python3 -c "
import json
d = json.load(open('$LIVE/instance_describe.json'))
d['status'] = 'STOPPED'
json.dump(d, open('$LIVE/instance_describe.json', 'w'))"
expect_rc 2 "stopped VM full-mode restart refuses" G provision "$DECLS/base.json" --authorization "$T/auth.json"
cp "$STATE/new_instance_describe.json" "$LIVE/instance_describe.json"

# ── 9 · restart-only mode (AC-16) ───────────────────────────────────────
: > "$LOG"
python3 -c "
import json
d = json.load(open('$LIVE/instance_describe.json'))
d['status'] = 'STOPPED'
json.dump(d, open('$LIVE/instance_describe.json', 'w'))"
G provision "$DECLS/restart.json" >/dev/null || bad "restart-only provision failed"
has 'instances] [start' 'restart-only runs instances start'
lacks 'instances] [create' 'restart-only never creates'
cp "$STATE/new_instance_describe.json" "$LIVE/instance_describe.json"

# ── 10 · harvest + teardown nine-step ordering (AC-12/14) ───────────────
prep_live() { # reset live state; callers then plant after_* mutations
  rm -rf "$REPORTS"/* "$LIVE"/after_* "$HH_PHASE_SPEND"
  printf '[{"name":"hh-p1-w1x","status":"RUNNING"}]\n' > "$LIVE/instances_list.json"
  cp "$STATE/new_instance_describe.json" "$LIVE/instance_describe.json"
  # the window's provision receipt — the cost record reads
  # resources.creation_timestamp from it (a deleted VM describes {})
  python3 - "$REPORTS" <<'PY'
import json, sys
r = {"schema": "hh-compute-receipt/1", "kind": "provision",
     "phase": "p1", "window": "w1x", "ticket": "R6.13",
     "resources": {"creation_timestamp": "2026-11-07T10:00:00.000Z"}}
json.dump(r, open(sys.argv[1] + "/2026-11-07-p1-w1x-provision.json", "w"))
PY
}
teardown_case() { prep_live; G teardown "$1"; }
: > "$LOG"
teardown_case "$DECLS/base.json" >/dev/null || bad "teardown failed"
TR="$REPORTS/$(date -u +%F)-p1-w1x-teardown.json"
python3 -I scripts/gce-backend/validate_receipt.py "$TR" \
  && ok "teardown receipt validates" || bad "teardown receipt invalid"
L1="$(lnof 'instances] [list')"
L2="$(lnof 'instances] [delete')"
L3="$(grep -nFe 'instances] [list' "$LOG" | tail -1 | cut -d: -f1)"
[ -n "$L1" ] && [ -n "$L2" ] && [ -n "$L3" ] && \
  [ "$L1" -lt "$L2" ] && [ "$L2" -lt "$L3" ] \
  && ok "teardown ordering: inventory → delete → re-inventory" \
  || bad "teardown ordering ($L1/$L2/$L3)"
# R6.14 — the step-9 hook is wired: one hh-compute-cost/1 per window,
# billed: pending, intervals from the receipt's lifecycle stamps, and the
# compute estimate row lands in the phase spend ledger.
CR="$REPORTS/$(date -u +%F)-p1-w1x-cost.json"
[ -f "$CR" ] && ok "hh-compute-cost/1 written at teardown" \
  || bad "no hh-compute-cost/1"
grep -q '"cost_record"' "$TR" && grep -q '"sha256"' "$TR" \
  && ok "receipt carries the cost-record reference" || bad "cost_record member"
grep -q '"status": "pending"' "$CR" \
  && ok "cost record billed: pending" || bad "billed status"
python3 - "$CR" "$TR" <<'PY' && ok "cost-record intervals == receipt lifecycle" \
  || bad "interval mismatch"
import json, sys
c = json.load(open(sys.argv[1])); t = json.load(open(sys.argv[2]))
iv = c["usage"]["instance"]["intervals"][0]
lc = t["lifecycle"]
assert iv["from"] == "2026-11-07T10:00:00.000Z", iv
assert iv["to"] == (lc.get("last_stop_timestamp")
                    or lc["deletion_operation_end"]), (iv, lc)
PY
python3 -I scripts/gce-backend/cost_lib.py costvalidate "$CR" "$TR" "$REPO" \
  && ok "cost validator recomputes total_micros" || bad "cost validator"
# a second record for the same window refuses (append-only)
PRV="$(ls "$REPORTS"/*-provision.json | head -1)"
expect_rc 2 "cost-record rewrite refuses" \
  python3 -I scripts/gce-backend/cost_lib.py costrecord \
  "$DECLS/base.json" "$TR" "$PRV" "$REPORTS" "$REPO"
# the estimate spend row is in the phase spend ledger
python3 - "$HH_PHASE_SPEND" <<'PY' && ok "compute estimate row in ledger" \
  || bad "ledger estimate row"
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1])]
r = rows[-1]
assert r["kind"] == "compute" and r["authority"] == "gcp_billing" \
    and r["basis"] == "estimate" and r["window"] == "w1x", r
PY
python3 -I scripts/gce-backend/cost_lib.py spend-validate "$HH_PHASE_SPEND" \
  && ok "phase spend ledger validates" || bad "ledger validator"
grep -q '"last_model_call_utc_hour": "2026-11-07T14:00Z"' "$TR" \
  && ok "settlement carries the UTC hour boundary" || bad "settlement clock"
grep -q '"exceeded": false' "$TR" \
  && ok "h_r inside bound" || bad "h_r flag"
grep -q '"instances": 0' "$TR" \
  && ok "post_state complete (instances 0)" || bad "post_state instances"
# teardown without harvest refuses before any delete
: > "$LOG"
printf '1' > "$STATE/ssh_rc/$(snorm "$HREMOTE")"
expect_rc 2 "teardown refuses when harvest fails" teardown_case "$DECLS/base.json"
lacks 'instances] [delete' "delete never runs without harvest"
rm -f "$STATE/ssh_rc/$(snorm "$HREMOTE")"
# leftover disk refuses
: > "$LOG"; prep_live
printf '[{"name":"leftover","type":"pd-balanced"}]\n' > "$LIVE/after_delete_disks.json"
expect_rc 2 "leftover disk refuses" G teardown "$DECLS/base.json"
rm -f "$LIVE/after_delete_disks.json"
# WARNING list → inconclusive, receipt written, exit 6, never empty
: > "$LOG"; prep_live
touch "$STATE/warn/addresses_list"
expect_rc 6 "WARNING list is inconclusive" G teardown "$DECLS/base.json"
TR="$REPORTS/$(date -u +%F)-p1-w1x-teardown.json"
grep -q 'inconclusive{addresses_list:warning}' "$TR" \
  && ok "post_state marks inconclusive class" || bad "warning not inconclusive"
rm -f "$STATE/warn/addresses_list"
# running VM post-delete is never admitted
: > "$LOG"; prep_live
printf '[{"name":"hh-p1-w1x","status":"RUNNING"}]\n' > "$LIVE/after_delete_instances.json"
cp "$STATE/new_instance_describe.json" "$LIVE/after_delete_describe.json"
expect_rc 2 "running VM never admitted" G teardown "$DECLS/base.json"
rm -f "$LIVE"/after_*
# admitted deviation: stopped instance + retained disk + dated DF row
: > "$LOG"; prep_live
printf '[{"name":"hh-p1-w1x","status":"STOPPED"}]\n' > "$LIVE/after_delete_instances.json"
printf '[{"name":"hh-p1-w1x","type":"pd-balanced","sizeGb":"300"}]\n' > "$LIVE/after_delete_disks.json"
python3 -c "
import json
d = json.load(open('$STATE/new_instance_describe.json'))
d['status'] = 'STOPPED'
d['lastStopTimestamp'] = '$(date -u +%FT%TZ)'
json.dump(d, open('$LIVE/after_delete_describe.json', 'w'))"
G teardown "$DECLS/residual.json" >/dev/null || bad "stopped-disk deviation teardown failed"
TR="$REPORTS/$(date -u +%F)-p1-w1x-teardown.json"
grep -q 'stopped_disk_retained' "$TR" \
  && ok "stopped-disk deviation admitted with DF row" || bad "deviation not recorded"
rm -f "$LIVE"/after_*
# h_r exceeded → receipt flags the dated DF-row requirement (AC-16)
: > "$LOG"; prep_live
python3 -c "
import json
d = json.load(open('$STATE/new_instance_describe.json'))
d['status'] = 'STOPPED'
d['lastStopTimestamp'] = '2026-11-06T00:00:00.000Z'
json.dump(d, open('$LIVE/after_delete_describe.json', 'w'))"
expect_rc 6 "teardown later than h_r" G teardown "$DECLS/late.json"
TR="$REPORTS/$(date -u +%F)-p1-w1x-teardown.json"
grep -q '"requires_dated_df_row": true' "$TR" \
  && ok "h_r-exceeded receipt requires dated DF row" || bad "h_r flag"

# ── 11 · phase-close project deletion (AC-8/12) ─────────────────────────
: > "$LOG"
teardown_case "$DECLS/close.json" >/dev/null || bad "phase-close teardown failed"
LDEL="$(lnof 'projects] [delete')"
LDESC="$(grep -nFe 'projects] [describe' "$LOG" | tail -1 | cut -d: -f1)"
LIV="$(grep -nFe 'disks] [list' "$LOG" | tail -1 | cut -d: -f1)"
[ -n "$LDEL" ] && [ -n "$LDESC" ] && [ -n "$LIV" ] && \
  [ "$LIV" -lt "$LDEL" ] && [ "$LDEL" -lt "$LDESC" ] \
  && ok "inventory precedes project deletion" || bad "deletion ordering"
# receipt fixture reusing a prior phase's project id refuses (AC-8)
TR="$REPORTS/$(date -u +%F)-p1-w1x-teardown.json"
python3 -c "
import json
r = json.load(open('$TR'))
r['resources']['project_id'] = 'hh-p0-ffffff'
json.dump(r, open('$T/stale.json', 'w'))"
python3 -I scripts/gce-backend/validate_receipt.py "$T/stale.json" 2>/dev/null \
  && bad "stale-phase project id accepted" || ok "prior-phase project id refuses"

# ── 12 · seeded secret refuses the write (AC-6) ──────────────────────────
: > "$LOG"
cp "$STATE/new_instance_describe.json" "$LIVE/instance_describe.json"
sshf 'uname -r' 'fix-ollama-fake-secret-9f8e7d6c'
expect_rc 5 "seeded secret refuses receipt write" G verify "$DECLS/base.json"
sshf 'uname -r' '6.1.0-26-amd64'

# ── 13 · compute-open + gate-span fixtures (AC-13/17) ───────────────────
python3 - "$REPORTS" <<'PY'
import json, sys
r = {"schema": "hh-compute-receipt/1", "phase": "p1", "window": "w2x",
     "ticket": "R6.13", "kind": "provision",
     "recorded_at": "2026-11-07T10:00:00Z"}
json.dump(r, open(sys.argv[1] + "/2026-11-07-p1-w2x-provision.json", "w"))
PY
OPEN="$(bash scripts/compute-open.sh --reports "$REPORTS")"
printf '%s' "$OPEN" | grep -q 'w2x' \
  && ok "compute-open lists the open window" || bad "compute-open empty"
python3 - "$REPORTS" <<'PY'
import json, sys
r = {"schema": "hh-compute-receipt/1", "phase": "p1", "window": "w2x",
     "ticket": "R6.13", "kind": "teardown",
     "recorded_at": "2026-11-07T11:00:00Z"}
json.dump(r, open(sys.argv[1] + "/2026-11-07-p1-w2x-teardown.json", "w"))
PY
OPEN="$(bash scripts/compute-open.sh --reports "$REPORTS")"
[ -z "$OPEN" ] && ok "compute-open empty after teardown" || bad "compute-open stale: $OPEN"
# gate-span: deterministic receipts — provision 10:00, teardown 11:00
GS="$T/gs_reports"; RD="$T/readouts"; mkdir -p "$GS" "$RD"
python3 - "$GS" <<'PY'
import json, sys
base = {"schema": "hh-compute-receipt/1", "phase": "p1", "window": "wgs",
        "ticket": "R6.13", "recorded_at": ""}
prov = dict(base, kind="provision", recorded_at="2026-11-07T10:00:00Z")
tor = dict(base, kind="teardown", recorded_at="2026-11-07T11:00:00Z")
json.dump(prov, open(sys.argv[1] + "/2026-11-07-p1-wgs-provision.json", "w"))
json.dump(tor, open(sys.argv[1] + "/2026-11-07-p1-wgs-teardown.json", "w"))
PY
cat > "$RD/GATE-FAKE.md" <<'EOF'
# GATE-FAKE readout
Status: PASSED — read 2026-11-07T10:30:00Z — layer: engineered
EOF
bash scripts/compute-gate-span.sh --reports "$GS" --readouts "$RD" >/dev/null 2>&1 \
  && bad "gate-span passed without vm_stays_up" || ok "gate-span fails unbounded wait"
cat >> "$RD/GATE-FAKE.md" <<'EOF'
- vm_stays_up: billing read pending — VM held for reconcile (operator record)
EOF
bash scripts/compute-gate-span.sh --reports "$GS" --readouts "$RD" >/dev/null \
  && ok "gate-span admits the stays-up record" || bad "gate-span marker not honoured"
cat > "$RD/GATE-FAKE.md" <<'EOF'
# GATE-FAKE readout
Status: PASSED — read 2026-11-07T11:30:00Z — layer: engineered
EOF
bash scripts/compute-gate-span.sh --reports "$GS" --readouts "$RD" >/dev/null \
  && ok "gate after teardown does not constrain" || bad "post-teardown gate flagged"

# ── 14 · single home + sweeps (AC-1/4) ──────────────────────────────────
OTHERS="$(grep -rln 'gcloud' scripts/ --include='*.sh' | grep -v 'gce-backend\.sh' | grep -v 'check-gce-backend\.sh' | grep -v 'check-hosting-offline\.sh' || true)"
[ -z "$OTHERS" ] && ok "rg gcloud scripts/ — single provisioning home" \
  || bad "gcloud outside gce-backend.sh: $OTHERS"
"$SCAN" scan --mask-env "$ENV" \
  "$DECLS/base.json" scripts/gce-backend/startup.sh \
  && ok "leak sweep over declaration + startup.sh clean" \
  || bad "leak sweep hit on committed files"
if grep -rn 'fix-ollama-fake-secret' scripts/gce-backend.sh scripts/gce-backend/ \
    >/dev/null 2>&1; then bad "fake secret literal committed"
else ok "no secret literals in recipe"; fi

# ── 15 · R6.14 — phase spend ledger rules + reconciliation + REQ-XH-118 ─
SLIB="scripts/gce-backend/cost_lib.py"
LED="$T/ledger.jsonl"
python3 - "$LED" <<'PY'
import json, sys
rows = [
 {"type":"ceilings","kind":"model","ceiling_micros":70000000,"phase":"p1"},
 {"type":"ceilings","kind":"compute","ceiling_micros":20000000,"phase":"p1"},
 {"type":"spend","ts":"2026-11-07T20:00:00Z","ticket":"R6.14",
  "leg":"compute-estimate","cost_usd":1.5,"cumulative_usd":1.5,
  "record":"r1","kind":"compute","authority":"gcp_billing",
  "basis":"estimate","phase":"p1","window":"w1x","row_id":"sp-est-1"},
 {"type":"spend","ts":"2026-11-07T20:05:00Z","ticket":"R6.14",
  "leg":"compute-estimate","cost_usd":2.0,"cumulative_usd":3.5,
  "record":"r2","kind":"compute","authority":"gcp_billing",
  "basis":"estimate","phase":"p1","window":"w2x","row_id":"sp-est-2"},
 {"type":"spend","ts":"2026-11-08T01:00:00Z","ticket":"R6.14",
  "leg":"compute-reconcile","cost_usd":1.4,"cumulative_usd":3.4,
  "record":"r3","kind":"compute","authority":"gcp_billing",
  "basis":"billed","phase":"p1","window":"w1x","row_id":"sp-bill-1",
  "supersedes":"sp-est-1"},
]
open(sys.argv[1], "w").write("".join(json.dumps(r) + "\n" for r in rows))
PY
python3 -I "$SLIB" spend-validate "$LED" \
  && ok "valid ledger passes" || bad "valid ledger refused"
fold_out() { python3 -I "$SLIB" spend-fold "$LED"; }
printf '%s' "$(fold_out)" | grep -q '"compute_micros": 3400000' \
  && ok "billed row supersedes estimate in totals" || bad "supersede fold"
append_row() {
  printf '%s\n' "$1" > "$T/row.json"
  python3 -I "$SLIB" spend-append "$LED" "$T/row.json"
}
expect_rc 2 "window counted twice refuses" append_row \
  '{"type":"spend","ts":"t","ticket":"R6.14","leg":"x","cost_usd":1,"record":"r","kind":"compute","authority":"gcp_billing","basis":"estimate","phase":"p1","window":"w2x"}'
expect_rc 2 "compute line on model credit refuses" append_row \
  '{"type":"spend","ts":"t","ticket":"R6.14","leg":"x","cost_usd":1,"record":"r","kind":"compute","authority":"ollama_usage","basis":"estimate","phase":"p1","window":"w9x"}'
expect_rc 2 "model line on gcp billing refuses" append_row \
  '{"type":"spend","ts":"t","ticket":"R6.14","leg":"x","cost_usd":1,"record":"r","kind":"model","authority":"gcp_billing","basis":"estimate","phase":"p1","window":"w9x"}'
expect_rc 2 "billed without supersedes refuses" append_row \
  '{"type":"spend","ts":"t","ticket":"R6.14","leg":"x","cost_usd":1,"record":"r","kind":"compute","authority":"gcp_billing","basis":"billed","phase":"p1","window":"w9x"}'
expect_rc 2 "billed superseding no estimate refuses" append_row \
  '{"type":"spend","ts":"t","ticket":"R6.14","leg":"x","cost_usd":1,"record":"r","kind":"compute","authority":"gcp_billing","basis":"billed","phase":"p1","window":"w9x","supersedes":"sp-none"}'
expect_rc 2 "per-kind ceiling exceeded refuses" append_row \
  '{"type":"spend","ts":"t","ticket":"R6.14","leg":"x","cost_usd":25000000,"record":"r","kind":"compute","authority":"gcp_billing","basis":"estimate","phase":"p1","window":"w9x"}'
# a validator-level double-count over a doctored file
printf '%s\n' '{"type":"spend","ts":"t","ticket":"R6.14","leg":"x","cost_usd":1,"record":"r","kind":"compute","authority":"gcp_billing","basis":"estimate","phase":"p1","window":"w2x","row_id":"sp-dup"}' >> "$LED"
expect_rc 2 "validator rejects window counted twice" \
  python3 -I "$SLIB" spend-validate "$LED"
python3 -c "open('$LED','w').write(''.join(open('$LED').readlines()[:-1]))"
# ── compute reconciliation statuses (REQ-XB-216 / AC-3.10-4) ────────────
prep_live; G teardown "$DECLS/base.json" >/dev/null || bad "teardown for reconcile failed"
TR="$REPORTS/$(date -u +%F)-p1-w1x-teardown.json"
CR="$REPORTS/$(date -u +%F)-p1-w1x-cost.json"
EST="$(python3 -c "import json;print(json.load(open('$CR'))['estimate']['total_micros'])")"
printf '{"amount_micros": %s}\n' "$EST" > "$T/bill-within.json"
python3 -I "$SLIB" reconcile "$CR" "$T/bill-within.json" "$REPORTS" "$REPO" \
  >/dev/null && ok "reconcile within appends" || bad "reconcile within"
RC="$REPORTS/$(date -u +%F)-p1-w1x-cost-reconcile.json"
grep -q '"verdict": "within"' "$RC" && ok "within verdict" || bad "within verdict"
python3 -I "$SLIB" costvalidate "$RC" "$TR" "$REPO" \
  && ok "reconcile record validates" || bad "reconcile record invalid"
rm -f "$RC"
printf '{"amount_micros": %s, "explanation": "egress estimate under"}\n' \
  "$((EST + EST/2))" > "$T/bill-out.json"
python3 -I "$SLIB" reconcile "$CR" "$T/bill-out.json" "$REPORTS" "$REPO" \
  >/dev/null && ok "reconcile outside w/ explanation" || bad "reconcile outside"
grep -q '"verdict": "outside"' "$RC" && ok "outside verdict" || bad "outside verdict"
rm -f "$RC"
printf '{"amount_micros": %s}\n' "$((EST + EST/2))" > "$T/bill-out2.json"
expect_rc 2 "outside without explanation refuses" \
  python3 -I "$SLIB" reconcile "$CR" "$T/bill-out2.json" "$REPORTS" "$REPO"
python3 -I "$SLIB" reconcile "$CR" - "$REPORTS" "$REPO" >/dev/null \
  && grep -q '"unavailable"' "$RC" \
  && ok "unavailable{reason} folds" || bad "unavailable fold"
rm -f "$RC"
# ── REQ-XH-118 first-pass declaration checks (AC-10.1-9) ────────────────
mkdecl "$DECLS/w1.json" 'd["window"]="w1x"; d["labels"]["hh-window"]="w1x"'
set_qid "$DECLS/w1.json"
mkdecl "$DECLS/wc.json" 'd["window"]="wcx"; d["labels"]["hh-window"]="wcx"; d["quote_inputs"]["work_items"]=[]; d["dead_man"]["max_run_duration_hours"]=4; d["dead_man"]["in_guest_timer_minutes"]=255; d["model_line"]="none"; d["authorization"]["model_line"]="none"'
set_qid "$DECLS/wc.json"
mkdecl "$DECLS/w3.json" 'd["window"]="w3x"; d["labels"]["hh-window"]="w3x"; d["quote_inputs"]["work_items"]=["hset_build","export"]'
set_qid "$DECLS/w3.json"
python3 -I "$SLIB" fpcheck "$REPO" "$DECLS/w1.json" "$DECLS/w3.json" "$DECLS/wc.json" \
  && ok "fpcheck passes the declaration set" || bad "fpcheck happy"
mkdecl "$DECLS/badsum.json" 'd["window"]="w9x"; d["labels"]["hh-window"]="w9x"; d["dead_man"]["max_run_duration_hours"]=2000; d["dead_man"]["in_guest_timer_minutes"]=120015'
set_qid "$DECLS/badsum.json"
expect_rc 2 "Σ ceilings above twenty refuses" \
  python3 -I "$SLIB" fpcheck "$REPO" "$DECLS/w1.json" "$DECLS/badsum.json" "$DECLS/wc.json"
mkdecl "$DECLS/badexport.json" 'd["window"]="w1x"; d["labels"]["hh-window"]="w1x"; d["quote_inputs"]["export_size_gib"]=40'
set_qid "$DECLS/badexport.json"
expect_rc 2 "W1 export above egress line refuses" \
  python3 -I "$SLIB" fpcheck "$REPO" "$DECLS/badexport.json" "$DECLS/wc.json"
mkdecl "$DECLS/baditems.json" 'd["window"]="w1x"; d["labels"]["hh-window"]="w1x"; d["quote_inputs"]["work_items"]=["observe_pass"]'
set_qid "$DECLS/baditems.json"
expect_rc 2 "W1 quote missing items refuses" \
  python3 -I "$SLIB" fpcheck "$REPO" "$DECLS/baditems.json" "$DECLS/wc.json"
mkdecl "$DECLS/badcut.json" 'd["window"]="w1x"; d["labels"]["hh-window"]="w1x"; d["dead_man"]["max_run_duration_hours"]=2; d["dead_man"]["in_guest_timer_minutes"]=135'
set_qid "$DECLS/badcut.json"
expect_rc 2 "W_p > D − cut-off refuses" \
  python3 -I "$SLIB" fpcheck "$REPO" "$DECLS/badcut.json" "$DECLS/wc.json"
expect_rc 2 "no contingency window refuses" \
  python3 -I "$SLIB" fpcheck "$REPO" "$DECLS/w1.json" "$DECLS/w3.json"
python3 -I "$SLIB" fpcheck "$REPO" "$DECLS/w1.json" "$DECLS/wc.json" \
  --export-measured-gib 10 >/dev/null \
  && ok "export overrun absorbed by Wc" || bad "overrun absorb"
expect_rc 2 "overrun Wc cannot absorb refuses" \
  python3 -I "$SLIB" fpcheck "$REPO" "$DECLS/w1.json" "$DECLS/wc.json" \
  --export-measured-gib 300

printf 'check-gce-backend: %d passed, %d failed\n' "$PASS" "$FAILED"
[ "$FAILED" -eq 0 ] || exit 1
