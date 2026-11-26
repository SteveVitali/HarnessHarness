#!/usr/bin/env bash
# p4c-setup.sh — P4-C staging on hh-p4-p4c (R7.5, manifest 332,
# GATE-G36). Cloned from p4x-setup.sh (R7.3): tools (uv/git/rustup
# 1.94), tb21 corpus at the recorded pin, task-env pulls for the 37
# held-out band-A tasks (task.toml's declared docker_image ref;
# repo-digest pinned images.json), registries.conf, in-guest release
# build of the window binaries, p4_admissibility emit-docs + bind_hir
# bound arm docs (baseline + V1 ONLY — the frozen c-prereg member
# list verbatim), mswea pack, cfg/lane/auth/pricing placement.
# Run as root:  sudo bash /tmp/p4c-stage/p4c-setup.sh
set -euo pipefail
set +x

R=/var/lib/hh/p4c
RR=/var/lib/hh/runs/run-root-p4c
O=/opt/p4c
L=$R/loads.jsonl
SRC=/opt/p4c/src
mkdir -p "$R"/{bin,oci} "$O" "$RR"/jobs

emit() { # $1 name $2 role $3 status $4 detail-json
  python3 - "$1" "$2" "$3" "$4" >>"$L" <<'PY'
import json,sys,time
print(json.dumps({"schema":"hh-carried-load/1","name":sys.argv[1],
 "role":sys.argv[2],"result":sys.argv[3],
 "detail":json.loads(sys.argv[4]),
 "loaded_at":time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime())}))
PY
}
fail() { echo "P4C-SETUP FAIL: $*" >&2; exit 1; }

export PATH=/root/.local/bin:/root/.cargo/bin:$PATH

# ── tools ────────────────────────────────────────────────────────────
apt-get install -y git curl ca-certificates build-essential pkg-config musl-tools >/dev/null 2>&1 || true
command -v uv >/dev/null 2>&1 || \
  (curl -LsSf https://astral.sh/uv/install.sh | sh >/dev/null 2>&1)
command -v uv || fail "uv absent"
if ! command -v cargo >/dev/null 2>&1; then
  curl -LsSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.94.0 >/dev/null 2>&1
fi
export PATH=/root/.cargo/bin:$PATH
rustup toolchain install 1.94.0 >/dev/null 2>&1 || true
rustup target add x86_64-unknown-linux-musl --toolchain 1.94.0 >/dev/null 2>&1 || true
echo "== tools: cargo $(cargo --version 2>/dev/null | head -1)"

# ── source tree at the declared runner.commit ────────────────────────
if [ ! -d "$SRC/crates" ]; then
  mkdir -p "$SRC"
  tar -xzf /tmp/p4c-stage/src.tar.gz -C "$SRC"
fi
[ -f "$SRC/Cargo.toml" ] || fail "source tree absent at $SRC"
echo "== source staged at $SRC"

# ── tb21 corpus at the recorded pin ──────────────────────────────────
if [ ! -d "$O/tb21" ]; then
  git clone -q https://github.com/harbor-framework/terminal-bench-2-1 "$O/tb21"
fi
( cd "$O/tb21" && git checkout -q 12b7565679ce5006230b820666353673a133db93 )
echo "corpus HEAD: $(git -C "$O/tb21" rev-parse HEAD)"
# The frozen held-out cohort `c` — T=37, verbatim from the frozen
# hh-p4-c-prereg/1 (1d55d48a…); never run in X0/X1.
TASKS="prove-plus-comm fix-git overfull-hbox configure-git-webserver make-doom-for-mips model-extraction-relu-logits sparql-university torch-tensor-parallelism write-compressor password-recovery gpt2-codegolf fix-code-vulnerability build-cython-ext count-dataset-tokens multi-source-data-merger kv-store-grpc headless-terminal git-leak-recovery polyglot-c-py regex-log vulnerable-secret adaptive-rejection-sampler chess-best-move hf-model-inference raman-fitting sanitize-git-repo git-multibranch db-wal-recovery nginx-request-logging tune-mjcf query-optimize merge-diff-arc-agi-task log-summary-date-ranges sqlite-db-truncate pypi-server largest-eigenval gcode-to-text"
for t in $TASKS; do
  [ -f "$O/tb21/tasks/$t/instruction.md" ] || fail "task $t instruction absent"
  [ -f "$O/tb21/tasks/$t/task.toml" ] || fail "task $t task.toml absent"
done
echo "== held-out cohort: 37 task dirs present"

# ── registries ────────────────────────────────────────────────────────
mkdir -p /etc/containers
# only an uncommented declaration counts — Debian's shipped file carries
# the key inside a comment block, which a bare grep falsely satisfies.
grep -q '^unqualified-search-registries' /etc/containers/registries.conf 2>/dev/null || \
  printf 'unqualified-search-registries = ["docker.io"]\n' >> /etc/containers/registries.conf

# ── task-env pulls (declared task.toml docker_image refs) ────────────
python3 - "$O/env-digests.json" <<'PY' >/dev/null 2>&1 || true
import json,sys; json.dump({},open(sys.argv[1],"w"))
PY
for t in $TASKS; do
  ref="$(python3 - "$O/tb21/tasks/$t/task.toml" <<'PY'
import re,sys
s=open(sys.argv[1]).read()
m=re.search(r'docker_image\s*=\s*"([^"]+)"',s)
if not m:
    m=re.search(r'image\s*=\s*"([^"]+)"',s)
print(m.group(1) if m else "")
PY
)"
  [ -n "$ref" ] || fail "task $t docker_image ref absent"
  case "$ref" in *@sha256:*) ;; *) ref="$ref";; esac
  podman pull -q "$ref" >/dev/null 2>&1 || fail "env pull $t ($ref)"
  dig="$(podman image inspect "$ref" --format '{{index .RepoDigests 0}}' 2>/dev/null || true)"
  python3 - "$O/env-digests.json" "$t" "$ref" "$dig" <<'PY'
