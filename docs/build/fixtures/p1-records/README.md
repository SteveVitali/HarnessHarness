# fixtures/p1-records — negative battery for `hh-p1-records-check/1`

`run-fixtures.sh` copies the verifier's real committed inputs into a mktemp
sandbox per case, applies `cases/<name>/patch.sh`, runs
`python3 -I docs/build/tools/p1_records.py --check` (or `--only RC-N`), and
requires `expected.txt`'s outcome: an `RC-N` token means that check must fail
(exit 1); `missing-input` means exit 2. The verifier is read-only over build
state; mutations never touch the committed tree.

Cases: `mutated-archive-byte` (RC-1), `deleted-raw-slot`,
`altered-packet-slot`, `altered-raw-slot-record` (RC-3),
`histogram-broken` (RC-4), `missing-input` (exit 2).
