#!/usr/bin/env bash
# gce-backend.sh — the compute recipe (manifest 266/R6.13; spec §3.9/§3.10
# inputs/§3.11; ADR-0431/AR-15). The single provisioning home — bash 3.2,
# `gcloud` + `ssh` + POSIX + `python3 -I` (`HH_GCLOUD_BIN` overrides the
# gcloud binary, the HH_PODMAN_BIN convention). No Terraform. A missing `gcloud`
# exits 7 "required tool absent" like live-verify.sh.
#
#   gce-backend.sh quote          <decl> [--out FILE]
#   gce-backend.sh provision      <decl> --authorization <gate-record.json>
#   gce-backend.sh verify         <decl>
#   gce-backend.sh prepull        <decl>
#   gce-backend.sh deliver-secret <decl> OLLAMA_API_KEY
#   gce-backend.sh status         <decl>
#   gce-backend.sh harvest        <decl>
#   gce-backend.sh teardown       <decl>
#   gce-backend.sh inventory      <decl>
#
# Receipts (hh-compute-receipt/1) land under docs/build/reports/compute/
# (HH_COMPUTE_REPORTS_DIR overrides — tests). Every text a receipt would
# carry passes hh-secret-scan before write; a hit refuses the write.
# Secrets ride stdin only (§3.11): HH_OPERATOR_ENV_FILE names the
# names-only env file, read at use, never copied; records say
# `provided: yes/no`. HH_BILLING_ACCOUNT names the operator-local billing
# account (never recorded — `provided` only).
#
# Exit codes: 0 pass · 1 usage · 2 Refused{…} · 4 Unsupported{backend.…} ·
# 5 leak-scan hit (nothing written) · 6 inconclusive / DF-row required ·
# 7 required tool absent.
set -euo pipefail
set +x # belt: tracing must never expose HH_OPERATOR_ENV_FILE contents

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
BACKEND="$REPO/scripts/gce-backend"
VDECL="$BACKEND/validate_decl.py"
LIB="$BACKEND/receipt_lib.py"
STARTUP="$BACKEND/startup.sh"
REPORTS="${HH_COMPUTE_REPORTS_DIR:-$REPO/docs/build/reports/compute}"
SCAN_BIN="${HH_SECRET_SCAN:-$REPO/target/debug/hh-secret-scan}"
export HH_IAP_CIDR="35.235.240.0/20"
NETWORK="hh-net"
FW_RULE="hh-iap-ssh"
ROUTER="hh-router"
NAT="hh-nat"

say()  { printf '%s\n' "gce-backend: $*"; }
fail() { printf '%s\n' "gce-backend: FAIL — $*" >&2; }
usage() {
  sed -n '3,29p' "$0"
  exit "${1:-1}"
}
require() { command -v "$1" >/dev/null 2>&1 || { fail "required tool absent: $1"; exit 7; }; }
GCLOUD="${HH_GCLOUD_BIN:-gcloud}"  # gcloud binary override (HH_PODMAN_BIN convention)
refuse() { printf 'Refused{%s}\n' "$1" >&2; exit 2; }
unsupported() { printf 'Unsupported{backend.%s}\n' "$1" >&2; exit 4; }
utc() { date -u +%FT%TZ; }
today() { date -u +%F; }

# ── environment plumbing ────────────────────────────────────────────────
TMPD=""
tmpd() {
  [ -n "$TMPD" ] && return
  TMPD="$(mktemp -d "${TMPDIR:-/tmp}/hh-gce.XXXXXX")"
  CMDS_JSONL="$TMPD/commands.jsonl"
  STEPS_JSONL="$TMPD/steps.jsonl"
  : > "$CMDS_JSONL"; : > "$STEPS_JSONL"
  NCMD=0; LAST_RC=0; LAST_OUT=""; LAST_ERR=""
  PROVIDED_JSON=""
  trap 'rm -rf "$TMPD"' EXIT
}

scan_bin() {
  if [ ! -x "$SCAN_BIN" ]; then
    require cargo
    (cd "$REPO" && cargo build -q -p hharness-secrets --bins) \
      || { fail "cannot build hh-secret-scan"; exit 7; }
  fi
  [ -x "$SCAN_BIN" ] || { fail "hh-secret-scan unavailable"; exit 7; }
  printf '%s\n' "$SCAN_BIN"
}

# leak_scan FILE… — the ADR-0363 D6 sweep with a mask built from the
# operator env file; a hit refuses the write (exit 5, nothing written).
leak_scan() {
  local sb; sb="$(scan_bin)"
  local envf="${HH_OPERATOR_ENV_FILE:-}"
  local margs=()
  [ -n "$envf" ] && [ -f "$envf" ] && margs+=(--mask-env "$envf")
  if ! "$sb" scan "${margs[@]}" "$@" > "$TMPD/scan.out" 2> "$TMPD/scan.err"; then
    fail "leak-scan hit — nothing written"
    sed 's/^/  /' "$TMPD/scan.err" >&2
    exit 5
  fi
}

# ── declaration ─────────────────────────────────────────────────────────
J() { python3 -I "$LIB" jget "$DECL" "$1"; }

load_decl() {
  require python3
  [ -n "$DECL" ] && [ -f "$DECL" ] || { fail "no declaration: $DECL"; usage 1; }
  python3 -I "$VDECL" "$DECL" || exit $?
  MODE="$(J mode)"; PHASE="$(J phase)"; WINDOW="$(J window)"; TICKET="$(J ticket)"
  PROJECT="$(J project_id)"; ZONE="$(J zone)"; MACHINE="$(J machine_type)"
  D_HOURS="$(J dead_man.max_run_duration_hours)"
  D_ACTION="$(J dead_man.action)"
  DMIN="$(J dead_man.in_guest_timer_minutes)"
  HR="$(J h_r_hours)"
  MODEL_LINE="$(J model_line)"
  PHASE_CLOSE="$(J phase_close)"
  IMG_PROJECT="$(J image.project)"; IMG_NAME="$(J image.name)"
  DISK_GIB="$(J boot_disk.size_gib)"
  EXT_IP="$(J network.external_ipv4)"; NAT_ON="$(J network.nat)"
  INSTANCE="$(printf 'hh-%s-%s' "$PHASE" "$WINDOW" | tr 'A-Z_' 'a-z-' | cut -c1-60)"
}

