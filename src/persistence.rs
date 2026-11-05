//! Private crash-recovery journals and small restartable session metadata.
//! No Rust source is transmitted; journal data is stored only on the user's disk.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    hash::{Hash, Hasher},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc::{self, Sender},
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
        serde_json::from_slice(&fs::read(self.dir.join("session.json")).ok()?).ok()
    }
    pub fn save_session(&self, session: &Session) -> io::Result<()> {
        let data = serde_json::to_vec(session).map_err(io::Error::other)?;
        self.atomic_write(&self.dir.join("session.json"), &data)
    }
    pub fn load_journal(&self, source: &Path) -> Option<Journal> {
        let path = self.journal_path(source);
        let bytes = fs::read(path).ok()?;
        if bytes.len() > MAX_JOURNAL_BYTES {
            return None;
        }
        let journal: Journal = serde_json::from_slice(&bytes).ok()?;
        if journal.path == source {
            Some(journal)
        } else {
            None
        }
    }

    /// Load every valid journal whose source still exists. This also recovers
    /// tabs if a crash happened before the latest session tab list was flushed.
    pub fn load_all_journals(&self) -> Vec<Journal> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                let name = path.file_name()?.to_string_lossy();
                if !name.starts_with("recover-") || !name.ends_with(".json") {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                if metadata.len() as usize > MAX_JOURNAL_BYTES {
                    return None;
                }
                let bytes = fs::read(&path).ok()?;
                let journal: Journal = serde_json::from_slice(&bytes).ok()?;
                if !journal.path.is_file() || self.journal_path(&journal.path) != path {
                    return None;
                }
                Some(journal)
            })
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
pub struct PersistenceWorker {
    sender: Option<Sender<Job>>,
    handle: Option<thread::JoinHandle<()>>,
}
impl PersistenceWorker {
    pub fn spawn(store: Store) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let handle = thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                match job {
                    Job::SaveJournal {
                        path,
                        baseline,
                        edited,
                    } => {
                        if let Err(e) = store.save_journal(&path, &baseline, &edited) {
                            eprintln!("RustBridge recovery journal: {e}");
                        }
                    }
                    Job::DeleteJournal(path) => {
                        let _ = store.delete_journal(&path);
                    }
                    Job::Session(session) => {
                        let _ = store.save_session(&session);
                    }
                }
            }
        });
        Self {
            sender: Some(sender),
            handle: Some(handle),
        }
    }
    pub fn send(&self, job: Job) {
        if let Some(tx) = self.sender.as_ref() {
            let _ = tx.send(job);
        }
    }
}
impl Drop for PersistenceWorker {
    fn drop(&mut self) {
        // Normal exit completes accepted journal operations, while abnormal
        // process termination retains the last durably committed journal.
        self.sender.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
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
