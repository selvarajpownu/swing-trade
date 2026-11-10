//! OAuth 2.1 login (authorization code + PKCE) for the INDmoney MCP server,
//! and a [`TokenSource`] that keeps the stored tokens fresh.
//!
//! The flow follows the MCP authorization spec: discover the protected
//! resource metadata, then the authorization server metadata, register a
//! public client dynamically, send the user to the browser (INDmoney signs
//! in with mobile number, OTP and MPIN), catch the redirect on a loopback
//! port and exchange the code. Tokens are saved as JSON with owner-only
//! permissions on unix.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::Rng;
use reqwest::header::{ACCEPT, WWW_AUTHENTICATE};
use reqwest::Url;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use super::ID;
use crate::mcp::{snippet, StaticToken, TokenSource, CLIENT_NAME, PROTOCOL_VERSION};
use crate::{Error, Result};

/// Default location of the saved tokens, relative to the working directory.
pub const DEFAULT_TOKEN_FILE: &str = ".data/indmoney-token.json";
/// Environment variable that overrides the token file with a fixed token.
pub const ACCESS_TOKEN_ENV: &str = "INDMONEY_ACCESS_TOKEN";
/// Loopback port the browser is redirected to after sign-in.
pub const REDIRECT_PORT: u16 = 8765;

const CALLBACK_PATH: &str = "/callback";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Refresh this long before the access token expires.
const REFRESH_MARGIN_SECS: i64 = 120;
const PKCE_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// Tokens persisted after login.
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub client_id: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Unix seconds; `None` when the server did not say.
    #[serde(default)]
    pub expires_at: Option<i64>,
    pub token_endpoint: String,
    /// RFC 8707 resource indicator (the MCP URL) sent with token requests.
    #[serde(default)]
    pub resource: Option<String>,
}

impl fmt::Debug for StoredToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredToken")
            .field("client_id", &self.client_id)
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_at", &self.expires_at)
            .field("token_endpoint", &self.token_endpoint)
            .finish()
    }
}

impl StoredToken {
    /// Reads tokens from `path`. A missing file means "not logged in".
    pub fn load(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::Unauthorized { provider: ID })
            }
            Err(e) => return Err(e.into()),
        };
        serde_json::from_str(&text).map_err(|e| decode(format!("{}: {e}", path.display())))
    }

    /// Writes tokens to `path` atomically, readable only by the owner on unix.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| decode(e.to_string()))?;
        let tmp = path.with_extension("json.tmp");
        write_private(&tmp, &json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    fn expires_soon(&self, now: i64) -> bool {
        self.expires_at
            .is_some_and(|t| t - REFRESH_MARGIN_SECS <= now)
    }
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // `mode` only applies on creation; tighten a pre-existing file too.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

/// Picks the token source for INDmoney: `INDMONEY_ACCESS_TOKEN` if set,
/// otherwise the token file if it exists. `None` means not configured.
pub fn token_source(token_file: &Path) -> Option<Arc<dyn TokenSource>> {
    if let Ok(token) = std::env::var(ACCESS_TOKEN_ENV) {
        let token = token.trim();
        if !token.is_empty() {
            return Some(Arc::new(StaticToken::new(token)));
        }
    }
    if !token_file.exists() {
        return None;
    }
    match FileTokenSource::new(token_file) {
        Ok(source) => Some(Arc::new(source)),
        Err(e) => {
            tracing::warn!(error = %e, "could not create INDmoney token source");
            None
        }
    }
}

/// Serves the access token from the token file and refreshes it with the
/// refresh token when it is about to expire or the server rejects it.
pub struct FileTokenSource {
    path: PathBuf,
    http: reqwest::Client,
    current: Mutex<Option<StoredToken>>,
}

impl FileTokenSource {
    /// Tokens are read lazily from `path` on first use.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            path: path.into(),
            http: http_client()?,
            current: Mutex::new(None),
        })
    }

    /// Location of the token file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    async fn refresh_and_save(&self, stored: &StoredToken) -> Result<StoredToken> {
        let fresh = refresh_grant(&self.http, stored).await?;
        fresh.save(&self.path)?;
        tracing::info!("refreshed INDmoney access token");
        Ok(fresh)
    }
}

