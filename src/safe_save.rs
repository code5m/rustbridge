//! Conservative atomic source replacement that refuses unsupported metadata rather than losing it.
//! On Linux this copies all readable xattrs (including POSIX ACL/security labels) or aborts.
//! It never promises a cross-process CAS: a hostile writer can still race the final rename.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(target_os = "linux")]
use std::{ffi::CString, os::unix::ffi::OsStrExt};
#[cfg(unix)]
use std::{
    fs::File,
    os::unix::fs::{MetadataExt, PermissionsExt},
};

#[cfg(any(windows, target_os = "macos"))]
use std::io::{Read, Seek, SeekFrom};
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::path::PathBuf;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    Durable,
    /// The source has already been atomically replaced, but syncing directory metadata failed.
    /// Never re-run the save with the old expected contents.
    CommittedDurabilityUncertain(String),
}

#[cfg(any(target_os = "linux", target_os = "android"))]
struct PendingTemp(PathBuf);
#[cfg(any(target_os = "linux", target_os = "android"))]
impl Drop for PendingTemp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn conflict(message: &str) -> io::Error {
    io::Error::other(message)
}
#[cfg(unix)]
fn lock_concurrent_rustbridge_saves(path: &Path) -> io::Result<File> {
    use std::{
        hash::{Hash, Hasher},
        os::fd::AsRawFd,
        os::unix::fs::OpenOptionsExt,
    };
    let canonical = path.canonicalize()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut hasher);
    let dir = crate::persistence::Store::from_system()?
        .directory()
        .join("locks");
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    let target = dir.join(format!("{:016x}.lock", hasher.finish()));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(target)?;
    // SAFETY: live owned file descriptor, nonblocking advisory mutex per real path.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result != 0 {
        return Err(io::Error::other(
            "Another RustBridge process is saving this file. Retry after it finishes.",
        ));
    }
    Ok(file)
}

pub fn save_if_unchanged(path: &Path, expected: &str, content: &str) -> io::Result<SaveOutcome> {
    save_with_hooks(path, expected, content, || Ok(()), sync_parent)
}
fn sync_parent(parent: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(parent)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
    }
    Ok(())
}

