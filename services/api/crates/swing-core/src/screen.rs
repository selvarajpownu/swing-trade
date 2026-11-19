//! Cross-sectional swing screen over a whole universe of daily candles.
//!
//! [`analyze`](crate::analyze) looks at one stock at a time. This module
//! compares stocks with each other: relative-strength rank, market breadth,
//! sector strength, and the gates and setups of the "Swing Stocks" page.
//!
//! Gates (each a pass/fail the UI can switch on or off):
//! 1. **Tradable**: main-board series and enough daily turnover.
//! 2. **Leader**: relative strength in the top of the universe, price above
//!    rising averages, near its 52-week high.
//! 3. **Sector**: the stock's industry is in the stronger half.
//!
//! Setups on the latest bar: [`SwingKind::Breakout`] and
//! [`SwingKind::Pullback`]. Every threshold is a starting value to be
//! back-tested, not a proven rule.

use std::collections::HashMap;

use crate::indicators::{atr, ema, sma};
use crate::plan::round_tick;
use crate::Candle;

/// Bars in roughly one, three and six months, and one year.
const BARS_1M: usize = 21;
const BARS_3M: usize = 63;
const BARS_6M: usize = 126;
const BARS_52W: usize = 250;
/// Minimum history for a row.
const MIN_BARS: usize = 30;
/// Industries with fewer ranked members are not ranked.
const MIN_SECTOR_MEMBERS: usize = 3;

/// One stock's daily history, borrowed from the caller.
#[derive(Debug, Clone)]
pub struct ScreenInput<'a> {
    pub symbol: &'a str,
    pub industry: Option<&'a str>,
    /// Daily candles, oldest first.
    pub candles: &'a [Candle],
    /// Delivery percentage per bar, parallel to `candles`; empty when unknown.
    pub delivery: &'a [Option<f64>],
    /// False for trade-to-trade and other restricted series.
    pub main_series: bool,
}

/// Thresholds of the screen. Defaults are the documented starting values.
#[derive(Debug, Clone)]
pub struct ScreenParams {
    /// Minimum median 20-day turnover (close × volume), in currency units.
    pub min_turnover: f64,
    /// Minimum relative-strength rating (0-100) for a leader.
    pub leader_min_rs: f64,
    /// A leader trades within this fraction of its 52-week high.
    pub leader_max_below_high: f64,
    /// Breakout: bars of the prior high the close must clear.
    pub breakout_lookback: usize,
    /// Breakout: volume against the prior 20-day average.
    pub breakout_min_volume: f64,
    /// Breakout: largest high-low range of the base, as a fraction of its high.
    pub breakout_max_base: f64,
    /// Breakout: a close further than this above the level is "extended".
    pub breakout_max_extension: f64,
    /// Pullback: depth from the swing high, as fractions.
    pub pullback_min_depth: f64,
    pub pullback_max_depth: f64,
    /// Pullback: bars since the swing high.
    pub pullback_min_bars: usize,
    pub pullback_max_bars: usize,
    /// Stops are never closer to entry than this many ATRs.
    pub min_stop_atr: f64,
    /// Entry sits this fraction above the trigger bar's high.
    pub entry_buffer: f64,
    pub capital: f64,
    /// Fraction of capital risked per trade.
    pub risk_per_trade: f64,
}

impl Default for ScreenParams {
    fn default() -> Self {
        Self {
            min_turnover: 1.0e8, // Rs 10 crore
            leader_min_rs: 70.0,
            leader_max_below_high: 0.25,
            breakout_lookback: 20,
            breakout_min_volume: 1.5,
            breakout_max_base: 0.15,
            breakout_max_extension: 0.05,
            pullback_min_depth: 0.03,
            pullback_max_depth: 0.10,
            pullback_min_bars: 3,
            pullback_max_bars: 10,
            min_stop_atr: 1.0,
            entry_buffer: 0.001,
            capital: 500_000.0,
            risk_per_trade: 0.01,
        }
    }
}

/// Share of the universe above its moving averages, in percent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Breadth {
    /// Stocks with enough history to be counted.
    pub universe: usize,
    pub above_sma50_pct: f64,
    pub above_sma200_pct: f64,
}

