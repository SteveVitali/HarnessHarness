#!/usr/bin/env bash
# check-ci-gating.sh — the step-mode lint for .github/workflows/ci.yml
# (R4.2a / ADR-0362); plain bash/awk for THIS workflow, not a YAML parser.
#   check-ci-gating.sh <workflow>    all checks, including the readers scan
#   check-ci-gating.sh --readers     readers scan only
# Checks: (1) every step after `id: path` carries a gate or a
# `# ci-mode: both — ` comment (the last two are checked by the anchor rule);
# (2) the anchor pair are the job's last two steps, each `if:` exactly the D-B5
# expression; (3) `permissions` has `actions: read`; (4) `push` is main-only;
# (5) `CI_ANCHOR_LISTING` never appears; (6) the readers scan.
# Self-test overrides (never set in ci.yml): CI_GATING_ROOT, CI_GATING_ALLOW.
set -euo pipefail

WF="${1:?usage: check-ci-gating.sh <workflow.yml> | --readers}"
ALLOW="${CI_GATING_ALLOW:-scripts/ci/docs-readers.allow}"
fails=0
fail() { echo "check-ci-gating: FAIL — $*" >&2; fails=$((fails + 1)); }

STEP_HINT="if this step reads committed content under docs/ or research/, mark it \`# ci-mode: both — <reason>'; otherwise add \`if: steps.path.outputs.mode != 'fast'\`"
READER_HINT="a gated test or script now reads committed content under docs/ or research/ — never allowlist a real read; move the input out of docs/ and research/, or STOP and ask"
GATE_RE="^steps\.path\.outputs\.mode != 'fast'([[:space:]]*$|[[:space:]]*&& )"
ANCHOR_IF="success() && steps.path.outputs.mode == 'full' && (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository)"

TMPD=$(mktemp -d); trap 'rm -rf "$TMPD"' EXIT
GATED_SCRIPTS=""

# readers scan — hits are `path:lineno:line`; an allowlist entry is
# `path<TAB>full hit line<TAB># reason` (one literal line; a stale entry fails).
readers_scan() {
  local root="${CI_GATING_ROOT:-$(git rev-parse --show-toplevel)}"
  local hits="$TMPD/hits" ; : > "$hits"
  ( cd "$root" && git grep -n -E '(["/]|^)(\.\./)*(docs|research)(/|"|$)' \
      -- 'crates/**' 'plugins/**' 'foreign/**' 'spikes/**' ) \
    > "$hits" 2>/dev/null || true
  local s
  for s in $GATED_SCRIPTS; do
    grep -nE '(["/=]|[[:space:]]|^)(\.\./)*(docs|research)(/|"|$)' \
      "$root/$s" 2>/dev/null | sed "s|^|$s:|" >> "$hits" || true
  done

  local n=0 entry_path entry_line entry_reason
  local ok_path="$TMPD/ok_path" ok_line="$TMPD/ok_line"
  : > "$ok_path"; : > "$ok_line"
  if [ -f "$ALLOW" ]; then
    while IFS= read -r raw; do
      [ -z "$raw" ] && continue
      n=$((n + 1))
      case "$raw" in
        *$'\t'*$'\t'*) ;;
        *) fail "allowlist line $n is not 'path<TAB>line<TAB># reason' — a \
whole-file entry is never allowed: ${raw:0:60}"; continue;;
      esac
      entry_path=${raw%%$'\t'*}
      entry_reason=${raw##*$'\t'}
      entry_line=${raw#*$'\t'}; entry_line=${entry_line%$'\t'*}
      case "$entry_reason" in '#'*) ;; *) fail "allowlist line $n has no '# reason' field";; esac
      [ -n "$entry_line" ] || { fail "allowlist line $n is a whole-file entry, not one literal line"; continue; }
      if ! grep -qF "$entry_line" "$hits" \
         && printf '%s' "$entry_line" | grep -qE '\*\*|\*$|^\*'; then
        fail "allowlist line $n looks like a pattern, not one literal line: ${entry_line:0:60}"
        continue
      fi
      printf '%s\n' "$entry_path" >> "$ok_path"
      printf '%s\n' "$entry_line" >> "$ok_line"
    done < "$ALLOW"
  fi

  local used="$TMPD/used" hit hit_path hit_rest hit_line matched i
  : > "$used"
  while IFS= read -r hit; do
    [ -z "$hit" ] && continue
    hit_path=${hit%%:*}
    hit_rest=${hit#*:}; hit_line=${hit_rest#*:}
    matched=""; i=0
    while IFS= read -r p && IFS= read -r l <&3; do
      i=$((i + 1))
      if [ "$p" = "$hit_path" ] && [ "$l" = "$hit_line" ]; then matched=$i; break; fi
    done < "$ok_path" 3< "$ok_line"
    if [ -z "$matched" ]; then fail "readers scan: $hit — $READER_HINT"
    else echo "$matched" >> "$used"; fi
  done < "$hits"
  i=0
  while IFS= read -r p; do
    i=$((i + 1))
    grep -qx "$i" "$used" || fail "stale allowlist entry: $p"
  done < "$ok_path"

  echo "check-ci-gating: readers scan clean ($(grep -c . "$hits" || true) hit(s), $n allowlisted)"
}

