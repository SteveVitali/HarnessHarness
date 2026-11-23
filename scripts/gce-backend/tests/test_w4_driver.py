#!/usr/bin/env python3
"""Offline controls for the R6.52 W4 driver (scripts/gce-backend/w4-main-driver.py).

Imports the driver module (never calls main) and drives its Scheduler with
in-memory launch/poll/clock/persist hooks. Continuation decisions go through
the REAL `fp-continuation` adapter binary (crates/hh-runner) — no copied
formula, no stub of the rule. Locate it with HH_FP_CONTINUATION_BIN or the
workspace target dir (`cargo build -p hharness-runner --bin fp_continuation`);
an absent binary is an error, never a skip. No network, process launch of a
slot, container, provider call or spend.

Supersedes the factory's AST-extraction controls (test_w4_driver_admission.py)
— the same six admission scenarios are kept below against the restructured
Scheduler.
"""
import ast
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import types
import unittest

HERE = Path(__file__).resolve().parent
DRIVER = HERE.parent / "w4-main-driver.py"
REPO = HERE.parents[2]
spec = importlib.util.spec_from_file_location("w4_main_driver", DRIVER)
drv = importlib.util.module_from_spec(spec)
spec.loader.exec_module(drv)

DROP_LABEL = "cost-triggered; cost correlates with failure length"


def adapter_bin():
    cand = [os.environ.get("HH_FP_CONTINUATION_BIN")]
    td = os.environ.get("CARGO_TARGET_DIR")
    if td:
        cand.append(os.path.join(td, "debug", "fp_continuation"))
    cand.append(str(REPO / "target" / "debug" / "fp_continuation"))
    for c in cand:
        if c and os.access(c, os.X_OK):
            return c
    raise RuntimeError("fp_continuation binary not found — build it "
                       "(cargo build -p hharness-runner --bin fp_continuation) "
                       "or set HH_FP_CONTINUATION_BIN; this battery never skips")


BIN = adapter_bin()

TASKS = [f"t{i}" for i in range(10)]


def cfg(root=64_188_120, caps_v5=10**12, cutoff=0):
    return {
        "schema": drv.CONT_CONFIG_SCHEMA,
        "root_cap_micros": root, "c": 4, "slice_micros": 850_000,
        "priors_micros": {a: 374_000 for a in drv.FIVE_ARMS},
        "client_timeouts_ms": {"proxy_upstream": 600_000},
        "priority": {"protected": ["mini-swe-agent", "hh_native"],
                     "drop_order": ["v5", "claude-code", "pi"]},
        "caps_micros": {"v5": caps_v5, "pi": 0, "claude-code": 0},
        "cutoff_ms": cutoff, "overhead_ms": 0, "load_ms": 0,
        "source_refs": {"fp_e": "fixture"},
    }


class Handle:
    def __init__(self, m, start, dur, rc):
        self.m, self.start, self.dur, self.rc = m, start, dur, rc
        self.terminated = False


class Harness:
    """In-memory hooks: a fake clock, launches that settle after `dur`."""

    def __init__(self, config=None, *, behave=None, probe_passed=True,
                 tasks=TASKS, replicates=5, arms=None, dead_man=10**9,
                 slot_deadline=3_738, launch_raise=None, record=None):
        self.t = 0.0
        self.behave = behave or (lambda m, n: (10.0, 0, 100_000, False))
        self.launch_raise = launch_raise or (lambda m: None)
        self.launches, self.costs, self.infra = [], {}, {}
        self.requests = []
        self.persisted = {}
        self.record = record
        rounds = drv.plan_schedule(tasks, replicates, arms or drv.ARMS, "seed")
        self.sched = drv.Scheduler(
            rounds, config or cfg(), self.adapter,
            launch=self.launch, poll=self.poll, terminate=self.terminate,
            finish=self.finish, cost_of=lambda sid: 0, orphans=lambda: [],
            now=lambda: self.t, sleep=self.sleep, persist=self.persist,
            concurrency=4, checkpoint_round=2, slot_deadline_s=slot_deadline,
            harvest_reserve_s=0, dead_man_epoch=dead_man,
            probe_passed=probe_passed)

    def adapter(self, req):
        self.requests.append(req)
        return drv.call_adapter(BIN, req)

    def sleep(self, s):
        self.t += s

    def persist(self, name, obj):
        self.persisted[name] = obj

    def launch(self, m):
        self.launch_raise(m)
        dur, rc, cost, infra = self.behave(m, len(self.launches))
        self.launches.append(m["slot_id"])
        self.costs[m["slot_id"]] = cost
        self.infra[m["slot_id"]] = infra
        return Handle(m, self.t, dur, rc)

    def poll(self, h):
        return h.rc if self.t - h.start >= h.dur else None

    def terminate(self, h):
        h.terminated = True
        return -15

    def finish(self, m, h, rc, started, end):
        row = {"slot_id": m["slot_id"], "arm": m["arm"], "task": m["task"],
               "replicate": m["replicate"], "rc": rc,
               "wall_ms": int((self.t - started) * 1000),
               "cost_micros": self.costs[m["slot_id"]],
               "infra": self.infra[m["slot_id"]]}
        if end:
            row["end"] = end
        return row

    def run(self):
        return self.sched.run()


