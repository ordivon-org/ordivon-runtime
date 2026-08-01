use serde::Serialize;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const DEFAULT_TRACE_ROTATION_BYTES: u64 = 64 * 1024 * 1024;

static TRACE_WRITE_LOCK: Mutex<()> = Mutex::new(());

pub fn rotated_trace_path(path: &Path) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(".1");
    PathBuf::from(name)
}

pub fn append_rotating_jsonl<T: Serialize>(
    path: &Path,
    value: &T,
    max_bytes: u64,
) -> io::Result<()> {
    if max_bytes == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "trace rotation limit must be positive",
        ));
    }
    let _guard = TRACE_WRITE_LOCK
        .lock()
        .map_err(|_| io::Error::other("trace write lock is poisoned"))?;
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.len() > 0 && metadata.len().saturating_add(bytes.len() as u64) > max_bytes {
            let rotated = rotated_trace_path(path);
            match fs::remove_file(&rotated) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            fs::rename(path, rotated)?;
        }
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&bytes)
}

#[cfg(test)]
mod tests {
    use super::{append_rotating_jsonl, rotated_trace_path};
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::fs;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_root(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ordivon-runtime-trace-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn trace_rotation_keeps_one_previous_segment() {
        let root = temporary_root("rotation");
        let path = root.join("runtime.jsonl");
        append_rotating_jsonl(&path, &json!({"event": "first"}), 40).unwrap();
        append_rotating_jsonl(&path, &json!({"event": "second-value"}), 40).unwrap();
        assert!(rotated_trace_path(&path).is_file());
        assert!(fs::read_to_string(&path).unwrap().contains("second-value"));
        append_rotating_jsonl(&path, &json!({"event": "third-value"}), 40).unwrap();
        assert!(fs::read_to_string(rotated_trace_path(&path))
            .unwrap()
            .contains("second-value"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_writers_rotate_once_without_losing_new_records() {
        let root = temporary_root("concurrent");
        let path = root.join("runtime.jsonl");
        let prefill = "x".repeat(4_090);
        fs::write(&path, &prefill).unwrap();
        let barrier = Arc::new(Barrier::new(33));
        let mut threads = Vec::new();
        for id in 0..32_u32 {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            threads.push(thread::spawn(move || {
                barrier.wait();
                append_rotating_jsonl(&path, &json!({"id": id}), 4_096).unwrap();
            }));
        }
        barrier.wait();
        for writer in threads {
            writer.join().unwrap();
        }
        assert_eq!(
            fs::read_to_string(rotated_trace_path(&path)).unwrap(),
            prefill
        );
        let observed = fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["id"]
                    .as_u64()
                    .unwrap()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(observed, (0_u64..32).collect());
        fs::remove_dir_all(root).unwrap();
    }
}
