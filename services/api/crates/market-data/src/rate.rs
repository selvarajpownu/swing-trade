//! A sliding-window rate limiter, shared by every call to one provider.
//!
//! INDmoney's MCP server allows a fixed number of "cost units" per minute
//! across all tools. Staying under that budget on our side is cheaper than
//! hitting the limit and waiting out `retry_after_seconds`, and it lets the
//! background scanner and page requests share the budget fairly.

use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

/// Allows at most `limit` cost units in any `window`.
pub struct RateLimiter {
    limit: u32,
    window: Duration,
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
            limit: limit.max(1),
            window,
            state: Mutex::new(State::default()),
        }
    }

    /// `limit` units per minute.
    pub fn per_minute(limit: u32) -> Self {
        Self::new(limit, Duration::from_secs(60))
    }

    /// Units allowed per window.
    pub fn limit(&self) -> u32 {
        self.limit
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

    /// Waits until `cost` more units fit in the window, then records them.
    /// A cost above the limit is capped at the limit so it can still run.
    pub async fn acquire(&self, cost: u32) {
        let cost = cost.clamp(1, self.limit);
        loop {
            let wait = {
                let mut state = self.state.lock().await;
                let now = Instant::now();
                if let Some(until) = state.paused_until.filter(|t| *t > now) {
                    drop(state);
                    tokio::time::sleep_until(until).await;
                    continue;
                }
                let log = &mut state.log;
                while log
                    .front()
                    .is_some_and(|(at, _)| now.duration_since(*at) >= self.window)
                {
                    log.pop_front();
                }
                let used: u32 = log.iter().map(|(_, c)| c).sum();
                if used + cost <= self.limit {
                    log.push_back((now, cost));
                    return;
                }
                // Sleep until enough of the oldest calls leave the window.
                let need = used + cost - self.limit;
                let mut freed = 0;
                let mut until = now;
                for (at, c) in log.iter() {
                    freed += c;
                    until = *at + self.window;
                    if freed >= need {
                        break;
                    }
                }
                until.saturating_duration_since(now)
            };
            tokio::time::sleep(wait).await;
        }
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
    async fn pause_holds_every_caller() {
        let limiter = RateLimiter::new(100, Duration::from_secs(60));
        limiter.pause(Duration::from_millis(120)).await;
        limiter.pause(Duration::from_millis(10)).await; // a shorter pause never shortens it
        let start = Instant::now();
        limiter.acquire(1).await;
        assert!(start.elapsed() >= Duration::from_millis(110));
    }

    #[tokio::test]
    async fn caps_oversized_costs() {
        let limiter = RateLimiter::new(3, Duration::from_millis(100));
        assert_eq!(limiter.limit(), 3);
        limiter.acquire(10).await; // capped at 3, so it does not block forever
        assert_eq!(RateLimiter::per_minute(0).limit(), 1);
    }
}
