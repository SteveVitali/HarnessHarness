#!/usr/bin/env bash
# w4x-layers-build.sh — W4x PP0 layer mint (ticket 325d; ADR-0453): the
# smoke battery's per-(arm × task) layers through the same committed
# surface as Ws1 (`hh-w1 image-build` → admission_build::run_build,
# LayerSpec-rendered shape — `FROM` digest-pinned base, adapter
# system-deps replay, `COPY bundle/ <root>/`, `.hh-version` marker,
# `/opt/hh-harness-link` alias). Each mint records a `built_mint`
# hh-carried-load/1 row — never labeled carried.
#
# The task↔base-ref map is data (--tasks-file: `{"<task>":"<ref>"}`),
# the arms/version map likewise (--arms-file: `{"<arm>":"<ver>"}`) —
# nothing W4-specific is hardcoded, so Ws1's own invocations stay
# untouched.
#
# usage: w4x-layers-build.sh --exports DIR --w1-bin BIN --tasks-file F \
#          --arms-file F --loads F --layers-file F [--arms csv]
set -euo pipefail
set +x
EXPORTS=""; W1=""; TASKS_F=""; ARMS_F=""; LOADS=""; LAYERS=""; ARMS=""
while [ $# -gt 0 ]; do case "$1" in
  --exports) EXPORTS="$2"; shift 2;;
  --w1-bin) W1="$2"; shift 2;;
  --tasks-file) TASKS_F="$2"; shift 2;;
  --arms-file) ARMS_F="$2"; shift 2;;
  --loads) LOADS="$2"; shift 2;;
  --layers-file) LAYERS="$2"; shift 2;;
  --arms) ARMS="$2"; shift 2;;
  *) echo "unknown arg: $1" >&2; exit 2;;
esac; done
[ -n "$EXPORTS" ] && [ -n "$W1" ] && [ -n "$TASKS_F" ] && [ -n "$ARMS_F" ] \
  && [ -n "$LOADS" ] && [ -n "$LAYERS" ] || {
  echo "w4x-layers-build: --exports/--w1-bin/--tasks-file/--arms-file/--loads/--layers-file required" >&2; exit 2; }
[ -f "$LAYERS" ] || echo '{}' > "$LAYERS"

emit() { # $1 name $2 status $3 detail-json
  python3 - "$1" "layer_build" "$2" "$3" >>"$LOADS" <<'PY'
import json,sys,time
print(json.dumps({"schema":"hh-carried-load/1","name":sys.argv[1],
 "role":sys.argv[2],"result":sys.argv[3],
 "detail":json.loads(sys.argv[4]),
 "loaded_at":time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime())}))
PY
}

TASKS="$(python3 -c 'import json,sys;print(" ".join(json.load(open(sys.argv[1]))))' "$TASKS_F")"
[ -n "$ARMS" ] || ARMS="$(python3 -c 'import json,sys;print(" ".join(json.load(open(sys.argv[1]))))' "$ARMS_F")"

for t in $TASKS; do
  ref="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$TASKS_F" "$t")"
  podman pull "$ref" >/dev/null 2>&1 || sudo podman pull "$ref" >/dev/null \
    || { emit "layer-base-$t" "machinery{base_pull_failed}" "{\"ref\":\"$ref\"}"; echo "PULL FAIL $t" >&2; continue; }
  dig="$(podman image inspect "$ref" --format '{{index .RepoDigests 0}}' 2>/dev/null \
        || sudo podman image inspect "$ref" --format '{{index .RepoDigests 0}}')"
  case "$dig" in *@sha256:*) ;; *) emit "layer-base-$t" "machinery{base_digest_absent}" "{\"ref\":\"$ref\"}"; continue;; esac
  for arm in $ARMS; do
    ver="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$ARMS_F" "$arm")"
    lctx="$EXPORTS/artifacts/lctx-$arm"
    [ -d "$lctx/bundle" ] || { emit "layer-$arm-$t" "machinery{lctx_absent}" "{}"; continue; }
    cf="$lctx/Containerfile-$t"
    cat > "$cf" <<CF
# hh-harness-layer/1 — per (harness × task image) layer (AR-18/ADR-0436)
FROM $dig
RUN apt-get update && apt-get install -y --no-install-recommends curl git ca-certificates bash procps
COPY bundle/ /opt/hh-harness/$arm-$ver/
RUN printf '%s\n' '$ver' > /opt/hh-harness/$arm-$ver/.hh-version
RUN mkdir -p "\$(dirname '/opt/hh-harness-link')" && ln -sfn '/opt/hh-harness/$arm-$ver' '/opt/hh-harness-link'
RUN mkdir -p /root/.nvm && : > /root/.nvm/nvm.sh && mkdir -p /app /logs/agent
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
echo "w4x-layers-build: done"
