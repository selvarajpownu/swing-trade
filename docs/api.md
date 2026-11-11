# API contract (v1)

The Rust service (`services/api`) serves JSON. The Next.js app (`apps/web`) calls it **only from the server side** (server components and route handlers), so the token never reaches the browser.

- Base URL: `API_URL` (default `http://localhost:8080`)
- Auth: every `/v1/*` call needs `Authorization: Bearer <API_TOKEN>`. `/health` is open.
- Numbers are plain JSON numbers (rupees, shares). Times are Unix seconds (UTC) unless the field name ends in `_at` (ISO 8601).
- Errors: HTTP status + `{"error": {"code": "not_found" | "unauthorized" | "bad_request" | "upstream" | "internal", "message": "..."}}`

## GET /health

```json
{ "status": "ok", "version": "0.1.0", "providers": ["indmoney", "demo"] }
```

## Markets

`/v1/stocks`, `/v1/stocks/{symbol}` and `/v1/stocks/{symbol}/news` take `market`: `in` (NSE, default), `us` (US stocks,
prices in USD) or `mcx` (MCX commodity futures, near-month contract, prices in INR). Responses carry `market` and
`currency` (`INR` or `USD`); every price in the response is in that currency. Symbols are always bare (`AAPL`, `GOLD`).

- US: `interval=15m` is a 400 (daily data only); `capital` defaults to `US_DEFAULT_CAPITAL` (USD).
- MCX: setups have `quantity: null` (futures trade in lots, and lot sizes are not available).

## GET /v1/stocks

Query: `market`, `symbols` (comma-separated; default = the market's watchlist: `WATCHLIST`, `US_WATCHLIST` or
`COMMODITY_WATCHLIST`), `capital` (default 500000, or `US_DEFAULT_CAPITAL` for `us`), `risk` (fraction, default 0.01).

```json
{
  "provider": "demo",
  "market": "in",
  "currency": "INR",
  "as_of": "2026-11-10T15:30:00Z",
  "items": [
    {
      "symbol": "COFORGE",
      "name": "Coforge",
      "exchange": "NSE",
      "price": 2047.0,
      "change": 2.0,
      "change_pct": 0.1,
      "high_52w": 2127.9,
      "low_52w": 1008.1,
      "volume": 1281105,
      "sparkline": [1990.1, 2001.5, 2047.0],
      "setup": {
        "preset": "pullback",
        "label": "Pullback",
        "state": "watching",
        "score": 74,
        "entry": 2061.5,
        "stop": 1985.0,
        "target": 2210.0,
        "risk_reward": 1.94,
        "quantity": 65
      }
    }
  ]
}
```

- `sparkline`: last 30 daily closes, oldest first.
- `setup`: `null` when no preset fires. `preset` ∈ `pullback | breakout | support_bounce | oversold`. `state` ∈ `watching | low_risk_reward`.

## GET /v1/stocks/{symbol}

Query: `market`, `interval` (`1d` default, `1w`, `15m`), `capital`, `risk` as above. The response also has `market` and
`currency` at the top level.

```json
{
  "provider": "demo",
  "quote": {
    "symbol": "COFORGE", "name": "Coforge", "exchange": "NSE",
    "price": 2047.0, "change": 2.0, "change_pct": 0.1,
    "day_high": 2060.0, "day_low": 2030.0,
    "high_52w": 2127.9, "low_52w": 1008.1,
    "volume": 1281105, "updated_at": "2026-11-09T15:59:59+05:30"
  },
  "candles": [ { "time": 1762819200, "open": 1990.0, "high": 2010.0, "low": 1985.0, "close": 2001.5, "volume": 900000 } ],
  "overlays": {
    "sma20":  [ { "time": 1762819200, "value": 1980.2 } ],
    "sma50":  [ { "time": 1762819200, "value": 1900.4 } ],
    "sma200": [ { "time": 1762819200, "value": 1700.9 } ]
  },
  "snapshot": {
    "close": 2047.0, "sma20": 1980.2, "sma50": 1900.4, "sma200": 1700.9,
    "rsi14": 58.2, "atr14": 61.3, "volume_avg20": 1100000,
    "high_52w": 2127.9, "low_52w": 1008.1
  },
  "zones": [ { "low": 1985.0, "high": 2001.0, "touches": 3, "kind": "support" } ],
  "setup": {
    "preset": "pullback", "label": "Pullback", "state": "watching", "score": 74,
    "entry": 2061.5, "stop": 1985.0, "target": 2210.0, "risk_reward": 1.94, "quantity": 65,
    "risk_per_share": 76.5, "reward_per_share": 148.5, "target_from_zone": true,
    "score_breakdown": { "trend": 30, "closeness": 22, "volume": 10, "risk_reward": 9.7, "quality": 0 },
    "reasons": ["Price is 1.2% from its 20/50-day average in an uptrend", "RSI(14) is 48", "Risk-reward 1.94"]
  }
}
```

