#!/usr/bin/env python3
# validate_receipt.py — hh-compute-receipt/1 validator (manifest 266/R6.13;
# spec §3.9, REQ-XB-208). Runs under `python3 -I` — stdlib only.
#
#   validate_receipt.py <receipt.json> [<receipt.json> ...]
#
# Exit 0 when every named receipt is schema-valid; exit 2 with a typed
# `Refused{receipt.<reason>}` line naming the file otherwise. Field sets
# are CLOSED. `provided` carries yes/no only (REQ-XH-023 — a provisioned
# name, never a value). `resources.project_id` must be the fresh
# `hh-<phase>-<6 hex>` id of the receipt's own phase (REQ-XB-212).
import json
import re
import sys

KINDS = ("provision", "verify", "prepull", "harvest", "teardown")
UTC = re.compile(r"^20\d\d-\d\d-\d\dT\d\d:\d\d(:\d\d(\.\d+)?)?Z$")


def refuse(path, reason):
    print(f"Refused{{receipt.{reason}}} — {path}", file=sys.stderr)
    sys.exit(2)


def need(r, key, where, path):
    if not isinstance(r, dict):
        refuse(path, f"{where}:not-object")
    if key not in r:
        refuse(path, f"missing:{where}.{key}")
    return r[key]


def closed(r, allowed, where, path):
    for k in r:
        if k not in allowed:
            refuse(path, f"unknown_field:{where}.{k}")


def sha(s):
    return isinstance(s, str) and re.fullmatch(r"[0-9a-f]{64}", s) is not None


def is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def utc(s):
    return isinstance(s, str) and UTC.match(s) is not None


def strv(v):
    return isinstance(v, str) and len(v) > 0


