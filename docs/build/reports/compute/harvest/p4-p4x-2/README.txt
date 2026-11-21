p4-p4x rerun-4 harvest (R7.3 G-L1 attempt 2, fold recorded_at 2026-11-21T01:35:00Z).
l1-evidence.tgz = lean operative-evidence bundle pulled pre-teardown:
fold/ (hh-l1-conformance.json result=fail + 50 hh-l1-diff/1 + _l1-summary),
hset/ (sealed baseline doc + h_set.json binding sha256:88042f…), pack/,
instructions/, bound HIR, l1-fold-spec-lite.json (request bodies replaced by
sha256s; full-spec digest in l1-fold-spec.sha256), per-leg drive.json +
drive-stdout.txt + events.json + events.wal, gl1-rerun4.log, gate-a.json,
admission.json, layers.json, l1-coverage.json.
serial.log / journald.log NOT captured this attempt (VM deleted before serial
pull — recorded honestly in the teardown receipt).
Full retained mirror: ~/hh-retained/p4x/gl1-rerun4/ (MANIFEST.sha256).
