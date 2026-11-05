//! Lazily started rust-analyzer Language Server Protocol client.
//! The UI starts it only when a real Rust editor in a Cargo project is entered.
//! Server lifecycle, JSON-RPC framing, utf16 coordinates, didOpen/didChange,
//! hover, completion, definition, timeouts and cancellation remain separate from UI.
//! It never applies workspace edits and disables build scripts/proc macros.
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};
const MAX_FRAME: usize = 8 * 1024 * 1024;
const SERVER_TIMEOUT: Duration = Duration::from_secs(45);
const QUERY_TIMEOUT: Duration = Duration::from_secs(25);
fn settings() -> Value {
    json!({"cargo":{"buildScripts":{"enable":false}},
        "procMacro":{"enable":false},
        "check":{"enable":false},
        "cachePriming":{"enable":false}})
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Hover,
    Completion,
    Definition,
}
impl Kind {
    fn method(self) -> &'static str {
        match self {
            Self::Hover => "textDocument/hover",
            Self::Completion => "textDocument/completion",
            Self::Definition => "textDocument/definition",
        }
    }
}
#[derive(Debug, Clone)]
pub struct SpanPosition {
    pub line: u32,
    pub character: u32,
}
#[derive(Debug, Clone)]
pub struct Location {
    pub path: PathBuf,
    pub line: u32,
    pub character: u32,
}
#[derive(Debug, Clone)]
pub struct Completion {
    pub label: String,
    pub text: String,
    pub is_snippet: bool,
    pub start: Option<SpanPosition>,
    pub end: Option<SpanPosition>,
}
#[derive(Debug)]
pub enum Event {
    Ready,
    Status(String),
    /// Emitted once the server worker and its subprocess have stopped.
    Stopped,
    Hover {
        path: PathBuf,
        revision: u64,
        line: u32,
        character: u32,
        text: String,
    },
    Completion {
        path: PathBuf,
        revision: u64,
        line: u32,
        character: u32,
        items: Vec<Completion>,
    },
    Definition {
        path: PathBuf,
        revision: u64,
        line: u32,
        character: u32,
        locations: Vec<Location>,
    },
}
enum Request {
    Open {
        path: PathBuf,
        source: String,
        version: u64,
    },
    Change {
        path: PathBuf,
        source: String,
        version: u64,
    },
    Query {
        path: PathBuf,
        kind: Kind,
        revision: u64,
        line: u32,
        character: u32,
    },
    Close {
        path: PathBuf,
    },
    WakeChanges,
    Stop,
}
pub struct LspTask {
    tx: Sender<Request>,
    rx: Receiver<Event>,
    process: Arc<Mutex<Option<Child>>>,
    changes: Arc<Mutex<HashMap<PathBuf, (String, u64)>>>,
    cancelled: Arc<AtomicBool>,
    pub root: PathBuf,
}
impl LspTask {
    pub fn start(root: PathBuf) -> Self {
        let (tx, commands) = mpsc::channel();
        let (events_sender, rx) = mpsc::channel();
        let process = Arc::new(Mutex::new(None));
        let changes = Arc::new(Mutex::new(HashMap::new()));
        let pending_changes = Arc::clone(&changes);
        let cancelled = Arc::new(AtomicBool::new(false));
        let process_worker = Arc::clone(&process);
        let stop = Arc::clone(&cancelled);
        let root_worker = root.clone();
        thread::spawn(move || {
            let done = events_sender.clone();
            run_server(
                root_worker,
                commands,
                events_sender,
                process_worker,
                pending_changes,
                stop,
            );
            let _ = done.send(Event::Stopped);
        });
        Self {
            tx,
            rx,
            process,
            changes,
            cancelled,
            root,
        }
    }
    pub fn open(&self, path: PathBuf, source: String, version: u64) {
        let _ = self.tx.send(Request::Open {
            path,
            source,
            version,
        });
    }
    pub fn change(&self, path: PathBuf, source: String, version: u64) {
        // Replacing a pending document snapshot keeps typing responsive when the
        // analyzer is indexing or its stdin is temporarily blocked.
        let mut changes = self.changes.lock().unwrap();
        let wake = changes.is_empty();
        match changes.entry(path) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if version >= entry.get().1 {
                    entry.insert((source, version));
                }
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert((source, version));
            }
        }
        drop(changes);
        if wake {
            let _ = self.tx.send(Request::WakeChanges);
        }
    }
    pub fn query(&self, path: PathBuf, kind: Kind, revision: u64, line: u32, character: u32) {
        let _ = self.tx.send(Request::Query {
            path,
            kind,
            revision,
            line,
            character,
        });
    }
    pub fn close(&self, path: PathBuf) {
        self.changes.lock().unwrap().remove(&path);
        let _ = self.tx.send(Request::Close { path });
    }
    pub fn poll(&self) -> Vec<Event> {
        let mut events = Vec::new();
        for _ in 0..70 {
            match self.rx.try_recv() {
                Ok(event) => events.push(event),
                Err(_) => break,
            }
        }
        events
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.tx.send(Request::Stop);
        if let Ok(mut slot) = self.process.try_lock()
            && let Some(child) = slot.as_mut()
        {
            kill_group_no_wait(child);
        }
    }
}
impl Drop for LspTask {
    fn drop(&mut self) {
        self.cancel();
    }
}
fn kill_group_no_wait(child: &mut Child) {
    #[cfg(unix)]
    {
        if child.id() <= i32::MAX as u32 {
            // SAFETY: rust-analyzer was spawned in its own process group below.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
}
fn file_uri(path: &Path) -> Option<String> {
    url::Url::from_file_path(path)
        .ok()
        .map(|value| value.to_string())
}
fn from_file_uri(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}
fn send_message(writer: &mut ChildStdin, value: &Value) -> io::Result<()> {
    let data = serde_json::to_vec(value).map_err(io::Error::other)?;
    if data.len() > MAX_FRAME {
        return Err(io::Error::other("Oversized LSP request"));
    }
    write!(writer, "Content-Length: {}\r\n\r\n", data.len())?;
    writer.write_all(&data)?;
    writer.flush()
}
fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    let mut header_total = 0;
    loop {
        let mut line = String::new();
        let count = reader.read_line(&mut line)?;
        if count == 0 {
            return Ok(None);
        }
        header_total += count;
        if header_total > 8192 {
            return Err(io::Error::other("Oversized LSP header"));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((key, value)) = line.split_once(':')
            && key.trim().eq_ignore_ascii_case("Content-Length")
        {
            length = Some(value.trim().parse::<usize>().map_err(io::Error::other)?);
        }
    }
    let length = length.ok_or_else(|| io::Error::other("LSP message missing Content-Length"))?;
    if length > MAX_FRAME {
        return Err(io::Error::other("LSP frame exceeds 8 MiB"));
    }
    let mut data = vec![0; length];
    reader.read_exact(&mut data)?;
    serde_json::from_slice(&data)
        .map(Some)
        .map_err(io::Error::other)
}
fn server_notification(writer: &mut ChildStdin, message: &Value) -> io::Result<()> {
    if let Some(id) = message.get("id") {
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let result = match method {
            "workspace/configuration" => {
                let length = message
                    .pointer("/params/items")
                    .and_then(Value::as_array)
                    .map_or(1, Vec::len)
                    .min(100);
                Value::Array((0..length).map(|_| settings()).collect())
            }
            "workspace/applyEdit" => {
                json!({"applied":false,"failureReason":"RustBridge forbids unsolicited edits"})
            }
            _ => Value::Null,
        };
        send_message(writer, &json!({"jsonrpc":"2.0","id":id,"result":result}))?;
    }
    Ok(())
}
fn initialize(writer: &mut ChildStdin, root: &Path) -> io::Result<()> {
    let uri = file_uri(root).ok_or_else(|| io::Error::other("Invalid project root path"))?;
    let name = root.file_name().unwrap_or_default().to_string_lossy();
    send_message(
        writer,
        &json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{
                "processId":std::process::id(),
                "clientInfo":{"name":"RustBridge","version":"0.5.0"},
                "rootUri":uri,
                "workspaceFolders":[{"uri":uri,"name":name}],
                "capabilities":{
                    "textDocument":{
                        "synchronization":{"didSave":false,"willSave":false},
                        "hover":{"contentFormat":["plaintext","markdown"]},
                        "completion":{"completionItem":{"snippetSupport":false}},
                        "definition":{"linkSupport":true}
                    },
                    "workspace":{"configuration":true,"workspaceFolders":true}
                },
                "initializationOptions":settings(),
                "trace":"off"
            }
        }),
    )
}
fn spawn_rust_analyzer(root: &Path) -> io::Result<Child> {
    let mut cmd = Command::new("rust-analyzer");
    cmd.current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
}
fn request_body(kind: Kind, id: u64, uri: &str, line: u32, character: u32) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":kind.method(),
        "params":{"textDocument":{"uri":uri},
                  "position":{"line":line,"character":character}}})
}
fn issue_request(
    writer: &mut ChildStdin,
    command: Request,
    opened: &mut HashMap<PathBuf, (String, u64)>,
    next_id: &mut u64,
    pending: &mut HashMap<u64, (Kind, PathBuf, u64, u32, u32, Instant)>,
) -> io::Result<()> {
    match command {
        Request::Open {
            path,
            source,
            version,
        } => {
            let uri = file_uri(&path).ok_or_else(|| io::Error::other("Invalid Rust file path"))?;
            if let Some((_, last_version)) = opened.get(&path) {
                if version > *last_version {
                    send_message(
                        writer,
                        &json!({"jsonrpc":"2.0","method":"textDocument/didChange",
                            "params":{"textDocument":{"uri":uri,"version":version},
                                "contentChanges":[{"text":source}]}}),
                    )?;
                    opened.insert(path, (uri, version));
                }
            } else {
                send_message(
                    writer,
                    &json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
                    "params":{"textDocument":{"uri":uri,"languageId":"rust",
                        "version":version,"text":source}}}),
                )?;
                opened.insert(path, (uri, version));
            }
        }
        Request::Change {
            path,
            source,
            version,
        } => {
            let uri = file_uri(&path).ok_or_else(|| io::Error::other("Invalid Rust file path"))?;
            if let Some((_, last_version)) = opened.get(&path) {
                if version > *last_version {
                    send_message(
                        writer,
                        &json!({"jsonrpc":"2.0","method":"textDocument/didChange",
                            "params":{"textDocument":{"uri":uri,"version":version},
                                "contentChanges":[{"text":source}]}}),
                    )?;
                    opened.insert(path, (uri, version));
                }
            } else {
                send_message(
                    writer,
                    &json!({"jsonrpc":"2.0","method":"textDocument/didOpen",
                    "params":{"textDocument":{"uri":uri,"languageId":"rust",
                        "version":version,"text":source}}}),
                )?;
                opened.insert(path, (uri, version));
            }
        }
        Request::Close { path } => {
            if let Some((uri, _)) = opened.remove(&path) {
                send_message(
                    writer,
                    &json!({"jsonrpc":"2.0","method":"textDocument/didClose",
                    "params":{"textDocument":{"uri":uri}}}),
                )?;
            }
        }
        Request::Query {
            path,
            kind,
            revision,
            line,
            character,
        } => {
            if let Some((uri, _)) = opened.get(&path) {
                let id = *next_id;
                *next_id += 1;
                send_message(writer, &request_body(kind, id, uri, line, character))?;
                pending.insert(
                    id,
                    (
                        kind,
                        path,
                        revision,
                        line,
                        character,
                        Instant::now() + QUERY_TIMEOUT,
                    ),
                );
            }
        }
        Request::WakeChanges | Request::Stop => {}
    }
    Ok(())
}

