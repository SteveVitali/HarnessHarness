#!/bin/sh
# hh-smoke/write-file verifier — reward 1 iff /app/hh_smoke.txt holds `ok`.
set -eu
mkdir -p /logs/verifier
if [ "$(cat /app/hh_smoke.txt 2>/dev/null || true)" = "ok" ]; then
  echo '{"reward": 1}' > /logs/verifier/reward.json
  echo "1" > /logs/verifier/reward.txt
else
  echo '{"reward": 0}' > /logs/verifier/reward.json
  echo "0" > /logs/verifier/reward.txt
fi
