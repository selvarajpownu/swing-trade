//! Setup presets and the 0-100 setup score.

use crate::indicators::rolling_high;
use crate::plan::{self, TradePlan};
use crate::zones::{nearest_support, Zone};
use crate::{AnalysisParams, Candle, Snapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Uptrend, price back near the 20/50-day average, RSI cooled off.
    Pullback,
    /// Close above the 55-day high on heavy volume.
    Breakout,
    /// Price at a support zone with a bullish reversal bar.
    SupportBounce,
    /// RSI under 30 (only meaningful for quality stocks; filtered upstream).
    Oversold,
}

impl Preset {
    pub fn label(&self) -> &'static str {
        match self {
            Preset::Pullback => "Pullback",
            Preset::Breakout => "Breakout",
            Preset::SupportBounce => "Support bounce",
            Preset::Oversold => "Oversold",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupState {
    /// Waiting for price to cross the entry.
    Watching,
    /// Risk-reward below the minimum: shown greyed out.
    LowRiskReward,
}

/// Points per part; max 30 + 30 + 15 + 15 + 10 = 100.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ScoreBreakdown {
    pub trend: f64,
    pub closeness: f64,
    pub volume: f64,
    pub risk_reward: f64,
    /// Filled in by the caller from fundamentals (Screener CSV score).
    pub quality: f64,
}

impl ScoreBreakdown {
    pub fn total(&self) -> f64 {
        (self.trend + self.closeness + self.volume + self.risk_reward + self.quality).round()
    }
}

#[derive(Debug, Clone)]
pub struct Setup {
    pub preset: Preset,
    pub state: SetupState,
    pub plan: TradePlan,
    pub score: ScoreBreakdown,
    /// Plain-language reasons, shown on the card and fed to the AI explainer.
    pub reasons: Vec<String>,
}

/// Try each preset on the latest bar and keep the highest-scoring hit.
pub fn detect(
    candles: &[Candle],
    snap: &Snapshot,
    rsi: &[Option<f64>],
    zones: &[Zone],
    params: &AnalysisParams,
) -> Option<Setup> {
    detect_all(candles, snap, rsi, zones, params)
        .into_iter()
        .next()
}

/// Every preset that fires on the latest bar, best score first.
pub fn detect_all(
    candles: &[Candle],
    snap: &Snapshot,
    rsi: &[Option<f64>],
    zones: &[Zone],
    params: &AnalysisParams,
) -> Vec<Setup> {
    let (Some(atr), Some(&last)) = (snap.atr14, candles.last()) else {
        return Vec::new();
    };
    if candles.len() < 2 {
        return Vec::new();
    }
    let trend = trend_points(snap);

    let mut hits: Vec<(Preset, f64, f64, Vec<String>)> = Vec::new(); // preset, closeness, volume, reasons

    // Pullback in an uptrend.
    if let (Some(s20), Some(s50)) = (snap.sma20, snap.sma50) {
        let uptrend = last.close > s50 && snap.sma200.is_none_or(|s200| s50 > s200);
        let near = [s20, s50]
            .iter()
            .map(|s| (last.close - s) / s)
            .fold(f64::MAX, |a, d| a.min(d.abs()));
        let rsi_ok = snap.rsi14.is_some_and(|r| (40.0..=55.0).contains(&r));
        if uptrend && near <= 0.03 && rsi_ok {
            let closeness = 30.0 * (1.0 - near / 0.03);
            let quiet = volume_ratio(snap, &last)
                .map_or(0.0, |v| {
                    if v < 1.0 {
                        15.0 * (1.0 - v).min(1.0) + 5.0
                    } else {
                        0.0
                    }
                })
                .min(15.0);
            hits.push((
                Preset::Pullback,
                closeness,
                quiet,
                vec![
                    format!(
                        "Price is {:.1}% from its 20/50-day average in an uptrend",
                        near * 100.0
                    ),
                    format!("RSI(14) is {:.0}", snap.rsi14.unwrap_or_default()),
                ],
            ));
        }
    }

    // Breakout above the 55-day high (excluding today).
    if candles.len() > 56 {
        let prior_high = rolling_high(&candles[..candles.len() - 1], 55)
            .last()
            .copied()
            .flatten();
        if let (Some(h55), Some(v)) = (prior_high, volume_ratio(snap, &last)) {
            if last.close > h55 && v >= 1.5 {
                let base_low = candles[candles.len() - 56..candles.len() - 1]
                    .iter()
                    .map(|c| c.low)
                    .fold(f64::MAX, f64::min);
                let tight_base = (h55 - base_low) / h55 <= 0.15;
                let closeness = if tight_base { 30.0 } else { 18.0 };
                hits.push((
                    Preset::Breakout,
                    closeness,
                    (5.0 * v).min(15.0),
                    vec![format!(
                        "Closed above the 55-day high on {:.1}x average volume",
                        v
                    )],
                ));
            }
        }
    }

    // Bounce at support with a bullish reversal bar.
    if let Some(sup) = nearest_support(zones, last.close) {
        let dist = (last.close - sup.mid()) / last.close;
        let prev = candles[candles.len() - 2];
        let hammer = last.is_bullish()
            && (last.open.min(last.close) - last.low) >= 2.0 * last.body().max(1e-9);
        let engulfing = last.is_bullish()
            && !prev.is_bullish()
            && last.close >= prev.open
            && last.open <= prev.close;
        if dist <= 0.02 && (hammer || engulfing) {
            hits.push((
                Preset::SupportBounce,
                30.0 * (1.0 - dist / 0.02),
                volume_ratio(snap, &last).map_or(0.0, |v| (7.5 * v).min(15.0)),
                vec![format!(
                    "{} at a support zone touched {} times",
                    if hammer {
                        "Hammer"
                    } else {
                        "Bullish engulfing bar"
                    },
                    sup.touches
                )],
            ));
        }
    }

    // Oversold.
    if let Some(r) = rsi.last().copied().flatten() {
        if r < 30.0 {
            hits.push((
                Preset::Oversold,
                30.0 * ((30.0 - r) / 30.0 + 0.3).min(1.0),
                5.0,
                vec![format!("RSI(14) is {:.0}, deeply oversold", r)],
            ));
        }
    }

    let mut setups: Vec<Setup> = hits
        .into_iter()
        .filter_map(|(preset, closeness, volume, mut reasons)| {
            let plan = plan::build(last.high, atr, zones, &params.plan)?;
            let rr_points = (plan.risk_reward / 3.0).clamp(0.0, 1.0) * 15.0;
            let state = if plan.risk_reward >= params.min_risk_reward {
                SetupState::Watching
            } else {
                SetupState::LowRiskReward
            };
            reasons.push(format!("Risk-reward {:.2}", plan.risk_reward));
            Some(Setup {
                preset,
                state,
                plan,
                score: ScoreBreakdown {
                    trend,
                    closeness,
                    volume,
                    risk_reward: rr_points,
                    quality: 0.0,
                },
                reasons,
            })
        })
        .collect();
    // Stable sort: on equal scores the preset order above decides.
    setups.sort_by(|a, b| b.score.total().total_cmp(&a.score.total()));
    setups
}

/// 0-30: 10 points each for close > 20-day, 20 > 50, 50 > 200.
fn trend_points(s: &Snapshot) -> f64 {
    let mut p = 0.0;
    if s.sma20.is_some_and(|v| s.close > v) {
        p += 10.0
    }
    if let (Some(a), Some(b)) = (s.sma20, s.sma50) {
        if a > b {
            p += 10.0
        }
    }
    if let (Some(a), Some(b)) = (s.sma50, s.sma200) {
        if a > b {
            p += 10.0
        }
    }
    p
}

fn volume_ratio(s: &Snapshot, last: &Candle) -> Option<f64> {
    s.volume_avg20
        .filter(|v| *v > 0.0)
        .map(|avg| last.volume / avg)
}
