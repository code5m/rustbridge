//! Atomic file replacement with conflict checks and cleanup on failure.
//!
//! Uncooperative writers can still race between the final comparison and rename.
//! We document this boundary rather than claim cross-process compare-and-swap.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(unix)]
use std::fs::File;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct PendingTemp(PathBuf);
impl Drop for PendingTemp {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

fn conflict(message: &str) -> io::Error {
    io::Error::other(message)
}

pub fn save_if_unchanged(path: &Path, expected: &str, content: &str) -> io::Result<()> {
    save_with_precommit(path, expected, content, || Ok(()))
}

// Separate pre-commit hook allows deterministic tests of errors and external races.
fn save_with_precommit(
    path: &Path,
    expected: &str,
    content: &str,
    before_commit: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let original_meta = fs::symlink_metadata(path)?;
    if original_meta.file_type().is_symlink() {
        return Err(conflict(
            "Refusing to replace a symlink. Open the real target file instead.",
        ));
    }
    if !original_meta.is_file() {
        return Err(conflict("Only regular Rust source files can be saved."));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if original_meta.nlink() > 1 {
            return Err(conflict(
                "Refusing atomic replacement of a multiply hard-linked file.",
            ));
        }
    }
    if fs::read_to_string(path)? != expected {
        return Err(conflict(
            "File changed externally; save blocked. Reload or preserve both versions.",
        ));
    }
    if expected == content {
        return Ok(());
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let (mut target, guard) = {
        let mut created = None;
        for _ in 0..32 {
            let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let candidate = parent.join(format!(
                ".rustbridge-save-{}-{:016x}",
                std::process::id(),
                serial
            ));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => {
                    created = Some((file, PendingTemp(candidate)));
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        created.ok_or_else(|| conflict("Could not allocate a unique temporary save file."))?
    };
    target.write_all(content.as_bytes())?;
    fs::set_permissions(&guard.0, original_meta.permissions())?;
    target.sync_all()?;

    before_commit()?;

    // Verify both content and inode identity immediately before atomic replacement.
    let current_meta = fs::symlink_metadata(path)?;
    if !current_meta.is_file()
        || current_meta.file_type().is_symlink()
        || !same_file(&original_meta, &current_meta)
        || fs::read_to_string(path)? != expected
    {
        return Err(conflict(
            "File changed during save; original file was not overwritten.",
        ));
    }
    fs::rename(&guard.0, path)?;
    // Successful rename means the file is atomically replaced; cleanup guard sees no temp file.
    // fsync parent metadata for durability (failure is reported, not silently swallowed).
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn same_file(previous: &fs::Metadata, current: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    previous.dev() == current.dev() && previous.ino() == current.ino()
}

#[cfg(not(unix))]
fn same_file(previous: &fs::Metadata, current: &fs::Metadata) -> bool {
    previous.len() == current.len() && previous.modified().ok() == current.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
    fn fixture() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rustbridge-safe-save-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn atomic_replace_preserves_content_and_permissions() {
        let root = fixture();
        let path = root.join("code.rs");
        fs::write(&path, "original").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        }
        save_if_unchanged(&path, "original", "新版本 🦀").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "新版本 🦀");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn injected_failure_leaves_source_and_cleans_temp_file() {
        let root = fixture();
        let path = root.join("source.rs");
        fs::write(&path, "before").unwrap();
        let error = save_with_precommit(&path, "before", "after", || {
            Err(io::Error::other("injected write failure"))
        })
        .unwrap_err();
        assert!(error.to_string().contains("injected"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "before");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_writer_between_staging_and_commit_is_detected() {
        let root = fixture();
        let path = root.join("source.rs");
        fs::write(&path, "before").unwrap();
        assert!(
            save_with_precommit(&path, "before", "mine", || {
                fs::write(&path, "other writer")
            })
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "other writer");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_edit_and_deletion_are_blocked() {
        let root = fixture();
        let path = root.join("source.rs");
        fs::write(&path, "external").unwrap();
        assert!(save_if_unchanged(&path, "old", "mine").is_err());
        fs::remove_file(&path).unwrap();
        assert!(save_if_unchanged(&path, "old", "mine").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_and_hard_link_replacement_are_rejected() {
        use std::os::unix::fs::symlink;
        let root = fixture();
        let path = root.join("actual.rs");
        let symlink_path = root.join("link.rs");
        let hard_path = root.join("hard.rs");
        fs::write(&path, "content").unwrap();
        symlink(&path, &symlink_path).unwrap();
        assert!(save_if_unchanged(&symlink_path, "content", "changed").is_err());
        fs::hard_link(&path, &hard_path).unwrap();
        assert!(save_if_unchanged(&path, "content", "changed").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "content");
        fs::remove_dir_all(root).unwrap();
    }
}
