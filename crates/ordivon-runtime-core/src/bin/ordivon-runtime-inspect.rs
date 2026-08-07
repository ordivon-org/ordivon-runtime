use ordivon_runtime_core::{
    inspect_job, inspect_workspace, summarize_experience, RuntimeInspectionConfig,
    RuntimeWorkspaceInspectionConfig, DEFAULT_INSPECTION_EVENT_LIMIT,
    DEFAULT_WORKSPACE_INSPECTION_JOB_LIMIT, MAX_INSPECTION_EVENT_LIMIT,
    MAX_WORKSPACE_INSPECTION_JOB_LIMIT,
};
use std::env;
use std::path::PathBuf;

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;
    let mut database = None;
    let mut store_root = None;
    let mut busy_timeout_ms = 5_000_u64;
    let mut pretty = false;
    let mut job_id = None;
    let mut workspace_id = None;
    let mut event_limit = DEFAULT_INSPECTION_EVENT_LIMIT;
    let mut job_limit = DEFAULT_WORKSPACE_INSPECTION_JOB_LIMIT;
    let mut include_detail = false;
    let mut since_ms = 0_u64;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--database" => database = Some(PathBuf::from(require_value(&mut args, "--database")?)),
            "--store-root" => {
                store_root = Some(PathBuf::from(require_value(&mut args, "--store-root")?))
            }
            "--busy-timeout-ms" => {
                busy_timeout_ms = require_value(&mut args, "--busy-timeout-ms")?
                    .parse()
                    .map_err(|_| "--busy-timeout-ms must be an integer".to_string())?;
            }
            "--pretty" => pretty = true,
            "--job-id" => job_id = Some(require_value(&mut args, "--job-id")?),
            "--workspace-id" => workspace_id = Some(require_value(&mut args, "--workspace-id")?),
            "--event-limit" => {
                event_limit = require_value(&mut args, "--event-limit")?
                    .parse()
                    .map_err(|_| "--event-limit must be an integer".to_string())?;
            }
            "--include-detail" => include_detail = true,
            "--job-limit" => {
                job_limit = require_value(&mut args, "--job-limit")?
                    .parse()
                    .map_err(|_| "--job-limit must be an integer".to_string())?;
            }
            "--since-ms" => {
                since_ms = require_value(&mut args, "--since-ms")?
                    .parse()
                    .map_err(|_| "--since-ms must be an integer".to_string())?;
            }
            "--help" | "-h" => return Err(usage()),
            _ => return Err(format!("unknown argument: {argument}\n{}", usage())),
        }
    }

    let database = database.ok_or_else(usage)?;
    let value = match command.as_str() {
        "job" => {
            if since_ms != 0
                || store_root.is_some()
                || workspace_id.is_some()
                || job_limit != DEFAULT_WORKSPACE_INSPECTION_JOB_LIMIT
            {
                return Err(
                    "--since-ms, --store-root, --workspace-id, and --job-limit are not valid for job"
                        .to_string(),
                );
            }
            let job_id = job_id.ok_or_else(|| "job requires --job-id".to_string())?;
            serde_json::to_value(
                inspect_job(
                    &RuntimeInspectionConfig {
                        db_path: database.clone(),
                        busy_timeout_ms,
                    },
                    &job_id,
                    event_limit,
                    include_detail,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        "summary" => {
            if job_id.is_some()
                || workspace_id.is_some()
                || store_root.is_some()
                || job_limit != DEFAULT_WORKSPACE_INSPECTION_JOB_LIMIT
                || event_limit != DEFAULT_INSPECTION_EVENT_LIMIT
                || include_detail
            {
                return Err(
                    "--job-id, --workspace-id, --store-root, --job-limit, --event-limit, and --include-detail are not valid for summary"
                        .to_string(),
                );
            }
            serde_json::to_value(
                summarize_experience(
                    &RuntimeInspectionConfig {
                        db_path: database.clone(),
                        busy_timeout_ms,
                    },
                    since_ms,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        "workspace" => {
            if job_id.is_some()
                || since_ms != 0
                || event_limit != DEFAULT_INSPECTION_EVENT_LIMIT
                || include_detail
            {
                return Err(
                    "--job-id, --since-ms, --event-limit, and --include-detail are not valid for workspace"
                        .to_string(),
                );
            }
            let workspace_id = workspace_id
                .ok_or_else(|| "workspace requires --workspace-id".to_string())?;
            let store_root = store_root
                .ok_or_else(|| "workspace requires --store-root".to_string())?;
            serde_json::to_value(
                inspect_workspace(
                    &RuntimeWorkspaceInspectionConfig {
                        db_path: database.clone(),
                        store_root,
                        busy_timeout_ms,
                    },
                    &workspace_id,
                    job_limit,
                )
                .map_err(|error| error.to_string())?,
            )
        }
        _ => return Err(usage()),
    }
    .map_err(|error| format!("cannot serialize Runtime inspection: {error}"))?;

    let output = if pretty {
        serde_json::to_string_pretty(&value)
    } else {
        serde_json::to_string(&value)
    }
    .map_err(|error| format!("cannot serialize Runtime inspection: {error}"))?;
    println!("{output}");
    Ok(())
}

fn require_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn usage() -> String {
    format!(
        "usage:\n  ordivon-runtime-inspect job --database ABSOLUTE_PATH --job-id ID [--event-limit N<= {MAX_INSPECTION_EVENT_LIMIT}] [--include-detail] [--busy-timeout-ms N] [--pretty]\n  ordivon-runtime-inspect workspace --database ABSOLUTE_PATH --store-root ABSOLUTE_PATH --workspace-id ID [--job-limit N<= {MAX_WORKSPACE_INSPECTION_JOB_LIMIT}] [--busy-timeout-ms N] [--pretty]\n  ordivon-runtime-inspect summary --database ABSOLUTE_PATH [--since-ms UNIX_MS] [--busy-timeout-ms N] [--pretty]"
    )
}