def withheld_of(s, arm=None, disp=None):
    return [w for w in s.withheld.values()
            if (arm is None or w["arm"] == arm)
            and (disp is None or w["disposition"] == disp)]


class SchedulerProtocolTests(unittest.TestCase):
    def test_full_window_partitions_and_checkpoints_every_boundary(self):
        h = Harness()
        closure = h.run()
        self.assertTrue(closure["ok"], closure)
        self.assertEqual(closure["planned"], 150)
        self.assertEqual(closure["attempted"], 150)
        self.assertEqual(sorted(h.persisted),
                         ["checkpoint-r2.json", "checkpoint-r3.json",
                          "checkpoint-r4.json"])
        for ck in h.persisted.values():
            self.assertEqual(ck["schema"], "hh-fp-checkpoint/1")
            self.assertEqual(ck["label"][:9], "w4-round-")

    def test_zero_remaining_budget_launches_no_later_pair(self):
        # Spend so large the protected pair cannot fit at the r2 boundary.
        h = Harness(cfg(root=8_000_000))
        closure = h.run()
        self.assertTrue(closure["ok"], closure)
        later = [s for s in h.launches if s.endswith(("-r3", "-r4", "-r5"))]
        self.assertEqual(later, [], "a protected pair launched after a breach")
        self.assertEqual(len(h.sched.inference_refusals), 3)
        for arm in drv.ARMS:
            rows = withheld_of(h.sched, arm, "withheld{budget}")
            self.assertEqual(len(rows), 30, arm)
            self.assertTrue(all(r["label"] == DROP_LABEL for r in rows))

    def test_v5_drop_emits_one_disposition_per_dropped_slot(self):
        # Fits the protected pair, not V5 (r3–r5: 30 slots per arm left).
        # r2: consumed 6.0 M; p̂ = ceil(1.2 × 150 000) = 180 000; 30 left
        # per arm → all three 16.2 M, without V5 10.8 M; rhs = root − 6.0 M
        # − 4 × 850 000 = 12.6 M: V5 drops, the pair fits (and keeps fitting).
        h = Harness(cfg(root=22_000_000))
        closure = h.run()
        self.assertTrue(closure["ok"], closure)
        v5 = withheld_of(h.sched, "v5", "withheld{budget}")
        self.assertEqual(len(v5), 30)
        self.assertTrue(all(r["label"] == DROP_LABEL for r in v5))
        self.assertTrue(all(r["cause"] == "checkpoint:w4_round_2" for r in v5))
        self.assertEqual(len([s for s in h.launches if "-hh_native-" in s]), 50)
        self.assertFalse(h.sched.inference_refusals)

    def test_round_three_cost_change_causes_a_later_action(self):
        def behave(m, n):
            cost = 900_000 if m["replicate"] >= 3 else 100_000
            return (10.0, 0, cost, False)
        h = Harness(cfg(root=64_188_120), behave=behave)
        h.run()
        r2 = h.persisted["checkpoint-r2.json"]
        self.assertEqual(r2["withheld"], [])
        later = [h.persisted[k]["withheld"] for k in
                 ("checkpoint-r3.json", "checkpoint-r4.json") if k in h.persisted]
        self.assertTrue(any(w and w[0]["arm"] == "v5" for w in later), later)

    def test_slow_prior_round_slot_is_drained_into_the_checkpoint(self):
        slow = "w4-hh_native-" + "t0" + "-r2"
        def behave(m, n):
            return ((5_000.0 if m["slot_id"] == slow else 10.0), 0, 100_000, False)
        h = Harness(behave=behave)
        h.run()
        req = next(r for r in h.requests if r["op"] == "checkpoint"
                   and r["input"]["after_round"] == 2)
        measured = {t["slot"] for t in req["input"]["measured"]["hh_native"]}
        self.assertIn(slow, measured)
        self.assertEqual(sum(len(v) for v in req["input"]["in_flight"].values()), 0)
        self.assertNotIn(slow, sum(req["input"]["remaining"].values(), []))

    def test_v5_unclassified_failure_holds_only_v5_immediately(self):
        def behave(m, n):
            if m["arm"] == "v5":
                return (1.0, 1, 0, False)      # rc 1, no spend: unclassified
            return (10.0, 0, 100_000, False)
        h = Harness(behave=behave)
        closure = h.run()
        self.assertTrue(closure["ok"], closure)
        v5_launched = [s for s in h.launches if "-v5-" in s]
        self.assertLessEqual(len(v5_launched), 3, v5_launched)
        held = withheld_of(h.sched, "v5", "withheld{admission:native_lane_hold}")
        self.assertEqual(len(held) + len(v5_launched), 50)
        self.assertFalse(withheld_of(h.sched, "hh_native"))
        self.assertEqual([x["arm"] for x in h.sched.holds], ["v5"])

    def test_hh_native_typed_infra_holds_hh_native_not_v5_or_mini(self):
        def behave(m, n):
            return (10.0, 0, 100_000, m["arm"] == "hh_native" and n < 3)
        h = Harness(behave=behave)
        h.run()
        self.assertEqual([x["arm"] for x in h.sched.holds], ["hh_native"])
        self.assertEqual(len([s for s in h.launches if "-mini-swe-agent-" in s]), 50)
        self.assertEqual(len([s for s in h.launches if "-v5-" in s]), 50)

    def test_admission_order_not_completion_order_feeds_the_hold(self):
        # hh_native's 4th admitted slot fails fast (typed infra) before the
        # first three settle clean: it is not among the first 3 → no hold.
        order = []
        def behave(m, n):
            if m["arm"] == "hh_native":
                order.append(m["slot_id"])
                k = len(order)
                if k == 4:
                    return (0.5, 1, 0, True)
                return (50.0, 0, 100_000, False)
            return (50.0, 0, 100_000, False)
        h = Harness(behave=behave)
        h.run()
        self.assertEqual(h.sched.first3["hh_native"], order[:3])
        self.assertEqual(h.sched.holds, [])

    def test_failed_probe_holds_native_arms_and_hosted_continues(self):
        h = Harness(probe_passed=False)
        closure = h.run()
        self.assertTrue(closure["ok"])
        self.assertEqual(sorted(x["arm"] for x in h.sched.holds), ["hh_native", "v5"])
        self.assertTrue(all("-mini-swe-agent-" in s for s in h.launches))
        self.assertEqual(len(h.launches), 50)

    def test_declared_v5_cap_exhaustion_withholds_budget_without_cell(self):
        h = Harness(cfg(caps_v5=1_500_000))
        closure = h.run()
        self.assertTrue(closure["ok"])
        v5 = [s for s in h.launches if "-v5-" in s]
        self.assertLess(len(v5), 50)
        rows = withheld_of(h.sched, "v5", "withheld{budget}")
        self.assertEqual(len(rows) + len(v5), 50)
        self.assertTrue(rows[0]["cause"].startswith("cap:"))

    def test_adapter_refusal_stops_admission_typed(self):
        h = Harness()
        h.sched.adapter = lambda req: (_ for _ in ()).throw(drv.AdapterRefused("fixture"))
        closure = h.run()
        self.assertTrue(closure["ok"], closure)
        self.assertEqual(h.launches, [])
        self.assertEqual(len(withheld_of(h.sched, None, "withheld{infra}")), 150)

    def test_second_member_launch_exception_rolls_back_the_first(self):
        def boom(m):
            if m["slot_id"] == "w4-hh_native-t0-r1":
                raise OSError("spawn")
        h = Harness(launch_raise=boom, tasks=["t0"], replicates=1)
        closure = h.run()
        self.assertTrue(closure["ok"], closure)
        mini = h.sched.results["w4-mini-swe-agent-t0-r1"]
        self.assertTrue(mini["end"].startswith("terminated{unit_launch_rollback"))
        self.assertEqual(h.sched.results["w4-hh_native-t0-r1"]["launch"], "failed{OSError}")

    def test_prelaunch_refusal_is_withheld_infra_and_partner_rolled_back(self):
        def refuse(m):
            if m["slot_id"] == "w4-hh_native-t0-r1":
                raise drv.LaunchRefused("image_absent")
        h = Harness(launch_raise=refuse, tasks=["t0"], replicates=1)
        closure = h.run()
        self.assertTrue(closure["ok"], closure)
        self.assertEqual(h.sched.withheld["w4-hh_native-t0-r1"]["disposition"],
                         "withheld{infra}")
        self.assertIn("terminated{unit_launch_rollback",
                      h.sched.results["w4-mini-swe-agent-t0-r1"]["end"])

    def test_hung_slot_is_terminated_and_recorded_attempted(self):
        def behave(m, n):
            return (10**7 if n == 0 else 10.0, 0, 100_000, False)
        h = Harness(behave=behave, tasks=["t0"], replicates=1, slot_deadline=60)
        closure = h.run()
        self.assertTrue(closure["ok"])
        first = h.launches[0]
        self.assertEqual(h.sched.results[first]["end"], "terminated{slot_deadline}")