# machine_expect — declared machine_type → vCPU/GiB for verify (REQ-XB-206)
machine_expect() {
  case "$MACHINE" in
    e2-standard-8) EXP_VCPU=8;  EXP_MEM_GIB=32 ;;
    e2-standard-4) EXP_VCPU=4;  EXP_MEM_GIB=16 ;;
    *) unsupported "machine_type:$MACHINE" ;;
  esac
}

# ── gcloud wrapper — every call lands in commands[] ─────────────────────
# gdo inside $( ) or a pipe runs in a subshell: status travels via files.
gdo() {
  tmpd
  local out err started ended rc
  out="$(mktemp "$TMPD/out.XXXXXX")"; err="$(mktemp "$TMPD/err.XXXXXX")"
  started="$(utc)"
  if "$GCLOUD" "$@" >"$out" 2>"$err"; then rc=0; else rc=$?; fi
  ended="$(utc)"
  python3 -I "$LIB" command "$CMDS_JSONL" "$started" "$ended" "$rc" "$out" "$err" -- "$@"
  printf '%s' "$rc" > "$TMPD/last_rc"
  printf '%s' "$out" > "$TMPD/last_out"
  printf '%s' "$err" > "$TMPD/last_err"
  cat "$out"
  return "$rc"
}
last_rc()  { cat "$TMPD/last_rc" 2>/dev/null || printf '0\n'; }
last_err() { cat "$(cat "$TMPD/last_err" 2>/dev/null)" 2>/dev/null || true; }

vm() { # one remote command over IAP-tunnelled ssh (stdin flows through)
  gdo compute ssh "$INSTANCE" --project "$PROJECT" --zone "$ZONE" \
    --tunnel-through-iap --command "$1"
}

ensure_provided() {
  [ -n "$PROVIDED_JSON" ] && return
  local accts pk=no ga=no envf="${HH_OPERATOR_ENV_FILE:-}"
  accts="$(gdo auth list --format='value(account)' 2>/dev/null || true)"
  printf '%s' "$accts" | grep -q . && ga=yes
  [ -n "$envf" ] && [ -f "$envf" ] && grep -q '^OLLAMA_API_KEY=' "$envf" && pk=yes
  PROVIDED_JSON="{\"gcloud_auth\":\"$ga\",\"OLLAMA_API_KEY\":\"$pk\"}"
}

# ── receipts ────────────────────────────────────────────────────────────
receipt_base() { # $1 kind → skeleton on stdout
  python3 -I - "$DECL" "$1" "$REPO" "$PROVIDED_JSON" <<'PY'
import datetime, hashlib, json, subprocess, sys
decl_p, kind, repo, provided = sys.argv[1:5]
d = json.load(open(decl_p))
def sha(p):
    return hashlib.sha256(open(p, "rb").read()).hexdigest()
try:
    head = subprocess.check_output(
        ["git", "-C", repo, "rev-parse", "HEAD"], text=True,
        stderr=subprocess.DEVNULL).strip()
except Exception:
    head = "unknown"
print(json.dumps({
    "schema": "hh-compute-receipt/1",
    "kind": kind,
    "recorded_at": datetime.datetime.now(datetime.timezone.utc)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
    "ticket": d["ticket"], "phase": d["phase"], "window": d["window"],
    "recipe": {"path": "scripts/gce-backend.sh",
               "sha256": sha(repo + "/scripts/gce-backend.sh"),
               "git_head": head},
    "declaration": {"path": decl_p, "sha256": sha(decl_p)},
    "authorization": {k: d["authorization"][k] for k in
        ("gate", "readout_path", "verbatim_sha256", "quote_id",
         "ceiling_micros")},
    "resources": {"project_id": d["project_id"], "project_number": None,
        "instance_name": None, "instance_id": None, "zone": d["zone"],
        "machine_type": d["machine_type"], "creation_timestamp": None,
        "deletion_operation_end": None, "disks": [],
        "external_ipv4": "absent"},
    "verification": None,
    "provided": json.loads(provided or "{}"),
}, sort_keys=True))
PY
}

receipt_patch() { # $1 patch json file → one-level dict merge into base
  python3 -I - "$TMPD/base.json" "$1" <<'PY'
import json, sys
base = json.load(open(sys.argv[1]))
patch = json.load(open(sys.argv[2]))
for k, v in patch.items():
    if isinstance(v, dict) and isinstance(base.get(k), dict):
        base[k].update(v)
    else:
        base[k] = v
json.dump(base, open(sys.argv[1], "w"), sort_keys=True)
PY
}

write_receipt() { # $1 kind → prints the committed path; leak-scans first
  receipt_base "$1" > "$TMPD/base.json"
  receipt_base "$1" > "$TMPD/base.json"
  if [ -n "${RECEIPT_PATCH_FILE:-}" ]; then receipt_patch "$RECEIPT_PATCH_FILE"; fi
  python3 -I "$LIB" build "$TMPD/base.json" "$CMDS_JSONL" > "$TMPD/receipt.json"
  if [ "$1" = "teardown" ] && [ -s "$STEPS_JSONL" ]; then
    python3 -I - "$TMPD/receipt.json" "$STEPS_JSONL" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
r["steps"] = [json.loads(l) for l in open(sys.argv[2]) if l.strip()]
json.dump(r, open(sys.argv[1], "w"), indent=2, sort_keys=True)
PY
  fi
  leak_scan "$TMPD/receipt.json"
  mkdir -p "$REPORTS"
  local name; name="$(today)-${PHASE}-${WINDOW}-$1.json"
  cp "$TMPD/receipt.json" "$REPORTS/$name"
  printf '%s\n' "$REPORTS/$name"
}

