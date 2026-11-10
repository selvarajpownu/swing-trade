//! Server configuration from flags and environment variables.
//!
//! Every option can be set with its environment variable (a `.env` file is
//! loaded first) or the matching `--flag`.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Args;
use market_data::indmoney;

use crate::params;

/// Runtime configuration for `swing-api`.
#[derive(Clone, Args)]
pub struct Config {
    /// Address to listen on.
    #[arg(long, env = "BIND_ADDR", default_value = "0.0.0.0:8080", global = true)]
    pub bind_addr: SocketAddr,

    /// Bearer token every /v1 request must carry.
    #[arg(long, env = "API_TOKEN", hide_env_values = true, global = true)]
    pub api_token: Option<String>,

    /// Allow starting without API_TOKEN (local development only).
    #[arg(long, env = "ALLOW_NO_TOKEN", global = true)]
    pub allow_no_token: bool,

    /// Data providers to try, in order (comma separated: indmoney, demo).
    #[arg(
        long,
        env = "DATA_PROVIDERS",
        default_value = "indmoney,demo",
        global = true
    )]
    pub data_providers: String,

    /// INDmoney MCP server URL.
    #[arg(long, env = "INDMONEY_MCP_URL", default_value = indmoney::DEFAULT_MCP_URL, global = true)]
    pub indmoney_mcp_url: String,

    /// Where INDmoney OAuth tokens are stored.
    #[arg(long, env = "INDMONEY_TOKEN_FILE", default_value = indmoney::oauth::DEFAULT_TOKEN_FILE, global = true)]
    pub indmoney_token_file: PathBuf,

    /// Origins allowed by CORS (comma separated; `*` for any).
    #[arg(
        long,
        env = "CORS_ORIGINS",
        default_value = "http://localhost:3000",
        global = true
    )]
    pub cors_origins: String,

    /// Default symbols for GET /v1/stocks (comma separated).
    #[arg(
        long,
        env = "WATCHLIST",
        default_value = "COFORGE,MCX,CAMS,IEX,ACE,AJAXENGG,ASKAUTOLTD,PERSISTENT,OFSS,BSE",
        global = true
    )]
    pub watchlist: String,

    /// Default trading capital in rupees for position sizing.
    #[arg(
        long,
        env = "DEFAULT_CAPITAL",
        default_value_t = 500_000.0,
        global = true
    )]
    pub default_capital: f64,

    /// Default fraction of capital risked per trade (max 0.05).
    #[arg(long, env = "DEFAULT_RISK", default_value_t = 0.01, global = true)]
    pub default_risk: f64,

    /// How long quotes are cached, in seconds.
    #[arg(long, env = "QUOTE_TTL_SECS", default_value_t = 15, global = true)]
    pub quote_ttl_secs: u64,

    /// How long candles are cached, in seconds.
    #[arg(long, env = "CANDLE_TTL_SECS", default_value_t = 600, global = true)]
    pub candle_ttl_secs: u64,

    /// Client-side INDmoney budget in cost units per minute (server allows 60).
    #[arg(long, env = "INDMONEY_RATE_PER_MIN", default_value_t = indmoney::DEFAULT_RATE_PER_MIN, global = true)]
    pub indmoney_rate_per_min: u32,

    /// Symbols to scan (comma separated); used when UNIVERSE_FILE is not set.
    /// Empty means the WATCHLIST.
    #[arg(long, env = "UNIVERSE", default_value = "", global = true)]
    pub universe: String,

    /// File with the symbols to scan: one per line, or a CSV with a `Symbol`
    /// column (NSE's index constituent files work as downloaded).
    #[arg(long, env = "UNIVERSE_FILE", global = true)]
    pub universe_file: Option<PathBuf>,

    /// Extra symbols never scanned (INDRAMEDCO is always excluded).
    #[arg(long, env = "SCAN_EXCLUDE", default_value = "", global = true)]
    pub scan_exclude: String,

    /// Minutes between scans; 0 scans once at startup only.
    #[arg(long, env = "SCAN_INTERVAL_MINS", default_value_t = 30, global = true)]
    pub scan_interval_mins: u64,

    /// Network candle fetches per minute during a scan (each costs 2 INDmoney
    /// units), leaving the rest of the budget for page requests.
    #[arg(
        long,
        env = "SCAN_SYMBOLS_PER_MIN",
        default_value_t = 15,
        global = true
    )]
    pub scan_symbols_per_min: u32,

    /// Directory for the daily-candle file store used by scans.
    #[arg(
        long,
        env = "CANDLE_STORE_DIR",
        default_value = ".data/candles",
        global = true
    )]
    pub candle_store_dir: PathBuf,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("api_token", &self.api_token().map(|_| "<redacted>"))
            .field("allow_no_token", &self.allow_no_token)
            .field("data_providers", &self.data_providers)
            .field("indmoney_mcp_url", &self.indmoney_mcp_url)
            .field("indmoney_token_file", &self.indmoney_token_file)
            .field("cors_origins", &self.cors_origins)
            .field("watchlist", &self.watchlist)
            .field("default_capital", &self.default_capital)
            .field("default_risk", &self.default_risk)
            .field("quote_ttl_secs", &self.quote_ttl_secs)
            .field("candle_ttl_secs", &self.candle_ttl_secs)
            .field("indmoney_rate_per_min", &self.indmoney_rate_per_min)
            .field("universe", &self.universe)
            .field("universe_file", &self.universe_file)
            .field("scan_exclude", &self.scan_exclude)
            .field("scan_interval_mins", &self.scan_interval_mins)
            .field("scan_symbols_per_min", &self.scan_symbols_per_min)
            .field("candle_store_dir", &self.candle_store_dir)
            .finish()
    }
}

