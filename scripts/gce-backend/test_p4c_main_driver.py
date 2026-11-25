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


if __name__ == "__main__":
    unittest.main()
