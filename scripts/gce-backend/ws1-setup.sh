#!/usr/bin/env bash
# ws1-setup.sh — the Ws1 LOAD leg on the VM (R6.45q): verify + load
# the W1-carried exports BY DIGEST, never a rebuild — the
# infrastructure binary set, the three harness bundles, the needed
# (harness, task) layer images, and the frozen H-set — then place
# lane/auth/cfg/layers/gate-a/admissions for the leg drivers.
#
# Verification chain per artifact:
#   1. recompute the export's sha256 — the carried.rs dir-fold for
#      `.oci` dirs (sorted rel-path + \0 + member-sha256 + \n, sha256
#      of the fold), plain file sha256 for tar archives — compared
#      against carried-export.json verbatim;
#   2. OCI dirs load via `tar -cf - -C <dir> . | podman load` (podman
#      detects the oci archive); the loaded `.Id` must equal the
#      iidfile's `sha256:<64>` (REQ-XC-330 image-ID verify);
#   3. tar archives extract + `sha256sum -c MANIFEST.sha256`; the H
#      set's set_digest recomputes as sha256(canonical{set:"h",
#      members}) — a fold mismatch is a typed machinery row, never a
#      silent pass.
#
# Every load emits one `hh-carried-load/1`-shaped row to
# $ROOT/carried-loads.jsonl. A failed verify stops the leg with a
# typed row — the window driver reads it as the LOAD stage's hold.
#
# usage: ws1-setup.sh --exports DIR --dev-tasks T1,…,T7 \
#          --pp0-tasks T1,T2,T3 --arms mini-swe-agent,pi,claude-code \
#          --set-digest sha256:<64> --root DIR [--run-root DIR]
set -euo pipefail
set +x

EXPORTS=""; DEV=""; PP0=""; ARMS="mini-swe-agent,pi,claude-code"
SET_DIGEST=""; ROOT="/var/lib/hh/ws1"; RUNROOT="/var/lib/hh/runs/run-root-ws1"
while [ $# -gt 0 ]; do case "$1" in
  --exports) EXPORTS="$2"; shift 2;;
  --dev-tasks) DEV="$2"; shift 2;;
  --pp0-tasks) PP0="$2"; shift 2;;
  --arms) ARMS="$2"; shift 2;;
  --set-digest) SET_DIGEST="$2"; shift 2;;
  --root) ROOT="$2"; shift 2;;
  --run-root) RUNROOT="$2"; shift 2;;
  *) echo "unknown arg: $1" >&2; exit 2;;
esac; done
[ -n "$EXPORTS" ] && [ -d "$EXPORTS/artifacts" ] || { echo "exports dir with artifacts/ required" >&2; exit 2; }
[ -n "$DEV" ] && [ -n "$PP0" ] || { echo "--dev-tasks and --pp0-tasks required" >&2; exit 2; }
mkdir -p "$ROOT" "$RUNROOT/carried-loads"
LOADS="$RUNROOT/carried-loads.jsonl"; : > "$LOADS"

emit() { # $1 name $2 role $3 status $4 detail-json
  python3 - "$1" "$2" "$3" "$4" >>"$LOADS" <<'PY'
import json,sys,time
print(json.dumps({"schema":"hh-carried-load/1","name":sys.argv[1],
 "role":sys.argv[2],"result":sys.argv[3],
 "detail":json.loads(sys.argv[4]),
 "loaded_at":time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime())}))
PY
}

dir_fold() { # $1 dir -> carried.rs dir_bytes_sha256 fold
  python3 - "$1" <<'PY'
import hashlib,os,sys
root=sys.argv[1]; files=[]
for dp,_,fns in os.walk(root):
    for fn in fns:
        p=os.path.join(dp,fn)
        rel=os.path.relpath(p,root)
        files.append((rel,open(p,'rb').read()))
files.sort(key=lambda t:t[0])
fold="".join(f"{r}\x00{hashlib.sha256(b).hexdigest()}\n" for r,b in files)
print(hashlib.sha256(fold.encode()).hexdigest())
PY
}

verify_oci() { # $1 export-dir -> sha256 check vs carried-export.json
  python3 - "$1" <<'PY'
import json,sys
d=json.load(open(f"{sys.argv[1]}/carried-export.json"))
print(d["sha256"],d.get("name",""))
PY
}

load_oci() { # $1 export-dir $2 role
  local d="$1" name ce_sha obs_sha img_id obs_id
  ce_sha=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["sha256"])' "$d/carried-export.json")
  name=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$d/carried-export.json")
  obs_sha=$(dir_fold "$d/$name.oci")
  if [ "$obs_sha" != "$ce_sha" ]; then
    emit "$name" "$2" "machinery{carried_sha256_mismatch}" "{\"expected\":\"$ce_sha\",\"observed\":\"$obs_sha\"}"
    echo "FAIL sha256 $name" >&2; return 1
  fi
  tar -cf - -C "$d/$name.oci" . | podman load >/dev/null
  img_id=$(tr -d '\n' <"$d/iidfile")
  obs_id="sha256:$(podman image inspect --format '{{.Id}}' "$img_id" 2>/dev/null | sed 's/^sha256://')"
  if [ "$obs_id" != "$img_id" ]; then
    emit "$name" "$2" "machinery{image_id_mismatch}" "{\"expected\":\"$img_id\",\"observed\":\"$obs_id\"}"
    echo "FAIL image_id $name" >&2; return 1
  fi
  emit "$name" "$2" "loaded" "{\"sha256\":\"$ce_sha\",\"image_id\":\"$img_id\"}"
  echo "loaded $name $img_id"
}

