//! Atomic file writes: a failure part-way never leaves half a file behind.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Makes each temporary name unique within the process, so two threads saving the same path
/// do not collide on one temporary file.
static WRITES: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` by way of a temporary file beside it, then renames
/// that over `path`. Any file already at `path` stays whole until the rename,
/// and the temporary file is removed when anything fails.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no file name"))?;
    let mut temp_name = OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let temp = path.with_file_name(temp_name);

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::write_atomic;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kerf-atomic-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn overwrites_an_existing_file_and_leaves_no_temporary_file() {
        let dir = folder("overwrite");
        let path = dir.join("patch.kerf");
        fs::write(&path, b"old and much longer than the new text").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["patch.kerf"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_folder_is_an_error_and_a_failed_rename_cleans_up() {
        let dir = folder("errors");
        let err = write_atomic(&dir.join("nowhere").join("a.kerf"), b"x").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        // A folder cannot be replaced by a file: the rename fails.
        let target = dir.join("taken");
        fs::create_dir(&target).unwrap();
        assert!(write_atomic(&target, b"x").is_err());
        assert!(target.is_dir());
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["taken"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn concurrent_writes_to_one_path_all_succeed() {
        let dir = folder("threads");
        let path = dir.join("patch.kerf");
        std::thread::scope(|scope| {
            for i in 0..8 {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..20 {
                        write_atomic(path, format!("writer {i}").as_bytes()).unwrap();
                    }
                });
            }
        });
        assert!(fs::read_to_string(&path).unwrap().starts_with("writer "));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
