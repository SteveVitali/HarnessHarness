#!/usr/bin/env python3
"""Rejection controls for the hash-bound historical publication views."""
import copy
import hashlib
import json
import shutil
import sys
import tempfile
import unittest
from html.parser import HTMLParser
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "scripts/site"))
from hhsite import publication  # noqa: E402
import build  # noqa: E402

PUB = "docs/results/publication/"
RELOCATIONS = PUB + "relocations/2026-11-19-l1.json"
ORIGINAL = PUB + "phase-3-2026-11-19.json"
FLIP = PUB + "phase-3-2026-11-19-flip.json"


class Relocations(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="publication-relocations-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        for src in sorted((ROOT / PUB).glob("*.json")):
            self.copy(src.relative_to(ROOT))
            doc = json.loads(src.read_text())
            self.copy(doc["readout"])
        self.copy(RELOCATIONS)
        manifest = self.read(RELOCATIONS)
        for entry in manifest["records"]:
            for move in entry["relocations"]:
                self.copy(move["to"])
        raw = self.read(ORIGINAL)
        self.inputs = [SimpleNamespace(
            path=i["path"].replace("/compute/", "/l1/"),
            sha256=i["sha256"], phase=raw["phase"])
            for i in raw["inputs"]]

    def copy(self, rel):
        dest = self.root / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / rel, dest)

    def read(self, rel):
        return json.loads((self.root / rel).read_text())

    def write(self, rel, doc):
        (self.root / rel).write_text(json.dumps(doc, sort_keys=True) + "\n")

    def manifest(self, mutate):
        doc = self.read(RELOCATIONS)
        mutate(doc)
        self.write(RELOCATIONS, doc)

    def refused(self):
        _records, errors = publication.load_dir(self.root)
        self.assertTrue(errors, "malformed correction must fail closed")

    def records(self):
        records, errors = publication.load_dir(self.root)
        self.assertEqual(errors, [])
        return dict(records)

    def test_positive_preserves_original_bytes_and_semantics(self):
        before = {p: p.read_bytes() for p in (self.root / PUB).glob("*.json")}
        records = self.records()
        for rel, view in records.items():
            original = self.read(rel)
            expected = copy.deepcopy(original)
            if "supersedes" in expected:
                expected["supersedes"] = PUB + expected["supersedes"]
            self.assertEqual(view, expected)
            self.assertEqual((self.root / rel).read_bytes(), before[self.root / rel])
        self.assertEqual(publication.checked_pin_errors(
            ORIGINAL, records[ORIGINAL], self.inputs, self.root), [])
        self.assertTrue(publication.pin_errors(records[ORIGINAL], self.inputs))
        self.assertEqual(publication.validate(self.read(FLIP), FLIP, self.root), [])

    def test_wrong_record_hash(self):
        self.manifest(lambda d: d["records"][0].update(sha256="0" * 64))
        self.refused()

    def test_tampered_semantic_record(self):
        doc = self.read(FLIP)
        doc["dataset"]["visibility"] = "private"
        self.write(FLIP, doc)
        self.refused()

    def test_unsupported_semantic_correction(self):
        self.manifest(lambda d: d["records"][1].update(
            metadata={"dataset": {"visibility": "private"}}))
        self.refused()

    def test_unsupported_manifest_field(self):
        self.manifest(lambda d: d.update(ignore_pins=True))
        self.refused()

    def test_malformed_manifest_containers_and_types(self):
        baseline = self.read(RELOCATIONS)
        cases = [None, [], "manifest", 7]
        for field, value in [("records", {}), ("records", [None]),
                             ("records", ["entry"]), ("reason", None)]:
            doc = copy.deepcopy(baseline)
            doc[field] = value
            cases.append(doc)
        for field, value in [("record", []), ("sha256", {}),
                             ("relocations", {}), ("relocations", [None])]:
            doc = copy.deepcopy(baseline)
            doc["records"][0][field] = value
            cases.append(doc)
        for number, doc in enumerate(cases):
            with self.subTest(case=number):
                self.write(RELOCATIONS, doc)
                _records, errors = publication.load_dir(self.root)
                self.assertTrue(errors)
                self.assertTrue(any("publication relocation:" in e for e in errors), errors)

    def test_duplicate_json_fields(self):
        baseline = (self.root / RELOCATIONS).read_text()
        for key in ["schema", "sha256", "to"]:
            with self.subTest(field=key):
                needle = '"%s":' % key
                text = baseline.replace(needle, needle + 'null,' + needle, 1)
                (self.root / RELOCATIONS).write_text(text)
                _records, errors = publication.load_dir(self.root)
                self.assertTrue(any("duplicate JSON field" in e for e in errors), errors)

    def test_malformed_json_syntax(self):
        (self.root / RELOCATIONS).write_text('{"schema":')
        _records, errors = publication.load_dir(self.root)
        self.assertTrue(any("publication relocation: invalid JSON" in e for e in errors))

    def test_unknown_schema(self):
        self.manifest(lambda d: d.update(schema="unknown/1"))
        self.refused()

    def test_duplicate_correction(self):
        self.manifest(lambda d: d["records"].append(copy.deepcopy(d["records"][0])))
        self.refused()

    def test_missing_record(self):
        (self.root / ORIGINAL).unlink()
        self.refused()

    def test_date_change(self):
        doc = self.read(FLIP)
        doc["recorded_at"] = "2026-11-20 (HUMAN-H17 flip 04:23Z)"
        self.write(FLIP, doc)
        digest = hashlib.sha256((self.root / FLIP).read_bytes()).hexdigest()
        self.manifest(lambda d: d["records"][1].update(sha256=digest))
        self.refused()

    def test_phase_change(self):
        doc = self.read(FLIP)
        doc["phase"] = "phase-4"
        self.write(FLIP, doc)
        digest = hashlib.sha256((self.root / FLIP).read_bytes()).hexdigest()
        self.manifest(lambda d: d["records"][1].update(sha256=digest))
        self.refused()

    def test_predecessor_change(self):
        doc = self.read(FLIP)
        doc["supersedes"] = "phase-4-2026-11-19.json"
        self.write(FLIP, doc)
        digest = hashlib.sha256((self.root / FLIP).read_bytes()).hexdigest()
        self.manifest(lambda d: d["records"][1].update(sha256=digest))
        self.refused()

    def test_missing_predecessor(self):
        # This predecessor has no correction entry: exercise the original
        # validator's existence check, not a missing correction target.
        (self.root / (PUB + "phase-4-2026-11-19.json")).unlink()
        self.refused()

    def test_packet_bytes_changed(self):
        move = self.read(RELOCATIONS)["records"][0]["relocations"][0]
        p = self.root / move["to"]
        p.write_bytes(p.read_bytes() + b" ")
        self.refused()

    def test_relocation_hash_change(self):
        self.manifest(lambda d: d["records"][0]["relocations"][0].update(
            sha256="0" * 64))
        self.refused()

    def test_arbitrary_relocation(self):
        self.manifest(lambda d: d["records"][0]["relocations"][0].update(
            to="docs/build/reports/elsewhere.json"))
        self.refused()

    def test_relocation_path_traversal(self):
        self.manifest(lambda d: d["records"][0]["relocations"][0].update(
            to="../outside.json"))
        self.refused()

    def test_relocation_symlink_escape(self):
        move = self.read(RELOCATIONS)["records"][0]["relocations"][0]
        p = self.root / move["to"]
        with tempfile.TemporaryDirectory(prefix="publication-outside-") as outside:
            target = Path(outside) / "same-bytes.json"
            target.write_bytes(p.read_bytes())
            p.unlink()
            p.symlink_to(target)
            self.refused()

    def test_duplicate_relocation(self):
        self.manifest(lambda d: d["records"][0]["relocations"].append(
            copy.deepcopy(d["records"][0]["relocations"][0])))
        self.refused()

    def test_old_and_new_bytes_differ(self):
        move = self.read(RELOCATIONS)["records"][0]["relocations"][0]
        p = self.root / move["from"]
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(b"different original bytes")
        self.refused()

    def test_original_digest_not_laundered(self):
        doc = self.read(ORIGINAL)
        doc["inputs_digest"] = "sha256:" + "0" * 64
        self.write(ORIGINAL, doc)
        digest = hashlib.sha256((self.root / ORIGINAL).read_bytes()).hexdigest()
        self.manifest(lambda d: d["records"][0].update(sha256=digest))
        self.refused()

    def test_uncorrected_malformed_record_still_refused(self):
        self.write(PUB + "invalid.json", self.read(FLIP))
        self.refused()

    def test_no_relocation_means_original_pin_checks(self):
        (self.root / RELOCATIONS).unlink()
        records = self.records()
        self.assertTrue(publication.checked_pin_errors(
            ORIGINAL, records[ORIGINAL], self.inputs, self.root))

    def test_original_gate_validation_still_runs(self):
        doc = self.read(FLIP)
        doc["gate_row"] = "GATE-G99"
        self.write(FLIP, doc)
        digest = hashlib.sha256((self.root / FLIP).read_bytes()).hexdigest()
        self.manifest(lambda d: d["records"][1].update(sha256=digest))
        self.refused()

    def test_current_pin_hash_change(self):
        records = self.records()
        self.inputs[0].sha256 = "0" * 64
        self.assertTrue(publication.checked_pin_errors(
            ORIGINAL, records[ORIGINAL], self.inputs, self.root))

    def test_current_pin_missing(self):
        records = self.records()
        self.assertTrue(publication.checked_pin_errors(
            ORIGINAL, records[ORIGINAL], self.inputs[1:], self.root))

    def test_current_extra_pin(self):
        records = self.records()
        self.inputs.append(SimpleNamespace(path="extra", sha256="0" * 64, phase="phase-3"))
        self.assertTrue(publication.checked_pin_errors(
            ORIGINAL, records[ORIGINAL], self.inputs, self.root))

    def test_effective_semantic_change_refused(self):
        records = self.records()
        records[ORIGINAL]["labels"]["outside_verified"] = True
        self.assertTrue(publication.checked_pin_errors(
            ORIGINAL, records[ORIGINAL], self.inputs, self.root))

    def test_published_phases_uses_checked_pin_views(self):
        records = list(self.records().items())
        self.assertEqual(publication.published_phases(
            records, self.inputs, self.root), {"phase-3"})
        self.inputs[0].sha256 = "0" * 64
        self.assertNotIn("phase-3", publication.published_phases(
            records, self.inputs, self.root))

    def test_actual_public_pages_archives_and_links(self):
        class Links(HTMLParser):
            def __init__(self):
                super().__init__()
                self.hrefs = []

            def handle_starttag(self, tag, attrs):
                if tag == "a":
                    self.hrefs.extend(v for k, v in attrs if k == "href")

        inputs, _digest, _bodies = build.emit_bodies()
        records = build.load_publications(inputs)
        self.assertEqual(publication.published_phases(records, inputs, ROOT),
                         {"phase-1", "phase-3", "phase-4"})
        out = self.root / "actual-public"
        build.run_out(out, "public")
        home = Links()
        home.feed((out / "index.html").read_text())
        for phase in ["phase-3", "phase-4"]:
            text = (out / phase / "index.html").read_text()
            self.assertNotIn("publication gate pending", text)
            self.assertIn("first-pass", text)
            self.assertTrue(any(h.rstrip("/").endswith(phase) for h in home.hrefs),
                            "published phase navigation link absent")
        changelog = (out / "changelog/index.html").read_text()
        for rel, doc in records:
            version = Path(rel).stem
            archive = (out / "archive" / version / "index.html").read_text()
            self.assertIn(rel, changelog)
            self.assertIn(rel, archive)
            self.assertIn(doc["recorded_at"], archive)
            for pin in doc["inputs"]:
                self.assertIn(pin["path"], archive)
                self.assertIn(pin["sha256"][:12], archive)
        for phase in ["phase-3", "phase-4"]:
            archive = (out / "archive" / (phase + "-2026-11-19") / "index.html").read_text()
            self.assertIn("superseded by " + phase + "-2026-11-19-flip", archive)


if __name__ == "__main__":
    unittest.main()
