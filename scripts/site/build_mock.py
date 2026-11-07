#!/usr/bin/env python3
"""build_mock.py — R6.4 (F2-a) mock build entry.

One stdlib command that builds the deterministic mock site into a given
directory:

    python3 scripts/site/build_mock.py --out <dir> [--profile internal|all-absent]

  internal    — the GATE-G21 mock: home + specimen on phase-1 fixtures,
                every other §9.2 path in its designed absent state
  all-absent  — every data path absent (the golden all-absent site;
                the always-present pages render their static content)

Deterministic: no wall clock, no network, sorted writes; two runs produce
an identical tree sha256 (AC-9). Exit 0 built; 2 usage/malformed input.
"""
import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))

from hhsite import pages, raster, tokparse  # noqa: E402

ASSETS = ("tokens.css", "site.css", "site.js", "glyphs.svg", "favicon.svg")


def tree_sha256(out_dir):
    """sha256 over sorted (relpath, file sha256) pairs — the output-tree
    digest quoted in the GATE-G21 packet."""
    h = hashlib.sha256()
    for f in sorted(Path(out_dir).rglob("*")):
        if f.is_file():
            h.update(str(f.relative_to(out_dir)).encode())
            h.update(b"\0")
            h.update(hashlib.sha256(f.read_bytes()).digest())
            h.update(b"\n")
    return h.hexdigest()


def manifest_text(out_dir):
    """The committed golden record for the all-absent profile:
    'sha256  relpath' per file, sorted, then 'tree  <tree_sha256>'."""
    lines = []
    for f in sorted(Path(out_dir).rglob("*")):
        if f.is_file():
            lines.append("%s  %s" % (hashlib.sha256(f.read_bytes()).hexdigest(),
                                     f.relative_to(out_dir)))
    lines.append("tree  %s" % tree_sha256(out_dir))
    return "\n".join(lines) + "\n"


def build(out_dir, profile="internal"):
    if profile not in ("internal", "all-absent"):
        raise SystemExit("build_mock: unknown profile %r" % profile)
    out = Path(out_dir)
    if out.exists():
        shutil.rmtree(out)
    (out / "assets").mkdir(parents=True)

    tokens = tokparse.parse((ROOT / "assets" / "tokens.css").read_text())
    fixture = json.loads((ROOT / "fixtures" / "phase1.json").read_text())
    harnesses = json.loads((ROOT / "config" / "harnesses.json").read_text())

    for name in ASSETS:
        shutil.copyfile(ROOT / "assets" / name, out / "assets" / name)
    (out / "assets" / "favicon.png").write_bytes(raster.favicon_png(tokens))
    (out / "assets" / "og.png").write_bytes(raster.og_png(tokens))

    written = []
    for path, title, absent, section in pages.URL_MAP:
        if path == "specimen/":
            if profile == "all-absent":
                continue  # internal-only page
            html = pages.specimen_page(fixture, harnesses)
        elif path == "":
            html = pages.home_page(fixture, all_absent=(profile == "all-absent"))
        elif path == "method/":
            html = pages.method_page()
        elif path == "limits/":
            html = pages.limits_page()
        elif path == "changelog/":
            html = pages.changelog_page()
        elif path == "data/":
            html = pages.data_page(fixture)
        elif path == "about/":
            html = pages.about_page()
        else:
            reason, schema, gate = absent
            html = pages._absent_page(path, title, reason, schema, gate,
                                      section)
        target = out / path / "index.html" if path else out / "index.html"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(html)
        written.append(str(target.relative_to(out)))
    return written


def main(argv=None):
    ap = argparse.ArgumentParser(prog="build_mock.py")
    ap.add_argument("--out", required=True, help="output directory")
    ap.add_argument("--profile", default="internal",
                    choices=("internal", "all-absent"))
    ap.add_argument("--manifest", metavar="PATH",
                    help="also write the golden manifest to PATH")
    args = ap.parse_args(argv)
    written = build(args.out, args.profile)
    print("build_mock: %d page(s), tree sha256 %s"
          % (len(written), tree_sha256(args.out)))
    if args.manifest:
        Path(args.manifest).write_text(manifest_text(args.out))
        print("build_mock: manifest %s" % args.manifest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
