#!/usr/bin/env bash
# Repoint chainTip at an orphan commit that does not descend from
# buildBranchBase — a stale base/signature never dispatches. Runs inside the
# git-initialized fixture sandbox ($1).
set -euo pipefail
cd "$1"
git checkout --orphan stale-line -q
git commit --allow-empty -qm "unrelated line"
git branch -f codex/harnessharness-evidence-r5.1 HEAD
git checkout -q codex/harnessharness-evidence-r5.1