fn parse_hover(result: &Value) -> String {
    let contents = result.get("contents").unwrap_or(result);
    if let Some(value) = contents.as_str() {
        value.into()
    } else if let Some(value) = contents.get("value").and_then(Value::as_str) {
        value.into()
    } else if let Some(arr) = contents.as_array() {
        arr.iter()
            .filter_map(|value| value.as_str().or_else(|| value.get("value")?.as_str()))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    }
}
fn as_position(value: &Value) -> Option<SpanPosition> {
    Some(SpanPosition {
        line: value.get("line")?.as_u64()?.try_into().ok()?,
        character: value.get("character")?.as_u64()?.try_into().ok()?,
    })
}
fn parse_completions(result: &Value) -> Vec<Completion> {
    let list = if let Some(list) = result.as_array() {
        Some(list)
    } else {
        result.get("items").and_then(Value::as_array)
    };
    let Some(list) = list else {
        return Vec::new();
    };
    list.iter()
        .take(50)
        .filter_map(|item| {
            let label = item.get("label")?.as_str()?.to_string();
            let edit = item.get("textEdit");
            let range = edit
                .and_then(|edit| edit.get("range"))
                .or_else(|| edit.and_then(|edit| edit.get("replace")));
            Some(Completion {
                text: edit
                    .and_then(|e| e.get("newText").and_then(Value::as_str))
                    .or_else(|| item.get("insertText").and_then(Value::as_str))
                    .unwrap_or(&label)
                    .to_string(),
                is_snippet: item.get("insertTextFormat").and_then(Value::as_u64) == Some(2),
                label,
                start: range
                    .and_then(|range| range.get("start"))
                    .and_then(as_position),
                end: range
                    .and_then(|range| range.get("end"))
                    .and_then(as_position),
            })
        })
        .collect()
}
/// Expand the common LSP snippet subset without interpreting arbitrary variables.
/// Tab stops are removed, placeholder defaults are retained, and choice lists use
/// their first option. This keeps completion insertion safe and deterministic.
pub fn expand_snippet(input: &str) -> String {
    fn expand(input: &str, depth: usize) -> String {
        if depth > 8 {
            return String::new();
        }
        let chars: Vec<char> = input.chars().collect();
        let mut output = String::new();
        let mut index = 0;
        while index < chars.len() {
            if chars[index] == '\\'
                && index + 1 < chars.len()
                && matches!(chars[index + 1], '$' | '\\' | '}')
            {
                output.push(chars[index + 1]);
                index += 2;
                continue;
            }
            if chars[index] != '$' || index + 1 >= chars.len() {
                output.push(chars[index]);
                index += 1;
                continue;
            }
            if chars[index + 1].is_ascii_digit() {
                index += 2;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                }
                continue;
            }
            if chars[index + 1] != '{' {
                output.push('$');
                index += 1;
                continue;
            }

            let mut end = index + 2;
            let mut nesting = 1_usize;
            while end < chars.len() && nesting > 0 {
                match chars[end] {
                    '{' => nesting += 1,
                    '}' => nesting -= 1,
                    _ => {}
                }
                end += 1;
            }
            if nesting != 0 {
                output.push('$');
                index += 1;
                continue;
            }
            let body: String = chars[index + 2..end - 1].iter().collect();
            let replacement = if let Some((_, default)) = body.split_once(':') {
                expand(default, depth + 1)
            } else if let Some((_, choices)) = body.split_once('|') {
                choices
                    .split('|')
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            } else {
                String::new()
            };
            output.push_str(&replacement);
            index = end;
        }
        output
    }
    expand(input, 0)
}

