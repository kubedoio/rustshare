# Getting Started with RustShare

Get RustShare running locally in under 5 minutes. This guide assumes you know Docker.

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) and Docker Compose
- 4 GB RAM available
- Ports 80 and 443 free on your host
- A domain name (optional for local testing)

## Quick Start

```bash
git clone https://github.com/kubedoio/rustshare.git
cd rustshare
cp .env.example .env
./scripts/pre-flight.sh
docker compose up -d
```

Open http://localhost and sign in:

> **Admin credentials** come from your `.env` file. If you ran `scripts/pre-flight.sh`, passwords were auto-generated. Retrieve them from the container logs:
> ```bash
> docker logs rustshare-backend-1 | grep "Bootstrap admin password"
> ```
> Or check the values of `RUSTSHARE_ADMIN_PASSWORD` and `RUSTSHARE_DEMO_VIEWER_PASSWORD` in your `.env` file.

## What Just Happened

Four containers started:

| Container | Purpose | Port |
|-----------|---------|------|
| `postgres` | Metadata and user database | 5432 |
| `rustfs` | S3-compatible object storage | 9000 / 9001 |
| `backend` | Rust API + compiled SvelteKit frontend | 8080 |
| `nginx` | Reverse proxy | 80 |

The backend image is built in one multi-stage Dockerfile: Node builds the SPA, Rust compiles the server, and the runtime image serves both.

## First-Time Setup

1. **Log in as admin** using the credentials above.
2. **Verify health**:
   ```bash
   ./scripts/final-launch-smoke.sh
   ```
   All checks should pass.
3. **(Optional) Configure a storage bucket** in the RustFS console at http://localhost:9001 (credentials are in `.env`).

## Production Setup

For production, use the production override to enable restart policies, resource limits, log rotation, and hardened port binding:

```bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

Key differences from the default stack:
- Containers restart automatically on failure
- CPU/memory limits prevent runaway growth
- Log rotation prevents disk exhaustion
- Internal ports bind to `127.0.0.1` only

See [`DEPLOYMENT.md`](DEPLOYMENT.md) for the full production guide.

## TLS / HTTPS

Three options for HTTPS:

| Option | Best For | Details |
|--------|----------|---------|
| **A** — Let's Encrypt | Public servers | Free certificates with auto-renewal |
| **B** — Manual certs | Internal / air-gapped | Bring your own certificate files |
| **C** — External termination | Cloudflare, AWS ALB | TLS terminated at the edge |

See the [TLS section in `DEPLOYMENT.md`](DEPLOYMENT.md#tls--https-setup) for step-by-step instructions.

## Updating

To upgrade to a new release:

```bash
./scripts/backup-stack.sh
docker compose pull
docker compose up -d
./scripts/final-launch-smoke.sh
```

See [`docs/upgrading.md`](upgrading.md) for version-specific notes and rollback procedures.

## Next Steps

- [`docs/configuration.md`](configuration.md) — Environment variables, OIDC, quotas
- [`docs/DEPLOYMENT.md`](DEPLOYMENT.md) — Production hardening, TLS, troubleshooting
- [`docs/troubleshooting.md`](troubleshooting.md) — Common issues and fixes
