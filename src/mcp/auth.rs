use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, Request, Response, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Form, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::mcp::FsPulseMcp;
use rmcp::transport::streamable_http_server::{
    StreamableHttpService,
    session::local::LocalSessionManager,
};

// ─── Constants ─────────────────────────────────────────────────────

/// Client credentials for Claude Desktop custom connector.
/// Users paste these into the Advanced Settings when adding the connector.
pub const CLIENT_ID: &str = "fspulse";
pub const CLIENT_SECRET: &str = "fspulse";

const CODE_EXPIRY: Duration = Duration::from_secs(60);
const TOKEN_EXPIRY: Duration = Duration::from_secs(86400);

// ─── OAuth state ───────────────────────────────────────────────────

struct AuthCode {
    code_challenge: String,
    redirect_uri: String,
    created_at: Instant,
}

pub struct OAuthState {
    auth_codes: Mutex<HashMap<String, AuthCode>>,
    tokens: Mutex<HashMap<String, Instant>>,
}

impl OAuthState {
    pub fn new() -> Self {
        Self {
            auth_codes: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
        }
    }

    fn store_code(&self, code: String, code_challenge: String, redirect_uri: String) {
        let mut codes = self.auth_codes.lock().unwrap();
        // Lazy cleanup of expired codes
        codes.retain(|_, v| v.created_at.elapsed() < CODE_EXPIRY);
        codes.insert(code, AuthCode { code_challenge, redirect_uri, created_at: Instant::now() });
    }

    fn exchange_code(&self, code: &str, redirect_uri: &str, code_verifier: &str) -> Result<String, &'static str> {
        let mut codes = self.auth_codes.lock().unwrap();
        let auth_code = codes.remove(code).ok_or("invalid_grant")?;

        if auth_code.created_at.elapsed() >= CODE_EXPIRY {
            return Err("invalid_grant");
        }
        if auth_code.redirect_uri != redirect_uri {
            return Err("invalid_grant");
        }
        if !verify_pkce(code_verifier, &auth_code.code_challenge) {
            return Err("invalid_grant");
        }

        let token = generate_random_hex(32);
        let mut tokens = self.tokens.lock().unwrap();
        // Lazy cleanup of expired tokens
        tokens.retain(|_, created| created.elapsed() < TOKEN_EXPIRY);
        tokens.insert(token.clone(), Instant::now());
        Ok(token)
    }

    fn validate_token(&self, token: &str) -> bool {
        let tokens = self.tokens.lock().unwrap();
        tokens.get(token).map_or(false, |created| created.elapsed() < TOKEN_EXPIRY)
    }
}

// ─── Shared handler state ──────────────────────────────────────────

type McpService = StreamableHttpService<FsPulseMcp, LocalSessionManager>;

#[derive(Clone)]
pub struct McpAuthState {
    oauth: std::sync::Arc<OAuthState>,
    mcp_service: McpService,
    base_url: String,
}

// ─── Router constructor ────────────────────────────────────────────

pub fn mcp_router(
    mcp_service: McpService,
    oauth_state: std::sync::Arc<OAuthState>,
    base_url: String,
) -> Router {
    let state = McpAuthState {
        oauth: oauth_state,
        mcp_service,
        base_url,
    };

    Router::new()
        // Well-known endpoints (path-scoped per MCP spec)
        .route("/mcp/.well-known/oauth-protected-resource", get(protected_resource_metadata))
        .route("/mcp/.well-known/oauth-authorization-server", get(authorization_server_metadata))
        // Root-level fallbacks
        .route("/.well-known/oauth-protected-resource", get(protected_resource_metadata))
        .route("/.well-known/oauth-authorization-server", get(authorization_server_metadata))
        // OAuth endpoints
        .route("/mcp/authorize", get(authorize))
        .route("/mcp/token", post(token_exchange))
        // MCP transport (auth-gated)
        .route("/mcp", post(mcp_handler).get(mcp_handler).delete(mcp_handler))
        .with_state(state)
}

// ─── Well-known metadata endpoints ─────────────────────────────────

async fn protected_resource_metadata(
    State(state): State<McpAuthState>,
) -> impl IntoResponse {
    let body = serde_json::json!({
        "resource": format!("{}/mcp", state.base_url),
        "authorization_servers": [format!("{}/mcp", state.base_url)],
        "bearer_methods_supported": ["header"]
    });
    (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], body.to_string())
}

async fn authorization_server_metadata(
    State(state): State<McpAuthState>,
) -> impl IntoResponse {
    let body = serde_json::json!({
        "issuer": format!("{}/mcp", state.base_url),
        "authorization_endpoint": format!("{}/mcp/authorize", state.base_url),
        "token_endpoint": format!("{}/mcp/token", state.base_url),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["client_secret_post"]
    });
    (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], body.to_string())
}

