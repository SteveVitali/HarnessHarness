#!/usr/bin/env python3
# receipt_lib.py — python3 -I helpers for scripts/gce-backend.sh
# (manifest 266/R6.13; spec §3.9/§3.10 inputs; REQ-XB-201/208/213-inputs;
# REQ-XD-051's `pre-quote` label). Stdlib only; deterministic output.
#
#   receipt_lib.py command <jsonl> <started> <ended> <exit> <out> <err> -- <argv...>
#       append one commands[] record (sha256 + <=4 KiB excerpts)
#   receipt_lib.py build <base.json> <commands.jsonl>
#       print the merged receipt JSON (commands[] inserted verbatim)
#   receipt_lib.py jget <file.json> <dotted.path>
#       print the scalar at path, or its JSON; exit 3 on a missing member
#   receipt_lib.py count
#       read a JSON list on stdin; print its length (or -1 when not a list)
#   receipt_lib.py quote <decl.json>
#       print the hh-compute-pre-quote/1 document — declaration quantities
#       only, labelled `pre-quote`; no money is computed (R6.14's).
#   receipt_lib.py quoteid <decl.json>
#       print pq-<16 hex> — the deterministic pre-quote id.
import hashlib
import json
import sys


def sha256_file(path):
    h = hashlib.sha256()
    try:
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(65536), b""):
                h.update(chunk)
    except OSError:
        return "0" * 64
    return h.hexdigest()


def sha256_text(s):
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def canon(obj):
    return json.dumps(obj, sort_keys=True, separators=(",", ":"))


def jwalk(doc, path):
    cur = doc
    for part in path.split("."):
        if not isinstance(cur, dict) or part not in cur:
            return None, False
        cur = cur[part]
    return cur, True


def redact_arg(a):
    # argv never carries a secret by construction (deliver-secret streams
    # stdin); this mask is defence-in-depth for =joined credential flags.
    for tok in ("token", "secret", "key", "password"):
        for sep in ("=", ":"):
            p = a.lower().find(tok + sep)
            if p >= 0:
                return a[: p + len(tok) + 1] + "<redacted>"
    return a


def cmd_append(argv):
    jsonl, started, ended, rc, outp, errp = argv[2:8]
    args = argv[9:]
    try:
        out_ex = open(outp, "r", encoding="utf-8", errors="replace").read(4097)[:4096]
    except OSError:
        out_ex = ""
    try:
        err_ex = open(errp, "r", encoding="utf-8", errors="replace").read(4097)[:4096]
    except OSError:
        err_ex = ""
    rec = {
        "argv_redacted": [redact_arg(a) for a in args],
        "started_at": started,
        "ended_at": ended,
        "exit": int(rc),
        "stdout_sha256": sha256_file(outp),
        "stdout_excerpt": out_ex,
        "stderr_excerpt": err_ex,
    }
    with open(jsonl, "a", encoding="utf-8") as f:
        f.write(json.dumps(rec, sort_keys=True) + "\n")


