# 0002. No login; a shared API token instead

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

There is exactly one user. User accounts, registration, passwords and sessions would add code, a database and attack surface for no benefit. The API still must not be open to the internet, because it uses the owner's INDmoney access and provider quotas.

## Decision

- No login or registration in the app.
- The API requires `Authorization: Bearer <API_TOKEN>` on every `/v1/*` route. `/health` stays open for health checks.
- The API refuses to start without `API_TOKEN` unless `ALLOW_NO_TOKEN=true` is set explicitly (local use only).
- The Next.js app calls the API only from the server side, so the token lives in server environment variables and never reaches the browser. It must never be put in a `NEXT_PUBLIC_*` variable.

## Consequences

- Very little auth code, no user table, nothing to reset.
- The token is the single secret protecting the API. It must be long, random, kept out of git, and rotated if leaked (change it on API and web together).
- The deployed web front end has no login, so anyone with its URL can view it. Keep the URL private; if that stops being enough, put an access layer (for example Cloudflare Access) in front rather than building accounts.
- Moving to multiple users later would need a real auth design and a new ADR.
