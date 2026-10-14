#!/usr/bin/env python3
"""E2-candidate — a foreign (Python 3, stdlib-only) implementation of the
`hh-xcheck-bundle/1` replay contract (CHECKER.md, spec §10.7, R2.21).

Reads a cross-impl bundle directory, replays all six arms, and writes an
`answers.json` that `hh-xcheck verify <bundle> <answers>` compares against
the bundle's committed E1 expectations.

    python3 foreign.py <bundle-dir> <answers.json>

Honesty note: this is a *candidate* implementation. Its answers are evidence
the foreign run produced; a green `verify` does not mint E1 truth. The
primitives here are implemented from CHECKER.md's spec text plus the
committed fixture shapes — no Rust code was transliterated.

Stdlib only: hashlib, json, pathlib, sys, time.
"""

import hashlib
import json
import sys
import time
from pathlib import Path

# ─────────────────────────────────────────────────────────────────────────────
# Primitive 1 — canonical JSON (`hh-json/1`)
#
# Byte-sorted object keys (UTF-8 byte order), `,` member separator, `:` pair
# separator, no other whitespace. Integers only. Strings are UTF-8; escapes
# are `\uXXXX` (lowercase hex) for U+0000–U+001F plus `\"`, `\\`, `\n`, `\r`,
# `\t`; every other character is verbatim UTF-8. `true`/`false`/`null`
# lowercase.
# ─────────────────────────────────────────────────────────────────────────────

_ESCAPES = {
    0x22: '\\"',
    0x5C: "\\\\",
    0x0A: "\\n",
    0x0D: "\\r",
    0x09: "\\t",
}


def _escape_string(s: str) -> bytes:
    out = []
    for ch in s:
        o = ord(ch)
        if o in _ESCAPES:
            out.append(_ESCAPES[o])
        elif o < 0x20:
            out.append("\\u%04x" % o)
        else:
            out.append(ch)
    return "".join(out).encode("utf-8")


def canonical(value) -> bytes:
    """`to_canonical_string` — the `hh-json/1` serialization, as bytes."""
    if value is None:
        return b"null"
    if value is True:
        return b"true"
    if value is False:
        return b"false"
    if isinstance(value, int):
        return str(value).encode("ascii")
    if isinstance(value, str):
        return b'"' + _escape_string(value) + b'"'
    if isinstance(value, list):
        return b"[" + b",".join(canonical(v) for v in value) + b"]"
    if isinstance(value, dict):
        parts = []
        for k in sorted(value.keys(), key=lambda s: s.encode("utf-8")):
            parts.append(b'"' + _escape_string(k) + b'":' + canonical(value[k]))
        return b"{" + b",".join(parts) + b"}"
    raise TypeError(f"not a canonical-JSON value: {type(value)!r}")


def _no_float(x):
    raise ValueError(f"non-integer number {x!r} is outside hh-json/1")


def _no_constant(x):
    raise ValueError(f"non-JSON literal {x!r} is outside hh-json/1")


def _repair_surrogates(value):
    """Combine UTF-16 surrogate-pair escapes json leaves as lone code units.

    json.loads decodes `\\uD83D\\uDE00` into a two-code-unit Python string;
    the canonical form is the joined code point's verbatim UTF-8. A truly
    unpaired surrogate has no UTF-8 spelling — refuse it (honest failure).
    """
    if isinstance(value, str):
        raw = value.encode("utf-16-le", "surrogatepass")
        return raw.decode("utf-16-le")  # raises on an unpaired surrogate
    if isinstance(value, list):
        return [_repair_surrogates(v) for v in value]
    if isinstance(value, dict):
        return {_repair_surrogates(k): _repair_surrogates(v) for k, v in value.items()}
    return value


def parse_json(data: bytes):
    """Parse a JSON document into the canonical value model."""
    text = data.decode("utf-8")
    return _repair_surrogates(
        json.loads(text, parse_float=_no_float, parse_constant=_no_constant)
    )


# ─────────────────────────────────────────────────────────────────────────────
# Primitive 2 — `idp/1` identity
#
#   idp_id(tag, payload) = "sha256:" + hex(SHA256("idp/1" ‖ 0x1f ‖ tag ‖ 0x1f ‖ payload))
# ─────────────────────────────────────────────────────────────────────────────

def idp_id(tag: str, payload: bytes) -> str:
    h = hashlib.sha256(b"idp/1" + b"\x1f" + tag.encode("utf-8") + b"\x1f" + payload)
    return "sha256:" + h.hexdigest()


def address_blob(data: bytes) -> str:
    """`address(bytes, media_type).id` — media_type is carried, never hashed."""
    return idp_id("blob", data)


def identify_text(data: bytes) -> str:
    """`identify_text` — the text-leaf content id.

    SPEC-TEXT DEVIATION (disclosed in README): CHECKER.md §idp/1 describes
    `idp_id("text", normalized)` with a `\\r\\n`/`\\r` → `\\n` normalization
    step. The committed `identity-records` expectation instead pins
    `idp_id("hir.text", bytes)` over the *verbatim* bytes — the E1 reference
    semantic (the `RecordKind::HirTextLeaf` domain tag; no CRLF rewrite
    exists in the reference leaf path). This implementation follows the
    bundle's evidence: `hir.text`, no normalization. The normalization axis
    is unexercised by the committed corpus (no `\\r` bytes appear in any
    input), so it is recorded as a spec-text defect, not a resolved
    semantic.
    """
    return idp_id("hir.text", data)


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