#[async_trait]
impl TokenSource for FileTokenSource {
    async fn token(&self) -> Result<String> {
        let mut slot = self.current.lock().await;
        let stored = match slot.take() {
            Some(s) => s,
            None => StoredToken::load(&self.path)?,
        };
        let stored = if stored.expires_soon(now_secs()) && stored.refresh_token.is_some() {
            match self.refresh_and_save(&stored).await {
                Ok(fresh) => fresh,
                Err(e) => {
                    // Let the server decide; a 401 will trigger `refresh` again.
                    tracing::warn!(error = %e, "proactive INDmoney token refresh failed");
                    stored
                }
            }
        } else {
            stored
        };
        let token = stored.access_token.clone();
        *slot = Some(stored);
        Ok(token)
    }

    async fn refresh(&self, rejected: &str) -> Result<Option<String>> {
        let mut slot = self.current.lock().await;
        // Another request may already have refreshed, or the user may have
        // logged in again while the server was running.
        if let Some(s) = slot.as_ref().filter(|s| s.access_token != rejected) {
            return Ok(Some(s.access_token.clone()));
        }
        let stored = StoredToken::load(&self.path)?;
        if stored.access_token != rejected {
            let token = stored.access_token.clone();
            *slot = Some(stored);
            return Ok(Some(token));
        }
        if stored.refresh_token.is_none() {
            return Ok(None);
        }
        let fresh = self.refresh_and_save(&stored).await?;
        let token = fresh.access_token.clone();
        *slot = Some(fresh);
        Ok(Some(token))
    }
}

/// Options for [`login`].
#[derive(Debug, Clone)]
pub struct LoginOptions {
    /// MCP server URL; also sent as the OAuth `resource`.
    pub mcp_url: String,
    /// Where to save the tokens.
    pub token_file: PathBuf,
    /// Try to open the authorization URL in the default browser.
    pub open_browser: bool,
    /// Use a pre-registered client instead of dynamic registration.
    pub client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResourceMetadata {
    #[serde(default)]
    authorization_servers: Vec<String>,
    #[serde(default)]
    scopes_supported: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AuthServerMetadata {
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default)]
    registration_endpoint: Option<String>,
    #[serde(default)]
    code_challenge_methods_supported: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<Value>,
}

impl TokenResponse {
    fn expires_at(&self, now: i64) -> Option<i64> {
        let secs = match self.expires_in.as_ref()? {
            Value::Number(n) => n.as_f64()?,
            Value::String(s) => s.trim().parse().ok()?,
            _ => return None,
        };
        (secs.is_finite() && secs > 0.0).then(|| now + secs as i64)
    }
}

/// Runs the interactive browser login and saves the tokens.
///
/// Prints the authorization URL to stdout, so call it from a CLI.
pub async fn login(opts: &LoginOptions) -> Result<StoredToken> {
    let http = http_client()?;
    let mcp_url =
        Url::parse(&opts.mcp_url).map_err(|e| upstream(format!("invalid MCP URL: {e}")))?;

    let resource_meta = discover_resource_metadata(&http, &mcp_url).await?;
    let issuer = resource_meta
        .authorization_servers
        .first()
        .cloned()
        .ok_or_else(|| upstream("protected resource metadata lists no authorization server"))?;
    let server = discover_auth_server(&http, &issuer).await?;
    if !server.code_challenge_methods_supported.is_empty()
        && !server
            .code_challenge_methods_supported
            .iter()
            .any(|m| m == "S256")
    {
        tracing::warn!("authorization server does not advertise PKCE S256; trying anyway");
    }

    // Bind before showing the URL so the redirect cannot arrive too early.
    let redirect_uri = format!("http://127.0.0.1:{REDIRECT_PORT}{CALLBACK_PATH}");
    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT))
        .await
        .map_err(|e| upstream(format!("cannot listen on 127.0.0.1:{REDIRECT_PORT}: {e}")))?;

    let client_id = match &opts.client_id {
        Some(id) => id.clone(),
        None => register_client(&http, &server, &redirect_uri).await?,
    };

    let verifier = pkce_verifier();
    let state = random_state();
    let auth_url = authorization_url(
        &server.authorization_endpoint,
        &client_id,
        &redirect_uri,
        &pkce_challenge(&verifier),
        &state,
        &opts.mcp_url,
        &resource_meta.scopes_supported,
    )?;

    println!("\nSign in to INDmoney in your browser to connect swing-trade (read-only).");
    println!("If it does not open automatically, visit:\n\n  {auth_url}\n");
    if opts.open_browser && !open_browser(auth_url.as_str()) {
        tracing::debug!("could not open a browser automatically");
    }

    let code = tokio::time::timeout(LOGIN_TIMEOUT, wait_for_callback(&listener, &state))
        .await
        .map_err(|_| upstream("timed out waiting for the browser sign-in"))??;

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("client_id", client_id.as_str()),
        ("code_verifier", verifier.as_str()),
        ("resource", opts.mcp_url.as_str()),
    ];
    let token = token_request(&http, &server.token_endpoint, &params).await?;
    let stored = StoredToken {
        client_id,
        expires_at: token.expires_at(now_secs()),
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        token_endpoint: server.token_endpoint,
        resource: Some(opts.mcp_url.clone()),
    };
    stored.save(&opts.token_file)?;
    Ok(stored)
}

