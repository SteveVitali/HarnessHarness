#!/usr/bin/env bash
# hf-upload.sh — the §4.10 per-window receipt-archive upload kit
# (manifest 276/R6.23; AR-20/ADR-0437; REQ-XH-102/103/104/107/023).
# Controller-only: runs after the window's harvest manifest verifies,
# before the next paid window's gate (FP-S's one authorization covers
# every window). The admitted tool is `hf` (huggingface_hub 2.4.0 —
# `HH_HF_BIN` overrides, the HH_GCLOUD_BIN convention).
#
#   hf-upload.sh upload    <decl.json>
#   hf-upload.sh verify    <decl.json>
#   hf-upload.sh remediate <remediation-decl.json>
#   hf-upload.sh login     — REFUSED (the token never persists, REQ-XH-023)
#
# The declaration (hh-archive-upload-decl/1) names {phase, window,
# ticket, dataset{repo, revision, visibility}, harvest{manifest, root},
# index, files[{path, role}]}; the remediation declaration adds
# rotated_provided, purge{note, revision}, tombstone{reason, refs[]}.
# A second env file the decl may name — slot_token_env — carries
# SLOT_TOKEN_<id>=<value> for the REQ-XH-103 tier-2 sweep; revoked lists
# the ids whose security.credential.revoked rows were verified.
#
# HF_WRITE_TOKEN is read at use from HH_OPERATOR_ENV_FILE and passed per
# invocation as HF_TOKEN — never printed, persisted or `login`ed; the
# receipt records provided: yes/no. Receipts land in
# $HH_ARCHIVE_REPORTS_DIR (default <repo>/docs/build/reports/archive) —
# every record is leak-scanned before write (exit 5 on a hit).
#
# Exit: 0 pass · 1 usage · 2 Refused{…} · 5 leak-scan hit ·
#       6 inconclusive{readback_unavailable} · 7 required tool absent.
set -euo pipefail
set +x

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
LIB="$REPO/scripts/gce-backend/receipt_lib.py"
REPORTS="${HH_ARCHIVE_REPORTS_DIR:-$REPO/docs/build/reports/archive}"
SCAN_BIN="${HH_SECRET_SCAN:-$REPO/target/debug/hh-secret-scan}"
HF="${HH_HF_BIN:-hf}"

say()  { printf '%s\n' "hf-upload: $*"; }
fail() { printf '%s\n' "hf-upload: FAIL — $*" >&2; }
usage() { sed -n '2,22p' "$0"; exit "${1:-1}"; }
require() { command -v "$1" >/dev/null 2>&1 || { fail "required tool absent: $1"; exit 7; }; }
refuse() { printf 'Refused{%s}\n' "$1" >&2; exit 2; }
inconclusive() { printf 'inconclusive{%s}\n' "$1" >&2; exit 6; }
utc() { date -u +%FT%TZ; }
today() { date -u +%F; }

TMPD=""
tmpd() {
  [ -n "$TMPD" ] && return
  TMPD="$(mktemp -d "${TMPDIR:-/tmp}/hh-hf.XXXXXX")"
  trap 'rm -rf "$TMPD"' EXIT
  CMDS_JSONL="$TMPD/commands.jsonl"; : > "$CMDS_JSONL"
}

scan_bin() {
  if [ ! -x "$SCAN_BIN" ]; then
    require cargo
    (cd "$REPO" && cargo build -q -p hharness-secrets --bins) \
      || { fail "cannot build hh-secret-scan"; exit 7; }
  fi
  printf '%s\n' "$SCAN_BIN"
}

