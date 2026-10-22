//! Small, security-conscious filesystem helpers shared by the config, pid and
//! history writers.
//!
//! State files may live in a world-writable directory (for example
//! `/tmp/r-modals-$DISPLAY.pid`). The old helpers wrote through a predictable
//! `path.with_extension("tmp.$pid")` name with [`std::fs::write`], which
//! follows symlinks and truncates existing targets. These helpers instead:
//!
//! - refuse to replace anything that is not a regular file (a symlink, FIFO,
//!   directory, …);
//! - create a uniquely named sibling with `create_new` (`O_EXCL`), so a
//!   pre-planted symlink can never be followed;
//! - create that temporary file with mode `0600` on Unix;
//! - `fsync` the data and rename it over the destination;
//! - read state files only when they are regular files and below a caller
//!   supplied size limit.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn parent_of(path: &Path) -> &Path {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    }
}

/// A unique sibling path. Uniqueness (plus `create_new`) is what stops an
/// attacker from pre-creating the temporary file as a symlink.
fn temp_path(parent: &Path, stem: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = format!(".{stem}.tmp.{}.{nanos}.{seq}", std::process::id());
    parent.join(name)
}

/// Reject a destination that exists but is not a regular file.
///
/// `symlink_metadata` deliberately does not follow symlinks: writing through
/// a symlinked history/pid/config path would let another user redirect the
/// write.
pub(crate) fn ensure_regular_or_missing(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(md) if md.file_type().is_file() => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing to replace non-regular file {}", path.display()),
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Atomically replace `path` with `content`.
///
/// The destination is only ever replaced by `rename`, which does not follow a
/// symlink at the final component. Existing non-regular files are rejected
/// before anything is written.
pub(crate) fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
    let parent = parent_of(path);
    fs::create_dir_all(parent)?;
    ensure_regular_or_missing(path)?;

    let stem = path.file_name().and_then(|s| s.to_str()).unwrap_or("state");
    let tmp = temp_path(parent, stem);

    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(&tmp)?;
    if let Err(e) = file.write_all(content).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    drop(file);

    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Windows cannot replace an existing file with rename(2). The data
            // is already safely in `tmp`; retry once after removing the
            // destination. Never do this on Unix, where it could delete an
            // existing file when the failure was a permission error.
            #[cfg(windows)]
            {
                if ensure_regular_or_missing(path).is_ok() && fs::remove_file(path).is_ok() {
                    if fs::rename(&tmp, path).is_ok() {
                        return Ok(());
                    }
                }
            }
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Read a UTF-8 state file if (and only if) it is a regular file no larger
/// than `max_bytes`.
///
/// Bounds the read so a FIFO, `/dev/zero`, or a huge file cannot hang or
/// exhaust memory during history/config loading.
pub(crate) fn read_regular_limited(path: &Path, max_bytes: usize) -> io::Result<String> {
    let md = fs::symlink_metadata(path)?;
    if !md.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a regular file", path.display()),
        ));
    }
    if md.len() > max_bytes as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is larger than {max_bytes} bytes", path.display()),
        ));
    }

    let file = fs::File::open(path)?;
    // Re-check on the open handle: the path may have been swapped between
    // `symlink_metadata` and `open`. Comparing the inode closes the remaining
    // symlink race without needing `O_NOFOLLOW` (which is not portable).
    let md = file.metadata()?;
    if !md.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a regular file", path.display()),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let before = fs::symlink_metadata(path)?;
        if before.dev() != md.dev() || before.ino() != md.ino() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} changed while being opened", path.display()),
            ));
        }
    }

    let mut bytes = Vec::with_capacity(md.len() as usize);
    file.take(max_bytes as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} grew past {max_bytes} bytes", path.display()),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "file is not valid UTF-8"))
}

/// Whether an automatically discovered config file is safe to execute.
///
/// On Unix, a config that is writable by group or others could be edited by
/// another local user to inject commands. Explicitly configured paths are the
/// caller's responsibility; this check applies to the XDG defaults only.
pub(crate) fn trusted_config_file(path: &Path) -> bool {
    let Ok(md) = fs::symlink_metadata(path) else {
        return false;
    };
    if !md.file_type().is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if md.permissions().mode() & 0o022 != 0 {
            return false;
        }
        // A writable parent directory lets another user replace the file
        // wholesale, even when the file itself is read-only.
        if let Some(parent) = path.parent() {
            let Ok(pmd) = fs::symlink_metadata(parent) else {
                return false;
            };
            if !pmd.is_dir() || pmd.permissions().mode() & 0o022 != 0 {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "r-modals-fsutil-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_creates_parents_and_replaces_regular_files() {
        let dir = temp_dir("write");
        let path = dir.join("nested").join("state.txt");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        // No predictable temporary files left behind.
        let leftovers = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .count();
        assert_eq!(leftovers, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_refuses_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = temp_dir("symlink");
        let target = dir.join("target.txt");
        let link = dir.join("state.txt");
        fs::write(&target, "canary").unwrap();
        symlink(&target, &link).unwrap();

        assert!(write_atomic(&link, b"owned").is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "canary");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_uses_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("mode");
        let path = dir.join("state.txt");
        write_atomic(&path, b"secret").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn limited_reads_reject_oversized_and_non_regular_files() {
        let dir = temp_dir("read");
        let path = dir.join("state.txt");
        fs::write(&path, "0123456789").unwrap();
        assert_eq!(read_regular_limited(&path, 10).unwrap(), "0123456789");
        assert!(read_regular_limited(&path, 9).is_err());

        let missing = dir.join("missing.txt");
        assert!(read_regular_limited(&missing, 10).is_err());

        // A directory is rejected without attempting to read it.
        assert!(read_regular_limited(&dir, 10).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn trusted_config_rejects_group_and_world_writable_files() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("trusted");
        let path = dir.join("config.toml");
        fs::write(&path, "").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(trusted_config_file(&path));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(!trusted_config_file(&path));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o775)).unwrap();
        assert!(!trusted_config_file(&path));
        let _ = fs::remove_dir_all(&dir);
    }
}