step() { # teardown step record
  printf '{"n":%s,"name":%s,"result":%s,"at":"%s"}\n' \
    "$1" "$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$2")" \
    "$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$3")" \
    "$(utc)" >> "$STEPS_JSONL"
}

# ── shared legs ─────────────────────────────────────────────────────────
check_authorization() { # $1 auth record path
  [ -f "$1" ] || refuse "authorization.absent:$1"
  local qid; qid="$(python3 -I "$LIB" quoteid "$DECL")"
  python3 -I "$LIB" authcheck "$DECL" "$1" "$qid" || exit $?
}

pkgs_file() { python3 -I "$LIB" pkgs "$DECL" > "$TMPD/packages.txt"; }

describe_instance() { # → stdout describe JSON ("{}" when absent)
  local raw
  raw="$(gdo compute instances describe "$INSTANCE" --project "$PROJECT" \
    --zone "$ZONE" --format json)" || true
  [ -n "$raw" ] || { printf '{}\n'; return; }
  printf '%s' "$raw" | python3 -I "$LIB" describe
}

instance_status() { # → status string, or ABSENT
  local d; d="$(describe_instance 2>/dev/null)" || true
  [ -n "$d" ] || { printf 'ABSENT\n'; return; }
  printf '%s' "$d" | \
    python3 -c 'import json,sys;print((json.load(sys.stdin) or {}).get("status") or "ABSENT")' \
    2>/dev/null || printf 'ABSENT\n'
}

# inventory_class <label> <gcloud args…> → count | inconclusive{…}
inventory_class() {
  local cls="$1"; shift
  local out rc
  out="$(gdo compute "$@" --project "$PROJECT" --format json)" || true
  rc="$(last_rc)"
  if [ "$rc" -ne 0 ]; then
    printf 'inconclusive{%s_list:exit_%s}' "$cls" "$rc"; return
  fi
  if last_err | grep -qi 'WARNING'; then
    printf 'inconclusive{%s_list:warning}' "$cls"; return
  fi
  printf '%s' "$out" | python3 -I "$LIB" count
}

inventory_all() { # → "label=value" lines over every billable class
  printf 'instances=%s\n'   "$(inventory_class instances instances list)"
  printf 'disks=%s\n'       "$(inventory_class disks disks list)"
  printf 'addresses=%s\n'   "$(inventory_class addresses addresses list)"
  printf 'snapshots=%s\n'   "$(inventory_class snapshots snapshots list)"
  printf 'images=%s\n'      "$(inventory_class images images list)"
  printf 'forwarding=%s\n'  "$(inventory_class forwarding forwarding-rules list)"
  printf 'routers=%s\n'     "$(inventory_class routers routers list)"
  printf 'firewalls=%s\n'   "$(inventory_class firewalls firewall-rules list)"
  if [ "$NAT_ON" = "true" ]; then
    printf 'nats=%s\n' "$(inventory_class nats routers nats list --router "$ROUTER" --region us-central1)"
  else
    printf 'nats=0\n'
  fi
}

# ── subcommands ─────────────────────────────────────────────────────────
cmd_quote() {
  require python3
  load_decl
  local out=""; [ "${1:-}" = "--out" ] && { out="$2"; }
  local q; q="$(python3 -I "$LIB" quote "$DECL")"
  if [ -n "$out" ]; then printf '%s\n' "$q" > "$out"; say "wrote $out"; else printf '%s\n' "$q"; fi
}

