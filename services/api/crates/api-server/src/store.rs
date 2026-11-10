//! A file store for daily candles, so scans re-fetch each symbol at most
//! once per trading session instead of on every run.
//!
//! Layout: `<CANDLE_STORE_DIR>/<provider>/1d/<SYMBOL>.json`. A file is fresh
//! while its `session` matches [`session_key`] for the current time.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration as ChronoDuration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use swing_core::Candle;

/// IST is UTC+05:30.
const IST_OFFSET_MINS: i64 = 330;
/// Daily bars are treated as final from this IST time (NSE closes 15:30).
const SESSION_CUTOFF: (u32, u32) = (15, 45);

/// The trading session whose daily bar is the latest complete one at `now`:
/// today's IST date after 15:45 IST, otherwise yesterday's. Weekends and
/// holidays simply cause one harmless re-fetch per day.
pub fn session_key(now: DateTime<Utc>) -> String {
    let ist = now.naive_utc() + ChronoDuration::minutes(IST_OFFSET_MINS);
    let cutoff =
        NaiveTime::from_hms_opt(SESSION_CUTOFF.0, SESSION_CUTOFF.1, 0).unwrap_or(NaiveTime::MIN);
    let date = if ist.time() >= cutoff {
        ist.date()
    } else {
        ist.date() - ChronoDuration::days(1)
    };
    date.format("%Y-%m-%d").to_string()
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredCandles {
    symbol: String,
    provider: String,
    session: String,
    /// Unix seconds.
    fetched_at: i64,
    /// `[time, open, high, low, close, volume]`, oldest first.
    candles: Vec<(i64, f64, f64, f64, f64, f64)>,
}

/// Daily candles on disk, per provider and symbol.
#[derive(Debug, Clone)]
pub struct CandleStore {
    dir: PathBuf,
}

impl CandleStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, provider: &str, symbol: &str) -> PathBuf {
        // Symbols are validated (A-Z, 0-9, &, -), all safe in file names.
        self.dir
            .join(provider)
            .join("1d")
            .join(format!("{symbol}.json"))
    }

    /// Stored candles for `session`; `None` if missing, stale or unreadable.
    pub async fn load(&self, provider: &str, symbol: &str, session: &str) -> Option<Vec<Candle>> {
        let path = self.path(provider, symbol);
        let bytes = tokio::fs::read(&path).await.ok()?;
        let stored: StoredCandles = match serde_json::from_slice(&bytes) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "ignoring unreadable candle file");
                return None;
            }
        };
        (stored.session == session && stored.symbol == symbol).then(|| {
            stored
                .candles
                .into_iter()
                .map(|(t, o, h, l, c, v)| Candle::new(t, o, h, l, c, v))
                .collect()
        })
    }

    /// Saves candles for `session`, replacing the file atomically.
    pub async fn save(
        &self,
        provider: &str,
        symbol: &str,
        session: &str,
        candles: &[Candle],
    ) -> std::io::Result<()> {
        let stored = StoredCandles {
            symbol: symbol.to_owned(),
            provider: provider.to_owned(),
            session: session.to_owned(),
            fetched_at: Utc::now().timestamp(),
            candles: candles
                .iter()
                .map(|c| (c.ts, c.open, c.high, c.low, c.close, c.volume))
                .collect(),
        };
        let path = self.path(provider, symbol);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec(&stored).map_err(std::io::Error::other)?;
        write_atomic(&path, &json).await
    }
}

async fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn sessions_roll_over_after_the_close() {
        // 15:44 IST = 10:14 UTC, still the previous session.
        let before = Utc.with_ymd_and_hms(2026, 11, 9, 10, 14, 0).unwrap();
        assert_eq!(session_key(before), "2026-11-08");
        let after = Utc.with_ymd_and_hms(2026, 11, 9, 10, 15, 0).unwrap();
        assert_eq!(session_key(after), "2026-11-09");
        // 00:30 IST on the 10th is still the 9th's session.
        let night = Utc.with_ymd_and_hms(2026, 11, 9, 19, 0, 0).unwrap();
        assert_eq!(session_key(night), "2026-11-09");
    }

    #[tokio::test]
    async fn saves_and_loads_fresh_candles_only() {
        let dir = std::env::temp_dir().join(format!("swing-store-test-{}", std::process::id()));
        let store = CandleStore::new(&dir);
        let candles = vec![
            Candle::new(1, 10.0, 11.0, 9.0, 10.5, 100.0),
            Candle::new(2, 10.5, 12.0, 10.0, 11.5, 200.0),
        ];
        assert!(store.load("indmoney", "M&M", "2026-11-09").await.is_none());
        store
            .save("indmoney", "M&M", "2026-11-09", &candles)
            .await
            .unwrap();
        let loaded = store.load("indmoney", "M&M", "2026-11-09").await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[1].close, 11.5);
        assert_eq!(loaded[1].ts, 2);
        assert!(store.load("indmoney", "M&M", "2026-11-10").await.is_none());
        assert!(store.load("demo", "M&M", "2026-11-09").await.is_none());

        tokio::fs::write(store.path("indmoney", "BAD"), b"not json")
            .await
            .unwrap();
        assert!(store.load("indmoney", "BAD", "2026-11-09").await.is_none());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