def check(path):
    try:
        with open(path, "r", encoding="utf-8") as f:
            r = json.load(f)
    except Exception as e:  # noqa: BLE001 — typed refusal, never a traceback
        refuse(path, f"unreadable:{e.__class__.__name__}")

    closed(
        r,
        {"schema", "kind", "recorded_at", "ticket", "phase", "window",
         "recipe", "declaration", "authorization", "commands", "resources",
         "verification", "post_state", "provided", "settlement",
         "lifecycle", "h_r", "cost_record", "harvest", "prepull",
         "restart", "steps"},
        "receipt", path,
    )

    if need(r, "schema", "receipt", path) != "hh-compute-receipt/1":
        refuse(path, "schema")
    kind = need(r, "kind", "receipt", path)
    if kind not in KINDS:
        refuse(path, f"kind:{kind}")
    if not utc(need(r, "recorded_at", "receipt", path)):
        refuse(path, "recorded_at")
    phase = need(r, "phase", "receipt", path)
    for k in ("ticket", "window"):
        if not strv(need(r, k, "receipt", path)):
            refuse(path, k)

    rec = need(r, "recipe", "receipt", path)
    closed(rec, {"path", "sha256", "git_head"}, "recipe", path)
    if not strv(need(rec, "path", "recipe", path)):
        refuse(path, "recipe.path")
    if not sha(need(rec, "sha256", "recipe", path)):
        refuse(path, "recipe.sha256")
    if re.fullmatch(r"[0-9a-f]{40}|unknown", str(need(rec, "git_head", "recipe", path))) is None:
        refuse(path, "recipe.git_head")

    dc = need(r, "declaration", "receipt", path)
    closed(dc, {"path", "sha256"}, "declaration", path)
    if not strv(need(dc, "path", "declaration", path)):
        refuse(path, "declaration.path")
    if not sha(need(dc, "sha256", "declaration", path)):
        refuse(path, "declaration.sha256")

    au = need(r, "authorization", "receipt", path)
    closed(
        au,
        {"gate", "readout_path", "verbatim_sha256", "quote_id", "ceiling_micros"},
        "authorization", path,
    )
    for k in ("gate", "readout_path", "quote_id"):
        if not strv(need(au, k, "authorization", path)):
            refuse(path, f"authorization.{k}")
    if not sha(need(au, "verbatim_sha256", "authorization", path)):
        refuse(path, "authorization.verbatim_sha256")
    if not is_int(need(au, "ceiling_micros", "authorization", path)):
        refuse(path, "authorization.ceiling_micros")

    cmds = need(r, "commands", "receipt", path)
    if not isinstance(cmds, list):
        refuse(path, "commands")
    for c in cmds:
        closed(c, {"argv_redacted", "started_at", "ended_at", "exit",
                   "stdout_sha256", "stdout_excerpt", "stderr_excerpt"},
               "commands[]", path)
        av = need(c, "argv_redacted", "commands[]", path)
        if not isinstance(av, list) or not all(isinstance(a, str) for a in av):
            refuse(path, "commands.argv_redacted")
        if not utc(need(c, "started_at", "commands[]", path)):
            refuse(path, "commands.started_at")
        if not utc(need(c, "ended_at", "commands[]", path)):
            refuse(path, "commands.ended_at")
        if not is_int(need(c, "exit", "commands[]", path)):
            refuse(path, "commands.exit")
        if not sha(need(c, "stdout_sha256", "commands[]", path)):
            refuse(path, "commands.stdout_sha256")
        for k in ("stdout_excerpt", "stderr_excerpt"):
            x = need(c, k, "commands[]", path)
            if not isinstance(x, str) or len(x.encode("utf-8")) > 4096:
                refuse(path, f"commands.{k}")

    res = need(r, "resources", "receipt", path)
    closed(res, {"project_id", "project_number", "instance_name",
                 "instance_id", "zone", "machine_type",
                 "creation_timestamp", "deletion_operation_end",
                 "disks", "external_ipv4"}, "resources", path)
    pid = need(res, "project_id", "resources", path)
    # REQ-XB-212: the receipt's project must be its own phase's fresh id
    if re.fullmatch(rf"hh-{re.escape(str(phase))}-[0-9a-f]{{6}}", str(pid) or "") is None:
        refuse(path, "resources.project_id:not-phase-fresh")
    if res["project_number"] is not None and not is_int(res["project_number"]):
        refuse(path, "resources.project_number")
    if res["instance_name"] is not None and not isinstance(res["instance_name"], str):
        refuse(path, "resources.instance_name")
    if res["instance_id"] is not None and not is_int(res["instance_id"]):
        refuse(path, "resources.instance_id")
    if res["zone"] is not None and res["zone"] != "us-central1-a":
        refuse(path, "resources.zone")
    if res["machine_type"] is not None and not strv(res["machine_type"]):
        refuse(path, "resources.machine_type")
    if res["creation_timestamp"] is not None and not utc(res["creation_timestamp"]):
        refuse(path, "resources.creation_timestamp")
    if res["deletion_operation_end"] is not None and not utc(res["deletion_operation_end"]):
        refuse(path, "resources.deletion_operation_end")
    if need(res, "external_ipv4", "resources", path) not in ("present", "absent"):
        refuse(path, "resources.external_ipv4")
    for dk in need(res, "disks", "resources", path):
        closed(dk, {"name", "type", "size_gib", "created", "deleted"},
               "resources.disks[]", path)
        if not strv(need(dk, "name", "disks[]", path)):
            refuse(path, "disks.name")
        for k in ("type", "size_gib"):
            if need(dk, k, "disks[]", path) is not None and not isinstance(dk[k], (str, int)):
                refuse(path, f"disks.{k}")
        for k in ("created", "deleted"):
            if need(dk, k, "disks[]", path) is not None and not utc(dk[k]):
                refuse(path, f"disks.{k}")

    if need(r, "verification", "receipt", path) is not None and not isinstance(r["verification"], dict):
        refuse(path, "verification")

    # `provided` reports names only — yes/no, never a value (REQ-XH-023)
    pv = need(r, "provided", "receipt", path)
    if not isinstance(pv, dict):
        refuse(path, "provided")
    for name, v in pv.items():
        if v not in ("yes", "no"):
            refuse(path, f"provided.{name}:value")

    if kind == "teardown":
        ps = need(r, "post_state", "receipt", path)
        if not isinstance(ps, dict) or not ps:
            refuse(path, "post_state")
        for cls, v in ps.items():
            if cls in ("admitted_deviation", "vm_final"):
                continue
            if not (is_int(v) or (isinstance(v, str) and v.startswith("inconclusive{"))):
                refuse(path, f"post_state.{cls}")
        st = need(r, "settlement", "receipt", path)
        closed(st, {"last_model_call_utc_hour"}, "settlement", path)
        lc = need(r, "lifecycle", "receipt", path)
        if not isinstance(lc, dict):
            refuse(path, "lifecycle")
        h = need(r, "h_r", "receipt", path)
        closed(h, {"declared_hours", "elapsed_hours", "exceeded",
                   "requires_dated_df_row"}, "h_r", path)
        if not isinstance(need(h, "exceeded", "h_r", path), bool):
            refuse(path, "h_r.exceeded")
        if not isinstance(need(h, "requires_dated_df_row", "h_r", path), bool):
            refuse(path, "h_r.requires_dated_df_row")
        if h["exceeded"] and not h["requires_dated_df_row"]:
            refuse(path, "h_r.exceeded-without-df-flag")
        cr = need(r, "cost_record", "receipt", path)
        if not (isinstance(cr, str) and cr.startswith("n/a{") and cr.endswith("}")):
            refuse(path, "cost_record")  # R6.14 owns hh-compute-cost/1
    elif "post_state" in r:
        refuse(path, "post_state:teardown-only")


def main():
    if len(sys.argv) < 2:
        print("usage: validate_receipt.py <receipt.json> [...]", file=sys.stderr)
        sys.exit(1)
    for p in sys.argv[1:]:
        check(p)


if __name__ == "__main__":
    main()