cmd_provision() {
  local auth=""
  while [ $# -gt 0 ]; do
    case "$1" in --authorization) auth="$2"; shift 2;; *) usage 1;; esac
  done
  require "$GCLOUD"; require ssh; require python3
  load_decl
  tmpd
  # REQ-XB-204: the declaration and startup script are leak-swept; no
  # metadata key ever carries a secret (deliver-secret is stdin-only).
  leak_scan "$DECL" "$STARTUP"
  ensure_provided

  if [ "$MODE" = "restart-only" ]; then
    # REQ-XB-224 / Q-X60 (pending — recorded default): one harvest-and-
    # teardown-only restart <= 1 h. Instances start, nothing else.
    local st; st="$(instance_status)"
    case "$st" in
      STOPPED|TERMINATED|SUSPENDED) ;;
      *) refuse "restart_only.instance_not_stopped:$st" ;;
    esac
    gdo compute instances start "$INSTANCE" --project "$PROJECT" --zone "$ZONE" \
      || refuse "restart_only.start_failed"
    RECEIPT_PATCH_FILE="$TMPD/restart.json"
    printf '{"restart":{"mode":"restart-only","max_hours":%s}}\n' \
      "$(J restart.max_hours)" > "$RECEIPT_PATCH_FILE"
    write_receipt provision
    return
  fi

  check_authorization "$auth"

  # Project, billing, APIs — fresh project per phase (REQ-XB-212, Q-X17).
  if ! gdo projects describe "$PROJECT" >/dev/null 2>"$TMPD/pd.err"; then
    gdo projects create "$PROJECT" --quiet \
      || refuse "project_create_failed"
  fi
  [ -n "${HH_BILLING_ACCOUNT:-}" ] || refuse "billing_account.unprovided"
  gdo beta billing projects link "$PROJECT" \
    --billing-account "$HH_BILLING_ACCOUNT" >/dev/null \
    || refuse "billing_link_failed"
  gdo services enable compute.googleapis.com iap.googleapis.com \
    --project "$PROJECT" >/dev/null || refuse "services_enable_failed"

  if ! gdo compute networks describe "$NETWORK" --project "$PROJECT" \
      >/dev/null 2>"$TMPD/net.err"; then
    gdo compute networks create "$NETWORK" --subnet-mode=auto \
      --project "$PROJECT" >/dev/null || refuse "network_create_failed"
  fi
  if ! gdo compute firewall-rules describe "$FW_RULE" --project "$PROJECT" \
      >/dev/null 2>"$TMPD/fw.err"; then
    gdo compute firewall-rules create "$FW_RULE" --network "$NETWORK" \
      --allow "tcp:22" --source-ranges "$HH_IAP_CIDR" \
      --project "$PROJECT" >/dev/null || refuse "firewall_create_failed"
  fi
  if [ "$NAT_ON" = "true" ]; then
    gdo compute routers describe "$ROUTER" --region us-central1 \
      --project "$PROJECT" >/dev/null 2>"$TMPD/rt.err" || \
      gdo compute routers create "$ROUTER" --network "$NETWORK" \
        --region us-central1 --project "$PROJECT" >/dev/null \
      || refuse "router_create_failed"
    gdo compute routers nats create "$NAT" --router "$ROUTER" \
      --region us-central1 --auto-allocate-nat-ips \
      --nat-all-subnet-ip-ranges --project "$PROJECT" >/dev/null \
      || refuse "nat_create_failed"
  fi

  # REQ-XB-209: a stopped VM restarts only under REQ-XB-224's mode or a
  # new authorization; an already-running instance is a provisioning bug.
  local st; st="$(instance_status)"
  case "$st" in
    ABSENT) ;;
    STOPPED|TERMINATED|SUSPENDED)
      refuse "restart_requires:REQ-XB-224_restart-only_or_new_authorization" ;;
    *) refuse "instance_exists:$st" ;;
  esac

  pkgs_file
  local dsecs; dsecs="$(python3 -c "print(int(float('$D_HOURS')*3600))")"
  local netargs=""
  [ "$EXT_IP" = "none" ] && netargs="--no-address"
  gdo compute instances create "$INSTANCE" \
    --project "$PROJECT" --zone "$ZONE" \
    --machine-type "$MACHINE" \
    --provisioning-model STANDARD \
    --image-project "$IMG_PROJECT" --image "$IMG_NAME" \
    --boot-disk-type pd-balanced --boot-disk-size "${DISK_GIB}GB" \
    --no-service-account --no-scopes \
    --shielded-secure-boot \
    --max-run-duration="${dsecs}s" \
    --instance-termination-action "$D_ACTION" \
    --metadata "hh_deadman_minutes=$DMIN" \
    --metadata-from-file "startup-script=$STARTUP" \
    --metadata-from-file "hh_packages=$TMPD/packages.txt" \
    --labels "hh-phase=$(J labels.hh-phase),hh-window=$(J labels.hh-window),hh-ticket=$(J labels.hh-ticket),ttl=$(J labels.ttl)" \
    $netargs >/dev/null || refuse "instances_create_failed"

  # Resource facts for the receipt (id, creation timestamp, IPv4) and the
  # ingress rule set the receipt must list (REQ-XB-203's acceptance).
  local djson fwjson; djson="$(describe_instance)"
  fwjson="$(gdo compute firewall-rules list --filter "network:$NETWORK" \
    --project "$PROJECT" --format json)"
  RECEIPT_PATCH_FILE="$TMPD/resources.json"
  printf '%s\n' "$djson" > "$TMPD/d.json"
  python3 -I - "$TMPD/d.json" "$RECEIPT_PATCH_FILE" <<'PY'
import json, sys
d = json.load(open(sys.argv[1])) or {}
patch = {"resources": {"instance_name": d.get("name"),
    "instance_id": d.get("id"),
    "creation_timestamp": d.get("creation_timestamp"),
    "external_ipv4": d.get("external_ipv4", "absent")}}
json.dump(patch, open(sys.argv[2], "w"))
PY
  printf '%s' "$fwjson" > "$TMPD/fw.json"
  python3 -I - "$TMPD/fw.json" "$RECEIPT_PATCH_FILE" <<'PY'
import json, sys
rules = json.load(open(sys.argv[1]))
patch = json.load(open(sys.argv[2]))
patch["verification"] = {"firewall_rules": [
    {"name": r.get("name"),
     "allowed": r.get("allowed"),
     "sourceRanges": r.get("sourceRanges")}
    for r in (rules if isinstance(rules, list) else [])]}
json.dump(patch, open(sys.argv[2], "w"))
PY
  write_receipt provision
}

