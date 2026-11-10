//! HTTP routes and middleware stack.

pub mod health;
pub mod news;
pub mod scan;
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
        .route("/stocks/{symbol}/news", get(stocks::news))
        .route("/news", get(news::headlines))
        .route("/scan", get(scan::list))
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
    async fn scans_the_universe_in_the_background() {
        // A wide demo universe so some presets fire; INDRAMEDCO must never show.
        let universe = "COFORGE,MCX,CAMS,IEX,ACE,AJAXENGG,ASKAUTOLTD,PERSISTENT,OFSS,BSE,                        INDRAMEDCO,TCS,INFY,HDFCBANK,ICICIBANK,SBIN,LT,ITC,TITAN,ASIANPAINT,                        MARUTI,SUNPHARMA,LUPIN,NESTLEIND,HAL,BEL,TRENT,DMART,POLYCAB,DIXON";
        let config = crate::config::tests::config_from(&[
            "--api-token",
            TOKEN,
            "--data-providers",
            "demo",
            "--universe",
            universe,
        ]);
        let universe = crate::universe::load(&config).unwrap();
        let market = build_market(&config).unwrap();
        let state = AppState::new(config, market);
        let app = router(state.clone()).unwrap();
        let call = |uri: &str| {
            let req = Request::builder()
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap();
            app.clone().oneshot(req)
        };

        let before = json(call("/v1/scan").await.unwrap()).await;
        assert_eq!(before["status"]["state"], "pending");
        assert_eq!(before["items"].as_array().unwrap().len(), 0);

        crate::scan::run_once(&state, &universe).await;
        let v = json(call("/v1/scan?limit=500").await.unwrap()).await;
        assert_eq!(v["provider"], "demo");
        assert_eq!(v["status"]["state"], "idle");
        assert_eq!(v["status"]["universe_size"], 29);
        assert_eq!(v["status"]["scanned"], 29);
        assert_eq!(v["status"]["excluded"], serde_json::json!(["INDRAMEDCO"]));
        assert!(v["status"]["finished_at"].as_str().unwrap().ends_with('Z'));
        let items = v["items"].as_array().unwrap();
        assert!(!items.is_empty(), "no demo setups fired");
        assert_eq!(v["total"].as_u64().unwrap() as usize, items.len());
        let scores: Vec<u64> = items
            .iter()
            .map(|i| i["setup"]["score"].as_u64().unwrap())
            .collect();
        assert!(scores.windows(2).all(|w| w[0] >= w[1]), "{scores:?}");
        for item in items {
            assert_ne!(item["symbol"], "INDRAMEDCO");
            assert!(item["presets"]
                .as_array()
                .unwrap()
                .contains(&item["setup"]["preset"]));
            assert!(item["bar_time"].is_i64());
            assert_eq!(item["sparkline"].as_array().unwrap().len(), 30);
        }

        // Filters narrow the list; quantity follows capital and risk.
        let best = &items[0];
        let preset = best["setup"]["preset"].as_str().unwrap();
        let only = json(
            call(&format!("/v1/scan?preset={preset}&limit=500"))
                .await
                .unwrap(),
        )
        .await;
        for item in only["items"].as_array().unwrap() {
            assert_eq!(item["setup"]["preset"], preset);
        }
        let strict = json(call("/v1/scan?min_score=100&min_rr=50").await.unwrap()).await;
        assert_eq!(strict["total"], 0);
        let one = json(
            call("/v1/scan?limit=1&capital=1000000&risk=0.02")
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(one["items"].as_array().unwrap().len(), 1);
        let q1 = best["setup"]["quantity"].as_u64().unwrap();
        let q4 = one["items"][0]["setup"]["quantity"].as_u64().unwrap();
        assert!(q4 >= q1 * 4 - 1 && q4 <= q1 * 4 + 4, "{q1} -> {q4}");

        let bad = call("/v1/scan?preset=nope").await.unwrap();
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn serves_us_and_commodity_markets() {
        let us = json(get("/v1/stocks?market=us", Some(TOKEN)).await).await;
        assert_eq!(us["market"], "us");
        assert_eq!(us["currency"], "USD");
        let symbols: Vec<&str> = us["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["symbol"].as_str().unwrap())
            .collect();
        assert_eq!(
            symbols,
            vec!["AAPL", "MSFT", "NVDA", "GOOGL", "AMZN", "META", "TSLA"]
        );
        assert_eq!(us["items"][0]["name"], "Apple");

        let mcx = json(get("/v1/stocks?market=mcx", Some(TOKEN)).await).await;
        assert_eq!(mcx["currency"], "INR");
        for item in mcx["items"].as_array().unwrap() {
            assert!(!item["symbol"].as_str().unwrap().contains(':'));
            if !item["setup"].is_null() {
                assert!(
                    item["setup"]["quantity"].is_null(),
                    "MCX plans have no quantity"
                );
            }
        }

        let aapl = json(get("/v1/stocks/aapl?market=us&interval=1w", Some(TOKEN)).await).await;
        assert_eq!(aapl["quote"]["symbol"], "AAPL");
        assert_eq!(aapl["quote"]["exchange"], "US");
        assert_eq!(aapl["currency"], "USD");
        let gold = json(get("/v1/stocks/GOLD?market=mcx", Some(TOKEN)).await).await;
        assert_eq!(gold["market"], "mcx");
        if !gold["setup"].is_null() {
            assert!(gold["setup"]["quantity"].is_null());
        }

        let resp = get("/v1/stocks/AAPL?market=us&interval=15m", Some(TOKEN)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let resp = get("/v1/stocks?market=lse", Some(TOKEN)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Demo has no news: an empty list, never invented headlines.
        let news = json(get("/v1/stocks/AAPL/news?market=us", Some(TOKEN)).await).await;
        assert_eq!(news["provider"], "demo");
        assert_eq!(news["symbol"], "AAPL");
        assert_eq!(news["items"], serde_json::json!([]));
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
