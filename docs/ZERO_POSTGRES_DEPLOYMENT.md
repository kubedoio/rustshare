# Zero-PostgreSQL Deployment Guide

This guide covers deploying RustShare without PostgreSQL.

## Overview

RustShare now supports a zero-PostgreSQL architecture where:
- **RustFS** (S3-compatible object storage) is the durable system of record
- **Redis** is optional and used only for distributed coordination
- All metadata is stored in RustFS with derived indexes for queries

## Deployment Modes

### Standalone Mode

Best for: Development, small deployments, single-node installations

```yaml
# docker-compose.standalone.yml
version: '3.8'

services:
  rustshare:
    image: rustshare:latest
    environment:
      RUSTSHARE_RUNTIME_PROFILE: standalone
      RUSTFS_ENDPOINT: http://rustfs:9000
      RUSTFS_REGION: us-east-1
      RUSTFS_BUCKET: rustshare
      RUSTSHARE_LOCAL_STORAGE_PATH: /data/metadata
      JWT_SECRET: your-jwt-secret
      ENCRYPTION_KEY: your-encryption-key
    volumes:
      - rustshare-metadata:/data/metadata
    ports:
      - "8080:8080"
    depends_on:
      - rustfs

  rustfs:
    image: minio/minio:latest
    command: server /data --console-address ":9001"
    environment:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin
    volumes:
      - rustfs-data:/data
    ports:
      - "9000:9000"
      - "9001:9001"

volumes:
  rustshare-metadata:
  rustfs-data:
```

### Distributed Mode

Best for: Production, high availability, horizontal scaling

```yaml
# docker-compose.distributed.yml
version: '3.8'

services:
  rustshare-1:
    image: rustshare:latest
    environment:
      RUSTSHARE_RUNTIME_PROFILE: distributed
      RUSTSHARE_REDIS_ENABLED: "true"
      RUSTSHARE_REDIS_URL: redis://redis:6379
      RUSTFS_ENDPOINT: http://rustfs:9000
      RUSTFS_REGION: us-east-1
      RUSTFS_BUCKET: rustshare
      JWT_SECRET: your-jwt-secret
      ENCRYPTION_KEY: your-encryption-key
    ports:
      - "8080:8080"
    depends_on:
      - redis
      - rustfs

  rustshare-2:
    image: rustshare:latest
    environment:
      RUSTSHARE_RUNTIME_PROFILE: distributed
      RUSTSHARE_REDIS_ENABLED: "true"
      RUSTSHARE_REDIS_URL: redis://redis:6379
      RUSTFS_ENDPOINT: http://rustfs:9000
      RUSTFS_REGION: us-east-1
      RUSTFS_BUCKET: rustshare
      JWT_SECRET: your-jwt-secret
      ENCRYPTION_KEY: your-encryption-key
    ports:
      - "8081:8080"
    depends_on:
      - redis
      - rustfs

  redis:
    image: redis:7-alpine
    volumes:
      - redis-data:/data
    ports:
      - "6379:6379"

  rustfs:
    image: minio/minio:latest
    command: server /data --console-address ":9001"
    environment:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin
    volumes:
      - rustfs-data:/data
    ports:
      - "9000:9000"
      - "9001:9001"

volumes:
  redis-data:
  rustfs-data:
```

## Configuration Reference

### Required Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `RUSTSHARE_RUNTIME_PROFILE` | Deployment mode | `standalone` or `distributed` |
| `RUSTFS_ENDPOINT` | S3-compatible endpoint | `http://localhost:9000` |
| `RUSTFS_REGION` | S3 region | `us-east-1` |
| `RUSTFS_BUCKET` | S3 bucket name | `rustshare` |
| `JWT_SECRET` | JWT signing key | (generate strong secret) |
| `ENCRYPTION_KEY` | Data encryption key | (generate 32-byte key) |

### Standalone-Specific Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUSTSHARE_LOCAL_STORAGE_PATH` | Local path for metadata | `./rustshare-data` |

### Distributed-Specific Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `RUSTSHARE_REDIS_ENABLED` | Enable Redis | `true` |
| `RUSTSHARE_REDIS_URL` | Redis connection URL | `redis://localhost:6379` |

### Optional Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUSTSHARE_METADATA_PREFIX` | Object key prefix | `apps/rustshare` |
| `RUSTSHARE_METADATA_NAMESPACE` | Namespace for isolation | `default` |
| `BROADCAST_CAPACITY` | WebSocket broadcast buffer | `1000` |

## Migration from PostgreSQL

If you have an existing PostgreSQL installation:

1. **Enable dual-write mode** (during transition):
   ```bash
   RUSTSHARE_METADATA_BACKEND=dual_write
   DATABASE_URL=postgres://...  # Keep existing
   ```

2. **Run migration tool**:
   ```bash
   cargo run --bin rustshare-migrate -- migrate --from postgres --to rustfs
   ```

3. **Verify data parity**:
   ```bash
   cargo run --bin rustshare-migrate -- verify
   ```

4. **Switch to rustfs-only**:
   ```bash
   # Remove DATABASE_URL, set:
   RUSTSHARE_METADATA_BACKEND=rustfs
   ```

5. **Remove PostgreSQL** from docker-compose.yml

## Security Considerations

### Session Management

- **Standalone**: Sessions are stateless JWTs. Revocation is in-memory only (cleared on restart).
- **Distributed**: Sessions use Redis-backed revocation cache for cross-instance consistency.

### Encryption

- All sensitive data is encrypted before storage in RustFS
- Encryption key must be provided via `ENCRYPTION_KEY` environment variable
- Use a strong 32-byte key (generate with `openssl rand -base64 32`)

### Network Security

- Use TLS for RustFS connections in production
- Use Redis with TLS and authentication in production
- Keep JWT secrets secure and rotate periodically

## Monitoring

### Health Checks

```bash
# Basic health check
curl http://localhost:8080/health

# Expected response
{"status":"ok"}
```

### Metrics

Key metrics to monitor:
- Object store latency
- Redis connection status (distributed mode)
- Job queue depth
- Session cache hit rate

## Troubleshooting

### Standalone Mode

**Issue**: Metadata lost on restart
- Check that `RUSTSHARE_LOCAL_STORAGE_PATH` is a persistent volume

**Issue**: Session revocation doesn't persist
- This is expected behavior in standalone mode
- Use distributed mode if persistent revocation is required

### Distributed Mode

**Issue**: Cannot claim jobs
- Check Redis connectivity: `redis-cli ping`
- Verify `RUSTSHARE_REDIS_URL` is correct

**Issue**: Inconsistent state across instances
- Ensure all instances use the same Redis and RustFS
- Check that `RUSTSHARE_RUNTIME_PROFILE=distributed` on all instances

## Performance Tuning

### Standalone Mode

- Increase `BROADCAST_CAPACITY` for high-traffic deployments
- Use fast local storage for `RUSTSHARE_LOCAL_STORAGE_PATH`

### Distributed Mode

- Redis: Use Redis Cluster for high availability
- RustFS: Use a production S3-compatible service (MinIO cluster, AWS S3, etc.)
- Consider read replicas for RustFS if supported

## Support

For issues specific to zero-PostgreSQL deployments:

1. Check the [Architecture Documentation](ZERO_POSTGRES_ARCHITECTURE.md)
2. Review the [Concern Classification Map](ZERO_POSTGRES_CONCERN_MAP.md)
3. File an issue with the `zero-postgres` label
