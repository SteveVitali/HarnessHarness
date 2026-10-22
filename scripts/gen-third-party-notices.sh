#!/usr/bin/env bash
# gen-third-party-notices.sh — regenerate THIRD_PARTY_NOTICES.md (L4.1).
#
# The component set + pinned versions come from the committed Cargo.lock
# (entries carrying a registry `source`); license expressions come from
# `cargo metadata --format-version 1` (the crates' own manifests).
# A lock package with no license expression, or a metadata↔lockfile set
# mismatch, fails loudly — a new admission forces a notice review, never
# a silent omission. Modes:
#   gen-third-party-notices.sh           write THIRD_PARTY_NOTICES.md
#   gen-third-party-notices.sh --check   regenerate to temp; fail on drift
set -euo pipefail
cd "$(dirname "$0")/.."

MODE=write
[ "${1:-}" = "--check" ] && MODE=check
command -v python3 >/dev/null || { echo "gen-third-party-notices: python3 absent" >&2; exit 2; }
command -v cargo   >/dev/null || { echo "gen-third-party-notices: cargo absent" >&2; exit 2; }

OUT=THIRD_PARTY_NOTICES.md
GEN_FILE="$OUT" GEN_TMP="" python3 - "$MODE" <<'PY'
import json, os, re, subprocess, sys

mode = sys.argv[1]
out_path = os.environ["GEN_FILE"]

# --- component set + versions from Cargo.lock ---------------------------------
lock = open("Cargo.lock", encoding="utf-8").read()
lock_pkgs = {}  # (name, version) -> has-registry-source
cur = {}
for line in lock.splitlines():
    line = line.strip()
    if line == "[[package]]":
        if cur.get("name") and cur.get("source"):
            lock_pkgs[(cur["name"], cur["version"])] = True
        cur = {}
    elif "=" in line:
        k, _, v = line.partition("=")
        cur[k.strip()] = v.strip().strip('"')
if cur.get("name") and cur.get("source"):
    lock_pkgs[(cur["name"], cur["version"])] = True

# --- license expressions from cargo metadata ----------------------------------
try:
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1"],
        stderr=subprocess.PIPE))
except subprocess.CalledProcessError as e:
    sys.exit("gen-third-party-notices: cargo metadata failed: "
             + e.stderr.decode()[:400])
meta_pkgs = {}
for p in meta["packages"]:
    if p.get("source") is None:
        continue  # workspace member / path dep — ours, not third-party
    meta_pkgs[(p["name"], p["version"])] = p.get("license")

if set(lock_pkgs) != set(meta_pkgs):
    only_lock = sorted(set(lock_pkgs) - set(meta_pkgs))
    only_meta = sorted(set(meta_pkgs) - set(lock_pkgs))
    sys.exit(f"gen-third-party-notices: lock↔metadata set mismatch "
             f"(lock-only={only_lock} metadata-only={only_meta})")

missing = sorted(k for k, v in meta_pkgs.items() if not v)
if missing:
    sys.exit(f"gen-third-party-notices: no license expression for {missing} "
             "— review the admission and extend this script")

# --- curated notice text -------------------------------------------------------
# Packages whose admission or license family needs a stated notice. Keyed by
# name (applies to every locked version of it).
DIRECT = {
    "ring":           "admitted by ADR-0359 — `hh-secrets` DPoP verify + blob-pool AEAD",
    "rustls":         "admitted by ADR-0365 — `hh-tls` closed HTTPS/mTLS scope (`ring` backend)",
    "rustls-webpki":  "admitted by ADR-0365 — certificate verification inside `hh-tls`",
    "webpki-roots":   "admitted by ADR-0365 — Mozilla root store inside `hh-tls`",
    "tempfile":       "dev-dependency of `hh-varhost` tests only — never a shipped artifact",
}
NOTES = {
    "ring": ("ring is licensed Apache-2.0 AND ISC. Portions are derived from "
             "OpenSSL and BoringSSL and carry the OpenSSL/SSLeay license terms; "
             "see the crate's LICENSE file."),
    "rustls-webpki": "ISC-licensed; see the ISC text in the appendix below.",
    "untrusted": "ISC-licensed; see the ISC text in the appendix below.",
    "webpki-roots": ("The crate is licensed CDLA-Permissive-2.0. The embedded "
                     "data is Mozilla's root certificate store, licensed "
                     "MPL-2.0 — a data set, not a code license."),
    "subtle": "BSD-3-Clause; see the BSD-3-Clause text in the appendix below.",
    "r-efi": ("MIT OR Apache-2.0 OR LGPL-2.1-or-later — an election among "
              "licenses; we rely on MIT OR Apache-2.0."),
    "linux-raw-sys": "License expression includes Apache-2.0 WITH LLVM-exception.",
    "rustix": "License expression includes Apache-2.0 WITH LLVM-exception.",
    "wasi": "License expression includes Apache-2.0 WITH LLVM-exception.",
}
ISC_TEXT = """\
ISC License

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE."""
BSD3_TEXT = """\
BSD 3-Clause License

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:
(1) Redistributions of source code must retain the copyright notice, this
list of conditions and the following disclaimer. (2) Redistributions in
binary form must reproduce the copyright notice, this list of conditions
and the following disclaimer in the documentation and/or other materials
provided with the distribution. (3) Neither the name of the copyright holder
nor the names of its contributors may be used to endorse or promote products
derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED "AS IS" WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING THE WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS
BE LIABLE FOR DAMAGES ARISING FROM USE OF THIS SOFTWARE."""

