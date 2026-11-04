//! Lazy asynchronous directory index. egui never calls read_dir or sorts entries.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    RustFile,
    OtherFile,
    Symlink,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
}

#[derive(Clone)]
pub enum Snapshot {
    Loading,
    Ready(Arc<Vec<Entry>>),
    Error(Arc<str>),
}

#[derive(Default)]
struct State {
    scans: HashMap<PathBuf, (Snapshot, Instant)>,
    queue: VecDeque<PathBuf>,
    queued: HashSet<PathBuf>,
    closed: bool,
}
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
}

pub struct DirectoryIndex {
    shared: Arc<Shared>,
}

impl DirectoryIndex {
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        thread::spawn(move || {
            loop {
                let directory = {
                    let mut state = worker_shared.state.lock().unwrap();
                    while state.queue.is_empty() && !state.closed {
                        state = worker_shared.ready.wait(state).unwrap();
                    }
                    if state.closed {
                        return;
                    }
                    let dir = state.queue.pop_front().unwrap();
                    state.queued.remove(&dir);
                    dir
                };
                let snapshot = match read_directory(&directory) {
                    Ok(entries) => Snapshot::Ready(Arc::new(entries)),
                    Err(error) => Snapshot::Error(error.into()),
                };
                let mut state = worker_shared.state.lock().unwrap();
                if state.closed {
                    return;
                }
                state.scans.insert(directory, (snapshot, Instant::now()));
            }
        });
        Self { shared }
    }

    pub fn get_or_request(&self, directory: PathBuf) -> Snapshot {
        self.get_or_request_at(directory, Duration::from_secs(3))
    }

    fn get_or_request_at(&self, directory: PathBuf, max_age: Duration) -> Snapshot {
        let mut state = self.shared.state.lock().unwrap();
        let now = Instant::now();
        if let Some((snapshot, updated)) = state.scans.get(&directory) {
            let snapshot = snapshot.clone();
            if now.duration_since(*updated) >= max_age {
                // Serve the last complete snapshot while refreshing: no UI flicker.
                if state.queued.insert(directory.clone()) {
                    state.queue.push_back(directory.clone());
                    self.shared.ready.notify_one();
                }
                if let Some((_, last)) = state.scans.get_mut(&directory) {
                    *last = now;
                }
            }
            return snapshot;
        }
        state
            .scans
            .insert(directory.clone(), (Snapshot::Loading, now));
        state.queued.insert(directory.clone());
        state.queue.push_back(directory);
        self.shared.ready.notify_one();
        Snapshot::Loading
    }
}

impl Default for DirectoryIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DirectoryIndex {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.closed = true;
        state.queue.clear();
        self.shared.ready.notify_all();
    }
}

fn read_directory(directory: &PathBuf) -> Result<Vec<Entry>, String> {
    let entries = fs::read_dir(directory).map_err(|error| error.to_string())?;
    let mut result = Vec::new();
    for next in entries {
        let entry = next.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = match entry.file_type() {
            Ok(file_type) if file_type.is_symlink() => EntryKind::Symlink,
            Ok(file_type) if file_type.is_dir() => EntryKind::Directory,
            Ok(file_type) if file_type.is_file() && path.extension().is_some_and(|s| s == "rs") => {
                EntryKind::RustFile
            }
            Ok(_) => EntryKind::OtherFile,
            Err(error) => return Err(error.to_string()),
        };
        result.push(Entry { name, path, kind });
    }
    result.sort_by(|a, b| {
        let a_dir = a.kind == EntryKind::Directory;
        let b_dir = b.kind == EntryKind::Directory;
        b_dir
            .cmp(&a_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    static SERIAL: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn real_directory_entries_not_limited_to_180() {
        let root = std::env::temp_dir().join(format!(
            "rustbridge-tree-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        for i in 0..205 {
            fs::write(root.join(format!("{i:03}.rs")), "").unwrap();
        }
        fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        let worker = DirectoryIndex::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match worker.get_or_request(root.clone()) {
                Snapshot::Ready(entries) => {
                    assert_eq!(entries.len(), 206);
                    assert!(entries.iter().any(|e| e.name == "Cargo.toml"));
                    break;
                }
                Snapshot::Error(error) => panic!("{error}"),
                Snapshot::Loading => {
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn real_directory_refreshes_after_external_creation() {
        let root = std::env::temp_dir().join(format!(
            "rustbridge-tree-refresh-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.rs"), "").unwrap();
        let worker = DirectoryIndex::new();
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Snapshot::Ready(entries) =
                worker.get_or_request_at(root.clone(), Duration::from_millis(25))
                && entries.len() == 1
            {
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(5));
        }
        fs::write(root.join("b.rs"), "").unwrap();
        loop {
            if let Snapshot::Ready(entries) =
                worker.get_or_request_at(root.clone(), Duration::from_millis(25))
                && entries.len() == 2
            {
                break;
            }
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(5));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn directory_symlink_is_displayed_not_followed() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!(
            "rustbridge-tree-link-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        symlink(&root, root.join("cyclic")).unwrap();
        let entries = read_directory(&root).unwrap();
        assert_eq!(entries[0].kind, EntryKind::Symlink);
        fs::remove_dir_all(root).unwrap();
    }
}
