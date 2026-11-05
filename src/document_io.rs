//! Source opens and saves run on two separately bounded background workers, never in egui.
//! Open requests are supersedable; saves are explicit, serialized and never silently cancelled.
use crate::{
    file_watch::{FileStamp, file_stamp},
    persistence::{Journal, Store},
    safe_save::{SaveOutcome, save_if_unchanged},
};
#[cfg(test)]
use std::fs;
use std::{
    collections::VecDeque,
    fs::OpenOptions,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};
pub const MAX_DOCUMENT_BYTES: u64 = 32 * 1024 * 1024;

pub struct Loaded {
    pub source: String,
    pub stamp: Option<FileStamp>,
    /// Loaded in the background together with source bytes, never on the GUI.
    pub recovery: Option<Journal>,
}
pub enum Reply {
    Open {
        id: u64,
        path: PathBuf,
        result: io::Result<Loaded>,
    },
    Save {
        id: u64,
        path: PathBuf,
        content: String,
        result: io::Result<SaveOutcome>,
        /// Captured by the background I/O worker after commit. The egui thread
        /// must never call filesystem metadata APIs to finish a save.
        stamp: Option<FileStamp>,
    },
}
enum Job {
    Open {
        id: u64,
        path: PathBuf,
        store: Option<Store>,
    },
    Save {
        id: u64,
        path: PathBuf,
        expected: String,
        content: String,
    },
}
#[derive(Default)]
struct State {
    queue: VecDeque<Job>,
    closed: bool,
}
struct Shared {
    state: Mutex<State>,
    cv: Condvar,
    latest_open: AtomicU64,
}
pub struct DocumentIo {
    shared: Arc<Shared>,
    receiver: Receiver<Reply>,
}
impl DocumentIo {
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            cv: Condvar::new(),
            latest_open: AtomicU64::new(0),
        });
        let (sender, receiver) = mpsc::channel();
        // A hung remote filesystem read cannot prevent an unrelated explicit
        // source save from progressing: two fixed workers handle separate queues.
        for open_role in [true, false] {
            let work = Arc::clone(&shared);
            let sender = sender.clone();
            thread::spawn(move || {
                loop {
                    let job = {
                        let mut state = work.state.lock().unwrap();
                        loop {
                            let index = state
                                .queue
                                .iter()
                                .position(|job| matches!(job, Job::Open { .. }) == open_role);
                            if let Some(index) = index {
                                break state.queue.remove(index);
                            }
                            if state.closed {
                                break None;
                            }
                            state = work.cv.wait(state).unwrap();
                        }
                    };
                    let Some(job) = job else {
                        return;
                    };
                    let reply = match job {
                        Job::Open { id, path, store } => {
                            if work.latest_open.load(Ordering::Acquire) != id {
                                continue;
                            }
                            let mut result = read_capped(
                                &path,
                                || work.latest_open.load(Ordering::Relaxed) != id,
                                Duration::from_secs(30),
                            );
                            if work.latest_open.load(Ordering::Acquire) != id {
                                continue;
                            }
                            if let (Some(store), Ok(loaded)) = (store, &mut result) {
                                loaded.recovery = store.load_journal(&path);
                            }
                            if work.latest_open.load(Ordering::Acquire) != id {
                                continue;
                            }
                            Reply::Open { id, path, result }
                        }
                        Job::Save {
                            id,
                            path,
                            expected,
                            content,
                        } => {
                            let result = save_if_unchanged(&path, &expected, &content);
                            // A slow network mount must never stall the GUI's
                            // save-completion callback just to stat the file.
                            let stamp = result.as_ref().ok().and_then(|_| file_stamp(&path).ok());
                            Reply::Save {
                                id,
                                path,
                                content,
                                result,
                                stamp,
                            }
                        }
                    };
                    if sender.send(reply).is_err() {
                        return;
                    }
                }
            });
        }
        Self { shared, receiver }
    }
    #[cfg(test)]
    pub fn open(&self, id: u64, path: PathBuf) {
        self.open_with_recovery(id, path, None);
    }

    pub fn open_with_recovery(&self, id: u64, path: PathBuf, store: Option<Store>) {
        let mut state = self.shared.state.lock().unwrap();
        self.shared.latest_open.store(id, Ordering::Release);
        state.queue.retain(|job| !matches!(job, Job::Open { .. }));
        state.queue.push_back(Job::Open { id, path, store });
        self.shared.cv.notify_all();
    }
    pub fn save(&self, id: u64, path: PathBuf, expected: String, content: String) {
        let mut state = self.shared.state.lock().unwrap();
        // Prioritize explicit save over yet-unstarted reads; never discard a pending save.
        let place = state
            .queue
            .iter()
            .position(|job| matches!(job, Job::Open { .. }))
            .unwrap_or(state.queue.len());
        state.queue.insert(
            place,
            Job::Save {
                id,
                path,
                expected,
                content,
            },
        );
        self.shared.cv.notify_all();
    }
    pub fn cancel_open(&self) {
        let mut state = self.shared.state.lock().unwrap();
        self.shared.latest_open.store(u64::MAX, Ordering::Release);
        state.queue.retain(|job| !matches!(job, Job::Open { .. }));
        self.shared.cv.notify_all();
    }
    pub fn take(&self) -> Option<Reply> {
        self.receiver.try_recv().ok()
    }
}
impl Default for DocumentIo {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for DocumentIo {
    fn drop(&mut self) {
        let mut s = self.shared.state.lock().unwrap();
        // Never drop/abort a previously accepted explicit save on shutdown:
        // complete queued saves, but cancel pending reads. The worker owns its Arc.
        s.queue.retain(|job| matches!(job, Job::Save { .. }));
        s.closed = true;
        self.shared.latest_open.store(u64::MAX, Ordering::Release);
        self.shared.cv.notify_all();
    }
}

fn read_capped(path: &Path, cancelled: impl Fn() -> bool, timeout: Duration) -> io::Result<Loaded> {
    let started = Instant::now();
    if path.extension().is_none_or(|ext| ext != "rs") {
        return Err(io::Error::other("Only real .rs files are supported."));
    }
    // On Unix a .rs-named FIFO could otherwise block open() forever on the
    // single I/O worker. O_NONBLOCK lets us inspect and reject nonregular files.
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::other(
            "Only regular Rust source files are supported.",
        ));
    }
    let size = metadata.len();
    if size > MAX_DOCUMENT_BYTES {
        return Err(io::Error::other(format!(
            "File is {size} bytes, above the {MAX_DOCUMENT_BYTES}-byte safety limit."
        )));
    }
    let mut buffer = Vec::with_capacity(size.min(1024 * 1024) as usize);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        if started.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Source read exceeded deadline; filesystem call cannot be interrupted.",
            ));
        }
        if cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Superseded open request",
            ));
        }
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        if buffer.len() + read > MAX_DOCUMENT_BYTES as usize {
            return Err(io::Error::other(
                "File grew past the safety limit while opening.",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let source = String::from_utf8(buffer)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(Loaded {
        source,
        stamp: file_stamp(path).ok(),
        recovery: None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn opening_a_source_also_loads_recovery_off_the_ui_thread() {
        let folder =
            std::env::temp_dir().join(format!("rustbridge-async-recovery-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let state = Store::new(folder.join("state")).unwrap();
        let path = folder.join("recovered.rs");
        fs::write(&path, "fn old() {}").unwrap();
        state
            .save_journal(&path, "fn old() {}", "fn changed() {}")
            .unwrap();
        let worker = DocumentIo::new();
        worker.open_with_recovery(1, path.clone(), Some(state));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(Reply::Open {
                path: name, result, ..
            }) = worker.take()
            {
                assert_eq!(name, path);
                let loaded = result.unwrap();
                assert_eq!(loaded.source, "fn old() {}");
                assert_eq!(loaded.recovery.unwrap().edited, "fn changed() {}");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "background recovery did not complete"
            );
            thread::sleep(Duration::from_millis(5));
        }
        drop(worker);
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn asynchronous_open_save_and_latest_revision() {
        let folder = std::env::temp_dir().join(format!("rustbridge-doc-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let a = folder.join("a.rs");
        let b = folder.join("b.rs");
        fs::write(&a, "fn first() {}").unwrap();
        fs::write(&b, "fn second() {}").unwrap();
        let worker = DocumentIo::new();
        worker.open(1, a.clone());
        worker.open(2, b.clone());
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(Reply::Open { id, result, .. }) = worker.take() {
                assert_eq!(id, 2);
                assert_eq!(result.unwrap().source, "fn second() {}");
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(4));
        }
        worker.save(
            3,
            b.clone(),
            "fn second() {}".into(),
            "fn changed() {}".into(),
        );
        loop {
            if let Some(Reply::Save { result, stamp, .. }) = worker.take() {
                assert_eq!(result.unwrap(), SaveOutcome::Durable);
                assert_eq!(
                    stamp,
                    Some(file_stamp(&b).unwrap()),
                    "the I/O worker must capture the saved file's metadata"
                );
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(4));
        }
        assert_eq!(fs::read_to_string(&b).unwrap(), "fn changed() {}");
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn cancelled_open_does_not_poison_future_open_requests() {
        let folder =
            std::env::temp_dir().join(format!("rustbridge-cancel-open-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let first = folder.join("first.rs");
        let second = folder.join("second.rs");
        fs::write(&first, "fn first() {}").unwrap();
        fs::write(&second, "fn second() {}").unwrap();
        let worker = DocumentIo::new();
        worker.open(1, first);
        worker.cancel_open();
        worker.open(2, second);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(Reply::Open { id, result, .. }) = worker.take() {
                assert_eq!(id, 2);
                assert_eq!(result.unwrap().source, "fn second() {}");
                break;
            }
            assert!(Instant::now() < deadline, "future open stayed cancelled");
            thread::sleep(Duration::from_millis(4));
        }
        drop(worker);
        fs::remove_dir_all(folder).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn fifo_disguised_as_rust_file_is_rejected_without_blocking() {
        use std::os::unix::ffi::OsStrExt;
        let folder = std::env::temp_dir().join(format!("rustbridge-fifo-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("pipe.rs");
        let raw = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(raw.as_ptr(), 0o600) }, 0);
        let start = std::time::Instant::now();
        let result = read_capped(&path, || false, Duration::from_secs(5));
        assert!(result.is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn read_deadline_is_enforced_even_if_source_is_regular() {
        let dir = std::env::temp_dir().join(format!("rustbridge-timeout-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("timeout.rs");
        fs::write(&path, vec![b'a'; 32 * 1024]).unwrap();
        assert!(read_capped(&path, || false, Duration::ZERO).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversized_file_rejected_before_read() {
        let dir = std::env::temp_dir().join(format!("rustbridge-max-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("oversized.rs");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_DOCUMENT_BYTES + 1)
            .unwrap();
        assert!(read_capped(&path, || false, Duration::from_secs(5)).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