# ─────────────────────────────────────────────────────────────────────────────
# Primitive 3 — `tree/1` addressing over the `<n>.input.json` manifest shape
#
# The committed `.canonical.json` members show the canonical tree document is
# the *Merkelized* form of the manifest: a file's `bytes_hex` becomes
# `content` (the blob content-address) and a dir's `entries` becomes `tree`
# (the recursive tree id). Tree address = idp_id("tree", canonical(doc)).
# ─────────────────────────────────────────────────────────────────────────────

def _hex_decode(s: str) -> bytes:
    return bytes.fromhex(s)


def tree_canonical_doc(manifest: dict):
    """Manifest → canonical tree document (and its id, computed bottom-up)."""
    if not isinstance(manifest, dict):
        raise ValueError("tree manifest must be an object")
    out = {}
    for name, node in manifest.items():
        kind = node.get("kind") if isinstance(node, dict) else None
        if kind == "file":
            out[name] = {
                "content": address_blob(_hex_decode(node["bytes_hex"])),
                "exec": node.get("exec") is True,
                "kind": "file",
            }
        elif kind == "symlink":
            out[name] = {"kind": "symlink", "target": node["target"]}
        elif kind == "dir":
            _doc, sub_id = tree_canonical_doc(node["entries"])
            out[name] = {"kind": "dir", "tree": sub_id}
        else:
            raise ValueError(f"{name}: unknown entry kind {kind!r}")
    doc_bytes = canonical(out)
    return out, idp_id("tree", doc_bytes)


def arm_identity_trees(arm_dir: Path) -> dict:
    answers = {}
    for p in sorted(arm_dir.glob("*.input.json")):
        case = p.name[: -len(".input.json")]
        manifest = parse_json(p.read_bytes())
        _doc, tree_id = tree_canonical_doc(manifest)
        answers[case] = tree_id
    return answers


# ─────────────────────────────────────────────────────────────────────────────
# Arm: identity-records — blob / text / record forms (CHECKER.md §Arm replay)
# ─────────────────────────────────────────────────────────────────────────────

def arm_identity_records(arm_dir: Path) -> dict:
    answers = {}
    for p in sorted(arm_dir.glob("*.input.json")):
        case = p.name[: -len(".input.json")]
        doc = parse_json(p.read_bytes())
        arm = doc.get("arm")
        if arm == "blob":
            data = _hex_decode(doc["bytes_hex"])
            answers[case] = {"version_id": address_blob(data), "semantic_id": None}
        elif arm == "text":
            data = _hex_decode(doc["bytes_hex"])
            answers[case] = {"version_id": identify_text(data), "semantic_id": None}
        elif arm == "record":
            tag = doc["domain_tag"]
            version_id = idp_id(tag, canonical(doc["canonical_full"]))
            sem = doc.get("canonical_semantic")
            semantic_id = (
                None if sem is None else idp_id(tag + "#semantic", canonical(sem))
            )
            answers[case] = {"version_id": version_id, "semantic_id": semantic_id}
        else:
            raise ValueError(f"{case}: unknown record arm tag {arm!r}")
    return answers


# ─────────────────────────────────────────────────────────────────────────────
# Primitive 4 — ledger WAL replay (`events.wal`, DF-S1.5-3)
#
#   prev = "genesis"; per `{"k":"e","v":env}` frame:
#   hash = idp_id("ledger.event", 0x00 ‖ canonical(env − {"hash"}) ‖ prev_hash)
# ─────────────────────────────────────────────────────────────────────────────

def arm_ledger_transcript(arm_dir: Path) -> dict:
    wal = (arm_dir / "events.wal").read_bytes()
    prev = "genesis"
    event_hashes = []
    for line in wal.split(b"\n"):
        if not line:
            continue
        frame = parse_json(line)
        if canonical(frame) != line:
            raise ValueError("WAL frame is not canonical JSON")
        k = frame.get("k")
        if k == "c":
            continue
        if k != "e":
            raise ValueError(f"unknown WAL frame kind {k!r}")
        env = frame["v"]
        recorded = env["hash"]
        preimage = {m: v for m, v in env.items() if m != "hash"}
        computed = idp_id(
            "ledger.event", b"\x00" + canonical(preimage) + prev.encode("utf-8")
        )
        if computed != recorded:
            raise ValueError(f"recomputed {computed} != recorded {recorded}")
        prev = recorded
        event_hashes.append(recorded)
    return {
        "head_hash": prev,
        "event_hashes": event_hashes,
        "event_count": len(event_hashes),
        "wal_sha256": sha256_hex(wal),
    }


# ─────────────────────────────────────────────────────────────────────────────
# Registry log replay (registry.json → the projections the scenario folds)
#
# Line kinds (append-only log): `record` (envelope+body), `name_history`,
# `supersedes`, `revocation`, `policy`. Rebuilt here: the record table, the
# name-entry sequence, the revocation set and the stale-by-dependency index
# (dependant → [revoked members]) — the contract the snapshot fold consumes.
# ─────────────────────────────────────────────────────────────────────────────

def _referenced_version_ids(kind: str, body) -> list:
    """The pinned `version_id`s a record references (the snapshot-closure
    edge set; names/labels/selectors are never pins)."""
    if not isinstance(body, dict):
        return []
    out = []
    if kind == "class":
        if body.get("conformance_suite_ref"):
            out.append(body["conformance_suite_ref"])
        for dep in body.get("depends_on") or []:
            if isinstance(dep, dict) and dep.get("version_id"):
                out.append(dep["version_id"])
    elif kind == "variant":
        if body.get("class_ref"):
            out.append(body["class_ref"])
    elif kind == "conformance_suite":
        if body.get("class_ref"):
            out.append(body["class_ref"])
    elif kind == "conformance_report":
        for k in ("subject_ref", "suite_ref"):
            if body.get(k):
                out.append(body[k])
    elif kind == "registry_snapshot":
        out.extend(body.get("members") or [])
    elif kind == "extension":
        for r in (body.get("contributes") or []):
            if isinstance(r, dict) and r.get("version_id"):
                out.append(r["version_id"])
        trust = body.get("trust") or {}
        for r in (trust.get("grants") or []):
            if isinstance(r, dict) and r.get("version_id"):
                out.append(r["version_id"])
    elif kind == "environment_record":
        v = (
            body.get("semantic", {})
            .get("containment_policy", {})
            .get("version_id")
        )
        if isinstance(v, str):
            out.append(v)
    return out


