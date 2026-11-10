//! `swing-api`: the swing-trading HTTP API and its admin commands.
//!
//! ```text
//! swing-api                 # same as `swing-api serve`
//! swing-api serve
//! swing-api login indmoney  # browser sign-in; saves tokens to INDMONEY_TOKEN_FILE
//! ```

use std::process::ExitCode;

use api_server::Config;
use clap::{Parser, Subcommand};
use market_data::indmoney;
use tracing_subscriber::EnvFilter;

/// Swing-trading analysis API for Indian stocks (view only; never places orders).
#[derive(Parser)]
#[command(name = "swing-api", version, about)]
struct Cli {
    #[command(flatten)]
    config: Config,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP API (the default).
    Serve,
    /// Connect a market-data provider.
    Login {
        #[command(subcommand)]
        provider: LoginProvider,
    },
}

#[derive(Subcommand)]
enum LoginProvider {
    /// Sign in to INDmoney in the browser (mobile number, OTP and MPIN).
    Indmoney {
        /// Only print the sign-in URL; do not try to open a browser.
        #[arg(long)]
        no_browser: bool,
        /// Use a pre-registered OAuth client id instead of dynamic registration.
        #[arg(long, env = "INDMONEY_CLIENT_ID")]
        client_id: Option<String>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    // Load `.env` before clap reads the environment.
    let dotenv = dotenvy::dotenv();
    let cli = Cli::parse();
    init_tracing();
    if let Err(e) = dotenv {
        if !e.not_found() {
            tracing::warn!(error = %e, "could not load .env");
        }
    }

    let result = match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => api_server::serve(cli.config, shutdown_signal()).await,
        Command::Login {
            provider:
                LoginProvider::Indmoney {
                    no_browser,
                    client_id,
                },
        } => login_indmoney(&cli.config, !no_browser, client_id).await,
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e:#}");
            ExitCode::FAILURE
        }
    }
}

/// JSON logs with `LOG_FORMAT=json`, multi-line pretty logs with
/// `LOG_FORMAT=pretty`, otherwise one human-readable line per event.
/// Filter from `RUST_LOG`, default `info,api_server=debug`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,api_server=debug"));
    let format = std::env::var("LOG_FORMAT")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match format.as_str() {
        "json" => builder.json().init(),
        "pretty" => builder.pretty().init(),
        _ => builder.init(),
    }
}

async fn login_indmoney(
    config: &Config,
    open_browser: bool,
    client_id: Option<String>,
) -> anyhow::Result<()> {
    if std::env::var(indmoney::oauth::ACCESS_TOKEN_ENV).is_ok_and(|v| !v.trim().is_empty()) {
        tracing::warn!(
            "{} is set and will override the token file saved by this login",
            indmoney::oauth::ACCESS_TOKEN_ENV
        );
    }
    let opts = indmoney::LoginOptions {
        mcp_url: config.indmoney_mcp_url.clone(),
        token_file: config.indmoney_token_file.clone(),
        open_browser,
        client_id,
    };
    let stored = indmoney::login(&opts).await?;
    let expiry = match stored.expires_at {
        Some(at) => format!("access token valid until unix time {at}"),
        None => "no expiry reported".to_owned(),
    };
    println!(
        "Connected to INDmoney. Tokens saved to {} ({expiry}{}).",
        opts.token_file.display(),
        if stored.refresh_token.is_some() {
            "; will refresh automatically"
        } else {
            "; no refresh token, log in again when it expires"
        }
    );
    Ok(())
}

/// Resolves on Ctrl+C, or SIGTERM on unix.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "cannot listen for Ctrl+C");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut sigterm) => {
                sigterm.recv().await;
            }
            Err(e) => {
                tracing::error!(error = %e, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received; draining connections");
}
