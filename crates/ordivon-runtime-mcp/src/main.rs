use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use ordivon_runtime_core::{
    InputAuthority, RegistryConfig, RuntimeConfig, UniversalExecutorConfig,
};
use ordivon_runtime_mcp::server::{ExecutionContext, RuntimeServer, ServerConfig};
use ordivon_runtime_mcp::{append_rotating_jsonl, DEFAULT_TRACE_ROTATION_BYTES};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use serde::Deserialize;
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InputAuthorityConfig {
    name: String,
    root: PathBuf,
}

struct AppConfig {
    bind: SocketAddr,
    token: String,
    body_limit_bytes: usize,
    trust_cf_access: bool,
    reconcile_interval_ms: u64,
    reconcile_batch_size: u32,
    server: ServerConfig,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ordivon_runtime_mcp=info,rmcp=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init();

    let app = load_config()?;
    validate_loopback_bind(app.bind)?;
    let runtime_server = RuntimeServer::new(app.server.clone())
        .map_err(|error| std::io::Error::other(error.message))?;
    let startup_runtime = runtime_server.runtime_handle();
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
    let background_runtime = startup_runtime.clone();
    let listener = tokio::net::TcpListener::bind(app.bind).await?;
    let address = listener.local_addr()?;
    let cancellation = CancellationToken::new();
    let service_server = runtime_server.clone();
    let service: StreamableHttpService<RuntimeServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(service_server.clone()),
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
        "Ordivon Runtime listening"
    );

    let reconcile_shutdown = cancellation.child_token();
    let reconcile_interval_ms = app.reconcile_interval_ms;
    let reconcile_batch_size = app.reconcile_batch_size;
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(reconcile_interval_ms));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = reconcile_shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    let runtime = background_runtime.clone();
                    match tokio::task::spawn_blocking(move || {
                        runtime.reconcile_maintenance_batch(reconcile_batch_size)
                    }).await {
                        Ok(Ok(report))
                            if report.reconciled > 0
                                || report.recovered_orphans > 0
                                || report.quarantined > 0
                                || report.failed > 0 =>
                        {
                            tracing::info!(
                                inspected = report.inspected,
                                reconciled = report.reconciled,
                                recovered_orphans = report.recovered_orphans,
                                quarantined = report.quarantined,
                                unchanged = report.unchanged,
                                failed = report.failed,
                                "runtime maintenance reconciliation completed"
                            );
                        }
                        Ok(Ok(_)) => {}
                        Ok(Err(error)) => {
                            tracing::warn!(
                                code = error.code.as_str(),
                                message = %error.message,
                                "runtime maintenance reconciliation failed"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(%error, "runtime maintenance reconciliation task failed");
                        }
                    }
                }
            }
        }
    });

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
        .with_legacy_session_mode(true)
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
    let token = load_bearer_token()?;
    if token.len() < 32 {
        return Err("Runtime Bearer token must be at least 32 characters".into());
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
    let input_authorities = optional_env("ORDIVON_INPUT_AUTHORITIES_JSON")?
        .map(|value| {
            serde_json::from_str::<Vec<InputAuthorityConfig>>(&value).map_err(|error| {
                format!(
                    "ORDIVON_INPUT_AUTHORITIES_JSON must be a JSON array of named roots: {error}"
                )
            })
        })
        .transpose()?
        .unwrap_or_default()
        .into_iter()
        .map(|authority| InputAuthority {
            name: authority.name,
            root: authority.root,
        })
        .collect::<Vec<_>>();
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
    let reconcile_interval_ms: u64 = std::env::var("ORDIVON_RECONCILE_INTERVAL_MS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(15_000);
    if reconcile_interval_ms == 0 {
        return Err("ORDIVON_RECONCILE_INTERVAL_MS must be positive".into());
    }
    let reconcile_batch_size: u32 = std::env::var("ORDIVON_RECONCILE_BATCH_SIZE")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(32);
    if reconcile_batch_size == 0 {
        return Err("ORDIVON_RECONCILE_BATCH_SIZE must be positive".into());
    }

    let global_limit = std::env::var("ORDIVON_GLOBAL_MAX_CONCURRENCY")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(8);
    if global_limit == 0 {
        return Err("ORDIVON_GLOBAL_MAX_CONCURRENCY must be positive".into());
    }
    let max_runtime_ms: u64 = std::env::var("ORDIVON_MAX_RUNTIME_MS")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(900_000);
    if max_runtime_ms == 0 {
        return Err("ORDIVON_MAX_RUNTIME_MS must be positive".into());
    }
    let max_output_bytes: u64 = std::env::var("ORDIVON_MAX_OUTPUT_BYTES")
        .ok()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(16 * 1024 * 1024);
    if max_output_bytes == 0 {
        return Err("ORDIVON_MAX_OUTPUT_BYTES must be positive".into());
    }
    let principal =
        std::env::var("ORDIVON_PRINCIPAL").unwrap_or_else(|_| "principal:local-owner".to_string());
    let (workspace_root, workspace_uid, workspace_gid) = (None, None, None);

    Ok(AppConfig {
        bind,
        token,
        body_limit_bytes,
        trust_cf_access,
        reconcile_interval_ms,
        reconcile_batch_size,
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
                    max_runtime_ms,
                    max_output_bytes,
                },
                startup_grace_ms,
            },
            input_authorities,
            execution: ExecutionContext {
                principal,
                global_limit,
            },
            trace_path,
        },
    })
}