cmd_verify() {
  require "$GCLOUD"; require ssh; require python3
  load_decl
  tmpd
  ensure_provided
  machine_expect

  local ping podver compose kernel nproc memb diskb sock tcp_n djson fw
  ping="$(vm 'curl -fs --unix-socket /run/podman/podman.sock http://d/_ping' | tr -d '[:space:]')"
  [ "$ping" = "OK" ] || unsupported "podman_ping:$ping"
  podver="$(vm 'podman version --format json' | \
    python3 -c 'import json,sys;print(json.load(sys.stdin)["Client"]["Version"])' 2>/dev/null || true)"
  [ "$podver" = "$(J packages.podman)" ] || unsupported "podman_version:$podver"
  compose="$(vm 'podman compose version --format json 2>/dev/null || podman-compose --version' | head -1)"
  [ -n "$compose" ] || unsupported "compose_frontend"
  kernel="$(vm 'uname -r' | tr -d '[:space:]')"
  [ -n "$kernel" ] || unsupported "kernel"
  # dpkg pins (REQ-XB-206/220)
  local cf; cf="$(J packages.compose_frontend.name)"
  local dpkg_out; dpkg_out="$(vm "dpkg-query -W podman crun conmon netavark $cf" || true)"
  printf '%s\n' "$dpkg_out" > "$TMPD/dpkg.txt"
  python3 -I - "$DECL" "$TMPD/dpkg.txt" <<'PY' || exit $?
import json, sys
decl = json.load(open(sys.argv[1]))
p = decl["packages"]
want = {k: p[k] for k in ("podman", "crun", "conmon", "netavark")}
want[p["compose_frontend"]["name"]] = p["compose_frontend"]["version"]
got = {}
for line in open(sys.argv[2]):
    parts = line.split()
    if len(parts) >= 2:
        got[parts[0]] = parts[1]
for k, v in want.items():
    if got.get(k) != v:
        print(f"Unsupported{{backend.dpkg_pin:{k}={got.get(k)}}}", file=sys.stderr)
        sys.exit(4)
PY
  nproc="$(vm 'nproc' | tr -d '[:space:]')"
  [ "$nproc" = "$EXP_VCPU" ] || unsupported "vcpus:$nproc"
  memb="$(vm "free -b | awk 'NR==2{print \$2}'" | tr -d '[:space:]')"
  python3 -c "
import sys
gib = int('${memb:-0}') / 1073741824
sys.exit(0 if gib >= ${EXP_MEM_GIB} - 1 else 1)" || unsupported "mem_gib:$memb"
  diskb="$(vm "df -B1 --output=avail / | tail -1" | tr -d '[:space:]')"
  sock="$(vm 'stat -c %a:%U:%G /run/podman/podman.sock' | tr -d '[:space:]')"
  tcp_n="$(vm 'sh -c "ss -tln 2>/dev/null | grep -c podman || true"' | tr -d '[:space:]')"
  local socket_mode
  if [ "$sock" = "660:root:hh" ] && [ "$tcp_n" = "0" ]; then
    socket_mode="local_rootful_group_restricted"
  else
    unsupported "socket_mode:${sock:-?}/${tcp_n:-?}"
  fi
  # REQ-XB-203: the ingress set must be exactly the IAP rule
  fw="$(gdo compute firewall-rules list --filter "network:$NETWORK" \
    --project "$PROJECT" --format json)"
  printf '%s' "$fw" | python3 -I "$LIB" fwrule || exit $?
  # identity posture (REQ-XB-204/209)
  djson="$(describe_instance)"
  printf '%s' "$djson" > "$TMPD/d.json"
  python3 -I - "$TMPD/d.json" "$D_HOURS" <<'PY' || exit $?
import json, sys
d = json.load(open(sys.argv[1]))
if d.get("service_accounts"):
    print("Unsupported{backend.service_account}", file=sys.stderr)
    sys.exit(4)
want = int(float(sys.argv[2]) * 3600)
if d["max_run_seconds"] != want:
    print(f"Unsupported{{backend.max_run_duration:{d['max_run_seconds']}}}",
          file=sys.stderr)
    sys.exit(4)
PY
  RECEIPT_PATCH_FILE="$TMPD/verify.json"
  python3 -I - "$TMPD/d.json" "$DECL" "$RECEIPT_PATCH_FILE" \
      "$podver" "$compose" "$kernel" "$nproc" "$memb" "$diskb" "$socket_mode" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
decl = json.load(open(sys.argv[2]))
(podver, compose, kernel, nproc, memb, diskb, sock_mode) = sys.argv[4:]
patch = {"verification": {
    "podman_ping": "OK",
    "podman_version": podver,
    "compose_frontend": compose.strip(),
    "kernel": kernel,
    "image": decl["image"]["name"],
    "dpkg_sha256": __import__("hashlib").sha256(
        json.dumps(decl["packages"], sort_keys=True).encode()).hexdigest(),
    "vcpus": int(nproc),
    "mem_gib": round(int(memb) / 1073741824, 1),
    "disk_free_gib": round(int(diskb) / 1073741824, 1),
    "firewall_rules": ["tcp:22 from 35.235.240.0/20 (iap)"],
    "service_account": "none",
    "max_run_duration": decl["dead_man"]["max_run_duration_hours"],
    "socket_mode": sock_mode,
}, "resources": {"instance_name": d.get("name"), "instance_id": d.get("id"),
    "creation_timestamp": d.get("creation_timestamp"),
    "external_ipv4": d.get("external_ipv4", "absent")}}
json.dump(patch, open(sys.argv[3], "w"))
PY
  write_receipt verify
}