rows = sorted(meta_pkgs.items())
direct = [(k, lic) for k, lic in rows if k[0] in DIRECT]
trans = [(k, lic) for k, lic in rows if k[0] not in DIRECT]

L = []
L.append("# Third-Party Notices")
L.append("")
L.append("HarnessHarness is dual-licensed under **MIT OR Apache-2.0** (see")
L.append("`LICENSE-MIT` and `LICENSE-APACHE`). The workspace is pure-std")
L.append("modulo a named, pinned admission set (ADR-0359; ADR-0365) plus one")
L.append("dev-dependency — the complete inventory is below.")
L.append("")
L.append("This file is **generated — do not edit**. Regenerate with")
L.append("`bash scripts/gen-third-party-notices.sh`; CI fails on drift")
L.append("(`--check`). The component set and pinned versions come from the")
L.append("committed `Cargo.lock`; license expressions come from")
L.append("`cargo metadata --format-version 1` (each crate's own manifest).")
L.append("")
L.append("## Direct admitted dependencies")
L.append("")
for (name, ver), lic in direct:
    L.append(f"### {name} {ver} — {lic}")
    L.append("")
    L.append(f"_{DIRECT[name]}._")
    if name in NOTES:
        L.append("")
        L.append(NOTES[name])
    L.append("")
L.append("## Transitive dependencies (lockfile-pinned)")
L.append("")
L.append("| package | version | license |")
L.append("|---|---|---|")
for (name, ver), lic in trans:
    L.append(f"| {name} | {ver} | {lic} |")
L.append("")
noted = [(k, lic) for k, lic in trans if k[0] in NOTES]
if noted:
    L.append("Notes on transitive entries:")
    L.append("")
    for (name, ver), _lic in noted:
        L.append(f"- **{name} {ver}** — {NOTES[name]}")
    L.append("")
L.append("## Appendix — referenced license texts")
L.append("")
L.append("### ISC license (`rustls-webpki`, `untrusted`; the ISC component of `ring`)")
L.append("")
L.append("```")
L.append(ISC_TEXT)
L.append("```")
L.append("")
L.append("### BSD-3-Clause (`subtle`)")
L.append("")
L.append("```")
L.append(BSD3_TEXT)
L.append("```")
L.append("")
L.append("Other referenced licenses: MPL-2.0 (the `webpki-roots` Mozilla root")
L.append("data), CDLA-Permissive-2.0 (`webpki-roots` packaging),")
L.append("Apache-2.0 WITH LLVM-exception (`linux-raw-sys`, `rustix`, `wasi`),")
L.append("LGPL-2.1-or-later (an `r-efi` election option) — see each crate's")
L.append("license files and https://spdx.org/licenses/.")
L.append("")

body = "\n".join(L)
if mode == "check":
    tmp = os.environ["GEN_TMP"] or (out_path + ".check-tmp")
    open(tmp, "w", encoding="utf-8").write(body)
    sys.exit(0)
open(out_path, "w", encoding="utf-8").write(body)
print(f"gen-third-party-notices: wrote {out_path} "
      f"({len(direct)} direct, {len(trans)} transitive)")
PY
rc=$?
[ "$MODE" = check ] || exit "$rc"
[ -f "$OUT" ] || { echo "gen-third-party-notices: $OUT absent — run without --check" >&2; rm -f "$OUT.check-tmp"; exit 2; }
if diff -u "$OUT" "$OUT.check-tmp" >/dev/null 2>&1; then
  rm -f "$OUT.check-tmp"; echo "gen-third-party-notices: $OUT in sync"
else
  echo "gen-third-party-notices: FAIL — $OUT drifted; run scripts/gen-third-party-notices.sh" >&2
  diff -u "$OUT" "$OUT.check-tmp" | head -40 || true
  rm -f "$OUT.check-tmp"; exit 1
fi