// ─── Authorization endpoint ────────────────────────────────────────

#[derive(Deserialize)]
struct AuthorizeParams {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
}

async fn authorize(
    State(state): State<McpAuthState>,
    Query(params): Query<AuthorizeParams>,
) -> impl IntoResponse {
    if params.response_type != "code" {
        return (StatusCode::BAD_REQUEST, "unsupported_response_type").into_response();
    }
    if params.client_id != CLIENT_ID {
        return (StatusCode::BAD_REQUEST, "invalid_client").into_response();
    }

    let code_challenge = match (&params.code_challenge, &params.code_challenge_method) {
        (Some(challenge), Some(method)) if method == "S256" => challenge.clone(),
        (Some(_), Some(_)) => return (StatusCode::BAD_REQUEST, "unsupported code_challenge_method").into_response(),
        (Some(challenge), None) => challenge.clone(),
        (None, _) => String::new(),
    };

    let code = generate_random_hex(16);
    state.oauth.store_code(code.clone(), code_challenge, params.redirect_uri.clone());

    let mut redirect_url = format!("{}?code={}", params.redirect_uri, code);
    if let Some(st) = &params.state {
        redirect_url.push_str(&format!("&state={}", st));
    }

    (StatusCode::FOUND, [(header::LOCATION, redirect_url)]).into_response()
}

// ─── Token endpoint ────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: String,
    redirect_uri: String,
    client_id: String,
    client_secret: Option<String>,
    code_verifier: Option<String>,
}

async fn token_exchange(
    State(state): State<McpAuthState>,
    Form(params): Form<TokenRequest>,
) -> impl IntoResponse {
    if params.grant_type != "authorization_code" {
        return (StatusCode::BAD_REQUEST, oauth_error("unsupported_grant_type")).into_response();
    }
    if params.client_id != CLIENT_ID {
        return (StatusCode::BAD_REQUEST, oauth_error("invalid_client")).into_response();
    }
    if params.client_secret.as_deref() != Some(CLIENT_SECRET) {
        return (StatusCode::BAD_REQUEST, oauth_error("invalid_client")).into_response();
    }

    let code_verifier = params.code_verifier.as_deref().unwrap_or("");

    match state.oauth.exchange_code(&params.code, &params.redirect_uri, code_verifier) {
        Ok(token) => {
            let body = serde_json::json!({
                "access_token": token,
                "token_type": "Bearer",
                "expires_in": TOKEN_EXPIRY.as_secs()
            });
            (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], body.to_string()).into_response()
        }
        Err(err) => {
            (StatusCode::BAD_REQUEST, oauth_error(err)).into_response()
        }
    }
}

fn oauth_error(error: &str) -> String {
    serde_json::json!({"error": error}).to_string()
}

// ─── MCP transport handler (auth-gated) ────────────────────────────

async fn mcp_handler(
    State(state): State<McpAuthState>,
    request: Request<Body>,
) -> Response<Body> {
    // Auth strategy: if the request carries an Authorization header, validate it.
    // If no header is present, allow the request through unauthenticated.
    //
    // This supports two connection paths:
    // - Custom Connector (Claude Desktop): completes OAuth flow, sends Bearer token
    //   on every request per the MCP spec. Invalid/expired tokens get a 401 with
    //   WWW-Authenticate so the client can re-authenticate.
    // - Developer Config / Claude Code: connects via mcp-remote or native HTTP
    //   without OAuth. No Authorization header is sent, so requests pass through.
    if let Some(auth_header) = request.headers().get(header::AUTHORIZATION) {
        let valid = auth_header
            .to_str()
            .ok()
            .and_then(|v| v.strip_prefix("Bearer "))
            .map_or(false, |token| state.oauth.validate_token(token));

        if !valid {
            let www_auth = format!(
                r#"Bearer resource_metadata="{}/mcp/.well-known/oauth-protected-resource""#,
                state.base_url
            );
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, www_auth)],
                "Unauthorized",
            )
                .into_response()
                .into();
        }
    }

    // Forward to rmcp service
    let response = state.mcp_service.handle(request).await;
    let (parts, body) = response.into_parts();
    Response::from_parts(parts, Body::new(body))
}

// ─── Helpers ───────────────────────────────────────────────────────

fn verify_pkce(code_verifier: &str, code_challenge: &str) -> bool {
    if code_challenge.is_empty() {
        return true;
    }
    let hash = Sha256::digest(code_verifier.as_bytes());
    let computed = URL_SAFE_NO_PAD.encode(hash);
    computed == code_challenge
}

fn generate_random_hex(bytes: usize) -> String {
    let random_bytes: Vec<u8> = (0..bytes).map(|_| rand::rng().random()).collect();
    hex::encode(random_bytes)
}