def cmd_build(argv):
    base_p, jsonl = argv[2:4]
    with open(base_p, "r", encoding="utf-8") as f:
        base = json.load(f)
    cmds = []
    try:
        with open(jsonl, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line:
                    cmds.append(json.loads(line))
    except OSError:
        pass
    base["commands"] = cmds
    print(json.dumps(base, indent=2, sort_keys=True))


def cmd_jget(argv):
    with open(argv[2], "r", encoding="utf-8") as f:
        doc = json.load(f)
    val, ok = jwalk(doc, argv[3])
    if not ok:
        sys.exit(3)
    if isinstance(val, (dict, list)):
        print(json.dumps(val, sort_keys=True))
    elif val is None:
        print("null")
    elif isinstance(val, bool):
        print("true" if val else "false")
    else:
        print(val)


def cmd_count(_argv=None):
    try:
        doc = json.load(sys.stdin)
    except Exception:  # noqa: BLE001
        print(-1)
        return
    print(len(doc) if isinstance(doc, list) else -1)


def quote_doc(decl):
    cc = decl["concurrency"]
    qi = decl["quote_inputs"]
    dm = decl["dead_man"]
    # REQ-XB-214 inputs: C = min(memory packing, vCPU budget, ceil(K/f))
    import math

    c_mem = math.floor(cc["memory_gib_available"] / cc["memory_per_trial_gib"])
    c_vcpu = cc["vcpu_budget"]
    c_model = math.ceil(cc["model_in_flight_limit"] / cc["model_wait_fraction"])
    c_val = int(min(c_mem, c_vcpu, c_model))
    restart_h = (
        decl["restart"]["max_hours"]
        if decl.get("mode") == "restart-only"
        else decl["restart"]["preauthorized_hours"]
    )
    return {
        "schema": "hh-compute-pre-quote/1",
        "label": "pre-quote",
        "phase": decl["phase"],
        "window": decl["window"],
        "ticket": decl["ticket"],
        "quantities": {
            "machine_type": decl["machine_type"],
            "provisioning_model": decl["provisioning_model"],
            "disk_gib": decl["boot_disk"]["size_gib"],
            "disk_type": decl["boot_disk"]["type"],
            "external_ipv4": decl["network"]["external_ipv4"],
            "nat": decl["network"]["nat"],
            "trials": qi["trials"],
            "surfaces": qi["surfaces"],
            "per_trial_planning_seconds": qi["per_trial_planning_seconds"],
            "per_trial_bound_seconds": qi["per_trial_bound_seconds"],
            "concurrency": {
                "value": c_val,
                "rule": "min(memory_packing, vcpu_budget, ceil(K/f))",
                "inputs": {
                    "memory_gib_available": cc["memory_gib_available"],
                    "memory_per_trial_gib": cc["memory_per_trial_gib"],
                    "vcpu_budget": c_vcpu,
                    "model_in_flight_limit": cc["model_in_flight_limit"],
                    "model_wait_fraction": cc["model_wait_fraction"],
                },
            },
            "overhead_hours": qi["overhead_hours"],
            "egress_gib": {"value": qi["egress_gib_estimate"], "label": "estimated"},
            "dead_man_hours": dm["max_run_duration_hours"],
            "residual_disk_hours": decl["h_r_hours"],
            "restart_hours": restart_h,
        },
        "rate_table": {
            "path": decl["rate_table"]["path"],
            "sha256": decl["rate_table"]["sha256"],
        },
        # the §3.10 template symbols → the declaration quantities they take
        "symbols": {
            "N": qi["trials"], "s": qi["surfaces"],
            "t_p": qi["per_trial_planning_seconds"],
            "t_b": qi["per_trial_bound_seconds"],
            "C": c_val, "H_o": qi["overhead_hours"],
            "E_gib": {"value": qi["egress_gib_estimate"], "label": "estimated"},
            "D": dm["max_run_duration_hours"],
            "h_r": decl["h_r_hours"], "H_rs": restart_h,
        },
        # REQ-XD-051: declaration quantities only until AR-13/R6.14 price them
        "pricing": "n/a{compute_pricing: R6.14}",
    }


def cmd_quote(argv):
    with open(argv[2], "r", encoding="utf-8") as f:
        decl = json.load(f)
    print(json.dumps(quote_doc(decl), indent=2, sort_keys=True))


def cmd_quoteid(argv):
    with open(argv[2], "r", encoding="utf-8") as f:
        decl = json.load(f)
    print("pq-" + sha256_text(canon(quote_doc(decl)))[:16])


def fail_typed(kind, reason, rc=2):
    print(f"{kind}{{{reason}}}", file=sys.stderr)
    sys.exit(rc)


def cmd_authcheck(argv):
    # authcheck <decl> <authorization-record.json> <quote-id>
    # A presented gate authorization must match the declaration's refs AND
    # the deterministic pre-quote id — a quote-id or ceiling mismatch
    # refuses (REQ-XB-209; §10.3 authorization verbatim).
    with open(argv[2], "r", encoding="utf-8") as f:
        decl = json.load(f)
    try:
        with open(argv[3], "r", encoding="utf-8") as f:
            auth = json.load(f)
    except Exception as e:  # noqa: BLE001
        fail_typed("Refused", f"authorization.unreadable:{e.__class__.__name__}")
    want = decl["authorization"]
    for k in ("gate", "readout_path", "verbatim_sha256",
              "quote_id", "ceiling_micros", "model_line"):
        if k not in auth:
            fail_typed("Refused", f"authorization.missing:{k}")
        if auth[k] != want[k]:
            fail_typed("Refused", f"authorization.{k}")
    if auth["quote_id"] != argv[4]:
        fail_typed("Refused", "authorization.quote_id:recompute")
    print("authorization: match")


def cmd_describe(_argv=None):
    # describe — read an `instances describe --format json` doc on stdin and
    # print the resource facts the recipe/receipt consume; "{}" on bad input
    try:
        doc = json.load(sys.stdin)
    except Exception:  # noqa: BLE001
        print("{}")
        return
    mrd = ((doc.get("scheduling") or {}).get("maxRunDuration") or {}).get("seconds")
    ext = "absent"
    for ni in doc.get("networkInterfaces") or []:
        if ni.get("accessConfigs"):
            ext = "present"
    print(json.dumps({
        "name": doc.get("name"),
        "id": int(doc["id"]) if doc.get("id") is not None else None,
        "status": doc.get("status"),
        "creation_timestamp": doc.get("creationTimestamp"),
        "last_stop_timestamp": doc.get("lastStopTimestamp"),
        "max_run_seconds": int(mrd) if mrd is not None else None,
        "service_accounts": doc.get("serviceAccounts") or [],
        "external_ipv4": ext,
    }, sort_keys=True))


def cmd_fwrule(argv):
    # fwrule — read a `firewall-rules list --format json` doc on stdin and
    # require the rule set to be exactly the IAP tcp:22 rule (REQ-XB-203).
    import os
    iap = os.environ.get("HH_IAP_CIDR", "35.235.240.0/20")
    doc = json.load(sys.stdin)
    rules = []
    for r in doc if isinstance(doc, list) else []:
        for a in r.get("allowed") or []:
            for port in a.get("ports") or [None]:
                for src in r.get("sourceRanges") or []:
                    rules.append((a.get("IPProtocol"), int(port), src))
    if rules != [("tcp", 22, iap)]:
        fail_typed("Unsupported", f"backend.ingress:{rules!r}", rc=4)
    print("ingress: iap-only")


def cmd_pkgs(argv):
    # pkgs <decl> — the name=version pin line for startup metadata
    with open(argv[2], "r", encoding="utf-8") as f:
        p = json.load(f)["packages"]
    parts = [f"{k}={p[k]}" for k in ("podman", "crun", "conmon", "netavark")]
    parts.append(f"{p['compose_frontend']['name']}={p['compose_frontend']['version']}")
    print(" ".join(parts))


def cmd_images(argv):
    # images <decl> — "task<TAB>ref<TAB>digest" lines over the admitted set
    with open(argv[2], "r", encoding="utf-8") as f:
        decl = json.load(f)
    for im in decl.get("images") or []:
        print("\t".join((im["task"], im["ref"], im["digest"])))


OPS = {
    "command": cmd_append,
    "build": cmd_build,
    "jget": cmd_jget,
    "count": cmd_count,
    "quote": cmd_quote,
    "quoteid": cmd_quoteid,
    "authcheck": cmd_authcheck,
    "describe": cmd_describe,
    "fwrule": cmd_fwrule,
    "pkgs": cmd_pkgs,
    "images": cmd_images,
}


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in OPS:
        print(__doc__.strip().splitlines()[1] if __doc__ else "usage", file=sys.stderr)
        sys.exit(1)
    OPS[sys.argv[1]](sys.argv)


if __name__ == "__main__":
    main()
