//! A sliding-window rate limiter, shared by every call to one provider.
//!
//! INDmoney's MCP server limits "cost units" both per minute (60) and per
//! hour (500) across all tools. Staying under those budgets on our side is
//! cheaper than hitting them, and lets the background scanner and page
//! requests share them. Callers can cap how long they are willing to wait,
//! so a page fails fast instead of hanging when the budget is spent.

use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// Allows at most `limit` cost units in each of its windows.
pub struct RateLimiter {
    /// `(limit, window)` pairs, all enforced together.
    windows: Vec<(u32, Duration)>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// When each recent call was let through, and its cost.
    log: VecDeque<(Instant, u32)>,
    /// No call goes through before this (set when the server says to wait).
    paused_until: Option<Instant>,
}

impl RateLimiter {
    /// `limit` units per `window`. A zero limit is treated as 1.
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            windows: vec![(limit.max(1), window)],
            state: Mutex::new(State::default()),
        }
    }

    /// `limit` units per minute.
    pub fn per_minute(limit: u32) -> Self {
        Self::new(limit, Duration::from_secs(60))
    }

    /// Adds another window, e.g. an hourly budget on top of a per-minute one.
    pub fn with_window(mut self, limit: u32, window: Duration) -> Self {
        self.windows.push((limit.max(1), window));
        self
    }

    /// The smallest limit of any window.
    pub fn limit(&self) -> u32 {
        self.windows.iter().map(|(l, _)| *l).min().unwrap_or(1)
    }

    /// Holds every caller back for `wait`, e.g. when the server reports its
    /// own limit was hit (our cost estimates or other clients can be off).
    pub async fn pause(&self, wait: Duration) {
        let until = Instant::now() + wait;
        let mut state = self.state.lock().await;
        if state.paused_until.is_none_or(|t| t < until) {
            state.paused_until = Some(until);
        }
    }

    /// Waits until `cost` more units fit in every window, then records them.
    /// A cost above the smallest limit is capped so it can still run.
    pub async fn acquire(&self, cost: u32) {
        // An hour-long wait is the most any window can ask for.
        let _ = self
            .acquire_within(cost, Duration::from_secs(24 * 3_600))
            .await;
    }

    /// Like [`acquire`](Self::acquire), but gives up without recording
    /// anything when the next wait would exceed `max_wait`, returning how
    /// long it would have had to wait.
    pub async fn acquire_within(&self, cost: u32, max_wait: Duration) -> Result<(), Duration> {
        let cost = cost.clamp(1, self.limit());
        loop {
            let wait = {
                let mut state = self.state.lock().await;
                let now = Instant::now();
                match state.paused_until.filter(|t| *t > now) {
                    Some(until) => until - now,
                    None => match self.wait_for(&mut state.log, now, cost) {
                        Duration::ZERO => {
                            state.log.push_back((now, cost));
                            return Ok(());
                        }
                        wait => wait,
                    },
                }
            };
            if wait > max_wait {
                return Err(wait);
            }
            tokio::time::sleep(wait).await;
        }
    }

    /// How long until `cost` fits in every window (zero when it fits now).
    fn wait_for(&self, log: &mut VecDeque<(Instant, u32)>, now: Instant, cost: u32) -> Duration {
        let longest = self
            .windows
            .iter()
            .map(|(_, w)| *w)
            .max()
            .unwrap_or_default();
        while log
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) >= longest)
        {
            log.pop_front();
        }
        let mut wait = Duration::ZERO;
        for &(limit, window) in &self.windows {
            let recent = log
                .iter()
                .filter(|(at, _)| now.duration_since(*at) < window);
            let used: u32 = recent.clone().map(|(_, c)| c).sum();
            if used + cost <= limit {
                continue;
            }
            // Wait until enough of this window's oldest calls leave it.
            let need = used + cost - limit;
            let mut freed = 0;
            for (at, c) in recent {
                freed += c;
                if freed >= need {
                    wait = wait.max((*at + window).saturating_duration_since(now));
                    break;
                }
            }
        }
        wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lets_calls_through_within_the_budget() {
        let limiter = RateLimiter::new(5, Duration::from_millis(200));
        let start = Instant::now();
        limiter.acquire(2).await;
        limiter.acquire(2).await;
        limiter.acquire(1).await;
        assert!(start.elapsed() < Duration::from_millis(100));
    }

    #[tokio::test]
    async fn waits_when_the_budget_is_spent() {
        let limiter = RateLimiter::new(4, Duration::from_millis(150));
        let start = Instant::now();
        limiter.acquire(2).await;
        limiter.acquire(2).await;
        limiter.acquire(2).await; // must wait for the first call to expire
        let waited = start.elapsed();
        assert!(waited >= Duration::from_millis(140), "{waited:?}");
        assert!(waited < Duration::from_millis(400), "{waited:?}");
    }

    #[tokio::test]
    async fn enforces_every_window() {
        // Plenty per "minute", but only 3 units per "hour".
        let limiter = RateLimiter::new(100, Duration::from_millis(50))
            .with_window(3, Duration::from_secs(60));
        assert_eq!(limiter.limit(), 3);
        limiter.acquire(2).await;
        limiter.acquire(1).await;
        let refused = limiter
            .acquire_within(1, Duration::from_millis(100))
            .await
            .unwrap_err();
        assert!(refused > Duration::from_secs(50), "{refused:?}");
    }

    #[tokio::test]
    async fn pause_holds_every_caller() {
        let limiter = RateLimiter::new(100, Duration::from_secs(60));
        limiter.pause(Duration::from_millis(120)).await;
        limiter.pause(Duration::from_millis(10)).await; // a shorter pause never shortens it
        let start = Instant::now();
        limiter.acquire(1).await;
        assert!(start.elapsed() >= Duration::from_millis(110));

        limiter.pause(Duration::from_secs(30)).await;
        let refused = limiter
            .acquire_within(1, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(refused > Duration::from_secs(25));
    }

    #[tokio::test]
    async fn caps_oversized_costs() {
        let limiter = RateLimiter::new(3, Duration::from_millis(100));
        assert_eq!(limiter.limit(), 3);
        limiter.acquire(10).await; // capped at 3, so it does not block forever
        assert_eq!(RateLimiter::per_minute(0).limit(), 1);
    }
}
