//! Crash-safe file write helper.
//!
//! The pattern: write to a sibling tmp file, fsync, then rename over
//! the destination. On POSIX rename(2) is atomic — readers see either
//! the old contents or the new contents, never a half-written file.
//!
//! Used by the TOML side-cars (`config.toml`, the auth-token cache)
//! that don't get the WAL crash-safety the SQLite stores have.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

/// Atomically replace `path` with `bytes`. Returns `io::Error` on any
/// step failure; does NOT leave a half-written `path` behind because
/// the rename is the last step and is atomic on POSIX.
///
/// Concurrent writers each get a unique tmp name (pid + counter) so
/// two simultaneous saves don't race on the same tmp path.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = tmp_path(path);
    {
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        // sync_all forces both data and metadata to disk before the
        // rename — without this a power loss after rename could leave
        // the new path pointing at unflushed blocks.
        f.sync_all()?;
    }
    // rename(2) is atomic on POSIX: either the old contents or the
    // new contents are visible, never partial.
    if let Err(e) = fs::rename(&tmp, path) {
        // Best-effort cleanup of the tmp file before bubbling up.
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

fn tmp_path(path: &Path) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stem = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".{}.{}.{}.tmp", stem, process::id(), n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_creates_file_with_contents() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_atomic(&path, b"key = \"value\"").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "key = \"value\"");
    }

    #[test]
    fn atomic_write_overwrites_existing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.toml");
        fs::write(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn atomic_write_creates_parent_dir() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/sub/dir/x.toml");
        write_atomic(&path, b"hi").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hi");
    }

    #[test]
    fn atomic_write_leaves_no_tmp_on_success() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a.toml");
        write_atomic(&path, b"x").unwrap();
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        // Only the destination file remains; no .tmp leftovers.
        assert_eq!(entries, vec!["a.toml"]);
    }
}