impl Config {
    /// The API token, if one is set and non-blank.
    pub fn api_token(&self) -> Option<&str> {
        self.api_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }

    /// Provider ids in order, lower case.
    pub fn providers(&self) -> Vec<String> {
        split_list(&self.data_providers)
            .map(|s| s.to_ascii_lowercase())
            .collect()
    }

    /// CORS origins.
    pub fn cors_origins(&self) -> Vec<String> {
        split_list(&self.cors_origins)
            .map(|s| s.trim_end_matches('/').to_owned())
            .collect()
    }

    /// Default watchlist, normalised. Invalid entries are rejected by [`validate`](Self::validate).
    pub fn watchlist(&self) -> Vec<String> {
        params::parse_symbols(&self.watchlist).unwrap_or_default()
    }

    /// Checks everything that can be checked before starting.
    pub fn validate(&self) -> anyhow::Result<()> {
        match self.api_token() {
            Some(token) if token.len() < 16 => {
                tracing::warn!("API_TOKEN is shorter than 16 characters; use a long random value");
            }
            Some(_) => {}
            None if self.allow_no_token => {
                tracing::warn!(
                    "!!! API_TOKEN is not set and ALLOW_NO_TOKEN=true: the API accepts requests \
                     from anyone who can reach it. Never do this outside local development. !!!"
                );
            }
            None => bail!(
                "API_TOKEN is not set. Set it to a long random value \
                 (or ALLOW_NO_TOKEN=true for local development only)"
            ),
        }
        if self.providers().is_empty() {
            bail!("DATA_PROVIDERS is empty (expected e.g. `indmoney,demo`)");
        }
        params::parse_symbols(&self.watchlist).map_err(|e| anyhow::anyhow!("WATCHLIST: {e}"))?;
        params::check_capital(self.default_capital)
            .map_err(|e| anyhow::anyhow!("DEFAULT_CAPITAL: {e}"))?;
        params::check_risk(self.default_risk).map_err(|e| anyhow::anyhow!("DEFAULT_RISK: {e}"))?;
        for origin in self.cors_origins() {
            if origin != "*" && !(origin.starts_with("http://") || origin.starts_with("https://")) {
                bail!("CORS_ORIGINS: `{origin}` must start with http:// or https://");
            }
        }
        check_http_url(&self.indmoney_mcp_url).context("INDMONEY_MCP_URL")?;
        if !(1..=600).contains(&self.indmoney_rate_per_min) {
            bail!("INDMONEY_RATE_PER_MIN must be between 1 and 600");
        }
        if !(1..=300).contains(&self.scan_symbols_per_min) {
            bail!("SCAN_SYMBOLS_PER_MIN must be between 1 and 300");
        }
        params::parse_symbols_lenient(&self.universe)
            .map_err(|e| anyhow::anyhow!("UNIVERSE: {e}"))?;
        params::parse_symbols_lenient(&self.scan_exclude)
            .map_err(|e| anyhow::anyhow!("SCAN_EXCLUDE: {e}"))?;
        Ok(())
    }
}

