//! HTTP routes and middleware stack.

pub mod health;
pub mod stocks;

use std::time::Duration;

use anyhow::Context;
use axum::http::{header, HeaderValue, Method};
use axum::routing::get;
use axum::{middleware, Router};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::auth;
use crate::error::ApiError;
use crate::state::AppState;

/// Hard limit for one request, including upstream calls.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// The full application router.
pub fn router(state: AppState) -> anyhow::Result<Router> {
    let cors = cors_layer(&state.config.cors_origins())?;

    let v1 = Router::new()
        .route("/stocks", get(stocks::list))
        .route("/stocks/{symbol}", get(stocks::detail))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_token,
        ));

    #[allow(deprecated)]
    // `TimeoutLayer::new` is deprecated in later 0.6 releases but present in all.
    let timeout = TimeoutLayer::new(REQUEST_TIMEOUT);

    Ok(Router::new()
        .route("/health", get(health::health))
        .nest("/v1", v1)
        .fallback(not_found)
        .with_state(state)
        .layer(timeout)
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        ))
}

async fn not_found() -> ApiError {
    ApiError::NotFound("no such route".to_owned())
}

fn cors_layer(origins: &[String]) -> anyhow::Result<CorsLayer> {
    let allow_origin = if origins.iter().any(|o| o == "*") {
        AllowOrigin::any()
    } else {
        let values = origins
            .iter()
            .map(|o| HeaderValue::from_str(o).with_context(|| format!("invalid CORS origin `{o}`")))
            .collect::<anyhow::Result<Vec<_>>>()?;
        AllowOrigin::list(values)
    };
    Ok(CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([Method::GET, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .max_age(Duration::from_secs(600)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::test_config;
    use crate::state::build_market;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::Response;
    use serde_json::Value;
    use tower::ServiceExt;

    const TOKEN: &str = "test-token-0123456789";

    fn app() -> Router {
        let config = test_config();
        let market = build_market(&config).expect("demo market");
        router(AppState::new(config, market)).expect("router")
    }

    async fn get(uri: &str, token: Option<&str>) -> Response {
        let mut req = Request::builder().uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        app()
            .oneshot(req.body(Body::empty()).expect("request"))
            .await
            .expect("response")
    }

    async fn json(resp: Response) -> Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 22)
            .await
            .expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[tokio::test]
    async fn health_is_open() {
        let resp = get("/health", None).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json(resp).await;
        assert_eq!(v["status"], "ok");
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(v["providers"], serde_json::json!(["demo"]));
    }

    #[tokio::test]
    async fn v1_requires_the_token() {
        for token in [None, Some("wrong-token-0123456789")] {
            let resp = get("/v1/stocks", token).await;
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(json(resp).await["error"]["code"], "unauthorized");
        }
    }

    #[tokio::test]
    async fn lists_the_watchlist() {
        let resp = get("/v1/stocks", Some(TOKEN)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json(resp).await;
        assert_eq!(v["provider"], "demo");
        assert!(v["as_of"].as_str().unwrap().ends_with('Z'));
        let items = v["items"].as_array().unwrap();
        let symbols: Vec<&str> = items
            .iter()
            .map(|i| i["symbol"].as_str().unwrap())
            .collect();
        assert_eq!(symbols, vec!["COFORGE", "MCX", "M&M"]);
        for item in items {
            assert_eq!(item["sparkline"].as_array().unwrap().len(), 30);
            assert!(item["setup"].is_null() || item["setup"]["preset"].is_string());
        }
    }

    #[tokio::test]
    async fn lists_requested_symbols_with_sizing() {
        let resp = get(
            "/v1/stocks?symbols=iex,cams&capital=100000&risk=0.02",
            Some(TOKEN),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json(resp).await;
        assert_eq!(v["items"].as_array().unwrap().len(), 2);
        assert_eq!(v["items"][0]["symbol"], "IEX");

        for bad in [
            "/v1/stocks?risk=0.5",
            "/v1/stocks?capital=-1",
            "/v1/stocks?symbols=B@D",
        ] {
            let resp = get(bad, Some(TOKEN)).await;
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{bad}");
            assert_eq!(json(resp).await["error"]["code"], "bad_request");
        }
    }

    #[tokio::test]
    async fn shows_one_stock() {
        let resp = get("/v1/stocks/coforge", Some(TOKEN)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let v = json(resp).await;
        assert_eq!(v["provider"], "demo");
        assert_eq!(v["quote"]["symbol"], "COFORGE");
        assert_eq!(v["quote"]["name"], "Coforge");
        assert_eq!(v["candles"].as_array().unwrap().len(), 300);
        assert!(!v["overlays"]["sma200"].as_array().unwrap().is_empty());
        assert!(v["snapshot"]["rsi14"].is_number());
        assert!(v["zones"].is_array());
        for key in ["quote", "candles", "overlays", "snapshot", "zones", "setup"] {
            assert!(v.get(key).is_some(), "missing {key}");
        }

        let weekly = json(get("/v1/stocks/MCX?interval=1w", Some(TOKEN)).await).await;
        assert!(weekly["candles"].as_array().unwrap().len() >= 60);
        let intraday = json(get("/v1/stocks/MCX?interval=15m", Some(TOKEN)).await).await;
        assert_eq!(intraday["candles"].as_array().unwrap().len(), 175);
    }

    #[tokio::test]
    async fn rejects_bad_detail_requests() {
        let resp = get("/v1/stocks/NOT%20OK", Some(TOKEN)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let resp = get("/v1/stocks/MCX?interval=1h", Some(TOKEN)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let resp = get(&format!("/v1/stocks/{}", "A".repeat(21)), Some(TOKEN)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_routes_are_json_404s() {
        let resp = get("/nope", None).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(json(resp).await["error"]["code"], "not_found");
    }

    #[test]
    fn wildcard_cors_does_not_panic() {
        assert!(cors_layer(&["*".to_owned()]).is_ok());
        assert!(cors_layer(&["http://localhost:3000".to_owned()]).is_ok());
        assert!(cors_layer(&[]).is_ok());
    }
}