class RegistryReplay:
    """The store state the scenario's request vector reads: records by
    version_id, the ordered name entries, the revocation set and the derived
    stale-by-dependency index."""

    def __init__(self, log):
        self.records = {}          # version_id -> {"env":…, "body":…}
        self.name_entries = []     # name_history lines, in order
        self.revoked = []          # revoked version_ids, in order
        self.deps = {}             # dependant -> [member version_ids]
        self.stale = {}            # dependant -> [revoked member ids]
        for line in log:
            ty = line.get("type")
            if ty == "record":
                env = line["envelope"]
                vid = env["version_id"]
                body = line.get("body")
                self.records[vid] = {"env": env, "body": body}
                for member in _referenced_version_ids(env.get("kind"), body):
                    self._declare_dependency(vid, member)
            elif ty == "name_history":
                self.name_entries.append(line)
            elif ty == "supersedes":
                pass  # lineage edge — not read by this scenario's fold
            elif ty == "revocation":
                self._revoke(line.get("revokes"))
            elif ty == "policy":
                pass  # corpus policy is the Stage-1 default (declared floor)

    def _stale_mark(self, dependant: str, member: str):
        lst = self.stale.setdefault(dependant, [])
        if member not in lst:
            lst.append(member)

    def _revoked_causes(self, member: str):
        causes = []
        if member in self.revoked:
            causes.append(member)
        causes.extend(self.stale.get(member, []))
        return causes

    def _declare_dependency(self, dependant: str, member: str):
        self.deps.setdefault(dependant, []).append(member)
        for cause in self._revoked_causes(member):
            self._stale_mark(dependant, cause)

    def _revoke(self, vid: str):
        if vid in self.revoked:
            return
        self.revoked.append(vid)
        causes = [vid] + list(self.stale.get(vid, []))
        # Every transitive dependant of `vid` becomes stale on all causes.
        queue = [vid]
        seen = {vid}
        while queue:
            node = queue.pop()
            for dep, members in self.deps.items():
                if node in members and dep not in seen:
                    seen.add(dep)
                    queue.append(dep)
                    for cause in causes:
                        self._stale_mark(dep, cause)

    def is_revoked(self, vid: str) -> bool:
        return vid in self.revoked

    def stale_for(self, vid: str) -> list:
        return list(self.stale.get(vid, []))

    def first_name_entry(self, vid: str):
        """The first name-history entry binding this version (a pinned id
        resolves with its status attached)."""
        for e in self.name_entries:
            if e.get("version_id") == vid:
                return e
        return None

    def snapshot(self, snap_id: str):
        rec = self.records.get(snap_id)
        if rec is None or rec["env"].get("kind") != "registry_snapshot":
            raise KeyError(f"UnknownSnapshot: {snap_id}")
        body = rec["body"]
        bindings = {}
        for k, v in (body.get("name_bindings") or {}).items():
            ns, _, name = k.partition("/")
            bindings[(ns, name)] = v
        return {"members": set(body.get("members") or []), "name_bindings": bindings}

    def class_catalog(self):
        """Every registered class record → {class_id, contract_version, tier}."""
        out = []
        for vid, rec in self.records.items():
            if rec["env"].get("kind") == "class" and isinstance(rec["body"], dict):
                out.append(
                    {
                        "class_id": rec["body"].get("class_id"),
                        "contract_version": rec["body"].get("contract_version"),
                        "tier": _parse_tier(rec["body"].get("tier")) or 0,
                    }
                )
        return out


def _resolved_json(vid: str, env: dict, store: RegistryReplay, name_status) -> dict:
    admission = "revoked" if store.is_revoked(vid) else env.get("admission")
    return {
        "admission": admission,
        "depends_on_revoked": store.stale_for(vid),
        "name_status": name_status,
        "semantic_id": env.get("semantic_id"),
        "version_id": vid,
    }


# The corpus' output vector — fixed by the arm's replay contract (the
# scenario carries `snapshot_id` + `variant_version_ids`; the selector names
# and the class list are the corpus contract CHECKER.md points at).
_OUTPUT_SELECTORS = [
    ("hh", "control_strategy/control_strategy-v0"),
    ("local", "control_strategy/control_strategy-v1"),
    ("hh", "context_policy/context_policy-v0"),
    ("local", "context_policy/context_policy-v1"),
    ("hh", "export_sink/export_sink-v0"),
    ("local", "export_sink/export_sink-v1"),
]
_OUTPUT_CLASSES = ["control_strategy", "context_policy", "export_sink"]


def _range_covers(rng: str, version: str) -> bool:
    """The one contract-version range grammar: `*`/empty covers all; `v`
    exact; `a-b` inclusive; `>=v`/`<=v` open; dotted-numeric compare."""
    rng = rng.strip()
    if rng == "*" or rng == "":
        return True
    if rng.startswith(">="):
        return _version_cmp(version, rng[2:].strip()) >= 0
    if rng.startswith("<="):
        return _version_cmp(version, rng[2:].strip()) <= 0
    if "-" in rng:
        lo, hi = rng.split("-", 1)
        return _version_cmp(version, lo.strip()) >= 0 and _version_cmp(version, hi.strip()) <= 0
    return _version_cmp(version, rng) == 0


