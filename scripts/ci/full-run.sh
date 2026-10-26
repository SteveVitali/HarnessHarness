#!/usr/bin/env bash
# full-run.sh — the full-run reader (R4.2a deliverable 5, moved to L4.3
# by the D6 fallback cut; ADR-0362 contract D-D).
#
#   full-run.sh read <sha> [--repo <owner/name>]
#
# Lists the `workflow_dispatch` runs of `.github/workflows/ci.yml`,
# newest first, and takes the first whose head commit is in the local
# clone AND has the same `nondocs-tree` fingerprint as <sha> (CC1 — the
# fingerprint has one home: fast-path.sh). Read-only: `gh` with its
# built-in --jq, plus `git`.
#
# The last stdout line and the exit code are exactly one of:
#   full-run: pass <run-id> @<sha7> (<n> jobs)     exit 0
#   full-run: fail <run-id> (<job>: <conclusion>)  exit 3
#   full-run: pending <run-id>                     exit 4
#   full-run: missing (<reason>)                   exit 3
#   full-run: unknown (<reason>)                   exit 5
# A pass needs run status `completed`, conclusion `success`, and EVERY
# job conclusion `success` — a cancelled or skipped job never reads
# pass (the M9 mutation).
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
FP="$DIR/fast-path.sh"

unknown() { echo "full-run: unknown ($1)"; exit 5; }
missing() { echo "full-run: missing ($1)"; exit 3; }

cmd_read() {
  local sha="${1:?usage: full-run.sh read <sha> [--repo <owner/name>]}" repo="${2:-}"
  git cat-file -e "${sha}^{commit}" 2>/dev/null \
    || unknown "$sha is not a commit in this clone"
  local want
  want=$(bash "$FP" nondocs-tree "$sha") \
    || unknown "nondocs-tree failed for $sha"
  local repo_args=()
  [ -z "$repo" ] || repo_args=(--repo "$repo")
  # Newest-first dispatch runs of ci.yml: id<TAB>headSha pairs.
  local runs
  runs=$(gh run list --workflow .github/workflows/ci.yml \
           --event workflow_dispatch --limit 50 ${repo_args[@]+"${repo_args[@]}"} \
           --json databaseId,headSha --jq '.[] | "\(.databaseId)\t\(.headSha)"' \
           2>/dev/null) \
    || unknown "gh run list failed"
  local cand="" cand_head="" id head fp
  while IFS=$'\t' read -r id head; do
    [ -n "$id" ] || continue
    git cat-file -e "${head}^{commit}" 2>/dev/null || continue
    fp=$(bash "$FP" nondocs-tree "$head" 2>/dev/null) || continue
    [ "$fp" = "$want" ] || continue
    cand="$id"; cand_head="$head"; break
  done <<<"$runs"
  [ -n "$cand" ] \
    || missing "no workflow_dispatch ci.yml run covers ${sha:0:7}'s non-docs tree"
  # One read for run status, conclusion and per-job conclusions.
  # Lines: 1 = status, 2 = conclusion, 3+ = job "<name>\t<conclusion>".
  local info status concl
  info=$(gh run view "$cand" ${repo_args[@]+"${repo_args[@]}"} \
           --json status,conclusion,jobs \
           --jq '.status, (.conclusion // ""), (.jobs[] | .name + "\t" + (.conclusion // ""))' \
           2>/dev/null) \
    || unknown "gh run view $cand failed"
  status=$(sed -n '1p' <<<"$info")
  concl=$(sed -n '2p' <<<"$info")
  [ "$status" = "completed" ] || { echo "full-run: pending $cand"; exit 4; }
  # Every job must conclude success (M9 — a cancelled job reads fail,
  # never pass). The first non-success job names the failure.
  local njobs=0 jname jconcl first=""
  while IFS=$'\t' read -r jname jconcl; do
    [ -n "$jname" ] || continue
    njobs=$((njobs + 1))
    if [ "$jconcl" != "success" ] && [ -z "$first" ]; then
      first="$jname: ${jconcl:-<none>}"
    fi
  done <<<"$(tail -n +3 <<<"$info")"
  if [ "$concl" = "success" ] && [ -z "$first" ]; then
    echo "full-run: pass $cand @${cand_head:0:7} ($njobs jobs)"
    exit 0
  fi
  echo "full-run: fail $cand (${first:-run: $concl})"
  exit 3
}

case "${1:-}" in
  read) shift; cmd_read "${1:?usage: full-run.sh read <sha> [--repo <owner/name>]}" \
          "$( [ "${2:-}" = "--repo" ] && printf '%s' "${3:-}" )" ;;
  *) echo "usage: full-run.sh read <sha> [--repo <owner/name>]" >&2; exit 2 ;;
esac
