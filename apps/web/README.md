# Swing Trade — web app

Next.js 15 (App Router, React 19, TypeScript, Tailwind CSS v4) front end for the
swing-trading analysis app for Indian stocks. **View and analysis only: it never
places orders.** There is no login.

## Requirements

- Node.js 20 or newer
- The Rust API (`services/api`) running and reachable from the web server

## Getting started

```bash
cd apps/web
cp .env.example .env.local   # then edit API_URL / API_TOKEN
npm install
npm run dev                  # http://localhost:3000
```

## Scripts

| Script                 | What it does                                 |
| ---------------------- | -------------------------------------------- |
| `npm run dev`          | Start the dev server                         |
| `npm run build`        | Production build (`output: "standalone"`)    |
| `npm run start`        | Serve the production build                   |
| `npm run lint`         | ESLint (flat config, `next/core-web-vitals`) |
| `npm run typecheck`    | `tsc --noEmit`                               |
| `npm run format`       | Prettier (with the Tailwind class sorter)    |
| `npm run format:check` | Prettier in check mode (for CI)              |

The standalone build lands in `.next/standalone`; run it with
`node .next/standalone/server.js` after copying `.next/static` (and `public`)
next to it.

## Environment

| Variable               | Where       | Default                 | Purpose                                           |
| ---------------------- | ----------- | ----------------------- | ------------------------------------------------- |
| `API_URL`              | server only | `http://localhost:8080` | Base URL of the Rust API                          |
| `API_TOKEN`            | server only | (none)                  | Bearer token for `/v1/*`; never sent to a browser |
| `NEXT_PUBLIC_APP_NAME` | build time  | `Swing Trade`           | Name in the sidebar and page titles               |

`API_URL` and `API_TOKEN` are read at request time, so one image works in every
environment. `NEXT_PUBLIC_APP_NAME` is inlined at build time.

## How data flows

```
browser ──► Next.js server components ──► Rust API (/v1/*, Bearer API_TOKEN)
browser ──► /api/stocks, /api/stocks/[symbol] (route handlers, proxy) ──► Rust API
```

- Pages render on the server with `src/lib/api/server.ts` (`import "server-only"`,
  `cache: "no-store"`, request timeouts, typed `ApiError`).
- Client components refresh every 15 s through the route handlers using
  `usePolling` (`src/hooks/use-polling.ts`). Polling pauses while the tab is hidden.
- The API contract lives in `docs/api.md`; its TypeScript mirror is
  `src/lib/api/types.ts`.

## URL parameters

| Page               | Param      | Values                                                                      |
| ------------------ | ---------- | --------------------------------------------------------------------------- |
| `/stocks`          | `q`        | search text (name or symbol)                                                |
|                    | `filter`   | `all`, `setup`, `pullback`, `breakout`, `support_bounce`, `oversold`, `rr2` |
|                    | `sort`     | `name`, `price`, `change`, `high_52w`, `volume`, `score`, `rr`              |
|                    | `dir`      | `asc`, `desc`                                                               |
| `/stocks/[symbol]` | `interval` | `1d` (default), `1w`, `15m`                                                 |
| both               | `capital`  | rupees, default `500000`                                                    |
|                    | `risk`     | **percent** of capital per trade, default `1` (sent to the API as `0.01`)   |

## Layout

```
src/
  app/                 routes (stocks, stocks/[symbol], settings, watchlists, journal, api/*)
  components/
    layout/            sidebar, mobile tab bar, top bar, Ctrl+K palette, data-source context
    stocks/            stock list table, mobile cards, setup badge, score bar, sparkline
    stock/             stock page: chart, trade plan, key levels, snapshot, tabs
    ui/                small hand-written primitives (card, badge, button, skeleton, states)
  hooks/use-polling.ts
  lib/
    api/               types, server client, browser client, param parsing, errors
    format.ts          ₹ / en-IN / IST formatting
    stock-list-view.ts list sort/filter state <-> URL
    utils.ts           cn() and helpers
```

_For personal analysis only. Not investment advice._
