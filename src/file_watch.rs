//! One replaceable, lightweight background metadata watcher per App, not per document.
//! The GUI is woken only when disk identity, metadata or contents' change-time changes.
use eframe::egui;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, SystemTime},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(unix)]
    ctime: i64,
    #[cfg(unix)]
    ctime_nsec: i64,
}
pub fn file_stamp(path: &Path) -> std::io::Result<FileStamp> {
    let meta = fs::metadata(path)?;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(FileStamp {
        len: meta.len(),
        modified: meta.modified().ok(),
        #[cfg(unix)]
        dev: meta.dev(),
        #[cfg(unix)]
        ino: meta.ino(),
        #[cfg(unix)]
        ctime: meta.ctime(),
        #[cfg(unix)]
        ctime_nsec: meta.ctime_nsec(),
    })
}

struct State {
    path: PathBuf,
    observed: Option<FileStamp>,
    generation: u64,
    closed: bool,
}
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
}
pub struct FileWatch {
    path: PathBuf,
    shared: Arc<Shared>,
    events: Receiver<Option<FileStamp>>,
}
impl FileWatch {
    pub fn new(path: PathBuf, initial: Option<FileStamp>, ctx: egui::Context) -> Self {
        Self::with_interval(path, initial, ctx, Duration::from_secs(1))
    }
    fn with_interval(
        path: PathBuf,
        initial: Option<FileStamp>,
        ctx: egui::Context,
        interval: Duration,
    ) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                path: path.clone(),
                observed: initial,
                generation: 0,
                closed: false,
            }),
            ready: Condvar::new(),
        });
        let copy = Arc::clone(&shared);
        let (sender, events) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let (path, generation) = {
                    let state = copy.state.lock().unwrap();
                    let (state, _) = copy.ready.wait_timeout(state, interval).unwrap();
                    if state.closed {
                        return;
                    }
                    (state.path.clone(), state.generation)
                };
                let current = file_stamp(&path).ok();
                let mut state = copy.state.lock().unwrap();
                if state.closed {
                    return;
                }
                if state.generation != generation {
                    continue;
                }
                if current != state.observed {
                    state.observed = current.clone();
                    drop(state);
                    if sender.send(current).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            }
        });
        Self {
            path,
            shared,
            events,
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn set_target(&mut self, path: PathBuf, initial: Option<FileStamp>) {
        if self.path == path {
            // Save of the current document establishes a new known disk baseline.
            let mut state = self.shared.state.lock().unwrap();
            state.observed = initial;
            state.generation += 1;
            self.shared.ready.notify_one();
        } else {
            self.path = path.clone();
            let mut state = self.shared.state.lock().unwrap();
            state.path = path;
            state.observed = initial;
            state.generation += 1;
            self.shared.ready.notify_one();
        }
        while self.events.try_recv().is_ok() {}
    }
    /// Return the latest worker-observed stamp; no UI-thread filesystem call.
    /// The nested Option distinguishes "no event" from "file was deleted".
    pub fn take_changed_stamp(&self) -> Option<Option<FileStamp>> {
        let mut latest = None;
        while let Ok(stamp) = self.events.try_recv() {
            latest = Some(stamp);
        }
        latest
    }

    #[cfg(test)]
    pub fn changed(&self) -> bool {
        self.take_changed_stamp().is_some()
    }
}
impl Drop for FileWatch {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.closed = true;
        self.shared.ready.notify_one();
        // No UI-thread join or blocking disk calls; this one worker exits on notification.
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn fixture() -> PathBuf {
        let id = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p =
            std::env::temp_dir().join(format!("rustbridge-watch-v4-{}-{id}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn watcher_tracks_file_changes_and_switches_without_spawning_another_thread() {
        let dir = fixture();
        let a = dir.join("a.rs");
        let b = dir.join("b.rs");
        fs::write(&a, "old").unwrap();
        fs::write(&b, "old").unwrap();
        let mut watch = FileWatch::with_interval(
            a.clone(),
            file_stamp(&a).ok(),
            egui::Context::default(),
            Duration::from_millis(15),
        );
        fs::write(&a, "longer").unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        while !watch.changed() {
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(5));
        }
        watch.set_target(b.clone(), file_stamp(&b).ok());
        fs::write(&b, "longer").unwrap();
        while !watch.changed() {
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(5));
        }
        drop(watch);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn file_deletion_is_sent_as_none_from_background_worker() {
        let dir = fixture();
        let path = dir.join("deleted.rs");
        fs::write(&path, "fn before() {}").unwrap();
        let watch = FileWatch::with_interval(
            path.clone(),
            file_stamp(&path).ok(),
            egui::Context::default(),
            Duration::from_millis(12),
        );
        fs::remove_file(&path).unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(stamp) = watch.take_changed_stamp() {
                assert!(stamp.is_none(), "deleted file must have no stamp");
                break;
            }
            assert!(Instant::now() < until, "worker missed file deletion");
            thread::sleep(Duration::from_millis(4));
        }
        drop(watch);
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn restored_mtime_same_length_still_detected_by_ctime() {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::MetadataExt;
        let dir = fixture();
        let a = dir.join("ctime.rs");
        fs::write(&a, "abcd").unwrap();
        let before = file_stamp(&a).unwrap();
        let old = fs::metadata(&a).unwrap();
        thread::sleep(Duration::from_millis(15));
        fs::write(&a, "wxyz").unwrap();
        let pathname = std::ffi::CString::new(a.as_os_str().as_bytes()).unwrap();
        let times = [
            libc::timespec {
                tv_sec: old.atime(),
                tv_nsec: old.atime_nsec(),
            },
            libc::timespec {
                tv_sec: old.mtime(),
                tv_nsec: old.mtime_nsec(),
            },
        ];
        let rc = unsafe { libc::utimensat(libc::AT_FDCWD, pathname.as_ptr(), times.as_ptr(), 0) };
        assert_eq!(rc, 0);
        let after = file_stamp(&a).unwrap();
        assert_eq!(before.len, after.len);
        assert_eq!(before.modified, after.modified);
        assert_ne!(before, after);
        fs::remove_dir_all(dir).unwrap();
    }
}
