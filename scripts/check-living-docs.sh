#!/usr/bin/env bash
# check-living-docs.sh — the "keep provenance, fix living documentation" guard
# (L4.1; launch ruling of 2026-10-20: personal absolute paths scrubbed from
# living human-facing files ONLY; the build record keeps its provenance).
#
# Living set = every tracked file EXCEPT:
#   - the provenance trees (append-only build record — a path there IS the
#     record): docs/build/, docs/tickets/, docs/adr/, research/
#   - code, test and lock files (*.rs, *.ts, *.js, *.py, *.lock, *.jsonl),
#     where a synthetic path like "/home/u" is a legitimate fixture
#   - this script and its self-test (they name the patterns they detect)
#
# Fails on /Users/<segment> or /home/<segment>; documented placeholder
# spellings (user, example, demo, runner, and <…> forms such as /home/<user>)
# are allowed — everything else is a personal path and fails.
# A candidate must start at a non-letter boundary: a "/" glued to a
# preceding ASCII letter (codec/home/class table) is a slashed word list,
# never a filesystem path.
#
# Usage: check-living-docs.sh [repo-root]   (default: the git toplevel)
set -euo pipefail
ROOT="${1:-$(git rev-parse --show-toplevel)}"
cd "$ROOT"

PLACEHOLDER_RE='^(user|example|demo|runner|<[^>]*>?)$'

living=$(git ls-files \
  | grep -vE '^(docs/(build|tickets|adr)/|research/)' \
  | grep -vE '\.(rs|ts|tsx|js|jsx|py|lock|jsonl)$' \
  | grep -vxE 'scripts/(check-living-docs\.sh|ci/test-check-living-docs\.sh)' || true)

count=0; hits=0
while IFS= read -r f; do
  [ -n "$f" ] || continue
  [ -f "$f" ] || continue
  count=$((count + 1))
  while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    tok=${hit##*:}                       # file:lineno:TOKEN -> TOKEN
    tok=${tok#"${tok%%/*}"}              # strip any leading boundary char
    seg=${tok#*/}                        # strip leading "/"
    seg=${seg#Users/}; seg=${seg#home/}  # -> segment
    seg=$(printf '%s' "$seg" | sed 's/[^A-Za-z0-9<>_-]*$//')  # trim trailing punctuation
    if printf '%s' "$seg" | grep -qE "$PLACEHOLDER_RE"; then continue; fi
    printf 'check-living-docs: FAIL — %s (personal path "%s")\n' "$hit" "$tok" >&2
    hits=$((hits + 1))
  done < <(grep -nIoE '(^|[^A-Za-z])/(Users|home)/[A-Za-z0-9._<>~-]+' "$f" 2>/dev/null || true)
done <<< "$living"

[ "$hits" -eq 0 ] || { echo "check-living-docs: $hits personal path(s) in living files" >&2; exit 1; }
echo "check-living-docs: OK — $count living files scanned, no personal paths"