def _version_cmp(a: str, b: str) -> int:
    def parts(s):
        out = []
        for p in s.strip().lstrip("v").split("."):
            out.append(int(p) if p.isdigit() else 0)
        return out

    pa, pb = parts(a), parts(b)
    return (pa > pb) - (pa < pb)


def _valid_version(v: str) -> bool:
    v = v.strip().lstrip("v")
    return bool(v) and all(seg.isdigit() for seg in v.split("."))


def _range_parses(s) -> bool:
    if not isinstance(s, str):
        return False
    s = s.strip()
    if s == "" or s == "*":
        return True
    if s.startswith(">=") or s.startswith("<="):
        return _valid_version(s[2:])
    if "-" in s:
        lo, hi = s.split("-", 1)
        return _valid_version(lo) and _valid_version(hi)
    return _valid_version(s)


def arm_registry_snapshot(arm_dir: Path) -> dict:
    log = parse_json((arm_dir / "registry.json").read_bytes())
    store = RegistryReplay(log)
    scenario = parse_json((arm_dir / "scenario.json").read_bytes())
    snap = store.snapshot(scenario["snapshot_id"])
    members = snap["members"]

    out = []
    # 1. resolve(version_id, audit) for each pinned member.
    for vid in scenario["variant_version_ids"]:
        rec = store.records.get(vid)
        if rec is None:
            out.append({"error": "Unresolved"})
            continue
        out.append(_resolved_json(vid, rec["env"], store, None))

    # 2. resolve(selector, execute) over the snapshot's name bindings.
    for ns, name in _OUTPUT_SELECTORS:
        pinned = snap["name_bindings"].get((ns, name))
        if pinned is None or pinned not in members or pinned not in store.records:
            out.append({"error": "Unresolved"})
            continue
        rec = store.records[pinned]
        env = rec["env"]
        if store.is_revoked(pinned):
            out.append({"error": "Revoked"})
        elif env.get("admission") not in ("resolved", "sealed"):
            out.append({"error": "Unresolved"})
        elif store.stale_for(pinned):
            out.append({"error": "Stale"})
        elif (
            env.get("kind") == "variant"
            and isinstance(rec["body"], dict)
            and not rec["body"].get("capability_declaration")
        ):
            # declared floor: a schema-valid capability_declaration must exist
            out.append({"error": "ConformanceBelowFloor"})
        else:
            out.append(_resolved_json(pinned, env, store, None))

    # 3. query(kind = variant) over snapshot members, sorted by version_id.
    rows = []
    for vid, rec in store.records.items():
        if vid not in members or rec["env"].get("kind") != "variant":
            continue
        entry = store.first_name_entry(vid)
        rows.append(_resolved_json(vid, rec["env"], store, entry and entry.get("status")))
    rows.sort(key=lambda r: r["version_id"])
    out.append(rows)

    # 4. slot_choices(class_id, {contract_version: "1.0"}, snapshot) per class.
    for cid in _OUTPUT_CLASSES:
        # declared floor: a class without a suite is SuiteMissing.
        has_suite = any(
            rec["env"].get("kind") == "class"
            and isinstance(rec["body"], dict)
            and rec["body"].get("class_id") == cid
            and rec["body"].get("conformance_suite_ref")
            for rec in store.records.values()
        )
        if not has_suite:
            out.append({"error": "SuiteMissing"})
            continue
        choices = []
        for vid, rec in store.records.items():
            env, body = rec["env"], rec["body"]
            if vid not in members or env.get("kind") != "variant" or not isinstance(body, dict):
                continue
            cls = store.records.get(body.get("class_ref"))
            belongs = (
                cls is not None
                and cls["env"].get("kind") == "class"
                and isinstance(cls["body"], dict)
                and cls["body"].get("class_id") == cid
            )
            if not belongs:
                continue
            if env.get("admission") not in ("resolved", "sealed"):
                continue
            if store.is_revoked(vid) or store.stale_for(vid):
                continue
            if not _range_covers(body.get("contract_range", ""), "1.0"):
                continue
            if not body.get("capability_declaration"):
                continue
            choices.append({"variant_id": body.get("variant_id"), "version_id": vid})
        choices.sort(key=lambda c: (c["variant_id"], c["version_id"]))
        out.append(choices)

    outputs = canonical(out)
    return {
        "outputs_sha256": sha256_hex(outputs),
        "registry_sha256": sha256_hex((arm_dir / "registry.json").read_bytes()),
    }


# ─────────────────────────────────────────────────────────────────────────────
# Primitive 5 — `PluginManifest/1` decode + admission (DF-S1.27-1)
#
# Posture: first_party=true, EvalPoint stage=1 (no date), requests_cap =
# deny-everything. Verdict: "admit:<namespace>/<name>" or "refuse:<kind>".
# Admission order: decode → namespace → locality → tier → compatibility →
# requests ⊆ cap.
# ─────────────────────────────────────────────────────────────────────────────

