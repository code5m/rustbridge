//! Low-frequency metadata observer; wake egui only if the real file changes.
//! The watcher never reads source contents; save still rechecks full bytes.
use eframe::egui;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
}

pub fn file_stamp(path: &Path) -> std::io::Result<FileStamp> {
    let metadata = fs::metadata(path)?;
    Ok(FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

pub struct FileWatch {
    path: PathBuf,
    stopped: Arc<AtomicBool>,
    receiver: Receiver<()>,
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
        let stopped = Arc::new(AtomicBool::new(false));
        let monitor = Arc::clone(&stopped);
        let (sender, receiver) = mpsc::channel();
        let watch_path = path.clone();
        thread::spawn(move || {
            let mut last = initial;
            while !monitor.load(Ordering::Acquire) {
                thread::sleep(interval);
                if monitor.load(Ordering::Acquire) {
                    break;
                }
                let current = file_stamp(&watch_path).ok();
                if current != last {
                    last = current;
                    if sender.send(()).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            }
        });
        Self {
            path,
            stopped,
            receiver,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn changed(&self) -> bool {
        let mut changed = false;
        while self.receiver.try_recv().is_ok() {
            changed = true;
        }
        changed
    }
}

impl Drop for FileWatch {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn file_change_wakes_without_needing_ui_timer() {
        let dir = std::env::temp_dir().join(format!(
            "rustbridge-watch-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("watch")
        ));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("example.rs");
        fs::write(&file, "old").unwrap();
        let watcher = FileWatch::with_interval(
            file.clone(),
            file_stamp(&file).ok(),
            egui::Context::default(),
            Duration::from_millis(15),
        );
        fs::write(&file, "longer content").unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if watcher.changed() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "watcher did not detect file change"
            );
            thread::sleep(Duration::from_millis(5));
        }
        drop(watcher);
        fs::remove_dir_all(dir).unwrap();
    }
}
