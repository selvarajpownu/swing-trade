//! Classic indicators. Every function returns one value per input bar,
//! `None` until enough history exists, so outputs line up with candles.

use crate::Candle;

/// Simple moving average.
pub fn sma(values: &[f64], period: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; values.len()];
    if period == 0 || values.len() < period {
        return out;
    }
    let mut sum: f64 = values[..period].iter().sum();
    out[period - 1] = Some(sum / period as f64);
    for i in period..values.len() {
        sum += values[i] - values[i - period];
        out[i] = Some(sum / period as f64);
    }
    out
}

/// Exponential moving average, seeded with the SMA of the first `period` values.
pub fn ema(values: &[f64], period: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; values.len()];
    if period == 0 || values.len() < period {
        return out;
    }
    let k = 2.0 / (period as f64 + 1.0);
    let mut prev = values[..period].iter().sum::<f64>() / period as f64;
    out[period - 1] = Some(prev);
    for i in period..values.len() {
        prev = values[i] * k + prev * (1.0 - k);
        out[i] = Some(prev);
    }
    out
}

/// Relative Strength Index with Wilder smoothing.
pub fn rsi(closes: &[f64], period: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; closes.len()];
    if period == 0 || closes.len() <= period {
        return out;
    }
    let (mut gain, mut loss) = (0.0, 0.0);
    for i in 1..=period {
        let d = closes[i] - closes[i - 1];
        if d >= 0.0 {
            gain += d
        } else {
            loss -= d
        }
    }
    let p = period as f64;
    let (mut avg_gain, mut avg_loss) = (gain / p, loss / p);
    out[period] = Some(rsi_value(avg_gain, avg_loss));
    for i in period + 1..closes.len() {
        let d = closes[i] - closes[i - 1];
        let (g, l) = if d >= 0.0 { (d, 0.0) } else { (0.0, -d) };
        avg_gain = (avg_gain * (p - 1.0) + g) / p;
        avg_loss = (avg_loss * (p - 1.0) + l) / p;
        out[i] = Some(rsi_value(avg_gain, avg_loss));
    }
    out
}

fn rsi_value(avg_gain: f64, avg_loss: f64) -> f64 {
    if avg_loss == 0.0 {
        return if avg_gain == 0.0 { 50.0 } else { 100.0 };
    }
    100.0 - 100.0 / (1.0 + avg_gain / avg_loss)
}

/// Average True Range with Wilder smoothing.
pub fn atr(candles: &[Candle], period: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; candles.len()];
    if period == 0 || candles.len() < period {
        return out;
    }
    let tr: Vec<f64> = candles
        .iter()
        .enumerate()
        .map(|(i, c)| {
            c.true_range(if i == 0 {
                None
            } else {
                Some(candles[i - 1].close)
            })
        })
        .collect();
    let p = period as f64;
    let mut prev = tr[..period].iter().sum::<f64>() / p;
    out[period - 1] = Some(prev);
    for i in period..candles.len() {
        prev = (prev * (p - 1.0) + tr[i]) / p;
        out[i] = Some(prev);
    }
    out
}

/// Highest high of the `period` bars ending at each index.
pub fn rolling_high(candles: &[Candle], period: usize) -> Vec<Option<f64>> {
    rolling(candles, period, |c| c.high, f64::max, f64::MIN)
}

/// Lowest low of the `period` bars ending at each index.
pub fn rolling_low(candles: &[Candle], period: usize) -> Vec<Option<f64>> {
    rolling(candles, period, |c| c.low, f64::min, f64::MAX)
}

fn rolling(
    candles: &[Candle],
    period: usize,
    pick: impl Fn(&Candle) -> f64,
    fold: impl Fn(f64, f64) -> f64,
    init: f64,
) -> Vec<Option<f64>> {
    (0..candles.len())
        .map(|i| {
            if period == 0 || i + 1 < period {
                None
            } else {
                Some(
                    candles[i + 1 - period..=i]
                        .iter()
                        .map(&pick)
                        .fold(init, &fold),
                )
            }
        })
        .collect()
}