_CONTRIBUTION_KINDS = {
    "variant", "capability", "mcp_server", "procedure_profile", "hook",
    "validator", "model_profile", "surface_family", "benchmark_adapter",
    "environment_family", "metric_declaration", "instruction_file",
    "harness_rule",
}
_CONTRACT_KINDS = {"class_contract", "record_kind", "dialect", "protocol_binding"}
_ISOLATION_CLASSES = {
    "in_process", "subprocess_confined", "container", "remote", "host_unconfined",
}
_PLACEMENTS = {
    "in_process", "subprocess_confined", "container", "remote", "component_model",
}
_AUTHORITY_RANK = {
    "unverified": 0, "external": 1, "environment": 2, "delegate": 3,
    "principal": 4, "definition": 5, "kernel": 6,
}
# registry/1's closed record-kind sum — each carries a `record_kind:<id>`
# contract policy with supported ["1"].
_RECORD_KINDS = [
    "class", "variant", "conformance_suite", "conformance_report", "namespace",
    "name_history_entry", "registry_snapshot", "foreign_import",
    "sealed_definition", "capability", "extension", "model_profile",
    "profile_test_report", "wire_dialect", "model_role_table",
    "procedure_profile", "surface_family", "metric_declaration", "validator",
    "environment_record", "environment_family", "trust_root_policy",
    "identity_profile", "participant", "adapter", "leaderboard_definition",
]
# The kernel's own contract table: dialects + the two boundary bindings, each
# supported ["1"], no sunsets (hh-hosting/1 is deliberately absent — AC-12).
_KERNEL_CONTRACTS = {
    ("dialect", "hir/1"): ["1"],
    ("dialect", "idp/1"): ["1"],
    ("dialect", "registry/1"): ["1"],
    ("dialect", "hh-experiment/1"): ["1"],
    ("protocol_binding", "plugin_abi/1"): ["1"],
    ("protocol_binding", "hh-embed/1"): ["1"],
}


class ManifestError(Exception):
    def __init__(self, kind: str, detail: str = ""):
        super().__init__(kind)
        self.kind = kind
        self.detail = detail


def _sv(path: str, detail: str) -> ManifestError:
    return ManifestError("SchemaViolation", f"{path}: {detail}")


def _req(j: dict, k: str, path: str):
    if k not in j:
        raise _sv(path, f"missing member `{k}`")
    return j[k]


def _req_str(j: dict, k: str, path: str) -> str:
    v = _req(j, k, path)
    if not isinstance(v, str):
        raise _sv(path, f"`{k}` is not a string")
    return v


def _opt_str(j: dict, k: str):
    v = j.get(k)
    return v if isinstance(v, str) else None


def _req_arr(j: dict, k: str, path: str) -> list:
    v = _req(j, k, path)
    if not isinstance(v, list):
        raise _sv(path, f"`{k}` is not an array")
    return v


def _str_vec(j: dict, k: str, path: str) -> list:
    out = []
    for i, v in enumerate(_req_arr(j, k, path)):
        if not isinstance(v, str):
            raise _sv(path, f"`{k}[{i}]` is not a string")
        out.append(v)
    return out


def _contract_ref(j, path: str) -> dict:
    if not isinstance(j, dict):
        raise _sv(path, "expected object")
    for k in j:
        if k not in ("kind", "id", "version_range", "ext"):
            raise _sv(path, f"unknown member `{k}`")
    kind = j.get("kind")
    if not isinstance(kind, str):
        raise _sv(path, "`kind` missing or not a string")
    if kind not in _CONTRACT_KINDS:
        raise _sv(path, f"`kind` `{kind}` is not in the closed ContractRef kind sum")
    cid = j.get("id")
    if not isinstance(cid, str):
        raise _sv(path, "`id` missing or not a string")
    if cid == "":
        raise _sv(path, "`id` is empty")
    vr = j.get("version_range")
    if not isinstance(vr, str):
        raise _sv(path, "`version_range` missing or not a string")
    if not _range_parses(vr):
        raise _sv(path, f"`version_range` `{vr}` is malformed")
    return {"kind": kind, "id": cid, "version_range": vr}


def _parse_tier(t) -> int | None:
    if not isinstance(t, str) or not t.startswith("C"):
        return None
    try:
        n = int(t[1:])
    except ValueError:
        return None
    return n if 0 <= n <= 4 else None


def _check_package_path(p: str, path: str):
    outside = (
        p == ""
        or p.startswith("/")
        or p.startswith("\\")
        or "\x00" in p
        or ":" in p
        or any(seg == ".." for seg in p.replace("\\", "/").split("/"))
    )
    if outside:
        raise _sv(path, f"path `{p}` escapes the package root (a manifest names package members only)")
    return ("path", p)


def _locator_pinned(loc: dict, path: str):
    for k in loc:
        if k not in ("scheme", "credential_free_uri", "selector", "resolved", "fetched_at", "ext"):
            raise _sv(path, f"unknown member `{k}`")
    scheme = _req_str(loc, "scheme", path)
    uri = _req_str(loc, "credential_free_uri", path)
    fetched = loc.get("fetched_at")
    if fetched is not None and not (isinstance(fetched, int) and fetched >= 0):
        raise _sv(path, "`fetched_at` is not a u64")
    # credential-free check: no userinfo, no credential-like query keys
    authority = uri.split("://", 1)[1] if "://" in uri else uri.split(":", 1)[0]
    if "@" in authority.split("/", 1)[0]:
        raise _sv(path + ".credential_free_uri", "credential userinfo in locator URI")
    if "?" in uri:
        for kv in uri.split("?", 1)[1].split("&"):
            k = kv.split("=", 1)[0].lower()
            if k in ("password", "passwd", "token", "secret", "key"):
                raise _sv(path + ".credential_free_uri", f"credential-like query member `{k}` in locator URI")
    if loc.get("selector") is not None or loc.get("resolved") is None or fetched is None:
        raise ManifestError(
            "UnpinnedInSealedForm",
            f"{path}: selector survived or resolved/fetched_at missing",
        )
    return ("locator", uri)


