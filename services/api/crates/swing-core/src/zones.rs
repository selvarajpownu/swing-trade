//! Support and resistance zones from clustered swing highs and lows.

use crate::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneKind {
    Support,
    Resistance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub low: f64,
    pub high: f64,
    /// Number of swing points that formed this zone.
    pub touches: usize,
    /// Unix seconds of the most recent touch.
    pub last_touch: i64,
    /// Relative to the latest close.
    pub kind: ZoneKind,
}

impl Zone {
    pub fn mid(&self) -> f64 {
        (self.low + self.high) / 2.0
    }
}

/// A local extreme: a bar whose high (or low) beats `window` bars on each side.
#[derive(Debug, Clone, Copy)]
struct Pivot {
    price: f64,
    ts: i64,
}

fn pivots(candles: &[Candle], window: usize) -> Vec<Pivot> {
    let mut out = Vec::new();
    if window == 0 || candles.len() < 2 * window + 1 {
        return out;
    }
    for i in window..candles.len() - window {
        let left = &candles[i - window..i];
        let right = &candles[i + 1..=i + window];
        let c = &candles[i];
        if left.iter().chain(right).all(|o| o.high < c.high) {
            out.push(Pivot {
                price: c.high,
                ts: c.ts,
            });
        }
        if left.iter().chain(right).all(|o| o.low > c.low) {
            out.push(Pivot {
                price: c.low,
                ts: c.ts,
            });
        }
    }
    out
}

/// Group swing points that sit within `merge_distance` of each other.
///
/// Zones are returned nearest-to-price first, labelled support (below the
/// latest close) or resistance (above it). The 52-week high is always
/// included as a resistance level once price is below it.
pub fn find_zones(candles: &[Candle], window: usize, merge_distance: f64) -> Vec<Zone> {
    let Some(last) = candles.last() else {
        return Vec::new();
    };
    let mut points = pivots(candles, window);
    points.sort_by(|a, b| a.price.total_cmp(&b.price));

    let mut zones: Vec<Zone> = Vec::new();
    for p in points {
        match zones.last_mut() {
            Some(z) if p.price - z.high <= merge_distance => {
                z.high = z.high.max(p.price);
                z.low = z.low.min(p.price);
                z.touches += 1;
                z.last_touch = z.last_touch.max(p.ts);
            }
            _ => zones.push(Zone {
                low: p.price,
                high: p.price,
                touches: 1,
                last_touch: p.ts,
                kind: ZoneKind::Support,
            }),
        }
    }

    let year_start = candles.len().saturating_sub(250);
    let high_52w = candles[year_start..]
        .iter()
        .map(|c| c.high)
        .fold(f64::MIN, f64::max);
    if high_52w > last.close
        && !zones
            .iter()
            .any(|z| high_52w >= z.low - merge_distance && high_52w <= z.high + merge_distance)
    {
        zones.push(Zone {
            low: high_52w,
            high: high_52w,
            touches: 1,
            last_touch: last.ts,
            kind: ZoneKind::Resistance,
        });
    }

    for z in &mut zones {
        z.kind = if z.mid() <= last.close {
            ZoneKind::Support
        } else {
            ZoneKind::Resistance
        };
    }
    zones.sort_by(|a, b| {
        (a.mid() - last.close)
            .abs()
            .total_cmp(&(b.mid() - last.close).abs())
    });
    zones
}

/// Nearest support zone at or below `price`.
pub fn nearest_support(zones: &[Zone], price: f64) -> Option<&Zone> {
    zones
        .iter()
        .filter(|z| z.kind == ZoneKind::Support && z.mid() <= price)
        .min_by(|a, b| (price - a.mid()).total_cmp(&(price - b.mid())))
}

/// Nearest resistance zone above `price`.
pub fn nearest_resistance(zones: &[Zone], price: f64) -> Option<&Zone> {
    zones
        .iter()
        .filter(|z| z.kind == ZoneKind::Resistance && z.low > price)
        .min_by(|a, b| (a.low - price).total_cmp(&(b.low - price)))
}
