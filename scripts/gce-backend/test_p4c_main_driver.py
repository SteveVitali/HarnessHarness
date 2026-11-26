#!/usr/bin/env python3
# test_p4c_main_driver.py — fixture regression for the GATE-G36
# amendment repair in scripts/gce-backend/p4c-main-driver.py:
# the DF-R7.3-3 running-member deadline fence (reap() previously
# only poll()ed — hung drives held lanes for hours until manual
# coordinator SIGTERM: X1 ×4, P4-C ×7 recorded) and the resume
# leg's settled-result seeding. Fixture-verified coverage of the
# machinery contract, never live evidence.
#
# Run:  python3 -m unittest scripts.gce-backend.test_p4c_main_driver
#   or: python3 scripts/gce-backend/test_p4c_main_driver.py
import argparse
import importlib.util
import json
import os
import sys
import tempfile
import unittest

_HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "p4c_main_driver", os.path.join(_HERE, "p4c-main-driver.py"))
DRV = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(DRV)


class FakeProc:
    """subprocess.Popen stand-in recording signal calls."""

    def __init__(self):
        self.terminated = 0
        self.killed = 0

    def terminate(self):
        self.terminated += 1

    def kill(self):
        self.killed += 1


class FenceFixture(unittest.TestCase):
    """fence_overdue — the deadline fence DF-R7.3-3 names."""

    def test_within_deadline_is_inert(self):
        p, fenced = FakeProc(), {}
        self.assertIsNone(DRV.fence_overdue(
            p, "s1", started=1000.0, deadline_s=3738, fenced=fenced,
            now=1000.0 + 3738))
        self.assertEqual((p.terminated, p.killed, fenced), (0, 0, {}))

    def test_past_deadline_terminates_once(self):
        p, fenced = FakeProc(), {}
        self.assertEqual(DRV.fence_overdue(
            p, "s1", started=1000.0, deadline_s=3738, fenced=fenced,
            now=1000.0 + 3739), "terminate")
        self.assertEqual(p.terminated, 1)
        self.assertIn("s1", fenced)
        # a second pass inside the grace window neither re-terms
        # nor escalates
        self.assertEqual(DRV.fence_overdue(
            p, "s1", started=1000.0, deadline_s=3738, fenced=fenced,
            now=1000.0 + 3739 + DRV.FENCE_GRACE_S - 1), "terminating")
        self.assertEqual((p.terminated, p.killed), (1, 0))

    def test_grace_expiry_escalates_to_kill(self):
        p, fenced = FakeProc(), {}
        DRV.fence_overdue(p, "s1", started=1000.0, deadline_s=3738,
                          fenced=fenced, now=1000.0 + 3739)
        self.assertEqual(DRV.fence_overdue(
            p, "s1", started=1000.0, deadline_s=3738, fenced=fenced,
            now=1000.0 + 3739 + DRV.FENCE_GRACE_S), "kill")
        self.assertEqual((p.terminated, p.killed), (1, 1))

    def test_signal_failure_still_records_the_fence(self):
        class DeadProc(FakeProc):
            def terminate(self):
                raise ProcessLookupError()
        p, fenced = DeadProc(), {}
        self.assertEqual(DRV.fence_overdue(
            p, "s1", started=0.0, deadline_s=60, fenced=fenced,
            now=61.0), "terminate")
        self.assertIn("s1", fenced)


