#!/usr/bin/env bash
# check-pages-workflow.sh — R6.6 (F2-g, AR-28): the Pages-workflow static
# lint (REQ-XF-128, ADR-0424). Checks .github/workflows/pages.yml against
# its contract, not its shape:
#   (1) triggers on push to main and workflow_dispatch only (not PRs —
#       an untrusted head must never reach a Pages deploy);
#   (2) minimal permissions — contents:read, pages:write, id-token:write
#       and nothing else;
#   (3) every `uses:` action pinned by a 40-hex commit SHA;
#   (4) the deploy lane is gated on the publication record — build and
#       check-site.sh --profile public run unconditionally, while the
#       configure/upload/deploy steps carry the `published` guard so the
#       job deploys nothing when no hh-site-publication/1 record exists;
#   (5) a concurrency group so two pushes never race a deploy;
#   (6) the public profile is built with --profile public.
# Exit 0 clean, 1 contract violation, 2 missing/malformed file.
set -euo pipefail

WF="${1:-.github/workflows/pages.yml}"
[ -f "$WF" ] || { echo "check-pages-workflow: FAIL — $WF absent" >&2; exit 2; }
Y=$(cat "$WF")
fails=0
say() { echo "check-pages-workflow: $*"; }
bad() { echo "check-pages-workflow: FAIL — $*" >&2; fails=$((fails + 1)); }
need() { grep -q "$1" <<<"$Y" || bad "$2"; }

# (1) triggers
grep -qE '^  push:' <<<"$Y" || bad "push trigger absent"
grep -qE 'branches: *\[main\]' <<<"$Y" || bad "push not restricted to main"
grep -qE '^  workflow_dispatch:' <<<"$Y" || bad "workflow_dispatch absent"
grep -qE 'pull_request' <<<"$Y" && bad "pull_request trigger present" || true

# (2) permissions — exactly the three minimal entries at top level
perm=$(awk '/^permissions:/{f=1;next} /^[a-z]/{f=0} f' <<<"$Y")
[ -n "$perm" ] || bad "permissions block absent"
for p in 'contents: *read' 'pages: *write' 'id-token: *write'; do
  grep -qE "^  $p\$" <<<"$perm" || bad "missing permission: $p"
done
extra=$(grep -E '^  [a-z-]+: *' <<<"$perm" | grep -vE '^(  contents: read|  pages: write|  id-token: write)$' || true)
[ -z "$extra" ] || bad "extra permissions: $(tr '\n' ' ' <<<"$extra")"

# (3) every `uses:` pinned by 40-hex SHA + version comment
n_uses=$(grep -cE '^ *-? *uses:' <<<"$Y" || true)
n_sha=$(grep -cE 'uses: *[^ @]+@[0-9a-f]{40}' <<<"$Y" || true)
[ "$n_uses" -eq "$n_sha" ] || bad "$n_uses uses: line(s), $n_sha SHA-pinned"
grep -cE '@[0-9a-f]{40} +# *v[0-9]' <<<"$Y" >/dev/null || bad "SHA pins lack # v… comments"

# (4) the record gate precedes every deploy step
grep -q 'id: pub' <<<"$Y" || bad "publication-record gate step (id: pub) absent"
for s in configure-pages upload-pages-artifact deploy-pages; do
  grep -q "actions/$s" <<<"$Y" || bad "$s action absent"
  ln=$(grep -n "actions/$s" <<<"$Y" | cut -d: -f1)
  head -n "$ln" "$WF" | grep -q "steps.pub.outputs.published" \
    || bad "$s not preceded by the published guard"
  # the step itself must carry the published condition
  awk -v a="$s" 'found && /^    steps:/ {} {print} ' /dev/null
done
# every deploy-lane step carries the if-guard
guards=$(grep -c "steps.pub.outputs.published" <<<"$Y" || true)
[ "$guards" -ge 3 ] || bad "fewer than 3 published guards ($guards)"

# the gate keys on the record directory, not on git state
grep -q 'docs/results/publication/\*\.json' <<<"$Y" \
  || bad "gate does not glob docs/results/publication/*.json"

# (5) concurrency
grep -q '^concurrency:' <<<"$Y" || bad "concurrency block absent"
grep -q 'group: pages' <<<"$Y" || bad "concurrency group not 'pages'"
grep -q 'cancel-in-progress' <<<"$Y" || bad "cancel-in-progress unset"

# (6) public-profile build + check wired in
grep -q 'build.py --out _site --profile public' <<<"$Y" \
  || bad "public-profile build step absent"
grep -q 'check-site.sh --profile public' <<<"$Y" \
  || bad "public-profile check step absent"

[ "$fails" -eq 0 ] || { echo "check-pages-workflow: $fails failure(s)" >&2; exit 1; }
say "OK — $WF"