def _contribution(j, path: str) -> dict:
    if not isinstance(j, dict):
        raise _sv(path, "expected object")
    for k in j:
        if k not in ("kind", "path_or_locator", "declaration", "executable", "contract_range", "ext"):
            raise _sv(path, f"unknown member `{k}`")
    kind = _req_str(j, "kind", path)
    if kind not in _CONTRIBUTION_KINDS:
        raise ManifestError("UnknownRecordKind", kind)

    pol = _req(j, "path_or_locator", path)
    lp = f"{path}.path_or_locator"
    if isinstance(pol, str):
        path_or_locator = _check_package_path(pol, lp)
    elif isinstance(pol, dict):
        path_or_locator = _locator_pinned(pol, lp)
    else:
        raise _sv(path, "`path_or_locator` is a package path string or a pinned locator object")

    dp = f"{path}.declaration"
    decl = _req(j, "declaration", path)
    if not isinstance(decl, dict):
        raise _sv(dp, "expected object")
    for k in decl:
        if k in ("body", "content", "closure", "code"):
            raise _sv(dp, f"member `{k}` is an inline body — declarations name records, they never carry them")

    executable = None
    ej = j.get("executable")
    if ej is not None:
        ep = f"{path}.executable"
        if not isinstance(ej, dict):
            raise _sv(ep, "expected object")
        for k in ej:
            if k not in ("code_pointer", "isolation", "placement_preference", "one_shot", "host_requirements", "ext"):
                raise _sv(ep, f"unknown member `{k}`")
        cp = f"{ep}.code_pointer"
        cm = ej.get("code_pointer")
        if not isinstance(cm, dict):
            raise ManifestError(
                "UnpinnedInSealedForm",
                f"{cp} is not a pinned ContentAddress object — a selector or bare path is a claim, never a pin",
            )
        for k in cm:
            if k not in ("algorithm", "digest", "idp", "media_type", "size", "ext"):
                raise _sv(cp, f"unknown member `{k}`")
        algorithm = cm.get("algorithm")
        if not isinstance(algorithm, str):
            raise ManifestError("UnpinnedInSealedForm", f"{cp}: missing `algorithm`")
        if algorithm != "sha256":
            raise ManifestError("UnpinnedInSealedForm", f"{cp}: algorithm `{algorithm}` is not `sha256`")
        digest = cm.get("digest")
        if not isinstance(digest, str) or digest == "":
            raise ManifestError("UnpinnedInSealedForm", f"{cp}: missing/empty `digest` — not a pin")
        size = cm.get("size")
        if not (isinstance(size, int) and not isinstance(size, bool) and size >= 0):
            raise ManifestError("UnpinnedInSealedForm", f"{cp}: missing `size` — not a pin")
        isolation = _req_str(ej, "isolation", ep)
        if isolation not in _ISOLATION_CLASSES:
            raise _sv(ep, "`isolation` is outside the closed IsolationClass sum")
        placement = _opt_str(ej, "placement_preference")
        if placement is not None and placement not in _PLACEMENTS:
            raise _sv(ep, f"`placement_preference` `{placement}` is outside the closed sum")
        one_shot = ej.get("one_shot")
        if one_shot is not None and not isinstance(one_shot, bool):
            raise _sv(ep, "`one_shot` is not a bool")
        executable = {
            "isolation": isolation,
            "placement_preference": placement,
        }

    return {
        "kind": kind,
        "path_or_locator": path_or_locator,
        "declaration": decl,
        "executable": executable,
        "contract_range": _opt_str(j, "contract_range"),
    }