fn parse_locations(result: &Value) -> Vec<Location> {
    let elements = if let Some(items) = result.as_array() {
        items.clone()
    } else if result.is_object() {
        vec![result.clone()]
    } else {
        Vec::new()
    };
    elements
        .into_iter()
        .take(40)
        .filter_map(|value| {
            let uri = value
                .get("targetUri")
                .or_else(|| value.get("uri"))?
                .as_str()?;
            let range = value
                .get("targetSelectionRange")
                .or_else(|| value.get("range"))?;
            let start = range.get("start").and_then(as_position)?;
            Some(Location {
                path: from_file_uri(uri)?,
                line: start.line,
                character: start.character,
            })
        })
        .collect()
}
fn run_server(
    root: PathBuf,
    commands: Receiver<Request>,
    events: Sender<Event>,
    child_slot: Arc<Mutex<Option<Child>>>,
    changes: Arc<Mutex<HashMap<PathBuf, (String, u64)>>>,
    stop: Arc<AtomicBool>,
) {
    let mut child = match spawn_rust_analyzer(&root) {
        Ok(child) => child,
        Err(error) => {
            let _ = events.send(Event::Status(format!(
                "rust-analyzer LSP 无法启动：{error}"
            )));
            return;
        }
    };
    let Some(mut writer) = child.stdin.take() else {
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        return;
    };
    let stderr = child.stderr.take();
    let reader_channel = mpsc::sync_channel::<io::Result<Value>>(256);
    let server_tx = reader_channel.0;
    let server_rx = reader_channel.1;
    let reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_message(&mut reader) {
                Ok(Some(value)) => {
                    if server_tx.send(Ok(value)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = server_tx.send(Err(error));
                    break;
                }
            }
        }
    });
    let stderr_reader = thread::spawn(move || {
        if let Some(stderr) = stderr {
            let _ = io::copy(&mut BufReader::new(stderr), &mut io::sink());
        }
    });
    {
        let mut slot = child_slot.lock().unwrap();
        if stop.load(Ordering::Acquire) {
            kill_group_no_wait(&mut child);
        }
        *slot = Some(child);
    }
    if let Err(error) = initialize(&mut writer, &root) {
        let _ = events.send(Event::Status(format!("LSP initialize failed: {error}")));
        stop.store(true, Ordering::Release);
    }
    let deadline = Instant::now() + SERVER_TIMEOUT;
    let mut ready = false;
    let mut queued = VecDeque::new();
    let mut opened = HashMap::<PathBuf, (String, u64)>::new();
    let mut next_id = 2;
    let mut pending = HashMap::<u64, (Kind, PathBuf, u64, u32, u32, Instant)>::new();
    while !stop.load(Ordering::Acquire) {
        if !ready && Instant::now() > deadline {
            let _ = events.send(Event::Status("rust-analyzer 初始化超过 45 秒。".into()));
            break;
        }
        while let Ok(command) = commands.try_recv() {
            if matches!(command, Request::Stop) {
                stop.store(true, Ordering::Release);
                break;
            }
            let commands: Vec<Request> = if matches!(command, Request::WakeChanges) {
                let snapshots = std::mem::take(&mut *changes.lock().unwrap());
                snapshots
                    .into_iter()
                    .map(|(path, (source, version))| Request::Change {
                        path,
                        source,
                        version,
                    })
                    .collect()
            } else {
                vec![command]
            };
            for command in commands {
                if ready {
                    if let Err(error) = issue_request(
                        &mut writer,
                        command,
                        &mut opened,
                        &mut next_id,
                        &mut pending,
                    ) {
                        let _ = events.send(Event::Status(format!("LSP request failed: {error}")));
                        stop.store(true, Ordering::Release);
                        break;
                    }
                } else {
                    // Changes are latest-revision-only; never drop didOpen/didClose
                    // just because the analyzer is initializing.
                    if let Request::Change { ref path, .. } = command {
                        queued.retain(|item| !matches!(item, Request::Change { path: other, .. } if other == path));
                    }
                    if matches!(command, Request::Query { .. }) && queued.len() >= 256 {
                        continue;
                    }
                    queued.push_back(command);
                }
            }
        }
        for (id, (_, _, _, _, _, deadline)) in &pending {
            if Instant::now() > *deadline {
                let _ = send_message(
                    &mut writer,
                    &json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}),
                );
            }
        }
        pending.retain(|_, (_, _, _, _, _, deadline)| Instant::now() <= *deadline);
        match server_rx.recv_timeout(Duration::from_millis(60)) {
            Ok(Ok(message)) => {
                if let Some(id) = message.get("id").and_then(Value::as_u64) {
                    if id == 1 && !ready {
                        if let Some(error) = message.get("error") {
                            let _ =
                                events.send(Event::Status(format!("LSP 初始化被拒绝：{error}")));
                            break;
                        }
                        if send_message(
                            &mut writer,
                            &json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
                        )
                        .is_err()
                        {
                            break;
                        }
                        ready = true;
                        let _ = events.send(Event::Ready);
                        while let Some(command) = queued.pop_front() {
                            if issue_request(
                                &mut writer,
                                command,
                                &mut opened,
                                &mut next_id,
                                &mut pending,
                            )
                            .is_err()
                            {
                                break;
                            }
                        }
                    } else if let Some((kind, path, revision, line, character, _)) =
                        pending.remove(&id)
                    {
                        let result = message.get("result").unwrap_or(&Value::Null);
                        let event = match kind {
                            Kind::Hover => Event::Hover {
                                path,
                                revision,
                                line,
                                character,
                                text: parse_hover(result),
                            },
                            Kind::Completion => Event::Completion {
                                path,
                                revision,
                                line,
                                character,
                                items: parse_completions(result),
                            },
                            Kind::Definition => Event::Definition {
                                path,
                                revision,
                                line,
                                character,
                                locations: parse_locations(result),
                            },
                        };
                        let _ = events.send(event);
                    } else if message.get("method").is_some() {
                        let _ = server_notification(&mut writer, &message);
                    }
                } else if message.get("method").is_some() {
                    let _ = server_notification(&mut writer, &message);
                }
            }
            Ok(Err(error)) => {
                let _ = events.send(Event::Status(format!("LSP 协议读取失败：{error}")));
                break;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = events.send(Event::Status("rust-analyzer LSP 已关闭。".into()));
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Ok(mut slot) = child_slot.try_lock()
            && let Some(child) = slot.as_mut()
            && child.try_wait().ok().flatten().is_some()
        {
            let _ = events.send(Event::Status("rust-analyzer 进程已退出".into()));
            break;
        }
    }
    stop.store(true, Ordering::Release);
    {
        let mut slot = child_slot.lock().unwrap();
        if let Some(mut child) = slot.take() {
            kill_group_no_wait(&mut child);
            let _ = child.wait();
        }
    }
    drop(writer);
    let _ = reader.join();
    let _ = stderr_reader.join();
}
/// An exact Rust byte cursor maps to the LSP's 0-based UTF-16 coordinates.
pub fn position_for_byte(source: &str, offset: usize) -> SpanPosition {
    let position = offset.min(source.len());
    let position = if source.is_char_boundary(position) {
        position
    } else {
        (0..=position)
            .rev()
            .find(|&byte| source.is_char_boundary(byte))
            .unwrap_or(0)
    };
    let prefix = &source[..position];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() as u32;
    let current = prefix.rfind('\n').map_or(0, |value| value + 1);
    let character = prefix[current..].encode_utf16().count() as u32;
    SpanPosition { line, character }
}
pub fn byte_for_position(source: &str, pos: &SpanPosition) -> Option<usize> {
    let mut start = 0;
    for _ in 0..pos.line {
        start += source[start..].find('\n')? + 1;
    }
    let end = source[start..]
        .find('\n')
        .map_or(source.len(), |relative| start + relative);
    let slice = &source[start..end];
    let mut used = 0_u32;
    for (byte, ch) in slice.char_indices() {
        if used == pos.character {
            return Some(start + byte);
        }
        used += ch.len_utf16() as u32;
        if used > pos.character {
            return None;
        }
    }
    if used == pos.character {
        Some(end)
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rapid_unsent_changes_keep_only_latest_document_snapshot() {
        let (tx, pending) = mpsc::channel();
        let (_event_sender, rx) = mpsc::channel();
        let task = LspTask {
            tx,
            rx,
            process: Arc::new(Mutex::new(None)),
            changes: Arc::new(Mutex::new(HashMap::new())),
            cancelled: Arc::new(AtomicBool::new(false)),
            root: PathBuf::new(),
        };
        let path = PathBuf::from("unsaved.rs");
        for revision in 1..=1000 {
            task.change(path.clone(), format!("source {revision}"), revision);
        }
        let changes = task.changes.lock().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes.get(&path),
            Some(&(String::from("source 1000"), 1000))
        );
        drop(changes);
        assert!(matches!(pending.try_recv(), Ok(Request::WakeChanges)));
        assert!(
            pending.try_recv().is_err(),
            "only one wake-up must be queued"
        );
        task.close(path);
        assert!(task.changes.lock().unwrap().is_empty());
    }
    #[test]
    fn frame_decoder_handles_utf8_and_content_length() {
        let value = json!({"jsonrpc":"2.0","id":5,"result":"中文🦀"});
        let payload = serde_json::to_vec(&value).unwrap();
        let wire = format!("Content-Length: {}\r\n\r\n", payload.len())
            .into_bytes()
            .into_iter()
            .chain(payload)
            .collect::<Vec<_>>();
        let decoded = read_message(&mut BufReader::new(wire.as_slice()))
            .unwrap()
            .unwrap();
        assert_eq!(decoded, value);
    }
    #[test]
    fn utf16_positions_handle_emoji_crlf_and_chinese() {
        let text = "x🦀\r\n中文let value=3;\n";
        let offset = text.find("let").unwrap();
        let pos = position_for_byte(text, offset);
        assert_eq!((pos.line, pos.character), (1, 2));
        assert_eq!(byte_for_position(text, &pos), Some(offset));
        assert_eq!(position_for_byte("a🦀b", 5).character, 3);
    }
    #[test]
    #[ignore = "manual real rust-analyzer process integration; opt-in only"]
    fn integration_real_rust_analyzer_hover_completion_definition() {
        use std::{fs, time::Instant};
        let root = std::env::temp_dir().join(format!("rustbridge-lsp-real-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"rustbridge_lsp_smoke\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )
        .unwrap();
        let source = String::from(
            "pub struct Widget { pub name: String }\nimpl Widget { pub fn label(&self)->&str { &self.name } }\npub fn use_it() { let item = Widget { name: String::new() }; let _s = item.label(); }\n",
        );
        let path = root.join("src/lib.rs");
        fs::write(&path, &source).unwrap();
        let task = LspTask::start(root.clone());
        task.open(path.clone(), source.clone(), 1);
        let init_deadline = Instant::now() + Duration::from_secs(50);
        let mut ready = false;
        while Instant::now() < init_deadline && !ready {
            for event in task.poll() {
                match event {
                    Event::Ready => ready = true,
                    Event::Status(message) => eprintln!("lsp status: {message}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(35));
        }
        assert!(
            ready,
            "rust-analyzer must initialize through real LSP JSON-RPC"
        );
        let usage = source.rfind("Widget").unwrap();
        let pos = position_for_byte(&source, usage);
        let completion_pos = position_for_byte(&source, source.find("item.la").unwrap() + 7);
        let query_deadline = Instant::now() + Duration::from_secs(60);
        let mut next_query = Instant::now();
        let mut definitions = false;
        let mut hover = false;
        let mut completion = false;
        while Instant::now() < query_deadline && !(definitions && hover && completion) {
            if Instant::now() >= next_query {
                task.query(path.clone(), Kind::Definition, 1, pos.line, pos.character);
                task.query(path.clone(), Kind::Hover, 1, pos.line, pos.character);
                task.query(
                    path.clone(),
                    Kind::Completion,
                    1,
                    completion_pos.line,
                    completion_pos.character,
                );
                next_query = Instant::now() + Duration::from_secs(6);
            }
            for event in task.poll() {
                match event {
                    Event::Hover { text, .. } => {
                        eprintln!(
                            "hover sample {:?}",
                            text.chars().take(150).collect::<String>()
                        );
                        if !text.is_empty() {
                            hover = true;
                        }
                    }
                    Event::Definition { locations, .. } => {
                        eprintln!("definitions {}", locations.len());
                        for location in &locations {
                            eprintln!("  {:?}:{}", location.path, location.line);
                        }
                        if locations.iter().any(|location| location.path == path) {
                            definitions = true;
                        }
                    }
                    Event::Completion { items, .. } => {
                        eprintln!("completion count {}", items.len());
                        for item in items.iter().take(5) {
                            eprintln!("  {}", item.label);
                        }
                        if !items.is_empty() {
                            completion = true;
                        }
                    }
                    Event::Status(message) => eprintln!("lsp status: {message}"),
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
        eprintln!(
            "real rust-analyzer hover={hover} completion={completion} definitions={definitions}"
        );
        assert!(
            hover && definitions,
            "real LSP must resolve hover and definition"
        );
        assert!(completion, "real LSP must provide completion items");
        drop(task);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snippet_expansion_keeps_defaults_and_first_choice_but_removes_tabstops() {
        assert_eq!(
            expand_snippet(concat!("println!(", "$", "{1:value})$0")),
            "println!(value)"
        );
        assert_eq!(expand_snippet(concat!("$", "{1|String,u32|}")), "String");
        assert_eq!(
            expand_snippet(concat!("let \\$name = ", "$", "{1:42};$0")),
            "let $name = 42;"
        );
    }

    #[test]
    fn completion_and_definition_response_shapes() {
        let items = parse_completions(&json!({"items":[{"label":"print",
            "textEdit":{"newText":"println!",
                "range":{"start":{"line":2,"character":3},"end":{"line":2,"character":4}}}}]}));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "println!");
        assert_eq!(items[0].start.as_ref().unwrap().line, 2);
        let native = std::env::temp_dir().join("rustbridge-cross-platform-main.rs");
        let uri = file_uri(&native).unwrap();
        let def = parse_locations(&json!({"targetUri":uri,
            "targetSelectionRange":{"start":{"line":5,"character":7},"end":{"line":5,"character":8}}}));
        assert_eq!(def[0].path, native);
        assert_eq!(def[0].line, 5);
    }
}
