use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use ordivon_exec::UniversalExecutorConfig;
use ordivon_mcp::m4::{M4Server, M4ServerConfig};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
struct AuthState {
    bearer: Arc<str>,
}

struct AppConfig {
    bind: SocketAddr,
    token: String,
    allowed_origins: Vec<String>,
    body_limit_bytes: usize,
    server: M4ServerConfig,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ordivon_mcp=info,rmcp=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init();

    let app = load_config()?;
    if !app.bind.ip().is_loopback() {
        return Err("ORDIVON_M4_BIND must use a loopback address".into());
    }
    let listener = tokio::net::TcpListener::bind(app.bind).await?;
    let address = listener.local_addr()?;
    let cancellation = CancellationToken::new();
    let server_config = app.server.clone();
    let service: StreamableHttpService<M4Server, LocalSessionManager> = StreamableHttpService::new(
        move || {
            M4Server::new(server_config.clone())
                .map_err(|error| std::io::Error::other(error.message))
        },
        Default::default(),
        transport_config(address, &app.allowed_origins, cancellation.child_token()),
    );

    let auth = AuthState {
        bearer: Arc::from(format!("Bearer {}", app.token)),
    };
    let router = Router::new()
        .nest_service("/mcp", service)
        .layer(DefaultBodyLimit::max(app.body_limit_bytes))
        .layer(middleware::from_fn_with_state(auth, authenticate));

    tracing::info!(
        bind = %address,
        endpoint = %format!("http://{address}/mcp"),
        "Ordivon M4 experimental MCP listening"
    );

    let shutdown = cancellation.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        shutdown.cancel();
    });

    axum::serve(listener, router)
        .with_graceful_shutdown(cancellation.cancelled_owned())
        .await?;
    Ok(())
}

async fn authenticate(
    State(state): State<AuthState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let supplied = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !constant_time_eq(supplied.as_bytes(), state.bearer.as_bytes()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

fn transport_config(
    address: SocketAddr,
    origins: &[String],
    cancellation: CancellationToken,
) -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        .with_stateful_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .with_allowed_hosts([
            format!("127.0.0.1:{}", address.port()),
            format!("localhost:{}", address.port()),
        ])
        .with_allowed_origins(origins.iter().cloned())
        .with_cancellation_token(cancellation)
}

fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let bind: SocketAddr = std::env::var("ORDIVON_M4_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8894".to_string())
        .parse()?;
    let token = required_env("ORDIVON_M4_BEARER_TOKEN")?;
    if token.len() < 32 {
        return Err("ORDIVON_M4_BEARER_TOKEN must be at least 32 characters".into());
    }
    let store_root = PathBuf::from(required_env("ORDIVON_M4_STORE_ROOT")?);
    let runner_path = PathBuf::from(required_env("ORDIVON_M4_RUNNER_PATH")?);
    let roots = required_env("ORDIVON_M4_ALLOWED_EXECUTABLE_ROOTS")?;
    let allowed_executable_roots = roots
        .split(':')
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect();
    let trace_path = std::env::var("ORDIVON_M4_TRACE_PATH")
        .ok()
        .map(PathBuf::from)
        .or_else(|| Some(store_root.join("m4-trace.jsonl")));
    let allowed_origins = std::env::var("ORDIVON_M4_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost,http://127.0.0.1".to_string())
        .split(',')
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect();
    let body_limit_bytes = std::env::var("ORDIVON_M4_BODY_LIMIT_BYTES")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1_048_576);

    Ok(AppConfig {
        bind,
        token,
        allowed_origins,
        body_limit_bytes,
        server: M4ServerConfig {
            executor: UniversalExecutorConfig {
                store_root,
                runner_path,
                allowed_executable_roots,
                max_runtime_ms: 900_000,
                max_output_bytes: 16 * 1024 * 1024,
            },
            trace_path,
        },
    })
}

fn required_env(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name).map_err(|error| format!("{name} is required: {error}").into())
}
