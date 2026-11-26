p4-p4x attempt-3 harvest (R7.3 G-L1 gen-2/gen-3 + X0 live window; deleted 2026-11-23T15:31:19Z).
run-root.tar.gz = X0 run-root (hh-fp-window/1 p4x0-window.json: 24 slots recorded,
withheld 0, V8/V9 withheld{inert_at_admissible_window}); MANIFEST.sha256 covers it.
l1-evidence-gen3.tgz = lean operative-evidence bundle pulled pre-teardown:
fold/ (gen-2: result=fail) + fold-g3/ (gen-3: result=pass, pass_id
l1-pass:sha256:5b807c53…), _l1-summary.json (gen-2) + fold-g3/_l1-summary.json
(gen-3), hset/ + pack/ + instructions/, bound HIR, l1-fold-spec-lite.json
(request bodies replaced by sha256s; full-spec digest in l1-fold-spec.sha256),
l10-durable-stores.txt, per-leg drive.json + drive-stdout.txt, gl1-gen2.log +
gl1-gen3.log, gate-a.json, admission.json, layers.json, l1-coverage.json.
serial.log + journald.log captured pre-delete this attempt (unlike attempts 1-2).
No X1 run-root exists — X1 was never launched (see run record).
usage.txt = in-guest usage snapshot from the recipe harvest.
Full retained mirror: ~/hh-retained/p4x/attempt3/ (MANIFEST.sha256).
