#!/usr/bin/env bash
# check-publish-metadata.sh — L4.5's scripted crates.io-metadata gate
# (ADR-0380). Every publishable workspace member must carry the full
# metadata set crates.io requires, derived from `[workspace.package]`
# (the values are read from the manifest, never pinned literally), and
# every intra-workspace dependency edge that a publish needs must carry
# the workspace version. Fails loudly naming every offender.
#
# Checks per workspace member:
#   * version == workspace.package.version; rust_version present and ==
#     workspace.package.rust-version (the deliberate declaration — CI builds
#     at exactly it, so the check only compares to the source);
#   * description, license, repository, homepage, documentation, readme,
#     keywords (1..=5 — the crates.io cap), categories all present;
#   * the `publish = false` set equals ADR-0380 (a)'s recorded set exactly
#     (drift in either direction is red);
# and per publishable member:
#   * every normal/build path dep carries the workspace version;
#   * no normal/build dep targets a publish=false crate (unresolvable at
#     publish time);
#   * workspace dev-deps stay path-only — cargo strips them from the
#     published manifest, and a versioned dev-dep that isn't packaged yet
#     breaks the in-flight publish ordering (L4.5 dry-run finding).
set -euo pipefail
cd "$(dirname "$0")/.."

command -v cargo   >/dev/null || { echo "check-publish-metadata: cargo absent" >&2; exit 2; }
command -v python3 >/dev/null || { echo "check-publish-metadata: python3 absent" >&2; exit 2; }

python3 - <<'PY'
import json, re, subprocess, sys

# ADR-0380 (a)'s recorded publish=false set — kept in sync by the ADR, not
# by this file; drift in either direction is red.
RECORDED_NOPUBLISH = {
    "hharness-plugin-fixture",
    "hharness-tracker-fixture",
    "hharness-compact-evict-oldest",
    "hharness-memory-store",
    "hharness-github-issues",
}
FIELDS = ["description", "license", "repository", "homepage",
          "documentation", "readme", "keywords", "categories"]

try:
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        stderr=subprocess.PIPE))
except subprocess.CalledProcessError as e:
    sys.exit("check-publish-metadata: cargo metadata failed: "
             + e.stderr.decode()[:400])

ws = set(meta["workspace_members"])
pkgs = [p for p in meta["packages"] if p["id"] in ws]
root = open("Cargo.toml").read()
m = re.search(r'\[workspace\.package\](.*?)\n\[', root, re.S)
if not m:
    sys.exit("check-publish-metadata: no [workspace.package] table")
wsp = m.group(1)
def wsv(k):
    mm = re.search(rf'^{k}\s*=\s*"([^"]+)"', wsp, re.M)
    return mm.group(1) if mm else None
ws_version, ws_rust = wsv("version"), wsv("rust-version")
if not ws_version or not ws_rust:
    sys.exit("check-publish-metadata: workspace version/rust-version unreadable")

names = {p["name"] for p in pkgs}
nopub = {p["name"] for p in pkgs if p.get("publish") in ([], False)}
pub = names - nopub
bad = []

for n in sorted(nopub - RECORDED_NOPUBLISH):
    bad.append(f"{n}: publish = false but not in ADR-0380 (a)'s recorded set")
for n in sorted(RECORDED_NOPUBLISH - nopub):
    bad.append(f"{n}: recorded publish = false by ADR-0380 (a) but flag absent")

for p in pkgs:
    n = p["name"]
    if p["version"] != ws_version:
        bad.append(f"{n}: version {p['version']} != workspace {ws_version}")
    if p.get("rust_version") != ws_rust:
        bad.append(f"{n}: rust-version {p.get('rust_version')!r} != workspace {ws_rust}")
    for f in FIELDS:
        if not p.get(f):
            bad.append(f"{n}: missing {f}")
    if p.get("keywords") is not None and not (1 <= len(p["keywords"]) <= 5):
        bad.append(f"{n}: keywords count {len(p['keywords'])} outside 1..=5")
    if n in pub:
        for d in p["dependencies"]:
            if not d.get("path") or not d["name"].startswith("hharness"):
                continue
            if d.get("kind") == "dev":
                if d.get("req") not in (None, "*"):
                    bad.append(f"{n}: workspace dev-dep {d['name']} carries a "
                               "version — keep it path-only (cargo strips it; "
                               "a versioned dev-dep breaks publish ordering)")
                continue
            if d["name"] in nopub:
                bad.append(f"{n}: normal/build dep on publish=false {d['name']} "
                           "is unresolvable at publish time")
            elif d.get("req") not in (f"^{ws_version}", ws_version, f"={ws_version}"):
                bad.append(f"{n}: path dep {d['name']} req {d.get('req')!r} "
                           f"!= workspace version {ws_version}")

if bad:
    print("check-publish-metadata: FAIL", file=sys.stderr)
    for b in bad:
        print("  " + b, file=sys.stderr)
    sys.exit(1)
print(f"check-publish-metadata: OK — {len(pub)} publishable crates carry the full "
      f"metadata set at v{ws_version}/rust {ws_rust}; {len(nopub)} publish=false "
      "(ADR-0380 (a) set)")
PY
