#!/usr/bin/env bash
# check-hf-upload.sh — the deterministic fake-shim battery for the
# §4.10 upload kit (manifest 276/R6.23; REQ-XH-102/103/104/107/023;
# AR-20/ADR-0437). A fake `hf` on PATH records argv and serves canned
# downloads; a fixture harvest dir carries a real sha256 manifest; the
# operator env file carries fixture secrets (never real). Fully offline —
# no HF call, no credential. A pass is fixture-verified, never live.
set -euo pipefail
cd "$(dirname "$0")/.."
REPO="$(pwd)"

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
BIN="$T/bin"; HARV="$T/harvest"; REPORTS="$T/reports"
ENV="$T/operator.env"; LOG="$T/argv.log"
mkdir -p "$BIN" "$REPORTS" "$HARV"
export HH_HF_BIN="$BIN/hf" HH_ARCHIVE_REPORTS_DIR="$REPORTS"
export HH_OPERATOR_ENV_FILE="$ENV" HH_SHIM_LOG="$LOG"
: > "$LOG"
printf 'HF_WRITE_TOKEN=fixture-hf-write-token-0000\nOLLAMA_API_KEY=fix-ollama-9f8e\n' > "$ENV"
command -v python3 >/dev/null 2>&1 || { echo "python3 required"; exit 7; }
sha256sum() { # portable: coreutils sha256sum, else shasum -a 256
  if type -P sha256sum.bin >/dev/null 2>&1; then sha256sum.bin "$@"
  elif [ -x /usr/bin/sha256sum ]; then /usr/bin/sha256sum "$@"
  else shasum -a 256 "$@"; fi
}
SCAN_BIN="$REPO/target/debug/hh-secret-scan"
[ -x "$SCAN_BIN" ] || (cd "$REPO" && cargo build -q -p hharness-secrets --bins)
export HH_SECRET_SCAN="$SCAN_BIN"

PASS=0; FAILS=0
ok()   { PASS=$((PASS + 1)); }
bad()  { FAILS=$((FAILS + 1)); printf 'check-hf-upload: FAIL — %s\n' "$1" >&2; }
expect_rc() { # $1 want $2 label; shift 2; cmd…
  local want="$1" label="$2"; shift 2
  set +e; "$@" >"$T/out" 2>"$T/err"; local rc=$?; set -e
  if [ "$rc" -ne "$want" ]; then bad "$label — rc $rc (want $want): $(cat "$T/err")"; else ok; fi
}

# ── fake hf ───────────────────────────────────────────────────────────
cat > "$BIN/hf" <<'SHIM'
#!/usr/bin/env bash
printf 'hf' >> "$HH_SHIM_LOG"
for a in "$@"; do printf ' [%s]' "$a" >> "$HH_SHIM_LOG"; done
printf '\n' >> "$HH_SHIM_LOG"
[ -n "${HF_TOKEN:-}" ] && printf 'HF_TOKEN env: present\n' >> "$HH_SHIM_LOG"
case "$1" in
  upload) exit "${HH_SHIM_UPLOAD_RC:-0}" ;;
  download)
    dest=""; prev=""
    for a in "$@"; do [ "$prev" = "--local-dir" ] && dest="$a"; prev="$a"; done
    [ -f "$HH_SHIM_DL_FAIL" ] && exit 1
    mkdir -p "$dest/$(dirname "$3")"
    printf '%s' "$HH_SHIM_DL_BYTES" > "$dest/$3"
    exit 0 ;;
  auth|login) exit 2 ;;
esac
exit 0
SHIM
chmod +x "$BIN/hf"
export HH_SHIM_DL_FAIL="$T/dl_fail" HH_SHIM_DL_BYTES=""

# ── fixture harvest dir: two files + a real sha256 manifest ────────────
printf 'receipt-bytes-a' > "$HARV/f1.bin"; printf 'receipt-bytes-b' > "$HARV/f2.bin"
( cd "$HARV" && sha256sum f1.bin f2.bin > MANIFEST.sha256 )
SHA_F1="$(sha256sum "$HARV/f1.bin" | cut -d' ' -f1)"
SHA_F2="$(sha256sum "$HARV/f2.bin" | cut -d' ' -f1)"

