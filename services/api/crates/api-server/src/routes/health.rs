//! `GET /health` (no auth).

use axum::extract::State;
use axum::Json;

use crate::dto::HealthResponse;
use crate::state::AppState;

/// Liveness plus the configured provider order.
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        providers: state.market.ids(),
    })
}
