# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Report them privately through GitHub: **Security → Report a vulnerability** on this repository (private vulnerability reporting). Include what you found, how to reproduce it and the impact you expect.

You will get an acknowledgement as soon as possible. Fixes go to `main`; there are no maintained release branches.

## Supported versions

Only the latest `main` is supported.

## Secrets handling

The app has no user accounts. Its security rests on two secrets:

| Secret | Where it lives | Notes |
|---|---|---|
| `API_TOKEN` | `.env` locally; platform env vars (Vercel, VM) in production | Protects every `/v1/*` route. Server-side only; never in a `NEXT_PUBLIC_*` variable or in client code. |
| INDmoney OAuth token | `.data/indmoney-token.json` or `INDMONEY_ACCESS_TOKEN` | Grants read access to your INDmoney data. |

Rules:

- `.env`, `.env.*` (except `.env.example`) and `.data/` are git-ignored. Never commit them, paste them in issues or include them in logs or screenshots.
- Generate `API_TOKEN` with a cryptographic RNG (`openssl rand -hex 32`), at least 32 bytes.
- Never expose the API to the internet without `API_TOKEN`, and never set `ALLOW_NO_TOKEN=true` outside your own machine. Serve it over HTTPS only.
- If a secret leaks: rotate `API_TOKEN` on both API and web, delete the token file and run `swing-api login indmoney` again, and revoke the INDmoney session if possible.
- The app never places orders and stores no broker order credentials ([ADR 0003](docs/decisions/0003-view-only-no-orders.md)).

Dependencies are kept current with Dependabot (Cargo, npm, GitHub Actions).
