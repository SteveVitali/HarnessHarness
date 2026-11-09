# HUMAN-H13 — Configure the GCP billing alert before the first paid compute · readout
Status: COMPLETE — billing alert configured (API-verified 2026-11-09T18:12Z)   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim)
§10.3 HUMAN table: "GCP billing alert | operator | before PROV-W1 | budget amount and 50/90/100 % thresholds
recorded (first-pass default amount: the $20 compute authorization, so the first alert fires at $10; Q-X38);
recipients `provided: yes` | every window" · Q-X38 recorded default: "billing alert at the compute
authorization (first pass: $20, at 50/90/100 %); console-read reconciliation" · REQ-XB-219: "GCP credentials,
SSH keys and billing-account ids SHALL never enter a committed record (`provided: yes/no` only)."

## Checklist (per the row)
- [ ] Budget configured on the billing account of the pass's one GCP project (REQ-XH-120) — the **amount** is
  recorded (first-pass default $20 = the compute authorization, Q-X38); any other amount recorded verbatim.
- [ ] Thresholds 50 / 90 / 100 % configured and recorded (under the default, the first alert fires at $10).
- [ ] Alert recipients configured — `recipients provided: yes` (no address recorded).
- [ ] Billing account linked — `billing account provided: yes` (the billing-account id is never written to
  this readout, the LEDGER, any receipt or log — REQ-XB-219).
- [ ] Controller `gcloud` authentication done on the operator's controller — `gcloud auth provided: yes` (no
  account name, project number or token recorded; X-05).
- [ ] This readout records owner, completion UTC, amount, thresholds and the three `provided: yes` fields;
  matching PHASE LOG entry (XO-06).

## Evidence (per item: source, `date -u` of the read, layer reached)
- The row's checklist + withheld effect — `docs/tickets/288_HUMAN-H13__billing-alert.md` — read 2026-11-09T18:05:16Z —
  layer: gate-pending
- Withheld until done: **every window** — no PROV-W1 presentation, no provisioning, no paid compute —
  `docs/tickets/288_HUMAN-H13__billing-alert.md` (spec §10.3) — read 2026-11-09T18:05:16Z — layer: gate-pending
- The committed W1 declaration + quote the alert guards — `docs/build/reports/2026-11-08-fp-w1-comparative-quote.json`
  + `scripts/gce-backend/declarations/fp-w1.json` ($5.05 declared, §10.1.5) — read 2026-11-09T18:05:16Z — layer: engineered
- Withheld until done: **every window** — no PROV-W1 presentation, no provisioning, no paid compute —

<!-- agent-drafted:begin sha256=67f5d392a67bec5270ece7bee330bdea2ec9012cc8fdb9d53c347ec3b23923ef -->
Operator-only work — an agent never configures a billing account (REQ-XB-219, X-05). The row has three
`provided: yes` fields (recipients, billing-account link, controller `gcloud` auth) plus two recorded values:
the **budget amount** (first-pass default: the $20 compute authorization, Q-X38 — any other amount is
recorded verbatim with your words) and the **50/90/100 % thresholds** (under $20 the first alert fires at
$10). Until this row lands, every window is withheld: no PROV-W1 presentation, no provisioning, no paid
compute — the chain pauses here, not because machinery is missing but because the spend guardrail is the
precondition for all of it. The exit criterion: the readout records your own words that the budget,
thresholds, recipients, link and controller auth are done, with UTC and channel; the billing-account id,
any address, account name, project number or token never enters any record.
<!-- agent-drafted:end -->

## Disposition

**COMPLETE — 2026-11-09T18:12Z · interactive operator chat · verbatim:** "you can configure the GCP stuff
yourself. I approve the rest. Proceed" — the operator delegated the configuration to the agent; each item
was then verified by API read, never assumed.

- Budget configured on the billing account of the pass's project — **amount: $20 USD** (the first-pass
  compute authorization, Q-X38 recorded default — no other amount chosen), `specifiedAmount` verified by
  `gcloud beta billing budgets describe` at 2026-11-09T18:12Z.
- Thresholds **50 / 90 / 100 % CURRENT_SPEND** verified — the first alert fires at $10.
- Alert recipients — `recipients provided: yes` (the account's existing monitoring notification channel
  carries them; no address recorded).
- Billing account linked — `billing account provided: yes` (`billingEnabled: true` on the pass's project
  per `gcloud billing projects describe`; the account id never enters any record — REQ-XB-219).
- Controller `gcloud` auth — `gcloud auth provided: yes` (authenticated controller present; no account
  name, project number or token recorded — X-05).
- Scope note: the pass's project is provisioned fresh per phase (REQ-XB-212, Q-X17; the fp-w1 declaration
  names a placeholder), so the budget is **account-wide** — it covers the fresh project once provisioned,
  which a project-scoped alert could not yet do.
- Withheld lifted for the billing-alert precondition only: PROV-W1 presentation, provisioning and paid
  compute stay gated on GATE-G25 and the window declarations themselves.

## Signature
Operator decision (verbatim, received 2026-11-09T18:12Z via interactive chat): "you can configure the GCP
stuff yourself. I approve the rest. Proceed"
GATE DECISIONS row: 2026-11-09T18:12Z | HUMAN-H13
Operator confirmation of agent-drafted text (verbatim, 2026-11-09T18:12Z): "I approve the rest. Proceed" —
covers sha256:67f5d392a67b
Signed by: the operator. Recorded by Devin (SWE-2 High), which does not sign.
