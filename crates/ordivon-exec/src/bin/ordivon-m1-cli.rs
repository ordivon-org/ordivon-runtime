use ordivon_exec::{
    cancel_universal_task, create_git_workspace, get_universal_task, read_task_artifact,
    read_workspace_text, start_universal_task, workspace_diff, write_workspace_text,
    ArtifactReadRequest, GitWorkspaceCreateRequest, TaskCancelRequest, TaskGetRequest,
    UniversalExecError, UniversalExecRequest, UniversalExecutorConfig, WorkspaceDiffRequest,
    WorkspaceReadRequest, WorkspaceWriteRequest,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let command = match std::env::args().nth(1) {
        Some(command) => command,
        None => {
            eprintln!("usage: ordivon-m1-cli <command> < request.json");
            return ExitCode::from(64);
        }
    };
    let config = match config_from_env() {
        Ok(config) => config,
        Err(error) => return emit_error(error),
    };
    let mut body = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut body) {
        return emit_error(UniversalExecError::new_for_cli(format!(
            "cannot read stdin: {error}"
        )));
    }
    let outcome = match command.as_str() {
        "workspace-create" => dispatch::<GitWorkspaceCreateRequest, _>(&body, |request| {
            create_git_workspace(&config, request)
        }),
        "workspace-read" => dispatch::<WorkspaceReadRequest, _>(&body, |request| {
            read_workspace_text(&config, request)
        }),
        "workspace-write" => dispatch::<WorkspaceWriteRequest, _>(&body, |request| {
            write_workspace_text(&config, request)
        }),
        "workspace-diff" => {
            dispatch::<WorkspaceDiffRequest, _>(&body, |request| workspace_diff(&config, request))
        }
        "task-start" => dispatch::<UniversalExecRequest, _>(&body, |request| {
            start_universal_task(&config, request)
        }),
        "task-get" => {
            dispatch::<TaskGetRequest, _>(&body, |request| get_universal_task(&config, request))
        }
        "task-cancel" => dispatch::<TaskCancelRequest, _>(&body, |request| {
            cancel_universal_task(&config, request)
        }),
        "artifact-read" => dispatch::<ArtifactReadRequest, _>(&body, |request| {
            read_task_artifact(&config, request)
        }),
        _ => Err(UniversalExecError::new_for_cli(format!(
            "unknown command: {command}"
        ))),
    };
    match outcome {
        Ok(value) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"ok": true, "result": value})).unwrap()
            );
            ExitCode::SUCCESS
        }
        Err(error) => emit_error(error),
    }
}

fn dispatch<T, R>(
    body: &str,
    operation: impl FnOnce(&T) -> Result<R, UniversalExecError>,
) -> Result<Value, UniversalExecError>
where
    T: DeserializeOwned,
    R: Serialize,
{
    let request: T = serde_json::from_str(body).map_err(|error| {
        UniversalExecError::new_for_cli(format!("invalid request JSON: {error}"))
    })?;
    let result = operation(&request)?;
    serde_json::to_value(result).map_err(|error| {
        UniversalExecError::new_for_cli(format!("cannot serialize result: {error}"))
    })
}

fn config_from_env() -> Result<UniversalExecutorConfig, UniversalExecError> {
    let store_root = required_env("ORDIVON_M1_STORE_ROOT")?;
    let runner_path = required_env("ORDIVON_M1_RUNNER_PATH")?;
    let roots = required_env("ORDIVON_M1_ALLOWED_EXECUTABLE_ROOTS")?;
    let allowed_executable_roots = roots
        .split(':')
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect();
    let config = UniversalExecutorConfig {
        store_root: PathBuf::from(store_root),
        runner_path: PathBuf::from(runner_path),
        allowed_executable_roots,
        max_runtime_ms: 900_000,
        max_output_bytes: 16 * 1024 * 1024,
    };
    config.validate()?;
    Ok(config)
}

fn required_env(name: &str) -> Result<String, UniversalExecError> {
    std::env::var(name)
        .map_err(|error| UniversalExecError::new_for_cli(format!("{name} is required: {error}")))
}

fn emit_error(error: UniversalExecError) -> ExitCode {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"ok": false, "error": error})).unwrap()
    );
    ExitCode::from(2)
}
