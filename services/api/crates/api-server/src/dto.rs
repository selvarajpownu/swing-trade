//! Response bodies, exactly as documented in `docs/api.md`.
//!
//! Prices are rounded to paise and volumes to whole shares so the JSON
//! stays clean (no `2061.5000000000002`).

use market_data::Quote;
use serde::Serialize;
use swing_core::{
    Analysis, Candle, Preset, ScoreBreakdown, Setup, SetupState, Snapshot, Zone, ZoneKind,
};

/// Closes in a list sparkline.
pub const SPARKLINE_LEN: usize = 30;
/// Bars in roughly one trading year.
const BARS_52W: usize = 250;

/// `GET /health`.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub providers: Vec<&'static str>,
}

/// `GET /v1/stocks`.
#[derive(Debug, Serialize)]
pub struct StocksResponse {
    pub provider: &'static str,
    /// ISO 8601, UTC.
    pub as_of: String,
    pub items: Vec<StockItem>,
}

/// One row of the stock list.
#[derive(Debug, Serialize)]
pub struct StockItem {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub price: f64,
    pub change: f64,
    pub change_pct: f64,
    pub high_52w: f64,
    pub low_52w: f64,
    pub volume: u64,
    /// Last 30 daily closes, oldest first.
    pub sparkline: Vec<f64>,
    pub setup: Option<SetupSummary>,
}

impl StockItem {
    /// Combines a quote with what was derived from its candles.
    pub fn new(quote: &Quote, sparkline: Vec<f64>, setup: Option<SetupSummary>) -> Self {
        Self {
            symbol: quote.symbol.clone(),
            name: quote.name.clone(),
            exchange: quote.exchange.clone(),
            price: round2(quote.price),
            change: round2(quote.change),
            change_pct: round2(quote.change_pct),
            high_52w: round2(quote.high_52w),
            low_52w: round2(quote.low_52w),
            volume: whole(quote.volume),
            sparkline,
            setup,
        }
    }
}

/// Setup fields shown in the list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetupSummary {
    pub preset: PresetDto,
    pub label: &'static str,
    pub state: SetupStateDto,
    pub score: u32,
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub risk_reward: f64,
    pub quantity: u64,
}

impl From<&Setup> for SetupSummary {
    fn from(s: &Setup) -> Self {
        Self {
            preset: s.preset.into(),
            label: s.preset.label(),
            state: s.state.into(),
            score: s.score.total().clamp(0.0, 100.0) as u32,
            entry: round2(s.plan.entry),
            stop: round2(s.plan.stop),
            target: round2(s.plan.target),
            risk_reward: round2(s.plan.risk_reward),
            quantity: s.plan.quantity,
        }
    }
}

/// Full setup on the stock page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetupDetail {
    #[serde(flatten)]
    pub summary: SetupSummary,
    pub risk_per_share: f64,
    pub reward_per_share: f64,
    pub target_from_zone: bool,
    pub score_breakdown: ScoreBreakdownDto,
    pub reasons: Vec<String>,
}

impl From<&Setup> for SetupDetail {
    fn from(s: &Setup) -> Self {
        Self {
            summary: s.into(),
            risk_per_share: round2(s.plan.risk_per_share),
            reward_per_share: round2(s.plan.reward_per_share),
            target_from_zone: s.plan.target_from_zone,
            score_breakdown: s.score.into(),
            reasons: s.reasons.clone(),
        }
    }
}

/// Points per scoring component.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScoreBreakdownDto {
    pub trend: f64,
    pub closeness: f64,
    pub volume: f64,
    pub risk_reward: f64,
    pub quality: f64,
}

impl From<ScoreBreakdown> for ScoreBreakdownDto {
    fn from(b: ScoreBreakdown) -> Self {
        Self {
            trend: round2(b.trend),
            closeness: round2(b.closeness),
            volume: round2(b.volume),
            risk_reward: round2(b.risk_reward),
            quality: round2(b.quality),
        }
    }
}

