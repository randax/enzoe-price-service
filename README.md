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