fn save_with_hooks(
    path: &Path,
    expected: &str,
    content: &str,
    before_commit: impl FnOnce() -> io::Result<()>,
    sync_directory: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<SaveOutcome> {
    // A stable per-path OS advisory lock serializes cooperating RustBridge
    // processes across rename. It does NOT lock out unrelated editors.
    #[cfg(unix)]
    let _save_lock = lock_concurrent_rustbridge_saves(path)?;
    let original_meta = fs::symlink_metadata(path)?;
    if original_meta.file_type().is_symlink() || !original_meta.is_file() {
        return Err(conflict(
            "Only real regular source files can be saved; symlinks are rejected.",
        ));
    }
    if original_meta.permissions().readonly() {
        return Err(conflict("Refusing to replace a read-only source file."));
    }
    #[cfg(unix)]
    {
        if original_meta.nlink() != 1 {
            return Err(conflict(
                "Refusing to replace a multiply hard-linked source file.",
            ));
        }
        if original_meta.uid() != unsafe { libc::geteuid() }
            || original_meta.gid() != unsafe { libc::getegid() }
        {
            return Err(conflict(
                "Original owner/group differ from the editor; refusing metadata-changing replacement.",
            ));
        }
        // A file owned by the current user but without owner write permission must
        // remain protected, even if a directory rename would be permitted.
        if original_meta.permissions().mode() & 0o200 == 0 {
            return Err(conflict("Current file owner has no write permission."));
        }
    }
    // On macOS/Windows an in-place handle write retains ACLs and file identity,
    // unlike a naive rename. It is accompanied by a private fsynced rollback
    // copy; unlike the Linux stage+rename it is NOT power-loss atomic.
    #[cfg(any(windows, target_os = "macos"))]
    return portable_save_in_place(path, expected, content, before_commit, sync_directory);

    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let original_attrs = snapshot_xattrs(path)?;
        if fs::read_to_string(path)? != expected {
            return Err(conflict("File contents changed externally; save blocked."));
        }
        if expected == content {
            return Ok(SaveOutcome::Durable);
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let (mut target, guard) = {
            let mut result = None;
            for _ in 0..64 {
                let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
                let candidate = parent.join(format!(
                    ".rustbridge-save-{}-{serial:016x}",
                    std::process::id()
                ));
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&candidate)
                {
                    Ok(file) => {
                        result = Some((file, PendingTemp(candidate)));
                        break;
                    }
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
            result.ok_or_else(|| conflict("Unable to allocate a unique staging file."))?
        };
        target.write_all(content.as_bytes())?;
        fs::set_permissions(&guard.0, original_meta.permissions())?;
        copy_xattrs(&guard.0, &original_attrs)?;
        let staged_meta = fs::metadata(&guard.0)?;
        if staged_meta.permissions().mode() & 0o7777 != original_meta.permissions().mode() & 0o7777
            || staged_meta.uid() != original_meta.uid()
            || staged_meta.gid() != original_meta.gid()
            || snapshot_xattrs(&guard.0)? != original_attrs
        {
            return Err(conflict(
                "Metadata could not be preserved on replacement; original untouched.",
            ));
        }
        target.sync_all()?;
        before_commit()?;
        let current_meta = fs::symlink_metadata(path)?;
        if !current_meta.is_file()
            || current_meta.file_type().is_symlink()
            || current_meta.dev() != original_meta.dev()
            || current_meta.ino() != original_meta.ino()
            || current_meta.permissions().mode() != original_meta.permissions().mode()
            || snapshot_xattrs(path)? != original_attrs
            || fs::read_to_string(path)? != expected
        {
            return Err(conflict(
                "Source changed during save; original was not overwritten.",
            ));
        }
        fs::rename(&guard.0, path)?;
        // Rename is the commitment point. A subsequent error is NOT a failed save:
        // the user's next save must use these newly committed contents as its baseline.
        match sync_directory(parent) {
            Ok(()) => Ok(SaveOutcome::Durable),
            Err(error) => Ok(SaveOutcome::CommittedDurabilityUncertain(error.to_string())),
        }
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn portable_save_in_place(
    path: &Path,
    expected: &str,
    content: &str,
    before_commit: impl FnOnce() -> io::Result<()>,
    sync_directory: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<SaveOutcome> {
    // The original handle remains open for the entire transaction, preserving
    // security descriptors, alternate streams, Finder tags and ownership.
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(windows)]
    {
        options.share_mode(0);
    }
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        let current = fs::symlink_metadata(path)?;
        if current.dev() != metadata.dev()
            || current.ino() != metadata.ino()
            || !current.is_file()
            || current.file_type().is_symlink()
        {
            return Err(conflict(
                "Source path was atomically replaced before save; refusing to write to a stale handle.",
            ));
        }
    }
    if !metadata.is_file() || metadata.permissions().readonly() {
        return Err(conflict(
            "Refusing to edit unsupported or read-only source.",
        ));
    }
    let mut actual = String::new();
    file.read_to_string(&mut actual)?;
    if actual != expected {
        return Err(conflict(
            "File changed externally before save; write denied.",
        ));
    }
    if expected == content {
        return Ok(SaveOutcome::Durable);
    }
    before_commit()?;
    // A cooperative or external writer may have changed the file during the
    // pre-commit hook. Re-check via the same stable open handle immediately
    // before building a backup and performing an in-place commit.
    file.seek(SeekFrom::Start(0))?;
    let mut latest = String::new();
    file.read_to_string(&mut latest)?;
    if latest != expected {
        return Err(conflict(
            "Source changed during portable save; original was not overwritten.",
        ));
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        let current = fs::symlink_metadata(path)?;
        if current.dev() != metadata.dev()
            || current.ino() != metadata.ino()
            || !current.is_file()
            || current.file_type().is_symlink()
        {
            return Err(conflict(
                "Source path was atomically replaced during save; external file preserved.",
            ));
        }
    }
    // Create an independent, user-private rollback before a non-atomic in-place
    // write, so a crash can be recovered manually even on Windows/macOS.
    let backup_dir = crate::persistence::Store::from_system()?
        .directory()
        .join("write-backups");
    fs::create_dir_all(&backup_dir)?;
    #[cfg(unix)]
    {
        fs::set_permissions(&backup_dir, fs::Permissions::from_mode(0o700))?;
    }
    let counter = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let backup = backup_dir.join(format!("backup-{}-{counter:016x}.rs", std::process::id()));
    {
        let mut stage = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup)?;
        stage.write_all(expected.as_bytes())?;
        stage.sync_all()?;
    }
    // A synced file is not enough if its containing directory is lost on
    // abrupt shutdown. Require the private backup directory to be durable
    // on macOS before beginning the non-atomic in-place write.
    #[cfg(target_os = "macos")]
    File::open(&backup_dir)?.sync_all()?;
    // Keep the backup when the write fails, leaving a recovery option.
    file.seek(SeekFrom::Start(0))?;
    let commit = (|| {
        file.write_all(content.as_bytes())?;
        file.set_len(content.len() as u64)?;
        file.sync_all()
    })();
    if let Err(error) = commit {
        let _ = file.seek(SeekFrom::Start(0));
        let restored = file
            .write_all(expected.as_bytes())
            .and_then(|()| file.set_len(expected.len() as u64))
            .and_then(|()| file.sync_all());
        if restored.is_err() {
            return Err(io::Error::other(format!(
                "Save failed and rollback could not be verified: {error}. Backup: {}",
                backup.display()
            )));
        }
        return Err(error);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        let current = fs::symlink_metadata(path)?;
        if current.dev() != metadata.dev() || current.ino() != metadata.ino() {
            // An editor atomically replaced the path after the pre-commit
            // check. Never report success against the detached old inode.
            // Keep the rollback snapshot for manual recovery/audit.
            return Err(conflict(
                "Source path changed during in-place save; external replacement preserved. A private rollback copy was retained.",
            ));
        }
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // Do not discard the only independent rollback copy until the source
    // filesystem acknowledges the post-commit directory durability barrier.
    match sync_directory(parent) {
        Ok(()) => {
            if let Err(error) = fs::remove_file(&backup) {
                // Do not misreport a successful committed source update as a
                // failed save just because an optional backup cleanup failed.
                eprintln!(
                    "RustBridge private save backup retained at {}: {error}",
                    backup.display()
                );
            }
            Ok(SaveOutcome::Durable)
        }
        Err(error) => Ok(SaveOutcome::CommittedDurabilityUncertain(format!(
            "{error}; private rollback retained at {}",
            backup.display(),
        ))),
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn linux_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| conflict("Path contains a NUL byte"))
}

/// Fail closed on metadata enumeration errors; do not silently strip security labels/ACLs.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn snapshot_xattrs(path: &Path) -> io::Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let cpath = linux_path(path)?;
    let size = unsafe { libc::llistxattr(cpath.as_ptr(), std::ptr::null_mut(), 0) };
    if size < 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOTSUP) {
            return Ok(Vec::new());
        }
        return Err(error);
    }
    let mut names = vec![0u8; size as usize];
    let filled =
        unsafe { libc::llistxattr(cpath.as_ptr(), names.as_mut_ptr().cast(), names.len()) };
    if filled < 0 {
        return Err(io::Error::last_os_error());
    }
    names.truncate(filled as usize);
    let mut attrs = Vec::new();
    for name in names.split(|&b| b == 0).filter(|name| !name.is_empty()) {
        let key = CString::new(name).map_err(|_| conflict("Invalid xattr name"))?;
        let size =
            unsafe { libc::lgetxattr(cpath.as_ptr(), key.as_ptr(), std::ptr::null_mut(), 0) };
        if size < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut value = vec![0u8; size as usize];
        let filled = unsafe {
            libc::lgetxattr(
                cpath.as_ptr(),
                key.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
            )
        };
        if filled < 0 {
            return Err(io::Error::last_os_error());
        }
        value.truncate(filled as usize);
        attrs.push((name.to_vec(), value));
    }
    attrs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(attrs)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn copy_xattrs(path: &Path, attrs: &[(Vec<u8>, Vec<u8>)]) -> io::Result<()> {
    let cpath = linux_path(path)?;
    for (name, value) in attrs {
        let key = CString::new(name.as_slice()).map_err(|_| conflict("Invalid xattr name"))?;
        let rc = unsafe {
            libc::lsetxattr(
                cpath.as_ptr(),
                key.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    fn fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rustbridge-safe-v4-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[cfg(unix)]
    #[test]
    fn subprocess_lock_helper() {
        let Ok(path) = std::env::var("RUSTBRIDGE_TEST_LOCK_PATH") else {
            return;
        };
        let Ok(marker) = std::env::var("RUSTBRIDGE_TEST_LOCK_MARKER") else {
            return;
        };
        let _lock = lock_concurrent_rustbridge_saves(Path::new(&path)).unwrap();
        fs::write(marker, "LOCKED").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1400));
    }

    #[cfg(unix)]
    #[test]
    fn separate_process_cannot_commit_while_rustbridge_holds_file_lock() {
        use std::process::Command;
        let dir = fixture();
        let path = dir.join("shared.rs");
        let marker = dir.join("lock-ready");
        fs::write(&path, "base").unwrap();
        let executable = std::env::current_exe().unwrap();
        let mut child = Command::new(executable)
            .arg("--exact")
            .arg("safe_save::tests::subprocess_lock_helper")
            .arg("--nocapture")
            .env("RUSTBRIDGE_TEST_LOCK_PATH", &path)
            .env("RUSTBRIDGE_TEST_LOCK_MARKER", &marker)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "lock holder did not start"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            save_if_unchanged(&path, "base", "parent write").is_err(),
            "a second process must not commit while the OS lock is held"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "base");
        assert!(child.wait().unwrap().success());
        assert_eq!(
            save_if_unchanged(&path, "base", "after release").unwrap(),
            SaveOutcome::Durable
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "after release");
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_exclusive_handle_blocks_second_rustbridge_save() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = fixture();
        let path = dir.join("shared.rs");
        fs::write(&path, "base").unwrap();
        let _guard = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(save_if_unchanged(&path, "base", "blocked").is_err());
        drop(_guard);
        assert_eq!(
            save_if_unchanged(&path, "base", "after release").unwrap(),
            SaveOutcome::Durable
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "after release");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn atomic_save_and_rollback() {
        let dir = fixture();
        let path = dir.join("code.rs");
        fs::write(&path, "before").unwrap();
        assert_eq!(
            save_if_unchanged(&path, "before", "新代码 🦀").unwrap(),
            SaveOutcome::Durable
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "新代码 🦀");
        assert!(
            save_with_hooks(
                &path,
                "新代码 🦀",
                "other",
                || Err(io::Error::other("injected")),
                sync_parent
            )
            .is_err()
        );
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), "新代码 🦀");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn post_commit_sync_failure_is_reported_as_committed_not_retryable() {
        let dir = fixture();
        let path = dir.join("code.rs");
        fs::write(&path, "before").unwrap();
        let status = save_with_hooks(
            &path,
            "before",
            "after",
            || Ok(()),
            |_| Err(io::Error::other("injected fsync error")),
        )
        .unwrap();
        assert!(matches!(
            status,
            SaveOutcome::CommittedDurabilityUncertain(_)
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), "after");
        assert!(save_if_unchanged(&path, "before", "stale").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn read_only_and_external_edits_are_rejected() {
        let dir = fixture();
        let path = dir.join("code.rs");
        fs::write(&path, "original").unwrap();
        let mut p = fs::metadata(&path).unwrap().permissions();
        p.set_readonly(true);
        fs::set_permissions(&path, p).unwrap();
        assert!(save_if_unchanged(&path, "original", "replacement").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        #[cfg(not(unix))]
        {
            let mut p = fs::metadata(&path).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            p.set_readonly(false);
            fs::set_permissions(&path, p).unwrap();
        }
        fs::write(&path, "external").unwrap();
        assert!(save_if_unchanged(&path, "original", "replacement").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn original_xattrs_and_file_mode_are_preserved() {
        use std::os::unix::fs::PermissionsExt;
        let dir = fixture();
        let path = dir.join("code.rs");
        fs::write(&path, "before").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let attrs = vec![(b"user.rustbridge_review".to_vec(), b"important".to_vec())];
        match copy_xattrs(&path, &attrs) {
            Ok(()) => {
                save_if_unchanged(&path, "before", "after").unwrap();
                assert_eq!(snapshot_xattrs(&path).unwrap(), attrs);
                assert_eq!(
                    fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o640
                );
            }
            Err(e) if e.raw_os_error() == Some(libc::ENOTSUP) => {}
            Err(e) => panic!("{e}"),
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlink_and_hardlink_rejected() {
        use std::os::unix::fs::symlink;
        let dir = fixture();
        let path = dir.join("code.rs");
        fs::write(&path, "before").unwrap();
        symlink(&path, dir.join("link.rs")).unwrap();
        assert!(save_if_unchanged(&dir.join("link.rs"), "before", "after").is_err());
        fs::hard_link(&path, dir.join("hard.rs")).unwrap();
        assert!(save_if_unchanged(&path, "before", "after").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn atomic_external_replacement_is_never_written_through_a_stale_handle() {
        let dir = fixture();
        let path = dir.join("code.rs");
        let external = dir.join("external.rs");
        fs::write(&path, "before").unwrap();
        fs::write(&external, "external winner").unwrap();
        assert!(
            save_with_hooks(
                &path,
                "before",
                "mine",
                || fs::rename(&external, &path),
                sync_parent
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "external winner");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn racing_writer_before_commit_is_rejected() {
        let dir = fixture();
        let path = dir.join("code.rs");
        fs::write(&path, "before").unwrap();
        assert!(
            save_with_hooks(
                &path,
                "before",
                "mine",
                || fs::write(&path, "other"),
                sync_parent
            )
            .is_err()
        );
        let final_content = fs::read_to_string(&path).unwrap();
        assert_ne!(
            final_content, "mine",
            "conflicting save may never silently commit"
        );
        #[cfg(unix)]
        assert_eq!(final_content, "other");
        #[cfg(windows)]
        assert_eq!(final_content, "before");
        fs::remove_dir_all(dir).unwrap();
    }
}