cmd_prepull() {
  require "$GCLOUD"; require ssh; require python3
  load_decl
  [ "$MODE" = "restart-only" ] && refuse "restart_only.prepull"
  tmpd
  ensure_provided
  # REQ-XB-207: only the declaration's admitted images, by digest.
  local pulls="$TMPD/pulls.json"; : > "$pulls"
  while IFS="$(printf '\t')" read -r task ref digest; do
    [ -n "$digest" ] || continue
    vm "podman pull '$ref@$digest'" >/dev/null \
      || refuse "prepull.pull_failed:$ref"
    local meta; meta="$(vm "podman image inspect '$digest' --format json")"
    printf '%s' "$meta" > "$TMPD/inspect.json"
    python3 -I - "$TMPD/inspect.json" "$pulls" "$task" "$ref" "$digest" <<'PY'
import json, sys
path, task, ref, digest = sys.argv[2:6]
doc = json.load(open(sys.argv[1]))
im = doc[0] if isinstance(doc, list) and doc else {}
rec = {"task": task, "ref": ref, "digest": digest,
       "size_bytes": im.get("Size"), "arch": im.get("Architecture")}
try:
    rows = json.load(open(path))
except Exception:
    rows = []
rows.append(rec)
json.dump(rows, open(path, "w"))
PY
  done < <(python3 -I "$LIB" images "$DECL")
  local free_b; free_b="$(vm "df -B1 --output=avail /var | tail -1" | tr -d '[:space:]')"
  RECEIPT_PATCH_FILE="$TMPD/prepull.json"
  python3 -I - "$pulls" "$RECEIPT_PATCH_FILE" "$free_b" "$DECL" <<'PY'
import json, sys
pulls, free_b, decl = open(sys.argv[1]), sys.argv[3], json.load(open(sys.argv[4]))
floor = decl["disk_floor_gib"]
free_gib = round((int(free_b or 0)) / 1073741824, 1)
patch = {"prepull": {
    "images": json.load(pulls),
    "diskwatch": {"floor_gib": floor, "free_gib": free_gib,
                  "below_floor": free_gib < floor}}}
json.dump(patch, open(sys.argv[2], "w"))
PY
  write_receipt prepull
}

cmd_deliver_secret() {
  local name="${1:-}"
  require "$GCLOUD"; require ssh; require python3
  load_decl
  tmpd
  # §3.11 / REQ-XB-219 / REQ-XB-224 / REQ-XH-023 — stdin-only path.
  [ "$MODE" = "restart-only" ] && refuse "restart_only.no_secret_delivery"
  [ "$name" = "OLLAMA_API_KEY" ] || refuse "secret.name:$name"
  [ "$MODEL_LINE" = "authorized" ] || refuse "authorization.model_line"
  local envf="${HH_OPERATOR_ENV_FILE:-}"
  [ -n "$envf" ] && [ -f "$envf" ] || refuse "env_file.absent:HH_OPERATOR_ENV_FILE"
  grep -q "^${name}=" "$envf" || refuse "secret.unprovided:$name"
  vm 'sudo install -d -m 0750 -o root -g hh /run/hh/secrets' >/dev/null \
    || refuse "secrets_dir_failed"
  # The value is piped over ssh stdin into tmpfs — never argv, never
  # metadata, never a local copy (read at use from the env file).
  grep "^${name}=" "$envf" | cut -d= -f2- | \
    gdo compute ssh "$INSTANCE" --project "$PROJECT" --zone "$ZONE" \
      --tunnel-through-iap \
      --command "sudo install -m 0400 -o root -g hh /dev/stdin /run/hh/secrets/$name" \
      >/dev/null || refuse "deliver_failed"
  printf '%s provided: yes\n' "$name"
}

cmd_status() {
  require "$GCLOUD"; require python3
  load_decl
  tmpd
  local d; d="$(describe_instance)" || true
  printf '%s\n' "${d:-{}}" > "$TMPD/d.json"
  python3 -I - "$TMPD/d.json" "$D_HOURS" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(f"instance: {d.get('status')} (id {d.get('id')}, created {d.get('creation_timestamp')})")
print(f"max_run_duration: {d.get('max_run_seconds')}s vs declared {sys.argv[2]}h")
PY
  vm "cat /proc/uptime; df -BG --output=avail / | tail -1" 2>/dev/null || true
}

