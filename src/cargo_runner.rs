//! Opt-in Cargo execution: explicit confirmation, bounded output, actionable diagnostics,
//! process-group cancellation and timeouts. Never run a user's project automatically.
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Check,
    Test,
    Run,
}
impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Self::Check => "cargo check",
            Self::Test => "cargo test",
            Self::Run => "cargo run",
        }
    }
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Check => &["check", "--message-format=json"],
            Self::Test => &["test", "--message-format=json"],
            Self::Run => &["run", "--message-format=json"],
        }
    }
    fn timeout(self) -> Duration {
        match self {
            Self::Check => Duration::from_secs(180),
            Self::Test | Self::Run => Duration::from_secs(240),
        }
    }
}
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: String,
    pub message: String,
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}
pub enum Event {
    Log(String),
    Diagnostic(Diagnostic),
    Finished(String),
}
fn parse_compiler_message(line: &str, root: &Path) -> Option<Diagnostic> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("reason")?.as_str()? != "compiler-message" {
        return None;
    }
    let diagnostic = value.get("message")?;
    let spans = diagnostic.get("spans")?.as_array()?;
    let primary = spans
        .iter()
        .find(|span| span.get("is_primary").and_then(Value::as_bool) == Some(true))
        .or_else(|| spans.first())?;
    let file = primary.get("file_name")?.as_str()?;
    if file.is_empty() {
        return None;
    }
    let path = Path::new(file);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    Some(Diagnostic {
        level: diagnostic
            .get("level")
            .and_then(Value::as_str)
            .unwrap_or("note")
            .into(),
        message: diagnostic
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("diagnostic")
            .into(),
        path,
        line: primary
            .get("line_start")
            .and_then(Value::as_u64)
            .unwrap_or(1) as usize,
        column: primary
            .get("column_start")
            .and_then(Value::as_u64)
            .unwrap_or(1) as usize,
    })
}
fn child_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}
fn cancel_process(child: &mut Child) {
    #[cfg(unix)]
    {
        let pid = child.id();
        if pid <= i32::MAX as u32 {
            // SAFETY: pid is a child whose process group we created in child_group.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}
pub struct CargoTask {
    receiver: Receiver<Event>,
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
    pub root: PathBuf,
}
impl CargoTask {
    pub fn start(root: PathBuf, action: Action) -> Self {
        Self::start_with(root, action, "cargo".into())
    }
    fn start_with(root: PathBuf, action: Action, program: std::ffi::OsString) -> Self {
        let (tx, receiver) = mpsc::sync_channel::<Event>(256);
        let child = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));
        let actor = Arc::clone(&child);
        let cancelled_worker = Arc::clone(&cancelled);
        let root_worker = root.clone();
        thread::spawn(move || {
            if !root_worker.join("Cargo.toml").is_file() {
                let _ = tx.send(Event::Finished("已取消：目录中没有 Cargo.toml".into()));
                return;
            }
            let mut command = Command::new(program);
            command
                .args(action.args())
                .current_dir(&root_worker)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            child_group(&mut command);
            let mut process = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    let _ = tx.send(Event::Finished(format!(
                        "{} 启动失败：{error}",
                        action.label()
                    )));
                    return;
                }
            };
            let stdout = process.stdout.take();
            let stderr = process.stderr.take();
            let stdout_reader = spawn_reader(stdout, tx.clone(), root_worker.clone());
            let stderr_reader = spawn_reader(stderr, tx.clone(), root_worker.clone());
            {
                let mut guard = actor.lock().unwrap();
                if cancelled_worker.load(Ordering::Acquire) {
                    cancel_process(&mut process);
                }
                *guard = Some(process);
            }
            let limit = Instant::now() + action.timeout();
            let outcome = loop {
                let expired = Instant::now() >= limit;
                let cancelling = cancelled_worker.load(Ordering::Acquire);
                let mut guard = actor.lock().unwrap();
                let Some(child) = guard.as_mut() else {
                    break "进程已经退出".to_string();
                };
                if cancelling || expired {
                    cancel_process(child);
                    guard.take();
                    break if cancelling {
                        "已取消，子进程组已停止".into()
                    } else {
                        format!("已超过 {} 秒超时并终止", action.timeout().as_secs())
                    };
                }
                match child.try_wait() {
                    Ok(Some(status)) => {
                        guard.take();
                        break format!("{} 已退出：{status}", action.label());
                    }
                    Ok(None) => {}
                    Err(error) => {
                        cancel_process(child);
                        guard.take();
                        break format!("进程等待失败：{error}");
                    }
                }
                drop(guard);
                thread::sleep(Duration::from_millis(45));
            };
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            let _ = tx.send(Event::Finished(outcome));
        });
        Self {
            receiver,
            child,
            cancelled,
            root,
        }
    }
    pub fn poll(&self) -> Vec<Event> {
        let mut result = Vec::new();
        for _ in 0..70 {
            match self.receiver.try_recv() {
                Ok(event) => result.push(event),
                Err(_) => break,
            }
        }
        result
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut child) = self.child.try_lock()
            && let Some(child) = child.as_mut()
        {
            #[cfg(unix)]
            {
                let pid = child.id();
                if pid <= i32::MAX as u32 {
                    unsafe {
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                }
            }
            let _ = child.kill();
        }
    }
}
impl Drop for CargoTask {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn spawn_reader(
    reader: Option<impl Read + Send + 'static>,
    tx: SyncSender<Event>,
    root: PathBuf,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let Some(reader) = reader else {
            return;
        };
        for line in BufReader::new(reader).lines() {
            let Ok(line) = line else {
                break;
            };
            if let Some(diagnostic) = parse_compiler_message(&line, &root) {
                if tx.send(Event::Diagnostic(diagnostic)).is_err() {
                    break;
                }
            } else if !line.starts_with('{') {
                let message = if line.len() > 4096 {
                    format!(
                        "{}… [line truncated]",
                        String::from_utf8_lossy(&line.as_bytes()[..4096])
                    )
                } else {
                    line
                };
                if tx.send(Event::Log(message)).is_err() {
                    break;
                }
            }
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_rustc_json_primary_spans_and_real_path() {
        let input = r#"{"reason":"compiler-message","message":{"level":"error","message":"cannot find type","spans":[{"file_name":"src/main.rs","line_start":12,"column_start":8,"is_primary":true}]}}"#;
        let root = std::env::temp_dir().join("rustbridge-cargo-smoke");
        let result = parse_compiler_message(input, &root).unwrap();
        assert_eq!(result.path, root.join("src/main.rs"));
        assert_eq!(result.line, 12);
        assert_eq!(result.column, 8);
        assert_eq!(result.level, "error");
        assert!(
            parse_compiler_message(r#"{"reason":"build-finished"}"#, Path::new("/opt")).is_none()
        );
    }
    #[test]
    fn correct_command_and_timeout_configuration() {
        assert_eq!(Action::Check.args(), ["check", "--message-format=json"]);
        assert_eq!(Action::Test.args(), ["test", "--message-format=json"]);
        assert_eq!(Action::Run.args(), ["run", "--message-format=json"]);
        assert!(Action::Run.timeout() > Action::Check.timeout());
    }
}
