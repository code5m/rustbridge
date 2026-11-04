//! Latest-revision-only, cancellable, incremental Rust AST projection worker.
//! All parsing happens off the egui event thread.
use crate::parser::{Projection, ProjectionEngine};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};

#[derive(Default)]
struct State {
    pending: Option<(u64, String)>,
    result: Option<(u64, Projection)>,
}
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
    latest_revision: AtomicU64,
    closed: AtomicBool,
}

pub struct ProjectionWorker {
    shared: Arc<Shared>,
}

impl ProjectionWorker {
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
            latest_revision: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        });
        let worker_shared = Arc::clone(&shared);
        thread::spawn(move || {
            let mut engine = match ProjectionEngine::new() {
                Ok(engine) => engine,
                Err(_) => return,
            };
            loop {
                let (revision, source) = {
                    let mut state = worker_shared.state.lock().unwrap();
                    while state.pending.is_none() && !worker_shared.closed.load(Ordering::Acquire) {
                        state = worker_shared.ready.wait(state).unwrap();
                    }
                    if worker_shared.closed.load(Ordering::Acquire) {
                        return;
                    }
                    state.pending.take().unwrap()
                };
                let cancelled = || {
                    worker_shared.closed.load(Ordering::Relaxed)
                        || worker_shared.latest_revision.load(Ordering::Acquire) != revision
                };
                let Some(projection) = engine.project(&source, &cancelled) else {
                    continue;
                };
                if cancelled() {
                    continue;
                }
                let mut state = worker_shared.state.lock().unwrap();
                if !cancelled() {
                    state.result = Some((revision, projection));
                }
            }
        });
        Self { shared }
    }

    pub fn submit(&self, revision: u64, source: String) {
        let mut state = self.shared.state.lock().unwrap();
        self.shared
            .latest_revision
            .store(revision, Ordering::Release);
        state.pending = Some((revision, source));
        state.result = None;
        self.shared.ready.notify_one();
    }

    pub fn take(&self) -> Option<(u64, Projection)> {
        self.shared.state.lock().unwrap().result.take()
    }
}

impl Drop for ProjectionWorker {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
        let mut state = self.shared.state.lock().unwrap();
        state.pending = None;
        self.shared.ready.notify_one();
        // Do not synchronously join a parser on the UI thread. The cancellation
        // signal is observed during parsing and AST traversal; its Arc is released.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    #[test]
    fn newest_revision_wins_and_worker_can_be_dropped() {
        let worker = ProjectionWorker::new();
        worker.submit(1, "fn old() {}".repeat(3000));
        worker.submit(2, "struct Latest {}".into());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some((revision, projection)) = worker.take() {
                assert_eq!(revision, 2);
                assert_eq!(projection.java, "class Latest {}");
                break;
            }
            assert!(Instant::now() < deadline, "worker did not finish");
            thread::sleep(Duration::from_millis(5));
        }
        drop(worker);
    }

    #[test]
    fn burst_updates_only_publish_latest() {
        let worker = ProjectionWorker::new();
        for rev in 1..=35 {
            worker.submit(rev, format!("struct 第{rev} {{ value: usize }}"));
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some((revision, projection)) = worker.take() {
                assert_eq!(revision, 35);
                assert!(projection.java.contains("class 第35"));
                break;
            }
            assert!(Instant::now() < deadline, "worker did not finish");
            thread::sleep(Duration::from_millis(5));
        }
    }
}
