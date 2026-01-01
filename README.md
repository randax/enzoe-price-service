# ENTSOE Price Fetcher

A Rust service to fetch European electricity prices from the ENTSOE API.

## Database Setup

### Prerequisites
- PostgreSQL 18 installed
- `sqlx-cli` installed: `cargo install sqlx-cli --no-default-features --features postgres`

### Steps

1. Create database:
   ```bash
   createdb entsoe_prices
   ```

2. Run migrations:
   ```bash
   sqlx migrate run --database-url postgresql://postgres:postgres@localhost:5432/entsoe_prices
   ```

3. Verify schema:
   ```bash
   psql entsoe_prices -c "\dt"
   ```

### Environment Variables

Copy `.env.example` to `.env` and configure:
```bash
cp .env.example .env
# Edit .env with your ENTSOE API token
```

## Development

```bash
# Check compilation
cargo check

# Run the service
cargo run

# Run with debug logging
RUST_LOG=debug cargo run
```

## Docker Build

Build the container image using the multi-stage Dockerfile with cargo-chef for optimal layer caching:

```bash
docker build -t entsoe-price-fetcher:latest .
```

The build process:
1. **Planner stage**: Generates dependency recipe with cargo-chef
2. **Builder stage**: Builds dependencies (cached) then application
3. **Runtime stage**: Creates minimal distroless image (~40-50MB)

## Local Development with Docker Compose

### Prerequisites
- Docker and Docker Compose installed
- ENTSOE API token

### Start Services

```bash
# Set your ENTSOE API token
export ENTSOE_SECURITY_TOKEN=your_token_here

# Start PostgreSQL and the application
docker-compose up -d

# View logs
docker-compose logs -f app

# Access API
curl http://localhost:8080/api/v1/zones

# Stop services
docker-compose down
```

**Note**: Database data persists in the `pgdata` volume. To reset: `docker-compose down -v`

## Kubernetes Deployment

### Prerequisites
- kubectl configured with cluster access
- Container image pushed to registry

### Deploy

```bash
# 1. Create namespace (optional)
kubectl create namespace entsoe

# 2. Create secrets
kubectl create secret generic entsoe-secrets \
  --from-literal=database-url='postgresql://user:password@host:5432/entsoe_prices' \
  --from-literal=entsoe-token='your_entsoe_api_token_here'

# 3. Apply manifests
kubectl apply -k k8s/

# 4. Verify deployment
kubectl get pods -l app=entsoe-price-fetcher

# 5. Check logs
kubectl logs -l app=entsoe-price-fetcher -f

# 6. Port forward for testing
kubectl port-forward svc/entsoe-price-fetcher 8080:80
```

## Environment Variables Reference

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `APP_DATABASE__URL` | Yes | - | PostgreSQL connection string |
| `APP_ENTSOE__SECURITY_TOKEN` | Yes | - | ENTSOE API token |
| `APP_SERVER__HOST` | No | `0.0.0.0` | Server bind address |
| `APP_SERVER__PORT` | No | `8080` | Server port |
| `APP_SCHEDULER__ENABLED` | No | `true` | Enable scheduled fetching |
| `RUST_LOG` | No | `info` | Log level (trace, debug, info, warn, error) |
| `LOG_FORMAT` | No | `json` | Log format (json or pretty) |

## Production Considerations

- **Database migrations**: Run `sqlx migrate run` before first deployment or use init container
- **Monitoring**: Prometheus metrics available at `/metrics`
- **Health checks**: `/health` (liveness), `/ready` (readiness)
- **Resource tuning**: Adjust memory/CPU limits based on zone count and query load
- **Scaling**: Horizontal scaling supported (stateless API, scheduler runs in all replicas)
- **Database connection pooling**: Configure `max_connections` based on replica count

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Container fails to start | Check logs for configuration errors, verify secrets exist |
| Database connection errors | Verify `DATABASE_URL`, check network policies, ensure PostgreSQL has pg_partman extension |
| ENTSOE API errors | Verify token validity, check rate limiting, review fetch_log table |
| Missing data | Check scheduler logs, verify ENTSOE API availability, review fetch times (13:00-16:00 CET) |