/// Minimal sanity check without pulling in a URL parser.
fn check_http_url(url: &str) -> anyhow::Result<()> {
    if url.starts_with("https://") || url.starts_with("http://") {
        Ok(())
    } else {
        bail!("`{url}` is not an http(s) URL")
    }
}

fn split_list(s: &str) -> impl Iterator<Item = &str> {
    s.split(',').map(str::trim).filter(|s| !s.is_empty())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Wrapper {
        #[command(flatten)]
        config: Config,
    }

    /// A config built from explicit flags (flags win over the environment).
    pub(crate) fn config_from(args: &[&str]) -> Config {
        let argv = std::iter::once("swing-api").chain(args.iter().copied());
        match Wrapper::try_parse_from(argv) {
            Ok(w) => w.config,
            Err(e) => panic!("{e}"),
        }
    }

    pub(crate) fn test_config() -> Config {
        config_from(&[
            "--api-token",
            "test-token-0123456789",
            "--data-providers",
            "demo",
            "--watchlist",
            "COFORGE,MCX,M&M",
            "--cors-origins",
            "http://localhost:3000",
            "--default-capital",
            "500000",
            "--default-risk",
            "0.01",
        ])
    }

    #[test]
    fn parses_lists_and_defaults() {
        let c = test_config();
        assert_eq!(c.providers(), vec!["demo"]);
        assert_eq!(c.watchlist(), vec!["COFORGE", "MCX", "M&M"]);
        assert_eq!(c.cors_origins(), vec!["http://localhost:3000"]);
        assert_eq!(c.api_token(), Some("test-token-0123456789"));
        assert!(c.validate().is_ok());
        assert!(!format!("{c:?}").contains("test-token"));
    }

    #[test]
    fn requires_a_token_unless_allowed() {
        let blank = config_from(&["--api-token", " ", "--data-providers", "demo"]);
        if std::env::var("ALLOW_NO_TOKEN").is_err() {
            assert!(blank.validate().is_err());
        }
        let open = config_from(&[
            "--api-token",
            " ",
            "--allow-no-token",
            "--data-providers",
            "demo",
        ]);
        assert_eq!(open.api_token(), None);
        assert!(open.validate().is_ok());
    }

    #[test]
    fn rejects_bad_defaults() {
        let base = [
            "--api-token",
            "test-token-0123456789",
            "--data-providers",
            "demo",
        ];
        let with = |extra: &[&str]| {
            let mut args = base.to_vec();
            args.extend_from_slice(extra);
            config_from(&args)
        };
        assert!(with(&["--default-risk", "0.2"]).validate().is_err());
        assert!(with(&["--default-capital", "0"]).validate().is_err());
        assert!(with(&["--watchlist", "GOOD,BAD SYMBOL"])
            .validate()
            .is_err());
        assert!(with(&["--cors-origins", "localhost:3000"])
            .validate()
            .is_err());
        // `base` already sets --data-providers, and clap rejects a repeated flag.
        let no_providers = config_from(&[
            "--api-token",
            "test-token-0123456789",
            "--data-providers",
            " , ",
        ]);
        assert!(no_providers.validate().is_err());

        assert!(with(&["--universe", "OK,NOT OK"]).validate().is_err());
        assert!(with(&["--scan-symbols-per-min", "0"]).validate().is_err());
        assert!(with(&["--indmoney-rate-per-min", "0"]).validate().is_err());
        let fine = with(&["--universe", "", "--scan-exclude", "IEX"]);
        assert!(fine.validate().is_ok());
    }
}
