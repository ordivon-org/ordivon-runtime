use serde::Serialize;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const DEFAULT_TRACE_ROTATION_BYTES: u64 = 64 * 1024 * 1024;

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
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn trace_rotation_keeps_one_previous_segment() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ordivon-runtime-trace-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
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
}
