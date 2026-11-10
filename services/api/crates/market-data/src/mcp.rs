//! A minimal Model Context Protocol client over Streamable HTTP.
//!
//! It speaks just enough JSON-RPC 2.0 to `initialize` a session and call
//! tools (protocol version [`PROTOCOL_VERSION`]). Responses may come back as
//! plain JSON or as a server-sent event stream; both are handled.
//!
//! Authentication is delegated to a [`TokenSource`] so OAuth refresh can be
//! plugged in without this module knowing about it.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE};
use reqwest::StatusCode;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::{Error, Result};

/// MCP protocol revision this client implements.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// `clientInfo.name` sent on `initialize`.
pub const CLIENT_NAME: &str = "swing-trade";

const SESSION_HEADER: &str = "mcp-session-id";
const VERSION_HEADER: &str = "mcp-protocol-version";
const ACCEPT_VALUE: &str = "application/json, text/event-stream";

/// Supplies bearer tokens for MCP requests.
#[async_trait]
pub trait TokenSource: Send + Sync {
    /// The token to send now. May refresh proactively when close to expiry.
    async fn token(&self) -> Result<String>;

    /// Called once after the server rejects `rejected` with HTTP 401.
    /// Return a different token to retry with, or `None` to give up.
    async fn refresh(&self, rejected: &str) -> Result<Option<String>> {
        let _ = rejected;
        Ok(None)
    }
}

/// A fixed token, e.g. from an environment variable. Cannot be refreshed.
#[derive(Clone)]
pub struct StaticToken(String);

impl StaticToken {
    /// Wraps a ready-made access token.
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }
}

impl fmt::Debug for StaticToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StaticToken(<redacted>)")
    }
}

#[async_trait]
impl TokenSource for StaticToken {
    async fn token(&self) -> Result<String> {
        Ok(self.0.clone())
    }
}

/// State negotiated by `initialize`.
#[derive(Debug, Clone)]
struct Session {
    /// `Mcp-Session-Id` assigned by the server, if it uses sessions.
    id: Option<String>,
    /// Protocol version the server agreed to.
    protocol_version: String,
    /// Increments on every new session, so a stale 404 cannot drop a newer one.
    generation: u64,
}

/// MCP client bound to one server URL.
pub struct McpClient {
    http: reqwest::Client,
    url: String,
    provider: &'static str,
    tokens: Arc<dyn TokenSource>,
    session: Mutex<Option<Session>>,
    next_id: AtomicU64,
    generation: AtomicU64,
}

impl McpClient {
    /// Creates a client. `provider` labels errors (e.g. `"indmoney"`).
    pub fn new(
        provider: &'static str,
        url: impl Into<String>,
        tokens: Arc<dyn TokenSource>,
    ) -> Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("swing-trade/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            url: url.into(),
            provider,
            tokens,
            session: Mutex::new(None),
            next_id: AtomicU64::new(1),
            generation: AtomicU64::new(0),
        })
    }

    /// The server URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Calls a tool and returns its output: the first `text` content item
    /// parsed as JSON, or that text as a JSON string when it is not JSON.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let result = self
            .request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await?;
        tool_output(self.provider, name, &result)
    }

    /// Sends a JSON-RPC request inside a session and returns its `result`.
    /// Re-initialises once if the server has forgotten the session (404).
    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let mut reinitialised = false;
        loop {
            let session = self.session().await?;
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let body = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params.clone(),
            });
            let resp = self.post(&body, Some(&session)).await?;
            if resp.status() == StatusCode::NOT_FOUND && session.id.is_some() && !reinitialised {
                tracing::debug!(
                    provider = self.provider,
                    "MCP session expired; re-initialising"
                );
                reinitialised = true;
                self.reset_session(session.generation).await;
                continue;
            }
            return self.read_result(resp, id).await;
        }
    }

    /// The current session, initialising one if needed.
    async fn session(&self) -> Result<Session> {
        let mut guard = self.session.lock().await;
        if let Some(s) = guard.as_ref() {
            return Ok(s.clone());
        }
        let s = self.initialize().await?;
        *guard = Some(s.clone());
        Ok(s)
    }

    async fn reset_session(&self, generation: u64) {
        let mut guard = self.session.lock().await;
        if guard.as_ref().is_some_and(|s| s.generation == generation) {
            *guard = None;
        }
    }

    /// `initialize` followed by `notifications/initialized`.
    async fn initialize(&self) -> Result<Session> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION") },
            },
        });
        let resp = self.post(&body, None).await?;
        let session_id = resp
            .headers()
            .get(SESSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let result = self.read_result(resp, id).await?;
        let protocol_version = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(PROTOCOL_VERSION)
            .to_owned();
        let session = Session {
            id: session_id,
            protocol_version,
            generation: self.generation.fetch_add(1, Ordering::Relaxed) + 1,
        };

        let note = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        let resp = self.post(&note, Some(&session)).await?;
        if !resp.status().is_success() {
            tracing::warn!(
                provider = self.provider,
                status = %resp.status(),
                "MCP server rejected notifications/initialized"
            );
        }
        tracing::debug!(
            provider = self.provider,
            protocol = %session.protocol_version,
            has_session = session.id.is_some(),
            "MCP session initialised"
        );
        Ok(session)
    }

    /// POSTs one JSON-RPC message, retrying once with a refreshed token on 401.
    async fn post(&self, body: &Value, session: Option<&Session>) -> Result<reqwest::Response> {
        let mut token = self.tokens.token().await?;
        let mut refreshed = false;
        loop {
            let mut req = self
                .http
                .post(&self.url)
                .header(ACCEPT, ACCEPT_VALUE)
                .bearer_auth(&token)
                .json(body);
            if let Some(s) = session {
                if let Some(id) = &s.id {
                    req = req.header(SESSION_HEADER, id.as_str());
                }
                req = req.header(VERSION_HEADER, s.protocol_version.as_str());
            }
            let resp = req.send().await?;
            if resp.status() != StatusCode::UNAUTHORIZED {
                return Ok(resp);
            }
            if !refreshed {
                if let Some(next) = self.tokens.refresh(&token).await? {
                    token = next;
                    refreshed = true;
                    continue;
                }
            }
            return Err(Error::Unauthorized {
                provider: self.provider,
            });
        }
    }

    /// Reads the JSON-RPC response with this `id` and returns its `result`.
    async fn read_result(&self, mut resp: reqwest::Response, id: u64) -> Result<Value> {
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(self.upstream(format!("HTTP {status}: {}", snippet(&text))));
        }

        let message = if is_event_stream(resp.headers()) {
            let mut parser = SseParser::default();
            let mut found = None;
            while let Some(chunk) = resp.chunk().await? {
                found = parser
                    .push(&chunk)
                    .into_iter()
                    .find_map(|data| response_in(&data, id));
                if found.is_some() {
                    break;
                }
            }
            found
                .or_else(|| parser.finish().and_then(|data| response_in(&data, id)))
                .ok_or_else(|| self.decode("event stream ended without a response"))?
        } else {
            let text = resp.text().await?;
            response_in(&text, id)
                .ok_or_else(|| self.decode(format!("no JSON-RPC response: {}", snippet(&text))))?
        };
        rpc_result(self.provider, message)
    }

    fn upstream(&self, message: impl Into<String>) -> Error {
        Error::Upstream {
            provider: self.provider,
            message: message.into(),
        }
    }

    fn decode(&self, message: impl Into<String>) -> Error {
        Error::Decode {
            provider: self.provider,
            message: message.into(),
        }
    }
}

