"""Hash-bound relocation views of immutable publication pins.

Relocations preserve the original pins and require identical content hashes.
"""
import copy
import hashlib
import json
import re
from pathlib import Path, PurePosixPath

DIRECTORY = "docs/results/publication"
SCHEMA = "hh-site-publication-relocations/1"
SHA = re.compile(r"^[0-9a-f]{64}$")


def _need(condition, message):
    if not condition:
        raise ValueError("publication relocation: " + message)


def _file(root, rel):
    _need(isinstance(rel, str) and rel and "\\" not in rel,
          "invalid relative path")
    parts = PurePosixPath(rel)
    _need(not parts.is_absolute() and ".." not in parts.parts
          and str(parts) == rel, "non-canonical relative path")
    root = Path(root).resolve()
    path = root / rel
    _need(path.resolve().is_relative_to(root), "path escapes repository")
    _need(path.is_file(), "file absent: " + rel)
    return path


def _hash(raw):
    return hashlib.sha256(raw).hexdigest()


def verify_record(raw, entry):
    _need(_hash(raw) == entry["sha256"],
          "immutable record hash mismatch: " + entry["record"])


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        _need(key not in result, "duplicate JSON field: " + key)
        result[key] = value
    return result


def load(root):
    """Reject unsupported, duplicate, stale or tampered correction entries."""
    result = {}
    for path in sorted((Path(root) / DIRECTORY / "relocations").glob("*.json")):
        rel = path.relative_to(root).as_posix()
        try:
            manifest = json.loads(_file(root, rel).read_text(),
                                  object_pairs_hook=_unique_object)
        except json.JSONDecodeError as e:
            raise ValueError("publication relocation: invalid JSON in %s: %s"
                             % (rel, e.msg)) from e
        _need(isinstance(manifest, dict)
              and set(manifest) == {"schema", "reason", "records"},
              "unsupported manifest fields")
        _need(manifest["schema"] == SCHEMA, "unsupported schema")
        _need(isinstance(manifest["reason"], str) and manifest["reason"].strip(),
              "reason required")
        _need(isinstance(manifest["records"], list) and manifest["records"],
              "nonempty records required")
        for entry in manifest["records"]:
            _need(isinstance(entry, dict) and set(entry) == {
                "record", "sha256", "relocations"},
                "unsupported record correction fields")
            rel = entry["record"]
            _need(isinstance(rel, str)
                  and PurePosixPath(rel).parent == PurePosixPath(DIRECTORY)
                  and rel.endswith(".json"), "record must be a publication JSON")
            _need(rel not in result, "duplicate correction: " + rel)
            _need(isinstance(entry["sha256"], str)
                  and SHA.fullmatch(entry["sha256"]), "invalid record hash")
            verify_record(_file(root, rel).read_bytes(), entry)
            _need(isinstance(entry["relocations"], list) and entry["relocations"],
                  "nonempty relocations required")
            result[rel] = entry
    return result


def relocated_inputs(doc, entry, root):
    """Comparison-only pin view. Returned records/archives retain original pins."""
    if entry is None:
        return doc["inputs"]
    inputs = copy.deepcopy(doc["inputs"])
    seen = set()
    for move in entry["relocations"]:
        _need(isinstance(move, dict) and set(move) == {"from", "to", "sha256"},
              "unsupported relocation fields")
        old, new, sha = move["from"], move["to"], move["sha256"]
        _need(isinstance(old, str) and isinstance(new, str)
              and old.startswith("docs/build/reports/compute/")
              and new == old.replace("/compute/", "/l1/", 1)
              and PurePosixPath(old).parent == PurePosixPath("docs/build/reports/compute"),
              "only the recorded compute-to-l1 same-name relocation is supported")
        _need(old not in seen, "duplicate relocation")
        seen.add(old)
        matches = [i for i in inputs if i.get("path") == old]
        _need(len(matches) == 1 and matches[0].get("sha256") == sha,
              "relocation does not match the original exact pin")
        _need(isinstance(sha, str) and SHA.fullmatch(sha), "invalid relocation hash")
        _need(not any(i.get("path") == new for i in inputs),
              "relocation would duplicate a pin")
        raw = _file(root, new).read_bytes()
        _need(_hash(raw) == sha, "relocated input bytes changed")
        if (Path(root) / old).exists():
            _need(_file(root, old).read_bytes() == raw,
                  "old and new input bytes differ")
        matches[0]["path"] = new
    return inputs