/// RFC 9728 metadata: root well-known URL, then the path-suffixed one, then
/// the `resource_metadata` link from a 401's `WWW-Authenticate` header.
async fn discover_resource_metadata(
    http: &reqwest::Client,
    mcp_url: &Url,
) -> Result<ResourceMetadata> {
    let origin = mcp_url.origin().ascii_serialization();
    let path = mcp_url.path().trim_end_matches('/');
    let mut urls = vec![format!("{origin}/.well-known/oauth-protected-resource")];
    if !path.is_empty() {
        urls.push(format!(
            "{origin}/.well-known/oauth-protected-resource{path}"
        ));
    }
    for url in &urls {
        if let Some(meta) = get_json::<ResourceMetadata>(http, url).await {
            if !meta.authorization_servers.is_empty() {
                return Ok(meta);
            }
        }
    }

    let probe = json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION") },
        },
    });
    let resp = http
        .post(mcp_url.clone())
        .header(ACCEPT, "application/json, text/event-stream")
        .json(&probe)
        .send()
        .await?;
    let linked = resp
        .headers()
        .get(WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| auth_param(h, "resource_metadata"));
    if let Some(url) = linked {
        if let Some(meta) = get_json::<ResourceMetadata>(http, &url).await {
            if !meta.authorization_servers.is_empty() {
                return Ok(meta);
            }
        }
    }
    Err(upstream(
        "could not discover the OAuth protected resource metadata",
    ))
}

/// RFC 8414 / OpenID discovery for `issuer`.
async fn discover_auth_server(http: &reqwest::Client, issuer: &str) -> Result<AuthServerMetadata> {
    for url in metadata_urls(issuer)? {
        if let Some(meta) = get_json::<AuthServerMetadata>(http, &url).await {
            return Ok(meta);
        }
    }
    Err(upstream(format!(
        "could not discover authorization server metadata for {issuer}"
    )))
}

/// Candidate metadata URLs for an issuer, in the order the MCP spec gives.
fn metadata_urls(issuer: &str) -> Result<Vec<String>> {
    let url = Url::parse(issuer).map_err(|e| upstream(format!("invalid issuer {issuer}: {e}")))?;
    let origin = url.origin().ascii_serialization();
    let path = url.path().trim_end_matches('/');
    Ok(if path.is_empty() {
        vec![
            format!("{origin}/.well-known/oauth-authorization-server"),
            format!("{origin}/.well-known/openid-configuration"),
        ]
    } else {
        vec![
            format!("{origin}/.well-known/oauth-authorization-server{path}"),
            format!("{origin}/.well-known/openid-configuration{path}"),
            format!("{origin}{path}/.well-known/openid-configuration"),
        ]
    })
}

/// RFC 7591 dynamic registration of a public client.
async fn register_client(
    http: &reqwest::Client,
    server: &AuthServerMetadata,
    redirect_uri: &str,
) -> Result<String> {
    let endpoint = server.registration_endpoint.as_deref().ok_or_else(|| {
        upstream("the authorization server does not support dynamic client registration")
    })?;
    let body = json!({
        "client_name": CLIENT_NAME,
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    });
    let resp = http.post(endpoint).json(&body).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(upstream(format!(
            "client registration failed ({status}): {}",
            snippet(&text)
        )));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| decode(format!("client registration response: {e}")))?;
    v.get("client_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| decode("client registration response has no client_id"))
}