class ResumeSeedFixture(unittest.TestCase):
    """seed_settled_results — settled slots load verbatim, never
    relaunched or re-emitted on a resumed window."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.slots = os.path.join(self.tmp.name, "slots")
        os.makedirs(self.slots)

    def _result(self, sid, body=None):
        d = os.path.join(self.slots, sid)
        os.makedirs(d)
        with open(os.path.join(d, "slot-result.json"), "w") as f:
            f.write(json.dumps(body if body is not None
                               else {"slot_id": sid, "rc": 0}))

    def test_loads_every_settled_result(self):
        self._result("p4c-V1-t1-r1")
        self._result("p4c-baseline-t1-r1", {"slot_id": "x", "rc": -15})
        out = DRV.seed_settled_results(self.slots)
        self.assertEqual(set(out), {"p4c-V1-t1-r1",
                                    "p4c-baseline-t1-r1"})
        self.assertEqual(out["p4c-baseline-t1-r1"]["rc"], -15)

    def test_unlaunched_and_unparseable_dirs_skip(self):
        self._result("p4c-V1-t1-r1")
        os.makedirs(os.path.join(self.slots, "p4c-V1-t2-r1"))  # no result
        bad = os.path.join(self.slots, "p4c-V1-t3-r1")
        os.makedirs(bad)
        with open(os.path.join(bad, "slot-result.json"), "w") as f:
            f.write("{not json")
        out = DRV.seed_settled_results(self.slots)
        self.assertEqual(list(out), ["p4c-V1-t1-r1"])

    def test_missing_root_seeds_nothing(self):
        self.assertEqual(DRV.seed_settled_results(
            os.path.join(self.tmp.name, "absent")), {})


class SessionArtifactFixture(unittest.TestCase):
    """DF-R7.5-1 — session/probe/checkpoint/window artifacts keyed on
    run_id; a stale same-path session file archives byte-untouched or
    refuses on a run_id mismatch, never a false lane-wide hold."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.out = self.tmp.name

    def _plant(self, name, body):
        p = os.path.join(self.out, name)
        with open(p, "w") as f:
            f.write(body if isinstance(body, str)
                    else json.dumps(body))
        return p

    def test_artifact_paths_are_run_keyed(self):
        art = DRV.session_artifact_paths(self.out, "p4c-c-r4")
        self.assertEqual(
            os.path.basename(art["session"]), "proxy-session-p4c-c-r4.json")
        self.assertEqual(
            os.path.basename(art["sentinel"]), "proxy-p4c-c-r4.sentinel")
        self.assertEqual(
            os.path.basename(art["probe"]), "lane-probe-p4c-c-r4.json")
        # a different run_id resolves different paths — a prior
        # window's session can never shadow this run's poll
        art2 = DRV.session_artifact_paths(self.out, "p4c-c-r2")
        self.assertNotEqual(art["session"], art2["session"])

    def test_run_id_sanitized_for_filenames(self):
        art = DRV.session_artifact_paths(self.out, "run/a b:c")
        self.assertNotIn("/", os.path.basename(art["session"]))
        self.assertTrue(os.path.basename(art["session"]).startswith(
            "proxy-session-"))

    def test_stale_same_path_session_archives_byte_untouched(self):
        stale = {"schema": "hh-w2-proxy-session/1",
                 "run_id": "p4c-c-r4",
                 "grants": {"p4c-lane-probe": {"slot_token": "DEAD"}}}
        sess = self._plant("proxy-session-p4c-c-r4.json", stale)
        before = open(sess, "rb").read()
        out = DRV.archive_stale_session(sess, "p4c-c-r4")
        self.assertEqual(out[0], "archived")
        self.assertFalse(os.path.exists(sess))
        self.assertEqual(open(out[1], "rb").read(), before)

    def test_mismatched_run_id_refuses_untouched(self):
        sess = self._plant("proxy-session-p4c-c-r4.json",
                           {"run_id": "p4c-c-r2", "grants": {}})
        before = open(sess, "rb").read()
        out = DRV.archive_stale_session(sess, "p4c-c-r4")
        self.assertEqual(out[0], "refuse")
        self.assertEqual(out[1], "p4c-c-r2")
        # the planted artifact keeps its bytes — never trusted,
        # never touched
        self.assertEqual(open(sess, "rb").read(), before)

    def test_prior_artifacts_survive_a_fresh_provision(self):
        """The verify criterion: prior-window session/probe artifacts
        exist at launch; the resumed run's keyed artifacts coexist —
        prior bytes stay verbatim."""
        p1 = self._plant("proxy-session-p4c-c-r2.json",
                         {"run_id": "p4c-c-r2"})
        p2 = self._plant("lane-probe-p4c-c-r2.json", {"disposition": "pass"})
        p3 = self._plant("proxy-session.json",
                         {"run_id": "legacy"})  # pre-fix name
        before = {p: open(p, "rb").read() for p in (p1, p2, p3)}
        art = DRV.session_artifact_paths(self.out, "p4c-c-r4")
        self.assertIsNone(DRV.archive_stale_session(
            art["session"], "p4c-c-r4"))
        for p, b in before.items():
            self.assertEqual(open(p, "rb").read(), b)


