//! Optional, opt-in diagnostics; bounded output, process group cancellation and timeout.
//! No LSP session or automatic execution of opened project code.
use std::{
    ffi::OsString,
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 12 * 1024;
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const DIAGNOSTIC_TIMEOUT: Duration = Duration::from_secs(45);

#[cfg(unix)]
fn separate_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}
#[cfg(not(unix))]
fn separate_process_group(_command: &mut Command) {}

fn stop_process(process: &mut Child) {
    // On Unix, cancel the isolated process group, including helper descendants.
    // The group was created for this child using CommandExt::process_group(0).
    #[cfg(unix)]
    {
        let pid = process.id();
        if pid <= i32::MAX as u32 {
            // SAFETY: kill() receives a valid negative process group ID spawned above.
            // Only the isolated rust-analyzer process group is targeted.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
    let _ = process.kill();
}

pub fn check_async() -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut command = Command::new("rust-analyzer");
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        separate_process_group(&mut command);
        let message = match command.spawn() {
            Err(error) => format!("rust-analyzer unavailable: {error}"),
            Ok(mut child) => {
                let deadline = Instant::now() + CHECK_TIMEOUT;
                let message = loop {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            let mut message = String::new();
                            if let Some(mut stdout) = child.stdout.take() {
                                let _ = stdout.read_to_string(&mut message);
                            }
                            if status.success() {
                                break message.trim().to_owned();
                            }
                            break format!("rust-analyzer exited with {status}");
                        }
                        Ok(None) if Instant::now() < deadline => {
                            thread::sleep(Duration::from_millis(30))
                        }
                        Ok(None) => {
                            stop_process(&mut child);
                            let _ = child.wait();
                            break "rust-analyzer availability check timed out.".into();
                        }
                        Err(error) => {
                            stop_process(&mut child);
                            let _ = child.wait();
                            break format!("rust-analyzer availability failed: {error}");
                        }
                    }
                };
                // Check is opt-in and no monitoring thread survives this path.
                message
            }
        };
        let _ = sender.send(message);
    });
    receiver
}

pub struct DiagnosticTask {
    receiver: Receiver<String>,
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
    project_root: PathBuf,
}

impl DiagnosticTask {
    pub fn start(root: PathBuf) -> Self {
        let args = vec![
            OsString::from("diagnostics"),
            root.as_os_str().to_owned(),
            OsString::from("--disable-build-scripts"),
            OsString::from("--disable-proc-macros"),
        ];
        Self::spawn(
            root,
            OsString::from("rust-analyzer"),
            args,
            DIAGNOSTIC_TIMEOUT,
        )
    }

    fn spawn(root: PathBuf, program: OsString, args: Vec<OsString>, timeout: Duration) -> Self {
        let (sender, receiver) = mpsc::channel();
        let child = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_child = Arc::clone(&child);
        let worker_cancelled = Arc::clone(&cancelled);
        thread::spawn(move || {
            let mut command = Command::new(program);
            command
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            separate_process_group(&mut command);
            let mut process = match command.spawn() {
                Ok(process) => process,
                Err(error) => {
                    let _ = sender.send(format!("rust-analyzer diagnostics unavailable: {error}"));
                    return;
                }
            };
            let stdout_pipe = process.stdout.take();
            let stderr_pipe = process.stderr.take();
            let stdout = thread::spawn(move || drain_capped(stdout_pipe));
            let stderr = thread::spawn(move || drain_capped(stderr_pipe));
            {
                let mut slot = worker_child.lock().unwrap();
                if worker_cancelled.load(Ordering::Acquire) {
                    stop_process(&mut process);
                }
                *slot = Some(process);
            }
            let deadline = Instant::now() + timeout;
            let mut timed_out = false;
            let mut terminated = false;
            let status = loop {
                let result = {
                    let mut slot = worker_child.lock().unwrap();
                    if !terminated
                        && (worker_cancelled.load(Ordering::Acquire) || Instant::now() >= deadline)
                    {
                        timed_out = !worker_cancelled.load(Ordering::Acquire);
                        if let Some(process) = slot.as_mut() {
                            stop_process(process);
                        }
                        terminated = true;
                    }
                    slot.as_mut().map(|process| process.try_wait())
                };
                match result {
                    Some(Ok(Some(status))) => {
                        worker_child.lock().unwrap().take();
                        break Some(status);
                    }
                    Some(Ok(None)) => thread::sleep(Duration::from_millis(40)),
                    Some(Err(error)) => {
                        if let Some(mut process) = worker_child.lock().unwrap().take() {
                            stop_process(&mut process);
                            let _ = process.wait();
                        }
                        let _ = stdout.join();
                        let _ = stderr.join();
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
            if timed_out {
                let _ = sender.send(format!(
                    "rust-analyzer diagnostics exceeded {} seconds and was stopped.",
                    timeout.as_secs()
                ));
            } else if worker_cancelled.load(Ordering::Acquire) {
                let _ = sender.send("rust-analyzer diagnostics cancelled.".into());
            } else if status.is_some_and(|status| status.success()) && output.is_empty() {
                let _ = sender.send("No rust-analyzer diagnostics were reported.".into());
            } else {
                let details = [output, errors]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = sender.send(format!(
                    "rust-analyzer diagnostics ({}):\n{details}",
                    status.map_or_else(|| "unknown".into(), |status| status.to_string())
                ));
            }
        });
        Self {
            receiver,
            child,
            cancelled,
            project_root: root,
        }
    }

    pub fn project_root(&self) -> &PathBuf {
        &self.project_root
    }

    pub fn take_result(&self) -> Option<String> {
        self.receiver.try_recv().ok()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(process) = self.child.lock().unwrap().as_mut() {
            stop_process(process);
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

    #[cfg(unix)]
    #[test]
    fn manual_cancel_stops_background_diagnostics() {
        let task = DiagnosticTask::spawn(
            PathBuf::from("."),
            OsString::from("/bin/sh"),
            vec![OsString::from("-c"), OsString::from("sleep 5")],
            Duration::from_secs(10),
        );
        task.cancel();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = task.take_result() {
                assert!(result.contains("cancelled"), "{result}");
                break;
            }
            assert!(Instant::now() < deadline, "cancelled child did not exit");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    #[test]
    fn hung_subprocess_is_terminated_by_timeout() {
        let task = DiagnosticTask::spawn(
            PathBuf::from("."),
            OsString::from("/bin/sh"),
            vec![OsString::from("-c"), OsString::from("sleep 5")],
            Duration::from_millis(160),
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = task.take_result() {
                assert!(result.contains("exceeded"), "{result}");
                break;
            }
            assert!(Instant::now() < deadline, "hung process was not terminated");
            thread::sleep(Duration::from_millis(20));
        }
    }
}
