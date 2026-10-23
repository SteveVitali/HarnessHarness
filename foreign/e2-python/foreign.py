#!/usr/bin/env python3
"""E2-candidate — a foreign (Python 3, stdlib-only) implementation of the
`hh-xcheck-bundle/1` replay contract (CHECKER.md, spec §10.7, R2.21).

Reads a cross-impl bundle directory, replays all eight arms, and writes an
`answers.json` that `hh-xcheck verify <bundle> <answers>` compares against
the bundle's committed E1 expectations (the R4.3 extension arms —
`hir-documents` + `ledger-scenarios` — landed at R4.16a, matching the e3
reference semantics in `foreign/e3-typescript`).

    python3 foreign.py <bundle-dir> <answers.json>

Honesty note: this is a *candidate* implementation. Its answers are evidence
the foreign run produced; a green `verify` does not mint E1 truth. The
primitives here are implemented from CHECKER.md's spec text plus the
committed fixture shapes — no Rust code was transliterated.

Stdlib only: copy, functools, hashlib, json, pathlib, sys, time.
"""

import copy
import functools
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

def _replay_wal(wal: bytes):
    """The shared `events.wal` fold — one spelling for every ledger arm
    (R4.16a's `ledger-scenarios` reuses it per case dir; the e3 reference is
    `lib/ledger.ts`'s `replayWal`). Returns `(head_hash, event_hashes)`; the
    callers add the informational `wal_sha256`/`event_count` members."""
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
    return prev, event_hashes


