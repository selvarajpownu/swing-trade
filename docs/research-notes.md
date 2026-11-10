# Research notes (carried over from the planning chat, 10 Nov 2026)

These notes record the stock research that shaped the app, so future work keeps the same rules.
They are **personal analysis, not investment advice**.

## Data used

- Screener.in CSV exports (62 columns each) for 56 NSE industries, data date inferred as **28 Oct 2026**
  (the only session where every file's prices sat inside INDmoney's daily ranges).
- Prices and candles: INDmoney MCP (`lookup_ind_keys`, `get_indian_stocks_details`, `get_indian_stocks_ohlc`).

## Quality gates (Stage 1, from the CSV) — the app's "Business quality" filter

| Gate | Test |
| --- | --- |
| G1 | Market cap ≥ Rs 300 cr, main board (not SME) |
| G2 | ROCE 3Y ≥ 15% and ROE 3Y ≥ 12% |
| G3 | Sales growth 3Y ≥ 10% and profit growth 3Y ≥ 12% |
| G4 | Debt/equity ≤ 0.6 and interest cover ≥ 4 |
| G5 | CFO positive both years, CFO/PAT ≥ 0.7, FCF positive in ≥ 2 of 3 years |
| G7 | Promoter ≥ 40% (or no promoter with FII+DII ≥ 25%), pledge ≤ 5%, promoter stake not down > 5 pts in the quarter |

CSV score (100): quality 30 (ROCE 12, ROE 10, OPM 8), financial 25 (D/E 7, interest cover 4, CFO/PAT 7, FCF years 7),
growth 20 (sales 3Y 7, profit 3Y 7, latest profit 3, sales 5Y 3), ownership 15 (promoter 5, pledge 5, change 5),
valuation 10 (P/E 6, PEG 4).

## Stage 2 checks (fetched, per company)

- **Hard exclusion (regulatory):** any SEBI order, settlement or adjudication on the company or promoter in the
  last 5 years; any tax, ED, CBI or customs search/probe on the company or promoter.
- **G6 earnings quality (counts as a miss):** OPM down > 1 pt in 2+ of FY24–26; debtor days up > 10 days and > 25%
  FY23→FY26; other income > 30% of PBT.
- **Amber flags (−3 each):** A1 tax rate < 20% latest year; A2 FY26 PAT > 10% below FY25 or TTM > 10% below FY26;
  A3 regulator warning/notice/information request in 24 months.
- **"Already re-rated" flag:** 5-year price CAGR > 43.1% (price up > 500% in 5 years).

## Companies that cleared everything (7 industries studied)

Capital markets: SBI Funds, HDFC AMC, CAMS, IEX, Prudent, Aditya Birla Sun Life AMC, UTI AMC ·
Vehicles: Action Construction Equipment, Ajax Engineering · Autos: Force Motors, Maruti ·
Pharma: Anthem, Lupin, Corona, Natco, Marksans, Windlas, Fermenta, Sun, Divi's ·
Food: Nestle, Bikaji, Ganesh Consumer, Dodla, Heritage · Electrical: GE Vernova T&D, Shilchar, Atlanta, Hitachi Energy, Apar ·
Beverages: none.

Dominant B2B suppliers that also passed every CSV gate (shares from fetched sources, Jun–Nov 2026):
CAMS (68% of MF AUM), IEX (> 85% of power-exchange trading; market-coupling risk), ACE (63% mobile cranes),
Ajax (75.5% self-loading concrete mixers), ASK Automotive (~50% of two-wheeler braking systems).
Removed only by the strict SEBI rule: MCX, BSE, Cummins India (parent's 2023 SEBI settlement).

## Swing behaviour observed (monthly candles, Nov 2025 – Nov 2026)

Coforge rose 103% from its March 2026 low, but only after a 49% fall (Dec → Mar); 12-month change +16%.
IT peers with the same fall split both ways: OFSS +61%, Rategain +60% from March lows, while KPIT (−54% over the
year) and Newgen (−54%) kept falling. Lesson built into the app: quality filters and setups describe the past;
every preset must be back-tested (phase 5) before it is trusted.

## User context

- Holds Indraprastha Medical Corporation already — exclude it from any "top" lists.
- Uses INDmoney; individual stocks for short-term swing trades, ETFs for long-term.
