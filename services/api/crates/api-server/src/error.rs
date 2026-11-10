//! API errors and their JSON form:
//! `{"error": {"code": "...", "message": "..."}}`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Every error the API returns.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// 400: the request is malformed.
    #[error("{0}")]
    BadRequest(String),
    /// 401: missing or wrong API token.
    #[error("missing or invalid API token")]
    Unauthorized,
    /// 404: unknown symbol or route.
    #[error("{0}")]
    NotFound(String),
    /// 502: a market-data provider failed.
    #[error("{0}")]
    Upstream(String),
    /// 500: a bug or local failure. The message is logged, not returned.
    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    /// HTTP status for this error.
    pub fn status(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Upstream(_) => StatusCode::BAD_GATEWAY,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Machine-readable code from the API contract.
    pub fn code(&self) -> &'static str {
        match self {
            ApiError::BadRequest(_) => "bad_request",
            ApiError::Unauthorized => "unauthorized",
            ApiError::NotFound(_) => "not_found",
            ApiError::Upstream(_) => "upstream",
            ApiError::Internal(_) => "internal",
        }
    }
}

/// JSON body of an error response.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

/// Code and human-readable message.
#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: &'static str,
    pub message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let message = match &self {
            ApiError::Internal(detail) => {
                tracing::error!(error = %detail, "internal error");
                "internal server error".to_owned()
            }
            ApiError::Upstream(detail) => {
                tracing::warn!(error = %detail, "upstream error");
                detail.clone()
            }
            other => other.to_string(),
        };
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message,
            },
        };
        (self.status(), Json(body)).into_response()
    }
}

impl From<market_data::Error> for ApiError {
    fn from(e: market_data::Error) -> Self {
        use market_data::Error as E;
        match e {
            E::NotFound(symbol) => ApiError::NotFound(format!("symbol not found: {symbol}")),
            E::Unauthorized { provider } => ApiError::Upstream(format!(
                "market data provider `{provider}` is not authorised; \
                 run `swing-api login {provider}` on the server"
            )),
            E::Io(io) => ApiError::Internal(format!("io error: {io}")),
            other @ (E::Upstream { .. }
            | E::Decode { .. }
            | E::Http(_)
            | E::AllProvidersFailed(_)) => ApiError::Upstream(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 16)
            .await
            .expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    #[test]
    fn maps_market_data_errors() {
        let e: ApiError = market_data::Error::NotFound("XYZ".into()).into();
        assert_eq!(e.status(), StatusCode::NOT_FOUND);
        assert_eq!(e.code(), "not_found");

        let e: ApiError = market_data::Error::Unauthorized {
            provider: "indmoney",
        }
        .into();
        assert_eq!(e.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(e.code(), "upstream");
        assert!(e.to_string().contains("swing-api login indmoney"));

        let e: ApiError = market_data::Error::AllProvidersFailed("a; b".into()).into();
        assert_eq!(e.status(), StatusCode::BAD_GATEWAY);

        let e: ApiError = market_data::Error::Decode {
            provider: "indmoney",
            message: "bad".into(),
        }
        .into();
        assert_eq!(e.code(), "upstream");

        let e: ApiError = market_data::Error::Io(std::io::Error::other("disk")).into();
        assert_eq!(e.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(e.code(), "internal");
    }

    #[tokio::test]
    async fn renders_contract_json() {
        let resp = ApiError::BadRequest("risk must be small".into()).into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let v = body_json(resp).await;
        assert_eq!(v["error"]["code"], "bad_request");
        assert_eq!(v["error"]["message"], "risk must be small");

        let v = body_json(ApiError::Unauthorized.into_response()).await;
        assert_eq!(v["error"]["code"], "unauthorized");

        let resp = ApiError::Internal("secret path /etc/x".into()).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let v = body_json(resp).await;
        assert_eq!(v["error"]["message"], "internal server error");
    }
}
