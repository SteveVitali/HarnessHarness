#!/usr/bin/env python3
# test_w4_main_driver.py — fixture regression for the native-lane
# verifier leg of scripts/gce-backend/w4-main-driver.py (R6.56
# continuation a5; the w4x/W4 verifier-absent defect). Every podman
# call is stubbed — this is fixture-verified coverage of the recorded
# argv shape and the typed dispositions, never live evidence.
#
# Run:  python3 -m unittest scripts.gce-backend.test_w4_main_driver
#   or: python3 scripts/gce-backend/test_w4_main_driver.py
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

_HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location(
    "w4_main_driver", os.path.join(_HERE, "w4-main-driver.py"))
DRV = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(DRV)


class Completed:
    def __init__(self, rc=0, out="", err=""):
        self.returncode = rc
        self.stdout = out
        self.stderr = err


class VerifierFixture(unittest.TestCase):
    """run_verifier's commit-clone shape + typed dispositions."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = self.tmp.name
        # task package with a verifier tree (the held-tasks shape)
        self.tasks_dir = os.path.join(root, "tasks")
        os.makedirs(os.path.join(self.tasks_dir, "t1", "tests"))
        with open(os.path.join(self.tasks_dir, "t1", "tests",
                               "test.sh"), "w") as f:
            f.write("#!/bin/sh\necho 1 > /logs/verifier/reward.txt\n")
        self.slot_dir = os.path.join(root, "slot")
        self.ws = os.path.join(self.slot_dir, "ws")
        os.makedirs(self.ws)
        self.calls = []  # recorded podman argv

        def fake_run(argv, **kw):
            self.calls.append(list(argv))
            if argv[:3] == ["podman", "exec", "cid"]:
                return Completed(rc=1)          # /tests absent in cell
            if argv[:2] == ["podman", "commit"]:
                return Completed(rc=0)
            if argv[:3] == ["podman", "image", "inspect"]:
                return Completed(rc=0, out="/app\n")
            if argv[:2] == ["podman", "run"]:
                # emulate the verifier dropping the reward through the
                # collect mount (host edge — the mount itself is what
                # the test asserts)
                collect = os.path.join(self.slot_dir, "collect")
                os.makedirs(collect, exist_ok=True)
                with open(os.path.join(collect, "reward.txt"), "w") as f:
                    f.write("1\n")
                return Completed(rc=0, out="verifier ok\n")
            return Completed(rc=0)

        self.fake_run = fake_run
        self.a = SimpleNamespace(
            tasks_dir=self.tasks_dir,
            cfg_obj={"verify_deadline_ms": 600000,
                     "workdir_default": "/app"},
            task_images={"t1": "docker.io/x/t1@sha256:d"})
        self.m = {"slot_id": "w4-v5-t1-r1", "task": "t1"}
        # the still-live agent cell, found by its /work mount == ws
        self.containers = [{"id": "cid", "name": "hh-env",
                            "mounts": [self.ws]}]

    def _verify(self, run=None, containers=None):
        with mock.patch.object(DRV.subprocess, "run",
                               side_effect=run or self.fake_run), \
             mock.patch.object(DRV, "podman_containers",
                               return_value=(self.containers
                                             if containers is None
                                             else containers)):
            return DRV.run_verifier(self.a, self.m, self.slot_dir)

    def run_argv(self):
        for c in self.calls:
            if c[:2] == ["podman", "run"]:
                return c
        self.fail("no podman run in recorded calls: %r" % self.calls)

    # ── the recorded shape ──────────────────────────────────────────
    def test_commit_clone_runs_tests_mounted_ro(self):
        row = self._verify()
        self.assertEqual(row["disposition"], "recorded")
        self.assertEqual(row["returncode"], 0)
        argv = self.run_argv()
        self.assertIn("--rm", argv)
        self.assertEqual(argv[argv.index("--network") + 1], "bridge")
        tests_mnt = [a for a in argv if a.endswith(":/tests:ro")]
        self.assertEqual(len(tests_mnt), 1)
        self.assertTrue(tests_mnt[0].startswith(
            os.path.join(self.tasks_dir, "t1", "tests")))
        self.assertTrue(any(a.endswith(":/logs/verifier:rw")
                            for a in argv))
        self.assertTrue(any(a == self.ws + ":/work:rw" for a in argv))
        self.assertEqual(argv[-2:], ["bash", "/tests/test.sh"])
        self.assertEqual(row["verify_cell"], "commit_clone")
        self.assertEqual(row["verify_egress"], "bridge")
        self.assertEqual(row["workdir"], "/app")
        self.assertEqual(row["reward_file"], "1")

    def test_commit_precedes_run(self):
        self._verify()
        kinds = [c[1] for c in self.calls if c[0] == "podman"]
        self.assertLess(kinds.index("commit"), kinds.index("run"))
        self.assertIn("exec", kinds)  # the phase-boundary probe

    # ── typed dispositions ──────────────────────────────────────────
    def test_no_test_in_package(self):
        os.remove(os.path.join(self.tasks_dir, "t1", "tests",
                               "test.sh"))
        row = self._verify()
        self.assertEqual(row["disposition"],
                         "verifier_unavailable{no_test_in_package}")
        self.assertFalse(any(c[:2] == ["podman", "run"]
                             for c in self.calls))

    def test_tests_present_is_phase_boundary_breach(self):
        def run(argv, **kw):
            self.calls.append(list(argv))
            if argv[:3] == ["podman", "exec", "cid"]:
                return Completed(rc=0)          # /tests already inside
            return Completed(rc=0)
        row = self._verify(run=run)
        self.assertEqual(
            row["disposition"],
            "verifier_unavailable{phase_boundary:tests_present}")
        self.assertFalse(any(c[:2] == ["podman", "commit"]
                             for c in self.calls))

    def test_env_closed_or_unmounted(self):
        row = self._verify(containers=[])
        self.assertEqual(row["disposition"],
                         "verifier_unavailable{env_closed_or_unmounted}")

    def test_commit_failed(self):
        def run(argv, **kw):
            self.calls.append(list(argv))
            if argv[:3] == ["podman", "exec", "cid"]:
                return Completed(rc=1)
            if argv[:2] == ["podman", "commit"]:
                return Completed(rc=1, err="commit blew up")
            return Completed(rc=0)
        row = self._verify(run=run)
        self.assertEqual(row["disposition"],
                         "verifier_unavailable{commit_failed}")
        self.assertIn("commit blew up", row["stderr_tail"])

    def test_verifier_timeout(self):
        def run(argv, **kw):
            self.calls.append(list(argv))
            if argv[:3] == ["podman", "exec", "cid"]:
                return Completed(rc=1)
            if argv[:2] == ["podman", "run"]:
                raise subprocess.TimeoutExpired(argv, 600.0)
            return Completed(rc=0)
        row = self._verify(run=run)
        self.assertEqual(row["disposition"],
                         "verifier_unavailable{verifier_timeout}")
        self.assertEqual(row["verify_cell"], "commit_clone")
        # the clone is stopped + force-removed, never left billed
        self.assertTrue(any(c[:3] == ["podman", "stop", "-t"]
                            for c in self.calls))
        self.assertTrue(any(c[:3] == ["podman", "rm", "-f"]
                            for c in self.calls))

    def test_exec_error_is_typed(self):
        def run(argv, **kw):
            self.calls.append(list(argv))
            if argv[:3] == ["podman", "exec", "cid"]:
                raise OSError("podman gone")
            return Completed(rc=0)
        row = self._verify(run=run)
        self.assertEqual(row["disposition"],
                         "verifier_unavailable{exec_error}")

    # ── deadline + cleanup discipline ───────────────────────────────
    def test_task_toml_deadline_wins_over_cfg(self):
        with open(os.path.join(self.tasks_dir, "t1", "task.toml"),
                  "w") as f:
            f.write("[verifier]\ntimeout_sec = 900\n")
        row = self._verify()
        self.assertEqual(row["deadline_s"], 900.0)
        self.assertEqual(row["deadline_source"], "task_toml")
        argv = self.run_argv()
        self.assertEqual(argv[argv.index("--workdir") + 1], "/app")

    def test_cell_and_verify_image_removed(self):
        self._verify()
        rm = [c for c in self.calls
              if c[:3] == ["podman", "rm", "-f"]]
        rmi = [c for c in self.calls
               if c[:3] == ["podman", "rmi", "-f"]]
        self.assertTrue(any(c[3] == "cid" for c in rm))
        self.assertEqual(len(rmi), 1)


if __name__ == "__main__":
    unittest.main()
