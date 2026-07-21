use std::fs::OpenOptions;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use ordivon_exec::UniversalExecutorConfig;
use ordivon_mcp::m4::{M4Server, M4ServerConfig, M5DogfoodPolicy};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;
use tower_http::limit::RequestBodyLimitLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
struct HttpState {
    bearer: Arc<str>,
    trace_path: Option<Arc<PathBuf>>,
    body_limit_bytes: u64,
}

static HTTP_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static HTTP_TRACE_LOCK: Mutex<()> = Mutex::new(());

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

    let http_state = HttpState {
        bearer: Arc::from(format!("Bearer {}", app.token)),
        trace_path: app
            .server
            .trace_path
            .as_ref()
            .map(|path| Arc::new(PathBuf::from(format!("{}.http.jsonl", path.display())))),
        body_limit_bytes: app.body_limit_bytes.try_into()?,
    };
    let router = Router::new()
        .nest_service("/mcp", service)
        .layer(RequestBodyLimitLayer::new(app.body_limit_bytes))
        .layer(middleware::from_fn_with_state(
            http_state,
            authenticate_and_trace,
        ));

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

async fn authenticate_and_trace(
    State(state): State<HttpState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let started = Instant::now();
    let trace_id = format!(
        "m4-http-{}-{}-{}",
        std::process::id(),
        unix_ms(),
        HTTP_TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let request_bytes = content_length(request.headers());
    let supplied = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let authorized = constant_time_eq(supplied.as_bytes(), state.bearer.as_bytes());
    let too_large = request_bytes.is_some_and(|bytes| bytes > state.body_limit_bytes);
    let mut response = if too_large {
        StatusCode::PAYLOAD_TOO_LARGE.into_response()
    } else if authorized {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    };
    if let Ok(value) = HeaderValue::from_str(&trace_id) {
        response.headers_mut().insert("x-ordivon-trace-id", value);
    }
    let response_bytes = content_length(response.headers());
    append_http_trace(
        state.trace_path.as_deref(),
        &trace_id,
        &method,
        &path,
        authorized,
        response.status().as_u16(),
        request_bytes,
        response_bytes,
        started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
    );
    response
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
            dogfood_policy: load_dogfood_policy()?,
        },
    })
}

fn load_dogfood_policy() -> Result<Option<M5DogfoodPolicy>, Box<dyn std::error::Error>> {
    let repos = std::env::var("ORDIVON_M5_ALLOWED_SOURCE_REPOS").ok();
    let revisions = std::env::var("ORDIVON_M5_ALLOWED_SOURCE_REVISIONS").ok();
    match (repos, revisions) {
        (None, None) => Ok(None),
        (Some(repos), Some(revisions)) => Ok(Some(M5DogfoodPolicy {
            allowed_source_repos: repos
                .split(':')
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .collect(),
            allowed_source_revisions: revisions
                .split(',')
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect(),
        })),
        _ => Err(
            "ORDIVON_M5_ALLOWED_SOURCE_REPOS and ORDIVON_M5_ALLOWED_SOURCE_REVISIONS must be set together"
                .into(),
        ),
    }
}

fn required_env(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name).map_err(|error| format!("{name} is required: {error}").into())
}

fn content_length(headers: &axum::http::HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

#[allow(clippy::too_many_arguments)]
fn append_http_trace(
    path: Option<&PathBuf>,
    trace_id: &str,
    method: &str,
    request_path: &str,
    authorized: bool,
    status: u16,
    request_bytes: Option<u64>,
    response_bytes: Option<u64>,
    total_ms: u64,
) {
    let Some(path) = path else {
        return;
    };
    let record = serde_json::json!({
        "traceId": trace_id,
        "kind": "http",
        "method": method,
        "path": request_path,
        "authorized": authorized,
        "status": status,
        "requestBytes": request_bytes,
        "responseBytes": response_bytes,
        "totalMs": total_ms,
        "observedUnixMs": unix_ms(),
    });
    let guard = match HTTP_TRACE_LOCK.lock() {
        Ok(guard) => guard,
        Err(error) => {
            tracing::warn!("M4 HTTP trace lock poisoned: {error}");
            return;
        }
    };
    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            serde_json::to_writer(&mut file, &record)?;
            file.write_all(b"\n")
        });
    drop(guard);
    if let Err(error) = result {
        tracing::warn!("cannot append M4 HTTP trace {}: {error}", path.display());
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
