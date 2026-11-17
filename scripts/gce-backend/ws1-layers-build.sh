#!/usr/bin/env bash
# ws1-layers-build.sh — Ws1 missing-layer mint (R6.45q, operator-authorized
# 2026-11-17): the 3 fp_dev7 tasks outside W1's 20-task manifest have no
# carried exports. This builds their (arm, task) layers on-VM through the
# same committed surface (`hh-w1 image-build` → admission_build::run_build,
# LayerSpec-rendered Containerfiles — the W1 shape verbatim) and records
# each as a `built_mint` load row — never labeled carried. The base refs
# are the declared tb21 task.toml docker_image values pulled + resolved to
# digest-pinned FROM (LayerSpec refuses non-digest bases).
#
# usage: ws1-layers-build.sh --exports DIR --w1-bin BIN --loads F --layers-file F
set -euo pipefail
set +x
EXPORTS=""; W1=""; LOADS=""; LAYERS=""
while [ $# -gt 0 ]; do case "$1" in
  --exports) EXPORTS="$2"; shift 2;;
  --w1-bin) W1="$2"; shift 2;;
  --loads) LOADS="$2"; shift 2;;
  --layers-file) LAYERS="$2"; shift 2;;
  *) echo "unknown arg: $1" >&2; exit 2;;
esac; done

emit() { # $1 name $2 status $3 detail-json
  python3 - "$1" "layer_build" "$2" "$3" >>"$LOADS" <<'PY'
import json,sys,time
print(json.dumps({"schema":"hh-carried-load/1","name":sys.argv[1],
 "role":sys.argv[2],"result":sys.argv[3],
 "detail":json.loads(sys.argv[4]),
 "loaded_at":time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime())}))
PY
}

# declared refs — the tb21 task.toml docker_image values at the recorded
# corpus pin (fetched verbatim 2026-11-17).
declare -A REF=( [cancel-async-tasks]="docker.io/alexgshaw/cancel-async-tasks:20251031"
                 [build-pmars]="docker.io/alexgshaw/build-pmars:20251031"
                 [pytorch-model-recovery]="docker.io/alexgshaw/pytorch-model-recovery:20260430" )
declare -A VER=( [mini-swe-agent]="2.4.6" [pi]="0.52.3" [claude-code]="2.1.50" )
TASKS="cancel-async-tasks build-pmars pytorch-model-recovery"
PP0="cancel-async-tasks build-pmars"

for t in $TASKS; do
  ref="${REF[$t]}"
  podman pull "$ref" >/dev/null 2>&1 || sudo podman pull "$ref" >/dev/null \
    || { emit "layer-base-$t" "machinery{base_pull_failed}" "{\"ref\":\"$ref\"}"; echo "PULL FAIL $t" >&2; continue; }
  dig="$(podman image inspect "$ref" --format '{{index .RepoDigests 0}}' 2>/dev/null \
        || sudo podman image inspect "$ref" --format '{{index .RepoDigests 0}}')"
  case "$dig" in *@sha256:*) ;; *) emit "layer-base-$t" "machinery{base_digest_absent}" "{\"ref\":\"$ref\"}"; continue;; esac
  for arm in mini-swe-agent pi claude-code; do
    # mini covers all 3; pi/claude-code only the PP0 pair
    [ "$arm" = "mini-swe-agent" ] || { case " $PP0 " in *" $t "*) ;; *) continue;; esac; }
    ver="${VER[$arm]}"; lctx="$EXPORTS/artifacts/lctx-$arm"
    [ -d "$lctx/bundle" ] || { emit "layer-$arm-$t" "machinery{lctx_absent}" "{}"; continue; }
    cf="$lctx/Containerfile-$t"
    cat > "$cf" <<CF
# hh-harness-layer/1 — per (harness × task image) layer (AR-18/ADR-0436)
FROM $dig
RUN apt-get update && apt-get install -y --no-install-recommends curl git ca-certificates
COPY bundle/ /opt/hh-harness/
RUN printf '%s\n' '$ver' > /opt/hh-harness/$arm-$ver/.hh-version
RUN mkdir -p "\$(dirname '/opt/hh-harness-link')" && ln -sfn '/opt/hh-harness/$arm-$ver' '/opt/hh-harness-link'
CF
    out="$lctx/build-$t"; mkdir -p "$out"
    if "$W1" image-build --kind layer --containerfile "$cf" --context "$lctx" \
        --tag "hh-admit/$arm-$ver-$t" --out "$out" >/dev/null 2>"$out/err.log"; then
      iid="$(cat "$out/iidfile" 2>/dev/null || true)"
      emit "layer-$arm-$t" "built_mint" "{\"base\":\"$dig\",\"iid\":\"$iid\",\"tag\":\"hh-admit/$arm-$ver-$t\"}"
      python3 - "$LAYERS" "$arm" "$t" "$iid" <<'PY'
import json,sys
lj=json.load(open(sys.argv[1])); lj.setdefault(sys.argv[2],{})[sys.argv[3]]=sys.argv[4]
json.dump(lj,open(sys.argv[1],"w"),indent=1,sort_keys=True)
PY
      echo "built layer-$arm-$t $iid"
    else
      emit "layer-$arm-$t" "machinery{layer_build_failed}" "{\"base\":\"$dig\"}"
      echo "BUILD FAIL layer-$arm-$t" >&2
    fi
  done
done
echo "ws1-layers-build: done"
