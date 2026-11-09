#!/usr/bin/env python3
"""test_fp_seal.py — R6.30 Stage-1 site leg (manifest 284; REQ-XH-121).

The internal-profile site fixture renders the sealed state: the
declared ``phase2-fp-sealed.json`` presentation fixture (fixture:true,
``hh-comparative-results/1``) carries every first-pass outcome member
as ``withheld{sealed_until_results}`` with cost and declared fields
only, and drops outcome-derived values (thetas, Holm family members,
$/solve, flip rates) a sealed readout cannot show. The leg loads it
through R6.5's own ``packets``/``project``/``sitepages`` path — the
same fp pages, never a second site path — and asserts:

  * the committed sealed fixture recomputes byte-identically from the
    unsealed fixture under the published masking rule (the same member
    set the Rust ``fp_seal`` module seals — one firewall, two doors);
  * no unmasked outcome member survives in the packet;
  * the fp tasks grid projects the withheld pip spellings and renders
    on the internal profile — the sealed state the site's internal
    profile carries (REQ-XH-121);
  * outcome-derived projections refuse typed rather than fabricate a
    leaderboard from absent thetas — the seal is mechanical, not a
    layout.

Exit 0 green, 1 a leg failed.
"""
import json
import sys
from pathlib import Path

SITE = Path(__file__).resolve().parent.parent
ROOT = SITE.parent.parent
sys.path.insert(0, str(SITE))

from hhsite import packets, project, sitepages, values  # noqa: E402

MARKER = "withheld{sealed_until_results}"
# Outcome members proper — masked wherever they appear (the same set
# fp_seal::is_outcome_member seals).
OUTCOME = ("verdict", "reward", "reward_ppm", "reward_raw", "ctrf",
           "ctrf_counts", "verifier", "verifier_output",
           "verifier_output_bytes", "success", "classification",
           "score", "outcome")
# Outcome-derived values — dropped, not masked: thetas, Holm family
# members, $/solve and flip rates are derivatives of outcome fields;
# a sealed packet does not carry them.
DERIVED = ("theta", "family", "exploratory_family", "per_solve",
           "flip", "pass5", "passk", "solve_rate", "delta")

fails = 0


def ok(n, m=""):
    print("test_fp_seal: ok [%s]%s" % (n, (" " + m) if m else ""))


def fail(n, m):
    global fails
    fails += 1
    print("test_fp_seal: FAIL [%s] %s" % (n, m), file=sys.stderr)


def _outcome_key(k):
    return (k in OUTCOME or k.startswith("reward")
            or k.startswith("verifier") or "outcome" in k)


def _drop_key(k):
    return k in DERIVED or k.startswith("theta") or k.startswith("flip")


def _seal(node):
    """The published masking rule — recompute check of the committed
    sealed fixture (mirrors fp_seal::seal_node + the dev-projection
    drop of outcome-derived values)."""
    if isinstance(node, dict):
        out = {}
        for k, v in node.items():
            if _outcome_key(k):
                out[k] = MARKER
            elif _drop_key(k):
                continue
            elif k == "pips" and isinstance(v, list):
                out[k] = ["withheld"
                          if p in ("resolved", "unresolved") else p
                          for p in v]
            else:
                out[k] = _seal(v)
        return out
    if isinstance(node, list):
        return [_seal(v) for v in node]
    if isinstance(node, str) and node.startswith("executed{"):
        return MARKER
    return node


def _leaks(node, path=""):
    found = []
    if isinstance(node, dict):
        for k, v in node.items():
            p = "%s.%s" % (path, k)
            if _outcome_key(k) and v != MARKER:
                found.append(p)
            else:
                found += _leaks(v, p)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            found += _leaks(v, "%s[%d]" % (path, i))
    elif isinstance(node, str) and node.startswith("executed{"):
        found.append(path)
    return found


def main():
    committed = json.loads(
        (SITE / "fixtures" / "phase2-fp-results.json").read_text())
    sealed_path = SITE / "fixtures" / "phase2-fp-sealed.json"
    sealed = json.loads(sealed_path.read_text())

    # Leg 1 — the committed sealed fixture recomputes byte-identically
    # from the unsealed fixture under the masking rule.
    expected = _seal(committed)
    expected["fixture_label"] = sealed["fixture_label"]
    if sealed != expected:
        fail("recompute", "sealed fixture does not recompute from the "
             "masking rule")
    else:
        ok("recompute", "sealed fixture = mask(unsealed fixture)")

    # Leg 2 — the sealed fixture carries no unmasked outcome member.
    found = _leaks(sealed)
    if found:
        fail("sealed-fields", "unmasked outcome members: %s" % found[:5])
    else:
        ok("sealed-fields", "every outcome member is the sealed marker")

    # Leg 3 — through the generator: the sealed packet projects the
    # withheld pip grid and the fp tasks page renders it on the
    # internal profile.
    fixture_inputs = json.loads(
        (SITE / "config" / "inputs.json").read_text())["inputs"]
    fp_entry = [e for e in fixture_inputs if e["id"] == "p2-fp"][0]
    sealed_entry = dict(fp_entry)
    sealed_entry["path"] = "scripts/site/fixtures/phase2-fp-sealed.json"
    inp = packets.Input(sealed_entry, ROOT)
    tasks = project._grid_tasks(inp)
    pips = [p["text"] for t in tasks for c in t["cells"]
            for p in c["pips"]]
    # infra pips stay — infrastructure members are not outcome fields
    # (REQ-XH-121's carve-out); resolved/unresolved are masked.
    if not pips or any(p not in ("withheld", "infra") for p in pips):
        fail("grid", "pip grid projects an outcome pip: %s"
             % sorted(set(pips)))
    else:
        ok("grid", "every outcome pip withheld; infra pips readable")

    claims_body = json.loads(
        (ROOT / "docs" / "results" / "site-data" / "claims.json")
        .read_text())["body"]
    sd = {
        "claims.json": json.dumps({"body": claims_body}),
        "tfp.json": json.dumps({"body": {"tasks": tasks}}),
    }
    pkts = {
        "p2-fp": {"path": inp.path, "schema": inp.schema,
                  "sha256": inp.sha256},
        "_digest": "sha256:sealed-test",
        "inputs": [inp.path],
    }
    html = sitepages.fp_tasks_page(sd, pkts, "sealedtest")
    if "pip p-withheld" not in html:
        fail("render", "fp tasks page lacks the withheld pip class")
    elif "executed{" in html or 'replicate resolved' in html \
            or "pip p-resolved" in html:
        fail("render", "an outcome value leaked into the render")
    else:
        ok("render", "the fp tasks page renders the sealed state")

    # Leg 4 — outcome-derived projections refuse typed: a leaderboard
    # needs thetas the sealed packet dropped; the projection raises the
    # typed Refusal rather than fabricate one.
    try:
        project.tfp([inp])
        fail("leaderboard", "tfp projected a leaderboard from a "
             "sealed packet")
    except values.Refusal:
        ok("leaderboard", "tfp refuses typed on a sealed packet")
    except Exception as e:  # noqa: BLE001
        fail("leaderboard", "non-typed failure: %r" % e)

    if fails:
        print("test_fp_seal: %d leg(s) failed" % fails, file=sys.stderr)
        sys.exit(1)
    print("test_fp_seal: all legs green")


if __name__ == "__main__":
    main()
