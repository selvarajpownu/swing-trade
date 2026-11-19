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

- US: `capital` defaults to `US_DEFAULT_CAPITAL` (USD). 15-minute bars come from Yahoo (INDmoney has none).

`provider` in a response is the source that answered: `yahoo`, `indmoney`, `nse` (cached NSE end-of-day files) or
`demo`. The order is `DATA_PROVIDERS`.
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

Headlines for one stock. Query: `market`. Cached 30 minutes; headlines are never invented.

- `provider: "indmoney"`: INDmoney news (title, summary, sentiment; no publisher or link), when INDmoney has any.
- `provider: "google_news"`: otherwise Google News RSS for the company's name (publisher and link; `summary` and
  `sentiment` are `null`).
- With only `demo` configured the list is empty and nothing is fetched.

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

When scans read NSE's end-of-day files (`NSE_EOD`, the default with real data), `provider` here is `nse`.

## GET /v1/swing

The Swing Stocks screen: market regime, sector strength and gated swing setups across the scan universe, computed
by the background scan on daily bars. Like `/v1/scan` it only reads memory.

Query (all optional):

| Param | Default | Meaning |
|---|---|---|
| `setup` | `breakout` | Which list: `breakout`, `pullback`, `results_gap`, or `leaders` (every stock passing the leader gate, strongest first). |
| `gates` | all three | Comma-separated gates a stock must pass: `tradable`, `leader`, `sector`; `none` requires nothing. |
| `limit` | `100` | Rows to return, 1-500. |
| `capital`, `risk` | server defaults | Only `quantity` depends on them. |

```json
{
  "source": "nse",
  "as_of": "2026-11-19",
  "status": { "state": "ready", "scan": "idle", "universe_size": 503, "failed": 4, "excluded": ["INDRAMEDCO"],
              "days_cached": 300, "days_target": 300, "error": null, "catalyst_errors": [] },
  "regime": { "state": "normal", "index": "Nifty 50", "index_close": 25514.3, "index_sma50": 23469.52,
              "index_sma200": 23990.75, "breadth_50_pct": 58.1, "breadth_200_pct": 50.3, "universe": 497 },
  "sectors": [ { "industry": "Healthcare", "members": 44, "ret_3m_pct": 0.7, "rank": 3, "strong": true } ],
  "counts": { "breakout": 1, "pullback": 4, "results_gap": 0, "leaders": 61 },
  "total": 1,
  "items": [
    {
      "symbol": "CRAFTSMAN", "name": "Craftsman Automation Ltd.", "exchange": "NSE",
      "industry": "Automobile and Auto Components",
      "price": 11930.0, "change": 585.0, "change_pct": 5.16, "volume": 37012, "bar_time": 1795046400,
      "sparkline": [11010.0, 11345.0, 11930.0],
      "turnover_cr": 44.0, "rs": 89, "ret_1m_pct": 6.2, "ret_3m_pct": 10.7, "ret_6m_pct": 31.0,
      "below_high_pct": 2.3, "delivery_pct": 69.0, "delivery_avg20": 52.4,
      "gates": { "tradable": true, "leader": true, "sector": true },
      "setup": {
        "kind": "breakout", "label": "Breakout", "score": 65,
        "score_breakdown": { "rs": 26.7, "trend": 15.0, "setup": 12.1, "sector": 6.1, "delivery": 5.0, "quality": 0.0 },
        "level": 11405.0, "extension_pct": 4.6, "volume_ratio": 2.0,
        "entry": 12222.2, "stop": 11226.55, "risk_per_share": 995.65,
        "target_2r": 14213.5, "target_3r": 15209.15, "quantity": 5,
        "review_by": "2026-12-31",
        "reasons": ["Closed 4.6% above its 20-day high on 2.0x average volume", "Base before the breakout was 9% deep"]
      },
      "catalysts": [
        { "kind": "results", "tone": "risk", "date": "2026-11-25", "days": 6, "title": "Results",
          "detail": "To consider and approve the financial results for the period ended September 30, 2026",
          "url": "https://nsearchives.nseindia.in/corporate/..." }
      ]
    }
  ]
}
```

