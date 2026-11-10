# 0001. Tech stack

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

A single person builds and runs this app, mostly on Windows, and wants it to run on free hosting first. It needs fast numeric work over candles (indicators, zones, scans across hundreds of stocks later), a good charting UI, and room to grow into scheduled data jobs and backtests.

## Decision

- **API:** Rust (stable, edition 2021) with Axum and Tokio, as a Cargo workspace in `services/api`:
  - `swing-core`: dependency-free analytics, easy to test and reuse in scans/backtests.
  - `market-data`: provider connectors behind one trait.
  - `api-server`: HTTP server, binary `swing-api`.
- **Web:** Next.js 15 (App Router, TypeScript) with npm, `output: "standalone"` for small Docker images, deployable as is on Vercel.
- **Storage:** none in phase 1 (in-memory caches). Postgres + TimescaleDB in phase 3.
- **Python:** not used now. A `services/quant` service is added later only if an analytics library we need exists only in Python.
- **Packaging:** Docker Compose for local and single-server runs; GitHub Actions for CI.

## Consequences

- One small Rust binary with low memory use fits free ARM VMs and laptops.
- Two languages (Rust + TypeScript) to maintain; the API contract in `docs/api.md` is the boundary between them.
- Rust compile times are slower than scripting languages; CI and Docker builds cache dependencies to compensate.
- Without a database, restarts lose caches and there is no history beyond what providers return. Accepted until phase 3.
