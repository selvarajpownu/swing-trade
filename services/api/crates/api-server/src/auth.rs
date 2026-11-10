//! Bearer-token authentication for `/v1/*`.

use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::state::AppState;

/// Middleware: lets the request through only with `Authorization: Bearer <API_TOKEN>`.
/// When no token is configured (`ALLOW_NO_TOKEN=true`) every request passes.
pub async fn require_token(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let authorised = match state.config.api_token() {
        None => true,
        Some(expected) => req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(bearer_token)
            .is_some_and(|given| constant_time_eq(given.as_bytes(), expected.as_bytes())),
    };
    if authorised {
        next.run(req).await
    } else {
        ApiError::Unauthorized.into_response()
    }
}

/// The token from a `Bearer <token>` header value (scheme is case-insensitive).
pub fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.trim().split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

/// Compares two byte strings in time that depends only on their lengths.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bearer_header() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer   abc "), Some("abc"));
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("Bearer"), None);
        assert_eq!(bearer_token("Bearer  "), None);
    }

    #[test]
    fn compares_tokens() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secret!"));
        assert!(constant_time_eq(b"", b""));
    }
}