mkdecl() { # $1 out.json $2 visibility $3 extra-members-json
  python3 -I - "$1" "$2" "$3" "$HARV" "$T" <<'PY'
import json, sys
out, vis, extra, harv, t = sys.argv[1:6]
d = {"schema": "hh-archive-upload-decl/1", "phase": "p1", "window": "W1",
     "ticket": "R6.23",
     "dataset": {"repo": "hh/receipts", "revision": "r1", "visibility": vis},
     "harvest": {"manifest": harv + "/MANIFEST.sha256", "root": harv},
     "files": [
       {"src": harv + "/f1.bin", "path": "w1/f1.bin", "role": "raw_receipts"},
       {"src": harv + "/f2.bin", "path": "w1/f2.bin", "role": "raw_receipts"}]}
if extra:
    d.update(json.loads(extra))
json.dump(d, open(out, "w"))
PY
}

mkindex() { # $1 out.json — receipt-index over the two harvest files
  python3 -I - "$1" "$SHA_F1" "$SHA_F2" <<'PY'
import json, sys
idx = {"schema": "hh-receipt-index/1",
       "dataset": {"repo": "hh/receipts", "revision": "r1", "visibility": "private"},
       "attempts": [{"slot_id": "s1", "attempt": 1, "files": [
         {"role": "raw_receipts", "path": "w1/f1.bin", "sha256": sys.argv[2], "bytes": 15},
         {"role": "raw_receipts", "path": "w1/f2.bin", "sha256": sys.argv[3], "bytes": 15}]}],
       "artifacts": [], "scans": {}}
json.dump(idx, open(sys.argv[1], "w"))
PY
}

DECL="$T/decl.json"; mkdecl "$DECL" private ''
IDX="$T/index.json"; mkindex "$IDX"

# ── upload: happy path — argv pinned, token env-ridden, receipt clean ───
expect_rc 0 "upload succeeds" bash scripts/hf-upload.sh upload "$DECL"
grep -q 'hf \[upload\] \[hh/receipts\] \[.*/f1.bin\] \[w1/f1.bin\] \[--repo-type\] \[dataset\]' "$LOG" \
  && ok || bad "upload argv not pinned"
grep -q 'HF_TOKEN env: present' "$LOG" && ok || bad "token not passed per invocation"
RCPT="$(ls "$REPORTS"/*-upload.json 2>/dev/null | tail -1)"
[ -n "$RCPT" ] || bad "no upload receipt"
python3 -I - "$RCPT" <<'PY' && ok || bad "receipt shape"
import json, sys
r = json.load(open(sys.argv[1]))
assert r["schema"] == "hh-archive-upload/1" and r["kind"] == "upload"
assert r["tool"]["name"] == "hf" and r["tool"]["version"] == "2.4.0"
assert r["tool"]["admission"] == "ADR-0437"
assert r["provided"]["HF_WRITE_TOKEN"] == "yes"
assert r["dataset"]["visibility"] == "private"
assert len(r["files"]) == 2
PY
# the receipt is leak-clean by construction (write refused on hit); the
# logged argv never carries a token value.
grep -q 'fixture-hf-write-token-0000' "$LOG" "$RCPT" 2>/dev/null \
  && bad "secret bytes in log/receipt" || ok

# ── refusals ───────────────────────────────────────────────────────────
expect_rc 2 "login invocation refuses" bash scripts/hf-upload.sh login
grep -q 'Refused{token.login_forbidden}' "$T/err" && ok || bad "login refusal spelling"

PUB="$T/pub.json"; mkdecl "$PUB" public ''
expect_rc 2 "non-private dataset refuses" bash scripts/hf-upload.sh upload "$PUB"
grep -q 'Refused{dataset.not_private}' "$T/err" && ok || bad "private-only refusal"

