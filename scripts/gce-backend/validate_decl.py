#!/usr/bin/env python3
# validate_decl.py — hh-compute-decl/1 validator (manifest 266/R6.13;
# spec §3.9, REQ-XB-202). Runs under `python3 -I` — stdlib only.
#
#   validate_decl.py <declaration.json>
#
# Exit 0 when the declaration is schema-valid; exit 2 with a typed
# `Refused{decl.<reason>}` line on stderr otherwise. The field set is
# CLOSED — a missing or unknown field refuses. An image family, an
# unpinned package, a missing dead-man D, a non-IAP ingress rule, a
# service account, or a restart-only declaration over 1 h all refuse.
import json
import re
import sys


def refuse(reason):
    print(f"Refused{{decl.{reason}}}", file=sys.stderr)
    sys.exit(2)


def need(obj, key, where):
    if not isinstance(obj, dict):
        refuse(f"{where}:not-object")
    if key not in obj:
        refuse(f"missing:{where}.{key}")
    return obj[key]


def closed(obj, allowed, where):
    for k in obj:
        if k not in allowed:
            refuse(f"unknown_field:{where}.{k}")


def is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def is_num(v):
    return (isinstance(v, (int, float)) and not isinstance(v, bool)) and v >= 0


def sha256(s):
    return isinstance(s, str) and re.fullmatch(r"[0-9a-f]{64}", s or "") is not None


def pinned(v):
    # an exact version pin: no range operators, globs or whitespace
    return (
        isinstance(v, str)
        and len(v) > 0
        and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9.:_+\-~]*", v) is not None
        and not re.search(r"[<>=|*]|\s", v)
    )


