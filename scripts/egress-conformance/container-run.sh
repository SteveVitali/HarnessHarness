#!/usr/bin/env bash
# container-run.sh — the in-container conformance orchestrator (R6.45k).
# Starts the subject hh-egress-mediator exactly as `hh-w1 h-run` starts
# it (same argv shape; the policy file is the rendered phase layer,
# re-read per connection — a phase swap is a file rewrite, exactly as
# DriveCell's policy_dst write), bridges it with hh-cell-relay on
# 127.0.0.1:3128, exports the closed_env proxy hints verbatim
# (crates/hh-runner/src/render.rs), runs the extracted shapes, and
# lands /out/verdict.json.
set -uo pipefail

OUT="${OUT:-/out}"
SHAPES="${SHAPES:-s1 s2 s3 s4 s5}"
SUBJECT_REF="${SUBJECT_REF:-unknown}"
FREEZE_EPOCH="${FREEZE_EPOCH:-0}"
FREEZE_DIGEST="${FREEZE_DIGEST:-unknown}"
DERIVED=/opt/hh/derived
POL=/opt/hh/policies
mkdir -p "$OUT"
STEPS_JSONL="$OUT/steps.jsonl"; : > "$STEPS_JSONL"
: > "$OUT/client.log"
: > "$OUT/probes.jsonl"

say() { printf 'conformance: %s\n' "$*"; }

# ── the mediator + relay (the h-run topology, single container) ────────
mkdir -p /run/hh
cp "$POL/verify.json" /run/hh/policy.json
/opt/hh/bin/hh-egress-mediator \
  --store-root /run/hh/store \
  --socket /run/hh/egress.sock \
  --policy /run/hh/policy.json \
  --env-handle env-conform \
  --effect-id eff-conform \
  --participant oracle \
  --run-id conform-1 \
  > "$OUT/mediator.log" 2> "$OUT/mediator.err" &
MED_PID=$!
deadline=$((SECONDS + 10))
while [ ! -S /run/hh/egress.sock ]; do
  [ $SECONDS -lt $deadline ] || { say "mediator never bound egress.sock"; exit 2; }
  kill -0 $MED_PID 2>/dev/null || { say "mediator died at boot"; cat "$OUT/mediator.err"; exit 2; }
  sleep 0.05
done
/opt/hh/bin/hh-cell-relay --pair egress=3128=/run/hh/egress.sock \
  > "$OUT/relay.log" 2>&1 &
RELAY_PID=$!
sleep 0.3
kill -0 $RELAY_PID 2>/dev/null || { say "relay died at boot"; cat "$OUT/relay.log"; exit 2; }

# ── the closed_env hints, exactly as render.rs sets them ───────────────
export http_proxy="http://127.0.0.1:3128"
export https_proxy="http://127.0.0.1:3128"
export HTTP_PROXY="http://127.0.0.1:3128"
export HTTPS_PROXY="http://127.0.0.1:3128"
export no_proxy="127.0.0.1,localhost"
export NO_PROXY="127.0.0.1,localhost"
export HOME=/root USER=root
# uv installs into $HOME/.local/bin — on PATH ahead of the shape lines
# (each step is its own `bash -c`, matching how a fresh exec sources it).
export PATH="$HOME/.local/bin:$PATH"
# Declared noble-compat switch: the verbatim `pip install` lines (some
# written for the members' own images, not externally-managed pip) run
# under the same permissive semantics `--break-system-packages` names.
export PIP_BREAK_SYSTEM_PACKAGES=1
cd /app

# ── peak-concurrency sampler (the D8 proof): /proc/net/tcp{,6} entries
# in state 01 (ESTABLISHED) on local :3128, every 100 ms ───────────────
python3 - "$OUT/peak_conns.txt" <<'PY' &
import sys, time
peak, outp = 0, sys.argv[1]
while True:
    n = 0
    for p in ("/proc/net/tcp", "/proc/net/tcp6"):
        try:
            for line in open(p).read().splitlines()[1:]:
                f = line.split()
                if len(f) > 3 and f[1].endswith(":0C78") and f[3] == "01":
                    n += 1
        except FileNotFoundError:
            pass
    if n > peak:
        peak = n
        with open(outp, "w") as f:
            f.write(str(peak))
    time.sleep(0.1)