import json,sys
p=json.load(open(sys.argv[1])); p[sys.argv[2]]={"ref":sys.argv[3],"repo_digest":sys.argv[4]}
json.dump(p,open(sys.argv[1],"w"),indent=1,sort_keys=True)
PY
  emit "env-$t" "task_env" "loaded" "{\"ref\":\"$ref\",\"digest\":\"$dig\"}"
done
echo "== env images pulled"

# ── images.json — {task:{ref,image:<repo>@sha256:<digest>}} the driver reads
python3 - "$O/env-digests.json" "$O/images.json" <<'PY'
import json,sys
d=json.load(open(sys.argv[1])); out={}
for t,e in d.items():
    img=e.get("repo_digest") or ""
    if img and "@" not in img:
        repo=e["ref"].split(":",1)[0].split("@",1)[0]
        img=f"{repo}@{img}"
    out[t]={"ref":e["ref"],"image":img}
json.dump(out,open(sys.argv[2],"w"),indent=1,sort_keys=True)
PY
python3 -c "import json;d=json.load(open('$O/images.json'));print('images.json tasks:',len(d))"

# ── release build of the window binaries ─────────────────────────────
cd "$SRC"
cargo build --release -p hharness-runner --bin hh_w1 \
  --bin p4_admissibility 2>&1 | tail -3
cargo build --release -p hharness-embed --bin hh_l1_drive 2>&1 | tail -3
cargo build --release -p hharness-hosting --bin hh-model-proxy 2>&1 | tail -3
cargo build --release -p hharness-helper --bin hh-helper 2>&1 | tail -3
for b in hh_w1 hh_l1_drive hh-model-proxy hh-helper p4_admissibility; do
  [ -x "target/release/$b" ] || fail "binary $b absent"
  n="$(echo "$b" | tr _ -)"; cp "target/release/$b" "$R/bin/$n"
done
echo "== binaries: $(ls $R/bin)"

# ── the member docs: emit-docs + bind_hir ────────────────────────────
mkdir -p "$O/docs-raw" "$O/docs"
"$R/bin/p4-admissibility" emit-docs "$O/docs-raw" | tee "$O/emit-docs.log" \
  || fail "emit-docs"
