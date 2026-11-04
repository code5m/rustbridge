//! File I/O occurs on one bounded background worker, never inside an egui draw callback.
//! Open requests are supersedable; saves are explicit, serialized and never silently cancelled.
use crate::{
    file_watch::{FileStamp, file_stamp},
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
};
pub const MAX_DOCUMENT_BYTES: u64 = 32 * 1024 * 1024;

pub struct Loaded {
    pub source: String,
    pub stamp: Option<FileStamp>,
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
    },
}
enum Job {
    Open {
        id: u64,
        path: PathBuf,
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
        let work = Arc::clone(&shared);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let job = {
                    let mut state = work.state.lock().unwrap();
                    while state.queue.is_empty() && !state.closed {
                        state = work.cv.wait(state).unwrap();
                    }
                    if state.closed && state.queue.is_empty() {
                        return;
                    }
                    state.queue.pop_front().unwrap()
                };
                let reply = match job {
                    Job::Open { id, path } => {
                        if work.latest_open.load(Ordering::Acquire) != id {
                            continue;
                        }
                        let result =
                            read_capped(&path, || work.latest_open.load(Ordering::Relaxed) != id);
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
                        Reply::Save {
                            id,
                            path,
                            content,
                            result,
                        }
                    }
                };
                if sender.send(reply).is_err() {
                    return;
                }
            }
        });
        Self { shared, receiver }
    }
    pub fn open(&self, id: u64, path: PathBuf) {
        let mut state = self.shared.state.lock().unwrap();
        self.shared.latest_open.store(id, Ordering::Release);
        state.queue.retain(|job| !matches!(job, Job::Open { .. }));
        state.queue.push_back(Job::Open { id, path });
        self.shared.cv.notify_one();
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
        self.shared.cv.notify_one();
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
        self.shared.cv.notify_one();
    }
}

fn read_capped(path: &Path, cancelled: impl Fn() -> bool) -> io::Result<Loaded> {
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
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
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
            if let Some(Reply::Save { result, .. }) = worker.take() {
                assert_eq!(result.unwrap(), SaveOutcome::Durable);
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(4));
        }
        assert_eq!(fs::read_to_string(&b).unwrap(), "fn changed() {}");
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
        let result = read_capped(&path, || false);
        assert!(result.is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        fs::remove_dir_all(folder).unwrap();
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
        assert!(read_capped(&path, || false).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