harvest_impl() { # returns rc; fills $HARV_DIR; records gdo/vm calls
  HARV_DIR="$REPORTS/harvest/${PHASE}-${WINDOW}"
  mkdir -p "$HARV_DIR"
  local rroot="/var/lib/hh/runs/run-root-$WINDOW"
  # 2a: remote sha256 manifest + tar over the run root
  vm "mkdir -p '$rroot' && cd '$rroot' && find . -type f -print0 | sort -z | xargs -0 --no-run-if-empty sha256sum > /tmp/hh-manifest.sha256 && tar czf /tmp/hh-harvest.tar.gz . && sha256sum /tmp/hh-manifest.sha256 /tmp/hh-harvest.tar.gz" \
    > "$TMPD/remote.sha" || return 1
  # 2b: copy back over IAP-scp
  gdo compute scp --tunnel-through-iap --project "$PROJECT" --zone "$ZONE" \
    "$INSTANCE:/tmp/hh-harvest.tar.gz" "$HARV_DIR/run-root.tar.gz" \
    >/dev/null || return 1
  gdo compute scp --tunnel-through-iap --project "$PROJECT" --zone "$ZONE" \
    "$INSTANCE:/tmp/hh-manifest.sha256" "$HARV_DIR/MANIFEST.sha256" \
    >/dev/null || return 1
  # 2c: verify locally — extract and check every hashed member (an empty
  #     manifest means an empty run root — verified trivially)
  mkdir -p "$TMPD/verify"
  tar xzf "$HARV_DIR/run-root.tar.gz" -C "$TMPD/verify" || return 1
  if [ -s "$HARV_DIR/MANIFEST.sha256" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
      (cd "$TMPD/verify" && sha256sum -c "$HARV_DIR/MANIFEST.sha256" >/dev/null) \
        || return 1
    else
      (cd "$TMPD/verify" && shasum -a 256 -c "$HARV_DIR/MANIFEST.sha256" >/dev/null) \
        || return 1
    fi
  fi
  # journald + serial output travel with the run root (REQ-XB-220)
  vm 'journalctl --no-pager -o short-iso' > "$HARV_DIR/journald.log" 2>/dev/null || true
  gdo compute instances get-serial-port-output "$INSTANCE" \
    --project "$PROJECT" --zone "$ZONE" > "$HARV_DIR/serial.log" 2>/dev/null || true
  # harvested text is leak-swept before it may persist (ADR-0363 D6)
  leak_scan "$HARV_DIR/journald.log" "$HARV_DIR/serial.log" "$HARV_DIR/MANIFEST.sha256"
  return 0
}

harvest_patch() { # $1 out-file — the harvest{} patch for a receipt
  python3 -I - "$1" "$HARV_DIR" <<'PY'
import hashlib, json, sys
d = sys.argv[2]
def sha(p):
    return hashlib.sha256(open(p, "rb").read()).hexdigest()
patch = {"harvest": {
    "manifest": {"path": f"{d}/MANIFEST.sha256",
                 "sha256": sha(f"{d}/MANIFEST.sha256")},
    "run_root_tar": {"path": f"{d}/run-root.tar.gz",
                     "sha256": sha(f"{d}/run-root.tar.gz")},
    "journald": {"path": f"{d}/journald.log", "sha256": sha(f"{d}/journald.log")},
    "serial": {"path": f"{d}/serial.log", "sha256": sha(f"{d}/serial.log")},
    "verified_locally": True}}
json.dump(patch, open(sys.argv[1], "w"))
PY
}

cmd_harvest() {
  require "$GCLOUD"; require ssh; require python3; require tar
  { command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1; } \
    || { fail "required tool absent: sha256sum/shasum"; exit 7; }
  load_decl
  tmpd
  ensure_provided
  harvest_impl || refuse "harvest.failed"
  RECEIPT_PATCH_FILE="$TMPD/harvest.json"
  harvest_patch "$RECEIPT_PATCH_FILE"
  write_receipt harvest
}

cmd_inventory() {
  require "$GCLOUD"; require python3
  load_decl
  tmpd
  inventory_all
}

cmd_teardown() {
  require "$GCLOUD"; require ssh; require python3; require tar
  { command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1; } \
    || { fail "required tool absent: sha256sum/shasum"; exit 7; }
  load_decl
  tmpd
  ensure_provided
  # §3.9 / REQ-XB-210 — the nine steps, recorded in order, unskippable.
  # 1 — stop admissions; drain in-flight slots
  local drain_rc=0
  vm "hh-runner drain --window '$WINDOW' --inconclusive teardown" >/dev/null || drain_rc=$?
  if [ "$drain_rc" -eq 0 ]; then step 1 stop_admissions '"drained"'
  else step 1 stop_admissions '"inconclusive{teardown}"'; fi
  # 2 — harvest under a sha256 manifest, verified locally; the delete
  #     steps REFUSE without it (REQ-XB-220)
  if harvest_impl; then step 2 harvest '"verified"'
  else step 2 harvest '"failed"'; refuse "teardown.harvest_required"; fi
  # 3 — serial output + usage counters (serial captured in harvest_impl)
  vm 'cat /proc/uptime; df -BG --output=avail / | tail -1' \
    > "$HARV_DIR/usage.txt" 2>/dev/null || true
  step 3 serial_and_usage '"captured"'
  RECEIPT_PATCH_FILE="$TMPD/harvest.json"; harvest_patch "$RECEIPT_PATCH_FILE"
  write_receipt harvest >/dev/null
  # 4 — pre-delete inventory over every billable class
  local inv_pre; inv_pre="$(inventory_all)"
  step 4 inventory '"recorded"'
  # 5 — delete the instance and any others
  local names n
  names="$(gdo compute instances list --project "$PROJECT" --format json | \
    python3 -c 'import json,sys;print(" ".join(x["name"] for x in json.load(sys.stdin)))' 2>/dev/null || true)"
  for n in $names; do
    gdo compute instances delete "$n" --project "$PROJECT" --zone "$ZONE" \
      --quiet >/dev/null || refuse "teardown.delete_failed:$n"
  done
  step 5 delete_instances "\"${names:-none}\""
  # 6 — re-inventory: every class empty, or exactly the declared
  #     non-billable residue (network + the firewall rule it carries);
  #     the only admitted billable deviation is "stopped, disk retained"
  #     under a dated DF-* row inside h_r (REQ-XB-210).
  local inv_post; inv_post="$(inventory_all)"
  local post_json deviation="null" running="" leftovers="" inconcl=""
  while IFS='=' read -r cls val; do
    case "$val" in
      inconclusive*) inconcl="$inconcl $cls" ;;
      ''|*[!0-9]*) ;;
      0) ;;
      *) case "$cls" in
           instances)
             # a surviving entry: stopped+disk under DF-* row may be
             # admitted; RUNNING never is (checked below)
             leftovers="$leftovers $cls" ;;
           disks) leftovers="$leftovers $cls" ;;
           firewalls|routers|nats) : ;;  # declared non-billable residue
           *) leftovers="$leftovers $cls" ;;
         esac ;;
    esac
  done <<EOF
$inv_post
EOF
  # running VM check — never an admitted end state
  local djson; djson="$(describe_instance)" || true
  printf '%s' "$djson" | grep -q '"status": "RUNNING"' && \
    refuse "teardown.running_instance"
  # admitted deviation: only "stopped, disk retained" — leftovers limited
  # to {instances stopped, disks} under a dated DF-* row inside h_r;
  # anything else refuses (REQ-XB-210).
  if [ -n "$leftovers" ]; then
    if printf '%s\n' $leftovers | grep -vqE '^(instances|disks)$'; then
      step 6 reinventory "\"leftover:$leftovers\""
      refuse "teardown.leftover:$leftovers"
    fi
    if python3 -I "$LIB" jget "$DECL" residual.admit_stopped_disk.df_row \
       >/dev/null 2>&1; then
      deviation="$(python3 -I - "$DECL" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))["residual"]["admit_stopped_disk"]