fn is_event_stream(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.trim_start().starts_with("text/event-stream"))
}

/// Finds the JSON-RPC response with `id` in a payload holding one message
/// or a batch. Requests and notifications from the server are ignored.
fn response_in(payload: &str, id: u64) -> Option<Value> {
    let value: Value = serde_json::from_str(payload).ok()?;
    let matches = |m: &Value| {
        let has_body = m.get("result").is_some() || m.get("error").is_some();
        let id_ok = m
            .get("id")
            .is_some_and(|v| v.as_u64() == Some(id) || v.as_str() == Some(id.to_string().as_str()));
        has_body && id_ok
    };
    match value {
        Value::Array(items) => items.into_iter().find(|m| matches(m)),
        m if matches(&m) => Some(m),
        _ => None,
    }
}

/// `result` of a JSON-RPC response, or its `error` as [`Error::Upstream`].
fn rpc_result(provider: &'static str, message: Value) -> Result<Value> {
    if let Some(err) = message.get("error") {
        let text = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        let code = err.get("code").and_then(Value::as_i64).unwrap_or_default();
        return Err(Error::Upstream {
            provider,
            message: format!("{text} (JSON-RPC code {code})"),
        });
    }
    match message {
        Value::Object(mut map) => map.remove("result").ok_or_else(|| Error::Decode {
            provider,
            message: "JSON-RPC response has no result".to_owned(),
        }),
        _ => Err(Error::Decode {
            provider,
            message: "JSON-RPC response is not an object".to_owned(),
        }),
    }
}

/// Interprets a `tools/call` result.
fn tool_output(provider: &'static str, tool: &str, result: &Value) -> Result<Value> {
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|i| i.get("type").and_then(Value::as_str) == Some("text"))
        })
        .and_then(|i| i.get("text"))
        .and_then(Value::as_str);

    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(Error::Upstream {
            provider,
            message: format!(
                "tool {tool} failed: {}",
                snippet(text.unwrap_or("no details"))
            ),
        });
    }
    if let Some(text) = text {
        return Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())));
    }
    if let Some(structured) = result.get("structuredContent") {
        return Ok(structured.clone());
    }
    Err(Error::Decode {
        provider,
        message: format!("tool {tool} returned no text content"),
    })
}

/// Trims and shortens upstream text for error messages.
pub(crate) fn snippet(text: &str) -> String {
    const MAX: usize = 300;
    let text = text.trim();
    if text.chars().count() > MAX {
        format!("{}…", text.chars().take(MAX).collect::<String>())
    } else {
        text.to_owned()
    }
}

