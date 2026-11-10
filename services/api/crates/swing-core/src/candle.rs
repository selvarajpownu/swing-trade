/// One OHLCV bar. `ts` is the bar's open time in Unix seconds (UTC).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candle {
    pub ts: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Candle {
    pub fn new(ts: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Self {
        Self {
            ts,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    /// True range against the previous close (Wilder).
    pub fn true_range(&self, prev_close: Option<f64>) -> f64 {
        let hl = self.high - self.low;
        match prev_close {
            Some(pc) => hl.max((self.high - pc).abs()).max((self.low - pc).abs()),
            None => hl,
        }
    }

    pub fn body(&self) -> f64 {
        (self.close - self.open).abs()
    }

    pub fn is_bullish(&self) -> bool {
        self.close > self.open
    }
}