def manifest_from_bytes(data: bytes) -> dict:
    try:
        j = parse_json(data)
    except Exception:
        raise ManifestError("NotJson")
    if not isinstance(j, dict):
        raise ManifestError("NotJson")
    path = "manifest"
    known = {
        "identity", "tier", "depends_on", "requires", "contributions",
        "requests", "claims", "conformance_claims", "parameters", "summary", "ext",
    }
    for k in j:
        if k not in known:
            raise _sv(path, f"unknown non-`ext` member `{k}`")

    ip = f"{path}.identity"
    ij = _req(j, "identity", path)
    if not isinstance(ij, dict):
        raise _sv(ip, "expected object")
    for k in ij:
        if k not in ("namespace", "name", "version_label", "ext"):
            raise _sv(ip, f"unknown member `{k}`")
    identity = {
        "namespace": _req_str(ij, "namespace", ip),
        "name": _req_str(ij, "name", ip),
        "version_label": _opt_str(ij, "version_label"),
    }

    tier = _req_str(j, "tier", path)
    if _parse_tier(tier) is None:
        raise _sv(path, f"`tier` `{tier}` is not a C0..C4 spelling")

    depends_on = [
        _contract_ref(v, f"{path}.depends_on[{i}]")
        for i, v in enumerate(_req_arr(j, "depends_on", path))
    ]

    rp = f"{path}.requires"
    rj = _req(j, "requires", path)
    if not isinstance(rj, dict):
        raise _sv(rp, "expected object")
    for k in rj:
        if k not in ("hir_dialect", "registry_dialect", "plugin_abi", "contracts", "records", "ext"):
            raise _sv(rp, f"unknown member `{k}`")
    contracts = [
        _contract_ref(v, f"{rp}.contracts[{i}]")
        for i, v in enumerate(_req_arr(rj, "contracts", rp))
    ]
    records = []
    for i, v in enumerate(_req_arr(rj, "records", rp)):
        p = f"{rp}.records[{i}]"
        if not isinstance(v, dict):
            raise _sv(p, "expected pinned record ref object")
        for k in v:
            if k not in ("version_id", "kind", "ext"):
                raise _sv(p, f"unknown member `{k}` (a record ref is a pin, not a selector)")
        version_id = _req_str(v, "version_id", p)
        if version_id == "" or ":" not in version_id:
            raise ManifestError("UnpinnedInSealedForm", f"{p}.version_id `{version_id}` is not a pinned id")
        records.append({"version_id": version_id, "kind": _opt_str(v, "kind")})
    requires = {
        "hir_dialect": _req_str(rj, "hir_dialect", rp),
        "registry_dialect": _req_str(rj, "registry_dialect", rp),
        "plugin_abi": _req_str(rj, "plugin_abi", rp),
        "contracts": contracts,
        "records": records,
    }

    contributions = [
        _contribution(v, f"{path}.contributions[{i}]")
        for i, v in enumerate(_req_arr(j, "contributions", path))
    ]

    rp = f"{path}.requests"
    requests = {"effects": [], "fs_roots": [], "egress": [], "env_keys": [],
                "budget_share": None, "hot_path_operations": []}
    rj = j.get("requests")
    if rj is not None:
        if not isinstance(rj, dict):
            raise _sv(rp, "expected object")
        for k in rj:
            if k not in ("effects", "fs_roots", "egress", "env_keys", "budget_share", "hot_path_operations", "ext"):
                raise _sv(rp, f"unknown member `{k}`")
        effects = []
        for i, v in enumerate(_req_arr(rj, "effects", rp)):
            p = f"{rp}.effects[{i}]"
            if not isinstance(v, dict):
                raise _sv(p, "expected object")
            for k in v:
                if k not in ("domain", "scope", "ext"):
                    raise _sv(p, f"unknown member `{k}`")
            effects.append({"domain": _req_str(v, "domain", p), "scope": _req_str(v, "scope", p)})
        requests = {
            "effects": effects,
            "fs_roots": _str_vec(rj, "fs_roots", rp),
            "egress": _req_arr(rj, "egress", rp),
            "env_keys": _str_vec(rj, "env_keys", rp),
            "budget_share": rj.get("budget_share") if "budget_share" in rj else None,
            "hot_path_operations": _str_vec(rj, "hot_path_operations", rp),
        }

    claims = _req(j, "claims", path)
    if not isinstance(claims, dict):
        raise _sv(path, "`claims` is not an object")

    for i, v in enumerate(_req_arr(j, "conformance_claims", path)):
        p = f"{path}.conformance_claims[{i}]"
        produced_by = v.get("produced_by") if isinstance(v, dict) else None
        if produced_by is None:
            raise _sv(p, "missing `produced_by`")
        if produced_by != "publisher_claim":
            raise _sv(p, f"produced_by `{produced_by}` — manifest claims are `publisher_claim` only")

    parameters = j.get("parameters")
    if parameters is not None and not isinstance(parameters, dict):
        raise _sv(path, "`parameters` is not an object")

    sp = f"{path}.summary"
    summary = _req(j, "summary", path)
    if not isinstance(summary, dict):
        raise _sv(sp, "not a Text leaf: expected object")
    # The Text-leaf shape: content_hash, owner, authority, provenance.
    for member in ("content_hash", "owner", "authority"):
        if not isinstance(summary.get(member), str):
            raise _sv(sp, f"not a Text leaf: `{member}` missing or not a string")
    if summary["authority"] not in _AUTHORITY_RANK:
        raise _sv(sp, f"not a Text leaf: bad authority `{summary['authority']}`")
    if "provenance" not in summary or not isinstance(summary["provenance"], dict):
        raise _sv(sp, "not a Text leaf: missing provenance")
    if _AUTHORITY_RANK[summary["authority"]] > _AUTHORITY_RANK["external"]:
        raise _sv(sp, f"authority {summary['authority']} exceeds `external` (a manifest never mints above external)")

    ext = j.get("ext")
    if ext is not None and not isinstance(ext, dict):
        raise _sv(path, "`ext` is not an object")

    return {
        "identity": identity,
        "tier": tier,
        "depends_on": depends_on,
        "requires": requires,
        "contributions": contributions,
        "requests": requests,
    }


def _contract_defining_tier(ref: dict, catalog: list):
    kind, cid = ref["kind"], ref["id"]
    if kind in ("dialect", "record_kind"):
        return 0
    if kind == "protocol_binding":
        return 2 if cid == "hh-hosting/1" else 0
    if kind == "class_contract":
        for c in catalog:
            if c["class_id"] == cid:
                return c["tier"]
        return None
    return None


def _check_compatibility(refs: list, catalog: list) -> bool:
    """Every ref must intersect a supported version (no past sunsets in the
    Stage-1 tables — sunset members are absent from every policy in play).
    True on success; False collects into ContractIncompatible."""
    supported_table = dict(_KERNEL_CONTRACTS)
    for kind in _RECORD_KINDS:
        supported_table[("record_kind", kind)] = ["1"]
    failures = 0
    for r in refs:
        if r["kind"] == "class_contract":
            supported = [
                c["contract_version"] for c in catalog if c["class_id"] == r["id"]
            ]
        else:
            supported = supported_table.get((r["kind"], r["id"]), [])
        covered = [v for v in supported if _range_covers(r["version_range"], v)]
        if not covered:
            failures += 1
    return failures == 0