fn authorization_url(
    endpoint: &str,
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
    resource: &str,
    scopes: &[String],
) -> Result<Url> {
    let mut url = Url::parse(endpoint)
        .map_err(|e| upstream(format!("invalid authorization endpoint: {e}")))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state)
            .append_pair("resource", resource);
        if !scopes.is_empty() {
            q.append_pair("scope", &scopes.join(" "));
        }
    }
    Ok(url)
}

/// POSTs a form to the token endpoint.
async fn token_request(
    http: &reqwest::Client,
    endpoint: &str,
    params: &[(&str, &str)],
) -> Result<TokenResponse> {
    let resp = http
        .post(endpoint)
        .header(ACCEPT, "application/json")
        .form(params)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        if text.contains("invalid_grant") || text.contains("invalid_client") {
            tracing::warn!(%status, body = %snippet(&text), "INDmoney rejected the stored grant");
            return Err(Error::Unauthorized { provider: ID });
        }
        return Err(upstream(format!(
            "token endpoint returned {status}: {}",
            snippet(&text)
        )));
    }
    serde_json::from_str(&text).map_err(|e| decode(format!("token response: {e}")))
}

async fn refresh_grant(http: &reqwest::Client, stored: &StoredToken) -> Result<StoredToken> {
    let refresh_token = stored
        .refresh_token
        .as_deref()
        .ok_or(Error::Unauthorized { provider: ID })?;
    let mut params = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", stored.client_id.as_str()),
    ];
    if let Some(resource) = stored.resource.as_deref() {
        params.push(("resource", resource));
    }
    let token = token_request(http, &stored.token_endpoint, &params).await?;
    Ok(StoredToken {
        expires_at: token.expires_at(now_secs()),
        access_token: token.access_token,
        // Servers that do not rotate refresh tokens omit it.
        refresh_token: token.refresh_token.or_else(|| stored.refresh_token.clone()),
        client_id: stored.client_id.clone(),
        token_endpoint: stored.token_endpoint.clone(),
        resource: stored.resource.clone(),
    })
}

/// What a request to the loopback listener means.
#[derive(Debug, PartialEq, Eq)]
enum Callback {
    /// Not the callback (e.g. `/favicon.ico`).
    Ignore,
    Code(String),
    Failed(String),
}

fn parse_callback(target: &str, expected_state: &str) -> Callback {
    let Ok(url) = Url::parse(&format!("http://127.0.0.1{target}")) else {
        return Callback::Ignore;
    };
    if url.path() != CALLBACK_PATH {
        return Callback::Ignore;
    }
    let (mut code, mut state, mut error, mut description) = (None, None, None, None);
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            "error_description" => description = Some(v.into_owned()),
            _ => {}
        }
    }
    if let Some(error) = error {
        return Callback::Failed(match description {
            Some(d) => format!("authorization failed: {error} ({d})"),
            None => format!("authorization failed: {error}"),
        });
    }
    if state.as_deref() != Some(expected_state) {
        return Callback::Failed("state mismatch; please start the login again".to_owned());
    }
    match code {
        Some(c) if !c.is_empty() => Callback::Code(c),
        _ => Callback::Failed("the callback did not include a code".to_owned()),
    }
}

/// Serves the loopback redirect until the real callback arrives.
async fn wait_for_callback(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let target = match read_request_target(&mut stream).await {
            Ok(t) => t,
            Err(e) => {
                tracing::debug!(error = %e, "ignoring unreadable callback request");
                continue;
            }
        };
        match parse_callback(&target, state) {
            Callback::Ignore => respond(&mut stream, "404 Not Found", "Not found").await,
            Callback::Code(code) => {
                respond(
                    &mut stream,
                    "200 OK",
                    "<h1>Connected to INDmoney</h1><p>You can close this tab and return to the terminal.</p>",
                )
                .await;
                return Ok(code);
            }
            Callback::Failed(message) => {
                let body = format!("<h1>Sign-in failed</h1><p>{}</p>", html_escape(&message));
                respond(&mut stream, "400 Bad Request", &body).await;
                return Err(upstream(message));
            }
        }
    }
}

