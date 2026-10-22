#!/usr/bin/env bash
# fast-path.sh — R4.2a / ADR-0362: the docs-only fast path for ci.yml.
#   nondocs-tree <commit>  sha256 of `git ls-tree -r -z --full-tree <commit>`
#                         with docs-path entries removed
#   decide               last stdout line is exactly `fast` or `full`
#   record <file>        writes the 4-line anchor record, prints the anchor name
# A docs path starts with `docs/` or `research/` (trailing slash included;
# `docsx/` and a file named `docs` are non-docs). The fingerprint is always HEAD
# of the job's own checkout (the merge commit on pull_request) — never
# pull_request.head.sha, which fails open when the base moved outside docs/.
# Fail-closed: every uncertain input answers `full`; ci.yml never sets
# CI_ANCHOR_LISTING (the self-test listing-file hook).
set -euo pipefail

sha256_of_stdin() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum;
  else shasum -a 256; fi | awk '{print $1}'
}

nondocs_tree() {
  local commit="$1"
  git cat-file -e "$commit^{commit}" 2>/dev/null \
    || { echo "fast-path: $commit is not a commit" >&2; return 2; }
  git ls-tree -r -z --full-tree "$commit" | python3 -c '
import sys
for ent in sys.stdin.buffer.read().split(b"\x00"):
    if not ent:
        continue
    _, _, path = ent.partition(b"\t")
    if path.startswith(b"docs/") or path.startswith(b"research/"):
        continue
    sys.stdout.buffer.write(ent + b"\x00")
' | sha256_of_stdin
}

# (d): docs entries carrying a symlink/gitlink mode force `full`.
docs_link_count() {
  git ls-tree -r -z --full-tree "${1:-HEAD}" | python3 -c '
import sys
n = 0
for ent in sys.stdin.buffer.read().split(b"\x00"):
    if not ent:
        continue
    meta, _, path = ent.partition(b"\t")
    if (path.startswith(b"docs/") or path.startswith(b"research/")) \
            and meta.split(b" ", 1)[0] in (b"120000", b"160000"):
        n += 1
sys.stdout.write(str(n))
'
}

report() {  # $1 = fast|full, $2 = reason
  echo "ci-$1-path: $2" >&2
  echo "::notice title=ci-$1-path::$2"
  [ -z "${GITHUB_STEP_SUMMARY:-}" ] || echo "- ci-$1-path: $2" >>"$GITHUB_STEP_SUMMARY"
}
full() { report full "$1"; echo full; exit 0; }
anchor_name() { echo "ci-anchor-v1-${CI_ANCHOR_JOB}-${RUNNER_OS}-$1"; }

decide() {
  # (a) pull_request only — push/dispatch always run full.
  [ "${CI_EVENT_NAME:-}" = "pull_request" ] \
    || full "event=${CI_EVENT_NAME:-<unset>} (the fast path is pull_request-only)"
  # (b) same-repository head — load-bearing against fork-supplied artifacts.
  [ -n "${CI_HEAD_REPO:-}" ] && [ -n "${CI_REPOSITORY:-}" ] \
    && [ "$CI_HEAD_REPO" = "$CI_REPOSITORY" ] \
    || full "head repo '${CI_HEAD_REPO:-<unset>}' != '${CI_REPOSITORY:-<unset>}'"
  # (c) sane anchor coordinates.
  case "${CI_ANCHOR_JOB:-}" in
    ""|*[!a-z0-9-]*) full "CI_ANCHOR_JOB '${CI_ANCHOR_JOB:-}' is not a ^[a-z0-9-]+\$ job id" ;;
  esac
  case "${RUNNER_OS:-}" in
    Linux|macOS) ;;
    *) full "RUNNER_OS '${RUNNER_OS:-<unset>}' is not Linux|macOS" ;;
  esac
  # (d) no symlink/gitlink under a docs path.
  links=$(docs_link_count HEAD) || links=x
  [ "$links" = "0" ] \
    || full "docs|research tree holds $links symlink/gitlink entries"
  fp=$(nondocs_tree HEAD) || full "nondocs-tree failed at HEAD"
  name=$(anchor_name "$fp")
  # (e) an unexpired, same-repository anchor for this exact non-docs tree.
  if [ -n "${CI_ANCHOR_LISTING:-}" ]; then
    listing=$(cat "$CI_ANCHOR_LISTING" 2>/dev/null) \
      || full "CI_ANCHOR_LISTING '$CI_ANCHOR_LISTING' unreadable"
  else
    listing=$(gh api -X GET "repos/$CI_REPOSITORY/actions/artifacts" \
                -f name="$name" -f per_page=20 2>/dev/null) \
      || full "artifacts API call failed for $name"
  fi
  run_id=$(printf '%s' "$listing" | python3 -c '
import json, sys
try:
    data = json.load(sys.stdin)
    rid = int(sys.argv[2])
except Exception:
    sys.exit(2)
for a in data.get("artifacts") or []:
    if a.get("name") != sys.argv[1]:
        continue
    if a.get("expired") is not False:
        continue
    wr = a.get("workflow_run") or {}
    if wr.get("repository_id") != rid or wr.get("head_repository_id") != rid:
        continue
    print(wr.get("id") or "")
    sys.exit(0)
sys.exit(1)
' "$name" "${CI_REPOSITORY_ID:-}" || true)
  [ -n "$run_id" ] \
    || full "no unexpired same-repository anchor '$name' (nondocs ${fp:0:12})"
  report fast "nondocs tree ${fp:0:12} covered by anchor from run $run_id"
  echo fast
}

record() {
  [ -n "${1:-}" ] || { echo "usage: fast-path.sh record <file>" >&2; exit 2; }
  fp=$(nondocs_tree HEAD) || exit 2
  {
    echo "nondocs=$fp"
    echo "head=$(git rev-parse HEAD)"
    echo "run=${GITHUB_RUN_ID:-unknown}"
    echo "event=${CI_EVENT_NAME:-unknown}"
  } >"$1"
  anchor_name "$fp"
}

case "${1:-}" in
  nondocs-tree) nondocs_tree "${2:?usage: fast-path.sh nondocs-tree <commit>}" ;;
  decide) decide ;;
  record) record "${2:-}" ;;
  *) echo "usage: fast-path.sh {nondocs-tree <commit>|decide|record <file>}" >&2; exit 2 ;;
esac
