#!/usr/bin/env python3
"""bind-hir.py — derive session-openable drive docs from sealed HIR
documents (R7.3, manifest 329): the ws1-l1-driver's `bind_hir` leg,
verbatim, applied to every <name>.json in a docs dir — the recorded
substitutions (model_port placeholder → admitted spellings) land in
<name>.bound.json beside each derived doc; the sealed source is never
written.

usage: bind-hir.py <docs-raw-dir> <docs-out-dir>
"""
import importlib.util
import json
import os
import shutil
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "ws1_l1_driver", os.path.join(_HERE, "ws1-l1-driver.py"))
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)
bind_hir = _mod.bind_hir


def main():
    if len(sys.argv) != 3:
        print("usage: bind-hir.py <docs-raw-dir> <docs-out-dir>",
              file=sys.stderr)
        return 1
    src_d, out_d = sys.argv[1], sys.argv[2]
    os.makedirs(out_d, exist_ok=True)
    for fn in sorted(os.listdir(src_d)):
        if not fn.endswith(".json"):
            continue
        src = os.path.join(src_d, fn)
        dst = os.path.join(out_d, fn)
        subs = bind_hir(src, dst)
        # bind_hir writes <dst>.bound.json with its own source field —
        # keep it verbatim; it is the substitution record.
        print(f"{fn}: {len(subs)} substitutions")
    return 0


if __name__ == "__main__":
    sys.exit(main())
