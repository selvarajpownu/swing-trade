pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("symbol not found: {0}")]
    NotFound(String),

    #[error("{provider} is not authorised; run `swing-api login {provider}`")]
    Unauthorized { provider: &'static str },

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
