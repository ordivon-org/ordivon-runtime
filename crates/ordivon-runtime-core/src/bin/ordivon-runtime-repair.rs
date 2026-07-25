use ordivon_runtime_core::{
    apply_runtime_repair, RuntimeDoctorConfig, RuntimeRepairConfig, RuntimeRepairRequest,
};
use std::collections::BTreeSet;
use std::env;
use std::path::PathBuf;

fn main() {
    match run() {
        Ok(()) => {}
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() != Some("apply") {
        return Err(usage());
    }
    let mut database = None;
    let mut store_root = None;
    let mut expected_fingerprint = None;
    let mut snapshot_path = None;
    let mut principal = None;
    let mut finalize_lost_attempt_ids = BTreeSet::new();
    let mut busy_timeout_ms = 5_000_u64;
    let mut apply = false;
    let mut pretty = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--database" => database = Some(PathBuf::from(require_value(&mut args, "--database")?)),
            "--store-root" => {
                store_root = Some(PathBuf::from(require_value(&mut args, "--store-root")?))
            }
            "--expected-fingerprint" => {
                expected_fingerprint = Some(require_value(&mut args, "--expected-fingerprint")?)
            }
            "--snapshot" => {
                snapshot_path = Some(PathBuf::from(require_value(&mut args, "--snapshot")?))
            }
            "--principal" => principal = Some(require_value(&mut args, "--principal")?),
            "--finalize-lost" => {
                finalize_lost_attempt_ids.insert(require_value(&mut args, "--finalize-lost")?);
            }
            "--busy-timeout-ms" => {
                busy_timeout_ms = require_value(&mut args, "--busy-timeout-ms")?
                    .parse()
                    .map_err(|_| "--busy-timeout-ms must be an integer".to_string())?;
            }
            "--apply" => apply = true,
            "--pretty" => pretty = true,
            _ => return Err(format!("unknown argument: {argument}\n{}", usage())),
        }
    }
    if !apply {
        return Err("refusing to write without explicit --apply".to_string());
    }
    let config = RuntimeRepairConfig {
        doctor: RuntimeDoctorConfig {
            db_path: database.ok_or_else(usage)?,
            store_root: store_root.ok_or_else(usage)?,
            busy_timeout_ms,
        },
    };
    let request = RuntimeRepairRequest {
        expected_fingerprint: expected_fingerprint.ok_or_else(usage)?,
        snapshot_path: snapshot_path.ok_or_else(usage)?,
        principal: principal.ok_or_else(usage)?,
        finalize_lost_attempt_ids,
    };
    let report = apply_runtime_repair(&config, &request).map_err(|error| error.to_string())?;
    let output = if pretty {
        serde_json::to_string_pretty(&report)
    } else {
        serde_json::to_string(&report)
    }
    .map_err(|error| format!("cannot serialize repair report: {error}"))?;
    println!("{output}");
    Ok(())
}

fn require_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn usage() -> String {
    "usage: ordivon-runtime-repair apply --database ABSOLUTE_PATH --store-root ABSOLUTE_PATH --expected-fingerprint sha256:... --snapshot ABSOLUTE_PATH --principal NAME [--finalize-lost ATTEMPT_ID ...] --apply [--pretty]".to_string()
}