leak_scan() { # FILE… — the ADR-0363 D6 sweep; a hit refuses the write (5)
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

J() { python3 -I "$LIB" jget "$DECL" "$1"; }

sha256_file() { # $1 file → the hex digest (sha256sum or shasum fallback)
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

verify_manifest() { # $1 manifest $2 root — sha256sum -c, either spelling
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$2" && sha256sum -c "$1" >/dev/null 2>&1)
  else
    (cd "$2" && shasum -a 256 -c "$1" >/dev/null 2>&1)
  fi
}

load_decl() {
  require python3
  [ -n "$DECL" ] && [ -f "$DECL" ] || { fail "no declaration: $DECL"; usage 1; }
  SCHEMA="$(J schema)"; [ "$SCHEMA" = "hh-archive-upload-decl/1" ] \
    || refuse "decl.schema:$SCHEMA"
  WINDOW="$(J window)"; PHASE="$(J phase)"; TICKET="$(J ticket)"
  REPO_ID="$(J dataset.repo)"; REV="$(J dataset.revision)"
  VIS="$(J dataset.visibility)"
  [ "$VIS" = "private" ] || refuse "dataset.not_private"
}

# REQ-XB-220 — the harvest manifest verifies locally before any upload.
verify_harvest() {
  local man root
  man="$(J harvest.manifest)"; root="$(J harvest.root)"
  [ -f "$man" ] || refuse "harvest.unverified:$man"
  verify_manifest "$man" "$root" || refuse "harvest.unverified:$man"
}

# REQ-XH-023 — the token is read at use; provided: yes|no only.
PROVIDED=""
token_check() {
  [ -n "$PROVIDED" ] && return
  local envf="${HH_OPERATOR_ENV_FILE:-}"
  [ -n "$envf" ] && [ -f "$envf" ] || refuse "env_file.absent:HH_OPERATOR_ENV_FILE"
  if grep -q '^HF_WRITE_TOKEN=' "$envf"; then PROVIDED=yes; else PROVIDED=no; fi
}

# token_env NAME — read one member's value at use (never printed).
token_val() {
  grep "^$1=" "${HH_OPERATOR_ENV_FILE}" | cut -d= -f2-
}

# hfo args… — one tool invocation; HF_TOKEN rides the environment per
# invocation only when provided. argv lands in commands[] (redacted).
hfo() {
  local out err started ended rc
  out="$(mktemp "$TMPD/out.XXXXXX")"; err="$(mktemp "$TMPD/err.XXXXXX")"
  started="$(utc)"
  if [ "$PROVIDED" = yes ]; then
    HF_TOKEN="$(token_val HF_WRITE_TOKEN)" "$HF" "$@" >"$out" 2>"$err" || rc=$?
  else
    "$HF" "$@" >"$out" 2>"$err" || rc=$?
  fi
  rc=${rc:-0}; ended="$(utc)"
  python3 -I "$LIB" command "$CMDS_JSONL" "$started" "$ended" "$rc" "$out" "$err" -- "$@"
  printf '%s' "$rc" > "$TMPD/last_rc"
  cat "$out"; return "$rc"
}

# REQ-XH-103 — pre-upload scan: live-credential mask set + repo detector
# set via hh-secret-scan (CC1); then tier-2 slot tokens — a revoked token
# may stay in a raw role, an unrevoked one is a live hit, and a token in
# a derived role was not tombstoned (refuse the file, stop the row).
RAW_ROLES=" raw_receipts egress_log native_log store_segment "
prescan() { # $1 file $2 role — exit 0 pass | refuse/stop
  local f="$1" role="$2" sb
  sb="$(scan_bin)"
  local envf="${HH_OPERATOR_ENV_FILE:-}" margs=()
  margs+=(--mask-env "$envf")
  "$sb" scan "${margs[@]}" "$f" > "$TMPD/ps.out" 2>/dev/null \
    || refuse "live_credential:$f"
  local tenv revoked="" id tok
  tenv="$(J slot_token_env 2>/dev/null || true)"
  if [ -n "$tenv" ] && [ -f "$tenv" ]; then
    revoked="$(J slot_token_revoked 2>/dev/null || true)"
    for id in $(sed -n 's/^\(SLOT_TOKEN_[A-Za-z0-9_]*\)=.*/\1/p' "$tenv"); do
      tok="$(grep "^$id=" "$tenv" | cut -d= -f2-)"
      if [ -n "$tok" ] && grep -qF -- "$tok" "$f"; then
        case " $revoked " in
          *" $id "*) ;; # revocation verified — stays in raw roles only
          *) refuse "unrevoked_slot_token:$f" ;;
        esac
        case "$RAW_ROLES" in *" $role "*) ;; *) refuse "tombstone_missing:$f" ;; esac
      fi
    done
  fi
}

write_receipt() { # $1 kind $2 verification-or-empty
  python3 -I - "$1" "$2" "$DECL" "$CMDS_JSONL" "$PROVIDED" "$REPO/scripts/hf-upload.sh" <<'PY'
import datetime, hashlib, json, subprocess, sys
kind, ver, decl_p, cmds_p, provided, script = sys.argv[1:7]
d = json.load(open(decl_p))
def sha(p): return hashlib.sha256(open(p, "rb").read()).hexdigest()
try:
    head = subprocess.check_output(["git", "-C", sys.argv[7] if len(sys.argv) > 7 else ".",
        "rev-parse", "HEAD"], text=True, stderr=subprocess.DEVNULL).strip()
except Exception:
    head = "unknown"
files = [{"path": f["path"], "sha256": sha(f["src"]), "bytes": len(open(f["src"],"rb").read())}
         for f in d.get("files", []) if kind in ("upload", "verify")]
cmds = [json.loads(l) for l in open(cmds_p) if l.strip()]
rec = {"schema": "hh-archive-upload/1", "kind": kind,
       "recorded_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
       "ticket": d["ticket"], "phase": d["phase"], "window": d["window"],
       "dataset": {"repo": d["dataset"]["repo"], "revision": d["dataset"]["revision"],
                   "visibility": d["dataset"]["visibility"]},
       "tool": {"name": "hf", "package": "huggingface_hub", "version": "2.4.0",
                "admission": "ADR-0437",
                "scope": "one private HF dataset repository",
                "token_handling": "HF_TOKEN env per invocation; no login"},
       "provided": {"HF_WRITE_TOKEN": provided},
       "files": files, "commands": cmds,
       "recipe": {"path": "scripts/hf-upload.sh", "sha256": sha(script),
                  "git_head": head}}
if ver: rec["verification"] = ver
print(json.dumps(rec, sort_keys=True))
PY
}