def admit_plugin(manifest: dict, catalog: list, first_party_ctx: bool = True) -> str:
    """The §8.4 §2 admission gate → 'admit:<id>' or raises ManifestError."""
    ns = manifest["identity"]["namespace"]
    if not (
        ns in ("hh", "local")
        or ns.startswith("hh/")
        or ns.startswith("local/")
    ):
        raise ManifestError("NamespaceForbidden", ns)
    first_party = first_party_ctx and (ns == "hh" or ns.startswith("hh/"))
    plugin_id = f"{ns}/{manifest['identity']['name']}"

    # locality — executable contributions only
    for c in manifest["contributions"]:
        exe = c["executable"]
        if exe is None:
            continue
        if exe["isolation"] == "host_unconfined":
            raise ManifestError("LocalityInadmissible", f"{plugin_id} at host_unconfined")
        if exe["placement_preference"] == "in_process" and not first_party:
            raise ManifestError("LocalityInadmissible", f"{plugin_id} at in_process")
        if exe["placement_preference"] == "component_model":
            raise ManifestError("LocalityInadmissible", f"{plugin_id} at component_model")

    # tier — every depends_on contract's defining tier ≤ the manifest's
    tier = _parse_tier(manifest["tier"])
    if tier is None:
        raise _sv("tier", f"`{manifest['tier']}` is not a tier spelling (C0..C4)")
    for r in manifest["depends_on"]:
        dt = _contract_defining_tier(r, catalog)
        if dt is None:
            raise ManifestError("ContractIncompatible", f"{r['kind']}:{r['id']} names no contract")
        if dt > tier:
            raise ManifestError("TierViolation", f"{r['kind']}:{r['id']} defined at C{dt} > manifest tier C{tier}")

    # compatibility — requires ∪ contribution contract_ranges
    refs = [
        {"kind": "dialect", "id": "hir/1", "version_range": manifest["requires"]["hir_dialect"]},
        {"kind": "dialect", "id": "registry/1", "version_range": manifest["requires"]["registry_dialect"]},
        {"kind": "protocol_binding", "id": "plugin_abi/1", "version_range": manifest["requires"]["plugin_abi"]},
    ]
    refs.extend(manifest["requires"]["contracts"])
    for c in manifest["contributions"]:
        if c["contract_range"] is not None:
            if c["kind"] == "variant":
                class_id = c["declaration"].get("class_id")
                if not isinstance(class_id, str):
                    raise _sv("contributions.declaration", "a variant contribution with contract_range must name declaration.class_id")
                refs.append({"kind": "class_contract", "id": class_id, "version_range": c["contract_range"]})
            else:
                refs.append({"kind": "record_kind", "id": c["kind"], "version_range": c["contract_range"]})
    if not _check_compatibility(refs, catalog):
        raise ManifestError("ContractIncompatible")

    # requests ⊆ requests_cap (the bundle's posture: deny-everything cap)
    req = manifest["requests"]
    if (
        req["effects"]
        or req["fs_roots"]
        or req["egress"]
        or req["env_keys"]
        or req["budget_share"] is not None
        or req["hot_path_operations"]
    ):
        raise ManifestError("RequestsExceedCap", "requests exceed the deny-everything cap")

    return f"admit:{plugin_id}"


def arm_plugin_manifests(arm_dir: Path, bundle_dir: Path) -> dict:
    # The arm's store is the deterministic registry corpus — the same log the
    # registry-snapshot arm ships. The class catalog (id → contract_version +
    # tier) is the only store state admission reads.
    corpus_log = parse_json(
        (bundle_dir / "arms/registry-snapshot/registry.json").read_bytes()
    )
    catalog = RegistryReplay(corpus_log).class_catalog()

    answers = {}
    for rel in sorted(
        str(p.relative_to(arm_dir))
        for p in arm_dir.rglob("*.json")
        if p.name != "expected.json"
    ):
        try:
            manifest = manifest_from_bytes((arm_dir / rel).read_bytes())
            answers[rel] = admit_plugin(manifest, catalog)
        except ManifestError as e:
            answers[rel] = f"refuse:{e.kind}"
    return answers


# ─────────────────────────────────────────────────────────────────────────────
# Arm: canonical-parse — parse + re-emit byte-identically; answer the digest.
# ─────────────────────────────────────────────────────────────────────────────

def arm_canonical_parse(arm_dir: Path) -> dict:
    answers = {}
    frames = arm_dir / "frames"
    for p in sorted(frames.glob("*.json")):
        case = p.name[: -len(".json")]
        data = p.read_bytes()
        t0 = time.perf_counter_ns()
        doc = parse_json(data)
        t1 = time.perf_counter_ns()
        reemitted = canonical(doc)
        t2 = time.perf_counter_ns()
        entry = {
            "sha256": sha256_hex(data),
            "parse_ns": t1 - t0,
            "rehash_ns": t2 - t1,
        }
        # Self-canonical honesty check (not part of the compared answer):
        if reemitted != data:
            entry["self_canonical"] = False
        answers[case] = entry
    return answers


# ─────────────────────────────────────────────────────────────────────────────
# Driver
# ─────────────────────────────────────────────────────────────────────────────

def main() -> int:
    if len(sys.argv) != 3:
        print("usage: python3 foreign.py <bundle-dir> <answers.json>", file=sys.stderr)
        return 2
    bundle = Path(sys.argv[1])
    arms_dir = bundle / "arms"
    answers = {
        "candidate": "E2-candidate",
        "arms": {
            "identity-trees": arm_identity_trees(arms_dir / "identity-trees"),
            "identity-records": arm_identity_records(arms_dir / "identity-records"),
            "ledger-transcript": arm_ledger_transcript(arms_dir / "ledger-transcript"),
            "registry-snapshot": arm_registry_snapshot(arms_dir / "registry-snapshot"),
            "plugin-manifests": arm_plugin_manifests(arms_dir / "plugin-manifests", bundle),
            "canonical-parse": arm_canonical_parse(arms_dir / "canonical-parse"),
        },
    }
    out = Path(sys.argv[2])
    out.write_bytes(canonical(answers) + b"\n")
    print(f"wrote {out} (candidate E2-candidate, {sum(1 for a in answers['arms'].values() for _ in ([a] if not isinstance(a, dict) else a))} arm answers)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