PY
SAMPLER_PID=$!

record_step() { # $1 shape $2 cmd $3 wall_ms $4 rc $5 expect
  python3 - "$STEPS_JSONL" "$1" "$2" "$3" "$4" "$5" <<'PY'
import json, sys
path, shape, cmd, wall, rc, expect = sys.argv[1:6]
with open(path, "a") as f:
    f.write(json.dumps({"shape": shape, "cmd": cmd, "wall_ms": int(wall),
                        "rc": int(rc), "expect": expect}) + "\n")
PY
}

run_line() { # $1 shape $2 line $3 expect(ok|deny)
  local shape="$1" line="$2" expect="${3:-ok}" t0 t1 rc
  say "[$shape] $line"
  t0=$(python3 -c 'import time;print(int(time.time()*1000))')
  timeout 900 bash -c "$line" >> "$OUT/client.log" 2>&1
  rc=$?
  t1=$(python3 -c 'import time;print(int(time.time()*1000))')
  record_step "$shape" "$line" "$((t1 - t0))" "$rc" "$expect"
  return $rc
}

shape_on() { case " $SHAPES " in *" $1 "*) return 0;; *) return 1;; esac; }

read_list() { grep -v '^[[:space:]]*$' "$DERIVED/$1" 2>/dev/null || true; }

# pytest-leg substitution (the contract's verbatim-with-substitution
# rule): the `pytest --ctrf …` / `python -m pytest --ctrf …` tails run
# as `--version` legs.
sub_pytest() { sed 's/pytest --ctrf.*$/pytest --version/; s/python -m pytest --ctrf.*$/python -m pytest --version/'; }

rc_overall=0

# ── verify-phase shapes ───────────────────────────────────────────────
cp "$POL/verify.json" /run/hh/policy.json

if shape_on s1; then
  # S1 — the stock bootstrap + every unique non-pytorch uvx variant.
  run_line s1 "apt-get update" || rc_overall=1
  run_line s1 "apt-get install -y curl" || rc_overall=1
  run_line s1 "curl -LsSf https://astral.sh/uv/0.9.5/install.sh | sh" || rc_overall=1
  run_line s1 'source $HOME/.local/bin/env' || rc_overall=1
  while IFS= read -r line; do
    case "$line" in *download.pytorch.org*) continue;; esac
    run_line s1 "$(printf '%s' "$line" | sub_pytest)" || rc_overall=1
  done < <(read_list uvx.txt)
fi

if shape_on s2; then
  # S2 — the pytorch-index shape (verbatim). Under a freeze whose verify
  # layer lacks download.pytorch.org the expectation is a typed deny.
  line="$(grep 'download.pytorch.org' "$DERIVED/uvx.txt" | head -1)"
  if [ -z "$line" ]; then
    say "[s2] no pytorch-index line extracted — inventory drift"
    record_step s2 "<extract>" 0 2 drift
    rc_overall=1
  elif grep -q 'download\.pytorch\.org' "$POL/verify.json"; then
    run_line s2 "$(printf '%s' "$line" | sub_pytest)" ok || rc_overall=1
  else
    run_line s2 "$(printf '%s' "$line" | sub_pytest)" deny || true
  fi
fi

if shape_on s3; then
  # S3 — the pip shapes (verbatim pip install lines; PIP_BREAK_SYSTEM_PACKAGES
  # is the declared noble-compat switch), then a pytest --version leg.
  run_line s3 "apt-get install -y python3-pip" || rc_overall=1
  while IFS= read -r line; do
    run_line s3 "$line" || rc_overall=1
  done < <(read_list pip.txt)
  run_line s3 "python -m pytest --version" || rc_overall=1
fi