class AdmissionCutoffTests(unittest.TestCase):
    """The factory's six admission controls, kept against the Scheduler."""

    def unit(self, *, at_start=0, after_wait=0, busy=False,
             arms=("mini-swe-agent", "hh_native")):
        h = Harness(tasks=["t0"], replicates=1, arms=list(arms), dead_man=10,
                    slot_deadline=0)
        h.t = at_start
        if busy:
            for i in range(4):
                prior = {"slot_id": f"prior-{i}", "arm": "mini-swe-agent"}
                h.sched.running[f"prior-{i}"] = (Handle(prior, 0, 0, 0), prior, 0)

            def reap():
                if h.sched.running:
                    h.t = after_wait
                h.sched.running.clear()
            h.sched.reap = reap
        h.sched.boundary = lambda _r: None
        h.sched.evaluate_holds = lambda: None
        h.run()
        return h.launches, sorted((w["slot_id"], w["disposition"])
                                  for w in h.sched.withheld.values())

    def test_probe_grant_and_declared_cap_are_fifty_thousand(self):
        self.assertEqual(drv.PROBE_SLICE_MICROS, 50_000)
        main = next(n for n in ast.parse(DRIVER.read_text()).body
                    if isinstance(n, ast.FunctionDef) and n.name == "main")
        reg = next(n for n in ast.walk(main) if isinstance(n, ast.Assign)
                   and any(isinstance(t, ast.Name) and t.id == "slot_lines"
                           for t in n.targets))
        env = {"PROBE_SLICE_MICROS": drv.PROBE_SLICE_MICROS,
               "a": types.SimpleNamespace(effort="high")}
        exec(compile(ast.fix_missing_locations(ast.Module(body=[reg], type_ignores=[])),
                     str(DRIVER), "exec"), env)
        self.assertEqual(env["slot_lines"], ["w4-lane-probe:50000:high"])

    def test_capacity_wait_crossing_cutoff_withholds_both_without_launch(self):
        launches, withheld = self.unit(busy=True, after_wait=11)
        self.assertEqual(launches, [])
        self.assertEqual(withheld, [("w4-hh_native-t0-r1", "withheld{window_deadline}"),
                                    ("w4-mini-swe-agent-t0-r1", "withheld{window_deadline}")])

    def test_capacity_wait_ending_at_cutoff_withholds_singleton(self):
        launches, withheld = self.unit(busy=True, after_wait=10, arms=("v5",))
        self.assertEqual(launches, [])
        self.assertEqual(withheld, [("w4-v5-t0-r1", "withheld{window_deadline}")])

    def test_capacity_wait_that_still_fits_preserves_pair(self):
        launches, withheld = self.unit(busy=True, after_wait=9)
        self.assertEqual(sorted(launches), ["w4-hh_native-t0-r1", "w4-mini-swe-agent-t0-r1"])
        self.assertEqual(withheld, [])

    def test_no_capacity_wait_preserves_pair(self):
        launches, withheld = self.unit()
        self.assertEqual(len(launches), 2)
        self.assertEqual(withheld, [])

    def test_initial_cutoff_withholds_once(self):
        launches, withheld = self.unit(at_start=10)
        self.assertEqual(launches, [])
        self.assertEqual(len(withheld), 2)


