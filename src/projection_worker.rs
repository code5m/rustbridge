//! A single worker keeps only the newest pending source and result.

use crate::parser::{self, Projection};
use std::sync::{Arc, Condvar, Mutex};

#[derive(Default)]
struct State {
    pending: Option<(u64, String)>,
    result: Option<(u64, Projection)>,
    closed: bool,
}

pub struct ProjectionWorker {
    shared: Arc<(Mutex<State>, Condvar)>,
}

impl ProjectionWorker {
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
                let (revision, source) = state.pending.take().unwrap();
                drop(state);
                let projection = parser::project(&source);
                let mut state = lock.lock().unwrap();
                if state.closed {
                    break;
                }
                if state
                    .pending
                    .as_ref()
                    .is_none_or(|(next, _)| *next <= revision)
                {
                    state.result = Some((revision, projection));
                }
            }
        });
        Self { shared }
    }

    pub fn submit(&self, revision: u64, source: String) {
        let (lock, ready) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.pending = Some((revision, source));
        state.result = None;
        ready.notify_one();
    }

    pub fn take(&self) -> Option<(u64, Projection)> {
        self.shared.0.lock().unwrap().result.take()
    }
}

impl Drop for ProjectionWorker {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        let mut state = lock.lock().unwrap();
        state.closed = true;
        state.pending = None;
        ready.notify_one();
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
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(worker);
    }
}