# unverified harvest manifest refuses — f1's bytes drift from the
# recorded manifest (the recorded sha no longer matches).
cp "$HARV/MANIFEST.sha256" "$T/man.good"
printf 'receipt-bytes-a-MODIFIED' > "$HARV/f1.bin"
expect_rc 2 "unverified harvest refuses" bash scripts/hf-upload.sh upload "$DECL"
grep -q 'Refused{harvest.unverified' "$T/err" && ok || bad "harvest refusal"
cp "$T/man.good" "$HARV/MANIFEST.sha256"
printf 'receipt-bytes-a' > "$HARV/f1.bin"

# env file absent refuses.
( unset HH_OPERATOR_ENV_FILE; set +e; HH_OPERATOR_ENV_FILE="$T/nope.env" \
    bash scripts/hf-upload.sh upload "$DECL" >"$T/out" 2>"$T/err"; echo $? > "$T/rc" )
[ "$(cat "$T/rc")" -eq 2 ] && grep -q 'env_file.absent' "$T/err" \
  && ok || bad "env-file refusal"

# a seeded live credential refuses the file and stops the row.
printf 'seed hf_fixturelivetoken0000000000 tail' > "$HARV/secret.bin"
printf 'HF_WRITE_TOKEN=hf_fixturelivetoken0000000000\n' >> "$ENV"
SECD="$T/secd.json"; mkdecl "$SECD" private ''
python3 -I - "$SECD" "$HARV" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
d["files"] = [{"src": sys.argv[2] + "/secret.bin", "path": "w1/secret.bin",
               "role": "raw_receipts"}]
json.dump(d, open(sys.argv[1], "w"))
PY
expect_rc 2 "live-credential file refuses" bash scripts/hf-upload.sh upload "$SECD"
grep -q 'Refused{live_credential' "$T/err" && ok || bad "live-credential refusal"
sed -i.bak '/HF_WRITE_TOKEN=hf_fixturelivetoken0000000000/d' "$ENV"

# tier-2: an unrevoked slot token refuses; a revoked one stays in raw.
printf 'SLOT_TOKEN_s1=slot-tok-fixture-aaa\nSLOT_TOKEN_s2=slot-tok-fixture-bbb\n' > "$T/tokens.env"
printf 'has slot-tok-fixture-aaa here' > "$HARV/tok.bin"
TOKD="$T/tokd.json"; mkdecl "$TOKD" private ''
python3 -I - "$TOKD" "$HARV" "$T" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
d["files"] = [{"src": sys.argv[2] + "/tok.bin", "path": "w1/tok.bin",
               "role": "raw_receipts"}]
d["slot_token_env"] = sys.argv[3] + "/tokens.env"
d["slot_token_revoked"] = "SLOT_TOKEN_s2"
json.dump(d, open(sys.argv[1], "w"))
PY
expect_rc 2 "unrevoked slot token refuses" bash scripts/hf-upload.sh upload "$TOKD"
grep -q 'Refused{unrevoked_slot_token' "$T/err" && ok || bad "unrevoked-token refusal"
printf 'has slot-tok-fixture-bbb here' > "$HARV/tok.bin"
expect_rc 0 "revoked slot token stays in raw" bash scripts/hf-upload.sh upload "$TOKD"
# …but in a derived role it was not tombstoned → refuse.
python3 -I - "$TOKD" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
d["files"][0]["role"] = "transcript"
json.dump(d, open(sys.argv[1], "w"))
PY
expect_rc 2 "derived-role token refuses" bash scripts/hf-upload.sh upload "$TOKD"
grep -q 'Refused{tombstone_missing' "$T/err" && ok || bad "tombstone refusal"

