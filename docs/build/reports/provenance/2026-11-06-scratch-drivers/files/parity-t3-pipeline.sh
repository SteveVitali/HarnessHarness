#!/bin/bash
# PARITY-TRANCHE-3 batch pipeline — O-side (tb, n-concurrent 1) ->
# H-side (podman, ANSI-fixed parser) -> prune -> next batch.
# One first attempt per task per runner; failures recorded, never retried.
export DOCKER_HOST=unix:///tmp/podman-api/podman.sock
HPY="$HOME/.local/share/uv/tools/terminal-bench/bin/python"
OUT=~/tb-work/parity-runs/o-side-3
LOG=/tmp/parity-t3-pipeline.log
say(){ echo "[$(date +%H:%M:%S)] $*" >> "$LOG"; }

wait_pid(){ while kill -0 "$1" 2>/dev/null; do sleep 20; done; }

run_obatch(){ # $1 = batch num; returns when tb exits
  local b=$1
  local args=()
  while read -r t; do [ -n "$t" ] && args+=(-t "$t"); done < "/tmp/parity-t3-b${b}.txt"
  say "O-side batch $b starting"
  tb run --dataset-path ~/tb-work/dataset "${args[@]}" -a oracle \
    --output-path "$OUT" --n-concurrent 1 --no-cleanup \
    --no-livestream --no-upload-results >> "/tmp/parity-o3-b${b}.log" 2>&1
  say "O-side batch $b done rc=$?"
}
run_hbatch(){
  local b=$1
  say "H-side batch $b starting"
  "$HPY" /tmp/parity-h-side.py t3 "/tmp/parity-t3-b${b}.txt" \
    >> "/tmp/parity-h3-b${b}.log" 2>&1
  say "H-side batch $b done rc=$?"
}
prune(){
  say "prune starting"
  podman system prune -af --volumes >> "$LOG" 2>&1
  docker buildx prune -af >> "$LOG" 2>&1
  podman machine ssh podman -- df -h /var 2>/dev/null | tail -1 >> "$LOG"
  say "prune done"
}

# batch 1 O-side already running under pid from the launching shell —
# detect it via tb.lock/process instead of trusting a fixed pid.
say "pipeline start; waiting for in-flight O-side batch 1"
while pgrep -f "tb run --dataset-path.*parity-runs/o-side-3" >/dev/null; do sleep 20; done
say "in-flight O-side batch 1 finished"

for b in 1 2 3 4 5; do
  if [ "$b" -gt 1 ]; then run_obatch "$b"; fi
  run_hbatch "$b"
  prune
done
say "PIPELINE COMPLETE"