persist() { # $1 receipt-json-file $2 name — leak-scan then commit path
  leak_scan "$1"
  mkdir -p "$REPORTS"
  cp "$1" "$REPORTS/$(today)-${PHASE}-${WINDOW}-$2.json"
  printf '%s\n' "$REPORTS/$(today)-${PHASE}-${WINDOW}-$2.json"
}

cmd_upload() {
  require "$HF"; require python3
  { command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1; } \
    || { fail "required tool absent: sha256sum/shasum"; exit 7; }
  load_decl
  verify_harvest
  token_check
  tmpd
  python3 -I - "$DECL" > "$TMPD/files.json" <<'PY'
import json, sys
for f in json.load(open(sys.argv[1])).get("files", []):
    print(f["src"] + "\t" + f["path"] + "\t" + f["role"])
PY
  while IFS="$(printf '\t')" read -r src path role; do
    [ -n "$src" ] || continue
    prescan "$src" "$role"
    hfo upload "$REPO_ID" "$src" "$path" --repo-type dataset \
      || refuse "upload.failed:$path"
  done < "$TMPD/files.json"
  write_receipt upload "" > "$TMPD/receipt.json"
  persist "$TMPD/receipt.json" upload
}

cmd_verify() {
  require "$HF"; require python3
  { command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1; } \
    || { fail "required tool absent: sha256sum/shasum"; exit 7; }
  load_decl
  token_check
  tmpd
  # REQ-XH-104 — read-back every indexed file; a mismatch fails the row,
  # an unavailable read-back is inconclusive{readback_unavailable}.
  python3 -I - "$DECL" "$TMPD/rb.tsv" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
idx = json.load(open(d["index"]))
rows = []
for a in idx.get("attempts", []):
    for f in a.get("files", []):
        rows.append((f["path"], f["sha256"]))
for a in idx.get("artifacts", []):
    if a.get("upload") == "archived":
        rows.append((a["path"], a["sha256"]))
with open(sys.argv[2], "w") as o:
    for p, s in rows:
        o.write(p + "\t" + s + "\n")
PY
  local bad="" missing=""
  while IFS="$(printf '\t')" read -r path sha; do
    [ -n "$path" ] || continue
    if ! hfo download "$REPO_ID" "$path" --repo-type dataset \
         --local-dir "$TMPD/rb" >/dev/null; then missing=1; break; fi
    got="$(sha256_file "$TMPD/rb/$path")"
    [ "$got" = "$sha" ] || { bad="$path"; break; }
  done < "$TMPD/rb.tsv"
  local ver
  if [ -n "$bad" ]; then
    ver="refused{sha_mismatch:$bad}"
    write_receipt verify "$ver" > "$TMPD/receipt.json"
    persist "$TMPD/receipt.json" verify
    refuse "sha_mismatch:$bad"
  elif [ -n "$missing" ]; then
    ver="inconclusive{readback_unavailable}"
    write_receipt verify "$ver" > "$TMPD/receipt.json"
    persist "$TMPD/receipt.json" verify
    inconclusive "readback_unavailable"
  else
    ver="archive_verified"
    write_receipt verify "$ver" > "$TMPD/receipt.json"
    persist "$TMPD/receipt.json" verify
    say "archive_verified"
  fi
}

cmd_remediate() {
  require python3
  load_decl
  tmpd
  # REQ-XH-107 — in order: the HUMAN rotation row (provided: yes/no),
  # the operator-recorded purge record, the tombstone index entry.
  # Q-X59 pending: the purge is recorded, never executed by this row.
  python3 -I - "$DECL" "$TMPD" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
out = sys.argv[2]
rot = {"schema": "hh-human-act/1", "act": "credential_rotation", "human": True,
       "provided": {"credential_rotated": "yes" if d.get("rotated_provided") else "no"}}
purge = {"schema": "hh-archive-purge/1", "act": "dataset_revision_purge",
         "repo": d["dataset"]["repo"], "revision": d["purge"]["revision"],
         "operator_recorded": True, "note": d["purge"]["note"],
         "pre_authorization": "Q-X59 pending — recorded default"}
tomb = {"schema": "hh-receipt-index-tombstone/1", "act": "tombstone",
        "tombstone": {"reason": d["tombstone"]["reason"]},
        "supersedes": [{"ref": r} for r in d["tombstone"]["refs"]]}
for name, rec in [("rotation", rot), ("purge", purge), ("tombstone", tomb)]:
    json.dump(rec, open(out + "/" + name + ".json", "w"), sort_keys=True)
PY
  for name in rotation purge tombstone; do
    persist "$TMPD/$name.json" "remediate-$name"
  done
}

case "${1:-}" in
  login)     refuse "token.login_forbidden" ;;
  upload)    DECL="${2:-}"; cmd_upload ;;
  verify)    DECL="${2:-}"; cmd_verify ;;
  remediate) DECL="${2:-}"; cmd_remediate ;;
  *)         usage 1 ;;
esac
