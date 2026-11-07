#!/usr/bin/env python3
# cost_lib.py — R6.14 spend machinery (manifest 267; spec §3.10/§10.1;
# REQ-XB-215/216/217, REQ-XH-006/118; ADR-0363 D4 posture; AR-14).
# Stdlib only; runs under `python3 -I`.
#
#   cost_lib.py costrecord <decl> <teardown-receipt> <provision-receipt|->
#       <reports-dir> <repo> [--slot-wall-ms N]
#       write the window's hh-compute-cost/1 with billed: pending —
#       intervals from the teardown receipt's lifecycle timestamps,
#       never run windows; a second record for the window refuses.
#   cost_lib.py costvalidate <cost.json> <teardown-receipt> <repo>
#       recompute every estimate line + total_micros from the pinned
#       rate table; intervals must equal the receipt's lifecycle stamps.
#   cost_lib.py reconcile <cost-record> <billing.json|-> <reports-dir> <repo>
#       append one dated reconcile record (supersedes_estimate_of) +
#       the billed spend row; statuses pending|unavailable{reason}|
#       within|outside (outside needs explanation; totals then billed).
#   cost_lib.py spend-append <ledger.jsonl> <row.json>
#   cost_lib.py spend-validate <ledger.jsonl>
#   cost_lib.py spend-fold <ledger.jsonl>
#       the one phase spend ledger — ADR-0363 row shape plus
#       kind/authority/basis/phase/window (REQ-XB-217, REQ-XH-006).
#   cost_lib.py fpcheck <repo> <decl.json>... [--export-measured-gib F]
#       REQ-XH-118 first-pass declaration/quote checks.
import datetime
import hashlib
import importlib.util
import json
import os
import sys

_SELF = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "receipt_lib", os.path.join(_SELF, "receipt_lib.py"))
_lib = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_lib)
Fraction = _lib.Fraction

UTC = "%Y-%m-%dT%H:%M:%SZ"
TOL_MICROS = 500_000            # max($0.50, 10 %) — REQ-XB-216
SPEND_KINDS = ("model", "compute")
AUTHORITIES = ("ollama_usage", "gcp_billing")
BASES = ("estimate", "billed")
# REQ-XH-006 — one authority per kind; never cross.
KIND_AUTHORITY = {"model": "ollama_usage", "compute": "gcp_billing"}
FP_SUM_CAP_MICROS = 20_000_000  # FP-9 / REQ-XH-118: Σ ceilings ≤ $20
W1_ITEMS = {"observe_pass", "rust_build", "admission_build",
            "layer_export", "census", "smoke_battery"}
W3_ITEMS = {"hset_build", "export"}


def fail(kind, reason, rc=2):
    print(f"{kind}{{{reason}}}", file=sys.stderr)
    sys.exit(rc)


def refuse(reason):
    fail("Refused", reason)


def canon(o):
    return json.dumps(o, sort_keys=True, separators=(",", ":"))


def sha_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for c in iter(lambda: f.read(65536), b""):
            h.update(c)
    return h.hexdigest()


def load(p):
    with open(p, "r", encoding="utf-8") as f:
        return json.load(f)


def jdump(o):
    return json.dumps(o, indent=2, sort_keys=True)


def parse_utc(s):
    return datetime.datetime.strptime(s[:19], "%Y-%m-%dT%H:%M:%S")


def secs_between(a, b):
    return max(0.0, (parse_utc(b) - parse_utc(a)).total_seconds())


def ceil_micros(fr):
    return _lib._ceil_frac(fr)


def window_of(rec):
    return (rec.get("phase"), rec.get("window"))


def cost_records(reports, phase, window):
    out = []
    if not os.path.isdir(reports):
        return out
    for n in sorted(os.listdir(reports)):
        if not n.endswith(".json") or "-cost" not in n:
            continue
        try:
            r = load(os.path.join(reports, n))
        except Exception:
            continue
        if r.get("schema") == "hh-compute-cost/1" and window_of(r) == (phase, window):
            out.append((os.path.join(reports, n), r))
    return out