/// How friendly the market is to new long swing trades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegimeState {
    /// Index above its 50- and 200-day averages with healthy breadth.
    Normal,
    /// Index below its 50-day average, or breadth is weak.
    Caution,
    /// Index below its 200-day average.
    Defensive,
}

impl RegimeState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RegimeState::Normal => "normal",
            RegimeState::Caution => "caution",
            RegimeState::Defensive => "defensive",
        }
    }
}

/// Market regime from a benchmark index and the universe's breadth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Regime {
    pub state: RegimeState,
    pub index_close: Option<f64>,
    pub index_sma50: Option<f64>,
    pub index_sma200: Option<f64>,
}

/// Breadth below this share above the 50-day average counts as weak.
const WEAK_BREADTH_PCT: f64 = 40.0;

/// Classifies the market. Without index closes only breadth is used.
pub fn regime(index_closes: Option<&[f64]>, breadth: &Breadth) -> Regime {
    let closes = index_closes.unwrap_or_default();
    let close = closes.last().copied();
    let sma50 = sma(closes, 50).last().copied().flatten();
    let sma200 = sma(closes, 200).last().copied().flatten();
    let weak = breadth.universe > 0 && breadth.above_sma50_pct < WEAK_BREADTH_PCT;
    let state = match (close, sma50, sma200) {
        (Some(c), _, Some(s200)) if c < s200 => RegimeState::Defensive,
        (Some(c), Some(s50), _) if c < s50 => RegimeState::Caution,
        (Some(_), _, _) if weak => RegimeState::Caution,
        (Some(_), _, _) => RegimeState::Normal,
        (None, _, _) if breadth.universe > 0 && breadth.above_sma200_pct < WEAK_BREADTH_PCT => {
            RegimeState::Defensive
        }
        (None, _, _) if weak => RegimeState::Caution,
        (None, _, _) => RegimeState::Normal,
    };
    Regime {
        state,
        index_close: close,
        index_sma50: sma50,
        index_sma200: sma200,
    }
}

/// Median three-month return of an industry's members.
#[derive(Debug, Clone, PartialEq)]
pub struct SectorStrength {
    pub industry: String,
    pub members: usize,
    /// Median 3-month return of the members, in percent.
    pub ret_3m_pct: f64,
    /// 1 is the strongest industry.
    pub rank: usize,
    /// In the stronger half.
    pub strong: bool,
}

/// Pass/fail of each gate for one stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gates {
    pub tradable: bool,
    pub leader: bool,
    /// `None` when the industry is unknown or too small to rank.
    pub sector: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwingKind {
    /// Close above the prior 20-day high on heavy volume, from a tight base.
    Breakout,
    /// A dip in an uptrend that touched its averages and turned up.
    Pullback,
}

impl SwingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SwingKind::Breakout => "breakout",
            SwingKind::Pullback => "pullback",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SwingKind::Breakout => "Breakout",
            SwingKind::Pullback => "Pullback",
        }
    }
}

/// Entry, stop and two R-multiple targets for a long swing trade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwingPlan {
    pub entry: f64,
    pub stop: f64,
    pub risk_per_share: f64,
    /// Entry + 2R.
    pub target_2r: f64,
    /// Entry + 3R.
    pub target_3r: f64,
    pub quantity: u64,
}

/// Points per part; the maximum is 30 + 15 + 30 + 10 + 5 + 10 = 100.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SwingScore {
    /// Relative strength, 0-30.
    pub rs: f64,
    /// Price above rising averages, 0-15.
    pub trend: f64,
    /// Quality of the setup itself, 0-30.
    pub setup: f64,
    /// Industry strength, 0-10.
    pub sector: f64,
    /// Delivery-volume confirmation, 0-5.
    pub delivery: f64,
    /// Business quality, 0-10. Stays 0 until fundamentals are imported.
    pub quality: f64,
}

impl SwingScore {
    pub fn total(&self) -> f64 {
        (self.rs + self.trend + self.setup + self.sector + self.delivery + self.quality).round()
    }
}

/// A setup firing on the latest bar.
#[derive(Debug, Clone, PartialEq)]
pub struct SwingSetup {
    pub kind: SwingKind,
    /// Breakout level, or the swing high a pullback came from.
    pub level: f64,
    /// Close above the breakout level, in percent (0 for pullbacks).
    pub extension_pct: f64,
    /// Latest volume against the prior 20-day average.
    pub volume_ratio: f64,
    pub plan: SwingPlan,
    pub score: SwingScore,
    pub reasons: Vec<String>,
}

