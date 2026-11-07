#!/usr/bin/env bash
# startup.sh — in-guest bootstrap for a hh compute window VM
# (manifest 266/R6.13; spec §3.9; ADR-0431/AR-15; REQ-XB-204/209/220).
#
# Passed to `instances create` as `--metadata-from-file startup-script=`.
# NO SECRETS: this file is recorded by sha256 in every provision receipt and
# leak-swept before use. The only inputs are instance metadata keys set by
# gce-backend.sh: `hh_packages` (name=version pins) and
# `hh_deadman_minutes` (D + 15 — REQ-XB-209's in-guest timer).
set -euo pipefail
set +x # belt: tracing never exposes metadata contents

META="http://metadata.google.internal/computeMetadata/v1/instance/attributes"
mh() { curl -fs -H "Metadata-Flavor: Google" "$META/$1"; }

DEADMAN_MIN="$(mh hh_deadman_minutes)"   # minutes until in-guest poweroff
PACKAGES="$(mh hh_packages)"            # whitespace-sep name=version pins

# ── REQ-XB-220: disable unattended upgrades for the window — the package
# set must not move mid-run (a podman restart kills in-flight cells).
systemctl disable --now unattended-upgrades.service 2>/dev/null || true
systemctl disable --now apt-daily.timer apt-daily-upgrade.timer 2>/dev/null || true
systemctl mask apt-daily.service apt-daily-upgrade.service 2>/dev/null || true

# ── pinned package set (podman, crun, conmon, netavark, compose frontend)
export DEBIAN_FRONTEND=noninteractive
apt-get update -y
apt-get install -y --allow-downgrades $PACKAGES
mkdir -p /var/log/hh
dpkg-query -W -f='${Package}=${Version}\n' \
  | sort > /var/log/hh/dpkg-pins.txt

# ── rootful podman socket, group-restricted (never TCP-bound, never
# forwarded — REQ-XB-205). The `hh` group owns the local socket.
groupadd -f hh
mkdir -p /etc/systemd/system/podman.socket.d
cat > /etc/systemd/system/podman.socket.d/10-hh.conf <<'UNIT'
[Socket]
SocketMode=0660
SocketGroup=hh
UNIT
systemctl daemon-reload
systemctl enable --now podman.socket

# ── REQ-XB-209: in-guest dead-man timer at D + 15 min — the second physical
# layer. Runs even if every controller session is lost.
shutdown -h "+${DEADMAN_MIN}" || \
  ( sleep "$((DEADMAN_MIN * 60))" && systemctl poweroff ) &

# ── secrets tmpfs — deliver-secret writes /run/hh/secrets/NAME (0400, owner
# root:hh) at use, over ssh stdin. Nothing arrives via metadata or argv.
mkdir -p /run/hh/secrets
chmod 0750 /run/hh /run/hh/secrets
chgrp -R hh /run/hh

echo "hh-window-startup: done (dead-man ${DEADMAN_MIN}m armed)"
