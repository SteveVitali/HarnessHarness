#!/usr/bin/env python3
"""Emit the gitleaks config for CI's repo-wide (`--log-opts=--all`) scan.

    python3 scripts/ci/gitleaks-union-config.py DEFAULT_CFG HEAD_CFG > union.toml

DEFAULT_CFG is the default branch's .gitleaks.toml; HEAD_CFG is the
checked-out head's. The `--all` scan walks every branch, not just this head,
so a head whose own config predates an allowlist fix that already landed on
the default branch must not go red on some other branch's history. The
output is HEAD_CFG with its `[extend]` table replaced by
`path = DEFAULT_CFG`; gitleaks 8.30.1 then
  * takes DEFAULT_CFG's rules (and, through its `useDefault`, the built-in
    set) and adds or overrides them with HEAD_CFG's own `[[rules]]`;
  * appends DEFAULT_CFG's `[[allowlists]]` to HEAD_CFG's.
So the scan uses the union of both allowlists, plus default ∪ head rules.
A head that legitimately allowlists its own content still passes, and a head
can never drop an allowlist or a rule that the default branch carries.

HEAD_CFG's `disabledRules` carry over (a head could already disable rules for
this scan before). `extend.path` in either file is refused (fail closed): this
wrapper is the one level of `path` chaining, and a relative path would
resolve against the wrong directory.
"""
import json
import os
import re
import sys
import tomllib

BARE_KEY = re.compile(r"^[A-Za-z0-9_-]+$")


def key(k):
    return k if BARE_KEY.match(k) else json.dumps(k, ensure_ascii=False)


def scalar(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return repr(v)
    if isinstance(v, str):
        # JSON string escapes are a subset of TOML basic-string escapes;
        # TOML additionally forbids a raw DEL.
        return json.dumps(v, ensure_ascii=False).replace("\x7f", "\\u007f")
    if isinstance(v, list):
        return "[" + ", ".join(scalar(x) for x in v) + "]"
    raise SystemExit(f"gitleaks-union-config: unsupported TOML value {v!r}")


def is_table_array(v):
    return isinstance(v, list) and v and all(isinstance(x, dict) for x in v)


def emit(table, prefix=""):
    lines = [f"{key(k)} = {scalar(v)}" for k, v in table.items()
             if not isinstance(v, dict) and not is_table_array(v)]
    for k, v in table.items():
        name = prefix + key(k)
        if isinstance(v, dict):
            lines += ["", f"[{name}]"] + emit(v, name + ".")
        elif is_table_array(v):
            for item in v:
                lines += ["", f"[[{name}]]"] + emit(item, name + ".")
    return lines


def main():
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    default_path, head_path = (os.path.abspath(p) for p in sys.argv[1:])
    with open(default_path, "rb") as f:
        default_cfg = tomllib.load(f)
    with open(head_path, "rb") as f:
        head_cfg = tomllib.load(f)
    for name, cfg in (("default-branch", default_cfg), ("head", head_cfg)):
        if "path" in cfg.get("extend", {}):
            raise SystemExit(f"gitleaks-union-config: the {name} config sets "
                             "extend.path; unsupported, update this script")
    extend = {"path": default_path}
    if head_cfg.get("extend", {}).get("disabledRules"):
        extend["disabledRules"] = head_cfg["extend"]["disabledRules"]
    union = {k: v for k, v in head_cfg.items() if k != "extend"}
    union["extend"] = extend
    out = "\n".join(emit(union)) + "\n"
    if tomllib.loads(out) != union:  # fail closed on any emitter gap
        raise SystemExit("gitleaks-union-config: emitted TOML does not round-trip")
    sys.stdout.write(out)


if __name__ == "__main__":
    main()