- `source`: `nse` when the scan read NSE's end-of-day files, otherwise the primary provider's id; `null` (with
  `regime: null` and no items) before the first scan. `status.state` is `syncing` while NSE files download for the
  first time, `pending` before any scan, then `ready`. `days_*` are `null` when NSE files are not used.
- **Gates.** `tradable`: normal `EQ` series and median 20-day turnover ≥ `SWING_MIN_TURNOVER_CR`. `leader`: RS ≥ 70,
  close > SMA 50 > rising SMA 200, within 25% of the 52-week high. `sector`: the industry is in the stronger half by
  median 3-month return (`null` when the industry is unknown or has fewer than 3 members; `null` never excludes).
- `rs`: average percentile rank (0-100) of the 3- and 6-month returns within the universe.
- `regime.state`: `defensive` when the index is below its 200-day average; `caution` when it is below its 50-day
  average or `breadth_50_pct` < 40; otherwise `normal`. Without index data (demo) only breadth is used.
- **Setups** fire on the latest daily bar. `breakout`: close above the prior 20-day high on ≥ 1.5× the 20-day average
  volume, base range ≤ 15%, close in the top third of the day's range, at most 5% above the level. `pullback`: 3-10%
  off a swing high made 3-10 bars ago on below-average volume, low within 1% of EMA 20 or SMA 50, close above the
  previous bar's high. For `leaders`, `setup` is the stock's best setup or `null`.
- `results_gap` (shown for three sessions): on the session of a results board meeting or the next one, an opening gap
  of ≥ 4% on ≥ 3× the 20-day average volume that closed in the upper half of its range, with no close below that
  day's low since and the latest close at most 5% above its high. Entry is above the highest high since the gap, the
  stop below the gap-day low.
- **Catalysts** (`catalysts`, from NSE's feeds; empty without them): events dated from 7 days before to 14 days after
  the latest bar, `risk` tone first, then newest first. `days` counts from the latest bar (negative is past). `kind` is
  one of `results`, `dividend`, `bonus`, `split`, `buyback`, `demerger`, `rights`, `fundraise`, `order_win`,
  `expansion`, `deal`, `credit_rating`, `management`, `legal`, `exchange_query`, `stake_change`, `bulk_deal`,
  `block_deal`, `surveillance`, `fno_ban`. `tone` (`positive`, `negative`, `neutral`, `risk`) is a rule of thumb for
  colouring: upcoming results, surveillance and F&O ban are `risk`; resignations, auditor changes and regulatory or
  legal filings are `negative`; order wins, capacity additions and buybacks are `positive`. A stock on an ASM or GSM
  surveillance list fails the `tradable` gate. `status.catalyst_errors` lists sources that could not be refreshed.
- **Plan.** `entry` = setup bar high + 0.1%; `stop` below the structure (breakout bar low / level, or the dip low), at
  least 1 ATR away; `target_2r` / `target_3r` = entry + 2× / 3× risk; `review_by` = setup bar + 42 days.
- `score` (0-100) = RS 30 + trend 15 + setup 30 + sector 10 + delivery 5 + quality 10. `quality` stays 0 until
  fundamentals are imported. All thresholds are starting values that have not been back-tested.

## GET /v1/stocks/{symbol}/events

NSE catalysts for one NSE stock: the same objects as `catalysts` above, for the last 7 days and the next 14, with
`days` counted from today (IST). Reads memory only.

```json
{
  "symbol": "ADANIPORTS", "as_of": "2026-11-19", "available": true,
  "items": [
    { "kind": "credit_rating", "tone": "neutral", "date": "2026-11-17", "days": -2, "title": "Credit rating",
      "detail": "Adani Ports and Special Economic Zone Limited has informed the Exchange about Credit Rating",
      "url": "https://nsearchives.nseindia.in/corporate/..." }
  ]
}
```

- `available` is `false` (and `items` empty) when NSE catalysts are not in use: demo data, or `NSE_EOD=false`.