/// Everything the screen knows about one stock.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenRow {
    pub symbol: String,
    pub industry: Option<String>,
    pub close: f64,
    pub prev_close: f64,
    pub volume: f64,
    /// Unix seconds of the latest bar.
    pub bar_time: i64,
    /// Median 20-day turnover (close × volume).
    pub turnover: f64,
    pub ret_1m_pct: Option<f64>,
    pub ret_3m_pct: Option<f64>,
    pub ret_6m_pct: Option<f64>,
    /// Relative-strength rating, 0 (weakest) to 100.
    pub rs: Option<f64>,
    /// Distance below the 52-week high, in percent (0 = at the high).
    pub below_high_pct: f64,
    pub sma50: Option<f64>,
    pub sma200: Option<f64>,
    pub atr14: Option<f64>,
    /// Latest delivery percentage and its 20-day average.
    pub delivery_pct: Option<f64>,
    pub delivery_avg20: Option<f64>,
    pub gates: Gates,
    /// Setups firing on the latest bar, best score first.
    pub setups: Vec<SwingSetup>,
}

/// Result of [`run`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Screen {
    pub breadth: Breadth,
    /// Strongest industry first.
    pub sectors: Vec<SectorStrength>,
    pub rows: Vec<ScreenRow>,
}

/// Per-stock numbers computed before any cross-sectional step.
struct Base<'a> {
    input: &'a ScreenInput<'a>,
    row: ScreenRow,
    sma200_rising: bool,
    ema20: Option<f64>,
    volume_avg20: Option<f64>,
}

