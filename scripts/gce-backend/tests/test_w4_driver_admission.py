#!/usr/bin/env python3
"""Offline controls for two W4 admission defects; never execute driver main.

Extract the actual schedule loop and probe function. All I/O, time, and
launch hooks are in-memory stubs. These tests do not certify the remaining
continuation, receipt binding, hold classification, or teardown behavior.
"""
import argparse
import ast
import copy
import io
import json
import os
from pathlib import Path
import types
import unittest

parser = argparse.ArgumentParser(add_help=False)
parser.add_argument('--driver', type=Path,
                    default=Path(__file__).resolve().parents[1] / 'w4-main-driver.py')
args, unittest_args = parser.parse_known_args()
SOURCE = args.driver.read_text()
TREE = ast.parse(SOURCE)
MAIN = next(n for n in TREE.body if isinstance(n, ast.FunctionDef) and n.name == 'main')
LOOP = next(n for n in MAIN.body if isinstance(n, ast.For)
            and isinstance(n.target, ast.Tuple)
            and [x.id for x in n.target.elts] == ['ri', 'units'])


def drive_unit(*, at_start=0, after_wait=0, busy=False, arms=('mini-swe-agent', 'hh_native')):
    """Run the production loop only; a fake reap releases initial capacity."""
    now = [at_start]
    launches, withheld = [], []
    running = {f'prior-{i}': None for i in range(4)} if busy else {}
    members = [{'slot_id': f'slot-{arm}', 'arm': arm} for arm in arms]

    def reap():
        if running and all(k.startswith('prior-') for k in running):
            now[0] = after_wait
        running.clear()

    def launch(_args, _session, member, _directory):
        launches.append(member['slot_id'])
        return object(), None

    def withhold(_unit, member, reason):
        withheld.append((member['slot_id'], reason))

    def checkpoint(_round):
        raise AssertionError('single round test must not invoke checkpoint')

    env = {
        'rounds': [[{'members': members}]],
        'a': types.SimpleNamespace(checkpoint_round=2, concurrency=4),
        'dropped': set(), 'held_native': {}, 'results': {},
        'native_held': True, 'infra_native': [], 'running': running,
        'slot_deadline_fits': lambda: now[0] < 10,
        'reap': reap, 'withhold': withhold, 'checkpoint': checkpoint,
        'launch_member': launch, 'sess': {}, 'slots_dir': '/not-used',
        'os': os, 'time': types.SimpleNamespace(sleep=lambda _s: None, time=lambda: now[0]),
    }
    module = ast.fix_missing_locations(ast.Module(body=[copy.deepcopy(LOOP)], type_ignores=[]))
    exec(compile(module, str(args.driver), 'exec'), env)
    return launches, withheld


class AdmissionTests(unittest.TestCase):
    def test_probe_grant_and_declared_cap_are_fifty_thousand(self):
        constant = next(n for n in TREE.body if isinstance(n, ast.Assign)
                        and any(isinstance(t, ast.Name) and t.id == 'PROBE_SLICE_MICROS'
                                for t in n.targets))
        probe = next(n for n in TREE.body if isinstance(n, ast.FunctionDef)
                     and n.name == 'run_lane_probe')
        class Sink(io.StringIO):
            def close(self):
                pass
        env = {
            'http_call': lambda *_a: 200,
            'read_events': lambda _p: [{'class': 'model.call.completed',
                                       'payload': {'response_ref': 'fixture-receipt'}}],
            'open': lambda *_a, **_kw: Sink(), 'os': os, 'json': json,
            'time': types.SimpleNamespace(sleep=lambda _s: None,
                strftime=lambda *_a: 'fixture-time', gmtime=lambda: None),
        }
        module = ast.fix_missing_locations(ast.Module(
            body=[copy.deepcopy(constant), copy.deepcopy(probe)], type_ignores=[]))
        exec(compile(module, str(args.driver), 'exec'), env)
        record, _ = env['run_lane_probe'](
            types.SimpleNamespace(model='fixture', effort='high', listen='unused', store_root='unused'),
            {'grants': {'w4-lane-probe': {'slot_token': 'fixture-only'}}}, '/not-used')
        self.assertEqual(record['slot']['slice_micros'], 50_000)
        self.assertEqual(record['plan']['cap_micros'], 50_000)
        # Session registration must consume that same constant, not another cap.
        registration = next(n for n in MAIN.body if isinstance(n, ast.Assign)
                            and any(isinstance(t, ast.Name) and t.id == 'slot_lines'
                                    for t in n.targets))
        env['a'] = types.SimpleNamespace(effort='high')
        module = ast.fix_missing_locations(ast.Module(
            body=[copy.deepcopy(registration)], type_ignores=[]))
        exec(compile(module, str(args.driver), 'exec'), env)
        self.assertEqual(env['slot_lines'], ['w4-lane-probe:50000:high'])

    def test_capacity_wait_crossing_cutoff_withholds_both_without_launch(self):
        launches, withheld = drive_unit(busy=True, after_wait=11)
        self.assertEqual(launches, [])
        self.assertEqual(withheld, [('slot-mini-swe-agent', 'withheld{window_deadline}'),
                                    ('slot-hh_native', 'withheld{window_deadline}')])

    def test_capacity_wait_ending_at_cutoff_withholds_singleton(self):
        launches, withheld = drive_unit(busy=True, after_wait=10, arms=('v5',))
        self.assertEqual(launches, [])
        self.assertEqual(withheld, [('slot-v5', 'withheld{window_deadline}')])

    def test_capacity_wait_that_still_fits_preserves_pair(self):
        launches, withheld = drive_unit(busy=True, after_wait=9)
        self.assertEqual(launches, ['slot-mini-swe-agent', 'slot-hh_native'])
        self.assertEqual(withheld, [])

    def test_no_capacity_wait_preserves_pair(self):
        launches, withheld = drive_unit()
        self.assertEqual(launches, ['slot-mini-swe-agent', 'slot-hh_native'])
        self.assertEqual(withheld, [])

    def test_initial_cutoff_withholds_once(self):
        launches, withheld = drive_unit(at_start=10)
        self.assertEqual(launches, [])
        self.assertEqual(len(withheld), 2)
        self.assertEqual(len(set(withheld)), 2)


if __name__ == '__main__':
    unittest.main(argv=[__file__] + unittest_args)