def merged_lifecycle(receipt, prov):
    """Lifecycle stamps for the estimate: creation from the provision
    receipt's resource facts (the VM cannot be described post-delete),
    stop/delete ends from the teardown receipt's lifecycle member."""
    lc = dict(receipt.get("lifecycle") or {})
    if prov is not None:
        res = prov.get("resources") or {}
        if res.get("creation_timestamp") and not lc.get("creation_timestamp"):
            lc["creation_timestamp"] = res["creation_timestamp"]
    return lc


def cost_estimate(decl, lifecycle, rate_rows, slot_wall_ms):
    """The five §3.10 estimate lines over lifecycle intervals (never run
    windows). billable_seconds = max(60, s) per running interval."""
    F = Fraction
    created = lifecycle.get("creation_timestamp")
    stop = lifecycle.get("last_stop_timestamp")
    end = lifecycle.get("deletion_operation_end")
    if not created or not end:
        refuse("lifecycle.timestamps")
    # running interval: creation → stop (STOP action) or deletion
    run_to = stop or end
    run_s = secs_between(created, run_to)
    bill_s = max(60.0, run_s)
    wall_s = secs_between(created, end)
    mt = decl["machine_type"]
    s_gib = decl["boot_disk"]["size_gib"]
    disk_t = decl["boot_disk"]["type"]
    r_vm = rate_rows[mt]["price_micros"]
    r_disk = rate_rows[disk_t]["price_micros"]
    lines = {}
    lines["instance"] = {
        "machine_type": mt,
        "intervals": [{"state": "RUNNING", "from": created, "to": run_to}],
        "seconds": round(run_s, 3),
        "billable_seconds": round(bill_s, 3),
        "micros": ceil_micros(F(r_vm) * F(str(bill_s)) / 3600),
    }
    # disk bills for the whole retained span (incl. residual h_r)
    gib_s = s_gib * wall_s
    lines["disk"] = {
        "type": disk_t,
        "size_gib": s_gib,
        "intervals": [{"from": created, "to": end}],
        "gib_seconds": round(gib_s, 3),
        "micros": ceil_micros(F(r_disk) * F(str(gib_s)) / 3600),
    }
    if decl["network"]["external_ipv4"] == "ephemeral":
        lines["ipv4"] = {
            "kind": "ephemeral",
            "seconds": round(bill_s, 3),
            "micros": ceil_micros(
                F(rate_rows["ipv4"]["price_micros"]) * F(str(bill_s)) / 3600),
        }
    else:
        lines["ipv4"] = {"kind": "none", "seconds": 0, "micros": 0}
    e_gib = F(str(decl["quote_inputs"]["egress_gib_estimate"]))
    lines["egress"] = {
        "gib_measured": None,
        "gib_bound": float(e_gib),
        "method": "declared estimate — guest NIC counters unrecorded",
        "label": "estimated",
        "micros": ceil_micros(F(rate_rows["egress"]["price_micros"]) * e_gib),
    }
    if decl["network"]["nat"]:
        lines["nat"] = {
            "present": True,
            "seconds": round(bill_s, 3),
            "micros": ceil_micros(
                F(rate_rows["nat_gateway"]["price_micros"])
                * F(str(bill_s)) / 3600),
        }
    else:
        lines["nat"] = {"present": False, "seconds": 0, "micros": 0}
    total = sum(l["micros"] for l in lines.values())
    util = 0
    if wall_s > 0:
        util = min(1_000_000, int(round(slot_wall_ms / (wall_s * 1000.0) * 1_000_000)))
    return lines, total, util


