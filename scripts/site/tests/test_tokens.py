#!/usr/bin/env python3
"""test_tokens.py — the committed WCAG ratio test (REQ-XF-102, AC-9.1-2).

Parses assets/tokens.css, computes every ratio with the WCAG 2 formula
(hhsite/contrast.py — reimplemented, not copied from the design scratch
script): every text and status token ≥ 4.5:1 and every meaningful
non-text token ≥ 3:1, against panel and paper, in both themes.

  python3 test_tokens.py            check + print the ratio table (exit 1 on a fail)
  python3 test_tokens.py --selftest mutation (m): lower one token below
                                    4.5:1 and assert the check fails
  python3 test_tokens.py --mutate token=#rrggbb
                                    run the check against a mutated token
                                    set (exits 1 — the committed mutation leg)
"""
import sys
from pathlib import Path

SITE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SITE))

from hhsite import contrast, tokparse  # noqa: E402

TEXT_STATUS = ["ink", "ink-2", "ink-3", "link",
               "st-verified", "st-inconclusive", "st-withheld",
               "st-failed", "st-infra"]
NONTEXT = (["axis", "floor-edge", "needle"] +
           ["series-%d" % i for i in range(1, 9)])
FLOORS = {"text/status": 4.5, "non-text": 3.0}


def ratio_table(tokens, mutate=None):
    """[(theme, token, kind, hex, panel_ratio, paper_ratio, floor, ok)] —
    floor is 'text/status' (4.5) or 'non-text' (3.0)."""
    rows = []
    for theme in ("light", "dark"):
        t = dict(tokens[theme])
        if mutate:
            name, value = mutate
            if name in t:
                t[name] = value
        for name in TEXT_STATUS + NONTEXT:
            kind = "text/status" if name in TEXT_STATUS else "non-text"
            rp = contrast.ratio(t[name], t["panel"])
            rq = contrast.ratio(t[name], t["paper"])
            floor = FLOORS[kind]
            rows.append((theme, name, kind, t[name], rp, rq, floor,
                         rp >= floor and rq >= floor))
    return rows


def main(argv):
    css = (SITE / "assets" / "tokens.css").read_text()
    tokens = tokparse.parse(css)
    mutate = None
    if "--selftest" in argv:
        # mutation (m): light --needle dropped below the non-text floor
        mut_rows = ratio_table(tokens, mutate=("needle", "#E8B090"))
        caught = [r for r in mut_rows if not r[7]]
        if not caught:
            print("test_tokens: FAIL — selftest mutation not caught")
            return 1
        print("test_tokens: selftest — mutation (m) caught "
              "(%d pair(s) below floor)" % len(caught))
        return 0
    if "--mutate" in argv:
        i = argv.index("--mutate")
        name, _, value = argv[i + 1].partition("=")
        mutate = (name, value)
    rows = ratio_table(tokens, mutate=mutate)
    fails = [r for r in rows if not r[7]]
    for theme, name, kind, hx, rp, rq, floor, ok in rows:
        print("  %-5s %-15s %-11s %s  panel %5.2f  paper %5.2f  ≥%.1f  %s"
              % (theme, name, kind, hx, rp, rq, floor,
                 "ok" if ok else "FAIL"))
    if fails:
        print("test_tokens: FAIL — %d pair(s) below floor" % len(fails))
        return 1
    print("test_tokens: OK — %d token×surface pairs, all above floor"
          % len(rows))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
