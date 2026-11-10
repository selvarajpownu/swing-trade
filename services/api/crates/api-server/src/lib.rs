//! HTTP API for the swing-trading app.
//!
//! View and analysis only: nothing here can place, modify or cancel orders.
//! The binary (`swing-api`) lives in `main.rs`; everything else is in this
//! library so it can be tested without a network listener.

pub mod auth;
pub mod cache;
pub mod config;
pub mod dto;
pub mod error;
pub mod params;
pub mod routes;
pub mod scan;
pub mod state;
pub mod store;
pub mod universe;

use std::future::Future;

use anyhow::Context;

pub use config::Config;
pub use error::ApiError;
pub use state::AppState;

/// Validates `config`, builds the providers and serves HTTP until `shutdown`
/// resolves, then drains in-flight requests.
pub async fn serve(
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    config.validate()?;
    let universe = universe::load(&config)?;
    let market = state::build_market(&config)?;
    let providers = market.ids();
    let bind_addr = config.bind_addr;
    let state = AppState::new(config, market);
    let app = routes::router(state.clone())?;
    tokio::spawn(scan::run_forever(state, universe));

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("cannot listen on {bind_addr}"))?;
    tracing::info!(
        addr = %listener.local_addr().unwrap_or(bind_addr),
        ?providers,
        "swing-api listening"
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .context("server error")?;
    tracing::info!("swing-api stopped");
    Ok(())
}