def arm_ledger_transcript(arm_dir: Path) -> dict:
    wal = (arm_dir / "events.wal").read_bytes()
    head, event_hashes = _replay_wal(wal)
    return {
        "head_hash": head,
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
# Arm: ledger-scenarios — additional WAL scenarios (the R4.3 extension arm,
# DF-S1.5-3; R4.16a). Every `<case>/` directory's `events.wal` replays through
# the same `_replay_wal` fold (CC1 — one ledger spelling; the e3 reference is
# `lib/ledger.ts`'s `armLedgerScenarios` over the shared `replayWal`). The
# per-case answer is `{event_hashes, head_hash}`; `<case>.wal_sha256` and
# `<case>.event_count` are the informational members the checker never gates.
# ─────────────────────────────────────────────────────────────────────────────

def arm_ledger_scenarios(arm_dir: Path) -> dict:
    answers = {}
    for case_dir in sorted(p for p in arm_dir.iterdir() if p.is_dir()):
        wal = (case_dir / "events.wal").read_bytes()
        head, event_hashes = _replay_wal(wal)
        case = case_dir.name
        answers[case] = {"event_hashes": event_hashes, "head_hash": head}
        answers[f"{case}.event_count"] = len(event_hashes)
        answers[f"{case}.wal_sha256"] = sha256_hex(wal)
    return answers


# ─────────────────────────────────────────────────────────────────────────────
# Arm: hir-documents — the §3.1.7 `parse → canonicalize → validate → seal`
# pipeline (the R4.3 extension arm, DF-S1.2-2; R4.16a).
#
# A corpus-faithful port matching the e3 reference semantics
# (`foreign/e3-typescript/lib/hir.ts` — itself the foreign spelling of the
# E1 `hir_replay` pipeline), on this file's own canonical-JSON/`idp_id`/sha256
# primitives (CC1 — no second spelling). Scope, as in e3: the corpus's five
# exercised node kinds — AgentProcess (native + hosted bodies), Budget,
# HarnessRule, Permission, ToolCapability — a root reference, and no edges.
# The port decodes and re-emits every member those kinds carry, ports the
# provenance codec end-to-end, and reproduces the validate and seal machinery
# the corpus reaches (plus the shared generic checks).
#
# Honest bound (e3's, kept): a case is *omitted* — the bundle's `missing
# answer` signal, never a fabricated verdict — when it reaches surface the
# port does not carry: the eight unexercised registered kinds, any edge,
# `assumption_debt`, or a `flow_contract` member. Registered-but-unexercised
# spellings still parse through the real vocabulary (`UnknownKind`,
# `MissingProvenance`, `SchemaViolation`, …).
# ─────────────────────────────────────────────────────────────────────────────

# The error channel: `_HirFail` carries the closed §3.1.4 `HirError::kind_str()`
# spelling the `refuse:`/`invalid:` answers join on; collected validation
# findings are `(kind, detail)` tuples — the join reads kinds only.


class _HirFail(Exception):
    def __init__(self, kind: str, detail: str = ""):
        super().__init__(f"{kind}: {detail}")
        self.kind = kind
        self.detail = detail


class _HirUnsupported(Exception):
    """The case needs machinery the port does not carry — the arm omits the
    answer (honest pending), never a fabricated verdict."""


def _hfail(kind: str, detail: str = ""):
    raise _HirFail(kind, detail)


def _hsv(p: str, detail: str):
    _hfail("SchemaViolation", p if detail == "" else f"{p}{detail}")


_ABSENT = object()  # `undefined` — an absent member is not a present null


def _hget(j, k):
    return j.get(k, _ABSENT) if isinstance(j, dict) else _ABSENT


def _hhas(j, k) -> bool:
    return isinstance(j, dict) and k in j


def _hreq(j, k, p):
    v = _hget(j, k)
    if v is _ABSENT:
        _hsv(p, f".{k} missing")
    return v


def _hreq_str(j, k, p) -> str:
    v = _hreq(j, k, p)
    if not isinstance(v, str):
        _hsv(p, f".{k} must be a string")
    return v


def _hreq_int(j, k, p) -> int:
    v = _hreq(j, k, p)
    if not isinstance(v, int) or isinstance(v, bool) or v < 0:
        _hsv(p, f".{k} must be a non-negative integer")
    return v


def _hopt_str(j, k):
    v = _hget(j, k)
    return v if isinstance(v, str) else None


def _hopt_or(j, k, dflt):
    v = _hget(j, k)
    return dflt if v is _ABSENT or v is None else v


def _harr(j, k, p) -> list:
    v = _hreq(j, k, p)
    if not isinstance(v, list):
        _hsv(p, f".{k} must be an array")
    return v


def _hstr_arr(items, p) -> list:
    out = []
    for i, s in enumerate(items):
        if not isinstance(s, str):
            _hsv(p, f"[{i}] must be a string")
        out.append(s)
    return out


def _hbool_or(j, k, p, dflt) -> bool:
    """`bool_at(...).unwrap_or(default)` — absent reads the default; a
    present non-bool is a schema violation."""
    v = _hget(j, k)
    if v is _ABSENT:
        return dflt
    if not isinstance(v, bool):
        _hsv(p, f".{k} must be a bool")
    return v


def _htag(j, p) -> str:
    v = _hget(j, "kind")
    if not isinstance(v, str):
        _hsv(p, ".kind missing")
    return v


# The closed registries the corpus reaches (the canonical spellings; `parse`
# failures answer `UnknownKind`, never a fabricated member).

_ENTITY_KINDS = {
    "Goal", "Observation", "ContextItem", "Memory", "Procedure",
    "ToolCapability", "Permission", "Effect", "Artifact", "Validator",
    "AgentProcess", "Budget", "HarnessRule",
}
_EDGE_KINDS = {
    "depends-on", "supersedes", "authorizes", "produced-by", "validates",
    "delegated-to", "derived-from",
}
_HAS_SURFACE = {"ToolCapability", "Validator", "Procedure", "ContextItem"}
_DECISION_POINTS = {
    "plan", "act", "retrieve", "compact", "verify", "delegate", "authorize",
    "retry", "stop", "escalate",
}
_OWNERS = {"code", "model", "human"}
_OBSERVABILITY = {"events", "model_io", "end_state", "ledger"}
_CAPABILITY_STATES = {"supported", "unsupported", "unknown"}
_HOSTING = {"session_abi", "model_boundary_intercept", "container_installed"}
_LOCALITIES = {"in_process", "out_of_process", "inherit"}
_PRECONDITION_DOMAINS = {
    "filesystem", "network", "credentials", "environment", "temporal",
    "capability",
}
_AUTHORITY_NAME = [
    "unverified", "external", "environment", "delegate", "principal",
    "definition", "kernel",
]
_AUTHORITY = {n: i for i, n in enumerate(_AUTHORITY_NAME)}
_SCOPES = {"definition", "user", "project", "session", "run", "turn"}
_HUMAN_ROLES = {"author", "principal", "reviewer"}
_ATTESTATION_KINDS = {"seal", "hash_chain", "signature", "pin"}
_DERIVATION_KINDS = {
    "summary", "compaction", "extraction", "quotation", "revision",
    "translation", "subagent_result", "projection",
}
# `EffectDomain` — the enum *rank* matters: `declared` sets are a
# `BTreeSet<EffectClass>` ordered by (domain rank, attributes).
_EFFECT_DOMAINS = [
    "fs_read", "fs_write", "exec", "net_egress", "secret_access", "spend",
    "message_human", "spawn_process", "memory_write", "permission_request",
    "model_call",
]
_MUTABILITIES = {"read_only", "additive", "destructive"}
_REPEAT_SAFETIES = {"idempotent", "non_idempotent"}
_WORLDS = {"closed", "open"}
_REVERSIBILITIES = {"compensable", "irreversible"}
# `DimensionKey::parse` — `DimensionId` ∪ registered `ext` names ∪ derived
# dimensions (hharness-ontology `dimensions.rs`).
_DIMENSION_KEYS = {
    "tokens.input.uncached", "tokens.input.cache_read",
    "tokens.input.cache_write", "tokens.output.visible",
    "tokens.output.reasoning", "model_calls", "tool_calls", "turns",
    "retries", "spawns", "approvals.requested", "approvals.granted",
    "evaluator_calls", "network.calls", "network.bytes_out",
    "network.bytes_in", "time.wall_ms", "time.working_ms",
    "time.model_latency_ms", "time.human_wait_ms", "message_human",
    "env.active_ms", "env.reserved_ms", "env.suspended_ms", "spend",
    "hh.egress.decisions.allowed", "hh.egress.decisions.denied",
    "hh.egress.decisions.asked", "hh.containment.violations",
    "ext.effects.external_irreversible", "hh.env.cpu_ms",
    "hh.env.memory_bytes", "hh.env.disk_bytes", "hh.env.pids",
    "hh.env.open_files", "context.occupancy", "fan_out", "delegation_depth",
    "reconciliation.holds", "tokens.input.total", "tokens.output.total",
    "tokens.blended",
}
_SCOPE_KINDS = {
    "fs_path", "host", "recipient", "spend_amount", "secret_ref",
    "process_target", "memory_scope", "resource_key",
}
_EXPOSURE_MODES = {"direct", "indexed", "deferred", "code_mode", "hidden"}


# Provenance — the corpus-reached codec, byte-faithful both directions.

def _authority_parse(s, kind):
    a = _AUTHORITY.get(s)
    if a is None:
        _hfail("UnknownKind", kind)
    return a


def _taint_parse(s):
    if s.startswith(("tool:", "participant:", "import:", "extension:")):
        return s
    _hfail("UnknownKind", f"taint tag {s}")


def _sorted_set(items):
    """`BTreeSet` semantics on decode: dedupe + byte sort (array order is
    semantic — canonical JSON does not re-sort arrays)."""
    return sorted(set(items), key=lambda s: s.encode("utf-8"))


def _origin_from_json(j, p):
    kind = _htag(j, p)

    def s(k):
        return _hreq_str(j, k, p)

    if kind == "kernel":
        return {"kind": kind, "component_ref": s("component_ref")}
    if kind == "human":
        role = s("role")
        if role not in _HUMAN_ROLES:
            _hfail("UnknownKind", f"human role {role}")
        return {"kind": kind, "author_ref": s("author_ref"), "role": role}
    if kind == "model":
        return {
            "kind": kind,
            "model_ref": s("model_ref"),
            "run_ref": s("run_ref"),
            "response_id": s("response_id"),
        }
    if kind == "tool":
        inner = _hopt_str(j, "inner_source")
        if inner is not None:
            _taint_parse(inner)
        o = {
            "kind": kind,
            "capability": s("capability"),
            "invocation_ref": s("invocation_ref"),
        }
        if inner is not None:
            o["inner_source"] = inner
        return o
    if kind == "evolution":
        return {
            "kind": kind,
            "candidate_id": s("candidate_id"),
            "hypothesis_ref": s("hypothesis_ref"),
        }
    if kind == "import":
        return {
            "kind": kind,
            "source_system": s("source_system"),
            "mapping_version": s("mapping_version"),
        }
    if kind == "migration":
        return {"kind": kind, "from_dialect": s("from_dialect")}
    if kind == "participant":
        return {
            "kind": kind,
            "participant_ref": s("participant_ref"),
            "hosting_mechanism": s("hosting_mechanism"),
        }
    _hfail("UnknownKind", f"origin {kind}")


def _origin_json(o):
    k = o["kind"]
    obj = {"kind": k}
    if k == "kernel":
        obj["component_ref"] = o["component_ref"]
    elif k == "human":
        obj["author_ref"] = o["author_ref"]
        obj["role"] = o["role"]
    elif k == "model":
        obj["model_ref"] = o["model_ref"]
        obj["run_ref"] = o["run_ref"]
        obj["response_id"] = o["response_id"]
    elif k == "tool":
        obj["capability"] = o["capability"]
        obj["invocation_ref"] = o["invocation_ref"]
        if "inner_source" in o:
            obj["inner_source"] = o["inner_source"]
    elif k == "evolution":
        obj["candidate_id"] = o["candidate_id"]
        obj["hypothesis_ref"] = o["hypothesis_ref"]
    elif k == "import":
        obj["source_system"] = o["source_system"]
        obj["mapping_version"] = o["mapping_version"]
    elif k == "migration":
        obj["from_dialect"] = o["from_dialect"]
    elif k == "participant":
        obj["participant_ref"] = o["participant_ref"]
        obj["hosting_mechanism"] = o["hosting_mechanism"]
    return obj


def _attestation_from_json(j, p):
    kind = _hreq_str(j, "kind", p)
    if kind not in _ATTESTATION_KINDS:
        _hfail("UnknownKind", f"attestation kind {kind}")
    aj = _hreq(j, "anchor", p)
    signer = _hget(aj, "signer")
    chain = _hget(aj, "chain")
    if isinstance(signer, str):
        anchor = {"signer": signer}
    elif isinstance(chain, str):
        anchor = {"chain": chain}
    else:
        _hsv(p, ".anchor must be signer|chain")
    return {
        "kind": kind,
        "subject_hash": _hreq_str(j, "subject_hash", p),
        "anchor": anchor,
        "verified_by": _hreq_str(j, "verified_by", p),
        "verified_at": _hreq_int(j, "verified_at", p),
    }


def _attestation_json(a):
    return {
        "kind": a["kind"],
        "subject_hash": a["subject_hash"],
        "anchor": (
            {"signer": a["anchor"]["signer"]}
            if "signer" in a["anchor"]
            else {"chain": a["anchor"]["chain"]}
        ),
        "verified_by": a["verified_by"],
        "verified_at": a["verified_at"],
    }


def _att_self_consistent(a) -> bool:
    signer = a["anchor"].get("signer")
    anchor = signer if signer is not None else a["anchor"].get("chain", "")
    return a["subject_hash"] != "" and a["verified_by"] != "" and anchor != ""


def _provenance_from_json(j, p):
    origin = _origin_from_json(_hreq(j, "origin", p), f"{p}.origin")
    authority = _authority_parse(_hreq_str(j, "authority", p), f"{p}.authority")
    scope = _hreq_str(j, "scope", p)
    if scope not in _SCOPES:
        _hfail("UnknownKind", f"scope {scope}")
    created_at = _hreq_int(j, "created_at", p)
    taint = []
    tj = _hget(j, "taint")
    if tj is not _ABSENT:
        if not isinstance(tj, list):
            _hsv(p, ".taint must be an array")
        parsed = []
        for t in tj:
            if not isinstance(t, str):
                _hsv(p, ".taint members must be strings")
            parsed.append(_taint_parse(t))
        taint = _sorted_set(parsed)
    readers = None
    rj = _hget(j, "readers")
    if rj is not _ABSENT:
        if not isinstance(rj, list):
            _hsv(p, ".readers must be an array")
        readers = _sorted_set(_hstr_arr(rj, f"{p}.readers"))
    derived_from = []
    dj = _hget(j, "derived_from")
    if dj is not _ABSENT:
        if not isinstance(dj, list):
            _hsv(p, ".derived_from must be an array")
        for i, d in enumerate(dj):
            dp = f"{p}.derived_from[{i}]"
            kind = _hreq_str(d, "kind", dp)
            if kind not in _DERIVATION_KINDS:
                _hfail("UnknownKind", f"derivation kind {kind}")
            det = _hget(d, "deterministic")
            if not isinstance(det, bool):
                _hsv(dp, ".deterministic must be a bool")
            derived_from.append({
                "kind": kind,
                "inputs": _hstr_arr(_harr(d, "inputs", dp), f"{dp}.inputs"),
                "deterministic": det,
            })
    att = _hget(j, "attestation")
    return {
        "origin": origin,
        "authority": authority,
        "scope": scope,
        "created_at": created_at,
        "taint": taint,
        "readers": readers,
        "derived_from": derived_from,
        "attestation": (
            None if att is _ABSENT else _attestation_from_json(att, f"{p}.attestation")
        ),
    }


def _provenance_json(p):
    out = {
        "origin": _origin_json(p["origin"]),
        "authority": _AUTHORITY_NAME[p["authority"]],
        "scope": p["scope"],
        "created_at": p["created_at"],
    }
    if p["taint"]:
        out["taint"] = list(p["taint"])
    if p["readers"] is not None:
        out["readers"] = list(p["readers"])
    if p["derived_from"]:
        out["derived_from"] = [
            {"kind": d["kind"], "inputs": list(d["inputs"]),
             "deterministic": d["deterministic"]}
            for d in p["derived_from"]
        ]
    if p["attestation"] is not None:
        out["attestation"] = _attestation_json(p["attestation"])
    return out


def _default_authority(o, att) -> int:
    """`default_authority(origin, scope, attestation)` under the default
    (empty) minting context — no closed-world tools, no vouched
    participants."""
    if att is not None and not _att_self_consistent(att):
        return _AUTHORITY["unverified"]
    kind = o["kind"]
    if kind == "kernel":
        return _AUTHORITY["kernel"]
    if kind == "human":
        return _AUTHORITY["principal"]
    if kind in ("model", "evolution"):
        return _AUTHORITY["delegate"]
    if kind == "tool":
        return _AUTHORITY["external"]
    if kind == "cache":
        return _AUTHORITY["environment"]
    return _AUTHORITY["unverified"]  # participant | import | migration | other


def _minted_ceiling(p) -> int:
    """`minted_ceiling`: the default authority, lifted to `definition` by a
    self-consistent seal/pin attestation."""
    base = _default_authority(p["origin"], p["attestation"])
    att = p["attestation"]
    if (
        att is not None
        and _att_self_consistent(att)
        and att["kind"] in ("seal", "pin")
    ):
        return max(base, _AUTHORITY["definition"])
    return base


def _check_provenance(p, what, errs):
    """`ProvenanceRecord::validate(None)` → the `HirError` spelling."""
    if p["taint"] and p["authority"] > _AUTHORITY["external"]:
        errs.append((
            "TaintedAboveExternal",
            f"{what}: {_AUTHORITY_NAME[p['authority']]}",
        ))
        return
    ceiling = _minted_ceiling(p)
    if p["authority"] > ceiling:
        errs.append((
            "AuthorityExceedsOrigin",
            f"{what}: claimed {_AUTHORITY_NAME[p['authority']]} above ceiling "
            f"{_AUTHORITY_NAME[ceiling]}",
        ))
        return
    att = p["attestation"]
    if att is not None and not _att_self_consistent(att):
        errs.append((
            "IllegitimateEndorsement",
            f"{what}: attestation failed: attestation is a bare self-claim "
            "(missing subject, anchor or verifier)",
        ))


# Refs — `Ref` / `ProfileRef` / `ComponentVariantRef`.

def _ref_from_json(j, p):
    sid = _hopt_str(j, "semantic_id")
    if sid is None:
        _hsv(p, ".semantic_id missing")
    v = _hopt_str(j, "version_id")
    s = _hopt_str(j, "version_selector")
    if v is not None and s is None:
        return {"sid": sid, "pin": v, "sel": None}
    if v is None and s is not None:
        return {"sid": sid, "pin": None, "sel": s}
    _hsv(p, " must carry exactly one of version_id|version_selector")


def _ref_json(r, semantic):
    if semantic:
        return {"semantic_id": r["sid"]}
    if r["pin"] is not None:
        return {"semantic_id": r["sid"], "version_id": r["pin"]}
    return {"semantic_id": r["sid"], "version_selector": r["sel"]}


def _refs_from_json(j, p):
    if not isinstance(j, list):
        _hsv(p, " must be an array of refs")
    return [_ref_from_json(r, f"{p}[{i}]") for i, r in enumerate(j)]


def _profile_ref_from_json(j, p):
    return {
        "profile": _hreq_str(j, "profile_ref", p),
        "pinned": _hbool_or(j, "pinned", p, False),
    }


def _profile_ref_json(pr):
    return {"profile_ref": pr["profile"], "pinned": pr["pinned"]}


def _cvr_from_json(j, p):
    if isinstance(j, dict):
        for k in j:
            if k not in ("class_id", "variant_id", "version_id", "version_selector"):
                _hsv(p, f".{k}: unknown ComponentVariantRef member")

    def field(k):
        x = _hopt_str(j, k)
        if x is None:
            _hsv(p, f".{k} missing")
        return x

    v = _hopt_str(j, "version_id")
    s = _hopt_str(j, "version_selector")
    if v is not None and s is None:
        return {"class_id": field("class_id"), "variant_id": field("variant_id"),
                "pin": v, "sel": None}
    if v is None and s is not None:
        return {"class_id": field("class_id"), "variant_id": field("variant_id"),
                "pin": None, "sel": s}
    _hsv(p, " must carry exactly one of version_id|version_selector")


def _cvr_json(c, semantic):
    if semantic:
        return {"class_id": c["class_id"], "variant_id": c["variant_id"]}
    out = {"class_id": c["class_id"], "variant_id": c["variant_id"]}
    if c["pin"] is not None:
        out["version_id"] = c["pin"]
    else:
        out["version_selector"] = c["sel"]
    return out


# Leaves — `Text` (`CompiledPayload` is corpus-unreached: it lives only on
# unexercised kinds, inside the honest bound).

def _text_from_json(j, p):
    def g(k):
        v = _hget(j, k)
        if not isinstance(v, str):
            _hsv(p, f".{k} missing")
        return v

    content_hash = g("content_hash")
    owner = g("owner")
    authority = _authority_parse(
        g("authority"), f"{p}.authority: bad AuthorityClass"
    )
    prov = _hget(j, "provenance")
    if prov is _ABSENT:
        raise _HirFail("MissingProvenance", f"{p} (Text leaf)")
    t = {
        "content_hash": content_hash,
        "owner": owner,
        "authority": authority,
        "provenance": _provenance_from_json(prov, p),
    }
    language = _hopt_str(j, "language")
    if language is not None:
        t["language"] = language
    return t


def _text_json(t, semantic):
    out = {
        "content_hash": t["content_hash"],
        "owner": t["owner"],
        "authority": _AUTHORITY_NAME[t["authority"]],
    }
    if not semantic:
        out["provenance"] = _provenance_json(t["provenance"])
    if "language" in t:
        out["language"] = t["language"]
    return out


# Effect classes — `EffectClass` / `EffectAttributes` / `Reversibility`; the
# `BTreeSet<EffectClass>` order is derived-`Ord` (domain rank, then
# attributes), so `declared` re-emits sorted.

_MUT_RANK = {"read_only": 0, "additive": 1, "destructive": 2}
_RS_RANK = {"idempotent": 0, "non_idempotent": 1}
_WORLD_RANK = {"closed": 0, "open": 1}


def _domain_rank(d):
    try:
        return _EFFECT_DOMAINS.index(d)
    except ValueError:
        _hfail("UnknownKind", f"effect domain {d}")


def _reversibility_from_json(j, p):
    if isinstance(j, str):
        if j not in _REVERSIBILITIES:
            _hfail("UnknownKind", f"reversibility {j}")
        return {"s": j}
    if isinstance(j, dict):
        r = _hget(j, "reversible")
        if r is _ABSENT:
            _hsv(p, ": bad reversibility")
        return {"pin": _ref_from_json(r, f"{p}.reversible")}
    _hsv(p, ": bad reversibility")


def _reversibility_json(r, semantic):
    if "pin" in r:
        return {"reversible": _ref_json(r["pin"], semantic)}
    return r["s"]


def _attributes_from_json(j, p):
    def g(k):
        v = _hopt_str(j, k)
        if v is None:
            _hsv(p, f".{k} missing")
        return v

    mutability = g("mutability")
    if mutability not in _MUTABILITIES:
        _hfail("UnknownKind", f"mutability {mutability}")
    repeat_safety = g("repeat_safety")
    if repeat_safety not in _REPEAT_SAFETIES:
        _hfail("UnknownKind", f"repeat_safety {repeat_safety}")
    world = g("world")
    if world not in _WORLDS:
        _hfail("UnknownKind", f"world {world}")
    rev = _hget(j, "reversibility")
    if rev is _ABSENT:
        _hsv(p, ".reversibility missing")
    return {
        "mutability": mutability,
        "repeat_safety": repeat_safety,
        "world": world,
        "reversibility": _reversibility_from_json(rev, f"{p}.reversibility"),
    }


def _attributes_json(a, semantic):
    return {
        "mutability": a["mutability"],
        "repeat_safety": a["repeat_safety"],
        "world": a["world"],
        "reversibility": _reversibility_json(a["reversibility"], semantic),
    }


def _effect_class_from_json(j, p):
    d = _hopt_str(j, "domain")
    if d is None:
        _hsv(p, ".domain missing")
    _domain_rank(d)  # UnknownKind on an unregistered domain
    a = _hget(j, "attributes")
    return {
        "domain": d,
        "attributes": (
            None if a is _ABSENT else _attributes_from_json(a, f"{p}.attributes")
        ),
    }


def _effect_class_json(e, semantic):
    if e["attributes"] is None:
        return {"domain": e["domain"]}
    return {
        "domain": e["domain"],
        "attributes": _attributes_json(e["attributes"], semantic),
    }


def _rev_rank(r):
    # Reversible < Compensable < Irreversible by declaration order.
    return 0 if "pin" in r else (1 if r["s"] == "compensable" else 2)


def _ref_cmp(a, b):
    if a["sid"] != b["sid"]:
        return -1 if a["sid"] < b["sid"] else 1
    if (a["pin"] is None) != (b["pin"] is None):
        return -1 if a["pin"] is not None else 1
    av = a["pin"] if a["pin"] is not None else a["sel"]
    bv = b["pin"] if b["pin"] is not None else b["sel"]
    return (av > bv) - (av < bv)


def _rev_cmp(a, b):
    ra, rb = _rev_rank(a), _rev_rank(b)
    if ra != rb:
        return ra - rb
    if "pin" in a and "pin" in b:
        return _ref_cmp(a["pin"], b["pin"])
    return 0


def _effect_cmp(a, b):
    d = _domain_rank(a["domain"]) - _domain_rank(b["domain"])
    if d != 0:
        return d
    if a["attributes"] is None or b["attributes"] is None:
        if a["attributes"] is None and b["attributes"] is None:
            return 0
        return -1 if a["attributes"] is None else 1
    A, B = a["attributes"], b["attributes"]
    for x, y, t in (
        (A["mutability"], B["mutability"], _MUT_RANK),
        (A["repeat_safety"], B["repeat_safety"], _RS_RANK),
        (A["world"], B["world"], _WORLD_RANK),
    ):
        c = t.get(x, 0) - t.get(y, 0)
        if c != 0:
            return c
    return _rev_cmp(A["reversibility"], B["reversibility"])


def _effect_set(items):
    """The `BTreeSet<EffectClass>` — sorted by (domain rank, attributes),
    duplicates collapsed."""
    out = sorted(items, key=functools.cmp_to_key(_effect_cmp))
    return [
        e for i, e in enumerate(out)
        if i == 0 or _effect_cmp(e, out[i - 1]) != 0
    ]


# Record-member decoders — validity / boundary / capability declaration /
# slots / supplies / rule action / grants / tool members.

_CAP_DECL_FIELDS = [
    "streaming", "interrupt", "steer", "live_queue", "resume", "fork",
    "compaction", "images", "subagents", "permission_surface",
    "instruction_delivery", "model_family", "effort_vocabulary",
    "trajectory_export", "native_config",
]


def _validity_from_json(j, p):
    until = _hget(j, "until")
    if until is not _ABSENT and (
        not isinstance(until, int) or isinstance(until, bool) or until < 0
    ):
        _hsv(p, ".until must be a non-negative integer")
    return {
        "from": _hreq_int(j, "from", p),
        "until": None if until is _ABSENT else until,
        "condition": _hopt_str(j, "condition"),
    }


def _validity_json(v):
    out = {"from": v["from"]}
    if v["until"] is not None:
        out["until"] = v["until"]
    if v["condition"] is not None:
        out["condition"] = v["condition"]
    return out


def _boundary_from_json(j, p):
    b = {"assignments": {}, "guards": {}}
    asg = _hget(j, "assignments")
    if isinstance(asg, dict):
        for k, v in asg.items():
            if not isinstance(v, str):
                _hsv(p, f".assignments.{k} must be a string")
            if v not in _OWNERS:
                _hfail("UnknownKind", f"owner {v}")
            if k not in _DECISION_POINTS:
                _hfail("UnknownKind", f"decision point {k}")
            b["assignments"][k] = v
    guards = _hget(j, "guards")
    if isinstance(guards, dict):
        for k, v in guards.items():
            if not isinstance(v, str):
                _hsv(p, f".guards.{k} must be a string")
            if k not in _DECISION_POINTS:
                _hfail("UnknownKind", f"decision point {k}")
            b["guards"][k] = v
    return b


def _boundary_json(b):
    return {
        "assignments": dict(b["assignments"]),
        "guards": dict(b["guards"]),
    }


def _cap_decl_from_json(j, p):
    out = {}
    for f in _CAP_DECL_FIELDS:
        v = _hreq_str(j, f, p)
        if v not in _CAPABILITY_STATES:
            _hfail("UnknownKind", f"capability state {v}")
        out[f] = v
    return out


def _cap_decl_json(c):
    return {f: c[f] for f in _CAP_DECL_FIELDS}


def _slot_binding_from_json(j, p):
    if not isinstance(j, dict):
        _hsv(p, " must be an object")
    for k in j:
        if k not in ("variant", "params", "enabled", "locality"):
            _hsv(p, f".{k}: unknown SlotBinding member")
    params = _hget(j, "params")
    if params is not _ABSENT and not isinstance(params, dict):
        _hsv(p, ".params must be an object")
    locality = _hget(j, "locality")
    if locality is not _ABSENT:
        if not isinstance(locality, str):
            _hsv(p, ".locality must be a string")
        if locality not in _LOCALITIES:
            _hfail("UnknownKind", f"locality:{locality}")
    out = {
        "variant": _cvr_from_json(_hreq(j, "variant", p), f"{p}.variant"),
        "params": {} if params is _ABSENT else params,
        "enabled": _hbool_or(j, "enabled", p, True),
    }
    if locality is not _ABSENT:
        out["locality"] = locality
    return out


def _slot_binding_json(b, semantic):
    out = {
        "variant": _cvr_json(b["variant"], semantic),
        "params": b["params"],
        "enabled": b["enabled"],
    }
    if "locality" in b:
        out["locality"] = b["locality"]
    return out


def _slots_from_json(j, p):
    if not isinstance(j, dict):
        _hsv(p, " must be an object")
    out = {}
    for name, v in j.items():
        one = _hget(v, "one")
        many = _hget(v, "many")
        if one is not _ABSENT:
            out[name] = {"one": _slot_binding_from_json(one, f"{p}.{name}.one")}
        elif isinstance(many, list):
            out[name] = {
                "many": [
                    _slot_binding_from_json(b, f"{p}.{name}.many[{i}]")
                    for i, b in enumerate(many)
                ]
            }
        else:
            _hsv(p, f".{name} must be one|many")
    return out


def _slots_json(slots, semantic):
    out = {}
    for name, bs in slots.items():
        if "one" in bs:
            out[name] = {"one": _slot_binding_json(bs["one"], semantic)}
        else:
            out[name] = {
                "many": [_slot_binding_json(b, semantic) for b in bs["many"]]
            }
    return out


def _supplies_from_json(j, p):
    def refs(k):
        v = _hget(j, k)
        return _refs_from_json([] if v is _ABSENT else v, f"{p}.{k}")

    return {
        "context": refs("context"),
        "tools": refs("tools"),
        "procedures": refs("procedures"),
    }


def _supplies_json(s, semantic):
    return {
        "context": [_ref_json(r, semantic) for r in s["context"]],
        "tools": [_ref_json(r, semantic) for r in s["tools"]],
        "procedures": [_ref_json(r, semantic) for r in s["procedures"]],
    }


def _action_from_json(j, p):
    def single(k):
        v = _hget(j, k)
        return None if v is _ABSENT else {"v": k, "r": _ref_from_json(v, f"{p}.{k}")}

    a = single("insert_context_item")
    if a is not None:
        return a
    rs = _hget(j, "restrict_tool_set")
    if rs is not _ABSENT:
        return {
            "v": "restrict_tool_set",
            "rs": _refs_from_json(rs, f"{p}.restrict_tool_set"),
        }
    for k in (
        "set_compaction_policy", "set_retry_stop_policy", "request_approval",
        "flow_policy", "pre_authorize", "auto_review", "consolidate_memories",
    ):
        v = _hget(j, k)
        if v is not _ABSENT:
            return {"v": k, "j": v}
    a = single("require_validator")
    if a is not None:
        return a
    d = _hget(j, "declassify")
    if d is not _ABSENT:
        if not isinstance(d, list):
            _hsv(p, ".declassify must be an array")
        return {"v": "declassify", "j": _hstr_arr(d, f"{p}.declassify")}
    san = _hget(j, "sanitize")
    if san is not _ABSENT:
        param = _hget(san, "param")
        return {
            "v": "sanitize",
            "s": _hreq_str(san, "sanitizer_ref", f"{p}.sanitize"),
            "j": None if param is _ABSENT else param,
        }
    _hsv(p, ": unknown rule action")


def _action_json(a, semantic):
    v = a["v"]
    if v == "insert_context_item":
        return {"insert_context_item": _ref_json(a["r"], semantic)}
    if v == "restrict_tool_set":
        return {"restrict_tool_set": [_ref_json(r, semantic) for r in a["rs"]]}
    if v == "require_validator":
        return {"require_validator": _ref_json(a["r"], semantic)}
    if v == "declassify":
        return {"declassify": a["j"]}
    if v == "sanitize":
        return {"sanitize": {"sanitizer_ref": a["s"], "param": a["j"]}}
    return {v: a["j"]}


def _grant_from_json(j, p):
    cj = _hreq(j, "constraints", p)
    c = {}
    bj = _hget(cj, "budget")
    if bj is not _ABSENT:
        c["budget"] = bj
    for k in ("time", "count"):
        v = _hget(cj, k)
        if v is _ABSENT:
            continue
        if not isinstance(v, int) or isinstance(v, bool) or v < 0:
            _hsv(p, f".constraints.{k} must be a non-negative int")
        c[k] = v
    delegable = _hget(j, "delegable")
    if not isinstance(delegable, bool):
        _hsv(p, ".delegable must be a bool")
    return {
        "effect": _effect_class_from_json(_hreq(j, "effect", p), f"{p}.effect"),
        "scope": _hreq_str(j, "scope", p),
        "constraints": c,
        "delegable": delegable,
    }


def _grant_json(g, semantic):
    c = {}
    for k in ("budget", "time", "count"):
        if k in g["constraints"]:
            c[k] = g["constraints"][k]
    return {
        "effect": _effect_class_json(g["effect"], semantic),
        "scope": g["scope"],
        "constraints": c,
        "delegable": g["delegable"],
    }


def _tool_effects_from_json(j, p):
    if j == "pure":
        return "pure"
    if isinstance(j, dict):
        d = _hreq(j, "declared", p)
        if not isinstance(d, list):
            _hsv(p, ".declared must be an array")
        return {
            "declared": _effect_set([
                _effect_class_from_json(e, f"{p}.declared[{i}]")
                for i, e in enumerate(d)
            ])
        }
    _hsv(p, '.effects must be "pure" or {declared:[...]}')


def _tool_effects_json(e, semantic):
    if e == "pure":
        return "pure"
    return {"declared": [_effect_class_json(x, semantic) for x in e["declared"]]}


def _resources_from_json(j, p):
    d = _hget(j, "declared")
    if d is not _ABSENT:
        if not isinstance(d, list):
            _hsv(p, ".declared must be an array")
        return {"declared": _sorted_set(_hstr_arr(d, f"{p}.declared"))}
    s = _hopt_str(j, "state")
    if s in ("none", "unknown"):
        return {"state": s}
    _hsv(p, ": resources must be declared|none|unknown")


def _resources_json(r):
    if "declared" in r:
        return {"declared": list(r["declared"])}
    return {"state": r["state"]}


def _scope_bindings_rows(s):
    """`tools::scope_bindings` — decode the bindings member into rows.
    `"unknown"` → None (the unknown spelling is not an error)."""
    if s == "unknown":
        return None
    rows = [] if s is None else (s if isinstance(s, list) else [s])
    out = []
    for i, r in enumerate(rows):
        pp = _hopt_str(r, "param_path")
        if pp is None:
            _hfail("SchemaViolation", f"scope_bindings[{i}].param_path missing")
        sk = _hopt_str(r, "scope_kind")
        if sk is None:
            _hfail("SchemaViolation", f"scope_bindings[{i}].scope_kind missing")
        if sk not in _SCOPE_KINDS:
            _hfail("UnknownKind", f"scope_kind {sk}")
        out.append({"param_path": pp, "scope_kind": sk})
    return out


# Semantic records — the five exercised kinds; unexercised registered kinds
# decode as `raw` and flag the case unported (the arm then omits the answer).

def _semantic_from_json(kind, j, p, unported):
    if kind == "AgentProcess":
        bj = _hreq(j, "body", p)
        n = _hget(bj, "native")
        h = _hget(bj, "hosted")
        if n is not _ABSENT:
            np = f"{p}.body.native"
            return {"k": "AgentProcess", "native": {
                "harness_def": _ref_from_json(_hreq(n, "harness_def", np), f"{np}.harness_def"),
                "profile": _profile_ref_from_json(_hreq(n, "profile", np), f"{np}.profile"),
                "slots": _slots_from_json(_hreq(n, "slots", np), f"{np}.slots"),
                "control_boundary": _boundary_from_json(
                    _hreq(n, "control_boundary", np), f"{np}.control_boundary"),
                "budget": _ref_from_json(_hreq(n, "budget", np), f"{np}.budget"),
                "permissions": _ref_from_json(_hreq(n, "permissions", np), f"{np}.permissions"),
                "environment": _hreq_str(_hreq(n, "environment", np), "environment", f"{np}.environment"),
            }}
        if h is not _ABSENT:
            hp = f"{p}.body.hosted"
            mech = _hreq_str(h, "hosting_mechanism", hp)
            if mech == "none":
                _hsv(hp, ".hosting_mechanism: none on OpaqueProcess (CF-351)")
            if mech not in _HOSTING:
                _hfail("UnknownKind", f"hosting mechanism {mech}")
            obs = []
            for o in _harr(h, "observability_levels", hp):
                if not isinstance(o, str):
                    _hsv(hp, ".observability_levels must be strings")
                if o not in _OBSERVABILITY:
                    _hfail("UnknownKind", f"observability {o}")
                obs.append(o)
            hosted = {
                "participant_ref": _hreq_str(h, "participant_ref", hp),
                "declared_capabilities": _cap_decl_from_json(
                    _hreq(h, "declared_capabilities", hp),
                    f"{hp}.declared_capabilities",
                ),
                "observability_levels": obs,
                "participant_version": _hreq_str(h, "participant_version", hp),
                "hosting_mechanism": mech,
                "supplies": _supplies_from_json(_hreq(h, "supplies", hp), f"{hp}.supplies"),
                "budget": _ref_from_json(_hreq(h, "budget", hp), f"{hp}.budget"),
                "permissions": _ref_from_json(_hreq(h, "permissions", hp), f"{hp}.permissions"),
            }
            vi = _hopt_str(h, "version_identity")
            if vi is not None:
                hosted["version_identity"] = vi
            params = _hget(h, "params")
            if params is not _ABSENT:
                hosted["params"] = params
            return {"k": "AgentProcess", "hosted": hosted}
        _hsv(p, ".body must be native|hosted")
    if kind == "Budget":
        dj = _hreq(j, "dimensions", p)
        if not isinstance(dj, dict):
            _hsv(p, ".dimensions must be an object")
        dims = {}
        for dn, v in dj.items():
            bound = {}
            hd = _hget(v, "hard")
            if isinstance(hd, int) and not isinstance(hd, bool) and hd >= 0:
                bound["hard"] = hd
            sd = _hget(v, "soft")
            if isinstance(sd, int) and not isinstance(sd, bool) and sd >= 0:
                bound["soft"] = sd
            dims[dn] = bound
        out = {
            "k": "Budget",
            "dims": dims,
            "scope": _hreq_str(j, "scope", p),
            "accounting": _ref_from_json(_hreq(j, "accounting", p), f"{p}.accounting"),
        }
        parent = _hget(j, "parent")
        if parent is not _ABSENT:
            out["parent"] = _ref_from_json(parent, f"{p}.parent")
        return out
    if kind == "HarnessRule":
        if _hhas(j, "assumption_debt"):
            unported.append(f"{p}.assumption_debt")
        out = {
            "k": "HarnessRule",
            "rule_id": _hreq_str(j, "rule_id", p),
            "trigger": _hreq(j, "trigger", p),
            "action": _action_from_json(_hreq(j, "action", p), f"{p}.action"),
            "scope": _hreq(j, "scope", p),
        }
        co = _hget(j, "conditioned_on")
        if co is not _ABSENT:
            out["conditioned_on"] = _profile_ref_from_json(co, f"{p}.conditioned_on")
        return out
    if kind == "Permission":
        ij = _hreq(j, "issuer", p)
        out = {
            "k": "Permission",
            "holder": _ref_from_json(_hreq(j, "holder", p), f"{p}.holder"),
            "grants": [
                _grant_from_json(g, f"{p}.grants[{i}]")
                for i, g in enumerate(_harr(j, "grants", p))
            ],
            "issuer": {
                "authority": _authority_parse(
                    _hreq_str(ij, "authority", f"{p}.issuer"),
                    f"{p}.issuer.authority",
                ),
                "reference": _hreq_str(ij, "reference", f"{p}.issuer"),
            },
            "validity": _validity_from_json(_hreq(j, "validity", p), f"{p}.validity"),
        }
        rev = _hget(j, "revocation")
        if rev is not _ABSENT:
            out["revocation"] = rev
        return out
    if kind == "ToolCapability":
        sb = _hget(j, "scope_bindings")
        sb = "unknown" if _hhas(j, "scope_bindings_unknown") else (
            None if sb is _ABSENT else sb
        )
        pre = []
        for x in _harr(j, "preconditions", p):
            if not isinstance(x, str):
                _hsv(p, ".preconditions members must be strings")
            if x not in _PRECONDITION_DOMAINS:
                _hfail("UnknownKind", f"precondition domain {x}")
            pre.append(x)
        action_patterns = []
        ap = _hget(j, "action_patterns")
        if ap is not _ABSENT:
            if not isinstance(ap, list):
                _hsv(p, ".action_patterns must be an array")
            for i, r in enumerate(ap):
                fs = _hget(r, "fields")
                if not isinstance(fs, list):
                    _hsv(p, f".action_patterns[{i}].fields missing")
                action_patterns.append({
                    "fields": _hstr_arr(fs, f"{p}.action_patterns[{i}].fields")
                })
        if _hhas(j, "flow_contract"):
            unported.append(f"{p}.flow_contract")
        t = {
            "purpose": _text_from_json(_hreq(j, "purpose", p), f"{p}.purpose"),
            "input_schema": _hreq(j, "input_schema", p),
            "effects": _tool_effects_from_json(_hreq(j, "effects", p), f"{p}.effects"),
            "preconditions": pre,
            "scope_bindings": sb,
            "resources": _resources_from_json(_hreq(j, "resources", p), f"{p}.resources"),
            "observation_contract": _hreq(j, "observation_contract", p),
            "execution_requirement": _hreq(j, "execution_requirement", p),
            "source": _hreq(j, "source", p),
            "exposure_hint": _hreq(j, "exposure_hint", p),
            "postconditions": _refs_from_json(_hreq(j, "postconditions", p), f"{p}.postconditions"),
            "action_patterns": action_patterns,
        }
        os_ = _hget(j, "output_schema")
        if os_ is not _ABSENT:
            t["output_schema"] = os_
        cm = _hget(j, "cost_model")
        if cm is not _ABSENT:
            t["cost_model"] = cm
        # `flow_contract` is flagged unported at decode above — the case
        # never reaches encode.
        return {"k": "ToolCapability", "t": t}
    unported.append(f"{p} (kind {kind})")
    return {"k": kind, "raw": j}


def _semantic_json(rec, semantic):
    """`semantic_record_json(rec, semantic)` — the faithful re-emit for the
    ported kinds; `semantic` drops provenance, selector spellings and the
    non-semantic members (`exposure_hint`, `cost_model.measured_ref`,
    `version_identity` stays — it is derived content)."""
    k = rec["k"]
    if k == "AgentProcess":
        if "native" in rec:
            n = rec["native"]
            body = {"native": {
                "harness_def": _ref_json(n["harness_def"], semantic),
                "profile": _profile_ref_json(n["profile"]),
                "slots": _slots_json(n["slots"], semantic),
                "control_boundary": _boundary_json(n["control_boundary"]),
                "budget": _ref_json(n["budget"], semantic),
                "permissions": _ref_json(n["permissions"], semantic),
                "environment": {"environment": n["environment"]},
            }}
        else:
            h = rec["hosted"]
            ho = {
                "participant_ref": h["participant_ref"],
                "declared_capabilities": _cap_decl_json(h["declared_capabilities"]),
                "observability_levels": list(h["observability_levels"]),
                "participant_version": h["participant_version"],
                "hosting_mechanism": h["hosting_mechanism"],
                "supplies": _supplies_json(h["supplies"], semantic),
                "budget": _ref_json(h["budget"], semantic),
                "permissions": _ref_json(h["permissions"], semantic),
            }
            if "version_identity" in h:
                ho["version_identity"] = h["version_identity"]
            if "params" in h:
                ho["params"] = h["params"]
            body = {"hosted": ho}
        return {"body": body}
    if k == "Budget":
        dims = {}
        for d, b in rec["dims"].items():
            bound = {}
            if "hard" in b:
                bound["hard"] = b["hard"]
            if "soft" in b:
                bound["soft"] = b["soft"]
            dims[d] = bound
        out = {
            "dimensions": dims,
            "scope": rec["scope"],
            "accounting": _ref_json(rec["accounting"], semantic),
        }
        if "parent" in rec:
            out["parent"] = _ref_json(rec["parent"], semantic)
        return out
    if k == "HarnessRule":
        out = {
            "rule_id": rec["rule_id"],
            "trigger": rec["trigger"],
            "action": _action_json(rec["action"], semantic),
            "scope": rec["scope"],
        }
        if "conditioned_on" in rec:
            out["conditioned_on"] = _profile_ref_json(rec["conditioned_on"])
        return out
    if k == "Permission":
        out = {
            "holder": _ref_json(rec["holder"], semantic),
            "grants": [_grant_json(g, semantic) for g in rec["grants"]],
            "issuer": {
                "authority": _AUTHORITY_NAME[rec["issuer"]["authority"]],
                "reference": rec["issuer"]["reference"],
            },
            "validity": _validity_json(rec["validity"]),
        }
        if "revocation" in rec:
            out["revocation"] = rec["revocation"]
        return out
    if k == "ToolCapability":
        t = rec["t"]
        out = {
            "purpose": _text_json(t["purpose"], semantic),
            "input_schema": t["input_schema"],
            "effects": _tool_effects_json(t["effects"], semantic),
            "preconditions": list(t["preconditions"]),
            "resources": _resources_json(t["resources"]),
            "observation_contract": t["observation_contract"],
            "execution_requirement": t["execution_requirement"],
            "source": t["source"],
            "postconditions": [_ref_json(r, semantic) for r in t["postconditions"]],
        }
        if not semantic:
            out["exposure_hint"] = t["exposure_hint"]
        if "output_schema" in t:
            out["output_schema"] = t["output_schema"]
        if t["scope_bindings"] == "unknown":
            out["scope_bindings_unknown"] = True
        else:
            out["scope_bindings"] = t["scope_bindings"]
        if "cost_model" in t:
            cm = t["cost_model"]
            if semantic and isinstance(cm, dict):
                cm = {k2: v for k2, v in cm.items() if k2 != "measured_ref"}
            out["cost_model"] = cm
        if t["action_patterns"]:
            out["action_patterns"] = [
                {"fields": list(p_["fields"])} for p_ in t["action_patterns"]
            ]
        return out
    return rec["raw"]


# Surface / edge / node / document decode + encode.

def _surface_from_json(kind, j, p):
    if kind not in _HAS_SURFACE:
        _hfail(
            "UnexpressibleSurface",
            f"{p}: kind {kind} carries no surface record (§3.1.5)",
        )
    if kind == "ToolCapability":
        s = {
            "name": _hreq_str(j, "name", p),
            "namespace": _hreq_str(j, "namespace", p),
            "description_template": _text_from_json(
                _hreq(j, "description_template", p), f"{p}.description_template"
            ),
            "argument_order": _hstr_arr(_harr(j, "argument_order", p), f"{p}.argument_order"),
            "examples": _hopt_or(j, "examples", []),
            "error_format": _hopt_or(j, "error_format", None),
            "result_renderer": _hopt_or(j, "result_renderer", None),
            "strictness": _hopt_or(j, "strictness", None),
            "schema_dialect_narrowing": _hopt_or(j, "schema_dialect_narrowing", None),
            "exposure_mode": _hopt_or(j, "exposure_mode", None),
        }
        dt = _hopt_str(j, "display_title")
        if dt is not None:
            s["display_title"] = dt
        ir = _hopt_str(j, "icon_ref")
        if ir is not None:
            s["icon_ref"] = ir
        return {"t": "Tool", "s": s}
    # Validator/Procedure/ContextItem surfaces: the host kind is unported,
    # so the surface is carried raw (the case answers "missing").
    return {"t": kind, "raw": j}


def _surface_json(s):
    if s["t"] != "Tool":
        return s["raw"]
    t = s["s"]
    out = {
        "name": t["name"],
        "namespace": t["namespace"],
        "description_template": _text_json(t["description_template"], False),
        "argument_order": list(t["argument_order"]),
        "examples": t["examples"],
        "error_format": t["error_format"],
        "result_renderer": t["result_renderer"],
        "strictness": t["strictness"],
        "schema_dialect_narrowing": t["schema_dialect_narrowing"],
        "exposure_mode": t["exposure_mode"],
    }
    if "display_title" in t:
        out["display_title"] = t["display_title"]
    if "icon_ref" in t:
        out["icon_ref"] = t["icon_ref"]
    return out


def _version_from_json(j, p):
    out = {"dialect": "HIR/1", "sealed": _hbool_or(j, "sealed", p, False)}
    vi = _hopt_str(j, "version_id")
    if vi is not None:
        out["version_id"] = vi
    si = _hopt_str(j, "semantic_id")
    if si is not None:
        out["semantic_id"] = si
    sup = _hopt_str(j, "supersedes")
    if sup is not None:
        out["supersedes"] = sup
    return out


def _version_json(v):
    out = {"sealed": v["sealed"]}
    if "version_id" in v:
        out["version_id"] = v["version_id"]
    if "semantic_id" in v:
        out["semantic_id"] = v["semantic_id"]
    if "supersedes" in v:
        out["supersedes"] = v["supersedes"]
    return out


def _node_from_json(j, p, unported):
    kind = _hreq_str(j, "kind", p)
    if kind not in _ENTITY_KINDS:
        _hfail("UnknownKind", kind)
    dialect = _hreq_str(j, "dialect", p)
    if dialect != "HIR/1":
        _hfail("DialectUnsupported", dialect)
    prov = _hget(j, "provenance")
    if prov is _ABSENT:
        raise _HirFail("MissingProvenance", f"{p} (node {kind})")
    provenance = _provenance_from_json(prov, f"{p}.provenance")
    semantic = _semantic_from_json(
        kind, _hreq(j, "semantic", p), f"{p}.semantic", unported
    )
    sj = _hget(j, "surface")
    surface = None if sj is _ABSENT else _surface_from_json(kind, sj, f"{p}.surface")
    ext = {}
    ej = _hget(j, "ext")
    if ej is not _ABSENT:
        if not isinstance(ej, dict):
            _hsv(p, ".ext must be an object")
        ext = dict(ej)
    version = _version_from_json(_hreq(j, "version", p), f"{p}.version")
    version["dialect"] = dialect
    node = {
        "kind": kind,
        "provenance": provenance,
        "semantic": semantic,
        "ext": ext,
        "version": version,
    }
    if surface is not None:
        node["surface"] = surface
    return node


def _node_to_json(n):
    out = {
        "kind": n["kind"],
        "dialect": n["version"]["dialect"],
        "provenance": _provenance_json(n["provenance"]),
        "semantic": _semantic_json(n["semantic"], False),
        "version": _version_json(n["version"]),
    }
    if "surface" in n:
        out["surface"] = _surface_json(n["surface"])
    if n["ext"]:
        out["ext"] = n["ext"]
    return out


def _node_semantic_projection(n):
    return {"kind": n["kind"], "semantic": _semantic_json(n["semantic"], True)}


def _node_identity_basis(n):
    basis = _node_to_json(n)
    v = dict(basis["version"])
    v.pop("version_id", None)
    v.pop("semantic_id", None)
    basis["version"] = v
    return basis


def _edge_from_json(j, p):
    kind = _hreq_str(j, "kind", p)
    if kind not in _EDGE_KINDS:
        _hfail("UnknownKind", kind)
    dialect = _hreq_str(j, "dialect", p)
    if dialect != "HIR/1":
        _hfail("DialectUnsupported", dialect)
    prov = _hget(j, "provenance")
    if prov is _ABSENT:
        raise _HirFail("MissingProvenance", f"{p} (edge {kind})")
    provenance = _provenance_from_json(prov, f"{p}.provenance")
    fields = _hreq(j, "fields", p)  # carried raw — edge-bearing cases flag unported
    version = _version_from_json(_hreq(j, "version", p), f"{p}.version")
    version["dialect"] = dialect
    return {
        "kind": kind,
        "from": _hreq_str(j, "from", p),
        "to": _hreq_str(j, "to", p),
        "provenance": provenance,
        "fields": fields,
        "version": version,
    }


def _document_from_json(j):
    hir_version = _hreq_str(j, "hir_version", "$")
    if hir_version != "HIR/1":
        _hfail("DialectUnsupported", hir_version)
    nodes_j = _hreq(j, "nodes", "$")
    if not isinstance(nodes_j, list):
        _hsv("$", ".nodes must be an array")
    edges_j = _hreq(j, "edges", "$")
    if not isinstance(edges_j, list):
        _hsv("$", ".edges must be an array")
    unported = []
    nodes = [_node_from_json(n, f"$.nodes[{i}]", unported) for i, n in enumerate(nodes_j)]
    edges = [_edge_from_json(e, f"$.edges[{i}]") for i, e in enumerate(edges_j)]
    if edges:
        unported.append("$.edges (edge-bearing document)")
    doc = {
        "hir_version": hir_version,
        "root": _ref_from_json(_hreq(j, "root", "$"), "$.root"),
        "nodes": nodes,
        "edges": edges,
        "unported": unported,
    }
    asm = _hget(j, "assembly")
    if asm is not _ABSENT:
        doc["assembly"] = asm
    return doc


def _doc_to_json(doc):
    nodes = sorted(
        (_node_to_json(n) for n in doc["nodes"]), key=canonical
    )
    edges = sorted(
        (
            {
                "kind": e["kind"],
                "dialect": e["version"]["dialect"],
                "from": e["from"],
                "to": e["to"],
                "provenance": _provenance_json(e["provenance"]),
                "fields": e["fields"],
                "version": _version_json(e["version"]),
            }
            for e in doc["edges"]
        ),
        key=canonical,
    )
    out = {
        "hir_version": doc["hir_version"],
        "root": _ref_json(doc["root"], False),
        "nodes": nodes,
        "edges": edges,
    }
    if "assembly" in doc:
        out["assembly"] = doc["assembly"]
    return out


# Identity — `idp/1` over the one canonical codec (CC1).

def _hash_tag(domain, j):
    return idp_id(domain, canonical(j))


def _semantic_id(n):
    return _hash_tag("hir.node#semantic", _node_semantic_projection(n))


def _version_id(n):
    return idp_id("hir.node", canonical(_node_identity_basis(n)))


def _node_semantic_id(n):
    return n["version"].get("semantic_id", _semantic_id(n))


def _opaque_version_identity(decl, participant_version):
    return _hash_tag("capability_declaration", {
        "declaration": _cap_decl_json(decl),
        "participant_version": participant_version,
    })


# tools.rs — scope kinds / authored exposure / capability checks.

def _domain_scope_kind(d):
    return {
        "fs_read": "fs_path",
        "fs_write": "fs_path",
        "net_egress": "host",
        "message_human": "recipient",
        "spend": "spend_amount",
        "secret_access": "secret_ref",
        "exec": "process_target",
        "spawn_process": "process_target",
        "memory_write": "memory_scope",
    }.get(d)


def _authored_exposure(j):
    out = {"hidden": False, "pinned": False, "admitted": {"direct"}}
    if j is None or j is _ABSENT:
        return out
    if not isinstance(j, dict):
        _hfail("SchemaViolation", "exposure_mode must be an object")
    mode = _hget(j, "mode")
    if mode == "hidden":
        out["hidden"] = True
    elif mode is not _ABSENT and mode not in ("primitive", "composite_member"):
        _hfail("UnknownKind", f"exposure_mode.mode {mode}")
    pinned = _hget(j, "pinned")
    if pinned is not _ABSENT:
        if not isinstance(pinned, bool):
            _hfail("SchemaViolation", "exposure_mode.pinned must be a bool")
        out["pinned"] = pinned
    modes = _hget(j, "admitted_modes")
    if modes is not _ABSENT and isinstance(modes, list):
        out["admitted"] = set()
        for m in modes:
            if not isinstance(m, str):
                _hfail("SchemaViolation", "exposure_mode.admitted_modes members must be strings")
            if m not in _EXPOSURE_MODES:
                _hfail("UnknownKind", f"admitted_modes {m}")
            out["admitted"].add(m)
    return out


def _param_path_exists(schema, path) -> bool:
    cur = schema
    for seg in path.split("."):
        if (
            _hhas(cur, "anyOf") or _hhas(cur, "oneOf")
            or _hhas(cur, "allOf") or _hhas(cur, "$ref")
        ):
            return True
        nxt = _hget(_hget(cur, "properties"), seg)
        if nxt is _ABSENT:
            additional = (
                _hget(cur, "additionalProperties") is True
                or isinstance(_hget(cur, "additionalProperties"), dict)
                or _hget(cur, "unevaluatedProperties") is True
                or isinstance(_hget(cur, "unevaluatedProperties"), dict)
            )
            unconstrained = (
                _hget(cur, "properties") is _ABSENT
                and _hget(cur, "type") != "object"
            )
            return additional or unconstrained
        cur = nxt
    return True


def _lifted_source_kind(source):
    k = _hget(source, "kind")
    return k if k in ("mcp_listing", "participant_supplied") else None


def _is_discovery_capability(t) -> bool:
    return _hget(t["exposure_hint"], "discovery") is True


def _capability_needs_flow_contract(t) -> bool:
    if t["effects"] == "pure":
        return False
    for e in t["effects"]["declared"]:
        gated = e["domain"] in ("net_egress", "message_human", "fs_read", "memory_write")
        open_ = e["attributes"] is not None and e["attributes"]["world"] == "open"
        if gated or open_:
            return True
    return False


def _validate_capability(t, authority):
    """`tools::validate_capability` — the ToolCapability member battery."""
    errs = []
    if t["effects"] != "pure" and not t["effects"]["declared"]:
        errs.append(("EmptyEffectSet", ""))
    # `scope_bindings()` errors are findings, not propagation (E1 pushes
    # the decode error into `errs`).
    rows = None
    if t["scope_bindings"] != "unknown":
        try:
            rows = _scope_bindings_rows(t["scope_bindings"])
        except _HirFail as e:
            errs.append((e.kind, e.detail))
            # E1's `Err(e) => errs.push(e)` arm skips both loops.
            rows = None
    if rows is not None:
        for b in rows:
            if not _param_path_exists(t["input_schema"], b["param_path"]):
                errs.append(("UnmappedParameter", b["param_path"]))
        if t["effects"] != "pure":
            for e in t["effects"]["declared"]:
                kind = _domain_scope_kind(e["domain"])
                if kind is not None and not any(
                    b["scope_kind"] == kind for b in rows
                ):
                    errs.append(("UnscopedParameter", e["domain"]))
    lifted = _lifted_source_kind(t["source"])
    if lifted is not None and authority != _AUTHORITY["unverified"]:
        errs.append(("LiftedDeclarationsNotUnverified", lifted))
    cm = t.get("cost_model")
    if cm is not None:
        has_measured = _hhas(cm, "measured_ref")
        declared = _hget(cm, "declared")
        if isinstance(declared, dict):
            for dim, est in declared.items():
                if _hget(est, "confidence") == "exact" and not has_measured:
                    errs.append(("ExactCostUnmeasured", dim))
    # `flow_contract` presence is flagged unported at decode (its check
    # needs the FlowContract member decode); the guard here is defensive.
    if "flow_contract" in t:
        raise _HirUnsupported("tool.flow_contract")
    return errs


# The corpus-reached check battery (validate.rs).

def _check_text_leaf(t, path, errs):
    # `check_authority`: authority > external requires a kernel/human origin
    # or a self-consistent seal|pin attestation.
    if t["authority"] > _AUTHORITY["external"]:
        att = t["provenance"]["attestation"]
        ok = t["provenance"]["origin"]["kind"] in ("kernel", "human") or (
            att is not None
            and _att_self_consistent(att)
            and att["kind"] in ("seal", "pin")
        )
        if not ok:
            errs.append((
                "TextAboveExternal",
                f"{path}: {_AUTHORITY_NAME[t['authority']]}",
            ))
    # `content_consistent` is vacuous — `Text::from_json` carries no
    # `content` member into the replayed record.
    _check_provenance(t["provenance"], path, errs)


def _check_member_provenance(p, sealed, what, errs):
    _check_provenance(p, what, errs)
    if (
        sealed
        and p["scope"] == "definition"
        and p["authority"] < _AUTHORITY["definition"]
    ):
        errs.append((
            "ScopeCeilingExceeded",
            f"{what}: sealed member scoped definition carries "
            f"{_AUTHORITY_NAME[p['authority']]}",
        ))


def _check_ext(ext, errs):
    for k in ext:
        if "/" not in k:
            errs.append((
                "SchemaViolation",
                f'ext key "{k}" is not a prefixed key (org.name/key)',
            ))
        elif k.startswith("hir/"):
            errs.append((
                "SchemaViolation",
                f'ext key "{k}" uses the reserved hir/ namespace',
            ))


_DENY_KEYS = {
    "model", "model_id", "model_name", "model_ref", "model_identifier",
    "provider", "llm", "llm_config",
}


def _scan_keys(j, hits):
    if isinstance(j, dict):
        for k, v in j.items():
            if k in _DENY_KEYS:
                hits.append(k)
            _scan_keys(v, hits)
    elif isinstance(j, list):
        for v in j:
            _scan_keys(v, hits)


def _check_model_identity_statics(n, errs):
    hits = []
    _scan_keys(_semantic_json(n["semantic"], False), hits)
    for h in hits:
        errs.append((
            "UnexpressibleSurface",
            f'{n["kind"]}: model-identity field "{h}" outside '
            "ProfileRef/surface (T-LCD-01)",
        ))


def _check_hosted(h, errs):
    if "version_identity" in h:
        expected = _opaque_version_identity(
            h["declared_capabilities"], h["participant_version"]
        )
        if h["version_identity"] != expected:
            errs.append((
                "SchemaViolation",
                "hosted version_identity != H(declaration ∥ participant_version)",
            ))


def _check_kind_record(n, errs):
    rec = n["semantic"]
    k = rec["k"]
    if k == "AgentProcess":
        if "native" in rec:
            for required in ("control_strategy", "context_policy"):
                if required not in rec["native"]["slots"]:
                    errs.append((
                        "SchemaViolation",
                        f'native process missing slot "{required}"',
                    ))
            for point, owner in rec["native"]["control_boundary"]["assignments"].items():
                if point == "authorize" and owner != "code":
                    errs.append((
                        "SchemaViolation",
                        f"control_boundary: BoundaryError {{ point: Authorize, "
                        f"owner: {owner} }}",
                    ))
        elif "hosted" in rec:
            _check_hosted(rec["hosted"], errs)
    elif k == "Budget":
        for d, bound in rec["dims"].items():
            if d not in _DIMENSION_KEYS:
                errs.append((
                    "SchemaViolation",
                    f'budget dimension "{d}": not in the kernel dimension '
                    "registry (§8.2)",
                ))
            if "hard" in bound and "soft" in bound and bound["soft"] > bound["hard"]:
                errs.append((
                    "SchemaViolation",
                    f'budget dimension "{d}": soft {bound["soft"]} > hard '
                    f'{bound["hard"]}',
                ))
    elif k == "HarnessRule":
        # `assumption_debt` presence is flagged unported at decode; a
        # `conditioned_on` rule with no complete debt refuses.
        if "conditioned_on" in rec:
            errs.append(("ConditionedRuleIncomplete", rec["rule_id"]))
    elif k == "Permission":
        if rec["issuer"]["authority"] == _AUTHORITY["delegate"]:
            errs.append((
                "IllegitimateEndorsement",
                "permission issued at delegate (model-claim) authority",
            ))
        if (
            rec["validity"]["until"] is not None
            and rec["validity"]["condition"] is not None
        ):
            errs.append((
                "SchemaViolation",
                "permission.validity carries both until and condition",
            ))
    elif k == "ToolCapability":
        _check_text_leaf(rec["t"]["purpose"], "tool.purpose", errs)
        errs.extend(_validate_capability(rec["t"], n["provenance"]["authority"]))
        surface = n.get("surface")
        if surface is not None and surface["t"] == "Tool":
            # `authored_exposure` errors surface as validation findings.
            try:
                _authored_exposure(surface["s"]["exposure_mode"])
            except _HirFail as e:
                errs.append((e.kind, e.detail))


def _collect_refs(rec, out):
    """The doc-level ref sites the corpus's ref-resolution leg reads —
    {path, allowed-kinds, r} rows in the E1 walk order."""

    def push(path, allowed, r):
        out.append({"path": path, "allowed": allowed, "r": r})

    k = rec["k"]
    if k == "Permission":
        push("holder", ["AgentProcess"], rec["holder"])
    elif k == "Budget":
        if "parent" in rec:
            push("parent", ["Budget"], rec["parent"])
        push("accounting", [], rec["accounting"])
    elif k == "AgentProcess":
        if "native" in rec:
            n = rec["native"]
            push("harness_def", ["AgentProcess"], n["harness_def"])
            push("budget", ["Budget"], n["budget"])
            push("permissions", ["Permission"], n["permissions"])
        elif "hosted" in rec:
            h = rec["hosted"]
            for r in h["supplies"]["context"]:
                push("supplies.context", ["ContextItem"], r)
            for r in h["supplies"]["tools"]:
                push("supplies.tools", ["ToolCapability"], r)
            for r in h["supplies"]["procedures"]:
                push("supplies.procedures", ["Procedure"], r)
            push("budget", ["Budget"], h["budget"])
            push("permissions", ["Permission"], h["permissions"])
    elif k == "HarnessRule":
        a = rec["action"]
        if a["v"] == "insert_context_item":
            push("action", ["ContextItem"], a["r"])
        elif a["v"] == "restrict_tool_set":
            for r in a["rs"]:
                push("action", ["ToolCapability"], r)
        elif a["v"] == "require_validator":
            push("action", ["Validator"], a["r"])
    elif k == "ToolCapability":
        for r in rec["t"]["postconditions"]:
            push("postconditions", ["Validator", "Observation"], r)
        if rec["t"]["effects"] != "pure":
            for e in rec["t"]["effects"]["declared"]:
                if e["attributes"] is not None and "pin" in e["attributes"]["reversibility"]:
                    push("reversibility", ["Procedure"], e["attributes"]["reversibility"]["pin"])


def _validate_doc(doc):
    errs = []
    if doc["hir_version"] != "HIR/1":
        return [("DialectUnsupported", doc["hir_version"])]
    index = {}
    for n in doc["nodes"]:
        index[_node_semantic_id(n)] = n

    for n in doc["nodes"]:
        # check_node_shape: kind/record agreement is by construction;
        # `classify_home_identifier` never fails on the 13 registered names;
        # the surface-placement gate lives in `_surface_from_json` at parse.
        if "surface" in n and n["kind"] not in _HAS_SURFACE:
            errs.append(("UnexpressibleSurface", f"surface record on {n['kind']}"))
        if n["version"]["dialect"] != "HIR/1":
            errs.append(("DialectUnsupported", n["version"]["dialect"]))
        _check_member_provenance(n["provenance"], n["version"]["sealed"], "node", errs)
        _check_kind_record(n, errs)
        _check_ext(n["ext"], errs)
        _check_model_identity_statics(n, errs)

    # check_refs_and_endpoints (the corpus's ref-resolution leg; edge
    # endpoints and acyclicity are unreachable — edge-bearing cases are
    # flagged unported).
    for n in doc["nodes"]:
        refs = []
        _collect_refs(n["semantic"], refs)
        for site in refs:
            r = site["r"]
            target = index.get(r["sid"])
            if target is None:
                errs.append((
                    "UnresolvedRef",
                    f"{n['kind']}.{site['path']} -> {r['sid']} does not "
                    "resolve in the document",
                ))
            elif site["allowed"] and target["kind"] not in site["allowed"]:
                errs.append((
                    "UnresolvedRef",
                    f"{n['kind']}.{site['path']} -> {r['sid']} "
                    f"(kind {target['kind']} not in allowed)",
                ))

    # check_capability_procedure_sources: a procedure-sourced capability
    # whose `source.ref` resolves to a non-Procedure mismatches.
    for n in doc["nodes"]:
        if n["semantic"]["k"] != "ToolCapability":
            continue
        t = n["semantic"]["t"]
        if _hget(t["source"], "kind") != "procedure":
            continue
        target_sid = _hopt_str(t["source"], "ref")
        target = index.get(target_sid) if target_sid is not None else None
        if target is None:
            continue
        if target["semantic"]["k"] != "Procedure":
            errs.append((
                "DerivedEffectsMismatch",
                f"{_node_semantic_id(n)}: source.ref does not resolve to a "
                "Procedure",
            ))
            continue
        # A resolved Procedure is corpus-unported — flagged at decode.
        raise _HirUnsupported("procedure-derived tool effects")

    # check_discovery_capability: a deferred/indexed-admitting tool surface
    # requires a discovery capability in the document. A malformed
    # exposure_mode counts as not-admitting here (its error is already
    # recorded by check_kind_record above).
    admits_deferred = False
    for n in doc["nodes"]:
        if n["semantic"]["k"] != "ToolCapability":
            continue
        surface = n.get("surface")
        if surface is None or surface["t"] != "Tool":
            continue
        try:
            a = _authored_exposure(surface["s"]["exposure_mode"])
        except _HirFail:
            continue
        if "deferred" in a["admitted"] or "indexed" in a["admitted"]:
            admits_deferred = True
            break
    if admits_deferred:
        has_discovery = any(
            n["semantic"]["k"] == "ToolCapability"
            and _is_discovery_capability(n["semantic"]["t"])
            for n in doc["nodes"]
        )
        if not has_discovery:
            errs.append(("NoDiscoverySurface", ""))

    # check_budgets: a budget parent must contain every declared bound.
    for n in doc["nodes"]:
        if n["semantic"]["k"] != "Budget" or "parent" not in n["semantic"]:
            continue
        pn = index.get(n["semantic"]["parent"]["sid"])
        if pn is None or pn["semantic"]["k"] != "Budget":
            continue
        for dim, bound in n["semantic"]["dims"].items():
            pb = pn["semantic"]["dims"].get(dim)
            if pb is None:
                continue

            def le(a, b):
                return a is None or (b is not None and a <= b)

            if not le(bound.get("hard"), pb.get("hard")) or not le(
                bound.get("soft"), pb.get("soft")
            ):
                errs.append((
                    "BudgetExceedsParent",
                    f'budget dimension "{dim}": child exceeds parent',
                ))

    # check_root.
    root_node = index.get(doc["root"]["sid"])
    if root_node is None:
        errs.append((
            "UnresolvedRef",
            f"root {doc['root']['sid']} does not resolve",
        ))
    elif root_node["kind"] != "AgentProcess":
        errs.append((
            "UnresolvedRef",
            f"root must be AgentProcess, got {root_node['kind']}",
        ))

    return None if not errs else errs


# `seal` — endorse → pin → identify → re-validate (ops.rs).

def _endorse_member(prov, subject_hash, what, sealed_at):
    if prov["scope"] != "definition" or prov["authority"] >= _AUTHORITY["definition"]:
        return None
    if prov["taint"]:
        return ("TaintedAboveExternal", f"{what}: {_AUTHORITY_NAME[prov['authority']]}")
    if prov["origin"]["kind"] in ("import", "migration"):
        # seal basis refuses imported/migrated content — a prior pin
        # endorsement covers it instead.
        att = prov["attestation"]
        pinned = (
            att is not None and _att_self_consistent(att) and att["kind"] == "pin"
        )
        if pinned:
            prov["authority"] = _AUTHORITY["definition"]
            return None
        return (
            "IllegitimateEndorsement",
            f"{what}: import/migration member requires a pin endorsement "
            "before seal",
        )
    prov["authority"] = _AUTHORITY["definition"]
    prov["attestation"] = {
        "kind": "seal",
        "subject_hash": subject_hash,
        "anchor": {"chain": "hir:seal"},
        "verified_by": "hir:seal",
        "verified_at": sealed_at,
    }
    return None


def _endorse_text(t, what, sealed_at):
    e = _endorse_member(t["provenance"], t["content_hash"], what, sealed_at)
    if e is not None:
        return e
    if (
        t["provenance"]["authority"] == _AUTHORITY["definition"]
        and t["authority"] < _AUTHORITY["definition"]
    ):
        t["authority"] = _AUTHORITY["definition"]
    return None


def _endorse_leaves(rec, sealed_at):
    if rec["k"] == "ToolCapability":
        return _endorse_text(rec["t"]["purpose"], "tool.purpose", sealed_at)
    return None


def _endorse_surface(s, sealed_at):
    if s is not None and s["t"] == "Tool":
        return _endorse_text(
            s["s"]["description_template"], "surface.description_template", sealed_at
        )
    return None


def _endorse_node(n, sealed_at):
    basis = _version_id(n)
    e = _endorse_member(n["provenance"], basis, "node", sealed_at)
    if e is not None:
        return e
    e = _endorse_leaves(n["semantic"], sealed_at)
    if e is not None:
        return e
    return _endorse_surface(n.get("surface"), sealed_at)


def _each_ref_mut(rec, f):
    """`each_ref_mut` over the ported kinds — every ref the corpus can carry."""
    k = rec["k"]
    if k == "Permission":
        f(rec["holder"])
    elif k == "Budget":
        if "parent" in rec:
            f(rec["parent"])
        f(rec["accounting"])
    elif k == "AgentProcess":
        if "native" in rec:
            n = rec["native"]
            f(n["harness_def"])
            f(n["budget"])
            f(n["permissions"])
        elif "hosted" in rec:
            h = rec["hosted"]
            for r in h["supplies"]["context"]:
                f(r)
            for r in h["supplies"]["tools"]:
                f(r)
            for r in h["supplies"]["procedures"]:
                f(r)
            f(h["budget"])
            f(h["permissions"])
    elif k == "HarnessRule":
        a = rec["action"]
        if a["v"] in ("insert_context_item", "require_validator"):
            f(a["r"])
        elif a["v"] == "restrict_tool_set":
            for r in a["rs"]:
                f(r)
    elif k == "ToolCapability":
        for r in rec["t"]["postconditions"]:
            f(r)
        if rec["t"]["effects"] != "pure":
            for e in rec["t"]["effects"]["declared"]:
                if e["attributes"] is not None and "pin" in e["attributes"]["reversibility"]:
                    f(e["attributes"]["reversibility"]["pin"])


def _check_profile_pins(rec, errs):
    def check(p, what):
        unbound_root_profile = (
            what == "native.profile" and not p["pinned"] and p["profile"] == "unbound"
        )
        if not p["pinned"] and not unbound_root_profile:
            errs.append((
                "UnresolvedRef",
                f"{what}: unpinned ProfileRef in a sealed form",
            ))

    if rec["k"] == "AgentProcess" and "native" in rec:
        check(rec["native"]["profile"], "native.profile")
    if rec["k"] == "HarnessRule" and "conditioned_on" in rec:
        check(rec["conditioned_on"], "rule.conditioned_on")


def _unresolved_in_assembly(j, path):
    if isinstance(j, dict):
        if _hhas(j, "version_selector"):
            return f"{path}: version_selector"
        for k, v in j.items():
            p = _unresolved_in_assembly(v, f"{path}/{k}")
            if p is not None:
                return p
        return None
    if isinstance(j, list):
        for i, v in enumerate(j):
            p = _unresolved_in_assembly(v, f"{path}/{i}")
            if p is not None:
                return p
        return None
    if isinstance(j, str) and (j.startswith("$param:") or j.startswith("$entity:")):
        return f"{path}: {j}"
    return None


def _unpinned_extension_in_assembly(j, path):
    if not isinstance(j, dict):
        return None
    refs = _hget(_hget(j, "extensions"), "refs")
    if not isinstance(refs, list):
        return None
    for i, r in enumerate(refs):
        if not isinstance(r, dict):
            continue
        rp = f"{path}/extensions/refs/{i}"
        nm = _hopt_str(r, "name")
        name = nm if nm is not None else "<unnamed>"
        loc = _hget(r, "locator")
        if isinstance(loc, dict):
            if _hhas(loc, "selector"):
                return f"{rp} ({name}): locator.selector survived into a sealed form"
            if not _hhas(loc, "resolved") or not _hhas(loc, "fetched_at"):
                return f"{rp} ({name}): locator lacks its resolved/fetched_at pin"
        if not _hhas(r, "content"):
            return f"{rp} ({name}): missing the content pin"
    return None


def _seal(doc, sealed_at):
    """`seal(doc, sealed_at)` — returns `(errs, sealed_doc, def_version_id)`;
    a non-empty `errs` is the refusal, `sealed_doc`/`def_version_id` null."""
    if "assembly" in doc:
        u = _unresolved_in_assembly(doc["assembly"], "assembly")
        if u is not None:
            return ([(
                "UnresolvedRef",
                "a document carrying an unresolved assembly section is not "
                f"sealable (resolve first): {u}",
            )], None, None)
        up = _unpinned_extension_in_assembly(doc["assembly"], "assembly")
        if up is not None:
            return ([("UnpinnedInSealedForm", up)], None, None)
    pre = _validate_doc(doc)
    if pre is not None:
        return (pre, None, None)
    errs = []
    for n in doc["nodes"]:
        if n["semantic"]["k"] == "ToolCapability":
            if (
                _capability_needs_flow_contract(n["semantic"]["t"])
                and "flow_contract" not in n["semantic"]["t"]
            ):
                errs.append(("ContributionUndeclared", _node_semantic_id(n)))
    if errs:
        return (errs, None, None)

    sealed = copy.deepcopy(doc)
    sids = {}
    for i, n in enumerate(sealed["nodes"]):
        sid = _node_semantic_id(n)
        n["version"]["semantic_id"] = sid
        sids[sid] = i
    for n in sealed["nodes"]:
        e = _endorse_node(n, sealed_at)
        if e is not None:
            errs.append(e)
        n["version"]["sealed"] = True
    # edges are flagged unported — unreachable in an answered case.
    if errs:
        return (errs, None, None)

    pinned_vid = [""] * len(sealed["nodes"])
    for i, n in enumerate(sealed["nodes"]):
        n["version"]["version_id"] = _version_id(n)
        pinned_vid[i] = n["version"]["version_id"]
    for n in sealed["nodes"]:
        pin_err = []

        def pin(r):
            if pin_err:
                return
            t = sids.get(r["sid"])
            if t is None:
                pin_err.append((
                    "UnresolvedRef",
                    f"seal: ref {r['sid']} unresolved",
                ))
            else:
                r["pin"] = pinned_vid[t]
                r["sel"] = None

        _each_ref_mut(n["semantic"], pin)
        errs.extend(pin_err)
    for n in sealed["nodes"]:
        _check_profile_pins(n["semantic"], errs)
    rt = sids.get(sealed["root"]["sid"])
    if rt is None:
        errs.append(("UnresolvedRef", "seal: root unresolved"))
    else:
        sealed["root"]["pin"] = pinned_vid[rt]
        sealed["root"]["sel"] = None
    for n in sealed["nodes"]:
        if n["semantic"]["k"] == "AgentProcess" and "hosted" in n["semantic"]:
            h = n["semantic"]["hosted"]
            h["version_identity"] = _opaque_version_identity(
                h["declared_capabilities"], h["participant_version"]
            )
    if errs:
        return (errs, None, None)

    post = _validate_doc(sealed)
    if post is not None:
        return (post, None, None)

    root_node = next(
        (n for n in sealed["nodes"] if _node_semantic_id(n) == sealed["root"]["sid"]),
        None,
    )
    return (
        [],
        sealed,
        "" if root_node is None else root_node["version"]["version_id"],
    )


# The `hir-documents` arm — one `replay` per `<name>.input.json`,
# `sealed_at` from `scenario.json`.

def _hir_kind_join(errs):
    """`<K1>,<K2>` — first-raised order, duplicates collapsed."""
    seen = []
    for kind, _detail in errs:
        if kind not in seen:
            seen.append(kind)
    return ",".join(seen)


def _hir_pipeline_answer(parse_v, validate_v, seal_v, canonical_sha, sealed_sha, def_version):
    return {
        "canonical_sha256": canonical_sha,
        "definition_version_id": def_version,
        "parse": parse_v,
        "seal": seal_v,
        "sealed_sha256": sealed_sha,
        "validate": validate_v,
    }


def _hir_replay(data: bytes, sealed_at: int) -> dict:
    """`hir_replay::replay` — the six-member pipeline answer for one case."""
    parse_v = "ok"
    validate_v = "skipped"
    seal_v = "skipped"
    canonical_sha = None
    sealed_sha = None
    def_version = None

    try:
        doc_json = parse_json(data)
        if canonical(doc_json) != data:
            raise _HirFail(
                "NonCanonicalInput",
                "document: input is not in canonical form (whitespace, "
                "member order or duplicate keys)",
            )
        doc = _document_from_json(doc_json)
    except _HirFail as e:
        return _hir_pipeline_answer(
            f"refuse:{e.kind}", validate_v, seal_v,
            canonical_sha, sealed_sha, def_version,
        )
    except Exception:
        # A malformed encoding (bad UTF-8, a JSON error, a float or an
        # unpaired surrogate) is a parse refusal, never a crash.
        return _hir_pipeline_answer(
            "refuse:NonCanonicalInput", validate_v, seal_v,
            canonical_sha, sealed_sha, def_version,
        )

    # Honest bound: corpus-unreached machinery yields no answer rather than
    # a fabricated verdict (the cell reports `missing answer`).
    if doc["unported"]:
        raise _HirUnsupported(doc["unported"][0])

    canonical_sha = sha256_hex(canonical(_doc_to_json(doc)))
    v = _validate_doc(doc)
    if v is not None:
        validate_v = f"invalid:{_hir_kind_join(v)}"
    else:
        validate_v = "valid"
        errs, sealed_doc, def_vid = _seal(doc, sealed_at)
        if errs:
            seal_v = f"refuse:{_hir_kind_join(errs)}"
        else:
            seal_v = "sealed"
            sealed_sha = sha256_hex(canonical(_doc_to_json(sealed_doc)))
            def_version = def_vid
    return _hir_pipeline_answer(
        parse_v, validate_v, seal_v, canonical_sha, sealed_sha, def_version
    )


def arm_hir_documents(arm_dir: Path) -> dict:
    scenario = parse_json((arm_dir / "scenario.json").read_bytes())
    sealed_at = scenario.get("sealed_at")
    if not isinstance(sealed_at, int) or isinstance(sealed_at, bool):
        raise ValueError("hir-documents scenario.json: sealed_at must be an integer")
    answers = {}
    for p in sorted(arm_dir.glob("*.input.json")):
        case = p.name[: -len(".input.json")]
        try:
            answers[case] = _hir_replay(p.read_bytes(), sealed_at)
        except _HirUnsupported:
            # The honest bound: a case needing machinery outside the ported
            # surface answers nothing — `verify` reports `missing answer`.
            continue
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
            "hir-documents": arm_hir_documents(arms_dir / "hir-documents"),
            "ledger-scenarios": arm_ledger_scenarios(arms_dir / "ledger-scenarios"),
        },
    }
    out = Path(sys.argv[2])
    out.write_bytes(canonical(answers) + b"\n")
    print(f"wrote {out} (candidate E2-candidate, {sum(1 for a in answers['arms'].values() for _ in ([a] if not isinstance(a, dict) else a))} arm answers)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