def main():
    if len(sys.argv) != 2:
        print("usage: validate_decl.py <declaration.json>", file=sys.stderr)
        sys.exit(1)
    try:
        with open(sys.argv[1], "r", encoding="utf-8") as f:
            d = json.load(f)
    except Exception as e:  # noqa: BLE001 — typed refusal, never a traceback
        refuse(f"unreadable:{e.__class__.__name__}")

    closed(
        d,
        {
            "schema", "mode", "phase", "window", "ticket", "project_id",
            "region", "zone", "machine_type", "provisioning_model",
            "boot_disk", "image", "packages", "network", "service_account",
            "labels", "dead_man", "restart", "h_r_hours", "runner",
            "proxy_on_vm", "model_line", "images", "disk_floor_gib",
            "concurrency", "rate_table", "authorization", "quote_inputs",
            "phase_close", "last_model_call_utc", "residual",
        },
        "decl",
    )

    if need(d, "schema", "decl") != "hh-compute-decl/1":
        refuse("schema")

    mode = need(d, "mode", "decl")
    if mode not in ("full", "restart-only"):
        refuse("mode")

    phase = need(d, "phase", "decl")
    if not re.fullmatch(r"[a-z0-9]+", str(phase)):
        refuse("phase")
    for k in ("window", "ticket"):
        v = need(d, k, "decl")
        if not isinstance(v, str) or not v:
            refuse(k)

    # REQ-XB-212: fresh project per phase — hh-<phase>-<6 hex>
    pid = need(d, "project_id", "decl")
    if re.fullmatch(rf"hh-{re.escape(str(phase))}-[0-9a-f]{{6}}", str(pid) or "") is None:
        refuse("project_id")

    if need(d, "region", "decl") != "us-central1":
        refuse("region")
    if need(d, "zone", "decl") != "us-central1-a":
        refuse("zone")

    if not isinstance(need(d, "machine_type", "decl"), str):
        refuse("machine_type")
    if need(d, "provisioning_model", "decl") != "STANDARD":
        refuse("provisioning_model")  # Spot excluded (§3.9)

    bd = need(d, "boot_disk", "decl")
    closed(bd, {"type", "size_gib"}, "boot_disk")
    if need(bd, "type", "boot_disk") != "pd-balanced":
        refuse("boot_disk.type")
    if not is_int(need(bd, "size_gib", "boot_disk")) or bd["size_gib"] <= 0:
        refuse("boot_disk.size_gib")

    img = need(d, "image", "decl")
    closed(img, {"project", "name"}, "image")
    for k in ("project", "name"):
        if not isinstance(need(img, k, "image"), str) or not img[k]:
            refuse(f"image.{k}")

    pk = need(d, "packages", "decl")
    closed(pk, {"podman", "crun", "conmon", "netavark", "compose_frontend"}, "packages")
    for k in ("podman", "crun", "conmon", "netavark"):
        if not pinned(need(pk, k, "packages")):
            refuse(f"packages.{k}:unpinned")
    cf = need(pk, "compose_frontend", "packages")
    closed(cf, {"name", "version"}, "packages.compose_frontend")
    if not isinstance(need(cf, "name", "compose_frontend"), str) or not cf["name"]:
        refuse("packages.compose_frontend.name")
    if not pinned(need(cf, "version", "compose_frontend")):
        refuse("packages.compose_frontend:unpinned")

    net = need(d, "network", "decl")
    closed(net, {"external_ipv4", "nat", "ingress"}, "network")
    ext = need(net, "external_ipv4", "network")
    if ext not in ("ephemeral", "none"):
        refuse("network.external_ipv4")
    nat = need(net, "nat", "network")
    if not isinstance(nat, bool):
        refuse("network.nat")
    if (ext, nat) not in (("ephemeral", False), ("none", True)):
        refuse("network.posture")  # declared set: ephemeral, or no-IP + NAT
    ing = need(net, "ingress", "network")
    if not isinstance(ing, list):
        refuse("network.ingress")
    # REQ-XB-203: the one admitted inbound rule is IAP tcp:22
    seen = []
    for rule in ing:
        closed(rule, {"proto", "port", "source"}, "network.ingress[]")
        seen.append(
            (
                need(rule, "proto", "ingress"),
                need(rule, "port", "ingress"),
                need(rule, "source", "ingress"),
            )
        )
    if seen != [("tcp", 22, "35.235.240.0/20")]:
        refuse("network.ingress:not-iap-only")

    if need(d, "service_account", "decl") != "none":
        refuse("service_account")

    lb = need(d, "labels", "decl")
    closed(lb, {"hh-phase", "hh-window", "hh-ticket", "ttl"}, "labels")
    for k in ("hh-phase", "hh-window", "hh-ticket", "ttl"):
        v = need(lb, k, "labels")
        if not isinstance(v, str) or not re.fullmatch(r"[a-z0-9_-]{1,63}", v):
            refuse(f"labels.{k}")

    # REQ-XB-209: dead-man D is mandatory; in-guest timer is D + 15 min
    dm = need(d, "dead_man", "decl")
    closed(dm, {"max_run_duration_hours", "action", "in_guest_timer_minutes"}, "dead_man")
    d_h = need(dm, "max_run_duration_hours", "dead_man")
    if not is_num(d_h) or d_h <= 0:
        refuse("dead_man.max_run_duration")
    if need(dm, "action", "dead_man") not in ("STOP", "DELETE"):
        refuse("dead_man.action")
    if need(dm, "in_guest_timer_minutes", "dead_man") != int(d_h * 60 + 15):
        refuse("dead_man.in_guest_timer")

    hr = need(d, "h_r_hours", "decl")
    if not is_num(hr) or hr <= 0 or hr > 6:
        refuse("h_r_hours")  # REQ-XB-224: declared, max 6 h (first pass)

    rst = need(d, "restart", "decl")
    closed(rst, {"preauthorized_hours", "max_hours"}, "restart")
    if mode == "restart-only":
        # REQ-XB-224 / Q-X60 (pending — recorded default): one harvest-and-
        # teardown-only restart, <= 1 h, no slots, no key delivery.
        mh = need(rst, "max_hours", "restart")
        if not is_num(mh) or mh <= 0 or mh > 1:
            refuse("restart.max_hours")
        if "preauthorized_hours" in rst:
            refuse("restart.restart-only-extra")
    else:
        ph = need(rst, "preauthorized_hours", "restart")
        if not is_num(ph) or ph <= 0 or ph > 1:
            refuse("restart.preauthorized_hours")

    rn = need(d, "runner", "decl")
    closed(rn, {"commit", "build"}, "runner")
    if re.fullmatch(r"[0-9a-f]{40}", str(need(rn, "commit", "runner")) or "") is None:
        refuse("runner.commit")
    if need(rn, "build", "runner") not in ("source", "release"):
        refuse("runner.build")

    if not isinstance(need(d, "proxy_on_vm", "decl"), bool):
        refuse("proxy_on_vm")

    ml = need(d, "model_line", "decl")
    if ml not in ("authorized", "none"):
        refuse("model_line")
    if mode == "restart-only" and ml != "none":
        refuse("model_line:restart-only")

    imgs = need(d, "images", "decl")
    if not isinstance(imgs, list):
        refuse("images")
    if mode == "restart-only" and imgs:
        refuse("images:restart-only")
    for im in imgs:
        closed(im, {"task", "ref", "digest"}, "images[]")
        for k in ("task", "ref"):
            if not isinstance(need(im, k, "images[]"), str) or not im[k]:
                refuse(f"images.{k}")
        dg = need(im, "digest", "images[]")
        if not isinstance(dg, str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", dg):
            refuse("images.digest")  # admitted by digest only (REQ-XB-207)

    if not is_int(need(d, "disk_floor_gib", "decl")) or d["disk_floor_gib"] < 0:
        refuse("disk_floor_gib")

    cc = need(d, "concurrency", "decl")
    closed(
        cc,
        {"memory_gib_available", "memory_per_trial_gib", "vcpu_budget",
         "model_in_flight_limit", "model_wait_fraction"},
        "concurrency",
    )
    for k in ("memory_gib_available", "memory_per_trial_gib", "vcpu_budget",
              "model_in_flight_limit"):
        if not is_num(need(cc, k, "concurrency")) or cc[k] <= 0:
            refuse(f"concurrency.{k}")
    f = need(cc, "model_wait_fraction", "concurrency")
    if not is_num(f) or f <= 0 or f > 1:
        refuse("concurrency.model_wait_fraction")

    rt = need(d, "rate_table", "decl")
    closed(rt, {"path", "sha256"}, "rate_table")
    if not isinstance(need(rt, "path", "rate_table"), str) or not rt["path"]:
        refuse("rate_table.path")
    if not sha256(need(rt, "sha256", "rate_table")):
        refuse("rate_table.sha256")

    au = need(d, "authorization", "decl")
    closed(
        au,
        {"gate", "readout_path", "verbatim_sha256", "quote_id",
         "ceiling_micros", "model_line"},
        "authorization",
    )
    for k in ("gate", "readout_path", "quote_id"):
        if not isinstance(need(au, k, "authorization"), str) or not au[k]:
            refuse(f"authorization.{k}")
    if not sha256(need(au, "verbatim_sha256", "authorization")):
        refuse("authorization.verbatim_sha256")
    if not is_int(need(au, "ceiling_micros", "authorization")) or au["ceiling_micros"] < 0:
        refuse("authorization.ceiling_micros")
    if need(au, "model_line", "authorization") not in ("authorized", "none"):
        refuse("authorization.model_line")
    if au["model_line"] != ml:
        refuse("authorization.model_line:mismatch")

    qi = need(d, "quote_inputs", "decl")
    closed(
        qi,
        {"trials", "surfaces", "per_trial_planning_seconds",
         "per_trial_bound_seconds", "overhead_hours", "egress_gib_estimate",
         # R6.14 / REQ-XH-118 — first-pass terms (optional in the decl;
         # `fpcheck` requires them where the first-pass rule binds)
         "retry_trials", "load_hours", "admission_cutoff_hours",
         "export_size_gib", "work_items"},
        "quote_inputs",
    )
    for k in ("trials", "surfaces"):
        if not is_int(need(qi, k, "quote_inputs")) or qi[k] <= 0:
            refuse(f"quote_inputs.{k}")
    for k in ("per_trial_planning_seconds", "per_trial_bound_seconds",
              "overhead_hours", "egress_gib_estimate"):
        if not is_num(need(qi, k, "quote_inputs")):
            refuse(f"quote_inputs.{k}")
    for k in ("retry_trials",):
        if k in qi and (not is_int(qi[k]) or qi[k] < 0):
            refuse(f"quote_inputs.{k}")
    for k in ("load_hours", "admission_cutoff_hours", "export_size_gib"):
        if k in qi and qi[k] is not None and not is_num(qi[k]):
            refuse(f"quote_inputs.{k}")
    if "work_items" in qi and not (
        isinstance(qi["work_items"], list)
        and all(isinstance(w, str) and w for w in qi["work_items"])
    ):
        refuse("quote_inputs.work_items")

    if not isinstance(need(d, "phase_close", "decl"), bool):
        refuse("phase_close")
    lmc = need(d, "last_model_call_utc", "decl")
    if lmc is not None and not isinstance(lmc, str):
        refuse("last_model_call_utc")

    if "residual" in d:
        res = d["residual"]
        closed(res, {"admit_stopped_disk"}, "residual")
        asd = res["admit_stopped_disk"]
        closed(asd, {"df_row", "owner", "deadline"}, "residual.admit_stopped_disk")
        if not re.fullmatch(r"DF-[A-Za-z0-9.-]+", str(asd.get("df_row") or "")):
            refuse("residual.df_row")
        if not isinstance(asd.get("owner"), str) or not asd["owner"]:
            refuse("residual.owner")
        if not re.fullmatch(r"20\d\d-\d\d-\d\d", str(asd.get("deadline") or "")):
            refuse("residual.deadline")


if __name__ == "__main__":
    main()
