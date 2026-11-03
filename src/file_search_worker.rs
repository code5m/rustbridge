//! A cancellable, bounded filename search worker for real Rust project trees.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};

#[derive(Default)]
struct State {
    pending: Option<(u64, Request)>,
    result: Option<(u64, Vec<PathBuf>)>,
    closed: bool,
}

struct Request {
    root: PathBuf,
    query: String,
    show_excluded: bool,
}

pub struct FileSearchWorker {
    shared: Arc<(Mutex<State>, Condvar)>,
}

impl FileSearchWorker {
    pub fn new() -> Self {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker_shared = Arc::clone(&shared);
        std::thread::spawn(move || {
            let (lock, ready) = &*worker_shared;
            loop {
                let mut state = lock.lock().unwrap();
                while state.pending.is_none() && !state.closed {
                    state = ready.wait(state).unwrap();
                }
                if state.closed {
                    break;
                }
                let (revision, request) = state.pending.take().unwrap();
                drop(state);
                let results = search(&request, || {
                    let state = lock.lock().unwrap();
                    state.closed
                        || state
                            .pending
                            .as_ref()
                            .is_some_and(|(next, _)| *next > revision)
                });
                let mut state = lock.lock().unwrap();
                if state.closed {
                    break;
                }
                if state
                    .pending
                    .as_ref()
                    .is_none_or(|(next, _)| *next <= revision)
                {
                    state.result = Some((revision, results));
                }
            }
        });
        Self { shared }
    }

    pub fn submit(&self, revision: u64, root: PathBuf, query: String, show_excluded: bool) {
        let (lock, ready) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.pending = Some((
            revision,
            Request {
                root,
                query,
                show_excluded,
            },
        ));
        state.result = None;
        ready.notify_one();
    }

    pub fn take(&self) -> Option<(u64, Vec<PathBuf>)> {
        self.shared.0.lock().unwrap().result.take()
    }
}

impl Drop for FileSearchWorker {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.closed = true;
        state.pending = None;
        ready.notify_one();
    }
}

fn search(request: &Request, is_cancelled: impl Fn() -> bool) -> Vec<PathBuf> {
    let mut results = Vec::new();
    let mut pending = VecDeque::from([(request.root.clone(), 0)]);
    let query = request.query.to_lowercase();
    while let Some((directory, depth)) = pending.pop_front() {
        if is_cancelled() || depth > 12 || results.len() >= 100 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if is_cancelled() || results.len() >= 100 {
                break;
            }
            let path = entry.path();
            let name = entry.file_name();
            let excluded = matches!(name.to_str(), Some(".git" | "target" | "node_modules"));
            if path.is_dir() {
                if request.show_excluded || !excluded {
                    pending.push_back((path, depth + 1));
                }
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && name.to_string_lossy().to_lowercase().contains(&query)
            {
                results.push(path);
            }
        }
    }
    results.sort();
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn search_skips_excluded_directories_unless_requested() {
        let root = std::env::temp_dir().join(format!(
            "rustbridge-worker-exclusion-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("src/demo.rs"), "fn demo() {}").unwrap();
        std::fs::write(root.join("target/generated.rs"), "fn generated() {}").unwrap();
        let request = Request {
            root: root.clone(),
            query: "demo".into(),
            show_excluded: false,
        };
        assert_eq!(search(&request, || false), vec![root.join("src/demo.rs")]);
        let request = Request {
            root: root.clone(),
            query: "generated".into(),
            show_excluded: true,
        };
        assert_eq!(
            search(&request, || false),
            vec![root.join("target/generated.rs")]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn newest_request_wins() {
        let root =
            std::env::temp_dir().join(format!("rustbridge-worker-newest-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("latest.rs"), "fn latest() {}").unwrap();
        let worker = FileSearchWorker::new();
        worker.submit(1, root.clone(), "missing".into(), false);
        worker.submit(2, root.clone(), "latest".into(), false);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some((revision, results)) = worker.take() {
                assert_eq!(revision, 2);
                assert_eq!(results, vec![root.join("latest.rs")]);
                break;
            }
            assert!(Instant::now() < deadline, "worker did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
