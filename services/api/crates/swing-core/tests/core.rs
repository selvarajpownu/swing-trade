use swing_core::{
    analyze, indicators,
    plan::{self, PlanParams},
    zones::{find_zones, Zone, ZoneKind},
    AnalysisParams, Candle,
};

fn approx(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn bar(i: i64, close: f64) -> Candle {
    Candle::new(
        i * 86_400,
        close - 0.5,
        close + 1.0,
        close - 1.0,
        close,
        1_000.0,
    )
}

#[test]
fn sma_matches_hand_calculation() {
    let v = [1.0, 2.0, 3.0, 4.0, 5.0];
    let s = indicators::sma(&v, 3);
    assert_eq!(s, vec![None, None, Some(2.0), Some(3.0), Some(4.0)]);
}

#[test]
fn ema_is_seeded_with_sma() {
    let v = [2.0, 4.0, 6.0, 8.0];
    let e = indicators::ema(&v, 3);
    assert_eq!(e[2], Some(4.0));
    // k = 0.5 -> 8*0.5 + 4*0.5
    assert_eq!(e[3], Some(6.0));
}

#[test]
fn rsi_extremes() {
    let up: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
    let down: Vec<f64> = (0..30).map(|i| 100.0 - i as f64).collect();
    assert_eq!(
        indicators::rsi(&up, 14).last().copied().flatten(),
        Some(100.0)
    );
    assert!(approx(
        indicators::rsi(&down, 14)
            .last()
            .copied()
            .flatten()
            .unwrap(),
        0.0,
        1e-9
    ));
}

#[test]
fn atr_of_constant_range_is_the_range() {
    let c: Vec<Candle> = (0..20)
        .map(|i| Candle::new(i, 10.0, 12.0, 10.0, 11.0, 1.0))
        .collect();
    let a = indicators::atr(&c, 14);
    assert!(approx(a.last().copied().flatten().unwrap(), 2.0, 1e-9));
}

/// The numbers from the user's trade-plan card: SL 371.90, entry 392, target 440.
#[test]
fn trade_plan_matches_example_card() {
    let atr = 10.0;
    let zones = vec![
        Zone {
            low: 372.9,
            high: 375.0,
            touches: 3,
            last_touch: 0,
            kind: ZoneKind::Support,
        },
        Zone {
            low: 440.0,
            high: 445.0,
            touches: 2,
            last_touch: 0,
            kind: ZoneKind::Resistance,
        },
    ];
    let params = PlanParams {
        entry_buffer: 0.0,
        ..PlanParams::default()
    };
    let p = plan::build(392.0, atr, &zones, &params).expect("plan");
    assert!(approx(p.entry, 392.0, 1e-9));
    assert!(approx(p.stop, 371.90, 1e-9));
    assert!(approx(p.target, 440.0, 1e-9));
    assert_eq!(p.risk_reward, 2.39);
    assert_eq!(p.quantity, 248); // Rs 5,00,000 x 1% / Rs 20.10
    assert!(p.target_from_zone);
}

#[test]
fn stop_is_never_tighter_than_one_atr() {
    let zones = vec![Zone {
        low: 99.8,
        high: 99.9,
        touches: 2,
        last_touch: 0,
        kind: ZoneKind::Support,
    }];
    let p = plan::build(
        100.0,
        5.0,
        &zones,
        &PlanParams {
            entry_buffer: 0.0,
            ..PlanParams::default()
        },
    )
    .unwrap();
    assert!(approx(p.entry - p.stop, 5.0, 0.05));
    assert!(!p.target_from_zone);
    assert!(approx(p.risk_reward, 2.0, 0.02));
}

#[test]
fn zones_cluster_repeated_lows() {
    // A sawtooth that keeps bouncing off ~100 and topping near ~120.
    let mut c = Vec::new();
    let mut i = 0;
    for _ in 0..6 {
        for k in 0..10 {
            c.push(bar(i, 100.0 + 2.0 * k as f64));
            i += 1;
        }
        for k in 0..10 {
            c.push(bar(i, 120.0 - 2.0 * k as f64));
            i += 1;
        }
    }
    c.push(bar(i, 110.0));
    let z = find_zones(&c, 3, 1.0);
    let support = z
        .iter()
        .find(|z| z.kind == ZoneKind::Support)
        .expect("support");
    let resistance = z
        .iter()
        .find(|z| z.kind == ZoneKind::Resistance)
        .expect("resistance");
    assert!(support.high < 102.0 && support.touches >= 3, "{support:?}");
    assert!(
        resistance.low > 118.0 && resistance.touches >= 3,
        "{resistance:?}"
    );
}

#[test]
fn analyze_needs_history_and_produces_snapshot() {
    assert!(analyze(&[bar(0, 1.0)], &AnalysisParams::default()).is_none());
    let c: Vec<Candle> = (0..260)
        .map(|i| bar(i, 100.0 + (i as f64 * 0.3).sin() * 5.0 + i as f64 * 0.1))
        .collect();
    let a = analyze(&c, &AnalysisParams::default()).expect("analysis");
    assert_eq!(a.sma20.len(), c.len());
    assert!(a.snapshot.sma200.is_some());
    assert!(a.snapshot.high_52w >= a.snapshot.close);
    if let Some(s) = a.setup {
        assert!(s.score.total() <= 100.0);
        assert!(s.plan.stop < s.plan.entry && s.plan.entry < s.plan.target);
    }
}
