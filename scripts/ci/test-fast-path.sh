#!/usr/bin/env bash
# test-fast-path.sh — self-test for scripts/ci/fast-path.sh (R4.2a / ADR-0362).
# Builds throwaway git repositories under mktemp -d and drives `decide` through
# every fast/full/failure case of D-B1–D-B4, each failure case on its own.
# Fixture artifact listings are served through CI_ANCHOR_LISTING (the hook
# ci.yml never sets).
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
FP="$ROOT/scripts/ci/fast-path.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
fails=0

say() { echo "test-fast-path: $*"; }
bad() { echo "test-fast-path: FAIL — $*" >&2; fails=$((fails + 1)); }

mk_repo() {  # $1 = dir name; creates a repo: code file + docs file + docsx file
  local d="$TMP/$1"
  mkdir -p "$d/docs" "$d/docsx" "$d/research"
  cd "$d"
  git init -q .
  echo code > main.rs
  echo doc > docs/readme.md
  echo res > research/note.md
  echo notdocs > docsx/file.txt
  git -c user.name=ci -c user.email=ci@localhost add -A
  git -c user.name=ci -c user.email=ci@localhost commit -qm init
}

listing() {  # writes a fixture listing; args: name expired repo_id head_repo_id run_id
  cat > "$TMP/listing.json" <<EOF
{"total_count":1,"artifacts":[{"name":"$1","expired":$2,
 "workflow_run":{"id":$5,"repository_id":$3,"head_repository_id":$4}}]}
EOF
  echo "$TMP/listing.json"
}

expect() {  # $1 = want (fast|full), $2 = label; env already set
  local got
  got=$(bash "$FP" decide 2>"$TMP/err" | tail -n 1) || got="<exit:$?>"
  if [ "$got" = "$1" ]; then say "ok  $2 -> $got"
  else bad "$2 -> got '$got' want '$1' ($(tail -n1 "$TMP/err"))"; fi
}

# --- nondocs-tree invariants -------------------------------------------------
mk_repo r1
fp1=$(bash "$FP" nondocs-tree HEAD)
echo change2 > docs/other.md
git -c user.name=ci -c user.email=ci@localhost add -A
git -c user.name=ci -c user.email=ci@localhost commit -qm docs-only
fp2=$(bash "$FP" nondocs-tree HEAD)
[ "$fp1" = "$fp2" ] || bad "docs-only change altered the nondocs fingerprint"
echo more >> main.rs
git -c user.name=ci -c user.email=ci@localhost add -A
git -c user.name=ci -c user.email=ci@localhost commit -qm code
fp3=$(bash "$FP" nondocs-tree HEAD)
[ "$fp1" != "$fp3" ] || bad "non-docs change kept the fingerprint"
git -c user.name=ci -c user.email=ci@localhost rm -q docs/readme.md
git -c user.name=ci -c user.email=ci@localhost commit -qm rmdocs
fp4=$(bash "$FP" nondocs-tree HEAD)
[ "$fp4" = "$fp3" ] || bad "docs removal altered the nondocs fingerprint"
echo newfile > docsfile
git -c user.name=ci -c user.email=ci@localhost add docsfile
git -c user.name=ci -c user.email=ci@localhost commit -qm docsfile
fp5=$(bash "$FP" nondocs-tree HEAD)
[ "$fp5" != "$fp3" ] || bad "a file named docs* outside docs/ changed nothing"
say "nondocs-tree invariants hold (docs-only and docs-removal invisible; code and docsx visible)"

# --- decide env scaffold ------------------------------------------------------
export CI_ANCHOR_JOB=build-test-drift RUNNER_OS=Linux \
       CI_REPOSITORY_ID=1363349009 CI_REPOSITORY=me/repo \
       CI_HEAD_REPO=me/repo CI_EVENT_NAME=pull_request
NAME="ci-anchor-v1-build-test-drift-Linux-$fp5"
unset CI_ANCHOR_LISTING GITHUB_STEP_SUMMARY || true

# fast: everything holds.
cd "$TMP/r1"
NAME="ci-anchor-v1-build-test-drift-Linux-$fp5"
CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
  expect fast "all conditions hold (same-repo anchor, unexpired)"
grep -q 'ci-fast-path' "$TMP/err" || bad "fast run did not name its reason on stderr"

# each failure case on its own --------------------------------------------------
CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
  CI_EVENT_NAME=push expect full "event=push"
CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
  CI_EVENT_NAME=workflow_dispatch expect full "event=workflow_dispatch"
CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
  CI_HEAD_REPO=fork/repo expect full "fork head repository"
CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
  CI_HEAD_REPO= expect full "empty head repository"
CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
  CI_ANCHOR_JOB=Bad_Job expect full "bad CI_ANCHOR_JOB"
CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
  RUNNER_OS=Windows expect full "RUNNER_OS=Windows"
CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
  RUNNER_OS= expect full "RUNNER_OS unset"

# (d) symlink under docs/ — even though the fingerprint ignores it.
mk_repo r3
ln -s ../main.rs docs/link.rs
git -c user.name=ci -c user.email=ci@localhost add -A
git -c user.name=ci -c user.email=ci@localhost commit -qm symlink
fpL=$(bash "$FP" nondocs-tree HEAD)
CI_ANCHOR_LISTING=$(listing "ci-anchor-v1-build-test-drift-Linux-$fpL" \
                   false 1363349009 1363349009 1) \
  expect full "symlink under docs/ (mode 120000)"
# gitlink under research/ (a submodule-shaped entry).
blob=$(git rev-parse HEAD:main.rs)
git -c user.name=ci -c user.email=ci@localhost \
    update-index --add --cacheinfo "160000,$blob,research/sub"
git -c user.name=ci -c user.email=ci@localhost commit -qm gitlink
fpG=$(bash "$FP" nondocs-tree HEAD)
CI_ANCHOR_LISTING=$(listing "ci-anchor-v1-build-test-drift-Linux-$fpG" \
                   false 1363349009 1363349009 1) \
  expect full "gitlink under research/ (mode 160000)"

# (e) listing failure shapes, each alone -----------------------------------------
cd "$TMP/r1"
CI_ANCHOR_LISTING="$TMP/nonexistent.json" expect full "unreadable listing file"
echo 'not json' > "$TMP/bad.json"
CI_ANCHOR_LISTING="$TMP/bad.json" expect full "unparsable listing"
echo '{"artifacts":[]}' > "$TMP/empty.json"
CI_ANCHOR_LISTING="$TMP/empty.json" expect full "empty listing"
CI_ANCHOR_LISTING=$(listing "$NAME" true 1363349009 1363349009 4242) \
  expect full "expired anchor only"
CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 999 4242) \
  expect full "anchor from a fork run (head_repository_id differs)"
CI_ANCHOR_LISTING=$(listing "$NAME" false 999 1363349009 4242) \
  expect full "anchor from another repository"
CI_ANCHOR_LISTING=$(listing "ci-anchor-v1-build-test-drift-Linux-deadbeef" \
                   false 1363349009 1363349009 4242) \
  expect full "anchor for a different fingerprint"
CI_REPOSITORY_ID= \
  CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
  expect full "CI_REPOSITORY_ID unset"

# gh API error (CI_ANCHOR_LISTING unset): a gh shim that always fails.
mkdir -p "$TMP/bin"
cat > "$TMP/bin/gh" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
chmod +x "$TMP/bin/gh"
PATH="$TMP/bin:/usr/bin:/bin:$PATH" expect full "gh api error answers full"
# and without any gh at all on PATH
PATH="/usr/bin:/bin" expect full "no gh on PATH answers full"

# last-line contract: stdout's last line is exactly fast|full.
out=$(CI_ANCHOR_LISTING=$(listing "$NAME" false 1363349009 1363349009 4242) \
      bash "$FP" decide 2>/dev/null)
[ "$(tail -n1 <<<"$out")" = fast ] || bad "last line was not exactly 'fast'"
grep -q '^::notice title=ci-fast-path::' <<<"$out" \
  || bad "no ::notice annotation emitted on the fast path"

# --- record --------------------------------------------------------------------
cd "$TMP/r1"
name=$(GITHUB_RUN_ID=777 bash "$FP" record "$TMP/rec.txt")
[ "$name" = "$NAME" ] || bad "record printed anchor name '$name', want '$NAME'"
[ "$(wc -l < "$TMP/rec.txt" | tr -d ' ')" = 4 ] || bad "record file is not 4 lines"
grep -q "^nondocs=$fp5\$" "$TMP/rec.txt" || bad "record missing nondocs= line"
grep -q '^run=777$' "$TMP/rec.txt" || bad "record missing run= line"
grep -q '^event=pull_request$' "$TMP/rec.txt" || bad "record missing event= line"
say "record emits the 4-line record + anchor name"

[ "$fails" -eq 0 ] || { echo "test-fast-path: $fails failure(s)" >&2; exit 1; }
echo "test-fast-path: OK"