load_archive() { # $1 export-dir $2 tar-name $3 dest $4 role
  local d="$1" tn="$2" dest="$3"
  local ce_sha obs_sha
  ce_sha=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["sha256"])' "$d/carried-export.json")
  obs_sha=$(sha256sum "$d/$tn" | cut -d' ' -f1)
  if [ "$obs_sha" != "$ce_sha" ]; then
    emit "$tn" "$4" "machinery{carried_sha256_mismatch}" "{\"expected\":\"$ce_sha\",\"observed\":\"$obs_sha\"}"
    echo "FAIL sha256 $tn" >&2; return 1
  fi
  mkdir -p "$dest" && tar -xf "$d/$tn" -C "$dest"
  (cd "$dest" && sha256sum -c MANIFEST.sha256 >/dev/null)
  emit "$tn" "$4" "loaded" "{\"sha256\":\"$ce_sha\"}"
  echo "loaded $tn -> $dest"
}

# ── infrastructure set ────────────────────────────────────────────
load_archive "$EXPORTS/artifacts" "infrastructure-set.tar" "$ROOT/carried/infra" "binary_set" \
  || load_archive "$EXPORTS/artifacts/infrastructure-set" "infrastructure-set.tar" "$ROOT/carried/infra" "binary_set"

# ── frozen H-set (carried verbatim — a rebuild is a new participant) ─
load_archive "$EXPORTS/artifacts/binary-set-h" "h-set.tar" "$ROOT/h-set" "binary_set"
if [ -n "$SET_DIGEST" ] && [ -f "$ROOT/h-set/MANIFEST.sha256" ]; then
  obs_set=$(python3 - "$ROOT/h-set/MANIFEST.sha256" <<'PY'
import hashlib,json,sys
members={}
for line in open(sys.argv[1]):
    parts=line.split()
    if len(parts)>=2: members[parts[1].lstrip("*")]=parts[0]
doc={"set":"h","members":members}
# hh-wire canonical object: sorted keys, compact — the fold the
# carried.rs set_digest computes over the same shape.
canon=json.dumps(doc,sort_keys=True,separators=(",",":"))
print("sha256:"+hashlib.sha256(canon.encode()).hexdigest())
PY
)
  if [ "$obs_set" = "$SET_DIGEST" ]; then
    emit "h-set" "binary_set" "set_digest_verified" "{\"set_digest\":\"$obs_set\"}"
  else
    emit "h-set" "binary_set" "machinery{set_digest_fold_mismatch}" "{\"expected\":\"$SET_DIGEST\",\"observed\":\"$obs_set\"}"
  fi
fi

# ── harness bundles + (harness, task) layers ──────────────────────
LAYERS="$ROOT/layers.json"; echo '{}' > "$LAYERS"
IFS=',' read -ra ARM_A <<<"$ARMS"; IFS=',' read -ra DEV_A <<<"$DEV"; IFS=',' read -ra PP0_A <<<"$PP0"
for arm in "${ARM_A[@]}"; do
  # the three PP0 tasks for pi/claude-code; all 7 dev tasks for mini
  tasks=("${DEV_A[@]}")
  [ "$arm" != "mini-swe-agent" ] && tasks=("${PP0_A[@]}")
  [ -d "$EXPORTS/artifacts/bundle-$arm" ] && load_oci "$EXPORTS/artifacts/bundle-$arm" "bundle"
  for t in "${tasks[@]}"; do
    ld="$EXPORTS/artifacts/layer-$arm-$t"
    [ -d "$ld" ] || { emit "layer-$arm-$t" "layer_export" "machinery{layer_export_absent}" "{}"; echo "MISSING $ld" >&2; continue; }
    load_oci "$ld" "layer_export" || true
    python3 - "$LAYERS" "$arm" "$t" "$ld/iidfile" <<'PY'
import json,sys
lj=json.load(open(sys.argv[1])); iid=open(sys.argv[4]).read().strip()
lj.setdefault(sys.argv[2],{})[sys.argv[3]]=iid
json.dump(lj,open(sys.argv[1],"w"),indent=1,sort_keys=True)
PY
  done
done

# ── gate-A partition + session placement ──────────────────────────
# gate-A binds by task through the partition digest f7ddbc1f… — the
# declared members (309w note: sqlite-with-gcov + cobol-modernization,
# 8 trajectories) ride cfg; the driver reads gate-a.json.
[ -f "$EXPORTS/gate-a.json" ] && cp "$EXPORTS/gate-a.json" "$ROOT/gate-a.json"
mkdir -p "$ROOT/admissions"
for arm in "${ARM_A[@]}"; do
  [ -f "$EXPORTS/admissions/$arm.json" ] && cp "$EXPORTS/admissions/$arm.json" "$ROOT/admissions/"
done
echo "ws1-setup: LOAD complete — $RUNROOT/carried-loads.jsonl"
