//! Stop-loss / entry / target card and position sizing.

use crate::zones::{nearest_resistance, nearest_support, Zone};

#[derive(Debug, Clone)]
pub struct PlanParams {
    /// Account capital in rupees.
    pub capital: f64,
    /// Fraction of capital risked per trade (0.01 = 1%).
    pub risk_per_trade: f64,
    /// Entry trigger sits this fraction above the setup bar's high.
    pub entry_buffer: f64,
    /// The stop is never closer to entry than this many ATRs.
    pub min_stop_atr: f64,
    /// Target used when no resistance zone is in reach, in multiples of risk.
    pub fallback_reward_multiple: f64,
}

impl Default for PlanParams {
    fn default() -> Self {
        Self {
            capital: 500_000.0,
            risk_per_trade: 0.01,
            entry_buffer: 0.001,
            min_stop_atr: 1.0,
            fallback_reward_multiple: 2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TradePlan {
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub risk_per_share: f64,
    pub reward_per_share: f64,
    pub risk_reward: f64,
    pub quantity: u64,
    /// True when the target came from a resistance zone rather than the fallback multiple.
    pub target_from_zone: bool,
}

/// Build a long trade plan from the setup bar's high, the ATR and the zones.
///
/// Returns `None` when the numbers make no sense (e.g. zero ATR).
// `!(x > 0.0)` is deliberate: it also rejects NaN.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn build(
    trigger_high: f64,
    atr: f64,
    zones: &[Zone],
    params: &PlanParams,
) -> Option<TradePlan> {
    if !(atr > 0.0) || !(trigger_high > 0.0) {
        return None;
    }
    let entry = round_tick(trigger_high * (1.0 + params.entry_buffer));

    let min_stop_distance = params.min_stop_atr * atr;
    let structural_stop = nearest_support(zones, entry).map(|z| z.low - 0.1 * atr);
    let stop = match structural_stop {
        Some(s) if entry - s >= min_stop_distance => s,
        _ => entry - min_stop_distance,
    };
    let stop = round_tick(stop);
    let risk = entry - stop;
    if risk <= 0.0 {
        return None;
    }

    let fallback = entry + params.fallback_reward_multiple * risk;
    let (target, from_zone) = match nearest_resistance(zones, entry) {
        Some(z) if z.low - entry >= risk => (z.low, true),
        _ => (fallback, false),
    };
    let target = round_tick(target);
    let reward = target - entry;

    let quantity = (params.capital * params.risk_per_trade / risk)
        .floor()
        .max(0.0) as u64;

    Some(TradePlan {
        entry,
        stop,
        target,
        risk_per_share: round2(risk),
        reward_per_share: round2(reward),
        risk_reward: round2(reward / risk),
        quantity,
        target_from_zone: from_zone,
    })
}

/// NSE equity tick size is Rs 0.05 for most stocks.
pub fn round_tick(price: f64) -> f64 {
    (price / 0.05).round() * 0.05
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
