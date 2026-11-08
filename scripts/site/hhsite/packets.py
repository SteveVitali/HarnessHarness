"""packets — named inputs (REQ-XF-119), schema check, pointer resolution.

Inputs are named in ``scripts/site/config/inputs.json``: each entry gives
``{id, path, schema, phase}`` — never discovered by glob. An unknown or
mismatched schema and a missing input each exit 2 through
``values.Refusal``.

The record input is a Markdown file whose bounds block is marked
``<!-- bounds:begin -->…<!-- bounds:end -->``; it is declared in
inputs.json with ``format: "markdown-bounds"`` and the schema token
``hh-p1-bounds-record/1`` — REQ-XF-119 names inputs by (path, schema,
phase); the record's schema is the block contract, not JSON.

Pointers are RFC-6901 JSON pointers into the packet
(``/counts/n_pairs``); into a markdown-bounds input a pointer
``/bounds/<i>`` resolves to the i-th (1-based) bounds statement text.
"""
import hashlib
import json
import re
from pathlib import Path

from .values import Refusal, check_hygiene

BOUNDS_BEGIN = "<!-- bounds:begin -->"
BOUNDS_END = "<!-- bounds:end -->"
BOUND_ITEM_RE = re.compile(r"^\s*\d+\.\s+(.*\S)\s*$", re.M)
MD_BOUNDS_FORMAT = "markdown-bounds"

# The seal (X-07): these schemas render declared presentation fixtures
# only — no real first-pass outcome reaches a page before its results
# row (REQ-XH-121). An input of a gated schema is admitted only when
# the inputs.json entry AND the packet document each carry
# ``fixture: true``; anything else is refused at load (exit 2).
FIXTURE_GATED_SCHEMAS = frozenset({"hh-comparative-results/1"})


def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()


class Input(object):
    """One named input: declared metadata + loaded content."""

    def __init__(self, entry, repo_root):
        self.repo_root = Path(repo_root)
        self.id = entry["id"]
        self.path = entry["path"]
        self.schema = entry["schema"]
        self.phase = entry["phase"]
        self.format = entry.get("format", "json")
        f = self.repo_root / self.path
        if not f.is_file():
            raise Refusal("input %r missing: %s" % (self.id, self.path))
        self.raw = f.read_bytes()
        self.sha256 = sha256_bytes(self.raw)
        if self.format == MD_BOUNDS_FORMAT:
            self.doc = self._load_bounds()
        else:
            try:
                self.doc = json.loads(self.raw.decode("utf-8"))
            except Exception as e:  # noqa: BLE001
                raise Refusal("input %r is not JSON: %s" % (self.id, e))
            declared = self.doc.get("schema") or self.doc.get("version")
            if declared != self.schema:
                raise Refusal(
                    "input %r schema mismatch: declared %r, packet "
                    "carries %r" % (self.id, self.schema, declared))
        if self.schema in FIXTURE_GATED_SCHEMAS and (
                entry.get("fixture") is not True
                or self.doc.get("fixture") is not True):
            raise Refusal(
                "input %r: %s admits declared fixtures only — the "
                "inputs.json entry and the packet must each carry "
                "fixture:true (the seal, X-07)"
                % (self.id, self.schema))
        if entry.get("page") is not None:
            self.page = entry["page"]

    def _load_bounds(self):
        text = self.raw.decode("utf-8")
        parts = text.split(BOUNDS_BEGIN)
        if len(parts) != 2 or BOUNDS_END not in parts[1]:
            raise Refusal("input %r: bounds block markers not found"
                          % self.id)
        block = parts[1].split(BOUNDS_END)[0]
        items = [m.group(1) for m in BOUND_ITEM_RE.finditer(block)]
        if not items:
            raise Refusal("input %r: bounds block holds no statements"
                          % self.id)
        return {"bounds": items, "block": block.strip()}

    def resolve(self, ptr):
        """Resolve a JSON pointer into this input. Raises Refusal on a
        missing member — a view shows 'not computed', never a gap."""
        node = self.doc
        if ptr in ("", "/"):
            return node
        if not ptr.startswith("/"):
            raise Refusal("pointer %r is not a JSON pointer" % ptr)
        for tok in ptr.lstrip("/").split("/"):
            tok = tok.replace("~1", "/").replace("~0", "~")
            if self.format == MD_BOUNDS_FORMAT and tok == "bounds":
                node = self.doc["bounds"]
                continue
            if isinstance(node, list):
                try:
                    node = node[int(tok)]
                except (ValueError, IndexError):
                    raise Refusal("pointer %r misses at %r" % (ptr, tok))
            elif isinstance(node, dict):
                if tok not in node:
                    raise Refusal("pointer %r misses at %r" % (ptr, tok))
                node = node[tok]
            else:
                raise Refusal("pointer %r descends past leaf %r"
                              % (ptr, tok))
        return node


def load_inputs(inputs_path, repo_root):
    """Load config/inputs.json and every named input. Never globs."""
    cfg_file = Path(inputs_path)
    if not cfg_file.is_file():
        raise Refusal("inputs.json missing: %s" % inputs_path)
    cfg = json.loads(cfg_file.read_text())
    if cfg.get("schema") != "hh-site-inputs/1":
        raise Refusal("inputs.json schema %r != hh-site-inputs/1"
                      % cfg.get("schema"))
    inputs = []
    seen = set()
    for entry in cfg["inputs"]:
        for key in ("id", "path", "schema", "phase"):
            if key not in entry:
                raise Refusal("input entry lacks %r: %r" % (key, entry))
        if "*" in entry["path"] or "?" in entry["path"]:
            raise Refusal("input %r path is a glob" % entry["id"])
        if entry["id"] in seen:
            raise Refusal("duplicate input id %r" % entry["id"])
        seen.add(entry["id"])
        inputs.append(Input(entry, repo_root))
    return cfg, inputs


def inputs_digest(inputs):
    """The packet-set digest (AC-13): sha256 over the canonical list of
    {path, schema, sha256} — recomputable from inputs.json + the packet
    files, nothing else."""
    rows = [{"path": i.path, "schema": i.schema, "sha256": i.sha256}
            for i in inputs]
    return "sha256:" + hashlib.sha256(
        json.dumps(rows, sort_keys=True,
                   separators=(",", ":")).encode()).hexdigest()


def input_rows(inputs):
    """The envelope member: named inputs consumed by one site-data file."""
    return [{"id": i.id, "path": i.path, "schema": i.schema,
             "sha256": i.sha256} for i in inputs]


def field(inp, ptr):
    """Copy one field with provenance: returns (raw, src_ref). Hygiene is
    enforced on copied strings by the caller-side builders."""
    from .values import src_ref
    raw = inp.resolve(ptr)
    if isinstance(raw, str):
        check_hygiene(raw, "%s#%s" % (inp.path, ptr))
    return raw, src_ref(inp.path, inp.sha256, ptr)