const MAX_BEARER_TOKEN_FILE_BYTES: u64 = 16_384;

fn load_bearer_token() -> Result<String, Box<dyn std::error::Error>> {
    let inline = optional_env("ORDIVON_BEARER_TOKEN")?;
    let token_file = optional_env("ORDIVON_BEARER_TOKEN_FILE")?;
    match (inline, token_file) {
        (Some(_), Some(_)) => {
            Err("configure exactly one of ORDIVON_BEARER_TOKEN or ORDIVON_BEARER_TOKEN_FILE".into())
        }
        (Some(token), None) => Ok(token),
        (None, Some(path)) => read_private_token_file(Path::new(&path)),
        (None, None) => {
            Err("one of ORDIVON_BEARER_TOKEN or ORDIVON_BEARER_TOKEN_FILE is required".into())
        }
    }
}

fn read_private_token_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if !path.is_absolute() {
        return Err("ORDIVON_BEARER_TOKEN_FILE must be absolute".into());
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err("Runtime Bearer token path must be a regular file".into());
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err("Runtime Bearer token file must not be accessible by group or others".into());
    }
    if metadata.len() > MAX_BEARER_TOKEN_FILE_BYTES {
        return Err("Runtime Bearer token file exceeds the configured bound".into());
    }
    let raw = fs::read_to_string(path)?;
    let token = raw.trim();
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err("Runtime Bearer token file must contain one non-whitespace token".into());
    }
    Ok(token.to_owned())
}

fn optional_env(name: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(format!("{name} is invalid: {error}").into()),
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
            tracing::warn!("HTTP trace lock poisoned: {error}");
            return;
        }
    };
    let result = append_rotating_jsonl(path, &record, DEFAULT_TRACE_ROTATION_BYTES);
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
    use super::{
        read_private_token_file, request_is_authorized, validate_loopback_bind, HttpState,
    };
    use axum::http::{header, HeaderMap, HeaderValue};
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};
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
    fn private_token_file_contract_is_strict() {
        let root =
            std::env::temp_dir().join(format!("ordivon-runtime-token-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let token_path = root.join("runtime-mcp.token");
        fs::write(&token_path, "local-secret-token-value-1234567890\n").unwrap();
        fs::set_permissions(&token_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            read_private_token_file(&token_path).unwrap(),
            "local-secret-token-value-1234567890"
        );

        fs::set_permissions(&token_path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(read_private_token_file(&token_path)
            .unwrap_err()
            .to_string()
            .contains("group or others"));
        fs::set_permissions(&token_path, fs::Permissions::from_mode(0o600)).unwrap();

        let link_path = root.join("runtime-mcp-link.token");
        symlink(&token_path, &link_path).unwrap();
        assert!(read_private_token_file(&link_path)
            .unwrap_err()
            .to_string()
            .contains("regular file"));
        fs::remove_dir_all(&root).unwrap();
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