/// `pullback | breakout | support_bounce | oversold`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetDto {
    Pullback,
    Breakout,
    SupportBounce,
    Oversold,
}

impl From<Preset> for PresetDto {
    fn from(p: Preset) -> Self {
        match p {
            Preset::Pullback => PresetDto::Pullback,
            Preset::Breakout => PresetDto::Breakout,
            Preset::SupportBounce => PresetDto::SupportBounce,
            Preset::Oversold => PresetDto::Oversold,
        }
    }
}

/// `watching | low_risk_reward`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupStateDto {
    Watching,
    LowRiskReward,
}

impl From<SetupState> for SetupStateDto {
    fn from(s: SetupState) -> Self {
        match s {
            SetupState::Watching => SetupStateDto::Watching,
            SetupState::LowRiskReward => SetupStateDto::LowRiskReward,
        }
    }
}

/// `support | resistance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKindDto {
    Support,
    Resistance,
}

impl From<ZoneKind> for ZoneKindDto {
    fn from(k: ZoneKind) -> Self {
        match k {
            ZoneKind::Support => ZoneKindDto::Support,
            ZoneKind::Resistance => ZoneKindDto::Resistance,
        }
    }
}

/// `GET /v1/stocks/{symbol}`.
#[derive(Debug, Serialize)]
pub struct StockDetailResponse {
    pub provider: &'static str,
    pub quote: QuoteDto,
    pub candles: Vec<CandleDto>,
    pub overlays: Overlays,
    pub snapshot: SnapshotDto,
    pub zones: Vec<ZoneDto>,
    pub setup: Option<SetupDetail>,
}

impl StockDetailResponse {
    /// Assembles the stock page. `analysis` is `None` when history is too short.
    pub fn build(
        provider: &'static str,
        quote: &Quote,
        candles: &[Candle],
        analysis: Option<&Analysis>,
    ) -> Self {
        let (overlays, snapshot, zones, setup) = match analysis {
            Some(a) => (
                Overlays {
                    sma20: overlay(candles, &a.sma20),
                    sma50: overlay(candles, &a.sma50),
                    sma200: overlay(candles, &a.sma200),
                },
                SnapshotDto::from(&a.snapshot),
                a.zones.iter().map(ZoneDto::from).collect(),
                a.setup.as_ref().map(SetupDetail::from),
            ),
            None => (
                Overlays::default(),
                plain_snapshot(candles),
                Vec::new(),
                None,
            ),
        };
        Self {
            provider,
            quote: quote.into(),
            candles: candles.iter().map(CandleDto::from).collect(),
            overlays,
            snapshot,
            zones,
            setup,
        }
    }
}

/// A quote as returned by the stock page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuoteDto {
    pub symbol: String,
    pub name: String,
    pub exchange: String,
    pub price: f64,
    pub change: f64,
    pub change_pct: f64,
    pub day_high: f64,
    pub day_low: f64,
    pub high_52w: f64,
    pub low_52w: f64,
    pub volume: u64,
    pub updated_at: Option<String>,
}

impl From<&Quote> for QuoteDto {
    fn from(q: &Quote) -> Self {
        Self {
            symbol: q.symbol.clone(),
            name: q.name.clone(),
            exchange: q.exchange.clone(),
            price: round2(q.price),
            change: round2(q.change),
            change_pct: round2(q.change_pct),
            day_high: round2(q.day_high),
            day_low: round2(q.day_low),
            high_52w: round2(q.high_52w),
            low_52w: round2(q.low_52w),
            volume: whole(q.volume),
            updated_at: q.updated_at.clone(),
        }
    }
}

/// One OHLCV bar; `time` is Unix seconds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandleDto {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
}

impl From<&Candle> for CandleDto {
    fn from(c: &Candle) -> Self {
        Self {
            time: c.ts,
            open: round2(c.open),
            high: round2(c.high),
            low: round2(c.low),
            close: round2(c.close),
            volume: whole(c.volume),
        }
    }
}