# ── verify: archive_verified / sha_mismatch / readback_unavailable ─────
python3 -I - "$DECL" "$IDX" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
d["index"] = sys.argv[2]
json.dump(d, open(sys.argv[1], "w"))
PY
HH_SHIM_DL_BYTES=""
python3 -I - "$HARV" <<'PY'
import sys
open(sys.argv[1] + "/dlb", "w").write("")
PY
# make the shim serve the real bytes for f1/f2:
cat > "$BIN/hf" <<'SHIM'
#!/usr/bin/env bash
printf 'hf' >> "$HH_SHIM_LOG"
for a in "$@"; do printf ' [%s]' "$a" >> "$HH_SHIM_LOG"; done
printf '\n' >> "$HH_SHIM_LOG"
case "$1" in
  upload) exit 0 ;;
  download)
    dest=""; prev=""
    for a in "$@"; do [ "$prev" = "--local-dir" ] && dest="$a"; prev="$a"; done
    [ -f "$HH_SHIM_DL_FAIL" ] && exit 1
    mkdir -p "$dest/$(dirname "$3")"
    if [ "$3" = "w1/f2.bin" ] && [ -n "$HH_SHIM_DL_BYTES" ]; then
      printf '%s' "$HH_SHIM_DL_BYTES" > "$dest/$3"
    else
      cp "$HH_SHIM_HARV/$(basename "$3")" "$dest/$3"
    fi
    exit 0 ;;
esac
exit 0
SHIM
chmod +x "$BIN/hf"
export HH_SHIM_HARV="$HARV"
expect_rc 0 "read-back verifies" bash scripts/hf-upload.sh verify "$DECL"
grep -q 'archive_verified' "$T/out" && ok || bad "archive_verified spelling"
VRCPT="$(ls "$REPORTS"/*-verify.json 2>/dev/null | tail -1)"
grep -q '"verification": "archive_verified"' "$VRCPT" && ok || bad "verify receipt"

export HH_SHIM_DL_BYTES="tampered-bytes"
expect_rc 2 "sha mismatch fails" bash scripts/hf-upload.sh verify "$DECL"
grep -q 'Refused{sha_mismatch:w1/f2.bin}' "$T/err" && ok || bad "mismatch refusal"
export HH_SHIM_DL_BYTES=""

touch "$HH_SHIM_DL_FAIL"
expect_rc 6 "unavailable read-back inconclusive" bash scripts/hf-upload.sh verify "$DECL"
grep -q 'inconclusive{readback_unavailable}' "$T/err" && ok || bad "inconclusive spelling"
rm -f "$HH_SHIM_DL_FAIL"

# ── remediate: rotation → purge → tombstone, in order, no secret ──────
RD="$T/rem.json"
python3 -I - "$RD" <<'PY'
import json, sys
d = {"schema": "hh-archive-upload-decl/1", "phase": "p1", "window": "W1",
     "ticket": "R6.23",
     "dataset": {"repo": "hh/receipts", "revision": "r1", "visibility": "private"},
     "harvest": {"manifest": "/dev/null", "root": "/dev/null"},
     "index": "/dev/null", "files": [],
     "rotated_provided": True,
     "purge": {"note": "operator purge of r1", "revision": "r1"},
     "tombstone": {"reason": "post-upload secret in w1/f1.bin",
                   "refs": ["receipt-index/w1-r1.json"]}}
json.dump(d, open(sys.argv[1], "w"))
PY
expect_rc 0 "remediate runs" bash scripts/hf-upload.sh remediate "$RD"
R1="$(ls "$REPORTS"/*-remediate-rotation.json | tail -1)"
R2="$(ls "$REPORTS"/*-remediate-purge.json | tail -1)"
R3="$(ls "$REPORTS"/*-remediate-tombstone.json | tail -1)"
for f in "$R1" "$R2" "$R3"; do
  [ -f "$f" ] || bad "missing remediation record $f"
  grep -q 'fixture-hf-write-token-0000' "$f" && bad "secret in $f"
done
ok
grep -q '"credential_rotated": "yes"' "$R1" && ok || bad "rotation provided"
grep -q '"dataset_revision_purge"' "$R2" && ok || bad "purge record"
grep -q 'post-upload secret in w1/f1.bin' "$R3" && ok || bad "tombstone reason"

printf 'check-hf-upload: %d/%d legs green\n' "$PASS" "$((PASS + FAILS))"
[ "$FAILS" -eq 0 ]
