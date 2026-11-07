#!/bin/bash
# Mid-batch buildx-cache watchdog: the O-side batch accumulates buildx
# cache (~18GB/batch observed); tagged images are preserved so the
# H-side still reuses the O-side build verbatim. Environment hygiene
# only — no task is retried.
export DOCKER_HOST=unix:///tmp/podman-api/podman.sock
LOG=/tmp/parity-t3-diskwatch.log
while true; do
  FREE=$(podman machine ssh podman -- "df -BG /var | tail -1 | awk '{print \$4}'" 2>/dev/null | tr -d 'G ')
  if [ -n "$FREE" ] && [ "$FREE" -lt 8 ]; then
    echo "[$(date +%H:%M:%S)] free=${FREE}G < 8G -> buildx prune" >> "$LOG"
    docker buildx prune -af >> "$LOG" 2>&1
    podman machine ssh podman -- df -h /var 2>/dev/null | tail -1 >> "$LOG"
  fi
  # stop when pipeline is done
  pgrep -f parity-t3-pipeline.sh >/dev/null || exit 0
  sleep 45
done