class RunRootMarkerFixture(unittest.TestCase):
    """DF-R7.3-4(a) — the driver stamps its recorded --out into a
    window-keyed marker beside the run roots for the harvest leg."""

    def test_marker_carries_absolute_out(self):
        tmp = tempfile.TemporaryDirectory()
        out = os.path.join(tmp.name, "run-root-p4x")
        os.makedirs(out)
        a = argparse.Namespace(out=out, window="P4X1")
        DRV.stamp_run_root_marker(a)
        marker = os.path.join(tmp.name, "last-run-root-p4x1")
        self.assertEqual(open(marker).read().strip(),
                         os.path.abspath(out))


class MemoryBoundFixture(unittest.TestCase):
    """DF-R7.5-2 — the declared per-trial memory bound rides
    connection_info.cell_spec into the slot cell launch."""

    def test_launch_declares_cell_spec_memory(self):
        tmp = tempfile.TemporaryDirectory()
        tasks = os.path.join(tmp.name, "tasks")
        docs = os.path.join(tmp.name, "docs")
        os.makedirs(os.path.join(tasks, "t1"))
        os.makedirs(docs)
        with open(os.path.join(tasks, "t1", "instruction.md"), "w") as f:
            f.write("do the thing")
        with open(os.path.join(docs, "baseline.json"), "w") as f:
            json.dump({"sealed": True}, f)
        captured = {}

        class CapPopen:
            def __init__(self, cmd, **kw):
                captured["cmd"] = cmd

        a = argparse.Namespace(
            docs_dir=docs, tasks_dir=tasks,
            task_images={"t1": "img"}, model="m", mswea_pack="p",
            l1_drive_bin="hh-l1-drive", model_upstream=4000,
            slot_memory_mb=6144)
        sess = {"grants": {"s1": {"slot_token": "tok"}}}
        m = {"slot_id": "s1", "arm": "baseline", "task": "t1",
             "replicate": 1}
        import unittest.mock as mock
        with mock.patch.object(DRV.subprocess, "Popen", CapPopen):
            DRV.launch_member(a, sess, m,
                              os.path.join(tmp.name, "slots", "s1"))
        env_arg = captured["cmd"][captured["cmd"].index("--environment") + 1]
        conn = json.loads(env_arg)["connection_info"]
        self.assertEqual(conn["cell_spec"], {"memory_mb": 6144})
        arm = json.load(open(os.path.join(
            tmp.name, "slots", "s1", "arm.json")))
        self.assertEqual(arm["applied_limits"], {"memory_mb": 6144})

    def test_zero_bound_appends_no_cell_spec(self):
        tmp = tempfile.TemporaryDirectory()
        tasks = os.path.join(tmp.name, "tasks")
        docs = os.path.join(tmp.name, "docs")
        os.makedirs(os.path.join(tasks, "t1"))
        os.makedirs(docs)
        with open(os.path.join(tasks, "t1", "instruction.md"), "w") as f:
            f.write("do the thing")
        with open(os.path.join(docs, "baseline.json"), "w") as f:
            json.dump({"sealed": True}, f)
        captured = {}

        class CapPopen:
            def __init__(self, cmd, **kw):
                captured["cmd"] = cmd

        a = argparse.Namespace(
            docs_dir=docs, tasks_dir=tasks,
            task_images={"t1": "img"}, model="m", mswea_pack="p",
            l1_drive_bin="hh-l1-drive", model_upstream=4000,
            slot_memory_mb=0)
        sess = {"grants": {"s1": {"slot_token": "tok"}}}
        m = {"slot_id": "s1", "arm": "baseline", "task": "t1",
             "replicate": 1}
        import unittest.mock as mock
        with mock.patch.object(DRV.subprocess, "Popen", CapPopen):
            DRV.launch_member(a, sess, m,
                              os.path.join(tmp.name, "slots", "s1"))
        env_arg = captured["cmd"][captured["cmd"].index("--environment") + 1]
        conn = json.loads(env_arg)["connection_info"]
        self.assertNotIn("cell_spec", conn)


