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

## GET /v1/stocks

Query: `symbols` (comma-separated NSE symbols; default = server `WATCHLIST`), `capital` (default 500000), `risk` (fraction, default 0.01).

```json
{
  "provider": "demo",
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

Query: `interval` (`1d` default, `1w`, `15m`), `capital`, `risk` as above.

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