/// Moving-average lines; only bars where the average exists.
#[derive(Debug, Default, Serialize)]
pub struct Overlays {
    pub sma20: Vec<Point>,
    pub sma50: Vec<Point>,
    pub sma200: Vec<Point>,
}

/// One overlay value.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Point {
    pub time: i64,
    pub value: f64,
}

/// Latest indicator values; `null` when history is too short.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotDto {
    pub close: f64,
    pub sma20: Option<f64>,
    pub sma50: Option<f64>,
    pub sma200: Option<f64>,
    pub rsi14: Option<f64>,
    pub atr14: Option<f64>,
    pub volume_avg20: Option<f64>,
    pub high_52w: f64,
    pub low_52w: f64,
}

impl From<&Snapshot> for SnapshotDto {
    fn from(s: &Snapshot) -> Self {
        Self {
            close: round2(s.close),
            sma20: s.sma20.map(round2),
            sma50: s.sma50.map(round2),
            sma200: s.sma200.map(round2),
            rsi14: s.rsi14.map(round2),
            atr14: s.atr14.map(round2),
            volume_avg20: s.volume_avg20.map(f64::round),
            high_52w: round2(s.high_52w),
            low_52w: round2(s.low_52w),
        }
    }
}

/// A support or resistance band.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneDto {
    pub low: f64,
    pub high: f64,
    pub touches: usize,
    pub kind: ZoneKindDto,
}

impl From<&Zone> for ZoneDto {
    fn from(z: &Zone) -> Self {
        Self {
            low: round2(z.low),
            high: round2(z.high),
            touches: z.touches,
            kind: z.kind.into(),
        }
    }
}

/// Last [`SPARKLINE_LEN`] closes, oldest first.
pub fn sparkline(candles: &[Candle]) -> Vec<f64> {
    candles[candles.len().saturating_sub(SPARKLINE_LEN)..]
        .iter()
        .map(|c| round2(c.close))
        .collect()
}

/// Pairs indicator values with bar times, dropping bars without a value.
pub fn overlay(candles: &[Candle], values: &[Option<f64>]) -> Vec<Point> {
    candles
        .iter()
        .zip(values)
        .filter_map(|(c, v)| {
            v.filter(|v| v.is_finite()).map(|v| Point {
                time: c.ts,
                value: round2(v),
            })
        })
        .collect()
}

/// Snapshot with only price-derived fields, for series too short to analyse.
fn plain_snapshot(candles: &[Candle]) -> SnapshotDto {
    let year = &candles[candles.len().saturating_sub(BARS_52W)..];
    SnapshotDto {
        close: candles.last().map_or(0.0, |c| round2(c.close)),
        sma20: None,
        sma50: None,
        sma200: None,
        rsi14: None,
        atr14: None,
        volume_avg20: None,
        high_52w: round2(year.iter().map(|c| c.high).fold(0.0, f64::max)),
        low_52w: round2(year.iter().map(|c| c.low).reduce(f64::min).unwrap_or(0.0)),
    }
}

/// A best-effort quote when the provider served candles but no quote.
pub fn quote_from_candles(symbol: &str, candles: &[Candle]) -> Quote {
    let last = candles.last().copied();
    let price = last.map_or(0.0, |c| c.close);
    let prev_close = candles
        .len()
        .checked_sub(2)
        .map(|i| candles[i].close)
        .unwrap_or(price);
    let snapshot = plain_snapshot(candles);
    let change = price - prev_close;
    Quote {
        symbol: symbol.to_owned(),
        name: symbol.to_owned(),
        exchange: "NSE".to_owned(),
        price,
        change,
        change_pct: if prev_close > 0.0 {
            change / prev_close * 100.0
        } else {
            0.0
        },
        day_high: last.map_or(price, |c| c.high),
        day_low: last.map_or(price, |c| c.low),
        high_52w: snapshot.high_52w,
        low_52w: snapshot.low_52w,
        volume: last.map_or(0.0, |c| c.volume),
        updated_at: None,
    }
}

