//! Private crash-recovery journals and small restartable session metadata.
//! No Rust source is transmitted; journal data is stored only on the user's disk.
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    hash::{Hash, Hasher},
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let from = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both paths are valid nul-terminated UTF-16 and remain alive for the call.
    let result = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
const MAX_JOURNAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_SESSION_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DocumentSession {
    pub path: PathBuf,
    pub cursor_byte: usize,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Session {
    pub root: Option<PathBuf>,
    /// Active document for backward compatibility with pre-UI-2.1 sessions.
    pub file: Option<PathBuf>,
    pub mode: String,
    pub lesson: usize,
    pub cursor_byte: usize,
    pub completed_lessons: Vec<bool>,
    /// Answer attempts (including errors) are locally persisted for targeted review.
    #[serde(default)]
    pub quiz_attempts: Vec<u32>,
    #[serde(default)]
    pub quiz_mistakes: Vec<u32>,
    /// Actual Cargo check results for the twelve editable exercises.
    #[serde(default)]
    pub practice_passed: Vec<bool>,
    /// User-authored scratchpad for each exercise; never treated as main project source.
    #[serde(default)]
    pub practice_drafts: Vec<String>,
    /// Open tabs in visual order. Old session files deserialize with an empty list.
    #[serde(default)]
    pub documents: Vec<DocumentSession>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Journal {
    pub path: PathBuf,
    /// Saved disk content at the time the edits started; exact comparison avoids
    /// accepting an unrelated external change or a hash collision.
    pub baseline: String,
    pub edited: String,
    pub captured_unix_seconds: u64,
}

/// Cheap, complete startup index. Never retains a journal's large source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryEntry {
    pub path: PathBuf,
    pub source_exists: bool,
    pub captured_unix_seconds: u64,
}

#[derive(Clone, Debug, Default)]
pub struct RecoveryInventory {
    pub entries: Vec<RecoveryEntry>,
    pub invalid: usize,
    pub oversized: usize,
    pub unreadable: usize,
    pub unreadable_session: bool,
}

impl RecoveryInventory {
    pub fn warning(&self) -> Option<String> {
        let failures = self.invalid + self.oversized + self.unreadable;
        (failures != 0 || self.unreadable_session).then(|| format!(
            "恢复扫描：{} 个记录可用，{} 个损坏或不匹配，{} 个超过单文件限制，{} 个不可读取。{}             原记录未被删除，可人工检查状态目录。",
            self.entries.len(), self.invalid, self.oversized, self.unreadable,
            if self.unreadable_session {
                "历史会话文件无法解析或超过限制；下一次写入前先备份旧文件。"
            } else {
                ""
            },
        ))
    }
}

#[derive(Deserialize)]
struct RecoveryHeader {
    path: PathBuf,
    #[serde(default)]
    captured_unix_seconds: u64,
    /// Validate required fields without allocating massive editor strings.
    #[serde(rename = "baseline")]
    _baseline: serde::de::IgnoredAny,
    #[serde(rename = "edited")]
    _edited: serde::de::IgnoredAny,
}

#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
}
impl Store {
    pub fn from_system() -> io::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let dir = if let Some(folder) = std::env::var_os("RUSTBRIDGE_STATE_DIR") {
            PathBuf::from(folder)
        } else if let Some(folder) = std::env::var_os("XDG_STATE_HOME") {
            PathBuf::from(folder).join("rustbridge")
        } else if cfg!(target_os = "windows") {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join("RustBridge")
        } else if cfg!(target_os = "macos") {
            home.ok_or_else(|| io::Error::other("HOME not available"))?
                .join("Library/Application Support/RustBridge")
        } else {
            home.ok_or_else(|| io::Error::other("HOME not available"))?
                .join(".local/state/rustbridge")
        };
        Self::new(dir)
    }
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let metadata = fs::symlink_metadata(&dir)?;
        if !metadata.file_type().is_dir() {
            return Err(io::Error::other(
                "Private state directory must be a real directory, not a symlink",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { dir })
    }
    pub fn directory(&self) -> &Path {
        &self.dir
    }
    fn journal_path(&self, source: &Path) -> PathBuf {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        self.dir
            .join(format!("recover-{:016x}.json", hasher.finish()))
    }
    pub fn load_session(&self) -> Option<Session> {
        let path = self.dir.join("session.json");
        let meta = fs::symlink_metadata(&path).ok()?;
        if !meta.file_type().is_file() || meta.len() > MAX_SESSION_BYTES {
            return None;
        }
        serde_json::from_reader(BufReader::new(File::open(path).ok()?)).ok()
    }
    pub fn save_session(&self, session: &Session) -> io::Result<()> {
        let data = serde_json::to_vec(session).map_err(io::Error::other)?;
        if data.len() as u64 > MAX_SESSION_BYTES {
            return Err(io::Error::other(
                "Workspace session exceeds the 8 MiB safety limit",
            ));
        }
        let path = self.dir.join("session.json");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.file_type().is_file() {
                return Err(io::Error::other("Workspace session is not a regular file"));
            }
            if self.load_session().is_none() {
                // Preserve corrupted or future-version learning progress
                // under a private unique name before writing new state.
                let mut moved = false;
                for _ in 0..64 {
                    let serial = SEQUENCE.fetch_add(1, Ordering::Relaxed);
                    let backup = self.dir.join(format!(
                        "session-unreadable-{}-{serial:016x}.json",
                        std::process::id()
                    ));
                    if backup.exists() {
                        continue;
                    }
                    fs::rename(&path, &backup)?;
                    #[cfg(unix)]
                    File::open(&self.dir)?.sync_all()?;
                    moved = true;
                    break;
                }
                if !moved {
                    return Err(io::Error::other("No unique session backup path available"));
                }
            }
        }
        self.atomic_write(&path, &data)
    }
    pub fn load_journal(&self, source: &Path) -> Option<Journal> {
        let path = self.journal_path(source);
        let metadata = fs::symlink_metadata(&path).ok()?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_JOURNAL_BYTES as u64 {
            return None;
        }
        let file = File::open(path).ok()?;
        let journal: Journal = serde_json::from_reader(BufReader::new(file)).ok()?;
        (journal.path == source).then_some(journal)
    }

    /// Inventories *every* recovery record without loading up to 64 MiB of
    /// editor text per entry. Used only on a background startup worker.
    /// Oversized, invalid and unreadable entries are counted rather than
    /// silently dropped after a directory-entry or global-byte cutoff.
    pub fn scan_journals(&self) -> RecoveryInventory {
        let mut inventory = RecoveryInventory {
            unreadable_session: self.dir.join("session.json").exists()
                && self.load_session().is_none(),
            ..RecoveryInventory::default()
        };
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    inventory.unreadable += 1;
                }
                return inventory;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    inventory.unreadable += 1;
                    continue;
                }
            };
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with("recover-") || !name.ends_with(".json") {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    inventory.unreadable += 1;
                    continue;
                }
            };
            if !metadata.file_type().is_file() {
                inventory.invalid += 1;
                continue;
            }
            if metadata.len() > MAX_JOURNAL_BYTES as u64 {
                inventory.oversized += 1;
                continue;
            }
            let file = match File::open(&path) {
                Ok(file) => file,
                Err(_) => {
                    inventory.unreadable += 1;
                    continue;
                }
            };
            let header: RecoveryHeader = match serde_json::from_reader(BufReader::new(file)) {
                Ok(header) => header,
                Err(_) => {
                    inventory.invalid += 1;
                    continue;
                }
            };
            if self.journal_path(&header.path) != path {
                inventory.invalid += 1;
                continue;
            }
            inventory.entries.push(RecoveryEntry {
                source_exists: header.path.is_file(),
                path: header.path,
                captured_unix_seconds: header.captured_unix_seconds,
            });
        }
        inventory
            .entries
            .sort_unstable_by(|a, b| a.path.cmp(&b.path));
        inventory
    }

    /// Backwards-compatible full-journal API for unit tests and recovery
    /// exports. Production startup uses scan_journals() so it stays bounded.
    #[cfg(test)]
    pub fn load_all_journals(&self) -> Vec<Journal> {
        self.scan_journals()
            .entries
            .into_iter()
            .filter_map(|entry| self.load_journal(&entry.path))
            .collect()
    }

    pub fn save_journal(&self, source: &Path, baseline: &str, edited: &str) -> io::Result<()> {
        if baseline == edited {
            return self.delete_journal(source);
        }
        let captured_unix_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let journal = Journal {
            path: source.to_path_buf(),
            baseline: baseline.into(),
            edited: edited.into(),
            captured_unix_seconds,
        };
        let data = serde_json::to_vec(&journal).map_err(io::Error::other)?;
        if data.len() > MAX_JOURNAL_BYTES {
            return Err(io::Error::other("Recovery journal exceeds 64 MiB limit"));
        }
        self.atomic_write(&self.journal_path(source), &data)
    }
    pub fn delete_journal(&self, source: &Path) -> io::Result<()> {
        let path = self.journal_path(source);
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
    pub fn export_recovery(&self, journal: &Journal, destination: &Path) -> io::Result<()> {
        // Never silently replace a user's file with an old recovery buffer.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        file.write_all(journal.edited.as_bytes())?;
        file.sync_all()
    }
    fn atomic_write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let stage = self.dir.join(format!(
            ".rustbridge-stage-{}-{id:016x}",
            std::process::id()
        ));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&stage)?;
            file.write_all(data)?;
            file.sync_all()?;
            #[cfg(windows)]
            atomic_replace(&stage, path)?;
            #[cfg(not(windows))]
            fs::rename(&stage, path)?;
            #[cfg(unix)]
            fs::File::open(&self.dir)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&stage);
        }
        result
    }
}
pub enum Job {
    SaveJournal {
        path: PathBuf,
        baseline: String,
        edited: String,
    },
    DeleteJournal(PathBuf),
    Session(Session),
}
// Only the most recent update for each document and the latest session are queued.
// This bounds memory to the number of open documents, not the number of edits.
// Operations for a document remain ordered (including deletion after save).
#[derive(Default)]
struct PersistenceQueue {
    jobs: VecDeque<Job>,
    closing: bool,
}
pub struct PersistenceWorker {
    queue: Arc<(Mutex<PersistenceQueue>, Condvar)>,
    handle: Option<thread::JoinHandle<()>>,
}
impl PersistenceWorker {
    pub fn spawn(store: Store) -> Self {
        let queue = Arc::new((Mutex::new(PersistenceQueue::default()), Condvar::new()));
        let shared = Arc::clone(&queue);
        let handle = thread::spawn(move || {
            loop {
                let job = {
                    let (lock, wake) = &*shared;
                    let mut pending = lock.lock().unwrap();
                    while pending.jobs.is_empty() && !pending.closing {
                        pending = wake.wait(pending).unwrap();
                    }
                    if pending.closing && pending.jobs.is_empty() {
                        return;
                    }
                    pending.jobs.pop_front().expect("pending nonempty")
                };
                match job {
                    Job::SaveJournal {
                        path,
                        baseline,
                        edited,
                    } => {
                        if let Err(error) = store.save_journal(&path, &baseline, &edited) {
                            eprintln!(
                                "RustBridge recovery journal for {}: {error}",
                                path.display()
                            );
                        }
                    }
                    Job::DeleteJournal(path) => {
                        if let Err(error) = store.delete_journal(&path) {
                            eprintln!(
                                "RustBridge recovery removal for {}: {error}",
                                path.display()
                            );
                        }
                    }
                    Job::Session(session) => {
                        if let Err(error) = store.save_session(&session) {
                            eprintln!("RustBridge session persistence: {error}");
                        }
                    }
                }
            }
        });
        Self {
            queue,
            handle: Some(handle),
        }
    }
    pub fn send(&self, job: Job) {
        let (lock, wake) = &*self.queue;
        let mut pending = lock.lock().unwrap();
        if pending.closing {
            return;
        }
        let obsolete = pending.jobs.iter().position(|old| match (old, &job) {
            (Job::Session(_), Job::Session(_)) => true,
            (Job::SaveJournal { path: a, .. }, Job::SaveJournal { path: b, .. })
            | (Job::SaveJournal { path: a, .. }, Job::DeleteJournal(b))
            | (Job::DeleteJournal(a), Job::SaveJournal { path: b, .. })
            | (Job::DeleteJournal(a), Job::DeleteJournal(b)) => a == b,
            _ => false,
        });
        if let Some(index) = obsolete {
            pending.jobs.remove(index);
        }
        pending.jobs.push_back(job);
        wake.notify_one();
    }
}
impl Drop for PersistenceWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        lock.lock().unwrap().closing = true;
        wake.notify_all();
        if let Some(handle) = self.handle.take() {
            // A blocked network filesystem or storage driver may never
            // return from a write. Do not make window destruction wait forever.
            // The accepted journal worker is allowed a bounded grace period;
            // if still blocked, report the unconfirmed persistence rather
            // than falsely claiming that all queued state reached disk.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while !handle.is_finished() && std::time::Instant::now() < deadline {
                thread::sleep(std::time::Duration::from_millis(10));
            }
            if handle.is_finished() {
                if handle.join().is_err() {
                    eprintln!("RustBridge persistence worker panicked during shutdown");
                }
            } else {
                let (lock, _) = &*self.queue;
                let pending_count = lock.lock().map_or(0, |queue| queue.jobs.len());
                eprintln!(
                    "WARNING: RustBridge persistence flush still blocked at shutdown;                      {pending_count} queued operations plus an in-flight write are unconfirmed.                      The worker has been detached; inspect the private recovery directory on restart."
                );
                // Joining this thread would make the native GUI unkillable.
                // OS-uninterruptible I/O cannot be made durable by waiting.
                drop(handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Store {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let folder = std::env::temp_dir().join(format!(
            "rustbridge-recovery-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        Store::new(folder).unwrap()
    }
    #[cfg(unix)]
    #[test]
    fn state_directory_symlink_is_rejected_before_permissions_change() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = fixture();
        let sensitive = root.directory().join("existing");
        let redirected = root.directory().join("linked");
        fs::create_dir_all(&sensitive).unwrap();
        fs::set_permissions(&sensitive, fs::Permissions::from_mode(0o755)).unwrap();
        symlink(&sensitive, &redirected).unwrap();
        assert!(Store::new(redirected).is_err());
        assert_eq!(
            fs::metadata(&sensitive).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::remove_dir_all(root.directory()).unwrap();
    }

    #[test]
    fn unreadable_session_is_backed_up_before_new_state_is_committed() {
        let store = fixture();
        let path = store.directory().join("session.json");
        fs::write(&path, b"{broken legacy state").unwrap();
        assert!(store.load_session().is_none());
        let inventory = store.scan_journals();
        assert!(inventory.unreadable_session);
        assert!(inventory.warning().unwrap().contains("会话"));
        let new_session = Session {
            mode: "rust".into(),
            lesson: 2,
            ..Session::default()
        };
        store.save_session(&new_session).unwrap();
        assert_eq!(store.load_session(), Some(new_session));
        let copies: Vec<_> = fs::read_dir(store.directory())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("session-unreadable-")
            })
            .collect();
        assert_eq!(copies.len(), 1);
        assert_eq!(fs::read(copies[0].path()).unwrap(), b"{broken legacy state");
        fs::remove_dir_all(store.directory()).unwrap();
    }

    #[test]
    fn inventory_indexes_all_300_entries_without_loading_full_editor_text() {
        let store = fixture();
        for index in 0..300 {
            let source = store.directory().join(format!("source-{index:04}.rs"));
            let journal = Journal {
                path: source.clone(),
                baseline: "fn before() {}".into(),
                edited: "fn after() {}".into(),
                captured_unix_seconds: index,
            };
            fs::write(
                store.journal_path(&source),
                serde_json::to_vec(&journal).unwrap(),
            )
            .unwrap();
        }
        // Do not use global byte/trailing directory entry cutoffs. Report
        // individual damaged or too-large files without deleting any of them.
        fs::write(store.directory().join("recover-broken.json"), b"{ invalid").unwrap();
        let oversized = store.directory().join("recover-too-large.json");
        File::create(&oversized)
            .unwrap()
            .set_len(MAX_JOURNAL_BYTES as u64 + 1)
            .unwrap();

        let inventory = store.scan_journals();
        assert_eq!(inventory.entries.len(), 300);
        assert_eq!(inventory.invalid, 1);
        assert_eq!(inventory.oversized, 1);
        assert_eq!(inventory.unreadable, 0);
        assert!(inventory.entries.iter().all(|entry| !entry.source_exists));
        assert!(inventory.warning().unwrap().contains("300"));
        assert!(store.load_journal(&oversized).is_none());
        fs::remove_dir_all(store.directory()).unwrap();
    }

    #[test]
    fn inventory_does_not_accept_forged_journal_paths() {
        let store = fixture();
        let wrong = store.directory().join("recover-wrong.json");
        fs::write(
            &wrong,
            br#"{"path":"/tmp/wrong.rs","baseline":"a","edited":"b"}"#,
        )
        .unwrap();
        let inventory = store.scan_journals();
        assert!(inventory.entries.is_empty());
        assert_eq!(inventory.invalid, 1);
        fs::remove_dir_all(store.directory()).unwrap();
    }
    #[test]
    fn deleted_source_is_still_discoverable_as_exportable_recovery() {
        let store = fixture();
        let source = store.directory().join("deleted.rs");
        let export = store.directory().join("recovered.rs");
        fs::write(&source, "fn original() {}").unwrap();
        store
            .save_journal(&source, "fn original() {}", "fn edited() {}")
            .unwrap();
        fs::remove_file(&source).unwrap();
        let orphan = store
            .load_all_journals()
            .into_iter()
            .find(|journal| journal.path == source)
            .expect("deleted document recovery");
        store.export_recovery(&orphan, &export).unwrap();
        assert_eq!(fs::read_to_string(export).unwrap(), "fn edited() {}");
    }
    #[test]
    fn shutdown_does_not_hang_forever_on_uninterruptible_worker() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let queue = Arc::new((Mutex::new(PersistenceQueue::default()), Condvar::new()));
        let still_blocked = Arc::new(AtomicBool::new(true));
        let gate = Arc::clone(&still_blocked);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            while gate.load(Ordering::Acquire) {
                thread::sleep(std::time::Duration::from_millis(10));
            }
            let _ = done_tx.send(());
        });
        let worker = PersistenceWorker {
            queue,
            handle: Some(handle),
        };
        let start = std::time::Instant::now();
        drop(worker);
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
        still_blocked.store(false, Ordering::Release);
        done_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
    }

    #[test]
    fn persistence_queue_coalesces_many_pending_edit_snapshots() {
        let worker = PersistenceWorker::spawn(fixture());
        let path = PathBuf::from("/tmp/rustbridge-coalescing-test.rs");
        for i in 0..1000 {
            worker.send(Job::SaveJournal {
                path: path.clone(),
                baseline: String::from("original"),
                edited: format!("new version {i}"),
            });
        }
        let pending = worker.queue.0.lock().unwrap();
        assert!(
            pending.jobs.len() <= 1,
            "only latest revision should remain pending"
        );
    }
    #[test]
    fn journal_survives_a_simulated_crash_and_rejects_unrelated_sources() {
        let store = fixture();
        let original = store.directory().join("hello.rs");
        store
            .save_journal(&original, "fn main(){}", "fn main(){ // 中文🦀 }")
            .unwrap();
        let restarted = Store::new(store.directory().to_path_buf()).unwrap();
        let recovered = restarted.load_journal(&original).unwrap();
        assert_eq!(recovered.baseline, "fn main(){}");
        assert!(recovered.edited.contains("中文🦀"));
        assert!(
            restarted
                .load_journal(&original.with_file_name("other.rs"))
                .is_none()
        );
        store.delete_journal(&original).unwrap();
        assert!(store.load_journal(&original).is_none());
        fs::remove_dir_all(store.directory()).unwrap();
    }
    #[test]
    fn session_and_export_never_overwrite_user_data() {
        let store = fixture();
        let session = Session {
            mode: "compare".into(),
            lesson: 4,
            completed_lessons: vec![true, false],
            ..Session::default()
        };
        store.save_session(&session).unwrap();
        assert_eq!(store.load_session(), Some(session));
        let journal = Journal {
            path: PathBuf::from("/none"),
            baseline: "".into(),
            edited: "recovered".into(),
            captured_unix_seconds: 1,
        };
        let file = store.directory().join("restore.rs");
        store.export_recovery(&journal, &file).unwrap();
        assert!(store.export_recovery(&journal, &file).is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "recovered");
        fs::remove_dir_all(store.directory()).unwrap();
    }
    #[test]
    fn learning_drafts_roundtrip_and_old_sessions_upgrade() {
        let store = fixture();
        let old = r#"{"root":null,"file":null,"mode":"java","lesson":2,"cursor_byte":0,"completed_lessons":[true,false]}"#;
        let mut session: Session = serde_json::from_str(old).unwrap();
        assert!(session.practice_passed.is_empty() && session.practice_drafts.is_empty());
        session.quiz_attempts = vec![3];
        session.quiz_mistakes = vec![2];
        session.practice_passed = vec![true, false];
        session.practice_drafts = vec!["let x = 42;".into(), "".into()];
        store.save_session(&session).unwrap();
        assert_eq!(store.load_session(), Some(session));
        fs::remove_dir_all(store.directory()).unwrap();
    }

    #[test]
    fn session_roundtrips_open_documents_and_orphan_journals_are_discoverable() {
        let store = fixture();
        let a = store.directory().join("a.rs");
        let b = store.directory().join("b.rs");
        fs::write(&a, "fn a() {}").unwrap();
        fs::write(&b, "fn b() {}").unwrap();
        let session = Session {
            file: Some(b.clone()),
            documents: vec![
                DocumentSession {
                    path: a.clone(),
                    cursor_byte: 4,
                    mode: "rust".into(),
                },
                DocumentSession {
                    path: b.clone(),
                    cursor_byte: 7,
                    mode: "compare".into(),
                },
            ],
            ..Session::default()
        };
        store.save_session(&session).unwrap();
        assert_eq!(store.load_session(), Some(session));
        store
            .save_journal(&a, "fn a() {}", "fn a() { /* dirty */ }")
            .unwrap();
        let journals = store.load_all_journals();
        assert_eq!(journals.len(), 1);
        assert_eq!(journals[0].path, a);
        fs::remove_dir_all(store.directory()).unwrap();
    }

    #[test]
    fn atomic_session_and_recovery_journals_can_be_updated_repeatedly() {
        let store = fixture();
        let first = Session {
            mode: "java".into(),
            lesson: 1,
            ..Session::default()
        };
        let second = Session {
            mode: "rust".into(),
            lesson: 9,
            cursor_byte: 123,
            completed_lessons: vec![true; 12],
            ..Session::default()
        };
        store.save_session(&first).unwrap();
        store.save_session(&second).unwrap();
        assert_eq!(store.load_session(), Some(second));
        let source = store.directory().join("overwrite.rs");
        store
            .save_journal(&source, "disk", "first unsaved")
            .unwrap();
        store
            .save_journal(&source, "disk", "second unsaved")
            .unwrap();
        assert_eq!(
            store.load_journal(&source).unwrap().edited,
            "second unsaved"
        );
        fs::remove_dir_all(store.directory()).unwrap();
    }
    #[test]
    fn background_journal_is_flushed_on_normal_shutdown() {
        let store = fixture();
        let path = store.directory().join("pending.rs");
        let worker = PersistenceWorker::spawn(store.clone());
        worker.send(Job::SaveJournal {
            path: path.clone(),
            baseline: "old".into(),
            edited: "unsaved".into(),
        });
        drop(worker);
        assert_eq!(store.load_journal(&path).unwrap().edited, "unsaved");
        fs::remove_dir_all(store.directory()).unwrap();
    }
}
