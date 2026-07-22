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
use ordivon_exec::{
    M6RegistryConfig, M6Runtime, M6RuntimeConfig, M7RuntimeHardeningConfig, M7WorkerIdentity,
    UniversalExecutorConfig,
};
use ordivon_mcp::m4::M5DogfoodPolicy;
use ordivon_mcp::m6::{M6Server, M6ServerConfig};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExecutionMode {
    TrustedLocal,
    Isolated,
}

impl ExecutionMode {
    fn parse(value: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match value {
            "trusted-local" => Ok(Self::TrustedLocal),
            "isolated" => Ok(Self::Isolated),
            _ => Err("ORDIVON_M7_EXECUTION_MODE must be trusted-local or isolated".into()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::TrustedLocal => "trusted-local",
            Self::Isolated => "isolated",
        }
    }
}

struct AppConfig {
    bind: SocketAddr,
    token: String,
    execution_mode: ExecutionMode,
    body_limit_bytes: usize,
    server: M6ServerConfig,
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
    let startup_runtime = M6Runtime::new(app.server.runtime.clone())?;
    startup_runtime.reconcile_all()?;
    drop(startup_runtime);
    if !app.bind.ip().is_loopback() {
        return Err("ORDIVON_M7_BIND must use a loopback address".into());
    }
    let listener = tokio::net::TcpListener::bind(app.bind).await?;
    let address = listener.local_addr()?;
    let cancellation = CancellationToken::new();
    let server_config = app.server.clone();
    let service: StreamableHttpService<M6Server, LocalSessionManager> = StreamableHttpService::new(
        move || {
            M6Server::new(server_config.clone())
                .map_err(|error| std::io::Error::other(error.message))
        },
        Default::default(),
        transport_config(cancellation.child_token()),
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
        execution_mode = app.execution_mode.as_str(),
        "Ordivon MCP listening"
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
        "m6-http-{}-{}-{}",
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

fn transport_config(cancellation: CancellationToken) -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        .with_stateful_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .disable_allowed_hosts()
        .disable_allowed_origins()
        .with_cancellation_token(cancellation)
}

fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let bind: SocketAddr = std::env::var("ORDIVON_M7_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8897".to_string())
        .parse()?;
    let token = required_env("ORDIVON_M7_BEARER_TOKEN")?;
    if token.len() < 32 {
        return Err("ORDIVON_M7_BEARER_TOKEN must be at least 32 characters".into());
    }
    let execution_mode = ExecutionMode::parse(
        &std::env::var("ORDIVON_M7_EXECUTION_MODE").unwrap_or_else(|_| "trusted-local".to_string()),
    )?;
    let store_root = PathBuf::from(required_env("ORDIVON_M7_STORE_ROOT")?);
    let registry_root = PathBuf::from(required_env("ORDIVON_M7_REGISTRY_ROOT")?);
    let runner_path = PathBuf::from(required_env("ORDIVON_M7_RUNNER_PATH")?);
    let roots = required_env("ORDIVON_M7_ALLOWED_EXECUTABLE_ROOTS")?;
    let allowed_executable_roots = roots
        .split(':')
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect();
    let trace_path = std::env::var("ORDIVON_M7_TRACE_PATH")
        .ok()
        .map(PathBuf::from)
        .or_else(|| Some(registry_root.join("runtime-trace.jsonl")));
    let body_limit_bytes = std::env::var("ORDIVON_M7_BODY_LIMIT_BYTES")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1_048_576);
    let busy_timeout_ms = std::env::var("ORDIVON_M7_BUSY_TIMEOUT_MS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(5_000);
    let startup_grace_ms = std::env::var("ORDIVON_M7_STARTUP_GRACE_MS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(2_000);

    let (workspace_root, workspace_uid, workspace_gid, hardening) = match execution_mode {
        ExecutionMode::TrustedLocal => (None, None, None, None),
        ExecutionMode::Isolated => {
            let control_root = PathBuf::from(required_env("ORDIVON_M7_CONTROL_ROOT")?);
            let worker_root = PathBuf::from(required_env("ORDIVON_M7_WORKER_ROOT")?);
            let cache_root = PathBuf::from(required_env("ORDIVON_M7_CACHE_ROOT")?);
            let runtime_view_root = PathBuf::from(required_env("ORDIVON_M7_RUNTIME_VIEW_ROOT")?);
            let worker_uid: u32 = required_env("ORDIVON_M7_WORKER_UID")?.parse()?;
            let worker_gid: u32 = required_env("ORDIVON_M7_WORKER_GID")?.parse()?;
            let hardening = M7RuntimeHardeningConfig {
                worker: M7WorkerIdentity {
                    user: "ordivon-worker".to_string(),
                    group: "ordivon-worker".to_string(),
                    uid: worker_uid,
                    gid: worker_gid,
                },
                control_root,
                worker_root,
                cache_root,
                runtime_view_root,
            };
            (
                Some(hardening.workspaces_root()),
                Some(worker_uid),
                Some(worker_gid),
                Some(hardening),
            )
        }
    };

    Ok(AppConfig {
        bind,
        token,
        execution_mode,
        body_limit_bytes,
        server: M6ServerConfig {
            runtime: M6RuntimeConfig {
                registry: M6RegistryConfig {
                    db_path: registry_root.join("registry.sqlite3"),
                    store_root: registry_root,
                    busy_timeout_ms,
                },
                executor: UniversalExecutorConfig {
                    store_root,
                    workspace_root,
                    workspace_uid,
                    workspace_gid,
                    runner_path,
                    allowed_executable_roots,
                    max_runtime_ms: 900_000,
                    max_output_bytes: 16 * 1024 * 1024,
                },
                startup_grace_ms,
                hardening,
            },
            trace_path,
            dogfood_policy: load_dogfood_policy()?,
        },
    })
}

fn load_dogfood_policy() -> Result<Option<M5DogfoodPolicy>, Box<dyn std::error::Error>> {
    let repos = std::env::var("ORDIVON_M7_ALLOWED_SOURCE_REPOS").ok();
    let revisions = std::env::var("ORDIVON_M7_ALLOWED_SOURCE_REVISIONS").ok();
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
            "ORDIVON_M7_ALLOWED_SOURCE_REPOS and ORDIVON_M7_ALLOWED_SOURCE_REVISIONS must be set together"
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
            tracing::warn!("M6 HTTP trace lock poisoned: {error}");
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
        tracing::warn!("cannot append M6 HTTP trace {}: {error}", path.display());
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::ExecutionMode;

    #[test]
    fn trusted_local_is_the_explicit_low_friction_mode() {
        assert_eq!(
            ExecutionMode::parse("trusted-local").unwrap(),
            ExecutionMode::TrustedLocal
        );
    }

    #[test]
    fn isolated_mode_remains_available() {
        assert_eq!(
            ExecutionMode::parse("isolated").unwrap(),
            ExecutionMode::Isolated
        );
    }

    #[test]
    fn unknown_execution_mode_is_rejected() {
        assert!(ExecutionMode::parse("sandbox-everything").is_err());
    }
}