if [ "$WF" = "--readers" ]; then
  readers_scan
  [ "$fails" -eq 0 ] || exit 1
  exit 0
fi
[ -f "$WF" ] || { echo "check-ci-gating: no such file: $WF" >&2; exit 2; }

# split the job's steps: a step starts at `      - `; a `      #` comment line
# outside a block is buffered into `pend` and lands atop the NEXT step's block,
# so a `# ci-mode: both — ` comment marks the step it precedes.
awk -v dir="$TMPD" -v countfile="$TMPD/count" '
  /^      - / {
    if (inblock) close(f)
    inblock=1; n++
    f=sprintf("%s/step%02d", dir, n)
    printf "%s", pend > f; pend=""
    print substr($0, index($0, "- ") + 2) > f
    next
  }
  /^      #/ {
    if (inblock) { close(f); inblock=0 }
    pend = pend substr($0, 7) "\n"; next
  }
  inblock && /^      [^ ]/ { close(f); inblock=0 }
  inblock { print substr($0, 9) > f }
  END { if (inblock) close(f); print n > countfile }
' "$WF"
NSTEPS=$(cat "$TMPD/count")

PATH_STEP=0
for i in $(seq 1 "$NSTEPS"); do
  grep -qE '^id: path$' "$TMPD/step$(printf %02d "$i")" && PATH_STEP=$i
done
step_if()   { awk '/^if:/   { sub(/^if:[[:space:]]*/,   ""); print; exit }' "$1"; }
step_name() { awk '/^name:/ { sub(/^name:[[:space:]]*/, ""); print; exit }' "$1"; }
is_gate() {
  [[ "$1" =~ $GATE_RE ]] || return 1
  case "$1" in *'||'*|*'always()'*|*'!cancelled()'*) return 1;; esac
}

# checks 1 + 2: step modes and the anchor pair
if [ "$PATH_STEP" -gt 0 ]; then
  for i in $(seq $((PATH_STEP + 1)) $((NSTEPS - 2))); do
    b="$TMPD/step$(printf %02d "$i")"; nm=$(step_name "$b"); ie=$(step_if "$b")
    if is_gate "$ie"; then
      while IFS= read -r s; do
        [ -n "$s" ] && GATED_SCRIPTS="$GATED_SCRIPTS $s"
      done <<EOF
$(grep -oE 'scripts/[A-Za-z0-9._/-]+\.sh' "$b" || true)
EOF
    elif grep -qF '# ci-mode: both — ' "$b"; then
      :
    else
      fail "step '${nm:-step $i}' has neither a gate nor a '# ci-mode: both — ' comment — $STEP_HINT"
    fi
  done
  for b in "$TMPD/step$(printf %02d $((NSTEPS - 1)))" "$TMPD/step$(printf %02d $NSTEPS)"; do
    [ "$(step_if "$b")" = "$ANCHOR_IF" ] \
      || fail "anchor step '$(step_name "$b")' if: is not exactly the D-B5 anchor expression"
    grep -qi 'anchor' "$b" \
      || fail "the job's last two steps are not the anchor pair"
  done
else
  echo "check-ci-gating: no 'id: path' step — step-mode checks skipped" >&2
fi

# check 3: permissions carries actions: read
awk '/^permissions:/ { p=1; next } p && /^[a-z]/ { p=0 } p' "$WF" \
  | grep -qE '^[[:space:]]+actions:[[:space:]]*read[[:space:]]*$' \
  || fail "permissions lacks 'actions: read'"

# check 4: push restricted to main
push_ok=$(awk '
  /^  push:/ { p=1; next }
  p && /^  [a-z_]+:/ { p=0 }
  p && /branches:/ {
    if ($0 ~ /branches:[[:space:]]*\[?"?main"?[[:space:]]*\]?[[:space:]]*$/) found=1
    else bad=1
  }
  p && /^    - main$/ { found=1 }
  p && /^    - / && $0 !~ /- main$/ { bad=1 }
  END { print (found && !bad) ? "yes" : "no" }
' "$WF")
[ "$push_ok" = yes ] || fail "push is not restricted to main"

# check 5: the self-test hook never appears in the workflow
if grep -q 'CI_ANCHOR_LISTING' "$WF"; then
  fail "CI_ANCHOR_LISTING appears in the workflow (it is the self-test hook)"
fi

# check 6: readers scan
readers_scan

[ "$fails" -eq 0 ] || { echo "check-ci-gating: $fails violation(s)" >&2; exit 1; }
echo "check-ci-gating: OK"
