//! A cancellable, bounded filename search worker for real Rust project trees.

use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};

#[derive(Default)]
struct State {
    pending: Option<(u64, Request)>,
    result: Option<(u64, Vec<PathBuf>)>,
    current_revision: Option<u64>,
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
                    state.closed || state.current_revision != Some(revision)
                });
                let mut state = lock.lock().unwrap();
                if state.closed {
                    break;
                }
                if state.current_revision == Some(revision) {
                    state.result = Some((revision, results));
                }
            }
        });
        Self { shared }
    }

    pub fn submit(&self, revision: u64, root: PathBuf, query: String, show_excluded: bool) {
        let (lock, ready) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.current_revision = Some(revision);
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

    pub fn cancel(&self) {
        let mut state = self.shared.0.lock().unwrap();
        state.current_revision = None;
        state.pending = None;
        state.result = None;
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
    let query = request.query.to_lowercase();
    let Ok(root) = std::fs::read_dir(&request.root) else {
        return results;
    };
    let mut stack = vec![(root, 0)];
    while let Some((entries, depth)) = stack.last_mut() {
        if is_cancelled() || results.len() >= 100 {
            break;
        }
        let Some(entry) = entries.next() else {
            stack.pop();
            continue;
        };
        let depth = *depth;
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let name = entry.file_name();
        let excluded = matches!(name.to_str(), Some(".git" | "target" | "node_modules"));
        // Never follow symlinks while scanning: avoid cycles and leaving the selected root.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if depth < 12
                && (request.show_excluded || !excluded)
                && let Ok(child) = std::fs::read_dir(&path)
            {
                stack.push((child, depth + 1));
            }
        } else if kind.is_file()
            && path.extension().is_some_and(|extension| extension == "rs")
            && name.to_string_lossy().to_lowercase().contains(&query)
        {
            results.push(path);
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

    #[test]
    fn cancelled_search_does_not_publish_and_worker_can_be_reused() {
        let root =
            std::env::temp_dir().join(format!("rustbridge-worker-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("new.rs"), "").unwrap();
        let worker = FileSearchWorker::new();
        worker.submit(1, root.clone(), "old".into(), false);
        worker.cancel();
        {
            let state = worker.shared.0.lock().unwrap();
            assert!(state.current_revision.is_none());
            assert!(state.pending.is_none());
            assert!(state.result.is_none());
        }
        worker.submit(2, root.clone(), "new".into(), false);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some((revision, results)) = worker.take() {
                assert_eq!(revision, 2);
                assert_eq!(results, vec![root.join("new.rs")]);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_respects_depth_and_can_stop_mid_scan() {
        use std::cell::Cell;
        let root =
            std::env::temp_dir().join(format!("rustbridge-worker-depth-{}", std::process::id()));
        let mut directory = root.clone();
        for depth in 0..=13 {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join(format!("match_{depth}.rs")), "").unwrap();
            directory.push("child");
        }
        let request = Request {
            root: root.clone(),
            query: "match".into(),
            show_excluded: false,
        };
        let results = search(&request, || false);
        assert_eq!(results.len(), 13);
        assert!(results.iter().any(|path| path.ends_with("match_12.rs")));
        assert!(!results.iter().any(|path| path.ends_with("match_13.rs")));
        let checks = Cell::new(0);
        let cancelled = search(&request, || {
            checks.set(checks.get() + 1);
            checks.get() > 3
        });
        assert!(cancelled.len() < results.len());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn search_limits_results_and_does_not_follow_directory_symlinks() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("rustbridge-worker-limit-{}", std::process::id()));
        std::fs::create_dir_all(root.join("project")).unwrap();
        std::fs::create_dir_all(root.join("outside")).unwrap();
        std::fs::write(root.join("outside/only_external.rs"), "").unwrap();
        symlink(root.join("outside"), root.join("project/link")).unwrap();
        for index in 0..110 {
            std::fs::write(root.join(format!("project/match_{index}.rs")), "").unwrap();
        }
        let mut request = Request {
            root: root.join("project"),
            query: "match".into(),
            show_excluded: true,
        };
        assert_eq!(search(&request, || false).len(), 100);
        request.query = "only_external".into();
        assert!(search(&request, || false).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
