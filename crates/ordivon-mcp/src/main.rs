use std::fs::OpenOptions;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use ordivon_exec::{RegistryConfig, Runtime, RuntimeConfig, UniversalExecutorConfig};
use ordivon_mcp::server::{ExecutionContext, OrdivonServer, ServerConfig};
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
    trust_cf_access: bool,
}

static HTTP_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static HTTP_TRACE_LOCK: Mutex<()> = Mutex::new(());

struct AppConfig {
    bind: SocketAddr,
    token: String,
    body_limit_bytes: usize,
    trust_cf_access: bool,
    server: ServerConfig,
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
    validate_loopback_bind(app.bind)?;
    let startup_runtime = Runtime::new(app.server.runtime.clone())?;
    let startup_reconciliation = startup_runtime.reconcile_all()?;
    if startup_reconciliation.failed > 0 {
        tracing::warn!(
            inspected = startup_reconciliation.inspected,
            reconciled = startup_reconciliation.reconciled,
            recovered_orphans = startup_reconciliation.recovered_orphans,
            quarantined = startup_reconciliation.quarantined,
            unchanged = startup_reconciliation.unchanged,
            failed = startup_reconciliation.failed,
            "runtime startup reconciliation isolated Job-level failures"
        );
        for failure in &startup_reconciliation.failures {
            tracing::warn!(
                job_id = %failure.job_id,
                attempt_id = %failure.attempt_id,
                code = failure.code.as_str(),
                message = %failure.message,
                "runtime startup reconciliation requires targeted recovery"
            );
        }
    } else {
        tracing::info!(
            inspected = startup_reconciliation.inspected,
            reconciled = startup_reconciliation.reconciled,
            recovered_orphans = startup_reconciliation.recovered_orphans,
            quarantined = startup_reconciliation.quarantined,
            unchanged = startup_reconciliation.unchanged,
            "runtime startup reconciliation completed"
        );
    }
    drop(startup_runtime);
    let listener = tokio::net::TcpListener::bind(app.bind).await?;
    let address = listener.local_addr()?;
    let cancellation = CancellationToken::new();
    let server_config = app.server.clone();
    let service: StreamableHttpService<OrdivonServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || {
                OrdivonServer::new(server_config.clone())
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
        trust_cf_access: app.trust_cf_access,
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
        execution_mode = "trusted-local",
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
        "ordivon-http-{}-{}-{}",
        std::process::id(),
        unix_ms(),
        HTTP_TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let request_bytes = content_length(request.headers());
    let authorized = request_is_authorized(request.headers(), &state);
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
fn request_is_authorized(headers: &HeaderMap, state: &HttpState) -> bool {
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    constant_time_eq(supplied.as_bytes(), state.bearer.as_bytes())
        || (state.trust_cf_access
            && headers
                .get("cf-access-jwt-assertion")
                .is_some_and(|value| !value.as_bytes().is_empty()))
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

fn validate_loopback_bind(bind: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    if bind.ip().is_loopback() {
        Ok(())
    } else {
        Err("ORDIVON_BIND must use a loopback address".into())
    }
}

fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let bind: SocketAddr = std::env::var("ORDIVON_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8897".to_string())
        .parse()?;
    let token = required_env("ORDIVON_BEARER_TOKEN")?;
    if token.len() < 32 {
        return Err("ORDIVON_BEARER_TOKEN must be at least 32 characters".into());
    }
    let store_root = PathBuf::from(required_env("ORDIVON_STORE_ROOT")?);
    let registry_root = PathBuf::from(required_env("ORDIVON_REGISTRY_ROOT")?);
    let runner_path = PathBuf::from(required_env("ORDIVON_RUNNER_PATH")?);
    let allowed_executable_roots = std::env::var("ORDIVON_ALLOWED_EXECUTABLE_ROOTS")
        .ok()
        .map(|roots| {
            roots
                .split(':')
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .filter(|roots| !roots.is_empty())
        .unwrap_or_else(|| vec![PathBuf::from("/")]);
    let trace_path = std::env::var("ORDIVON_TRACE_PATH")
        .ok()
        .map(PathBuf::from)
        .or_else(|| Some(registry_root.join("runtime-trace.jsonl")));
    let body_limit_bytes = std::env::var("ORDIVON_BODY_LIMIT_BYTES")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1_048_576);
    let trust_cf_access = match std::env::var("ORDIVON_TRUST_CF_ACCESS") {
        Ok(value) if matches!(value.as_str(), "1" | "true" | "yes") => true,
        Ok(value) if matches!(value.as_str(), "0" | "false" | "no") => false,
        Ok(_) => return Err("ORDIVON_TRUST_CF_ACCESS must be true or false".into()),
        Err(std::env::VarError::NotPresent) => false,
        Err(error) => return Err(Box::new(error)),
    };
    let busy_timeout_ms = std::env::var("ORDIVON_BUSY_TIMEOUT_MS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(5_000);
    let startup_grace_ms = std::env::var("ORDIVON_STARTUP_GRACE_MS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(2_000);

    let global_limit = std::env::var("ORDIVON_GLOBAL_MAX_CONCURRENCY")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(4);
    if global_limit == 0 {
        return Err("ORDIVON_GLOBAL_MAX_CONCURRENCY must be positive".into());
    }
    let principal =
        std::env::var("ORDIVON_PRINCIPAL").unwrap_or_else(|_| "principal:local-owner".to_string());
    let (workspace_root, workspace_uid, workspace_gid) = (None, None, None);

    Ok(AppConfig {
        bind,
        token,
        body_limit_bytes,
        trust_cf_access,
        server: ServerConfig {
            runtime: RuntimeConfig {
                registry: RegistryConfig {
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
            },
            execution: ExecutionContext {
                principal,
                global_limit,
            },
            trace_path,
        },
    })
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
            tracing::warn!("HTTP trace lock poisoned: {error}");
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
        tracing::warn!("cannot append HTTP trace {}: {error}", path.display());
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
    use super::{request_is_authorized, validate_loopback_bind, HttpState};
    use axum::http::{header, HeaderMap, HeaderValue};
    use std::sync::Arc;

    fn auth_state(trust_cf_access: bool) -> HttpState {
        HttpState {
            bearer: Arc::from("Bearer local-secret-token-value-1234567890"),
            trace_path: None,
            body_limit_bytes: 1024,
            trust_cf_access,
        }
    }

    #[test]
    fn only_loopback_bindings_are_accepted() {
        assert!(validate_loopback_bind("127.0.0.1:8897".parse().unwrap()).is_ok());
        assert!(validate_loopback_bind("[::1]:8897".parse().unwrap()).is_ok());
        assert!(validate_loopback_bind("0.0.0.0:8897".parse().unwrap()).is_err());
    }

    #[test]
    fn bearer_and_explicit_cloudflare_access_are_valid_authentication_paths() {
        let mut bearer = HeaderMap::new();
        bearer.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer local-secret-token-value-1234567890"),
        );
        assert!(request_is_authorized(&bearer, &auth_state(false)));

        let mut access = HeaderMap::new();
        access.insert(
            "cf-access-jwt-assertion",
            HeaderValue::from_static("signed-access-assertion"),
        );
        assert!(request_is_authorized(&access, &auth_state(true)));
        assert!(!request_is_authorized(&access, &auth_state(false)));
    }
}
