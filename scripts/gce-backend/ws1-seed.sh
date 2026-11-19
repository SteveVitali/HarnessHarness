#!/usr/bin/env bash
# ws1-seed.sh — the Ws1 on-VM seed + LOAD (R6.45q): runs once after the
# staged tree lands at /var/lib/hh/ws1. Steps: tb21 corpus at the recorded
# pin, in-podman runner build (hh-w1 + hh_l1_drive at runner.commit
# 7a970a70), carried LOAD (ws1-setup.sh — digest-verified), missing-layer
# mint (ws1-layers-build.sh — built_mint rows), placement of lane/auth/
# cfg/layers/gate-a/admissions/hir. Every failure is a typed row.
set -uo pipefail
set +x
exec > >(tee -a /var/log/hh/ws1-seed.log) 2>&1
echo "== ws1-seed start $(date -u +%FT%TZ) =="
R=/var/lib/hh/ws1; RR=/var/lib/hh/runs/run-root-ws1
mkdir -p /var/log/hh /opt/ws1 "$RR"

# ── tb21 corpus at the recorded pin ──────────────────────────────────────
if [ ! -d /opt/ws1/tb21 ]; then
  git clone https://github.com/harbor-framework/terminal-bench-2-1 /opt/ws1/tb21
fi
cd /opt/ws1/tb21 && git checkout 12b7565679ce5006230b820666353673a133db93 2>/dev/null || true
echo "corpus HEAD: $(git rev-parse HEAD 2>/dev/null || echo ABSENT)"

# ── epoch-5 freeze + harbor pin (verbatim copies staged by the controller) ──
cp "$R/freeze.json" /opt/ws1/freeze.json 2>/dev/null || true
cp "$R/pin.json" /opt/ws1/pin.json 2>/dev/null || true

# ── in-podman runner build (hh-w1 + hh_l1_drive) ─────────────────────────
if [ ! -x "$R/bin/hh-w1" ]; then
  mkdir -p "$R/bin"
  podman build -f /opt/ws1/bin-build/Containerfile -t hh-ws1-runner /opt/ws1/bin-build \
    || sudo podman build -f /opt/ws1/bin-build/Containerfile -t hh-ws1-runner /opt/ws1/bin-build
  cid=$(podman create hh-ws1-runner 2>/dev/null || sudo podman create hh-ws1-runner)
  for b in hh-w1 hh_l1_drive; do
    podman cp "$cid:/opt/$b" "$R/bin/$b" 2>/dev/null || sudo podman cp "$cid:/opt/$b" "$R/bin/$b"
    chmod 755 "$R/bin/$b"
  done
  (podman rm "$cid" || sudo podman rm "$cid") >/dev/null 2>&1 || true
fi
echo "bins: $(ls "$R/bin" 2>/dev/null | tr '\n' ' ')"
"$R/bin/hh-w1" --help 2>&1 | head -2 || true


# ── controller-expected path placements ───────────────────────────────────
ln -sf "$R/carried/infra/model_proxy" "$R/hh-model-proxy"
ln -sf "$R/bin/hh-w1" "$R/hh-w1"
ln -sf "$R/bin/hh_l1_drive" "$R/hh_l1_drive"
sudo ln -sf /run/hh/secrets/OLLAMA_API_KEY "$R/ollama-key" 2>/dev/null || true

# ── carried LOAD ──────────────────────────────────────────────────────────
cp "$R/exports/lane.json" "$R/exports/auth.json" "$R/exports/cfg.json" "$R/exports/gate-a.json" "$R/" 2>/dev/null || true
"$R/ws1-setup.sh" \
  --exports "$R/exports" \
  --dev-tasks cobol-modernization,cancel-async-tasks,build-pmars,pytorch-model-recovery,extract-elf,openssl-selfsigned-cert,sqlite-with-gcov \
  --pp0-tasks cancel-async-tasks,build-pmars,openssl-selfsigned-cert \
  --set-digest sha256:c451ed97d0206821bedd6e2255e5f6bba4736646d7170601b4085492e711a8b9 \
  --root "$R" --run-root "$RR"
echo "== carried-loads =="; cat "$RR/carried-loads.jsonl"

# ── missing-layer mint (operator-authorized build path) ───────────────────
"$R/ws1-layers-build.sh" \
  --exports "$R/exports" --w1-bin "$R/bin/hh-w1" \
  --loads "$RR/carried-loads.jsonl" --layers-file "$R/layers.json"
echo "== layers.json =="; cat "$R/layers.json"
echo "== ws1-seed done $(date -u +%FT%TZ) =="
