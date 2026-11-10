# Contributing

This is a personal project, but the same rules apply to every change, including the owner's.

## Branching

- `main` is always deployable. Don't push to it directly; open a pull request.
- Branch from `main` with a short prefix: `feat/scanner-filters`, `fix/zone-merge`, `docs/deployment`, `chore/deps`.
- Keep branches small and short-lived. Rebase on `main` before merging; squash-merge is preferred.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <summary>

feat(api): add 1w interval to /v1/stocks/{symbol}
fix(swing-core): never place stop above entry
docs(readme): add Windows notes
```

Types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`, `chore`. Scopes: `api`, `web`, `swing-core`, `market-data`, `docs`, `ci`, `deps`. Mark breaking API changes with `!` (`feat(api)!: ...`) and update `docs/api.md` in the same PR.

## Checks before opening a PR

```bash
make lint     # cargo fmt --check, cargo clippy -D warnings, npm run lint
make test     # cargo test, npm run typecheck, npm run lint
```

On Windows without `make`, run the commands listed under "Testing and linting" in the README. CI runs the same checks and must be green before merge.

## Guidelines

- The app is view and analysis only. Don't add anything that places, modifies or cancels orders ([ADR 0003](docs/decisions/0003-view-only-no-orders.md)).
- Keep `swing-core` free of I/O and external dependencies; add unit tests for any change to indicators, zones, setups or plans.
- New data sources implement the `MarketData` trait ([ADR 0004](docs/decisions/0004-pluggable-market-data.md)).
- Never commit secrets, `.env` files or anything in `.data/`.
- Record significant decisions as a new ADR in `docs/decisions/` (Status, Context, Decision, Consequences).