def cmd_costrecord(argv):
    decl = load(argv[2])
    receipt = load(argv[3])
    prov = None if argv[4] == "-" else load(argv[4])
    reports, repo = argv[5], argv[6]
    slot_ms = 0
    if len(argv) > 8 and argv[7] == "--slot-wall-ms":
        slot_ms = int(argv[8])
    phase, window = decl["phase"], decl["window"]
    if receipt.get("kind") != "teardown":
        refuse("receipt.not_teardown")
    if window_of(receipt) != (phase, window):
        refuse("receipt.window_mismatch")
    if prov is None:
        refuse("receipt.provision_required")
    existing = cost_records(reports, phase, window)
    if any("supersedes_estimate_of" not in r for _, r in existing):
        refuse(f"cost_record.rewrite:{phase}-{window}")
    rate_rows, table = _lib.load_rate_rows(decl, repo)
    for sku in (decl["machine_type"], decl["boot_disk"]["type"], "egress"):
        if sku not in rate_rows:
            refuse(f"rate_table.missing:{sku}")
    lifecycle = merged_lifecycle(receipt, prov)
    lines, total, util = cost_estimate(decl, lifecycle, rate_rows, slot_ms)
    receipts = {
        "provision": {"path": argv[4], "sha256": sha_file(argv[4])},
        # the teardown receipt is cited by path only — the same step-9
        # transaction patches its cost_record member back to this record,
        # so a stored sha would go stale by construction; readers recompute.
        "teardown": {"path": argv[3]},
    }
    rec = {
        "schema": "hh-compute-cost/1",
        "phase": phase,
        "window": window,
        "ticket": decl["ticket"],
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).strftime(UTC),
        "provider": "gce",
        "project_id": decl["project_id"],
        "zone": decl["zone"],
        "receipts": receipts,
        "quote": {
            "id": _lib.sha256_text(canon(_lib.quote_doc(decl, repo)))[:16],
            "ceiling_micros": decl["authorization"]["ceiling_micros"],
            "dead_man_h": decl["dead_man"]["max_run_duration_hours"],
        },
        "rate_table": {
            "schema": "hh-live-pricing/2",
            "path": decl["rate_table"]["path"],
            "sha256": decl["rate_table"]["sha256"],
            "rows": [rate_rows[k] for k in sorted(rate_rows)],
        },
        "usage": lines,
        "estimate": {
            "lines_micros": {k: v["micros"] for k, v in lines.items()},
            "total_micros": total,
            "rounding": "ceil per line",
            "free_tier_applied": False,
        },
        "attribution": {
            "slot_wall_ms": slot_ms,
            "utilization_ppm": util,
            "idle_seconds": round(
                max(0.0, secs_between(lifecycle["creation_timestamp"], lifecycle["deletion_operation_end"]) - slot_ms / 1000.0), 3),
        },
        "billed": {"status": "pending", "reason": None,
                   "amount_micros": None,
                   "source": "Cloud Billing report, project filter, read by operator",
                   "read_at": None, "record": None},
        "reconciliation": {"delta_micros": None, "tolerance_micros": None,
                           "verdict": "pending"},
        "provided": receipt.get("provided") or {},
    }
    rec_id = "cc-" + _lib.sha256_text(canon(rec))[:16]
    rec["record_id"] = rec_id
    name = "%s-%s-%s-cost.json" % (
        datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d"),
        phase, window)
    path = os.path.join(reports, name)
    if os.path.exists(path):
        refuse(f"cost_record.rewrite:{name}")
    os.makedirs(reports, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(jdump(rec) + "\n")
    print(path)


def cmd_costvalidate(argv):
    rec = load(argv[2])
    receipt = load(argv[3])
    repo = argv[4]
    if rec.get("schema") != "hh-compute-cost/1":
        refuse("cost.schema")
    # reconciliation appends carry supersedes_estimate_of; the base record
    # must not — the record is never rewritten in place (REQ-XB-215/216).
    base = "supersedes_estimate_of" not in rec
    if receipt.get("kind") != "teardown":
        refuse("receipt.not_teardown")
    decl_path = None
    # recompute needs the declaration; it rides the receipt's declaration ref
    decl_path = receipt.get("declaration", {}).get("path")
    if not decl_path or not os.path.exists(decl_path):
        refuse("declaration.absent")
    decl = load(decl_path)
    prov_p = rec.get("receipts", {}).get("provision", {}).get("path")
    prov = load(prov_p) if prov_p and os.path.exists(prov_p) else None
    rate_rows, _ = _lib.load_rate_rows(decl, repo)
    lifecycle = merged_lifecycle(receipt, prov)
    lines, total, _u = cost_estimate(decl, lifecycle, rate_rows,
                                     (rec.get("attribution") or {}).get("slot_wall_ms", 0))
    if base:
        got = rec.get("estimate", {})
        for k, v in lines.items():
            if got.get("lines_micros", {}).get(k) != v["micros"]:
                refuse(f"estimate.lines_micros.{k}")
            iv = rec.get("usage", {}).get(k, {})
            if "intervals" in v and iv.get("intervals") != v["intervals"]:
                refuse(f"usage.{k}.intervals")
            if k == "instance" and iv.get("billable_seconds") != v["billable_seconds"]:
                refuse("usage.instance.billable_seconds")
        if got.get("total_micros") != total:
            refuse("estimate.total_micros")
        for k in ("instance", "disk", "ipv4", "egress", "nat"):
            if k not in (rec.get("usage") or {}):
                refuse(f"usage.missing:{k}")
        if (rec.get("billed") or {}).get("status") != "pending":
            refuse("billed.not_pending")
    else:
        # a reconcile append: verdict must be one of the closed set and an
        # `outside` verdict needs its explanation (REQ-XB-216).
        st = (rec.get("billed") or {}).get("status")
        if st not in ("within", "outside"):
            refuse(f"billed.status:{st}")
        recn = rec.get("reconciliation") or {}
        if st == "outside" and not recn.get("explanation"):
            refuse("reconciliation.explanation_absent")
    print("cost record: valid")


def cmd_reconcile(argv):
    cost_p, billing_p, reports, repo = argv[2], argv[3], argv[4], argv[5]
    rec = load(cost_p)
    if rec.get("schema") != "hh-compute-cost/1" or "supersedes_estimate_of" in rec:
        refuse("reconcile.target")
    now = datetime.datetime.now(datetime.timezone.utc).strftime(UTC)
    if billing_p == "-":
        status, amount, reason = "unavailable", None, "no_billing_read"
    else:
        b = load(billing_p)
        amount = b.get("amount_micros")
        reason = None
        if amount is None:
            status = "unavailable"
            reason = b.get("reason") or "billing_read_absent"
        else:
            est = rec["estimate"]["total_micros"]
            delta = amount - est
            tol = max(TOL_MICROS, est // 10)
            status = "within" if abs(delta) <= tol else "outside"
    est = rec["estimate"]["total_micros"]
    delta = None if amount is None else amount - est
    tol = None if amount is None else max(TOL_MICROS, est // 10)
    explanation = None
    if billing_p != "-" and status == "outside":
        explanation = load(billing_p).get("explanation")
        if not explanation:
            refuse("reconciliation.explanation_required")
    new = dict(rec)
    new["supersedes_estimate_of"] = rec.get("record_id")
    new["recorded_at"] = now
    new["billed"] = {
        "status": status,
        "reason": reason,
        "amount_micros": amount,
        "source": "Cloud Billing report, project filter, read by operator",
        "read_at": now,
        "record": None if billing_p == "-" else billing_p,
    }
    new["reconciliation"] = {
        "delta_micros": delta,
        "tolerance_micros": tol,
        "verdict": status if status != "unavailable" else "unavailable",
        **({"explanation": explanation} if explanation else {}),
    }
    if status == "unavailable":
        new["billed"]["status"] = "unavailable"
        new["reconciliation"]["verdict"] = "unavailable"
        new["reconciliation"]["reason"] = reason
    new["record_id"] = "cc-" + _lib.sha256_text(canon(new))[:16]
    name = "%s-%s-%s-cost-reconcile.json" % (
        now[:10], rec["phase"], rec["window"])
    path = os.path.join(reports, name)
    with open(path, "w", encoding="utf-8") as f:
        f.write(jdump(new) + "\n")
    # the phase spend ledger's billed row supersedes the estimate by
    # reference (REQ-XB-217 rule 2)
    ledger = os.environ.get(
        "HH_PHASE_SPEND",
        os.path.join(repo, "docs/build/reports/live/PHASE_SPEND.jsonl"))
    if os.path.exists(ledger) and status in ("within", "outside"):
        est_rows = [r for r in ledger_rows(ledger)
                    if r.get("type") == "spend" and r.get("kind") == "compute"
                    and r.get("basis") == "estimate"
                    and r.get("phase") == rec["phase"]
                    and r.get("window") == rec["window"]]
        if est_rows:
            row = {
                "type": "spend", "ts": now, "ticket": rec["ticket"],
                "leg": "compute-reconcile", "cost_usd": amount / 1e6,
                "record": os.path.basename(path),
                "kind": "compute", "authority": "gcp_billing",
                "basis": "billed", "phase": rec["phase"],
                "window": rec["window"],
                "supersedes": est_rows[-1]["row_id"],
            }
            spend_append_rows(ledger, [row])
    print(path)


# ── phase spend ledger (ADR-0363 row shape + kind/authority/basis/phase/window)

def ledger_rows(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for i, line in enumerate(f):
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def effective_micros(rows):
    """Fold the ledger: billed supersedes its estimate by reference; totals
    per kind (then combined). cost_usd is 6-decimal USD per ADR-0363."""
    superseded = {r.get("supersedes") for r in rows if r.get("supersedes")}
    per = {"model": 0, "compute": 0}
    for r in rows:
        if r.get("type") != "spend":
            continue
        if r.get("row_id") in superseded:
            continue
        per[r["kind"]] += int(round(r["cost_usd"] * 1e6))
    return per


def spend_append_rows(path, new_rows):
    rows = ledger_rows(path) if os.path.exists(path) else []
    ceilings = {r["kind"]: r.get("ceiling_micros") for r in rows
                if r.get("type") == "ceilings"}
    seen_ids = {r.get("row_id") for r in rows if r.get("row_id")}
    est_keys = {(r.get("phase"), r.get("window"), r.get("kind"))
                for r in rows
                if r.get("type") == "spend" and r.get("basis") == "estimate"}
    est_ids = {r.get("row_id") for r in rows
               if r.get("type") == "spend" and r.get("basis") == "estimate"}
    for row in new_rows:
        if row.get("type") != "spend":
            if not row.get("window") or not row.get("class"):
                refuse("spend.row_missing_window_class")
            rows.append(row)
            continue
        for k in ("kind", "authority", "basis", "phase", "window"):
            if not row.get(k):
                refuse(f"spend.missing:{k}")
        if row["kind"] not in SPEND_KINDS:
            refuse(f"spend.kind:{row['kind']}")
        if row["authority"] != KIND_AUTHORITY[row["kind"]]:
            refuse(f"spend.authority:{row['authority']}:{row['kind']}")
        if row["basis"] not in BASES:
            refuse(f"spend.basis:{row['basis']}")
        key = (row["phase"], row["window"], row["kind"])
        if row["basis"] == "estimate":
            if key in est_keys:
                refuse(f"spend.window_counted_twice:{key[0]}-{key[1]}")
            est_keys.add(key)
        if row["basis"] == "billed":
            if not row.get("supersedes"):
                refuse("spend.billed_without_supersedes")
            if row["supersedes"] not in est_ids:
                refuse("spend.billed_supersedes_no_estimate")
        rid = row.get("row_id") or (
            "sp-" + _lib.sha256_text(canon(row))[:16])
        if rid in seen_ids:
            refuse(f"spend.duplicate:{rid}")
        row["row_id"] = rid
        seen_ids.add(rid)
        rows.append(row)
        per = effective_micros(rows)
        if row["kind"] in ceilings and ceilings[row["kind"]] is not None:
            if per[row["kind"]] > ceilings[row["kind"]]:
                refuse(f"spend.ceiling_exceeded:{row['kind']}")
        # cumulative_usd = effective combined total after this append
        eff = sum(per.values())
        row["cumulative_usd"] = round(eff / 1e6, 6)
    with open(path, "a", encoding="utf-8") as f:
        for row in new_rows:
            f.write(canon(row) + "\n")


def spend_validate_rows(rows):
    ceilings = {}
    est_seen = set()
    errors = []
    for i, r in enumerate(rows):
        t = r.get("type")
        if t == "ceilings":
            if r.get("kind") in ceilings:
                errors.append(f"row {i}: ceilings repeated for {r.get('kind')}")
            ceilings[r.get("kind")] = r.get("ceiling_micros")
            continue
        if t == "spend":
            kind, auth, basis = r.get("kind"), r.get("authority"), r.get("basis")
            if kind not in SPEND_KINDS:
                errors.append(f"row {i}: kind {kind}")
            if KIND_AUTHORITY.get(kind) != auth:
                errors.append(
                    f"row {i}: {kind} line drawn on {auth} — one authority per kind")
            if basis not in BASES:
                errors.append(f"row {i}: basis {basis}")
            key = (r.get("phase"), r.get("window"), kind)
            if basis == "estimate":
                if key in est_seen:
                    errors.append(f"row {i}: window counted twice {key}")
                est_seen.add(key)
            if basis == "billed":
                sup = r.get("supersedes")
                if not sup:
                    errors.append(f"row {i}: billed row without supersedes")
                elif not any(o.get("row_id") == sup and o.get("basis") == "estimate"
                             for o in rows[:i]):
                    errors.append(f"row {i}: billed row supersedes no estimate")
        elif t in ("provider_usage", "cost_reconciled"):
            if not r.get("window") or not r.get("class"):
                errors.append(f"row {i}: {t} missing window/class")
        else:
            errors.append(f"row {i}: unknown type {t}")
    per = effective_micros(rows)
    for kind, total in per.items():
        cap = ceilings.get(kind)
        if cap is not None and total > cap:
            errors.append(f"kind {kind} cumulative {total} > ceiling {cap}")
    return errors


def cmd_spend_append(argv):
    row = load(argv[3])
    spend_append_rows(argv[2], [row])
    print("spend row appended")


def cmd_spend_validate(argv):
    rows = ledger_rows(argv[2]) if os.path.exists(argv[2]) else []
    errors = spend_validate_rows(rows)
    if errors:
        for e in errors:
            print(f"spend: {e}", file=sys.stderr)
        sys.exit(2)
    print("phase spend ledger: valid")


def cmd_spend_fold(argv):
    rows = ledger_rows(argv[2]) if os.path.exists(argv[2]) else []
    per = effective_micros(rows)
    print(jdump({
        "model_micros": per["model"],
        "compute_micros": per["compute"],
        "total_micros": per["model"] + per["compute"],
    }))


# ── REQ-XH-118 — first-pass declaration/quote checks (AC-10.1-9)

def cmd_fpcheck(argv):
    repo = argv[2]
    decls = []
    measured = None
    i = 3
    while i < len(argv):
        if argv[i] == "--export-measured-gib":
            measured = Fraction(argv[i + 1])
            i += 2
        else:
            decls.append(load(argv[i]))
            i += 1
    errors = []
    ceilings = {}
    wc = None
    for d in decls:
        w = d["window"]
        q = _lib.quote_doc(d, repo)
        ceilings[w] = q["pricing"]["ceiling_micros"]
        cc = q["quantities"]["concurrency"]["value"]
        if cc < 4:
            errors.append(f"{w}: C {cc} < 4")
        qi = d["quote_inputs"]
        for term in ("retry_trials", "load_hours", "admission_cutoff_hours"):
            if qi.get(term) is None:
                errors.append(f"{w}: quote_inputs.{term} absent")
        if qi.get("admission_cutoff_hours") is not None:
            wp_h = Fraction(q["pricing"]["lines"]["planning"]["instance_hours"])
            room = Fraction(str(d["dead_man"]["max_run_duration_hours"])) \
                - Fraction(str(qi["admission_cutoff_hours"]))
            if wp_h > room:
                errors.append(
                    f"{w}: W_p {float(wp_h):.3f}h > D − cut-off {float(room):.3f}h")
        items = set(qi.get("work_items") or [])
        if w.startswith("w1"):
            missing = W1_ITEMS - items
            if missing:
                errors.append(f"{w}: quote items absent {sorted(missing)}")
            eg = qi.get("egress_gib_estimate")
            ex = qi.get("export_size_gib")
            if ex is not None and eg is not None and Fraction(str(ex)) > Fraction(str(eg)):
                errors.append(
                    f"{w}: export-size estimate {ex} GiB exceeds egress line {eg} GiB")
        if w.startswith("w3"):
            missing = W3_ITEMS - items
            if missing:
                errors.append(f"{w}: quote items absent {sorted(missing)}")
        if w.startswith("wc"):
            wc = d
    if wc is None:
        errors.append("no contingency window (wc) in the declaration set")
    total = sum(ceilings.values())
    if total > FP_SUM_CAP_MICROS:
        errors.append(f"Σ ceiling_micros {total} > {FP_SUM_CAP_MICROS}")
    # the W1 export overrun is taken from Wc's D (REQ-XH-118): shrink the
    # contingency window's dead-man until its ceiling drops by the overrun;
    # refuse when Wc cannot absorb it.
    if measured is not None and wc is not None:
        w1 = next((d for d in decls if d["window"].startswith("w1")), None)
        if w1 is not None:
            q1 = _lib.quote_doc(w1, repo)
            eg = Fraction(str(w1["quote_inputs"]["egress_gib_estimate"]))
            rate = q1["pricing"]["rate_rows"]["egress_gib"]
            overrun_m = _lib._ceil_frac(Fraction(rate) * max(Fraction(0), measured - eg))
            if overrun_m > 0:
                qc = _lib.quote_doc(wc, repo)
                wc_r = qc["pricing"]["hourly_micros"]
                shrink_h = (Fraction(overrun_m) + Fraction(wc_r) - 1) / Fraction(wc_r)
                new_d = Fraction(str(wc["dead_man"]["max_run_duration_hours"])) - shrink_h
                # Wc's residual is the restart-overflow reservation —
                # it must keep ≥ restart + h_r coverage of a stopped VM.
                wc_min = Fraction(str(wc["restart"]["preauthorized_hours"]))
                if new_d < wc_min:
                    errors.append(
                        f"wc cannot absorb export overrun {overrun_m} µ$ "
                        f"(needs {float(shrink_h):.3f}h of D)")
    if errors:
        for e in errors:
            print(f"fpcheck: {e}", file=sys.stderr)
        sys.exit(2)
    print("fpcheck: declarations satisfy REQ-XH-118")


OPS = {
    "costrecord": cmd_costrecord,
    "costvalidate": cmd_costvalidate,
    "reconcile": cmd_reconcile,
    "spend-append": cmd_spend_append,
    "spend-validate": cmd_spend_validate,
    "spend-fold": cmd_spend_fold,
    "fpcheck": cmd_fpcheck,
}


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in OPS:
        print(__doc__.strip().splitlines()[1] if __doc__ else "usage", file=sys.stderr)
        sys.exit(1)
    OPS[sys.argv[1]](sys.argv)


if __name__ == "__main__":
    main()
