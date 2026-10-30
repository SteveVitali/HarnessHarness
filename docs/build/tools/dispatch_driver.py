#!/usr/bin/env python3
"""dispatch_driver.py — hh-dispatch-driver/1, the versioned guarded dispatch
driver for the Round-5 native launch-path proof (R5.2; REQ-H-008, §9.4 AC-9-2).

The driver performs every step of the coordinator's launch path up to the
literal spawn: it reads the ledger/manifest, runs the retained
hh-supplement-guard/1 (PASS with a DISPATCH route is causally required before
any envelope is emitted), re-verifies chain-tip/base ancestry, consumes the
recorded ci-boundary/1 read for the previous ticket's PR, resolves the
first-eligible unlanded row, and emits an exact hh-dispatch-envelope/1 —
profile, title, prompt and expected evidence — for the coordinator to execute
on the harness's native fresh-worker primitive (ledger dispatchTarget:
subagent → run_subagent/subagent_general on Devin Desktop).

`validate` then checks the coordinator's recorded hh-dispatch-transcript/1:
ordered calls (guard before spawn, guard again after the boundary/state
transition), exactly the contracted spawn count, prompt/profile identity,
versions and input digests, a clean product tree, zero provider budget, and —
for the resume leg — a fresh ledger re-read before the resumed stub acts.

Read-only on build state like the guard: it never mutates LEDGER/BUILD_INDEX/
manifest/coverage; a stub envelope's transcript contract allows only the
gitignored report path it names.

Usage:
  dispatch_driver.py plan --leg {happy,resume} --ci-json FILE
      [--root DIR] [--checker-json FILE | --no-checker | --guard-json FILE]
      [--envelope-out FILE] [--json]
  dispatch_driver.py validate --envelope FILE --transcript FILE [--json]
  dispatch_driver.py --version

Exit: 0 = EMIT / VALID · 1 = REFUSE / INVALID · 2 = invocation or missing
prerequisite (a missing prerequisite is never a pass).
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys

DRIVER_SCHEMA = "hh-dispatch-driver/1"
DRIVER_VERSION = "1.0.0"
DECISION_SCHEMA = "hh-dispatch-decision/1"
ENVELOPE_SCHEMA = "hh-dispatch-envelope/1"
TRANSCRIPT_SCHEMA = "hh-dispatch-transcript/1"
VALIDATION_SCHEMA = "hh-dispatch-validation/1"
NATIVE_PRIMITIVE = "run_subagent"
NATIVE_PROFILE = "subagent_general"
LEDGER = "docs/build/LEDGER.md"
MANIFEST = "docs/tickets/00_MANIFEST.md"
CI_OK_STATES = {"pass", "none-declared", "not-applicable"}


def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()


def git(root, *args, check=True):
    return subprocess.run(["git", "-C", root, *args], capture_output=True,
                          text=True, check=check)


class Driver:
    def __init__(self, root):
        self.root = root
        self.diags = []           # (code, message)
        self.steps = []           # ordered preflight calls

    def err(self, code, msg):
        self.diags.append((code, msg))

    def step(self, name, result, detail):
        self.steps.append({"step": name, "result": result, "detail": detail})

    def path(self, rel):
        return os.path.join(self.root, rel)

    # ---------------- inputs ----------------

    def load_state(self):
        p = self.path(LEDGER)
        if not os.path.isfile(p):
            self.err("missing-input", f"{LEDGER} missing")
            return {}
        text = open(p, encoding="utf-8").read()
        m = re.search(r"^## CURRENT STATE\s*```\s*(.*?)```", text, re.S | re.M)
        if not m:
            self.err("ledger-state", "LEDGER.md has no CURRENT STATE block")
            return {}
        state = {}
        for line in m.group(1).splitlines():
            kv = re.match(r"^([A-Za-z]+):\s+(.*)$", line.strip())
            if kv:
                state[kv.group(1)] = kv.group(2).strip()
        return state

    def manifest_row(self, row_id):
        p = self.path(MANIFEST)
        if not os.path.isfile(p):
            self.err("missing-input", f"{MANIFEST} missing")
            return None
        for line in open(p, encoding="utf-8"):
            if not line.startswith("|"):
                continue
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) < 6:
                continue
            fm = re.match(r"^`?(\d+[a-z]*_([A-Za-z0-9.-]+)__[^`]+?\.md)`?$",
                          cells[1])
            if fm and fm.group(2) == row_id:
                return {"seq": cells[0], "file": fm.group(1), "id": row_id,
                        "kind": cells[3]}
        self.err("resolve", f"row {row_id} not found in the manifest chain table")
        return None

    # ---------------- preflight steps ----------------

    def step_dispatch_target(self, state):
        target = state.get("dispatchTarget", "")
        if target != "subagent":
            self.err("dispatch-target",
                     f"dispatchTarget {target!r} != 'subagent' — this driver "
                     "proves the native subagent launch route only")
            self.step("dispatch-target", "fail", target)
        else:
            self.step("dispatch-target", "ok",
                      f"{target} → {NATIVE_PRIMITIVE}/{NATIVE_PROFILE}")
        return target == "subagent"

    def step_guard(self, checker_json, no_checker, guard_json):
        tool = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                            "supplement_guard.py")
        if guard_json:
            try:
                out = open(guard_json, encoding="utf-8").read()
            except OSError as e:
                self.err("guard-run", f"cannot read --guard-json: {e}")
                self.step("guard", "fail", "unreadable --guard-json")
                return False, None
        else:
            if not os.path.isfile(tool):
                self.err("guard-run", f"guard tool missing at {tool}")
                self.step("guard", "fail", "tool missing")
                return False, None
            cmd = [sys.executable, tool, "--root", self.root, "--json"]
            if checker_json:
                cmd += ["--checker-json", checker_json]
            elif no_checker:
                cmd += ["--no-checker"]
            try:
                r = subprocess.run(cmd, capture_output=True, text=True,
                                   timeout=600)
            except Exception as e:  # noqa: BLE001 — report, never crash
                self.err("guard-run", f"guard invocation failed: {e}")
                self.step("guard", "fail", str(e))
                return False, None
            out = r.stdout
            if r.returncode not in (0, 1):
                self.err("guard-run", f"guard exited {r.returncode}: "
                                      f"{r.stderr.strip()[:200]}")
                self.step("guard", "fail", f"exit {r.returncode}")
                return False, None
        try:
            g = json.loads(out)
        except json.JSONDecodeError as e:
            self.err("guard-run", f"guard output not parseable: {e}")
            self.step("guard", "fail", "unparseable output")
            return False, None
        if g.get("schema") != "hh-supplement-guard/1":
            self.err("guard-run",
                     f"guard schema {g.get('schema')!r} != hh-supplement-guard/1")
        for d in g.get("diagnostics", []):
            if d.get("severity") == "error":
                self.err(d.get("code", "guard-error"),
                         f"guard: {d.get('message', '')}")
        verdict = g.get("verdict")
        route = g.get("route", {})
        if verdict != "PASS":
            self.err("guard-fail",
                     "guard verdict FAIL — no dispatch may be emitted")
            self.step("guard", "fail",
                      f"verdict {verdict}; route {route.get('verdict')} "
                      f"{route.get('detail')}")
            return False, g
        if route.get("verdict") != "DISPATCH":
            self.err("route-not-dispatch",
                     f"guard route {route.get('verdict')} "
                     f"({route.get('detail')}) — nothing dispatches")
            self.step("guard", "fail", f"route {route.get('verdict')}")
            return False, g
        self.step("guard", "ok",
                  f"PASS v{g.get('guard_version')} route DISPATCH "
                  f"{route.get('detail')}")
        return True, g

    def step_ancestry(self, state):
        try:
            if git(self.root, "rev-parse", "--git-dir",
                   check=False).returncode != 0:
                self.step("ancestry", "skipped", "root is not a git checkout")
                return True, None
            tip = state.get("chainTip", "")
            base_branch = state.get("buildBranchBase", "")
            pinned = state.get("pinnedBaseSha", "")
            for ref in (tip, base_branch):
                if git(self.root, "rev-parse", "--verify",
                       f"{ref}^{{commit}}", check=False).returncode != 0:
                    self.err("base-ancestry", f"ref {ref!r} does not resolve")
                    self.step("ancestry", "fail", f"{ref} unresolvable")
                    return False, None
            ok1 = git(self.root, "merge-base", "--is-ancestor",
                      base_branch, tip, check=False).returncode == 0
            ok2 = git(self.root, "merge-base", "--is-ancestor",
                      pinned, tip, check=False).returncode == 0
            if not (ok1 and ok2):
                self.err("base-ancestry",
                         f"ancestry fails: {base_branch} ancestor of {tip}: "
                         f"{ok1}; {pinned[:12]} ancestor of {tip}: {ok2}")
                self.step("ancestry", "fail", "merge-base check failed")
                return False, None
            sha = git(self.root, "rev-parse", tip).stdout.strip()
            self.step("ancestry", "ok",
                      f"{base_branch} ≤ {tip}; pinned {pinned[:12]} ≤ tip")
            return True, sha
        except FileNotFoundError:
            self.step("ancestry", "skipped", "git not available")
            return True, None

    def step_ci(self, ci_json):
        if not ci_json:
            self.err("ci-missing",
                     "no --ci-json ci-boundary/1 record — a missing boundary "
                     "read is never green")
            self.step("ci", "fail", "record missing")
            return False, None
        try:
            with open(ci_json, encoding="utf-8") as f:
                data = json.load(f)
        except (OSError, json.JSONDecodeError) as e:
            self.err("ci-unreadable", f"cannot read {ci_json}: {e}")
            self.step("ci", "fail", "unreadable record")
            return False, None
        if data.get("schema") != "ci-boundary/1":
            self.err("ci-unreadable",
                     f"ci record schema {data.get('schema')!r} != ci-boundary/1")
            self.step("ci", "fail", "wrong schema")
            return False, None
        state = data.get("state")
        if state not in CI_OK_STATES:
            self.err("ci-not-green",
                     f"ci-boundary state {state!r} on #{data.get('pr')} — "
                     f"{data.get('line', '')[:160]}")
            self.step("ci", "fail", f"state {state} pr {data.get('pr')}")
            return False, data
        self.step("ci", "ok",
                  f"{data.get('line', 'state ' + str(state))[:160]}")
        return True, data

    # ---------------- plan ----------------

    def plan(self, leg, ci_path, checker_json, no_checker, guard_json):
        state = self.load_state()
        # ok accumulates only the ordered steps' own verdicts — each gate is
        # independently load-bearing (a single neutered check must flip EMIT).
        dt_ok = self.step_dispatch_target(state)
        g_ok, guard = self.step_guard(checker_json, no_checker, guard_json)
        anc_ok, base_sha = self.step_ancestry(state)
        ci_ok, ci = self.step_ci(ci_path)
        ok = dt_ok and g_ok and anc_ok and ci_ok
        envelope = None
        if ok:
            row_id = guard.get("route", {}).get("detail")
            if not row_id:
                self.err("route-not-dispatch", "guard route lacks a row id")
            row = self.manifest_row(row_id) if row_id else None
            if row is None:
                ok = False
            else:
                self.step("resolve", "ok",
                          f"{row['id']} seq {row['seq']} kind {row['kind']}")
                envelope = self.make_envelope(leg, row, state, guard, ci,
                                              base_sha)
                self.step("emit", "ok", f"envelope for {row['id']} leg {leg}")
        verdict = "EMIT" if ok and envelope is not None else "REFUSE"
        return {"schema": DECISION_SCHEMA, "driver_version": DRIVER_VERSION,
                "leg": leg, "verdict": verdict, "ordered_steps": self.steps,
                "diagnostics": [{"code": c, "message": m}
                                for c, m in self.diags],
                "envelope": envelope}

    def make_envelope(self, leg, row, state, guard, ci, base_sha):
        worktree = state.get("buildWorktree", self.root)
        if leg == "resume":
            spawn_count, resume_count = 2, 1
            allowed = ["docs/build/logs/stub-r5.2-resume.json"]
            prompt = self.stub_prompt_resume(worktree, allowed[0])
            attempt1 = self.stub_prompt_attempt1(worktree)
            title = f"{row['id']} stub dispatch — resume leg (hh-dispatch-driver/1)"
            order = ["guard.pre", "spawn.1", "stub_report.1(interrupted)",
                     "spawn.2(resume)", "ledger_reread", "stub_report.2",
                     "guard.post"]
        else:
            spawn_count, resume_count = 1, 0
            allowed = ["docs/build/logs/stub-r5.2-happy.json"]
            prompt = self.stub_prompt_happy(worktree, allowed[0])
            title = f"{row['id']} stub dispatch — happy leg (hh-dispatch-driver/1)"
            order = ["guard.pre", "spawn.1", "stub_report.1", "guard.post"]
        return {
            "schema": ENVELOPE_SCHEMA, "driver_version": DRIVER_VERSION,
            "leg": leg,
            "row": row,
            "dispatch": dict(
                {"primitive": NATIVE_PRIMITIVE, "profile": NATIVE_PROFILE,
                 "worker_kind": "stub", "title": title, "prompt": prompt},
                **({"attempt1_prompt": attempt1} if leg == "resume" else {})),
            "inputs": {
                "guard": {"schema": guard.get("schema"),
                          "guard_version": guard.get("guard_version"),
                          "verdict": guard.get("verdict"),
                          "route": guard.get("route"),
                          "counts": guard.get("counts"),
                          "input_digests": guard.get("input_digests")},
                "ci": {k: ci.get(k) for k in
                       ("schema", "pr", "head_sha", "state", "line")},
                "ledger": {k: state.get(k) for k in
                           ("nextTicket", "lastCompleted", "chainTip",
                            "buildBranchBase", "pinnedBaseSha", "updatedAt")},
                "base_branch": state.get("chainTip"),
                "expected_base_sha": base_sha,
            },
            "transcript_contract": {
                "schema": TRANSCRIPT_SCHEMA,
                "driver_version": DRIVER_VERSION,
                "required_order": order,
                "spawn_count": spawn_count,
                "resume_count": resume_count,
                "allowed_writes": allowed,
                "required_spend_usd": 0,
                "required_tree_status": "clean",
                "forbidden": ["product_mutation", "provider_budget",
                              "network", "commits_or_branches"],
            },
        }

    @staticmethod
    def stub_prompt_happy(worktree, report):
        return (
            "You are a harmless STUB worker proving the guarded native dispatch "
            "path (R5.2, hh-dispatch-driver/1). You MUST NOT implement, edit, "
            "commit or dispatch anything.\n"
            f"Working directory: {worktree}\n"
            "Do exactly this, in order, then stop:\n"
            "1. Run `bash scripts/deferrals-open.sh`; record its output verbatim.\n"
            "2. Read docs/build/LEDGER.md CURRENT STATE; record nextTicket, "
            "lastCompleted, chainTip, updatedAt.\n"
            "3. Record `git rev-parse HEAD` and `git branch --show-current`.\n"
            f"4. Write one JSON object to {report} with keys: schema "
            '"hh-stub-report/1", leg "happy", deferrals_open (verbatim), '
            "observed_state {nextTicket,lastCompleted,chainTip,updatedAt}, "
            "head_sha, branch, writes [the report path], actions (the ordered "
            "steps), spend_usd 0.\n"
            "5. Return that JSON object as your reply. Do not touch any other "
            "file; run no builds or tests; use no network; create no commits "
            "or branches; spend USD 0."
        )

    @staticmethod
    def stub_prompt_attempt1(worktree):
        return (
            "You are the FIRST attempt of a harmless STUB dispatch (R5.2, "
            "hh-dispatch-driver/1). You MUST NOT implement, edit, commit or "
            "dispatch anything. This attempt SIMULATES an interruption.\n"
            f"Working directory: {worktree}\n"
            "Do exactly this, in order, then stop — simulating a worker that "
            "dies before finishing:\n"
            "1. Record `git rev-parse HEAD` and `git branch --show-current`.\n"
            "2. Return a JSON object {\"schema\": \"hh-stub-report/1\", \"leg\": "
            "\"resume\", \"attempt\": 1, \"interrupted\": true, \"head_sha\": "
            "..., \"branch\": ..., \"actions\": [\"git-rev-parse\"], "
            "\"spend_usd\": 0} as your reply — nothing else, no file writes."
        )

    @staticmethod
    def stub_prompt_resume(worktree, report):
        return (
            "You are the RESUMED half of a harmless STUB dispatch (R5.2, "
            "hh-dispatch-driver/1) continuing after a simulated interruption. "
            "You MUST NOT implement, edit, commit or dispatch anything, and "
            "you carry NO trustworthy state from the interrupted attempt.\n"
            f"Working directory: {worktree}\n"
            "Do exactly this, in order, then stop:\n"
            "1. FIRST ACTION — re-read fresh state before anything else: run "
            "`bash scripts/deferrals-open.sh` and read docs/build/LEDGER.md "
            "CURRENT STATE; record nextTicket, lastCompleted, chainTip, "
            "updatedAt as ledger_reread.\n"
            "2. Record `git rev-parse HEAD` and `git branch --show-current`.\n"
            f"3. Write one JSON object to {report} with keys: schema "
            '"hh-stub-report/1", leg "resume", attempt 2, ledger_reread '
            "{nextTicket,lastCompleted,chainTip,updatedAt}, head_sha, branch, "
            "writes [the report path], actions (the ordered steps), "
            "spend_usd 0.\n"
            "4. Return that JSON object as your reply. Do not touch any other "
            "file; run no builds or tests; use no network; create no commits "
            "or branches; spend USD 0."
        )

    # ---------------- validate ----------------

    def validate(self, envelope_path, transcript_path):
        try:
            env_bytes = open(envelope_path, "rb").read()
            env = json.loads(env_bytes)
        except (OSError, json.JSONDecodeError) as e:
            self.err("validation-error", f"envelope unreadable: {e}")
            return self.validation_result(None)
        try:
            tr = json.load(open(transcript_path, encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as e:
            self.err("validation-error", f"transcript unreadable: {e}")
            return self.validation_result(env)
        if env.get("schema") != ENVELOPE_SCHEMA:
            self.err("validation-error",
                     f"envelope schema {env.get('schema')!r} != "
                     f"{ENVELOPE_SCHEMA}")
            return self.validation_result(env)
        if tr.get("schema") != TRANSCRIPT_SCHEMA:
            self.err("validation-error",
                     f"transcript schema {tr.get('schema')!r} != "
                     f"{TRANSCRIPT_SCHEMA}")
        if tr.get("driver_version") != env.get("driver_version"):
            self.err("envelope-mismatch",
                     f"driver_version {tr.get('driver_version')} != envelope "
                     f"{env.get('driver_version')}")
        if tr.get("leg") != env.get("leg"):
            self.err("envelope-mismatch",
                     f"transcript leg {tr.get('leg')!r} != envelope leg "
                     f"{env.get('leg')!r}")
        if tr.get("envelope_sha256") != sha256_bytes(env_bytes):
            self.err("envelope-mismatch",
                     "transcript envelope_sha256 does not match the envelope "
                     "file bytes — input identity broken")
        contract = env.get("transcript_contract", {})
        events = tr.get("events", [])
        if not isinstance(events, list) or \
                any(not isinstance(e, dict) for e in events):
            self.err("validation-error",
                     "transcript events must be a list of objects")
            events = [e for e in events if isinstance(e, dict)] \
                if isinstance(events, list) else []
        self.check_events(env, contract, events)
        if tr.get("tree_status") != contract.get("required_tree_status",
                                                "clean"):
            self.err("product-mutation",
                     f"tree_status {tr.get('tree_status')!r} != "
                     "'clean' — the stub mutated the product tree")
        for ev in events:
            if ev.get("kind") == "stub_report":
                if ev.get("spend_usd", 0) not in (0, 0.0):
                    self.err("provider-budget",
                             f"stub_report attempt {ev.get('attempt')} spent "
                             f"{ev.get('spend_usd')} — budget is USD 0")
                for w in ev.get("writes", []):
                    if w not in contract.get("allowed_writes", []):
                        self.err("product-mutation",
                                 f"stub wrote {w!r} outside allowed_writes "
                                 f"{contract.get('allowed_writes')}")
                if not ev.get("actions") and not ev.get("interrupted"):
                    self.err("no-progress",
                             f"stub_report attempt {ev.get('attempt')} records "
                             "no actions — no-progress is not a launch")
        return self.validation_result(env)

    def check_events(self, env, contract, events):
        # 1. Guard causality: the first event must be the pre-dispatch guard,
        #    and no spawn may precede it — an absent/mocked guard is a failure.
        spawns = [e for e in events if e.get("kind") == "spawn"]
        guards = [e for e in events if e.get("kind") == "guard"]
        reports = [e for e in events if e.get("kind") == "stub_report"]
        if not guards:
            self.err("guard-required",
                     "transcript records no guard run — a dispatch without "
                     "the retained guard is refused, mock or not")
        elif events and events[0].get("kind") != "guard":
            self.err("ordering",
                     f"first event is {events[0].get('kind')}, not guard.pre")
        elif spawns and guards:
            first_guard = events.index(guards[0])
            first_spawn = events.index(spawns[0])
            if first_spawn < first_guard:
                self.err("ordering",
                         "spawn precedes the pre-dispatch guard — guard "
                         "success must be causally required before launch")
        phases = {g.get("phase") for g in guards}
        if guards and ("pre" not in phases or "post" not in phases):
            self.err("guard-required",
                     f"guard phases {sorted(phases)} — the contract needs "
                     "guard.pre before launch and guard.post after the "
                     "boundary/state transition")
        for g in guards:
            if g.get("phase") == "post":
                if spawns and events.index(g) < events.index(spawns[-1]):
                    self.err("ordering",
                             "guard.post precedes the last spawn — the "
                             "post-boundary re-check must follow the spawn")
            if g.get("verdict") != "PASS":
                self.err("guard-required",
                         f"guard.{g.get('phase')} verdict "
                         f"{g.get('verdict')!r} != PASS")
            route = g.get("route", "")
            if env.get("row", {}).get("id") and \
                    env["row"]["id"] not in str(route):
                self.err("envelope-mismatch",
                         f"guard route {route!r} does not name the dispatched "
                         f"row {env['row']['id']}")
            exp = env.get("inputs", {}).get("guard", {})
            if g.get("guard_version") != exp.get("guard_version"):
                self.err("envelope-mismatch",
                         f"guard_version {g.get('guard_version')} != envelope "
                         f"{exp.get('guard_version')}")
            if g.get("input_digests") != exp.get("input_digests"):
                self.err("state-mutation",
                         f"guard.{g.get('phase')} input digests differ from "
                         "the envelope's recorded inputs — build-memory state "
                         "changed")
        # pre/post guard digests must agree: build-memory state untouched
        if len(guards) >= 2:
            if guards[0].get("input_digests") != guards[-1].get("input_digests"):
                self.err("state-mutation",
                         "post-dispatch guard digests differ from "
                         "pre-dispatch — state mutated across the spawn")
        # 2. Spawn count + identity
        want = contract.get("spawn_count", 1)
        if len(spawns) != want:
            self.err("spawn-count",
                     f"{len(spawns)} spawn events != contracted {want} — an "
                     "eligible row dispatches exactly one stub (plus the "
                     "contracted resume attempt)")
        disp = env.get("dispatch", {})
        for s in spawns:
            if s.get("primitive") != disp.get("primitive") or \
                    s.get("profile") != disp.get("profile"):
                self.err("envelope-mismatch",
                         f"spawn {s.get('primitive')}/{s.get('profile')} != "
                         f"{disp.get('primitive')}/{disp.get('profile')}")
            want_prompt = disp.get("prompt", "")
            if s.get("attempt") == 1 and disp.get("attempt1_prompt"):
                want_prompt = disp["attempt1_prompt"]
            if s.get("prompt_sha256") != sha256_bytes(want_prompt.encode()):
                self.err("envelope-mismatch",
                         f"spawn attempt {s.get('attempt')} prompt_sha256 != "
                         "sha256(envelope prompt)")
        # 3. Per-attempt report presence — no-progress is never a launch
        for s in spawns:
            a = s.get("attempt", 1)
            if not any(r.get("attempt", 1) == a for r in reports):
                self.err("no-progress",
                         f"spawn attempt {a} produced no stub_report — "
                         "no-progress cannot count as a dispatch")
        # 4. Resume leg: a fresh ledger re-read before the resumed stub acts
        if contract.get("resume_count", 0) > 0:
            reread = [e for e in events if e.get("kind") == "ledger_reread"]
            s2 = [s for s in spawns if s.get("attempt") == 2]
            r2 = [r for r in reports if r.get("attempt") == 2]
            if not s2:
                self.err("ordering",
                         "resume contract but no attempt-2 spawn recorded")
            elif not reread:
                self.err("stale-state",
                         "resumed stub never re-read ledger state — resume "
                         "must recheck fresh state")
            elif events.index(reread[0]) < events.index(s2[0]):
                self.err("ordering",
                         "ledger_reread precedes the resume spawn")
            elif r2 and events.index(reread[0]) > events.index(r2[0]):
                self.err("stale-state",
                         "resumed stub reported before re-reading ledger "
                         "state — it acted on carried-over state")

    def validation_result(self, env):
        verdict = "INVALID" if self.diags else "VALID"
        return {"schema": VALIDATION_SCHEMA, "driver_version": DRIVER_VERSION,
                "verdict": verdict,
                "row": (env or {}).get("row", {}).get("id"),
                "leg": (env or {}).get("leg"),
                "ordered_steps": self.steps,
                "diagnostics": [{"code": c, "message": m}
                                for c, m in self.diags]}


def main():
    ap = argparse.ArgumentParser(
        description="hh-dispatch-driver/1 — guarded native dispatch driver")
    ap.add_argument("--version", action="store_true")
    sub = ap.add_subparsers(dest="cmd")
    p = sub.add_parser("plan")
    p.add_argument("--root", default=".")
    p.add_argument("--leg", choices=["happy", "resume"], default="happy")
    p.add_argument("--ci-json")
    p.add_argument("--checker-json")
    p.add_argument("--no-checker", action="store_true")
    p.add_argument("--guard-json")
    p.add_argument("--envelope-out")
    p.add_argument("--json", action="store_true")
    v = sub.add_parser("validate")
    v.add_argument("--envelope", required=True)
    v.add_argument("--transcript", required=True)
    v.add_argument("--json", action="store_true")
    args = ap.parse_args()
    if args.version:
        print(f"{DRIVER_SCHEMA} tool {DRIVER_VERSION}")
        return 0
    if args.cmd == "plan":
        d = Driver(os.path.abspath(args.root))
        res = d.plan(args.leg, args.ci_json, args.checker_json,
                     args.no_checker, args.guard_json)
        if args.envelope_out and res.get("envelope"):
            with open(args.envelope_out, "w", encoding="utf-8") as f:
                json.dump(res["envelope"], f, indent=2)
                f.write("\n")
        if args.json:
            print(json.dumps(res, indent=2))
        else:
            print(f"dispatch-driver: {res['schema']} v{res['driver_version']}"
                  f" — verdict {res['verdict']} (leg {res['leg']})")
            for s in res["ordered_steps"]:
                mark = "✓" if s["result"] == "ok" else \
                       ("·" if s["result"] == "skipped" else "✗")
                print(f"  {mark} {s['step']}: {s['detail']}")
            for dg in res["diagnostics"]:
                print(f"  ✗ [{dg['code']}] {dg['message']}")
            if res.get("envelope"):
                e = res["envelope"]["dispatch"]
                print(f"  envelope → {e['primitive']} profile={e['profile']} "
                      f"row={res['envelope']['row']['id']}")
        return 0 if res["verdict"] == "EMIT" else 1
    if args.cmd == "validate":
        d = Driver(os.getcwd())
        res = d.validate(args.envelope, args.transcript)
        if args.json:
            print(json.dumps(res, indent=2))
        else:
            print(f"dispatch-driver: {res['schema']} v{res['driver_version']}"
                  f" — verdict {res['verdict']} "
                  f"(row {res.get('row')} leg {res.get('leg')})")
            for dg in res["diagnostics"]:
                print(f"  ✗ [{dg['code']}] {dg['message']}")
        return 0 if res["verdict"] == "VALID" else 1
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
