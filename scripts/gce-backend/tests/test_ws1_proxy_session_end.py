#!/usr/bin/env python3
"""Offline controls for ws1-proxy-session.py's close path (R6.52 repair).

A fake `hh-model-proxy` (a Python script speaking the committed control
protocol: one JSON op per line in, one `{"ok":…}` / `{"error":…}` reply per
line out) stands in for the binary. Only acknowledged revocations may enter
`revoked`; seal/revoke/stop failures are recorded per id and the shim exits
nonzero. No network, credential or provider call.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SHIM = Path(__file__).resolve().parents[1] / "ws1-proxy-session.py"

FAKE = r'''#!/usr/bin/env python3
import json, os, sys
fail = set(filter(None, os.environ.get("FAKE_FAIL", "").split(",")))
silent = set(filter(None, os.environ.get("FAKE_SILENT_SEAL", "").split(",")))
for line in sys.stdin:
    op = json.loads(line)
    name = op["op"]
    key = f"{name}:{op.get('slot_id', '')}"
    if key in fail or name in fail:
        rep = {"error": f"fixture {key}"}
    elif name == "configure":
        rep = {"ok": {"listen": "127.0.0.1:1"}}
    elif name == "register_slot":
        rep = {"ok": {"token": "tok-" + op["slot_id"]}}
    elif name == "seal_recording" and op["slot_id"] in silent:
        rep = {"neither": True}
    elif name == "seal_recording":
        rep = {"ok": {"recording_ref": "r", "recording": {}}}
    else:
        rep = {"ok": {}}
    print(json.dumps(rep), flush=True)
    if name == "stop":
        break
'''


class SessionEndTests(unittest.TestCase):
    def run_shim(self, fail="", seal=False, silent=""):
        d = tempfile.mkdtemp()
        fake = os.path.join(d, "fake-proxy")
        with open(fake, "w") as f:
            f.write(FAKE)
        os.chmod(fake, 0o755)
        for n in ("lane.json", "auth.json"):
            with open(os.path.join(d, n), "w") as f:
                json.dump({}, f)
        sentinel = os.path.join(d, "done")
        open(sentinel, "w").close()   # close immediately
        sess = os.path.join(d, "s", "session.json")
        env = dict(os.environ, FAKE_FAIL=fail, FAKE_SILENT_SEAL=silent,
                   OLLAMA_API_KEY="fixture-not-a-key")
        cmd = [sys.executable, str(SHIM), "--proxy-bin", fake,
               "--store-root", d, "--run-id", "run-1", "--listen", "127.0.0.1:0",
               "--lane-file", os.path.join(d, "lane.json"),
               "--authorization-file", os.path.join(d, "auth.json"),
               "--slots", "a:1:high,b:1:high", "--session-out", sess,
               "--sentinel", sentinel, "--poll-seconds", "1"]
        if seal:
            cmd.append("--seal-recordings")
        p = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=60)
        with open(sess + ".end") as f:
            return p.returncode, json.load(f)

    def test_all_acknowledged_is_clean(self):
        rc, end = self.run_shim(seal=True)
        self.assertEqual(rc, 0)
        self.assertEqual(end["revoked"], ["a", "b"])
        self.assertEqual(end["stop"], "ok")
        self.assertNotIn("revoke_failed", end)

    def test_failed_revoke_is_not_listed_revoked_and_exits_nonzero(self):
        rc, end = self.run_shim(fail="revoke_slot:b")
        self.assertNotEqual(rc, 0)
        self.assertEqual(end["revoked"], ["a"])
        self.assertIn("b", end["revoke_failed"])

    def test_failed_seal_and_stop_are_recorded(self):
        rc, end = self.run_shim(fail="seal_recording:a,stop", seal=True)
        self.assertEqual(rc, 3)
        self.assertIn("a", end["seal_failed"])
        self.assertTrue(end["stop"].startswith("failed{run_end_probe:"))

    def test_seal_reply_with_neither_ok_nor_error_is_a_failure(self):
        rc, end = self.run_shim(seal=True, silent="b")
        self.assertEqual(rc, 3)
        self.assertEqual(end["seal_failed"], {"b": "unacknowledged"})
        self.assertEqual(end["revoked"], ["a", "b"])

    def test_run_end_probe_failure_alone_exits_4_with_all_revoked(self):
        rc, end = self.run_shim(fail="stop")
        self.assertEqual(rc, 4)
        self.assertEqual(end["revoked"], ["a", "b"])
        self.assertTrue(end["stop"].startswith("failed{run_end_probe:"))


if __name__ == "__main__":
    unittest.main()
