# Independent Memory worker (Compose)

The independent worker is opt-in. Apply all backend migrations first, provision
a dedicated PostgreSQL `LOGIN` role, and grant it only the
`elembra_memory_worker` group. The group must not inherit another role; the
grant script fails closed if it does. Keep the login password in the deployment
secret store and set `MEMORY_DATABASE_URL` to that login. Do not use the
backend's `DATABASE_URL`.

After migrations, run the grant script as the database owner. Replace the login
role name with the credential provisioned for this worker:

```sh
psql "$MIGRATOR_DATABASE_URL" -v ON_ERROR_STOP=1 \
  -v memory_worker_login=elembra_memory_worker_login \
  -f scripts/grant-memory-worker.sql
```

Set `RUSTSHARE_MEMORY_WORKER_SECRET` to a strong random secret shared by the
backend and worker (for example, `openssl rand -hex 32`). The API origin defaults
to `http://backend:8080`; override it with `MEMORY_WORKER_API_ORIGIN` only when
the backend has a different internal address. Then start the opt-in profile:

```sh
docker compose -f docker-compose.yml -f docker-compose.memory-worker.yml \
  --profile memory-worker up -d --build
docker compose -f docker-compose.yml -f docker-compose.memory-worker.yml \
  --profile memory-worker ps
docker compose -f docker-compose.yml -f docker-compose.memory-worker.yml \
  --profile memory-worker logs -f memory-worker
```

Compose checks the worker's internal `/ready` endpoint. The health port is not
published to the host. Stop the profile with `docker compose` and the same two
file/profile options followed by `stop memory-worker`; graceful shutdown
releases its execution-mode lease. Once stopped, recreate the backend from the
base Compose file so it returns to embedded Memory processing:

```sh
docker compose -f docker-compose.yml -f docker-compose.memory-worker.yml \
  --profile memory-worker stop memory-worker
docker compose -f docker-compose.yml up -d --no-deps --force-recreate backend
```

If the worker was killed and could not release its lease, wait up to 30 seconds
for lease expiry before restarting embedded processing. Verify backend
readiness after the switch. Keep PostgreSQL volumes, outbox rows and consumer
receipts during rollback; they preserve pending work and idempotency state.
