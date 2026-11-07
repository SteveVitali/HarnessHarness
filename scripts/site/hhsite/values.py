"""values — hh-site-data/1 value objects, typed absents, canonical JSON.

A displayed value is a provenance-carrying object
``{v, lo, hi, unit, interval{kind, level_ppm}, n{tasks, k},
src{path, sha256, ptr}, label}`` (REQ-XF-121). An absent value is a typed
object — ``not_computed``, ``not_yet_available``, ``refused``,
``undefined`` — never ``null`` or ``0`` (REQ-XF-122). Text copied verbatim
from a packet or the record is a ``{text, src}`` object so every rendered
string can also carry ``data-src``/``data-sha``.

Canonical form (REQ-XF-120): ``json.dumps(..., sort_keys=True,
ensure_ascii=False, separators=(",", ":"))`` plus a trailing newline —
byte-stable across Python versions and operating systems. Numbers in the
emitted layer are integers (micro-units, ppm); a float leaf fails
validation.

Hygiene (REQ-XF-125): the projection copies no local path or host field.
``hygiene_scan`` is applied to every copied string and to the final
emitted document; a hit refuses the build (exit 2).
"""
import re

SCHEMA = "hh-site-data/1"
GENERATOR = "hh-site/1"
GENERATOR_VERSION = "0.1.0"
PROFILE = "internal"

ABSENT_STATES = ("not_computed", "not_yet_available", "refused", "undefined")

# Local/host-specific path patterns refused in the projection (REQ-XF-125).
# `~/` requires a preceding non-letter boundary so "~18GB" prose is safe.
_HYGIENE_RE = re.compile(r"/Users/|/home/|(?:^|[^A-Za-z0-9])~/[A-Za-z0-9._~-]")


class Refusal(Exception):
    """A build-stopping refusal: exit 2, never a fabricated result."""

    def __init__(self, reason):
        super().__init__(reason)
        self.reason = reason


def hygiene_scan(text):
    """Return the offending local-path token, or None."""
    m = _HYGIENE_RE.search(text)
    return m.group(0) if m else None


def check_hygiene(text, where=""):
    """Refuse if the text carries a local path (REQ-XF-125)."""
    hit = hygiene_scan(str(text))
    if hit:
        raise Refusal("hygiene: local/host path %r refused%s"
                      % (hit, (" in " + where) if where else ""))


def src_ref(path, sha256, ptr):
    """Provenance of a copied field: packet path, sha256, JSON pointer."""
    check_hygiene(path, "src.path")
    return {"path": path, "ptr": ptr, "sha256": sha256}


def value(v, unit, src, lo=None, hi=None, interval=None, n=None, label=None):
    """A displayed numeric value. v/lo/hi are integers (micro-units or
    ppm); v may be omitted on an interval-only object; interval/n/label
    are optional members."""
    check_hygiene(unit, "unit")
    out = {"src": src, "unit": unit}
    if v is not None:
        out["v"] = v
    if lo is not None:
        out["lo"] = lo
    if hi is not None:
        out["hi"] = hi
    if interval is not None:
        out["interval"] = interval
    if n is not None:
        out["n"] = n
    if label is not None:
        check_hygiene(label, "label")
        out["label"] = label
    return out


def text(t, src, label=None):
    """A verbatim copied string with provenance (bounds, verdict text,
    refusal displays, arm names)."""
    check_hygiene(t, "copied text")
    out = {"src": src, "text": t}
    if label is not None:
        check_hygiene(label, "label")
        out["label"] = label
    return out


def absent(state, reason=None, awaits=None, gate=None, variant=None):
    """A typed absent value (REQ-XF-122)."""
    if state not in ABSENT_STATES:
        raise Refusal("absent state %r is not one of %s"
                      % (state, ABSENT_STATES))
    out = {"state": state}
    if reason is not None:
        check_hygiene(reason, "absent reason")
        out["reason"] = reason
    if variant is not None:
        check_hygiene(variant, "absent variant")
        out["variant"] = variant
    if awaits is not None:
        check_hygiene(awaits, "absent awaits")
        out["awaits"] = awaits
    if gate is not None:
        check_hygiene(gate, "absent gate")
        out["gate"] = gate
    return out


def derived(derivation, inputs, out):
    """Mark a value produced by a registered derivation: {derived, inputs,
    value}. inputs are the src refs the derivation consumed."""
    return {"derived": derivation, "inputs": inputs, "value": out}


def validate_absent(obj):
    """Schema check for an absent position (AC-9.3-6): the four typed
    states are the only legal inhabitants — null, 0 or a bare string is
    never an absent value."""
    if not isinstance(obj, dict) or "state" not in obj:
        return ["absent position holds %r, not a typed object" % (obj,)]
    if obj["state"] not in ABSENT_STATES:
        return ["absent state %r not in %s" % (obj["state"],
                                             ABSENT_STATES)]
    return []


def validate(obj, path="$"):
    """Schema check over an emitted document (AC-6): no null leaves, no
    float leaves, absent objects carry a registered typed state, and no
    bare null/0 stands in for an absent value."""
    errs = []
    if obj is None:
        errs.append("%s: null leaf (typed absent required)" % path)
        return errs
    if isinstance(obj, float):
        errs.append("%s: float leaf %r (integer units only)" % (path, obj))
    if isinstance(obj, dict):
        if isinstance(obj.get("state"), str) \
                and obj["state"] not in ABSENT_STATES:
            errs.append("%s.state: %r not in %s"
                        % (path, obj["state"], ABSENT_STATES))
        for k, v in obj.items():
            errs.extend(validate(v, "%s/%s" % (path, k)))
    if isinstance(obj, list):
        for i, v in enumerate(obj):
            errs.extend(validate(v, "%s/%d" % (path, i)))
    return errs


def canonical(body):
    """Canonical JSON text: sorted keys, compact separators, trailing
    newline (REQ-XF-120)."""
    import json
    return json.dumps(body, sort_keys=True, ensure_ascii=False,
                      separators=(",", ":")) + "\n"


def envelope(file_name, body, inputs, inputs_digest):
    """The hh-site-data/1 envelope (design/39 F8): schema, generator,
    profile, the packet-set digest and the named inputs this file
    consumed. No wall-clock member."""
    return {
        "body": body,
        "file": file_name,
        "generator": "%s v%s" % (GENERATOR, GENERATOR_VERSION),
        "inputs": inputs,
        "inputs_digest": inputs_digest,
        "profile": PROFILE,
        "schema": SCHEMA,
    }
