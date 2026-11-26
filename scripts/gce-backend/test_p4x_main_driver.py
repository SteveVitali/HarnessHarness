#!/usr/bin/env python3
# test_p4x_main_driver.py — fixture regression for the DF-sweep
# repairs mirrored into scripts/gce-backend/p4x-main-driver.py:
# DF-R7.5-1 run_id-keyed session/probe artifacts + stale-session
# archive/refuse, DF-R7.5-2 the cell_spec memory bound, DF-CAP7.1-1
# post-seal cost reconciliation, DF-CAP7.1-2 run-keyed verify-cell
# names + bounded container_conflict retry, DF-R7.3-4(a) the
# run-root marker. Fixture-verified coverage of the machinery
# contract, never live evidence.
#
# Run:  python3 scripts/gce-backend/test_p4x_main_driver.py
import argparse
import importlib.util
import json
import os
import tempfile
import unittest

_HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "p4x_main_driver", os.path.join(_HERE, "p4x-main-driver.py"))
DRV = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(DRV)


class SessionArtifactFixture(unittest.TestCase):
    """DF-R7.5-1 — keyed names + archive/refuse, prior bytes
    verbatim."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.out = self.tmp.name

    def _plant(self, name, body):
        p = os.path.join(self.out, name)
        with open(p, "w") as f:
            f.write(json.dumps(body))
        return p

    def test_artifact_paths_are_run_keyed(self):
        art = DRV.session_artifact_paths(self.out, "p4x-x1")
        self.assertEqual(os.path.basename(art["session"]),
                         "proxy-session-p4x-x1.json")
        self.assertEqual(os.path.basename(art["probe"]),
                         "lane-probe-p4x-x1.json")
        self.assertNotEqual(
            art["session"],
            DRV.session_artifact_paths(self.out, "p4x-x0")["session"])

    def test_stale_same_path_session_archives(self):
        sess = self._plant("proxy-session-p4x-x1.json",
                           {"run_id": "p4x-x1", "grants": {}})
        before = open(sess, "rb").read()
        out = DRV.archive_stale_session(sess, "p4x-x1")
        self.assertEqual(out[0], "archived")
        self.assertEqual(open(out[1], "rb").read(), before)

    def test_mismatched_run_id_refuses_untouched(self):
        sess = self._plant("proxy-session-p4x-x1.json",
                           {"run_id": "p4x-x0", "grants": {}})
        before = open(sess, "rb").read()
        self.assertEqual(DRV.archive_stale_session(sess, "p4x-x1"),
                         ("refuse", "p4x-x0"))
        self.assertEqual(open(sess, "rb").read(), before)


class RunRootMarkerFixture(unittest.TestCase):
    """DF-R7.3-4(a) — the driver stamps its recorded --out into a
    window-keyed marker beside the run roots."""

    def test_marker_carries_absolute_out(self):
        tmp = tempfile.TemporaryDirectory()
        out = os.path.join(tmp.name, "run-root-p4x")
        os.makedirs(out)
        DRV.stamp_run_root_marker(
            argparse.Namespace(out=out, window="P4X1"))
        marker = os.path.join(tmp.name, "last-run-root-p4x1")
        self.assertEqual(open(marker).read().strip(),
                         os.path.abspath(out))


class VerifyTagFixture(unittest.TestCase):
    """DF-CAP7.1-2 — verify-cell names keyed on run_id."""

    def test_tag_is_run_keyed(self):
        import hashlib as _h
        t1 = DRV.verify_tag("run-a", "s1")
        self.assertNotEqual(t1, DRV.verify_tag("run-b", "s1"))
        self.assertNotEqual(t1, _h.sha256(b"s1").hexdigest()[:16])


class CostReconcileFixture(unittest.TestCase):
    """DF-CAP7.1-1 — late-landing attributed events reconcile the
    committed record to the sealed-WAL sum."""

    def test_late_events_reconcile(self):
        tmp = tempfile.TemporaryDirectory()
        store = os.path.join(tmp.name, "store")
        slots = os.path.join(tmp.name, "slots")
        os.makedirs(os.path.join(store, "r"))
        os.makedirs(os.path.join(slots, "s1"))
        with open(os.path.join(store, "r", "events.wal"), "w") as f:
            for micros in (100, 30):
                f.write(json.dumps({
                    "class": "measurement.cost.attributed",
                    "payload": {"participant_ref": "s1",
                                "money": {"micro_units": micros}}})
                        + "\n")
        results = {"s1": {"slot_id": "s1", "rc": 0, "cost_micros": 100}}
        rows = DRV.reconcile_costs(store, results, slots)
        self.assertEqual(rows[0]["residual_micros"], 30)
        self.assertEqual(results["s1"]["cost_micros"], 130)
        committed = json.load(open(os.path.join(
            slots, "s1", "slot-result.json")))
        self.assertEqual(committed["cost_micros"], 130)
        self.assertEqual(committed["cost_snapshot"]["class"],
                         "cost_snapshot{approximate}")


class MemoryBoundFixture(unittest.TestCase):
    """DF-R7.5-2 — the conn spec carries cell_spec.memory_mb."""

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


if __name__ == "__main__":
    unittest.main()
