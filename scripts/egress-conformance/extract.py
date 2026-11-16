#!/usr/bin/env python3
"""extract.py — the R6.45k mechanical shape extractor.

Reads the committed `staging/tests/*` bytes of the 14 re-proof members
(the newest committed `*-fp-wc*-tasklists.json` member set, excluding
`no_adjacent_donor` placeholders) and their pre-re-proofed successors
(the W1 draw's committed trial members under
`docs/build/reports/compute/harvest/fp-w1/run-root/trials/`), joins `\\`
continuations into logical lines, and emits the deduplicated bootstrap
inventory under `scripts/egress-conformance/derived/` — the shape list
cannot drift: `check-derived.sh` regenerates and diffs.

Emits:
  members.json       member -> committed test.sh copies (path + sha256)
  test-scripts.txt   one committed test.sh path per member (dedup by sha)
  logical-lines.txt  every unique non-comment logical line
  bootstrap.txt      the unique bootstrap lines (apt-get/curl/uvx/pip/env)
  apt-install.txt    unique `apt-get install` lines (verbatim)
  pip.txt            unique `pip[3] install` lines (verbatim)
  uvx.txt            unique `uvx` command lines (verbatim, pytest tail kept)
  scan-inputs.txt    every committed staging/tests/* file (the D16 scan set)
"""
import glob
import hashlib
import json
import os
import re
import sys

REPO = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
DERIVED = (
    sys.argv[2]
    if len(sys.argv) > 2
    else os.path.join(REPO, "scripts/egress-conformance/derived")
)

HARVEST = "docs/build/reports/compute/harvest"
W1_TRIALS = os.path.join(REPO, HARVEST, "fp-w1/run-root/trials")


def logical_lines(path):
    """Join trailing-backslash continuations; strip comments/blanks."""
    out, buf = [], None
    with open(path, errors="replace") as f:
        for raw in f:
            line = raw.rstrip("\n")
            buf = line if buf is None else buf + line
            if buf.endswith("\\"):
                buf = buf[:-1]
                continue
            out.append(buf)
            buf = None
    if buf is not None:
        out.append(buf)
    return out


def members():
    """The member set: latest fp-wc* tasklists members ∪ W1 trial members."""
    mset = set()
    lists = sorted(
        glob.glob(os.path.join(REPO, "docs/build/reports/*-fp-wc*-tasklists.json"))
    )
    tasklists = []
    if lists:
        latest = lists[-1]
        tasklists.append(os.path.relpath(latest, REPO))
        d = json.load(open(latest))
        for m in d.get("members", []):
            name = m.get("member", "")
            if name and "no_adjacent_donor" not in name:
                mset.add(name)
    member_dirs = []
    if os.path.isdir(W1_TRIALS):
        member_dirs.append(os.path.relpath(W1_TRIALS, REPO))
        for name in os.listdir(W1_TRIALS):
            if os.path.isdir(os.path.join(W1_TRIALS, name)):
                mset.add(name)
    return sorted(mset), tasklists, member_dirs


def staging_tests(member):
    """Committed staging/tests/* files for a member, across harvests."""
    pat = os.path.join(
        REPO, HARVEST, "fp-*", "run-root*", "trials", member, "*", "r*", "staging", "tests", "*"
    )
    return sorted(p for p in glob.glob(pat) if os.path.isfile(p))


def main():
    mset, tasklists, member_dirs = members()
    if not mset:
        sys.exit("extract: no members found")
    os.makedirs(DERIVED, exist_ok=True)

    member_doc = {
        "schema": "hh-egress-conformance-members/1",
        "generated_from": {"tasklists": tasklists, "member_dirs": member_dirs},
        "members": {},
    }
    all_lines = {}
    test_scripts = []
    scan_inputs = []
    for m in mset:
        files = staging_tests(m)
        scan_inputs.extend(files)
        scripts = [f for f in files if os.path.basename(f) == "test.sh"]
        seen_sha = set()
        for s in scripts:
            b = open(s, "rb").read()
            sha = hashlib.sha256(b).hexdigest()
            if sha in seen_sha:
                continue
            seen_sha.add(sha)
            rel = os.path.relpath(s, REPO)
            member_doc["members"].setdefault(m, {"test_scripts": []})[
                "test_scripts"
            ].append({"path": rel, "sha256": sha})
            test_scripts.append(rel)
            for line in logical_lines(s):
                t = line.strip()
                if t and not t.startswith("#"):
                    all_lines.setdefault(t, m)

    uniq = sorted(all_lines)

    def classified(rx):
        return [l for l in uniq if re.match(rx, l)]

    apt = classified(r"apt-get\s+install\b")
    pip = classified(r"pip3?\s+install\b")
    uvx = classified(r"uvx\b")
    bootstrap = classified(
        r"(apt-get\b|curl\b|wget\b|uvx\b|pip3?\b|npm\b|npx\b|source\s+\$HOME/\.local/bin/env\b)"
    )

    def w(name, lines):
        with open(os.path.join(DERIVED, name), "w") as f:
            for l in lines:
                f.write(l + "\n")

    w("members.json", [json.dumps(member_doc, sort_keys=True, indent=2)])
    w("test-scripts.txt", test_scripts)
    w("logical-lines.txt", uniq)
    w("bootstrap.txt", bootstrap)
    w("apt-install.txt", apt)
    w("pip.txt", pip)
    w("uvx.txt", uvx)
    w(
        "scan-inputs.txt",
        sorted(os.path.relpath(p, REPO) for p in scan_inputs),
    )
    print(
        "extract: %d members, %d unique logical lines (%d apt-install, %d pip, %d uvx), %d scan files"
        % (len(mset), len(uniq), len(apt), len(pip), len(uvx), len(scan_inputs))
    )


if __name__ == "__main__":
    main()
