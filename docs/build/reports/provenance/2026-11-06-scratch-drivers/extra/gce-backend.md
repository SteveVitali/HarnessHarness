# GCE Podman Backend — S4 Completion Evidence Infra

Temporary evidence infrastructure. **Deprovision when S4 work is done** (teardown ticket tracks this).

## Resources

| Item | Value |
|---|---|
| GCP project | `harnessharness-evidence-8d7a99` |
| Billing account | `018CB8-639CF9-C243C7` ("Rhema") |
| VM | `podman-s4-backend` |
| Zone | `us-central1-a` |
| Machine type | `e2-standard-8` (8 vCPU / 32 GiB), on-demand (not spot) |
| Boot disk | 200 GB `pd-balanced`, debian-12 |
| External IP | 35.193.37.223 |
| Labels | `purpose=s4-completion`, `ttl=deprovision-on-done` |
| Podman | 4.3.1, **rootful**, systemd unit `podman-remote.service` (enabled, restarts on boot/crash), remote socket `/tmp/podman.sock` (mode 0666) |

## Working endpoints (via SSH forward)

- `DOCKER_HOST=tcp://localhost:23750` — works with docker CLI / clients that need TCP
- `DOCKER_HOST=unix:///tmp/podman-gce/podman.sock` — local unix socket forwarded to remote `/tmp/podman.sock`

Verified: `docker pull hello-world` + `docker run --rm hello-world` run on the **remote** podman.

## SSH forward command (keep alive during work)

```bash
mkdir -p /tmp/podman-gce && rm -f /tmp/podman-gce/podman.sock
gcloud compute ssh podman-s4-backend \
  --project=harnessharness-evidence-8d7a99 --zone=us-central1-a \
  --ssh-flag="-N" \
  --ssh-flag="-L /tmp/podman-gce/podman.sock:/tmp/podman.sock" \
  --ssh-flag="-L 23750:/tmp/podman.sock" \
  --ssh-flag="-o ExitOnForwardFailure=yes" \
  --ssh-flag="-o ServerAliveInterval=30"
```

(One session forwards both the unix socket and TCP 23750 → remote `/tmp/podman.sock`.)

## Cost estimate

- e2-standard-8 on-demand, us-central1: ~$0.268/hr
- 200 GB pd-balanced: ~$0.027/hr (~$20/mo)
- **Total ≈ $0.30/hr ≈ $7.20/day** → ~2.7 days inside the ~$20 budget
- Ingress (docker pulls) is free; egress minimal for evidence capture.

## Deprovision commands (verbatim, for teardown ticket)

```bash
# 1. Kill any local ssh forward for /tmp/podman-gce or localhost:23750 first
gcloud compute instances delete podman-s4-backend \
  --project=harnessharness-evidence-8d7a99 --zone=us-central1-a --quiet
gcloud projects delete harnessharness-evidence-8d7a99 --quiet
# Verify nothing remains:
gcloud compute instances list --project=harnessharness-evidence-8d7a99 2>&1 | tail -3
```

Project deletion is a soft-delete (30-day hold) — deleting the VM first stops all billing.

## Notes

- SSH access is via `gcloud compute ssh` (OS Login / metadata keys — standard).
- The remote socket is rootful podman → supports privileged containers, real overlay storage.
- VM reboot-safe: `podman-remote.service` is `enabled` + `Restart=always`.
- No credentials stored anywhere; teardown tracked by ticket.