print(json.dumps({"state": "stopped_disk_retained", **r}))
PY
)"
    else
      step 6 reinventory "\"leftover:$leftovers\""
      refuse "teardown.leftover:$leftovers"
    fi
  fi
  step 6 reinventory '"checked"'
  # 7 — phase close: project deletion shows DELETE_REQUESTED
  if [ "$PHASE_CLOSE" = "true" ]; then
    gdo projects delete "$PROJECT" --quiet >/dev/null \
      || refuse "teardown.project_delete_failed"
    local pst; pst="$(gdo projects describe "$PROJECT" --format json | \
      python3 -c 'import json,sys;print(json.load(sys.stdin)["lifecycleState"])' 2>/dev/null || true)"
    [ "$pst" = "DELETE_REQUESTED" ] || \
      refuse "teardown.project_state:$pst"
    step 7 project_delete '"DELETE_REQUESTED"'
  else
    step 7 project_delete '"skipped{not_phase_close}"'
  fi
  # 8 — kill local helper processes (leftover ssh tunnels to this VM)
  local killed=0 pids
  pids="$(pgrep -f "compute ssh.*$INSTANCE" 2>/dev/null | grep -v "$$" || true)"
  for n in $pids; do kill "$n" 2>/dev/null && killed=$((killed + 1)) || true; done
  step 8 kill_local_helpers "\"$killed\""
  # 9 — teardown receipt: settlement clock (REQ-XC-127's last-model-call
  #     UTC hour boundary), lifecycle timestamps (REQ-XB-215's intervals),
  #     the declared cost-record hook — n/a{compute_cost_record: R6.14}
  #     until R6.14 wires it; no hh-compute-cost/1 is written.
  local post_map
  printf '%s\n' "$inv_post" > "$TMPD/inv.txt"
  post_map="$(python3 -I - "$TMPD/inv.txt" "$deviation" <<'PY'
import json, sys
post = {}
for line in open(sys.argv[1]):
    cls, _, val = line.strip().partition("=")
    if val.startswith("inconclusive"):
        post[cls] = val
    elif val:
        post[cls] = int(val)
dev = json.loads(sys.argv[2]) if sys.argv[2] != "null" else None
if dev:
    post["admitted_deviation"] = dev
print(json.dumps(post))
PY
)"
  RECEIPT_PATCH_FILE="$TMPD/teardown.json"
  printf '%s' "$djson" > "$TMPD/d.json"
  python3 -I - "$TMPD/d.json" "$DECL" "$RECEIPT_PATCH_FILE" "$post_map" \
      "$(utc)" <<'PY'
import datetime, json, sys
d = json.load(open(sys.argv[1]))
decl = json.load(open(sys.argv[2]))
post = json.loads(sys.argv[4])
now = datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None)
hr = decl["h_r_hours"]
# basis: the VM's last stop timestamp when present, else now (still live)
basis = d.get("last_stop_timestamp") or now.strftime("%Y-%m-%dT%H:%M:%SZ")
bt = datetime.datetime.strptime(basis[:19], "%Y-%m-%dT%H:%M:%S")
elapsed = round(max(0.0, (now - bt).total_seconds() / 3600.0), 2)
exceeded = elapsed > hr
lmc = decl.get("last_model_call_utc")
hour = "n/a{no_model_calls}"
if isinstance(lmc, str) and lmc:
    hour = lmc[:13] + ":00Z"  # REQ-XC-127: the UTC hour boundary
patch = {
    "post_state": post,
    "lifecycle": {"creation_timestamp": d.get("creation_timestamp"),
                  "last_stop_timestamp": d.get("last_stop_timestamp"),
                  "deletion_operation_end": now.strftime("%Y-%m-%dT%H:%M:%SZ")},
    "settlement": {"last_model_call_utc_hour": hour},
    "h_r": {"declared_hours": hr, "elapsed_hours": elapsed,
            "exceeded": exceeded, "requires_dated_df_row": exceeded},
    # §3.9 step 9 — the declared cost-record hook; R6.14 owns
    # hh-compute-cost/1 and wires it (billed: pending). This row writes none.
    "cost_record": "n/a{compute_cost_record: R6.14}",
}
json.dump({"patch": patch, "exceeded": exceeded}, open(sys.argv[3], "w"))
PY
  local exceeded; exceeded="$(python3 -c 'import json;print(json.load(open("'"$RECEIPT_PATCH_FILE"'"))["exceeded"])')"
  python3 -c 'import json;json.dump(json.load(open("'"$RECEIPT_PATCH_FILE"'"))["patch"],open("'"$RECEIPT_PATCH_FILE"'","w"))'
  local rp; rp="$(write_receipt teardown)"
  say "teardown receipt: $rp"
  if [ -n "$inconcl" ]; then
    printf 'post_state inconclusive:%s\n' "$inconcl" >&2
    exit 6
  fi
  if [ "$exceeded" = "True" ] || [ "$exceeded" = "true" ]; then
    fail "teardown later than h_r — a dated DF-* row is required (REQ-XB-224)"
    exit 6
  fi
}

# ── dispatch ────────────────────────────────────────────────────────────
[ $# -ge 1 ] || usage 1
SUB="$1"; shift
case "$SUB" in
  quote|provision|verify|prepull|deliver-secret|status|harvest|teardown|inventory) ;;
  -h|--help) usage 0 ;;
  *) usage 1 ;;
esac
[ $# -ge 1 ] || usage 1
DECL="$1"; shift
case "$SUB" in
  quote)          cmd_quote "$@" ;;
  provision)      cmd_provision "$@" ;;
  verify)         cmd_verify ;;
  prepull)        cmd_prepull ;;
  deliver-secret) cmd_deliver_secret "$@" ;;
  status)         cmd_status ;;
  harvest)        cmd_harvest ;;
  teardown)       cmd_teardown ;;
  inventory)      cmd_inventory ;;
esac