/// Reads an HTTP request head and returns the request target.
async fn read_request_target(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 16 * 1024 {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&buf);
    Ok(head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_owned())
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let page = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>swing-trade</title></head>\
         <body style=\"font-family:system-ui,sans-serif;margin:3rem\">{body}</body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    if let Err(e) = stream.write_all(response.as_bytes()).await {
        tracing::debug!(error = %e, "could not answer callback request");
    }
    let _ = stream.shutdown().await;
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Value of an auth-param (e.g. `resource_metadata`) in a `WWW-Authenticate` header.
fn auth_param(header: &str, name: &str) -> Option<String> {
    let lower = header.to_ascii_lowercase();
    let needle = format!("{}=", name.to_ascii_lowercase());
    let mut from = 0;
    while let Some(pos) = lower[from..].find(&needle) {
        let start = from + pos;
        let at_boundary = start == 0 || matches!(lower.as_bytes()[start - 1], b' ' | b',' | b'\t');
        let value_start = start + needle.len();
        if at_boundary {
            let rest = &header[value_start..];
            let value = match rest.strip_prefix('"') {
                Some(quoted) => quoted.split('"').next().unwrap_or_default(),
                None => rest
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .next()
                    .unwrap_or_default(),
            };
            return (!value.is_empty()).then(|| value.to_owned());
        }
        from = value_start;
    }
    None
}

/// GETs JSON, returning `None` on any failure (discovery tries alternatives).
async fn get_json<T: DeserializeOwned>(http: &reqwest::Client, url: &str) -> Option<T> {
    let resp = match http
        .get(url)
        .header(ACCEPT, "application/json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(%url, error = %e, "metadata request failed");
            return None;
        }
    };
    if !resp.status().is_success() {
        tracing::debug!(%url, status = %resp.status(), "metadata not found");
        return None;
    }
    match resp.json::<T>().await {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::debug!(%url, error = %e, "metadata did not parse");
            None
        }
    }
}

/// 64-character PKCE code verifier from the unreserved alphabet.
fn pkce_verifier() -> String {
    let mut rng = rand::thread_rng();
    (0..64)
        .map(|_| char::from(PKCE_CHARS[rng.gen_range(0..PKCE_CHARS.len())]))
        .collect()
}

/// S256 code challenge for a verifier.
fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random_state() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill(&mut bytes[..]);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Best-effort attempt to open `url` in the default browser.
fn open_browser(url: &str) -> bool {
    match browser_command(url) {
        Some(mut cmd) => cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok(),
        None => false,
    }
}

#[cfg(target_os = "macos")]
fn browser_command(url: &str) -> Option<Command> {
    let mut cmd = Command::new("open");
    cmd.arg(url);
    Some(cmd)
}

