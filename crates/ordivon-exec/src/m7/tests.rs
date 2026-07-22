use super::*;
use std::path::PathBuf;

fn config() -> M7RuntimeHardeningConfig {
    M7RuntimeHardeningConfig {
        worker: M7WorkerIdentity {
            user: "ordivon-worker".to_string(),
            group: "ordivon-worker".to_string(),
            uid: 65_534,
            gid: 65_534,
        },
        control_root: PathBuf::from("/var/lib/ordivon/control"),
        worker_root: PathBuf::from("/var/lib/ordivon/worker"),
        cache_root: PathBuf::from("/var/cache/ordivon-worker"),
        runtime_view_root: PathBuf::from("/run/ordivon"),
    }
}

#[test]
fn isolated_layout_accepts_distinct_absolute_roots() {
    config().validate_layout().unwrap();
}

#[test]
fn isolated_layout_rejects_root_worker() {
    let mut config = config();
    config.worker.uid = 0;
    assert!(config.validate_layout().is_err());
}

#[test]
fn isolated_layout_rejects_overlapping_control_and_worker_roots() {
    let mut config = config();
    config.worker_root = config.control_root.join("worker");
    assert!(config.validate_layout().is_err());
}

#[test]
fn isolated_layout_rejects_relative_roots() {
    let mut config = config();
    config.cache_root = PathBuf::from("cache");
    assert!(config.validate_layout().is_err());
}
