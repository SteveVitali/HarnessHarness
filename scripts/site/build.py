#!/usr/bin/env python3
"""build.py — R6.5 (F2-b + F2-e) site generator.

Reads only the committed packets named in
``scripts/site/config/inputs.json`` (never globbed — REQ-XF-119),
projects them into the generated, committed ``hh-site-data/1`` layer at
``docs/results/site-data/`` (canonical JSON, integer units, per-value
provenance, derivations D-1…D-8 only — REQ-XF-120/121/122/125), and
renders the phase-1 and always-present pages of §9.2 through the R6.4
shell and components (one renderer — CC1).

    python3 scripts/site/build.py --emit       write site-data in place
    python3 scripts/site/build.py --check      rebuild & byte-compare (drift)
    python3 scripts/site/build.py --out DIR    render the site tree
    python3 scripts/site/build.py --out DIR --profile public
        render the deployable profile: a phase's pages only where a
        committed hh-site-publication/1 record pins its exact input set
        (REQ-XF-128/129); the public profile refuses fixture-labelled
        values, the internal-preview banner, private dataset paths and
        an input-set mismatch (REQ-XF-142)

Deterministic: no wall clock, no network. Exit 0 green; 1 a check or
render failed; 2 usage or malformed input (unknown schema, missing
input, globbed path, hygiene refusal).

Minimum Python: declared floor in ``hhsite.MIN_PYTHON`` (3.9); the stdlib
import audit lives in tests/test_sitedata.py (AC-9.3-1).
"""
import argparse
import json
import sys
from pathlib import Path

SITE = Path(__file__).resolve().parent
ROOT = SITE.parent.parent
sys.path.insert(0, str(SITE))

from hhsite import packets, project, publication, values  # noqa: E402

SITE_DATA_DIR = ROOT / "docs" / "results" / "site-data"
MIN_PYTHON = (3, 9)


def check_python():
    if sys.version_info < MIN_PYTHON:
        raise values.Refusal(
            "python %s below declared floor %s"
            % (sys.version.split()[0], ".".join(str(x) for x in MIN_PYTHON)))


def emit_bodies(repo_root=ROOT):
    """Load named inputs and project every site-data file body.
    Returns {file_name: canonical_json_text}."""
    cfg, inputs = packets.load_inputs(SITE / "config" / "inputs.json",
                                      repo_root)
    digest = packets.inputs_digest(inputs)
    by_id = {i.id: i for i in inputs}
    bodies = {}
    for name, (fn, ids) in sorted(project.FILES.items()):
        bodies[name] = fn([by_id[k] for k in ids])
    bodies["manifest.json"] = project.manifest(inputs, digest)
    bodies["changelog.json"] = project.changelog(
        inputs, values.GENERATOR_VERSION)

    out = {}
    for name, body in bodies.items():
        consumed = [by_id[k] for k in dict(project.FILES)[name][1]] \
            if name in project.FILES else inputs
        env = values.envelope(name, body, packets.input_rows(consumed),
                              digest)
        errs = values.validate(env)
        if errs:
            raise values.Refusal("site-data %s failed schema: %s"
                                 % (name, errs[0]))
        text_ = values.canonical(env)
        values.check_hygiene(text_, name)
        out[name] = text_
    return inputs, digest, out


def run_emit(dest=SITE_DATA_DIR):
    _, _, bodies = emit_bodies()
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    for name in sorted(bodies):
        (dest / name).write_text(bodies[name])
    return sorted(bodies)


def run_check(dest=SITE_DATA_DIR):
    """The drift leg: a rebuild byte-equals the committed layer."""
    _, _, bodies = emit_bodies()
    dest = Path(dest)
    drift = []
    for name in sorted(bodies):
        f = dest / name
        if not f.is_file():
            drift.append("%s (missing)" % name)
        elif f.read_text() != bodies[name]:
            drift.append(name)
    committed = {p.name for p in dest.glob("*.json")} if dest.is_dir() \
        else set()
    for name in sorted(n for n in committed if n not in bodies):
        drift.append("%s (stale)" % name)
    return drift


def load_publications(inputs, repo_root=ROOT):
    """Committed hh-site-publication/1 records, validated. A malformed
    record refuses (fail closed); a record whose pinned input set
    mismatches the current one refuses the public build (REQ-XF-129)."""
    records, errors = publication.load_dir(repo_root)
    for e in errors:
        raise values.Refusal("publication record — %s" % e)
    for rel, doc in records:
        mism = publication.pin_errors(doc, inputs)
        for e in mism:
            raise values.Refusal(
                "publication record %s does not pin the current input "
                "set: %s" % (rel, e))
    return records


def run_out(out_dir, profile="internal"):
    from hhsite import sitepages  # stage-2 renderer (F2-e)
    inputs, digest, bodies = emit_bodies()
    pubs = load_publications(inputs)
    written = sitepages.build(out_dir, inputs, digest, bodies,
                              profile=profile, publications=pubs)
    return written


def main(argv=None):
    ap = argparse.ArgumentParser(prog="build.py")
    ap.add_argument("--emit", action="store_true",
                    help="write hh-site-data/1 into docs/results/site-data/")
    ap.add_argument("--check", action="store_true",
                    help="byte-compare a rebuild against committed site-data")
    ap.add_argument("--out", metavar="DIR",
                    help="render the site tree into DIR")
    ap.add_argument("--site-data", metavar="DIR", default=str(SITE_DATA_DIR),
                    help="site-data directory (default committed layer)")
    ap.add_argument("--profile", default="internal",
                    choices=("internal", "public"),
                    help="render profile: internal (all input content) or "
                         "public (publication-record-gated)")
    args = ap.parse_args(argv)
    try:
        check_python()
        did = False
        if args.emit:
            names = run_emit(args.site_data)
            print("build: emitted %s (%d file(s))"
                  % (args.site_data, len(names)))
            did = True
        if args.check:
            drift = run_check(args.site_data)
            if drift:
                print("build: DRIFT — %s" % ", ".join(drift),
                      file=sys.stderr)
                return 1
            print("build: site-data drift check clean")
            did = True
        if args.out:
            from hhsite import sitepages
            written = run_out(args.out, args.profile)
            print("build: %d page(s) into %s (%s profile), "
                  "tree sha256 %s"
                  % (len(written), args.out, args.profile,
                     sitepages.tree_sha256(args.out)))
            did = True
        if not did:
            ap.print_help()
            return 2
        return 0
    except values.Refusal as e:
        print("build: REFUSED — %s" % e.reason, file=sys.stderr)
        return 2
    except KeyError as e:
        print("build: malformed input — %r" % e, file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