if shape_on s4; then
  # S4 — the heavier apt sets (every unique apt-get install line).
  run_line s4 "apt-get update" || rc_overall=1
  while IFS= read -r line; do
    run_line s4 "$line" || rc_overall=1
  done < <(read_list apt-install.txt)
fi

# ── S5 — the agent-phase shape (the Q-X55 posture: pip/npm/apt under
# the agent allowlist, no github.com) ──────────────────────────────────
if shape_on s5; then
  cp "$POL/agent.json" /run/hh/policy.json
  run_line s5 "apt-get update" || rc_overall=1
  run_line s5 "apt-get install -y python3-pip npm" || rc_overall=1
  run_line s5 "pip install --break-system-packages requests==2.32.5" || rc_overall=1
  run_line s5 "cd /tmp && npm install --no-save is-number@7.0.0" || rc_overall=1
  # the named-deny probe: github.com is never in the agent list (Q-X55).
  run_line s5 "curl -sS -o /dev/null -w 'github-deny:%{http_code}\n' --max-time 30 https://github.com/" deny || true
  cp "$POL/verify.json" /run/hh/policy.json
fi

# ── direct proxy probes — the verbatim status every reply emits ───────
python3 - "$OUT/probes.jsonl" <<'PY' || rc_overall=1
import json, socket, sys
out = open(sys.argv[1], "a")
def status_line(host, port, request):
    rec = {"target": f"{host}:{port}", "request": request.split("\r\n")[0]}
    try:
        s = socket.create_connection(("127.0.0.1", 3128), timeout=30)
        s.settimeout(30)
        s.sendall(request.encode())
        buf = b""
        while b"\r\n" not in buf and len(buf) < 8192:
            b = s.recv(4096)
            if not b:
                break
            buf += b
        line = buf.split(b"\r\n", 1)[0].decode("latin-1")
        rec["status_line"] = line
        parts = line.split()
        if len(parts) >= 2 and parts[1].isdigit():
            rec["status"] = int(parts[1])
        if rec.get("status") == 200 and request.startswith("CONNECT"):
            # confirm the tunnel actually carries bytes — not just a 200
            s.sendall(b"GET / HTTP/1.0\r\nHost: %b\r\n\r\n" % host.encode())
            data = s.recv(64)
            rec["tunneled_bytes"] = len(data)
        s.close()
    except OSError as e:
        rec["status"] = None; rec["error"] = f"{type(e).__name__}: {e}"
    out.write(json.dumps(rec) + "\n"); out.flush()

status_line("pypi.org", 443, "CONNECT pypi.org:443 HTTP/1.1\r\nHost: pypi.org:443\r\n\r\n")
status_line("archive.ubuntu.com", 80,
            "GET http://archive.ubuntu.com/ HTTP/1.1\r\nHost: archive.ubuntu.com\r\nConnection: close\r\n\r\n")
status_line("github.com", 443, "CONNECT github.com:443 HTTP/1.1\r\nHost: github.com:443\r\n\r\n")
PY

sleep 0.3
kill $SAMPLER_PID 2>/dev/null; wait $SAMPLER_PID 2>/dev/null
kill $RELAY_PID $MED_PID 2>/dev/null; wait $MED_PID 2>/dev/null

# ── assertions → verdict.json ─────────────────────────────────────────
PEAK="$(cat "$OUT/peak_conns.txt" 2>/dev/null || echo 0)"
python3 /opt/hh/walcheck.py \
  --store /run/hh/store --run conform-1 \
  --steps "$STEPS_JSONL" --probes "$OUT/probes.jsonl" \
  --mediator-err "$OUT/mediator.err" --peak "$PEAK" \
  --subject "$SUBJECT_REF" --epoch "$FREEZE_EPOCH" --digest "$FREEZE_DIGEST" \
  --verify-policy "$POL/verify.json" --agent-policy "$POL/agent.json" \
  --out "$OUT/verdict.json"
rc=$?
say "verdict rc=$rc → $OUT/verdict.json"
exit $rc
