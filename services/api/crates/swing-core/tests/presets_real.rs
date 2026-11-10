//! Golden tests on real NSE daily candles (INDmoney, Nov 2025 to 9 Nov 2026).
//!
//! Each case cuts a recorded series at a dated bar and checks which presets
//! fire there, with what score and plan. They pin today's engine behaviour so
//! a change to a preset shows up as a failing test, ahead of the phase 5
//! back-tests. Fixtures: `tests/fixtures/<SYMBOL>_1d.csv` (time is the bar's
//! Unix second at 00:00 UTC = 05:30 IST). Market data, for tests only.

use swing_core::{analyze, AnalysisParams, Candle, Preset, Setup, SetupState};

fn load(symbol: &str) -> Vec<Candle> {
    let path = format!(
        "{}/tests/fixtures/{symbol}_1d.csv",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    text.lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f: Vec<f64> = line
                .split(',')
                .map(|x| x.trim().parse().expect("number"))
                .collect();
            Candle::new(f[0] as i64, f[1], f[2], f[3], f[4], f[5])
        })
        .collect()
}

/// The series up to and including the bar at `time`.
fn until(candles: &[Candle], time: i64) -> &[Candle] {
    let end = candles
        .iter()
        .position(|c| c.ts == time)
        .unwrap_or_else(|| panic!("no bar at {time}"));
    &candles[..=end]
}

/// Setups firing on the last bar, best first.
fn setups(symbol: &str, time: Option<i64>) -> Vec<Setup> {
    let all = load(symbol);
    let series = time.map_or(&all[..], |t| until(&all, t));
    let analysis = analyze(series, &AnalysisParams::default()).expect("enough history");
    for s in &analysis.setups {
        check_plan(s);
    }
    assert_eq!(
        analysis.setup.as_ref().map(|s| s.preset),
        analysis.setups.first().map(|s| s.preset),
        "`setup` must be the best of `setups`"
    );
    analysis.setups
}

/// Invariants every long plan must meet, whatever the preset.
fn check_plan(s: &Setup) {
    let p = &s.plan;
    assert!(p.stop < p.entry && p.entry < p.target, "{p:?}");
    let on_tick = |x: f64| ((x / 0.05).round() * 0.05 - x).abs() < 1e-6;
    assert!(on_tick(p.entry) && on_tick(p.stop), "{p:?}");
    let rr = (p.target - p.entry) / (p.entry - p.stop);
    assert!((rr - p.risk_reward).abs() < 0.01, "{p:?}");
    // Rs 5,00,000 x 1% = Rs 5,000 risked.
    assert_eq!(p.quantity, (5_000.0 / (p.entry - p.stop)).floor() as u64);
    let expected = if p.risk_reward >= 2.0 {
        SetupState::Watching
    } else {
        SetupState::LowRiskReward
    };
    assert_eq!(s.state, expected);
    let total = s.score.total();
    assert!((0.0..=100.0).contains(&total));
    assert!(!s.reasons.is_empty());
}

fn summary(setups: &[Setup]) -> Vec<(Preset, f64)> {
    setups.iter().map(|s| (s.preset, s.score.total())).collect()
}

#[test]
fn pullback_ask_automotive_9_nov_2026() {
    let s = setups("ASKAUTOLTD", None);
    assert_eq!(summary(&s), vec![(Preset::Pullback, 68.0)]);
    assert_eq!(s[0].state, SetupState::Watching);
}

#[test]
fn support_bounce_bse_9_nov_2026() {
    let s = setups("BSE", None);
    assert_eq!(summary(&s), vec![(Preset::SupportBounce, 53.0)]);
    assert!(
        s[0].reasons[0].contains("support zone"),
        "{:?}",
        s[0].reasons
    );
}

#[test]
fn breakout_mcx_24_sep_2026() {
    let s = setups("MCX", Some(1_790_208_000));
    assert_eq!(summary(&s), vec![(Preset::Breakout, 68.0)]);
    assert!(s[0].reasons[0].contains("55-day high"));
}

#[test]
fn breakout_ask_automotive_19_jun_2026() {
    let s = setups("ASKAUTOLTD", Some(1_781_827_200));
    assert_eq!(summary(&s), vec![(Preset::Breakout, 71.0)]);
}

#[test]
fn oversold_persistent_24_feb_2026() {
    let s = setups("PERSISTENT", Some(1_771_891_200));
    assert_eq!(summary(&s), vec![(Preset::Oversold, 41.0)]);
    assert!(s[0].plan.risk_reward > 4.0, "{:?}", s[0].plan);
}

#[test]
fn two_presets_cams_6_may_2026() {
    let s = setups("CAMS", Some(1_778_025_600));
    assert_eq!(
        summary(&s),
        vec![(Preset::SupportBounce, 67.0), (Preset::Breakout, 56.0)]
    );
}

#[test]
fn low_risk_reward_mcx_2_mar_2026() {
    let s = setups("MCX", Some(1_772_409_600));
    assert_eq!(summary(&s), vec![(Preset::SupportBounce, 67.0)]);
    assert_eq!(s[0].state, SetupState::LowRiskReward);
    assert!((s[0].plan.risk_reward - 1.65).abs() < 1e-9);
}

#[test]
fn nothing_fires_on_coforge_9_nov_2026() {
    assert!(setups("COFORGE", None).is_empty());
}

#[test]
fn every_watchlist_fixture_analyses_cleanly() {
    for symbol in [
        "COFORGE",
        "MCX",
        "CAMS",
        "IEX",
        "ACE",
        "AJAXENGG",
        "ASKAUTOLTD",
        "PERSISTENT",
        "OFSS",
        "BSE",
    ] {
        let candles = load(symbol);
        assert_eq!(candles.len(), 249, "{symbol}");
        assert!(candles.windows(2).all(|w| w[0].ts < w[1].ts), "{symbol}");
        // Every bar from 60 on: plans always satisfy the invariants.
        for end in 60..=candles.len() {
            if let Some(a) = analyze(&candles[..end], &AnalysisParams::default()) {
                a.setups.iter().for_each(check_plan);
            }
        }
    }
}