class CostReconcileFixture(unittest.TestCase):
    """DF-CAP7.1-1 — post-seal per-slot reconciliation: the committed
    slot record equals the sealed-WAL sum, the reap snapshot stays
    typed under cost_snapshot{approximate}."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.store = os.path.join(self.tmp.name, "store")
        self.slots = os.path.join(self.tmp.name, "slots")
        os.makedirs(self.store)
        os.makedirs(self.slots)

    def _wal(self, slot, micros):
        os.makedirs(os.path.join(self.store, "r"), exist_ok=True)
        with open(os.path.join(self.store, "r", "events.wal"), "a") as f:
            f.write(json.dumps({
                "class": "measurement.cost.attributed",
                "payload": {"participant_ref": slot,
                            "money": {"micro_units": micros}}}) + "\n")

    def _result(self, sid, body):
        d = os.path.join(self.slots, sid)
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "slot-result.json"), "w") as f:
            f.write(json.dumps(body))
        return os.path.join(d, "slot-result.json")

    def test_late_landing_events_reconcile_to_wal(self):
        # reap snapshot 100; two late events land before the seal →
        # the committed record carries the WAL-authoritative 130.
        self._wal("s1", 100)
        self._wal("s1", 30)
        self._wal("s2", 50)
        p1 = self._result("s1", {"slot_id": "s1", "rc": 0,
                                 "cost_micros": 100})
        self._result("s2", {"slot_id": "s2", "rc": 0,
                            "cost_micros": 50})
        results = {"s1": {"slot_id": "s1", "rc": 0, "cost_micros": 100},
                   "s2": {"slot_id": "s2", "rc": 0, "cost_micros": 50},
                   "w1": {"result": "withheld{stop:budget}"}}
        rows = DRV.reconcile_costs(self.store, results, self.slots)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["participant_ref"], "s1")
        self.assertEqual(rows[0]["class"], "measurement.cost.reconciled")
        self.assertEqual(rows[0]["snapshot_micros"], 100)
        self.assertEqual(rows[0]["wal_micros"], 130)
        self.assertEqual(rows[0]["residual_micros"], 30)
        self.assertEqual(results["s1"]["cost_micros"], 130)
        self.assertEqual(results["s1"]["cost_snapshot"]["class"],
                         "cost_snapshot{approximate}")
        committed = json.load(open(p1))
        self.assertEqual(committed["cost_micros"], 130)
        # zero-residual slots stay verbatim — no label, no rewrite
        self.assertNotIn("cost_snapshot", results["s2"])
        self.assertNotIn("cost_micros", results.get("w1", {}))

    def test_prior_seeded_results_label_never_rewrite(self):
        self._wal("p1", 100)
        self._wal("p1", 25)
        pj = self._result("p1", {"slot_id": "p1", "rc": 0,
                                 "cost_micros": 100})
        before = open(pj, "rb").read()
        results = {"p1": {"slot_id": "p1", "rc": 0, "cost_micros": 100}}
        rows = DRV.reconcile_costs(self.store, results, self.slots,
                                   prior_ids={"p1"})
        self.assertEqual(rows[0]["disposition"], "prior{byte_untouched}")
        self.assertEqual(rows[0]["residual_micros"], 25)
        # the committed bytes are append-only — the label lands on the
        # window row only
        self.assertEqual(open(pj, "rb").read(), before)
        self.assertEqual(results["p1"]["cost_snapshot"]["class"],
                         "cost_snapshot{approximate}")
        self.assertEqual(results["p1"]["cost_micros"], 100)


class VerifyCellFixture(unittest.TestCase):
    """DF-CAP7.1-2 — verify-cell names keyed on run_id + a bounded
    pre-clean/retry; a persisting collision types
    verifier_unavailable{container_conflict}, never a bare rc-125."""

    def _a(self, tmp):
        tasks = os.path.join(tmp, "tasks")
        os.makedirs(os.path.join(tasks, "t1", "tests"), exist_ok=True)
        with open(os.path.join(tasks, "t1", "tests", "test.sh"), "w") as f:
            f.write("#!/bin/sh\n")
        return argparse.Namespace(
            tasks_dir=tasks, cfg_obj={}, task_images={"t1": "img"},
            run_id="run-a", slot_memory_mb=6144)

    def _m(self):
        return {"slot_id": "s1", "arm": "baseline", "task": "t1",
                "replicate": 1}

    def _stub(self, tmp, run_results):
        """Patch podman: one cell mounts the slot ws; `run` answers
        from run_results in order."""
        ws = os.path.join(tmp, "slots", "s1", "ws")
        os.makedirs(ws, exist_ok=True)
        self._orig_containers = DRV.podman_containers
        DRV.podman_containers = lambda: [
            {"id": "cid1", "name": "cell", "mounts": [ws]}]
        calls = []

        class P:
            def __init__(self, rc, out="", err=""):
                self.returncode, self.stdout, self.stderr = rc, out, err

        def fake_run(argv, **kw):
            calls.append(argv)
            if argv[:2] == ["podman", "exec"]:
                return P(1)                     # /tests absent
            if argv[:2] == ["podman", "commit"]:
                return P(0)
            if argv[:2] == ["podman", "image"]:
                return P(0, "/app\n")
            if argv[:2] == ["podman", "run"]:
                return run_results.pop(0)
            return P(0)                         # rm/stop/rmi
        import unittest.mock as mock
        return mock.patch.object(DRV.subprocess, "run", fake_run), calls

    def tearDown(self):
        if getattr(self, "_orig_containers", None) is not None:
            DRV.podman_containers = self._orig_containers

    def test_tag_is_run_keyed(self):
        t1 = DRV.verify_tag("run-a", "s1")
        t2 = DRV.verify_tag("run-b", "s1")
        import hashlib as _h
        self.assertNotEqual(t1, t2)
        self.assertNotEqual(
            t1, _h.sha256(b"s1").hexdigest()[:16])  # the pre-fix form

    def test_conflict_retries_once_then_records(self):
        import unittest.mock as mock  # noqa
        tmp = tempfile.TemporaryDirectory().name
        a = self._a(tmp)
        confl = type("P", (), {"returncode": 125, "stdout": "",
                               "stderr": 'the container name "x" is '
                                         "already in use"})()
        ok = type("P", (), {"returncode": 0, "stdout": "", "stderr": ""})()
        patch, calls = self._stub(tmp, [confl, ok])
        with patch:
            row = DRV.run_verifier(a, self._m(),
                                   os.path.join(tmp, "slots", "s1"))
        self.assertEqual(row["disposition"], "recorded")
        runs = [c for c in calls if c[:2] == ["podman", "run"]]
        self.assertEqual(len(runs), 2)          # bounded retry, once
        self.assertIn("--memory", runs[0])
        self.assertIn("6144m", runs[0])
        self.assertIn(f"hh-verify-run-{DRV.verify_tag('run-a', 's1')}",
                      runs[0])

    def test_persistent_conflict_types_verifier_unavailable(self):
        tmp = tempfile.TemporaryDirectory().name
        a = self._a(tmp)
        confl = type("P", (), {"returncode": 125, "stdout": "",
                               "stderr": 'the container name "x" is '
                                         "already in use"})()
        patch, calls = self._stub(tmp, [confl, confl])
        with patch:
            row = DRV.run_verifier(a, self._m(),
                                   os.path.join(tmp, "slots", "s1"))
        self.assertEqual(row["disposition"],
                         "verifier_unavailable{container_conflict}")
        self.assertNotEqual(row["disposition"], "recorded")
        runs = [c for c in calls if c[:2] == ["podman", "run"]]
        self.assertEqual(len(runs), 2)          # bounded — never more


if __name__ == "__main__":
    unittest.main()