/// Incremental parser for `text/event-stream` bodies.
///
/// Feed raw bytes with [`push`](Self::push); it returns the `data` payload of
/// every event completed so far (multi-line data joined with `\n`).
#[derive(Debug, Default)]
pub(crate) struct SseParser {
    line: Vec<u8>,
    data: Vec<String>,
}

impl SseParser {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut events = Vec::new();
        for &b in bytes {
            if b == b'\n' {
                let mut line = std::mem::take(&mut self.line);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if let Some(event) = self.line_done(&line) {
                    events.push(event);
                }
            } else {
                self.line.push(b);
            }
        }
        events
    }

    /// Flushes a final event that was not followed by a blank line.
    pub(crate) fn finish(mut self) -> Option<String> {
        let mut line = std::mem::take(&mut self.line);
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if !line.is_empty() {
            if let Some(event) = self.line_done(&line) {
                return Some(event);
            }
        }
        self.dispatch()
    }

    fn line_done(&mut self, line: &[u8]) -> Option<String> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line[0] == b':' {
            return None; // comment / keep-alive
        }
        let line = String::from_utf8_lossy(line);
        let (field, value) = match line.find(':') {
            Some(i) => {
                let value = &line[i + 1..];
                (&line[..i], value.strip_prefix(' ').unwrap_or(value))
            }
            None => (&line[..], ""),
        };
        if field == "data" {
            self.data.push(value.to_owned());
        }
        None
    }

    fn dispatch(&mut self) -> Option<String> {
        if self.data.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.data).join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_parser_handles_split_chunks_and_crlf() {
        let mut p = SseParser::default();
        assert!(p.push(b"event: message\r\nda").is_empty());
        assert!(p.push(b"ta: {\"a\":").is_empty());
        let events = p.push(b"1}\r\n\r\n: keep-alive\n\ndata:x\ndata: y\n\n");
        assert_eq!(events, vec!["{\"a\":1}".to_owned(), "x\ny".to_owned()]);
    }

    #[test]
    fn sse_parser_flushes_trailing_event() {
        let mut p = SseParser::default();
        assert!(p.push(b"id: 7\ndata: {\"id\":1}").is_empty());
        assert_eq!(p.finish().as_deref(), Some("{\"id\":1}"));
        assert_eq!(SseParser::default().finish(), None);
    }

    #[test]
    fn sse_parser_keeps_multibyte_text_across_chunks() {
        let mut p = SseParser::default();
        let bytes = "data: ₹100\n\n".as_bytes();
        let (a, b) = bytes.split_at(8); // splits the rupee sign
        assert!(p.push(a).is_empty());
        assert_eq!(p.push(b), vec!["₹100".to_owned()]);
    }

    #[test]
    fn finds_matching_response() {
        let note = r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{}}"#;
        assert_eq!(response_in(note, 1), None);
        let other = r#"{"jsonrpc":"2.0","id":2,"result":{}}"#;
        assert_eq!(response_in(other, 1), None);
        let ok = r#"{"jsonrpc":"2.0","id":1,"result":{"x":1}}"#;
        assert_eq!(response_in(ok, 1).unwrap()["result"]["x"], 1);
        let batch =
            r#"[{"jsonrpc":"2.0","id":2,"result":{}},{"jsonrpc":"2.0","id":"3","result":{"y":2}}]"#;
        assert_eq!(response_in(batch, 3).unwrap()["result"]["y"], 2);
        assert_eq!(response_in("not json", 1), None);
    }

    #[test]
    fn maps_rpc_errors() {
        let err = rpc_result(
            "x",
            json!({"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"bad params"}}),
        )
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("bad params (JSON-RPC code -32602)"));
        let ok = rpc_result("x", json!({"jsonrpc":"2.0","id":1,"result":{"a":true}})).unwrap();
        assert_eq!(ok, json!({"a": true}));
    }

    #[test]
    fn interprets_tool_results() {
        let json_text = json!({"content":[{"type":"text","text":"[{\"ind_key\":\"INDS1\"}]"}]});
        assert_eq!(
            tool_output("x", "t", &json_text).unwrap(),
            json!([{"ind_key": "INDS1"}])
        );

        let plain = json!({"content":[{"type":"image","data":""},{"type":"text","text":"hello"}]});
        assert_eq!(tool_output("x", "t", &plain).unwrap(), json!("hello"));

        let failed = json!({"isError":true,"content":[{"type":"text","text":"no such key"}]});
        match tool_output("x", "t", &failed).unwrap_err() {
            Error::Upstream { message, .. } => assert!(message.contains("no such key")),
            e => panic!("unexpected {e:?}"),
        }

        let structured = json!({"content":[],"structuredContent":{"a":1}});
        assert_eq!(tool_output("x", "t", &structured).unwrap(), json!({"a": 1}));
        assert!(tool_output("x", "t", &json!({"content": []})).is_err());
    }

    #[test]
    fn snippet_truncates_on_char_boundaries() {
        let long = "₹".repeat(400);
        let s = snippet(&long);
        assert_eq!(s.chars().count(), 301);
        assert_eq!(snippet("  hi \n"), "hi");
    }
}