class ProbeReceiptTests(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp()
        os.makedirs(os.path.join(self.d, "blobs"))
        self.a = types.SimpleNamespace(model="m1", effort="high", listen="x",
                                       store_root=self.d, run_id="run-1")
        self.sess = {"grants": {"w4-lane-probe": {"slot_token": "fixture"}},
                     "store_root": self.d}

    def blob(self, name, obj):
        with open(os.path.join(self.d, "blobs", name), "w") as f:
            json.dump(obj, f)
        return f"sha256:{name}"

    def call(self, *, slot="w4-lane-probe", run="run-1", effort="high",
             disposition="forwarded", cls="model.call.completed", cost=1_000):
        body = self.blob("body", {"model": "m1", "reasoning_effort": effort})
        rec = self.blob("rcpt", {"proxy": {"slot_id": slot, "phase_run_id": run,
                                           "downstream": {"path": "/v1/chat/completions",
                                                          "body_ref": body}}})
        return [{"class": cls, "payload": {"model_call_id": "mc1",
                                            "response_ref": rec,
                                            "disposition": disposition}},
                {"class": "measurement.cost.attributed",
                 "payload": {"subject": slot, "money": {"micro_units": cost}}}]

    def probe(self, evs, status=200):
        return drv.run_lane_probe(self.a, self.sess, self.d,
                                  http=lambda *_a: status, events=lambda _s: evs)

    def test_http_200_without_receipt_fails(self):
        rec, ok = self.probe([])
        self.assertFalse(ok)
        self.assertEqual(rec["calls"][0]["disposition"], "unreceipted{no_bound_receipt}")

    def test_bound_matching_receipt_passes_with_metered_cost(self):
        rec, ok = self.probe(self.call())
        self.assertTrue(ok, rec)
        self.assertEqual(rec["calls"][0]["cost_micros"], 1_000)
        self.assertEqual(rec["plan"]["cap_micros"], 50_000)

    def test_wrong_slot_run_effort_or_disposition_fail(self):
        for kw in ({"slot": "w4-mini-swe-agent-t0-r1"}, {"run": "run-2"},
                   {"effort": "low"}, {"disposition": "upstream_error",
                                       "cls": "model.call.failed"}):
            rec, ok = self.probe(self.call(**kw))
            self.assertFalse(ok, kw)

    def test_non_2xx_or_over_cap_fails(self):
        self.assertFalse(self.probe(self.call(), status=500)[1])
        self.assertFalse(self.probe(self.call(cost=50_001))[1])


class ConfigAndTeardownTests(unittest.TestCase):
    def write(self, obj):
        fd, p = tempfile.mkstemp(suffix=".json")
        with os.fdopen(fd, "w") as f:
            json.dump(obj, f)
        return p

    def test_config_refuses_missing_unknown_and_uncapped_arm(self):
        self.assertEqual(drv.load_continuation_config(self.write(cfg()))["c"], 4)
        bad = cfg(); del bad["cutoff_ms"]
        self.assertRaises(drv.ContinuationConfigError,
                          drv.load_continuation_config, self.write(bad))
        bad = cfg(); bad["extra"] = 1
        self.assertRaises(drv.ContinuationConfigError,
                          drv.load_continuation_config, self.write(bad))
        bad = cfg(); del bad["caps_micros"]["v5"]
        self.assertRaises(drv.ContinuationConfigError,
                          drv.load_continuation_config, self.write(bad))
        bad = cfg(); bad["root_cap_micros"] = True
        self.assertRaises(drv.ContinuationConfigError,
                          drv.load_continuation_config, self.write(bad))

    def test_session_end_is_clean_only_when_acknowledged(self):
        d = tempfile.mkdtemp()
        s = os.path.join(d, "proxy-session.json")
        g = ["a", "b"]
        self.assertEqual(drv.verify_session_end(s, g, 0)["status"],
                         "incomplete{session_end_absent}")
        def end(obj):
            with open(s + ".end", "w") as f:
                json.dump(obj, f)
        end({"revoked": ["a", "b"], "stop": "ok"})
        self.assertEqual(drv.verify_session_end(s, g, 0)["status"], "clean")
        self.assertNotEqual(drv.verify_session_end(s, g, "timeout")["status"], "clean")
        end({"revoked": ["a"], "stop": "ok", "revoke_failed": {"b": "x"}})
        self.assertNotEqual(drv.verify_session_end(s, g, 0)["status"], "clean")
        end({"revoked": ["a", "b"]})   # the pre-repair shape never claims clean
        self.assertNotEqual(drv.verify_session_end(s, g, 0)["status"], "clean")

    def test_exact_slot_cost_attribution(self):
        evs = [{"class": "measurement.cost.attributed",
                "payload": {"subject": "w4-v5-t1-r10", "money": {"micro_units": 7}}},
               {"class": "measurement.cost.attributed",
                "payload": {"subject": "w4-v5-t1-r1", "money": {"micro_units": 5}}}]
        self.assertEqual(drv.slot_cost_micros(evs, "w4-v5-t1-r1"), 5)


if __name__ == "__main__":
    unittest.main()
