//! Optional rust-analyzer checks and bounded, cancellable project diagnostics.

use std::{
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

const OUTPUT_LIMIT: usize = 12 * 1024;

pub fn check_async() -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let message = match Command::new("rust-analyzer").arg("--version").output() {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            }
            Ok(output) => format!("rust-analyzer exited with {}", output.status),
            Err(error) => format!("rust-analyzer unavailable: {error}"),
        };
        let _ = sender.send(message);
    });
    receiver
}

pub struct DiagnosticTask {
    receiver: Receiver<String>,
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
}

impl DiagnosticTask {
    pub fn start(root: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel();
        let child = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_child = Arc::clone(&child);
        let worker_cancelled = Arc::clone(&cancelled);
        std::thread::spawn(move || {
            let root = root.to_string_lossy().into_owned();
            let spawned = Command::new("rust-analyzer")
                .args([
                    "diagnostics",
                    root.as_str(),
                    "--disable-build-scripts",
                    "--disable-proc-macros",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let mut process = match spawned {
                Ok(process) => process,
                Err(error) => {
                    let _ = sender.send(format!("rust-analyzer diagnostics unavailable: {error}"));
                    return;
                }
            };
            let stdout_pipe = process.stdout.take();
            let stderr_pipe = process.stderr.take();
            let stdout = std::thread::spawn(move || drain_capped(stdout_pipe));
            let stderr = std::thread::spawn(move || drain_capped(stderr_pipe));
            {
                let mut slot = worker_child.lock().unwrap();
                if worker_cancelled.load(Ordering::Relaxed) {
                    let _ = process.kill();
                }
                *slot = Some(process);
            }
            let status = loop {
                let result = {
                    let mut slot = worker_child.lock().unwrap();
                    if worker_cancelled.load(Ordering::Relaxed)
                        && let Some(process) = slot.as_mut()
                    {
                        let _ = process.kill();
                    }
                    slot.as_mut().map(|process| process.try_wait())
                };
                match result {
                    Some(Ok(Some(status))) => {
                        worker_child.lock().unwrap().take();
                        break status;
                    }
                    Some(Ok(None)) => std::thread::sleep(Duration::from_millis(50)),
                    Some(Err(error)) => {
                        worker_child.lock().unwrap().take();
                        let _ = sender.send(format!("rust-analyzer diagnostics failed: {error}"));
                        return;
                    }
                    None => return,
                }
            };
            let output = String::from_utf8_lossy(&stdout.join().unwrap_or_default())
                .trim()
                .to_owned();
            let errors = String::from_utf8_lossy(&stderr.join().unwrap_or_default())
                .trim()
                .to_owned();
            if worker_cancelled.load(Ordering::Relaxed) {
                let _ = sender.send("rust-analyzer diagnostics cancelled.".into());
            } else if status.success() && output.is_empty() {
                let _ = sender.send("No rust-analyzer diagnostics were reported.".into());
            } else {
                let details = [output, errors]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = sender.send(format!("rust-analyzer diagnostics ({status}):\n{details}"));
            }
        });
        Self {
            receiver,
            child,
            cancelled,
        }
    }

    pub fn take_result(&self) -> Option<String> {
        self.receiver.try_recv().ok()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(process) = self.child.lock().unwrap().as_mut() {
            let _ = process.kill();
        }
    }
}

impl Drop for DiagnosticTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn drain_capped(reader: Option<impl Read>) -> Vec<u8> {
    let Some(mut reader) = reader else {
        return Vec::new();
    };
    let mut kept = Vec::new();
    let mut buffer = [0; 1024];
    while let Ok(read) = reader.read(&mut buffer) {
        if read == 0 {
            break;
        }
        let remaining = OUTPUT_LIMIT.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_output_is_bounded_while_the_stream_is_drained() {
        let input = vec![b'x'; OUTPUT_LIMIT + 100];
        let output = drain_capped(Some(std::io::Cursor::new(input)));
        assert_eq!(output.len(), OUTPUT_LIMIT);
    }
}