/// Screens the whole universe. Stocks with fewer than 30 bars are skipped.
pub fn run(inputs: &[ScreenInput<'_>], params: &ScreenParams) -> Screen {
    let mut bases: Vec<Base<'_>> = inputs.iter().filter_map(|i| base(i, params)).collect();

    // Relative strength: average percentile rank of 3- and 6-month returns.
    let ret3: Vec<f64> = bases.iter().filter_map(|b| b.row.ret_3m_pct).collect();
    let ret6: Vec<f64> = bases.iter().filter_map(|b| b.row.ret_6m_pct).collect();
    for b in &mut bases {
        let r3 = b.row.ret_3m_pct.map(|v| percentile_rank(&ret3, v));
        let r6 = b.row.ret_6m_pct.map(|v| percentile_rank(&ret6, v));
        b.row.rs = match (r3, r6) {
            (Some(a), Some(c)) => Some((a + c) / 2.0),
            (Some(a), None) => Some(a),
            _ => None,
        };
    }

    let sectors = sector_strength(&bases);
    let sector_of: HashMap<&str, &SectorStrength> =
        sectors.iter().map(|s| (s.industry.as_str(), s)).collect();

    let counted = bases.iter().filter(|b| b.row.sma50.is_some()).count();
    let pct = |n: usize| {
        if counted == 0 {
            0.0
        } else {
            n as f64 / counted as f64 * 100.0
        }
    };
    let breadth = Breadth {
        universe: counted,
        above_sma50_pct: pct(bases
            .iter()
            .filter(|b| b.row.sma50.is_some_and(|s| b.row.close > s))
            .count()),
        above_sma200_pct: pct(bases
            .iter()
            .filter(|b| b.row.sma200.is_some_and(|s| b.row.close > s))
            .count()),
    };

    let rows = bases
        .into_iter()
        .map(|mut b| {
            let sector = b
                .row
                .industry
                .as_deref()
                .and_then(|i| sector_of.get(i).copied());
            b.row.gates.sector = sector.map(|s| s.strong);
            b.row.gates.leader = is_leader(&b, params);
            let common = common_score(&b, sector, sectors.len());
            let mut setups: Vec<SwingSetup> = [breakout(&b, params), pullback(&b, params)]
                .into_iter()
                .flatten()
                .map(|mut s| {
                    s.score = SwingScore {
                        setup: s.score.setup,
                        ..common
                    };
                    s
                })
                .collect();
            setups.sort_by(|a, c| c.score.total().total_cmp(&a.score.total()));
            b.row.setups = setups;
            b.row
        })
        .collect();

    Screen {
        breadth,
        sectors,
        rows,
    }
}

fn base<'a>(input: &'a ScreenInput<'a>, params: &ScreenParams) -> Option<Base<'a>> {
    let c = input.candles;
    let n = c.len();
    if n < MIN_BARS {
        return None;
    }
    let last = c[n - 1];
    let closes: Vec<f64> = c.iter().map(|x| x.close).collect();
    let ret = |bars: usize| {
        (n > bars && closes[n - 1 - bars] > 0.0)
            .then(|| (last.close / closes[n - 1 - bars] - 1.0) * 100.0)
    };
    let sma50 = sma(&closes, 50);
    let sma200 = sma(&closes, 200);
    let sma200_now = sma200[n - 1];
    let sma200_rising = match (sma200_now, n.checked_sub(21).and_then(|i| sma200[i])) {
        (Some(now), Some(before)) => now > before,
        _ => false,
    };
    let year = &c[n.saturating_sub(BARS_52W)..];
    let high_52w = year.iter().map(|x| x.high).fold(f64::MIN, f64::max);

    let mut turnovers: Vec<f64> = c[n - 20..].iter().map(|x| x.close * x.volume).collect();
    turnovers.sort_by(f64::total_cmp);
    let turnover = (turnovers[9] + turnovers[10]) / 2.0;

    // Average volume of the 20 bars before the latest one.
    let volume_avg20 =
        (n >= 21).then(|| c[n - 21..n - 1].iter().map(|x| x.volume).sum::<f64>() / 20.0);

    let delivery_pct = input.delivery.get(n - 1).copied().flatten();
    let recent: Vec<f64> = input
        .delivery
        .iter()
        .rev()
        .skip(1)
        .take(20)
        .filter_map(|d| *d)
        .collect();
    let delivery_avg20 =
        (recent.len() >= 10).then(|| recent.iter().sum::<f64>() / recent.len() as f64);

    Some(Base {
        input,
        sma200_rising,
        ema20: ema(&closes, 20)[n - 1],
        volume_avg20,
        row: ScreenRow {
            symbol: input.symbol.to_owned(),
            industry: input.industry.map(str::to_owned),
            close: last.close,
            prev_close: c[n - 2].close,
            volume: last.volume,
            bar_time: last.ts,
            turnover,
            ret_1m_pct: ret(BARS_1M),
            ret_3m_pct: ret(BARS_3M),
            ret_6m_pct: ret(BARS_6M),
            rs: None,
            below_high_pct: if high_52w > 0.0 {
                ((1.0 - last.close / high_52w) * 100.0).max(0.0)
            } else {
                0.0
            },
            sma50: sma50[n - 1],
            sma200: sma200_now,
            atr14: atr(c, 14)[n - 1],
            delivery_pct,
            delivery_avg20,
            gates: Gates {
                tradable: input.main_series && turnover >= params.min_turnover,
                leader: false,
                sector: None,
            },
            setups: Vec::new(),
        },
    })
}

/// Share of `values` strictly below `x`, 0-100.
pub fn percentile_rank(values: &[f64], x: f64) -> f64 {
    if values.len() < 2 {
        return 50.0;
    }
    let below = values.iter().filter(|v| **v < x).count();
    below as f64 / (values.len() - 1) as f64 * 100.0
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

fn sector_strength(bases: &[Base<'_>]) -> Vec<SectorStrength> {
    let mut by_industry: HashMap<&str, Vec<f64>> = HashMap::new();
    for b in bases {
        if let (Some(industry), Some(r)) = (b.row.industry.as_deref(), b.row.ret_3m_pct) {
            by_industry.entry(industry).or_default().push(r);
        }
    }
    let mut sectors: Vec<SectorStrength> = by_industry
        .into_iter()
        .filter(|(_, v)| v.len() >= MIN_SECTOR_MEMBERS)
        .map(|(industry, mut v)| SectorStrength {
            industry: industry.to_owned(),
            members: v.len(),
            ret_3m_pct: median(&mut v),
            rank: 0,
            strong: false,
        })
        .collect();
    sectors.sort_by(|a, b| {
        b.ret_3m_pct
            .total_cmp(&a.ret_3m_pct)
            .then_with(|| a.industry.cmp(&b.industry))
    });
    let strong = sectors.len().div_ceil(2);
    for (i, s) in sectors.iter_mut().enumerate() {
        s.rank = i + 1;
        s.strong = i < strong;
    }
    sectors
}

fn is_leader(b: &Base<'_>, params: &ScreenParams) -> bool {
    let r = &b.row;
    let (Some(rs), Some(s50), Some(s200)) = (r.rs, r.sma50, r.sma200) else {
        return false;
    };
    rs >= params.leader_min_rs
        && r.close > s50
        && s50 > s200
        && b.sma200_rising
        && r.below_high_pct <= params.leader_max_below_high * 100.0
}

/// The score parts that do not depend on the setup.
fn common_score(b: &Base<'_>, sector: Option<&SectorStrength>, sectors: usize) -> SwingScore {
    let r = &b.row;
    let mut trend = 0.0;
    if r.sma50.is_some_and(|s| r.close > s) {
        trend += 5.0;
    }
    if let (Some(a), Some(c)) = (r.sma50, r.sma200) {
        if a > c {
            trend += 5.0;
        }
    }
    if b.sma200_rising {
        trend += 5.0;
    }
    let sector_points = match sector {
        Some(s) if sectors > 1 => 10.0 * (1.0 - (s.rank - 1) as f64 / (sectors - 1) as f64),
        Some(_) => 10.0,
        None => 5.0,
    };
    // Delivery above its own average on an up day suggests real buying.
    let delivery = match (r.delivery_pct, r.delivery_avg20) {
        (Some(d), Some(avg)) if avg > 0.0 && r.close > r.prev_close => {
            ((d / avg - 1.0) / 0.3).clamp(0.0, 1.0) * 5.0
        }
        _ => 0.0,
    };
    SwingScore {
        rs: r.rs.unwrap_or(0.0) / 100.0 * 30.0,
        trend,
        setup: 0.0,
        sector: sector_points,
        delivery,
        quality: 0.0,
    }
}

/// Where the close sits in the day's range: 0 at the low, 1 at the high.
fn close_position(c: &Candle) -> f64 {
    let range = c.high - c.low;
    if range > 0.0 {
        (c.close - c.low) / range
    } else {
        1.0
    }
}

fn plan(
    entry_high: f64,
    structural_stop: f64,
    atr: f64,
    params: &ScreenParams,
) -> Option<SwingPlan> {
    if !(atr > 0.0 && entry_high > 0.0) {
        return None;
    }
    let entry = round_tick(entry_high * (1.0 + params.entry_buffer));
    let stop = round_tick(structural_stop.min(entry - params.min_stop_atr * atr));
    let risk = entry - stop;
    if risk <= 0.0 || stop <= 0.0 {
        return None;
    }
    Some(SwingPlan {
        entry,
        stop,
        risk_per_share: (risk * 100.0).round() / 100.0,
        target_2r: round_tick(entry + 2.0 * risk),
        target_3r: round_tick(entry + 3.0 * risk),
        quantity: (params.capital * params.risk_per_trade / risk)
            .floor()
            .max(0.0) as u64,
    })
}

fn breakout(b: &Base<'_>, params: &ScreenParams) -> Option<SwingSetup> {
    let c = b.input.candles;
    let n = c.len();
    let look = params.breakout_lookback;
    if n < look + 2 {
        return None;
    }
    let last = c[n - 1];
    let base = &c[n - 1 - look..n - 1];
    let level = base.iter().map(|x| x.high).fold(f64::MIN, f64::max);
    let base_low = base.iter().map(|x| x.low).fold(f64::MAX, f64::min);
    let base_range = (level - base_low) / level;
    let volume_ratio = b
        .volume_avg20
        .filter(|v| *v > 0.0)
        .map(|v| last.volume / v)?;
    let extension = last.close / level - 1.0;
    let position = close_position(&last);
    let atr = b.row.atr14?;

    let fires = last.close > level
        && b.row.sma50.is_some_and(|s| last.close > s)
        && volume_ratio >= params.breakout_min_volume
        && base_range <= params.breakout_max_base
        && position >= 2.0 / 3.0
        && extension <= params.breakout_max_extension;
    if !fires {
        return None;
    }

    // Stop below the breakout bar's low or the level, whichever is lower.
    let plan = plan(last.high, last.low.min(level) - 0.1 * atr, atr, params)?;
    let setup_points = (volume_ratio / 3.0).min(1.0) * 10.0
        + (1.0 - base_range / params.breakout_max_base).clamp(0.0, 1.0) * 10.0
        + position * 5.0
        + (1.0 - extension / params.breakout_max_extension).clamp(0.0, 1.0) * 5.0;
    Some(SwingSetup {
        kind: SwingKind::Breakout,
        level,
        extension_pct: extension * 100.0,
        volume_ratio,
        plan,
        score: SwingScore {
            setup: setup_points,
            ..SwingScore::default()
        },
        reasons: vec![
            format!(
                "Closed {:.1}% above its {look}-day high on {volume_ratio:.1}x average volume",
                extension * 100.0
            ),
            format!(
                "Base before the breakout was {:.0}% deep",
                base_range * 100.0
            ),
        ],
    })
}

fn pullback(b: &Base<'_>, params: &ScreenParams) -> Option<SwingSetup> {
    let c = b.input.candles;
    let n = c.len();
    if n < 40 {
        return None;
    }
    let last = c[n - 1];
    let prev = c[n - 2];
    let (s50, s200, ema20, atr, avg_volume) = (
        b.row.sma50?,
        b.row.sma200?,
        b.ema20?,
        b.row.atr14?,
        b.volume_avg20.filter(|v| *v > 0.0)?,
    );

    // The swing high: highest high of the last `max_bars` bars before today.
    let from = n - 1 - params.pullback_max_bars;
    let (high_idx, swing_high) = c[from..n - 1]
        .iter()
        .enumerate()
        .map(|(i, x)| (from + i, x.high))
        .max_by(|a, d| a.1.total_cmp(&d.1))?;
    let bars_since = n - 1 - high_idx;
    if bars_since < params.pullback_min_bars {
        return None;
    }
    // It must be a real high, not a bounce inside a decline.
    let prior_high = c[n.saturating_sub(31)..from]
        .iter()
        .map(|x| x.high)
        .fold(f64::MIN, f64::max);
    let dip = &c[high_idx + 1..n];
    let dip_low = dip.iter().map(|x| x.low).fold(f64::MAX, f64::min);
    let depth = (swing_high - dip_low) / swing_high;
    let quiet = &c[high_idx + 1..n - 1];
    let dip_volume = if quiet.is_empty() {
        avg_volume
    } else {
        quiet.iter().map(|x| x.volume).sum::<f64>() / quiet.len() as f64
    };
    let dry_up = dip_volume / avg_volume;
    let touched = dip_low <= ema20 * 1.01 || dip_low <= s50 * 1.01;

    let fires = last.close > s200
        && s50 > s200
        && last.close >= s50 * 0.97
        && swing_high >= prior_high * 0.98
        && (params.pullback_min_depth..=params.pullback_max_depth).contains(&depth)
        && touched
        && dry_up < 1.0
        && last.close > prev.high
        && last.is_bullish();
    if !fires {
        return None;
    }

    let plan = plan(last.high, dip_low - 0.1 * atr, atr, params)?;
    let sweet = 0.06;
    let setup_points = (1.0 - (depth - sweet).abs() / sweet).clamp(0.0, 1.0) * 10.0
        + (1.0 - dry_up).clamp(0.0, 1.0) * 10.0
        + close_position(&last) * 5.0
        + if dip_low <= ema20 * 1.01 && dip_low >= ema20 * 0.97 {
            5.0
        } else {
            2.5
        };
    Some(SwingSetup {
        kind: SwingKind::Pullback,
        level: swing_high,
        extension_pct: 0.0,
        volume_ratio: last.volume / avg_volume,
        plan,
        score: SwingScore {
            setup: setup_points,
            ..SwingScore::default()
        },
        reasons: vec![
            format!(
                "Pulled back {:.1}% over {bars_since} days to its 20/50-day average",
                depth * 100.0
            ),
            format!(
                "Volume during the dip was {:.0}% of average; closed above the prior day's high",
                dry_up * 100.0
            ),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    /// `bars` daily candles drifting by `drift` per bar from `start`.
    fn trend(start: f64, drift: f64, bars: usize, volume: f64) -> Vec<Candle> {
        (0..bars)
            .map(|i| {
                let close = start * (1.0 + drift).powi(i as i32);
                Candle::new(
                    i as i64 * DAY,
                    close * 0.995,
                    close * 1.01,
                    close * 0.99,
                    close,
                    volume,
                )
            })
            .collect()
    }

    fn input<'a>(symbol: &'a str, industry: &'a str, candles: &'a [Candle]) -> ScreenInput<'a> {
        ScreenInput {
            symbol,
            industry: Some(industry),
            candles,
            delivery: &[],
            main_series: true,
        }
    }

    #[test]
    fn ranks_percentiles() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile_rank(&v, 5.0), 100.0);
        assert_eq!(percentile_rank(&v, 1.0), 0.0);
        assert_eq!(percentile_rank(&v, 3.0), 50.0);
        assert_eq!(percentile_rank(&[7.0], 7.0), 50.0);
    }

    #[test]
    fn gates_and_breadth_across_a_universe() {
        let strong = trend(100.0, 0.004, 260, 2_000_000.0);
        let flat = trend(100.0, 0.0, 260, 2_000_000.0);
        let weak = trend(100.0, -0.003, 260, 2_000_000.0);
        let strong2 = trend(100.0, 0.0035, 260, 2_000_000.0);
        let thin = trend(100.0, 0.003, 260, 100.0);
        let short = trend(100.0, 0.004, 20, 2_000_000.0);
        let inputs = vec![
            input("STRONG", "Tech", &strong),
            input("STRONG2", "Tech", &strong2),
            input("THIN", "Tech", &thin),
            input("FLAT", "Bank", &flat),
            input("FLAT2", "Bank", &flat),
            input("WEAK", "Bank", &weak),
            input("SHORT", "Bank", &short),
        ];
        let screen = run(&inputs, &ScreenParams::default());
        assert_eq!(screen.rows.len(), 6, "SHORT has too little history");
        assert_eq!(screen.breadth.universe, 6);
        // STRONG, STRONG2 and THIN are above both averages.
        assert!((screen.breadth.above_sma50_pct - 50.0).abs() < 1e-9);

        let row = |s: &str| screen.rows.iter().find(|r| r.symbol == s).unwrap();
        assert!(row("STRONG").gates.tradable && row("STRONG").gates.leader);
        assert!(!row("THIN").gates.tradable, "turnover is far too small");
        assert!(!row("WEAK").gates.leader);
        assert!(row("STRONG").rs.unwrap() > row("FLAT").rs.unwrap());
        assert!(row("FLAT").rs.unwrap() > row("WEAK").rs.unwrap());

        assert_eq!(screen.sectors.len(), 2);
        assert_eq!(screen.sectors[0].industry, "Tech");
        assert!(screen.sectors[0].strong && !screen.sectors[1].strong);
        assert_eq!(row("STRONG").gates.sector, Some(true));
        assert_eq!(row("WEAK").gates.sector, Some(false));
    }

    #[test]
    fn detects_a_breakout_from_a_tight_base() {
        // A steady climb, 25 flat bars, then a wide up bar on 3x volume.
        let mut c = trend(100.0, 0.003, 235, 1_000_000.0);
        let base = c.last().unwrap().close;
        for i in 0..25 {
            let ts = (235 + i) as i64 * DAY;
            c.push(Candle::new(
                ts,
                base,
                base * 1.01,
                base * 0.99,
                base,
                1_000_000.0,
            ));
        }
        let ts = 260 * DAY;
        c.push(Candle::new(
            ts,
            base,
            base * 1.035,
            base * 0.998,
            base * 1.03,
            3_000_000.0,
        ));
        let inputs = vec![input("BRK", "Tech", &c)];
        let screen = run(&inputs, &ScreenParams::default());
        let setups = &screen.rows[0].setups;
        assert_eq!(setups.len(), 1);
        let s = &setups[0];
        assert_eq!(s.kind, SwingKind::Breakout);
        assert!((s.level - base * 1.01).abs() < 1e-6);
        assert!((s.volume_ratio - 3.0).abs() < 1e-9);
        assert!(s.extension_pct > 1.5 && s.extension_pct < 2.5);
        let p = &s.plan;
        assert!(p.stop < p.entry && p.entry < p.target_2r && p.target_2r < p.target_3r);
        let risk = p.entry - p.stop;
        assert!((p.target_2r - (p.entry + 2.0 * risk)).abs() < 0.051);
        assert_eq!(p.quantity, (5_000.0 / risk).floor() as u64);
        assert!(s.score.setup > 20.0 && s.score.total() <= 100.0);

        // The same bar on ordinary volume is not a breakout.
        let mut quiet = c.clone();
        quiet.last_mut().unwrap().volume = 1_000_000.0;
        let inputs = vec![input("BRK", "Tech", &quiet)];
        assert!(run(&inputs, &ScreenParams::default()).rows[0]
            .setups
            .is_empty());

        // Far above the level it is extended, so it is not offered.
        let mut far = c.clone();
        let last = far.last_mut().unwrap();
        last.close = base * 1.09;
        last.high = base * 1.095;
        let inputs = vec![input("BRK", "Tech", &far)];
        assert!(run(&inputs, &ScreenParams::default()).rows[0]
            .setups
            .is_empty());
    }

    #[test]
    fn detects_a_pullback_that_turns_up() {
        // Uptrend, a 5-bar dip of about 6% on light volume, then a strong bar.
        let mut c = trend(100.0, 0.004, 250, 1_000_000.0);
        let top = c.last().unwrap().close;
        let dips = [0.985, 0.97, 0.955, 0.945, 0.95];
        for (i, f) in dips.iter().enumerate() {
            let close = top * f;
            let ts = (250 + i) as i64 * DAY;
            c.push(Candle::new(
                ts,
                close * 1.005,
                close * 1.01,
                close * 0.995,
                close,
                500_000.0,
            ));
        }
        let prev_high = c.last().unwrap().high;
        let close = prev_high * 1.012;
        c.push(Candle::new(
            255 * DAY,
            close * 0.985,
            close * 1.002,
            close * 0.98,
            close,
            1_200_000.0,
        ));
        let inputs = vec![input("PB", "Tech", &c)];
        let screen = run(&inputs, &ScreenParams::default());
        let setups = &screen.rows[0].setups;
        assert_eq!(setups.len(), 1, "{:?}", screen.rows[0]);
        let s = &setups[0];
        assert_eq!(s.kind, SwingKind::Pullback);
        assert!(s.plan.stop < top * 0.945, "stop sits below the dip low");
        assert!(s.reasons[0].contains("Pulled back"));

        // Without the turn (close below the prior high) nothing fires.
        let mut no_turn = c.clone();
        let last = no_turn.last_mut().unwrap();
        last.close = prev_high * 0.995;
        last.high = prev_high;
        let inputs = vec![input("PB", "Tech", &no_turn)];
        assert!(run(&inputs, &ScreenParams::default()).rows[0]
            .setups
            .is_empty());
    }

    #[test]
    fn classifies_the_market_regime() {
        let healthy = Breadth {
            universe: 500,
            above_sma50_pct: 60.0,
            above_sma200_pct: 70.0,
        };
        let weak = Breadth {
            above_sma50_pct: 30.0,
            ..healthy
        };
        let up: Vec<f64> = (0..260).map(|i| 100.0 + i as f64).collect();
        let down: Vec<f64> = (0..260).map(|i| 400.0 - i as f64).collect();
        // Rising for 250 bars, then a dip under the 50-day average only.
        let mut dip = up.clone();
        dip.extend([330.0, 325.0, 320.0]);

        assert_eq!(regime(Some(&up), &healthy).state, RegimeState::Normal);
        assert_eq!(regime(Some(&up), &weak).state, RegimeState::Caution);
        assert_eq!(regime(Some(&dip), &healthy).state, RegimeState::Caution);
        assert_eq!(regime(Some(&down), &healthy).state, RegimeState::Defensive);
        assert_eq!(regime(None, &healthy).state, RegimeState::Normal);
        assert_eq!(regime(None, &weak).state, RegimeState::Caution);
        let r = regime(Some(&up), &healthy);
        assert_eq!(r.index_close, Some(359.0));
        assert!(r.index_sma200.is_some());
    }
}