# The gen-3 sealed baseline identity — the control_strategy
# submission contract minted at this tree (R7.3). The emit pins its
# own printed definition_ref; a stale binary refuses here, never a
# silently different H. `l1-pass:sha256:5b807c53…` binds this exact
# digest; V1 is a declared level of the same sealed doc family — no
# new H identity, no new G-L1 leg (ticket 332 step 1).
grep -q "baseline.json definition_ref=sha256:4142fb105a9abf67bf7c8e39ca2bd69b26dfc9ae9813ebd04244fd221af557b5" \
  "$O/emit-docs.log" || fail "emit-docs baseline identity (want 4142fb10…)"
python3 "$SRC/scripts/gce-backend/bind-hir.py" "$O/docs-raw" "$O/docs" \
  || fail "bind-hir"
# The frozen member list verbatim — baseline re-run + V1 (selected,
# m=1). V2–V7 are absent by selection, never bound; V8/V9 stay
# withheld{inert_at_admissible_window}.
python3 - "$O/docs" <<'PY'
import json,sys,os
for arm in ("baseline","V1"):
    p=os.path.join(sys.argv[1],f"{arm}.json")
    assert os.path.isfile(p), f"bound doc absent: {arm}"
print("bound docs: baseline + V1 present; baseline binds 4142fb10…")
PY
echo "== member docs staged"

# ── mswea pack — the r6_35 fixture pair (byte-identical to the X1
#    pack, asserted in the run doc) ───────────────────────────────────
mkdir -p "$O/l1"
rm -rf "$O/l1/pack"
cp -R "$SRC/crates/hh-compiler/tests/fixtures/r6_35" "$O/l1/pack"
[ -f "$O/l1/pack/system_template.tmpl" ] && \
[ -f "$O/l1/pack/instance_template.tmpl" ] || fail "mswea pack stage"
echo "== mswea pack staged at $O/l1/pack"

# ── window config files ──────────────────────────────────────────────
python3 - "$O" <<'PY'
import json,sys,os
O=sys.argv[1]
cfg={"agent_deadline_ms":900000,"family":"debian",
 "images_file":f"{O}/images.json",
 "jobs_root":"/var/lib/hh/runs/run-root-p4c/jobs",
 "proxy_bin":"/var/lib/hh/p4c/bin/hh-model-proxy",
 "run_root":"/var/lib/hh/runs/run-root-p4c",
 "slot_token":"hh-p4c-slot-token",
 "staging_root":"/var/lib/hh/p4c/staging",
 "tasks_dir":f"{O}/tb21/tasks",
 "verify_deadline_ms":600000,
 "workdir_default":"/app"}
json.dump(cfg,open(f"{O}/cfg.json","w"),indent=1,sort_keys=True)
lane={"catalog_base":"https://ollama.com",
 "endpoint_ref":"https://ollama.com/v1",
 "lane_id":"fp-p4c","model":"glm-5.3-flash",
 "nonstream_read_deadline_ms":300000,
 "reservation":{"declared_body_bytes":200000,
  "kind":"worst_case_formula","max_output_bound":131072,
  "max_tokens_policy":"reserve_model_cap",
  "template_overhead_tokens":2048},
 "upstream_host":"ollama.com"}
json.dump(lane,open(f"{O}/lane.json","w"),indent=1,sort_keys=True)
# GATE-G36's authorized model ceiling verbatim (46,353,600 µ$); the
# window operates the declared tighter hard stop (40,000,000 µ$) on
# the driver's --model-budget-micros — inside authorization.
auth={"authorized_micros":46353600,
 "available_credit_micros":46353600,
 "gate_record_ref":"GATE-G36","m_ppm":200000,
 "prior_windows":[],"verbatim":"run"}
json.dump(auth,open(f"{O}/auth.json","w"),indent=1,sort_keys=True)
print("cfg/lane/auth written")
PY
cp /tmp/p4c-stage/pricing.json "$O/pricing.json" 2>/dev/null || \
  echo '{}' > "$O/pricing.json"

echo "P4C-SETUP: done"
