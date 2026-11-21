#!/usr/bin/env bash
# p4x-setup.sh — P4-X staging on hh-p4-p4x (R7.3, manifest 329,
# GATE-G35). Steps: tools (uv/git/rustup 1.94), tb21 corpus at the
# recorded pin, task-env pulls for the 11 band-A tasks (task.toml's
# declared docker_image ref; repo-digest pinned images.json),
# registries.conf, in-guest release build of the window binaries,
# p4_admissibility emit-docs + bind_hir bound arm docs, cfg/lane/
# auth/pricing placement. Run as root:  sudo bash /tmp/p4x-stage/p4x-setup.sh
set -euo pipefail
set +x

R=/var/lib/hh/p4x
RR=/var/lib/hh/runs/run-root-p4x
O=/opt/p4x
L=$R/loads.jsonl
SRC=/opt/p4x/src
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
fail() { echo "P4X-SETUP FAIL: $*" >&2; exit 1; }

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
  tar -xzf /tmp/p4x-stage/src.tar.gz -C "$SRC"
fi
[ -f "$SRC/Cargo.toml" ] || fail "source tree absent at $SRC"
echo "== source staged at $SRC"

# ── tb21 corpus at the recorded pin ──────────────────────────────────
if [ ! -d "$O/tb21" ]; then
  git clone -q https://github.com/harbor-framework/terminal-bench-2-1 "$O/tb21"
fi
( cd "$O/tb21" && git checkout -q 12b7565679ce5006230b820666353673a133db93 )
echo "corpus HEAD: $(git -C "$O/tb21" rev-parse HEAD)"
TASKS="cobol-modernization torch-pipeline-parallelism polyglot-rust-c cancel-async-tasks extract-elf modernize-scientific-stack build-pmars pytorch-model-cli pytorch-model-recovery openssl-selfsigned-cert sqlite-with-gcov"
for t in $TASKS; do
  [ -f "$O/tb21/tasks/$t/instruction.md" ] || fail "task $t instruction absent"
  [ -f "$O/tb21/tasks/$t/task.toml" ] || fail "task $t task.toml absent"
done
echo "== band-A 11 task dirs present"

# ── registries ────────────────────────────────────────────────────────
mkdir -p /etc/containers
grep -q 'unqualified-search-registries' /etc/containers/registries.conf 2>/dev/null || \
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
python3 -c "import json;d=json.load(open('$O/images.json'));print('images.json tasks:',len(d));[print(' ',k,d[k]['image']) for k in sorted(d)]"

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

# ── the arm docs: emit-docs + bind_hir ───────────────────────────────
mkdir -p "$O/docs-raw" "$O/docs"
"$R/bin/p4-admissibility" emit-docs "$O/docs-raw" || fail "emit-docs"
python3 "$SRC/scripts/gce-backend/bind-hir.py" "$O/docs-raw" "$O/docs" \
  || fail "bind-hir"
python3 - "$O/docs" <<'PY'
import json,sys,os
want={"baseline":"sha256:88042f890bdedb91bcf865d34630499b87f17b05d53d3640a3deaa863a8a0844"}
for arm in ("baseline","V1","V2","V3","V4","V5","V6","V7"):
    p=os.path.join(sys.argv[1],f"{arm}.json")
    assert os.path.isfile(p), f"bound doc absent: {arm}"
print("bound docs: 8 present; baseline binds", want["baseline"][:20])
PY
echo "== arm docs staged"

# ── window config files ──────────────────────────────────────────────
python3 - "$O" <<'PY'
import json,sys,os
O=sys.argv[1]
cfg={"agent_deadline_ms":900000,"family":"debian",
 "images_file":f"{O}/images.json",
 "jobs_root":"/var/lib/hh/runs/run-root-p4x/jobs",
 "proxy_bin":"/var/lib/hh/p4x/bin/hh-model-proxy",
 "run_root":"/var/lib/hh/runs/run-root-p4x",
 "slot_token":"hh-p4x-slot-token",
 "staging_root":"/var/lib/hh/p4x/staging",
 "tasks_dir":f"{O}/tb21/tasks",
 "verify_deadline_ms":600000,
 "workdir_default":"/app"}
json.dump(cfg,open(f"{O}/cfg.json","w"),indent=1,sort_keys=True)
lane={"catalog_base":"https://ollama.com",
 "endpoint_ref":"https://ollama.com/v1",
 "lane_id":"fp-p4x","model":"glm-5.3-flash",
 "nonstream_read_deadline_ms":300000,
 "reservation":{"declared_body_bytes":200000,
  "kind":"worst_case_formula","max_output_bound":131072,
  "max_tokens_policy":"reserve_model_cap",
  "template_overhead_tokens":2048},
 "upstream_host":"ollama.com"}
json.dump(lane,open(f"{O}/lane.json","w"),indent=1,sort_keys=True)
auth={"authorized_micros":157760000,
 "available_credit_micros":157760000,
 "gate_record_ref":"GATE-G35","m_ppm":200000,
 "prior_windows":[],"verbatim":"run"}
json.dump(auth,open(f"{O}/auth.json","w"),indent=1,sort_keys=True)
print("cfg/lane/auth written")
PY
cp /tmp/p4x-stage/pricing.json "$O/pricing.json" 2>/dev/null || \
  echo '{}' > "$O/pricing.json"

# ── G-L1 h-set for the exact baseline identity ───────────────────────
mkdir -p "$O/l1/hset"
cp "$O/docs-raw/baseline.json" "$O/l1/hset/hir"
python3 - "$O/l1/hset/h_set.json" <<'PY'
import json,sys
hs={"identity":{"participant_ref":"H","version":"frozen",
 "digest":"sha256:88042f890bdedb91bcf865d34630499b87f17b05d53d3640a3deaa863a8a0844"},
 "members":[{"kind":"hir","name":"hir","path":"hir"}]}
json.dump(hs,open(sys.argv[1],"w"),indent=1,sort_keys=True)
print("h_set.json written (H digest 88042f…)")
PY

echo "P4X-SETUP: done"
