pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("symbol not found: {0}")]
    NotFound(String),

    #[error("{provider} is not authorised; run `swing-api login {provider}`")]
    Unauthorized { provider: &'static str },

    /// The provider's request budget is spent; try again after `retry_after`.
    #[error("{provider} is rate limited; try again in {}s", retry_after.as_secs().max(1))]
    RateLimited {
        provider: &'static str,
        retry_after: std::time::Duration,
    },

    /// The provider does not cover this market, symbol or candle size.
    /// Expected, not a fault: the next provider simply takes over.
    #[error("{provider} does not cover this: {message}")]
    Unsupported {
        provider: &'static str,
        message: String,
    },

    #[error("upstream {provider} error: {message}")]
    Upstream {
        provider: &'static str,
        message: String,
    },

    #[error("unexpected response from {provider}: {message}")]
    Decode {
        provider: &'static str,
        message: String,
    },

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("no market-data provider could serve the request: {0}")]
    AllProvidersFailed(String),
}