#[cfg(windows)]
fn browser_command(url: &str) -> Option<Command> {
    let mut cmd = Command::new("cmd");
    // `start` treats the first quoted argument as a title; `^&` escapes `&` for cmd.
    cmd.args(["/C", "start", ""]).arg(url.replace('&', "^&"));
    Some(cmd)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn browser_command(url: &str) -> Option<Command> {
    let mut cmd = Command::new("xdg-open");
    cmd.arg(url);
    Some(cmd)
}

#[cfg(not(any(unix, windows)))]
fn browser_command(_url: &str) -> Option<Command> {
    None
}

fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("swing-trade/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

fn upstream(message: impl Into<String>) -> Error {
    Error::Upstream {
        provider: ID,
        message: message.into(),
    }
}

fn decode(message: impl Into<String>) -> Error {
    Error::Decode {
        provider: ID,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let v = pkce_verifier();
        assert_eq!(v.len(), 64);
        assert!(v.bytes().all(|b| PKCE_CHARS.contains(&b)));
        assert_ne!(v, pkce_verifier());
        assert_eq!(random_state().len(), 32);
    }

    #[test]
    fn parses_callback_requests() {
        assert_eq!(parse_callback("/favicon.ico", "s"), Callback::Ignore);
        assert_eq!(
            parse_callback("/callback?code=abc%2F1&state=s", "s"),
            Callback::Code("abc/1".to_owned())
        );
        assert!(matches!(
            parse_callback("/callback?code=abc&state=evil", "s"),
            Callback::Failed(m) if m.contains("state mismatch")
        ));
        assert!(matches!(
            parse_callback("/callback?error=access_denied&error_description=User+cancelled&state=s", "s"),
            Callback::Failed(m) if m.contains("access_denied (User cancelled)")
        ));
        assert!(matches!(
            parse_callback("/callback?state=s", "s"),
            Callback::Failed(_)
        ));
    }

    #[test]
    fn reads_www_authenticate_params() {
        let h = r#"Bearer realm="mcp", resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource""#;
        assert_eq!(
            auth_param(h, "resource_metadata").as_deref(),
            Some("https://mcp.example.com/.well-known/oauth-protected-resource")
        );
        assert_eq!(
            auth_param(
                "Bearer resource_metadata=https://x/y, error=\"invalid_token\"",
                "resource_metadata"
            )
            .as_deref(),
            Some("https://x/y")
        );
        assert_eq!(
            auth_param("Bearer xresource_metadata=\"a\"", "resource_metadata"),
            None
        );
        assert_eq!(auth_param("Bearer realm=\"x\"", "resource_metadata"), None);
    }

    #[test]
    fn builds_metadata_urls() {
        assert_eq!(
            metadata_urls("https://auth.example.com").unwrap(),
            vec![
                "https://auth.example.com/.well-known/oauth-authorization-server",
                "https://auth.example.com/.well-known/openid-configuration",
            ]
        );
        assert_eq!(
            metadata_urls("https://example.com/tenant/").unwrap(),
            vec![
                "https://example.com/.well-known/oauth-authorization-server/tenant",
                "https://example.com/.well-known/openid-configuration/tenant",
                "https://example.com/tenant/.well-known/openid-configuration",
            ]
        );
    }

    #[test]
    fn builds_authorization_url() {
        let url = authorization_url(
            "https://auth.example.com/authorize",
            "cid",
            "http://127.0.0.1:8765/callback",
            "chal",
            "st",
            "https://mcp.indmoney.com/mcp",
            &["read".to_owned(), "offline_access".to_owned()],
        )
        .unwrap();
        let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let get = |k: &str| pairs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("response_type"), Some("code"));
        assert_eq!(get("code_challenge_method"), Some("S256"));
        assert_eq!(get("redirect_uri"), Some("http://127.0.0.1:8765/callback"));
        assert_eq!(get("resource"), Some("https://mcp.indmoney.com/mcp"));
        assert_eq!(get("scope"), Some("read offline_access"));
    }

    #[test]
    fn token_expiry() {
        let t = TokenResponse {
            access_token: "a".into(),
            refresh_token: None,
            expires_in: Some(json!(3600)),
        };
        assert_eq!(t.expires_at(1_000), Some(4_600));
        let s = TokenResponse {
            expires_in: Some(json!("60")),
            ..t
        };
        assert_eq!(s.expires_at(0), Some(60));

        let stored = StoredToken {
            client_id: "c".into(),
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: Some(1_000),
            token_endpoint: "https://x/token".into(),
            resource: None,
        };
        assert!(stored.expires_soon(1_000 - REFRESH_MARGIN_SECS));
        assert!(!stored.expires_soon(1_000 - REFRESH_MARGIN_SECS - 1));
        assert!(!format!("{stored:?}").contains("\"a\""));
    }

    #[test]
    fn saves_and_loads_token_file() {
        let dir = std::env::temp_dir().join(format!("swing-token-test-{}", std::process::id()));
        let path = dir.join("nested").join("token.json");
        let stored = StoredToken {
            client_id: "c".into(),
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: Some(42),
            token_endpoint: "https://x/token".into(),
            resource: Some("https://mcp".into()),
        };
        stored.save(&path).unwrap();
        let loaded = StoredToken::load(&path).unwrap();
        assert_eq!(loaded.access_token, "a");
        assert_eq!(loaded.refresh_token.as_deref(), Some("r"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(matches!(
            StoredToken::load(&dir.join("missing.json")),
            Err(Error::Unauthorized { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
