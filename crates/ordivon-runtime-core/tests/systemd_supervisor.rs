#![cfg(feature = "systemd-supervisor")]

use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Marker {
    pid: u32,
    process_start_ticks: u64,
    cgroup: String,
    ignored_sigterm: bool,
    detached_session: bool,
}

struct SpikeUnit {
    unit: String,
    marker_dir: PathBuf,
}

impl Drop for SpikeUnit {
    fn drop(&mut self) {
        let _ = Command::new("systemctl")
            .args(["stop", &self.unit])
            .output();
        let _ = Command::new("systemctl")
            .args(["reset-failed", &self.unit])
            .output();
        let _ = fs::remove_dir_all(&self.marker_dir);
    }
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, and explicit local opt-in"]
fn launcher_exit_recovery_and_cgroup_stop_are_real() {
    require_opt_in();
    let spike = launch_tree("lifecycle", 300_000, 0);
    let markers = wait_for_markers(&spike.marker_dir);
    let properties = systemctl_show(&spike.unit);
    assert_eq!(
        properties.get("LoadState").map(String::as_str),
        Some("loaded")
    );
    assert_eq!(
        properties.get("ActiveState").map(String::as_str),
        Some("active")
    );
    assert_eq!(
        properties.get("SubState").map(String::as_str),
        Some("running")
    );
    let control_group = properties.get("ControlGroup").unwrap();
    assert_eq!(markers["root"].cgroup, *control_group);
    assert_eq!(markers["child"].cgroup, *control_group);
    assert_eq!(markers["grandchild"].cgroup, *control_group);
    assert!(markers["root"].ignored_sigterm);
    assert!(markers["child"].ignored_sigterm);
    assert!(markers["child"].detached_session);
    assert!(markers["grandchild"].ignored_sigterm);
    for marker in markers.values() {
        assert_eq!(
            read_start_ticks(marker.pid),
            Some(marker.process_start_ticks)
        );
    }

    let started = Instant::now();
    assert!(Command::new("systemctl")
        .args(["stop", &spike.unit])
        .status()
        .unwrap()
        .success());
    assert!(started.elapsed() >= Duration::from_millis(1_500));
    wait_for_pids_to_exit(markers.values().map(|marker| marker.pid));
    let after = systemctl_show(&spike.unit);
    assert_eq!(after.get("Result").map(String::as_str), Some("timeout"));
    assert_eq!(after.get("ExecMainCode").map(String::as_str), Some("2"));
    assert_eq!(after.get("ExecMainStatus").map(String::as_str), Some("9"));
}

#[test]
#[ignore = "requires root, systemd, cgroup v2, and explicit local opt-in"]
fn successful_units_can_be_garbage_collected_while_failed_units_retain_exit_evidence() {
    require_opt_in();
    let successful = launch_single("success", 50, 0);
    wait_for_terminal(&successful.unit);
    let success = wait_for_load_state(&successful.unit, "not-found");
    assert_eq!(
        success.get("ActiveState").map(String::as_str),
        Some("inactive")
    );

    let failed = launch_single("failure", 50, 7);
    wait_for_terminal(&failed.unit);
    let failure = systemctl_show(&failed.unit);
    assert_eq!(failure.get("LoadState").map(String::as_str), Some("loaded"));
    assert_eq!(
        failure.get("ActiveState").map(String::as_str),
        Some("failed")
    );
    assert_eq!(failure.get("Result").map(String::as_str), Some("exit-code"));
    assert_eq!(failure.get("ExecMainCode").map(String::as_str), Some("1"));
    assert_eq!(failure.get("ExecMainStatus").map(String::as_str), Some("7"));
}

fn require_opt_in() {
    assert_eq!(env::var("ORDIVON_RUN_SYSTEMD_SPIKE").as_deref(), Ok("1"));
    assert_eq!(unsafe { libc::geteuid() }, 0, "systemd spike requires root");
    assert_eq!(
        fs::read_to_string("/proc/1/comm").unwrap().trim(),
        "systemd"
    );
    let cgroup_type = Command::new("stat")
        .args(["-fc", "%T", "/sys/fs/cgroup"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(cgroup_type.stdout).unwrap().trim(),
        "cgroup2fs"
    );
}

fn launch_tree(label: &str, hold_ms: u64, exit_code: u8) -> SpikeUnit {
    launch(label, hold_ms, exit_code, true)
}

fn launch_single(label: &str, hold_ms: u64, exit_code: u8) -> SpikeUnit {
    launch(label, hold_ms, exit_code, false)
}

fn launch(label: &str, hold_ms: u64, exit_code: u8, tree: bool) -> SpikeUnit {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let unit = format!(
        "ordivon-spike-test-{label}-{}-{unique}.service",
        std::process::id()
    );
    let marker_dir = env::temp_dir().join(unit.trim_end_matches(".service"));
    fs::create_dir_all(&marker_dir).unwrap();
    let mut command = Command::new("systemd-run");
    command
        .arg(format!("--unit={unit}"))
        .args([
            "--property=Type=exec",
            "--property=KillMode=control-group",
            "--property=TimeoutStopSec=2s",
            "--property=SendSIGKILL=yes",
            "--property=StandardOutput=journal",
            "--property=StandardError=journal",
        ])
        .arg(env!("CARGO_BIN_EXE_ordivon-runtime-job-fixture"))
        .args(["--role", "root", "--marker-dir"])
        .arg(&marker_dir)
        .args([
            "--hold-ms",
            &hold_ms.to_string(),
            "--exit-code",
            &exit_code.to_string(),
        ]);
    if tree {
        command.args(["--ignore-term", "--spawn-descendant"]);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "systemd-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    SpikeUnit { unit, marker_dir }
}

fn wait_for_markers(marker_dir: &Path) -> BTreeMap<String, Marker> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result: Option<BTreeMap<String, Marker>> = ["root", "child", "grandchild"]
            .into_iter()
            .map(|role| {
                let path = marker_dir.join(format!("{role}.json"));
                fs::read_to_string(path)
                    .ok()
                    .and_then(|body| serde_json::from_str(&body).ok())
                    .map(|marker| (role.to_string(), marker))
            })
            .collect();
        if let Some(markers) = result {
            return markers;
        }
        assert!(Instant::now() < deadline, "fixture markers did not appear");
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_terminal(unit: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = systemctl_show(unit);
        if let Some("inactive" | "failed") = state.get("ActiveState").map(String::as_str) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "unit did not reach a terminal state"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_load_state(unit: &str, expected: &str) -> BTreeMap<String, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = systemctl_show(unit);
        if state.get("LoadState").map(String::as_str) == Some(expected) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "unit did not reach load state {expected}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_pids_to_exit(pids: impl Iterator<Item = u32>) {
    let pids: Vec<u32> = pids.collect();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if pids
            .iter()
            .all(|pid| !Path::new(&format!("/proc/{pid}")).exists())
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "fixture processes survived cgroup stop"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn systemctl_show(unit: &str) -> BTreeMap<String, String> {
    let output = Command::new("systemctl")
        .arg("show")
        .arg(unit)
        .args([
            "-pLoadState",
            "-pActiveState",
            "-pSubState",
            "-pResult",
            "-pInvocationID",
            "-pControlGroup",
            "-pMainPID",
            "-pExecMainCode",
            "-pExecMainStatus",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn read_start_ticks(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = stat.rfind(')')?;
    stat[close + 1..].split_whitespace().nth(19)?.parse().ok()
}