- Overlay arrays only contain bars where the average exists (no nulls).
- `snapshot` indicator fields may be `null` when history is too short.
- `zones` are nearest-to-price first.
- `setup` may be `null`.

## GET /v1/stocks/{symbol}/news

Headlines from the data provider (INDmoney news: title, summary, sentiment; no publisher or link). Query: `market`.
Cached 30 minutes. MCX and demo return an empty list; headlines are never invented.

```json
{
  "provider": "indmoney", "market": "in", "symbol": "ACE",
  "items": [
    { "title": "ACE Secures Defence Order, Shares Rise 3%", "summary": "Action Construction Equipment announced ...",
      "sentiment": "positive", "published_at": "2026-09-30", "source": null, "url": null }
  ]
}
```

## GET /v1/news

Google News RSS headlines (publisher and link, no sentiment) for one stock (`symbol`) or the whole watchlist of
`market`, newest first, at most 60, de-duplicated by title. Fetched server-side and cached 30 minutes per stock.

```json
{
  "source": "google_news", "market": "in", "as_of": "2026-11-10T17:40:00Z",
  "items": [
    { "symbol": "COFORGE", "name": "Coforge", "title": "Coforge shares rise after Q2 results",
      "summary": null, "sentiment": null, "published_at": "2026-11-09T08:15:00Z",
      "source": "Moneycontrol", "url": "https://news.google.com/rss/articles/..." }
  ]
}
```

## GET /v1/scan

Ranked swing setups across the scan universe (`UNIVERSE_FILE`, else `UNIVERSE`, else `WATCHLIST`). A background task scans
on daily candles; this endpoint only reads the latest results, so it is fast and never calls a data provider.

Query (all optional):

| Param | Default | Meaning |
|---|---|---|
| `preset` | any | `pullback`, `breakout`, `support_bounce` or `oversold`. With a preset, each row shows that preset's plan. |
| `min_score` | `0` | Minimum setup score (0-100). |
| `min_rr` | `0` | Minimum risk-reward. |
| `state` | any | `watching` or `low_risk_reward`. |
| `limit` | `50` | Rows to return, 1-500. |
| `capital`, `risk` | server defaults | As for `/v1/stocks`; only `quantity` depends on them. |

```json
{
  "provider": "indmoney",
  "status": {
    "state": "idle",
    "universe_size": 500,
    "scanned": 497,
    "failed": ["XYZ"],
    "excluded": ["INDRAMEDCO"],
    "started_at": "2026-11-10T10:20:00Z",
    "finished_at": "2026-11-10T10:41:12Z",
    "next_run_at": "2026-11-10T11:11:12Z"
  },
  "total": 23,
  "items": [
    {
      "symbol": "ASKAUTOLTD", "name": "ASK Automotive", "exchange": "NSE", "industry": "Auto Components",
      "price": 613.45, "change": -8.45, "change_pct": -1.36,
      "high_52w": 687.7, "low_52w": 375.3, "volume": 512345,
      "bar_time": 1794182400,
      "sparkline": [601.2, 610.0, 613.45],
      "presets": ["pullback"],
      "setup": { "preset": "pullback", "label": "Pullback", "state": "watching", "score": 68,
                 "entry": 625.1, "stop": 590.0, "target": 700.0, "risk_reward": 2.13, "quantity": 142 }
    }
  ]
}
```

- `status.state`: `pending` (no scan finished yet), `running` or `idle`. While a scan runs, `items` holds the previous
  completed scan's results and `scanned` counts progress. During the very first scan (no earlier results) hits are
  published as they are found.
- `provider`: the provider the scan read from: the first entry in `DATA_PROVIDERS`. A scan never falls back to demo.
- Items are the rows with at least one setup that passes every filter, best score first; `total` counts them before `limit`.
- Prices come from the latest **daily candle** (`bar_time`, Unix seconds), not a live quote. `change` is against the previous
  daily close. `industry` is `null` unless the universe file has an industry column.
- `presets`: every preset firing on the latest bar; `setup` is the best one that passes the filters.
- Indraprastha Medical (`INDRAMEDCO`) and any `SCAN_EXCLUDE` symbols are never scanned and are listed in `status.excluded`.