/// Rounds to two decimals (paise).
pub fn round2(v: f64) -> f64 {
    if v.is_finite() {
        (v * 100.0).round() / 100.0
    } else {
        0.0
    }
}

fn whole(v: f64) -> u64 {
    if v.is_finite() && v > 0.0 {
        v.round() as u64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use swing_core::TradePlan;

    fn setup() -> Setup {
        Setup {
            preset: Preset::SupportBounce,
            state: SetupState::LowRiskReward,
            plan: TradePlan {
                entry: 2061.5000000000002,
                stop: 1985.0,
                target: 2210.0,
                risk_per_share: 76.5,
                reward_per_share: 148.5,
                risk_reward: 1.94,
                quantity: 65,
                target_from_zone: true,
            },
            score: ScoreBreakdown {
                trend: 30.0,
                closeness: 22.0,
                volume: 10.0,
                risk_reward: 9.7,
                quality: 0.0,
            },
            reasons: vec!["RSI(14) is 48".to_owned()],
        }
    }

    fn candle(ts: i64, close: f64) -> Candle {
        Candle::new(ts, close, close + 1.0, close - 1.0, close, 1000.0)
    }

    #[test]
    fn enums_serialize_as_snake_case() {
        assert_eq!(json!(PresetDto::from(Preset::Pullback)), json!("pullback"));
        assert_eq!(json!(PresetDto::from(Preset::Breakout)), json!("breakout"));
        assert_eq!(
            json!(PresetDto::from(Preset::SupportBounce)),
            json!("support_bounce")
        );
        assert_eq!(json!(PresetDto::from(Preset::Oversold)), json!("oversold"));
        assert_eq!(
            json!(SetupStateDto::from(SetupState::Watching)),
            json!("watching")
        );
        assert_eq!(
            json!(SetupStateDto::from(SetupState::LowRiskReward)),
            json!("low_risk_reward")
        );
        assert_eq!(
            json!(ZoneKindDto::from(ZoneKind::Support)),
            json!("support")
        );
        assert_eq!(
            json!(ZoneKindDto::from(ZoneKind::Resistance)),
            json!("resistance")
        );
    }

    #[test]
    fn setup_detail_matches_contract() {
        let v = serde_json::to_value(SetupDetail::from(&setup())).unwrap();
        assert_eq!(
            v,
            json!({
                "preset": "support_bounce", "label": "Support bounce", "state": "low_risk_reward",
                "score": 72, "entry": 2061.5, "stop": 1985.0, "target": 2210.0,
                "risk_reward": 1.94, "quantity": 65,
                "risk_per_share": 76.5, "reward_per_share": 148.5, "target_from_zone": true,
                "score_breakdown": { "trend": 30.0, "closeness": 22.0, "volume": 10.0, "risk_reward": 9.7, "quality": 0.0 },
                "reasons": ["RSI(14) is 48"]
            })
        );
        let summary = serde_json::to_value(SetupSummary::from(&setup())).unwrap();
        assert_eq!(summary.as_object().unwrap().len(), 9);
        assert!(serde_json::to_string(&SetupSummary::from(&setup()))
            .unwrap()
            .contains("\"entry\":2061.5,"));
    }

    #[test]
    fn overlays_drop_missing_values() {
        let candles = [candle(1, 10.0), candle(2, 11.0), candle(3, 12.0)];
        let points = overlay(&candles, &[None, Some(10.5), Some(f64::NAN)]);
        assert_eq!(
            points,
            vec![Point {
                time: 2,
                value: 10.5
            }]
        );
    }

    #[test]
    fn sparkline_is_last_30_closes() {
        let candles: Vec<Candle> = (0..40).map(|i| candle(i, i as f64)).collect();
        let s = sparkline(&candles);
        assert_eq!(s.len(), 30);
        assert_eq!(s[0], 10.0);
        assert_eq!(s[29], 39.0);
        assert_eq!(sparkline(&candles[..3]), vec![0.0, 1.0, 2.0]);
        assert!(sparkline(&[]).is_empty());
    }

    #[test]
    fn list_item_matches_contract() {
        let quote = Quote {
            symbol: "COFORGE".into(),
            name: "Coforge".into(),
            exchange: "NSE".into(),
            price: 2047.0,
            change: 2.0,
            change_pct: 0.1,
            day_high: 2060.0,
            day_low: 2030.0,
            high_52w: 2127.9,
            low_52w: 1008.1,
            volume: 1_281_105.0,
            updated_at: None,
        };
        let item = StockItem::new(
            &quote,
            vec![1990.1, 2047.0],
            Some(SetupSummary::from(&setup())),
        );
        let v = serde_json::to_value(&item).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        let mut expected = vec![
            "symbol",
            "name",
            "exchange",
            "price",
            "change",
            "change_pct",
            "high_52w",
            "low_52w",
            "volume",
            "sparkline",
            "setup",
        ];
        expected.sort_unstable();
        let mut keys_sorted = keys.clone();
        keys_sorted.sort_unstable();
        assert_eq!(keys_sorted, expected);
        assert_eq!(v["volume"], json!(1_281_105));

        let none = serde_json::to_value(StockItem::new(&quote, vec![], None)).unwrap();
        assert!(none["setup"].is_null());

        let q = serde_json::to_value(QuoteDto::from(&quote)).unwrap();
        assert!(q["updated_at"].is_null());
        assert_eq!(q["day_high"], json!(2060.0));
    }

    #[test]
    fn detail_without_analysis_has_plain_snapshot() {
        let candles: Vec<Candle> = (0..5).map(|i| candle(i, 100.0 + i as f64)).collect();
        let quote = quote_from_candles("NEW", &candles);
        assert_eq!(quote.price, 104.0);
        assert_eq!(quote.change, 1.0);
        assert_eq!(quote.high_52w, 105.0);
        assert_eq!(quote.low_52w, 99.0);

        let resp = StockDetailResponse::build("demo", &quote, &candles, None);
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["provider"], "demo");
        assert_eq!(v["candles"].as_array().unwrap().len(), 5);
        assert_eq!(v["candles"][0]["time"], 0);
        assert_eq!(v["overlays"]["sma20"], json!([]));
        assert!(v["snapshot"]["rsi14"].is_null());
        assert_eq!(v["snapshot"]["close"], json!(104.0));
        assert_eq!(v["zones"], json!([]));
        assert!(v["setup"].is_null());
    }

    #[test]
    fn detail_with_analysis() {
        let candles: Vec<Candle> = (0..260)
            .map(|i| {
                let close = 100.0 + (i as f64 / 7.0).sin() * 10.0 + i as f64 * 0.2;
                Candle::new(
                    i * 86_400,
                    close - 0.5,
                    close + 1.5,
                    close - 1.5,
                    close,
                    1e5,
                )
            })
            .collect();
        let analysis = swing_core::analyze(&candles, &swing_core::AnalysisParams::default())
            .expect("enough history");
        let quote = quote_from_candles("X", &candles);
        let v = serde_json::to_value(StockDetailResponse::build(
            "demo",
            &quote,
            &candles,
            Some(&analysis),
        ))
        .unwrap();
        assert_eq!(v["overlays"]["sma20"].as_array().unwrap().len(), 260 - 19);
        assert_eq!(v["overlays"]["sma200"].as_array().unwrap().len(), 260 - 199);
        assert_eq!(v["overlays"]["sma20"][0]["time"], json!(19 * 86_400));
        assert!(v["snapshot"]["sma20"].is_number());
        for z in v["zones"].as_array().unwrap() {
            assert!(z["kind"] == "support" || z["kind"] == "resistance");
        }
    }
}
