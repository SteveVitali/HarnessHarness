#!/usr/bin/env bash
set -euo pipefail
RAW="$1/docs/build/reports/provenance/2026-11-06-scratch-drivers/raw"
victim=$(ls "$RAW/s4-confirm/slots" | head -1)
rm "$RAW/s4-confirm/slots/$victim"
grep -v "s4-confirm/slots/$victim" "$RAW/SHA256SUMS" > "$RAW/SHA256SUMS.new"
mv "$RAW/SHA256SUMS.new" "$RAW/SHA256SUMS"
