mod analyzer;
mod cargo_runner;
mod code_view;
mod document_io;
mod editor_core;
mod editor_eol;
mod editor_ui;
mod file_search_worker;
mod file_tree;
mod file_watch;
mod lesson_quiz;
mod lessons;
mod line_index;
mod lsp;
mod parser;
mod persistence;
mod projection_worker;
mod safe_save;
mod symbol_search;
mod ui_fonts;

use editor_core::line_number_gutter;
use eframe::egui::{self, Color32};
use file_watch::{FileStamp, file_stamp};
#[cfg(test)]
use safe_save::save_if_unchanged;
#[cfg(test)]
use std::fs;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Java,
    Rust,
    Compare,
    Learn,
}

struct App {
    root: Option<PathBuf>,
    cargo_project: bool,
    file: Option<PathBuf>,
    rust: String,
    projection: parser::Projection,
    worker: Option<projection_worker::ProjectionWorker>,
    revision: u64,
    projection_pending: bool,
    projection_due: Option<std::time::Instant>,
    mode: Mode,
    lesson: usize,
    selected: Option<usize>,
    jump_to: Option<usize>,
    sync_scroll: bool,
    scroll_follow: Option<(bool, f32)>,
    last_scroll: Option<(f32, f32)>,
    dirty: bool,
    saved_source: String,
    confirm_close: bool,
    pending_open: Option<PathBuf>,
    show_excluded: bool,
    dir_index: file_tree::DirectoryIndex,
    directory_limits: HashMap<PathBuf, usize>,
    file_query: String,
    search_results: Vec<PathBuf>,
    file_search_worker: Option<file_search_worker::FileSearchWorker>,
    file_search_revision: u64,
    file_search_pending: bool,
    recent_files: Vec<PathBuf>,
    disk_stamp: Option<FileStamp>,
    external_change: bool,
    file_watch: Option<file_watch::FileWatch>,
    document_io: Option<document_io::DocumentIo>,
    io_revision: u64,
    opening: Option<(u64, PathBuf, u64)>,
    saving: Option<(u64, PathBuf, String)>,
    opening_started: Option<std::time::Instant>,
    saving_started: Option<std::time::Instant>,
    after_save_open: bool,
    after_save_close: bool,
    discard_on_open: bool,
    symbol_worker: Option<symbol_search::SymbolSearch>,
    symbol_revision: u64,
    symbol_query: String,
    symbol_hits: Vec<symbol_search::Declaration>,
    symbol_pending: bool,
    symbol_target: Option<(PathBuf, parser::ByteSpan)>,
    editor_query: String,
    editor_find: Option<parser::ByteSpan>,
    editor_jump: Option<parser::ByteSpan>,
    editor_line_starts: Vec<usize>,
    editor_gutter: String,
    editor_gutter_revision: u64,
    editor_page_start: usize,
    editor_page_target: usize,
    editor_scroll_override: Option<f32>,
    lesson_return: Option<(Mode, Option<usize>)>,
    analyzer_status: String,
    analyzer_check: Option<std::sync::mpsc::Receiver<String>>,
    analyzer_diagnostics: Option<analyzer::DiagnosticTask>,
    analyzer_diagnostics_status: String,
    status: String,
    state_store: Option<persistence::Store>,
    state_worker: Option<persistence::PersistenceWorker>,
    last_session: Option<persistence::Session>,
    session_due: Option<std::time::Instant>,
    recovery_due: Option<std::time::Instant>,
    pending_recovery: Option<persistence::Journal>,
    recovered_mode: Option<Mode>,
    cursor_byte: usize,
    completed_lessons: Vec<bool>,
    quiz_choice: Option<usize>,
    cargo_task: Option<cargo_runner::CargoTask>,
    cargo_confirm: Option<cargo_runner::Action>,
    cargo_log: Vec<String>,
    cargo_diagnostics: Vec<cargo_runner::Diagnostic>,
    cargo_status: String,
    cargo_diag_target: Option<(PathBuf, usize, usize)>,
    lsp_task: Option<lsp::LspTask>,
    lsp_status: String,
    lsp_ready: bool,
    lsp_hover: String,
    lsp_completions: Vec<lsp::Completion>,
    lsp_definitions: Vec<lsp::Location>,
    lsp_target: Option<(PathBuf, lsp::SpanPosition)>,
}

impl Default for App {
    fn default() -> Self {
        let rust = include_str!("../examples/basics.rs").to_owned();
        let editor_line_starts = parser::line_starts(&rust);
        let editor_gutter = line_number_gutter(&rust);
        Self {
            root: None,
            cargo_project: false,
            file: None,
            projection: parser::project(&rust),
            worker: None,
            revision: 0,
            projection_pending: false,
            projection_due: None,
            saved_source: rust.clone(),
            rust,
            mode: Mode::Java,
            lesson: 0,
            selected: None,
            jump_to: None,
            sync_scroll: true,
            scroll_follow: None,
            last_scroll: None,
            dirty: false,
            confirm_close: false,
            pending_open: None,
            show_excluded: false,
            dir_index: file_tree::DirectoryIndex::new(),
            directory_limits: HashMap::new(),
            file_query: String::new(),
            search_results: Vec::new(),
            file_search_worker: None,
            file_search_revision: 0,
            file_search_pending: false,
            recent_files: Vec::new(),
            disk_stamp: None,
            external_change: false,
            file_watch: None,
            document_io: None,
            io_revision: 0,
            opening: None,
            saving: None,
            opening_started: None,
            saving_started: None,
            after_save_open: false,
            after_save_close: false,
            discard_on_open: false,
            symbol_worker: None,
            symbol_revision: 0,
            symbol_query: String::new(),
            symbol_hits: Vec::new(),
            symbol_pending: false,
            symbol_target: None,
            editor_query: String::new(),
            editor_find: None,
            editor_jump: None,
            editor_line_starts,
            editor_gutter,
            editor_gutter_revision: 0,
            editor_page_start: 0,
            editor_page_target: 1,
            editor_scroll_override: None,
            lesson_return: None,
            analyzer_status: "Not checked; core reading does not require rust-analyzer.".into(),
            analyzer_check: None,
            analyzer_diagnostics: None,
            analyzer_diagnostics_status: "Diagnostics are off until requested for a Cargo project."
                .into(),
            status: "Java view is an AST-aligned teaching aid, not compilable Java.".into(),
            state_store: None,
            state_worker: None,
            last_session: None,
            session_due: None,
            recovery_due: None,
            pending_recovery: None,
            recovered_mode: None,
            cursor_byte: 0,
            completed_lessons: vec![false; lessons::LESSONS.len()],
            quiz_choice: None,
            cargo_task: None,
            cargo_confirm: None,
            cargo_log: Vec::new(),
            cargo_diagnostics: Vec::new(),
            cargo_status: "Cargo 检查、测试、运行需要手动授权，不会自动执行。".into(),
            cargo_diag_target: None,
            lsp_task: None,
            lsp_status: "rust-analyzer LSP 未启动。按需启用，不在启动时索引项目。".into(),
            lsp_ready: false,
            lsp_hover: String::new(),
            lsp_completions: Vec::new(),
            lsp_definitions: Vec::new(),
            lsp_target: None,
        }
    }
}

impl App {
    fn flush_shutdown_state(&self) {
        if let Some(worker) = self.state_worker.as_ref() {
            worker.send(persistence::Job::Session(self.session_snapshot()));
            if let Some(path) = self.file.clone() {
                if self.dirty {
                    worker.send(persistence::Job::SaveJournal {
                        path,
                        baseline: self.saved_source.clone(),
                        edited: self.rust.clone(),
                    });
                } else {
                    worker.send(persistence::Job::DeleteJournal(path));
                }
            }
        }
    }

    fn initialize_state(&mut self, restore_last_document: bool) {
        let store = match persistence::Store::from_system() {
            Ok(store) => store,
            Err(error) => {
                self.status = format!("Session storage unavailable: {error}");
                return;
            }
        };
        self.initialize_state_from_store(store, restore_last_document);
    }
    fn initialize_state_from_store(
        &mut self,
        store: persistence::Store,
        restore_last_document: bool,
    ) {
        if let Some(session) = store.load_session() {
            self.lesson = session.lesson.min(lessons::LESSONS.len().saturating_sub(1));
            self.completed_lessons = normalized_completion(&session.completed_lessons);
            self.cursor_byte = session.cursor_byte;
            self.recovered_mode = Some(mode_from_name(&session.mode));
            if restore_last_document {
                self.root = session.root.clone().filter(|p| p.is_dir());
                self.cargo_project = self.root.as_ref().is_some_and(|p| is_cargo_root(p));
                if let Some(path) = session.file.clone().filter(|p| p.is_file()) {
                    self.open_file(path);
                } else if let Some(mode) = self.recovered_mode.take() {
                    self.mode = mode;
                }
            }
            self.last_session = Some(session);
        }
        self.state_worker = Some(persistence::PersistenceWorker::spawn(store.clone()));
        self.state_store = Some(store);
    }

    fn session_snapshot(&self) -> persistence::Session {
        persistence::Session {
            root: self.root.clone(),
            file: self.file.clone(),
            mode: mode_name(self.mode).into(),
            lesson: self.lesson,
            cursor_byte: self.cursor_byte,
            completed_lessons: self.completed_lessons.clone(),
        }
    }

    fn mark_dirty_for_recovery(&mut self) {
        self.recovery_due = Some(std::time::Instant::now() + std::time::Duration::from_millis(350));
    }
    fn flush_recovery_journal(&mut self) {
        self.recovery_due = None;
        if let (Some(path), Some(worker)) = (self.file.clone(), self.state_worker.as_ref()) {
            if self.dirty {
                worker.send(persistence::Job::SaveJournal {
                    path,
                    baseline: self.saved_source.clone(),
                    edited: self.rust.clone(),
                });
            } else {
                worker.send(persistence::Job::DeleteJournal(path));
            }
        }
    }
    fn discard_current_recovery(&mut self) {
        if let (Some(path), Some(worker)) = (self.file.as_ref(), self.state_worker.as_ref()) {
            worker.send(persistence::Job::DeleteJournal(path.clone()));
        }
        self.pending_recovery = None;
        self.recovery_due = None;
    }
    fn poll_persistence(&mut self) {
        if self
            .recovery_due
            .is_some_and(|due| std::time::Instant::now() >= due)
        {
            self.flush_recovery_journal();
        }
        let session = self.session_snapshot();
        if self.last_session.as_ref() != Some(&session) && self.session_due.is_none() {
            self.session_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(600));
        }
        if self
            .session_due
            .is_some_and(|due| std::time::Instant::now() >= due)
        {
            if let Some(worker) = self.state_worker.as_ref() {
                worker.send(persistence::Job::Session(session.clone()));
            }
            self.last_session = Some(session);
            self.session_due = None;
        }
    }

    fn start_lsp(&mut self) {
        let Some(root) = self.root.clone().and_then(|p| p.canonicalize().ok()) else {
            self.lsp_status = "请先打开实际 Cargo 项目目录。".into();
            return;
        };
        if !is_cargo_root(&root) {
            self.lsp_status = "当前目录缺少 Cargo.toml，无法初始化项目 LSP。".into();
            return;
        }
        self.lsp_task = None;
        let worker = lsp::LspTask::start(root);
        if let Some(path) = self.file.as_ref().and_then(|p| p.canonicalize().ok()) {
            worker.open(path, self.rust.clone(), self.revision);
        }
        self.lsp_task = Some(worker);
        self.lsp_ready = false;
        self.lsp_hover.clear();
        self.lsp_completions.clear();
        self.lsp_definitions.clear();
        self.lsp_status = "正在初始化 rust-analyzer LSP；默认关闭 build.rs 与 proc macros。".into();
    }
    fn poll_lsp(&mut self) {
        let Some(task) = self.lsp_task.as_ref() else {
            return;
        };
        if !self
            .root
            .as_ref()
            .is_some_and(|root| root.canonicalize().is_ok_and(|p| p == task.root))
        {
            task.cancel();
            self.lsp_task = None;
            self.lsp_ready = false;
            return;
        }
        for event in task.poll() {
            match event {
                lsp::Event::Ready => {
                    self.lsp_ready = true;
                    self.lsp_status =
                        "rust-analyzer LSP 已初始化；可以请求类型、补全和定义。".into();
                }
                lsp::Event::Status(text) => {
                    self.lsp_status = text;
                }
                lsp::Event::Hover { revision, text } if revision == self.revision => {
                    self.lsp_hover = text.chars().take(12000).collect();
                    self.lsp_status = "rust-analyzer 悬浮信息已返回。".into();
                }
                lsp::Event::Completion { revision, items } if revision == self.revision => {
                    self.lsp_completions = items;
                    self.lsp_status = format!(
                        "rust-analyzer 返回 {} 项补全候选。",
                        self.lsp_completions.len()
                    );
                }
                lsp::Event::Definition {
                    revision,
                    locations,
                } if revision == self.revision => {
                    self.lsp_definitions = locations;
                    self.lsp_status = format!(
                        "rust-analyzer 返回 {} 个定义位置。",
                        self.lsp_definitions.len()
                    );
                }
                _ => {}
            }
        }
    }
    fn lsp_query(&mut self, kind: lsp::Kind) {
        let Some(task) = self.lsp_task.as_ref() else {
            return;
        };
        let byte = self
            .selected
            .and_then(|idx| self.projection.anchors.get(idx))
            .map_or(self.cursor_byte, |anchor| anchor.rust.start);
        let pos = lsp::position_for_byte(&self.rust, byte);
        task.query(kind, self.revision, pos.line, pos.character);
        self.lsp_status = format!("正在请求 {:?}，源版本 {}…", kind, self.revision);
    }
    fn insert_completion(&mut self, completion: lsp::Completion) {
        if self.file.is_none() {
            return;
        }
        if completion.text.contains('$') && completion.text.contains('{') {
            self.lsp_status = "服务端给出了片段占位符；为避免破坏源码，暂不自动插入。".into();
            return;
        }
        let cursor = self.cursor_byte.min(self.rust.len());
        let range = match (&completion.start, &completion.end) {
            (Some(start), Some(end)) => {
                let Some(a) = lsp::byte_for_position(&self.rust, start) else {
                    return;
                };
                let Some(b) = lsp::byte_for_position(&self.rust, end) else {
                    return;
                };
                a..b
            }
            _ => {
                let mut start = cursor;
                while let Some((offset, ch)) = self.rust[..start].char_indices().last() {
                    if ch == '_' || ch.is_alphanumeric() {
                        start = offset;
                    } else {
                        break;
                    }
                }
                start..cursor
            }
        };
        if range.start > range.end
            || range.end > self.rust.len()
            || !self.rust.is_char_boundary(range.start)
            || !self.rust.is_char_boundary(range.end)
        {
            return;
        }
        self.rust.replace_range(range.clone(), &completion.text);
        self.cursor_byte = range.start + completion.text.len();
        self.update_projection();
        self.dirty = self.rust != self.saved_source;
        self.mark_dirty_for_recovery();
        self.lsp_completions.clear();
        self.status = "已插入真实 rust-analyzer 补全文本；原始 Rust 文件尚未保存。".into();
    }
    fn jump_lsp_definition(&mut self, location: lsp::Location) {
        if self.dirty {
            self.status = "当前文件仍有未保存修改，请保存后再跨文件跳转。".into();
            return;
        }
        let position = lsp::SpanPosition {
            line: location.line,
            character: location.character,
        };
        if self.file.as_ref() == Some(&location.path) {
            if let Some(byte) = lsp::byte_for_position(&self.rust, &position) {
                self.editor_jump = Some(parser::ByteSpan {
                    start: byte,
                    end: byte,
                });
                self.mode = Mode::Rust;
                self.cursor_byte = byte;
            }
        } else {
            self.lsp_target = Some((location.path.clone(), position));
            self.open_file(location.path);
        }
    }

    fn start_cargo_action(&mut self, action: cargo_runner::Action) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.dirty || self.saving.is_some() {
            self.cargo_status = "当前 Rust 文件有未保存修改，请先保存再运行 Cargo。".into();
            return;
        }
        if !is_cargo_root(&root) {
            self.cargo_status = "只支持实际 Cargo.toml 根目录。".into();
            return;
        }
        if self.cargo_task.is_some() {
            self.cargo_status = "已有 Cargo 任务运行中。".into();
            return;
        }
        self.cargo_log.clear();
        self.cargo_diagnostics.clear();
        self.cargo_status = format!("{} 已由用户明确批准，正在执行…", action.label());
        self.cargo_task = Some(cargo_runner::CargoTask::start(root, action));
    }
    fn poll_cargo(&mut self) {
        let Some(task) = self.cargo_task.as_ref() else {
            return;
        };
        if self.root.as_ref() != Some(&task.root) {
            task.cancel();
        }
        let mut finished = false;
        for event in task.poll() {
            match event {
                cargo_runner::Event::Log(line) => {
                    self.cargo_log.push(line);
                    if self.cargo_log.len() > 200 {
                        self.cargo_log.drain(0..self.cargo_log.len() - 200);
                    }
                }
                cargo_runner::Event::Diagnostic(diagnostic) => {
                    if self.cargo_diagnostics.len() < 200 {
                        self.cargo_diagnostics.push(diagnostic);
                    }
                }
                cargo_runner::Event::Finished(status) => {
                    self.cargo_status = status;
                    finished = true;
                }
            }
        }
        if finished {
            self.cargo_task = None;
        }
    }
    fn jump_to_cargo_diagnostic(&mut self, diag: cargo_runner::Diagnostic) {
        if self.dirty {
            self.status = "请先保存当前修改，再跳转诊断目标文件。".into();
            return;
        }
        if self.file.as_ref() == Some(&diag.path) {
            let span = diagnostic_location(&self.rust, diag.line, diag.column);
            self.editor_jump = Some(span);
            self.mode = Mode::Rust;
        } else {
            self.cargo_diag_target = Some((diag.path.clone(), diag.line, diag.column));
            self.open_file(diag.path);
        }
    }

    fn close_requires_confirmation(&self) -> bool {
        self.dirty || self.saving.is_some()
    }
    fn open(&mut self, path: PathBuf) {
        if self.dirty || self.saving.is_some() {
            self.pending_open = Some(path);
            return;
        }
        self.open_file(path);
    }

    fn open_file(&mut self, path: PathBuf) {
        // Reading from slow disks/network mounts never blocks the rendering thread.
        self.io_revision += 1;
        let id = self.io_revision;
        self.opening = Some((id, path.clone(), self.revision));
        self.opening_started = Some(std::time::Instant::now());
        self.document_io
            .get_or_insert_with(document_io::DocumentIo::new)
            .open(id, path);
        self.status = "正在后台读取 Rust 文件…".into();
    }

    fn complete_open(&mut self, path: PathBuf, loaded: document_io::Loaded) {
        if self.discard_on_open {
            self.discard_current_recovery();
            self.discard_on_open = false;
        }
        self.disk_stamp = loaded.stamp;
        self.external_change = false;
        self.saved_source = loaded.source.clone();
        self.rust = loaded.source;
        self.dirty = false;
        self.update_projection();
        self.file = Some(path.clone());
        self.record_recent_file();
        self.selected = None;
        self.jump_to = None;
        self.editor_find = None;
        self.editor_jump = None;
        self.editor_page_start = 0;
        self.editor_page_target = 1;
        self.editor_scroll_override = None;
        self.scroll_follow = None;
        self.last_scroll = None;
        self.mode = self.recovered_mode.take().unwrap_or(Mode::Java);
        if let Some(store) = self.state_store.as_ref() {
            self.pending_recovery = store
                .load_journal(&path)
                .filter(|journal| journal.edited != self.rust);
        }
        if self.cursor_byte <= self.rust.len() && self.rust.is_char_boundary(self.cursor_byte) {
            self.editor_jump = Some(parser::ByteSpan {
                start: self.cursor_byte,
                end: self.cursor_byte,
            });
        }
        if let Some((target, span)) = self.symbol_target.take()
            && target == path
        {
            self.mode = Mode::Rust;
            self.editor_jump = Some(span);
        }
        if let Some((target, line, column)) = self.cargo_diag_target.take()
            && target == path
        {
            let span = diagnostic_location(&self.rust, line, column);
            self.editor_jump = Some(span);
            self.mode = Mode::Rust;
        }
        if let Some((target, pos)) = self.lsp_target.take()
            && target == path
            && let Some(byte) = lsp::byte_for_position(&self.rust, &pos)
        {
            self.editor_jump = Some(parser::ByteSpan {
                start: byte,
                end: byte,
            });
            self.cursor_byte = byte;
            self.mode = Mode::Rust;
        }
        if let Some(task) = self.lsp_task.as_ref()
            && let Ok(path) = path.canonicalize()
        {
            task.open(path, self.rust.clone(), self.revision);
        }
        self.status = "Opened actual Rust file. Java teaching view is read-only.".into();
    }

    fn poll_document_io(&mut self, ctx: &egui::Context) {
        while let Some(reply) = self
            .document_io
            .as_ref()
            .and_then(document_io::DocumentIo::take)
        {
            match reply {
                document_io::Reply::Open { id, path, result } => {
                    let matching = self.opening.as_ref().is_some_and(|(pending, p, rev)| {
                        *pending == id && p == &path && *rev == self.revision
                    });
                    if matching {
                        self.opening = None;
                        self.opening_started = None;
                        match result {
                            Ok(loaded) => self.complete_open(path, loaded),
                            Err(error) => {
                                self.discard_on_open = false;
                                self.status = format!("Open failed: {error}");
                            }
                        }
                    }
                }
                document_io::Reply::Save {
                    id,
                    path,
                    content,
                    result,
                } => {
                    if !self.saving.as_ref().is_some_and(|(pending, p, snapshot)| {
                        *pending == id && *p == path && *snapshot == content
                    }) {
                        continue;
                    }
                    self.saving = None;
                    self.saving_started = None;
                    match result {
                        Ok(outcome) => {
                            let durable = matches!(&outcome, safe_save::SaveOutcome::Durable);
                            self.saved_source = content;
                            self.dirty = self.rust != self.saved_source;
                            self.disk_stamp = file_watch::file_stamp(&path).ok();
                            if let Some(watch) = self.file_watch.as_mut() {
                                watch.set_target(path.clone(), self.disk_stamp.clone());
                            }
                            self.external_change = false;
                            self.flush_recovery_journal();
                            self.status = match outcome {
                                safe_save::SaveOutcome::Durable => {
                                    "Saved original Rust source".into()
                                }
                                safe_save::SaveOutcome::CommittedDurabilityUncertain(error) => {
                                    format!("内容已写入磁盘，但目录同步失败，持久性未确认：{error}")
                                }
                            };
                            if !durable {
                                // The content crossed the commit point, but the user
                                // must explicitly acknowledge uncertain directory durability.
                                if self.after_save_close {
                                    self.confirm_close = true;
                                }
                                self.after_save_close = false;
                                self.after_save_open = false;
                            }
                            if !self.dirty && durable && self.after_save_close {
                                self.after_save_close = false;
                                self.confirm_close = false;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            if !self.dirty && durable && self.after_save_open {
                                self.after_save_open = false;
                                if let Some(next) = self.pending_open.take() {
                                    self.open_file(next);
                                }
                            }
                            if self.dirty {
                                self.after_save_close = false;
                                self.after_save_open = false;
                                self.status.push_str("；保存过程中又有新修改，请再次保存。");
                            }
                        }
                        Err(error) => {
                            self.after_save_close = false;
                            self.after_save_open = false;
                            self.status =
                                format!("Save failed (original source preserved): {error}");
                        }
                    }
                }
            }
        }
    }

    #[cfg(test)]
    fn wait_for_document_io(&mut self) {
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let ctx = egui::Context::default();
        while self.opening.is_some() || self.saving.is_some() {
            self.poll_document_io(&ctx);
            assert!(
                std::time::Instant::now() < limit,
                "Document I/O did not complete"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn reload_disk_copy(&mut self) {
        if let Some(path) = self.file.clone() {
            self.open(path);
        }
    }

    fn update_projection(&mut self) {
        self.editor_line_starts = parser::line_starts(&self.rust);
        self.schedule_projection_update();
    }
    fn source_edited(&mut self, previous: std::ops::Range<usize>, replacement: &str) {
        line_index::apply_line_patch(&mut self.editor_line_starts, previous, replacement);
        self.schedule_projection_update();
    }
    fn schedule_projection_update(&mut self) {
        self.revision += 1;
        self.projection_pending = true;
        self.selected = None;
        self.jump_to = None;
        self.projection_due =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(200));
    }

    fn save(&mut self) {
        if self.saving.is_some() {
            self.status = "保存任务已经在后台运行，请勿重复提交。".into();
            return;
        }
        let Some(path) = self.file.clone() else {
            self.status = "Example is read-only; open a real .rs file to save.".into();
            return;
        };
        self.io_revision += 1;
        let id = self.io_revision;
        let content = self.rust.clone();
        self.document_io
            .get_or_insert_with(document_io::DocumentIo::new)
            .save(id, path.clone(), self.saved_source.clone(), content.clone());
        self.saving = Some((id, path, content));
        self.saving_started = Some(std::time::Instant::now());
        self.status = "正在后台安全保存 Rust 文件…".into();
    }

    fn directory(&mut self, ui: &mut egui::Ui, dir: &Path, depth: usize) {
        if depth > 24 {
            ui.small("目录深度超过 24 层，请直接使用「Open folder」进入此目录。");
            return;
        }
        match self.dir_index.get_or_request(dir.to_path_buf()) {
            file_tree::Snapshot::Loading => {
                ui.small("正在后台读取真实目录…");
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(90));
            }
            file_tree::Snapshot::Error(error) => {
                ui.colored_label(Color32::YELLOW, format!("无法读取目录: {error}"));
            }
            file_tree::Snapshot::Ready(entries) => {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs(3));
                let total = entries
                    .iter()
                    .filter(|entry| {
                        self.show_excluded
                            || !matches!(entry.name.as_str(), ".git" | "target" | "node_modules")
                    })
                    .count();
                // A user must always be able to see every meaningful top-level
                // directory without a pagination click. Files beyond the page
                // limit remain available through explicit "show more".
                let root_dirs = if depth == 0 {
                    entries
                        .iter()
                        .filter(|entry| {
                            (self.show_excluded
                                || !matches!(
                                    entry.name.as_str(),
                                    ".git" | "target" | "node_modules"
                                ))
                                && entry.kind == file_tree::EntryKind::Directory
                        })
                        .count()
                } else {
                    0
                };
                let default_limit = root_dirs.max(180);
                let limit = self
                    .directory_limits
                    .get(dir)
                    .copied()
                    .unwrap_or(default_limit);
                let show_excluded = self.show_excluded;
                for entry in entries
                    .iter()
                    .filter(|entry| {
                        show_excluded
                            || !matches!(entry.name.as_str(), ".git" | "target" | "node_modules")
                    })
                    .take(limit)
                {
                    match entry.kind {
                        file_tree::EntryKind::Directory => {
                            egui::CollapsingHeader::new(format!("▸ {}", entry.name))
                                .id_salt(&entry.path)
                                .show(ui, |ui| self.directory(ui, &entry.path, depth + 1));
                        }
                        file_tree::EntryKind::RustFile => {
                            if ui
                                .selectable_label(
                                    self.file.as_ref() == Some(&entry.path),
                                    format!("  {}", entry.name),
                                )
                                .clicked()
                            {
                                self.open(entry.path.clone());
                            }
                        }
                        file_tree::EntryKind::OtherFile => {
                            ui.label(egui::RichText::new(format!("  {}", entry.name)).weak())
                                .on_hover_text("真实文件；当前版本只编辑 Rust .rs 文件");
                        }
                        file_tree::EntryKind::Symlink => {
                            ui.label(
                                egui::RichText::new(format!("  ↗ {}（符号链接）", entry.name))
                                    .weak(),
                            )
                            .on_hover_text("为了避免目录循环，不自动递归进入符号链接");
                        }
                    }
                }
                if total > limit
                    && ui
                        .button(format!("显示更多目录项（已显示 {limit} / {total}）"))
                        .clicked()
                {
                    self.directory_limits
                        .insert(dir.to_path_buf(), limit.saturating_add(180));
                }
            }
        }
    }

    fn refresh_file_search(&mut self) {
        self.file_search_revision += 1;
        self.search_results.clear();
        let Some(root) = self.root.clone() else {
            self.file_search_pending = false;
            return;
        };
        let query = self.file_query.trim().to_lowercase();
        if query.is_empty() {
            self.file_search_pending = false;
            return;
        }
        self.file_search_pending = true;
        self.file_search_worker
            .get_or_insert_with(file_search_worker::FileSearchWorker::new)
            .submit(self.file_search_revision, root, query, self.show_excluded);
    }

    fn update_file_search_results(&mut self) {
        if let Some((revision, results)) = self
            .file_search_worker
            .as_ref()
            .and_then(|worker| worker.take())
            && revision == self.file_search_revision
        {
            self.search_results = results;
            self.file_search_pending = false;
        }
    }

    fn record_recent_file(&mut self) {
        let Some(path) = self.file.clone() else {
            return;
        };
        self.recent_files.retain(|recent| recent != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(8);
    }

    fn check_external_change(&mut self) {
        let Some(path) = self.file.as_ref() else {
            return;
        };
        let current = file_stamp(path).ok();
        let changed = current != self.disk_stamp;
        if changed && !self.external_change {
            self.status =
                "File changed outside RustBridge; save is blocked until you reopen it.".into();
        }
        self.external_change = changed;
    }

    fn check_analyzer(&mut self) {
        if self.analyzer_check.is_none() {
            self.analyzer_status = "Checking rust-analyzer in the background…".into();
            self.analyzer_check = Some(analyzer::check_async());
        }
    }

    fn start_analyzer_diagnostics(&mut self) {
        let Some(root) = self.root.clone().filter(|_| self.cargo_project) else {
            self.analyzer_diagnostics_status =
                "Open a Cargo project before requesting diagnostics.".into();
            return;
        };
        if self.dirty
            && self
                .file
                .as_ref()
                .is_some_and(|path| path.starts_with(&root))
        {
            self.analyzer_diagnostics_status =
                "当前 Rust 文件有未保存修改：请先保存；诊断工具读取的是磁盘内容。".into();
            return;
        }
        self.analyzer_diagnostics_status =
            format!("正在诊断 {}（仅使用磁盘文件；超时 45 秒）…", root.display());
        self.analyzer_diagnostics = Some(analyzer::DiagnosticTask::start(root));
    }

    fn select(&mut self, index: usize, from_java_only: bool) {
        if let Some(anchor) = self.projection.anchors.get(index) {
            self.selected = Some(index);
            self.jump_to = Some(index);
            self.scroll_follow = None;
            self.last_scroll = None;
            self.status = format!(
                "Linked {} · Rust L{}:{} ↔ Java L{}:{} (same AST occurrence)",
                anchor.symbol,
                anchor.rust_at.line + 1,
                anchor.rust_at.column + 1,
                anchor.java_at.line + 1,
                anchor.java_at.column + 1
            );
            if from_java_only && self.mode == Mode::Java {
                self.mode = Mode::Compare;
            }
        }
    }

    fn lesson_for_topic(topic: &str) -> Option<usize> {
        let rust = match topic {
            "class ↔ struct" | "Struct declaration" => "struct + impl",
            "interface ↔ trait" | "Trait declaration" => "trait",
            "Option<T> ↔ Optional<T>" => "Option<T>",
            "Result<T,E> ↔ explicit errors" => "Result<T,E>",
            "Vec<T> ↔ ArrayList<T>" => "Vec<T>",
            "HashMap<K,V> ↔ Map<K,V>" => "HashMap<K,V>",
            "Reference and borrowing" | "move closure capture" => "Ownership + Borrowing",
            _ => return None,
        };
        lessons::LESSONS
            .iter()
            .position(|lesson| lesson.rust == rust)
    }

    fn read_only(&mut self, ui: &mut egui::Ui, java: bool) {
        let jump = self.jump_to.take();
        let result = if java {
            ui.colored_label(Color32::YELLOW,
                "Java 心智阅读模式 · 点击蓝色标记查看实际 Rust；这不是可编译 Java，复杂语义以 Rust 原文为准。");
            code_view::show(
                ui,
                &self.projection.java,
                &self.projection,
                code_view::ViewOptions {
                    id: "java_only",
                    side: code_view::Side::Java,
                    selected: self.selected,
                    jump_to: jump,
                    scroll_to: None,
                },
            )
        } else {
            code_view::show(
                ui,
                &self.rust,
                &self.projection,
                code_view::ViewOptions {
                    id: "rust_only",
                    side: code_view::Side::Rust,
                    selected: self.selected,
                    jump_to: jump,
                    scroll_to: None,
                },
            )
        };
        if let Some(index) = result.clicked {
            self.select(index, java);
        }
    }

    fn compare(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                "Click a highlighted token in either pane to inspect the SAME AST occurrence.",
            );
            ui.checkbox(&mut self.sync_scroll, "Link scrolling");
        });
        ui.separator();
        let jump = self.jump_to.take();
        let pending = self.scroll_follow.take();
        let (mut left, mut right) = (None, None);
        ui.columns(2, |columns| {
            columns[0].strong("☕ Java learning projection · read-only");
            columns[1].strong("🦀 Original Rust · read-only");
            left = Some(code_view::show(
                &mut columns[0],
                &self.projection.java,
                &self.projection,
                code_view::ViewOptions {
                    id: "linked_java",
                    side: code_view::Side::Java,
                    selected: self.selected,
                    jump_to: jump,
                    scroll_to: pending.filter(|(rust, _)| !*rust).map(|(_, y)| y),
                },
            ));
            right = Some(code_view::show(
                &mut columns[1],
                &self.rust,
                &self.projection,
                code_view::ViewOptions {
                    id: "linked_rust",
                    side: code_view::Side::Rust,
                    selected: self.selected,
                    jump_to: jump,
                    scroll_to: pending.filter(|(rust, _)| *rust).map(|(_, y)| y),
                },
            ));
        });
        if let (Some(left), Some(right)) = (left, right) {
            if let Some(index) = left.clicked.or(right.clicked) {
                self.select(index, false);
            } else if self.sync_scroll
                && jump.is_none()
                && pending.is_none()
                && let Some((prev_left, prev_right)) = self.last_scroll
            {
                // Detect wheel, touchpad, drag and scrollbar changes, not only wheel events.
                let left_delta = (left.offset_y - prev_left).abs();
                let right_delta = (right.offset_y - prev_right).abs();
                if left_delta > 0.5 && (left.hovered || right_delta <= 0.5) {
                    self.scroll_follow = Some((true, left.offset_y));
                } else if right_delta > 0.5 && (right.hovered || left_delta <= 0.5) {
                    self.scroll_follow = Some((false, right.offset_y));
                }
            }
            self.last_scroll = Some((left.offset_y, right.offset_y));
        }
    }

    fn lessons(&mut self, ui: &mut egui::Ui) {
        let lesson = &lessons::LESSONS[self.lesson];
        let chinese = &lessons::CHINESE_LESSONS[self.lesson];
        ui.heading(format!(
            "Lesson {} / {}: {} → {}",
            self.lesson + 1,
            lessons::LESSONS.len(),
            lesson.java,
            lesson.rust
        ));
        ui.strong("核心区别 / Key difference");
        ui.label(chinese.difference);
        ui.small(lesson.why);
        ui.small("Examples are teaching sketches. Check the real Rust source and compiler before treating them as production code.");
        ui.separator();
        ui.columns(2, |columns| {
            columns[0].heading("Java 8 mental model");
            columns[0].monospace(lesson.java_code);
            columns[1].heading("Rust example");
            columns[1].monospace(lesson.rust_code);
        });
        ui.separator();
        ui.strong("Java 程序员常见误区 / Common mistake");
        ui.label(chinese.mistake);
        ui.small(lesson.common_mistake);
        ui.separator();
        ui.strong("交互练习 · 选择正确答案");
        let quiz = &lesson_quiz::QUIZZES[self.lesson];
        ui.label(quiz.prompt);
        for (index, choice) in quiz.choices.iter().enumerate() {
            if ui
                .selectable_label(
                    self.quiz_choice == Some(index),
                    format!("{} · {}", (b'A' + index as u8) as char, choice),
                )
                .clicked()
            {
                self.quiz_choice = Some(index);
                if index == quiz.correct {
                    self.completed_lessons[self.lesson] = true;
                    self.status = format!("第 {} 课掌握完成，学习进度已保存。", self.lesson + 1);
                }
            }
        }
        if let Some(chosen) = self.quiz_choice {
            ui.colored_label(
                if chosen == quiz.correct {
                    Color32::GREEN
                } else {
                    Color32::YELLOW
                },
                if chosen == quiz.correct {
                    "回答正确 ✓"
                } else {
                    "还需巩固，看看下面的原理再试一次。"
                },
            );
            ui.small(quiz.explanation);
        }
        let completed = self.completed_lessons.iter().filter(|&&done| done).count();
        ui.small(format!(
            "已掌握 {completed}/{} 个核心概念",
            lessons::LESSONS.len()
        ));
        ui.separator();
        ui.horizontal(|ui| {
            if let Some((mode, anchor)) = self.lesson_return
                && ui.button("← Back to code").clicked()
            {
                self.mode = mode;
                self.jump_to = anchor;
                self.lesson_return = None;
            }
            if ui
                .add_enabled(self.lesson > 0, egui::Button::new("← Previous"))
                .clicked()
            {
                self.lesson -= 1;
                self.quiz_choice = None;
            }
            if ui
                .add_enabled(
                    self.lesson + 1 < lessons::LESSONS.len(),
                    egui::Button::new("Next →"),
                )
                .clicked()
            {
                self.lesson += 1;
                self.quiz_choice = None;
            }
        });
    }

    fn right_guide(&mut self, ui: &mut egui::Ui) {
        ui.heading("Java ↔ Rust 概念说明");
        ui.small("AST 精确对应位置；并非类型解析或可编译 Java 转换");
        ui.separator();
        if self.projection_pending {
            ui.label("Updating projection; links are temporarily unavailable.");
            return;
        }
        let insights: Vec<_> = self
            .projection
            .java_insights
            .iter()
            .take(60)
            .cloned()
            .collect();
        egui::CollapsingHeader::new(format!(
            "Java 结构导览（{} 项）",
            self.projection.java_insights.len()
        ))
        .default_open(true)
        .show(ui, |ui| {
            ui.small(
                "把真实 struct / trait / impl 翻译成 Java 开发者熟悉的结构语言，不猜测类型身份。",
            );
            for item in insights {
                if ui
                    .selectable_label(false, &item.headline)
                    .on_hover_text(item.explanation)
                    .clicked()
                    && let Some(idx) = self.projection.rust_anchor(item.rust_byte)
                {
                    self.select(idx, self.mode == Mode::Java);
                }
            }
            if self.projection.java_insights.len() > 60 {
                ui.small("结构较多，列表只展示前 60 项；其余可以通过源码锚点访问。");
            }
        });
        ui.separator();
        if self.projection.warning {
            ui.colored_label(
                Color32::YELLOW,
                "Syntax incomplete: only recognized tokens are linked.",
            );
        }
        if let Some(idx) = self.selected
            && let Some(anchor) = self.projection.anchors.get(idx).cloned()
        {
            ui.strong(format!("{} · {}", anchor.kind, anchor.symbol));
            ui.colored_label(Color32::from_rgb(126, 188, 247), anchor.topic);
            ui.label(anchor.explanation);
            ui.separator();
            ui.monospace(format!(
                "Rust  L{}:{}  →  Java  L{}:{}",
                anchor.rust_at.line + 1,
                anchor.rust_at.column + 1,
                anchor.java_at.line + 1,
                anchor.java_at.column + 1
            ));
            ui.monospace(format!(
                "Rust token: {}",
                &self.rust[anchor.rust.start..anchor.rust.end]
            ));
            ui.monospace(format!(
                "Java token: {}",
                &self.projection.java[anchor.java.start..anchor.java.end]
            ));
            ui.horizontal(|ui| {
                if ui.button("View pair").clicked() {
                    self.mode = Mode::Compare;
                    self.jump_to = Some(idx);
                }
                if ui.button("Edit Rust").clicked() {
                    self.mode = Mode::Rust;
                    self.jump_to = Some(idx);
                }
                if let Some(lesson) = Self::lesson_for_topic(anchor.topic)
                    && ui.button("Open lesson").clicked()
                {
                    self.lesson_return = Some((self.mode, Some(idx)));
                    self.lesson = lesson;
                    self.mode = Mode::Learn;
                }
            });
            if let Some(root) = self.root.clone()
                && ui
                    .add_enabled(
                        !self.symbol_pending,
                        egui::Button::new("在项目中查找同名声明（仅语法匹配）"),
                    )
                    .clicked()
            {
                self.symbol_revision += 1;
                self.symbol_query = anchor.symbol;
                self.symbol_pending = true;
                self.symbol_hits.clear();
                self.symbol_worker
                    .get_or_insert_with(symbol_search::SymbolSearch::new)
                    .submit(self.symbol_revision, root, self.symbol_query.clone());
            }
        } else {
            ui.label(
                "Choose a highlighted symbol in Java or Rust to see its exact paired occurrence.",
            );
        }
        if self.symbol_pending {
            ui.spinner();
            ui.small("正在后台查找真实 Rust 声明…");
        }
        if !self.symbol_hits.is_empty() {
            ui.separator();
            ui.strong(format!(
                "同名声明候选：{}（{} 项）",
                self.symbol_query,
                self.symbol_hits.len()
            ));
            ui.small("仅 AST 同名声明，不推断类型、模块可见性或真实定义归属。");
            let hits: Vec<_> = self.symbol_hits.iter().take(35).cloned().collect();
            egui::ScrollArea::vertical()
                .max_height(165.)
                .show(ui, |ui| {
                    for hit in hits {
                        let label = format!(
                            "{} — {} :{} [{}]",
                            hit.name,
                            hit.path.file_name().unwrap_or_default().to_string_lossy(),
                            hit.line + 1,
                            hit.kind
                        );
                        if ui
                            .selectable_label(false, label)
                            .on_hover_text(hit.path.display().to_string())
                            .clicked()
                        {
                            let span = parser::ByteSpan {
                                start: hit.byte_start,
                                end: hit.byte_end,
                            };
                            if self.file.as_ref() == Some(&hit.path) {
                                self.mode = Mode::Rust;
                                self.editor_jump = Some(span);
                            } else {
                                self.symbol_target = Some((hit.path.clone(), span));
                                self.open(hit.path);
                            }
                        }
                    }
                });
        }
        ui.separator();
        ui.strong("Outline · declarations / concepts");
        let outline: Vec<_> = self
            .projection
            .anchors
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                matches!(a.kind, "method" | "field" | "trait" | "concept")
                    || (a.kind == "type" && a.topic == "Struct declaration")
            })
            .take(250)
            .map(|(idx, anchor)| (idx, anchor.rust_at.line, anchor.symbol.clone(), anchor.kind))
            .collect();
        egui::ScrollArea::vertical()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                for (idx, line, name, kind) in outline {
                    if ui
                        .selectable_label(
                            self.selected == Some(idx),
                            format!("L{}  {} · {}", line + 1, name, kind),
                        )
                        .clicked()
                    {
                        self.select(idx, self.mode == Mode::Java);
                    }
                }
            });
    }

    fn body(&mut self, ui: &mut egui::Ui) {
        if self.opening.is_some() {
            ui.spinner();
            ui.label("文件正在后台读取；当前源码保持原状。");
            if let Some(started) = self.opening_started {
                let elapsed = started.elapsed();
                if elapsed > std::time::Duration::from_secs(20) {
                    ui.colored_label(
                        Color32::YELLOW,
                        "读取耗时较长；网络文件系统调用可能不支持强制中断。",
                    );
                }
                ui.label(format!("已等待 {} 秒", elapsed.as_secs()));
            }
            if ui.button("取消读取并保留当前文件").clicked() {
                if let Some(io) = self.document_io.as_ref() {
                    io.cancel_open();
                }
                self.opening = None;
                self.opening_started = None;
                self.discard_on_open = false;
                self.status = "已取消当前打开请求；旧文件和未保存编辑保持不变。".into();
            }
            return;
        }
        if self.projection_pending && matches!(self.mode, Mode::Java | Mode::Compare) {
            ui.label("Updating projection in the background…");
            return;
        }
        match self.mode {
            Mode::Java => self.read_only(ui, true),
            Mode::Rust => self.editor(ui),
            Mode::Compare => self.compare(ui),
            Mode::Learn => self.lessons(ui),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_document_io(ctx);
        self.poll_cargo();
        self.poll_lsp();
        // Watch real disk metadata off the event thread. Idle windows do not
        // repeatedly repaint just to poll file stamps.
        if let Some(path) = self.file.clone() {
            if let Some(watch) = self.file_watch.as_mut() {
                if watch.path() != path {
                    watch.set_target(path, self.disk_stamp.clone());
                }
            } else {
                self.file_watch = Some(file_watch::FileWatch::new(
                    path,
                    self.disk_stamp.clone(),
                    ctx.clone(),
                ));
            }
            if self
                .file_watch
                .as_ref()
                .is_some_and(|watch| watch.changed())
            {
                self.check_external_change();
            }
        } else {
            self.file_watch = None;
        }
        if self
            .projection_due
            .is_some_and(|due| std::time::Instant::now() >= due)
        {
            self.projection_due = None;
            self.worker
                .get_or_insert_with(projection_worker::ProjectionWorker::new)
                .submit(self.revision, self.rust.clone());
            if let (Some(task), Some(path)) = (self.lsp_task.as_ref(), self.file.as_ref())
                && let Ok(path) = path.canonicalize()
            {
                task.change(path, self.rust.clone(), self.revision);
            }
        }
        if let Some((revision, projection)) = self.worker.as_ref().and_then(|worker| worker.take())
            && revision == self.revision
        {
            self.projection = projection;
            self.projection_pending = false;
            self.status = "Reading projection updated.".into();
        }
        self.update_file_search_results();
        if let Some((rev, hits)) = self
            .symbol_worker
            .as_ref()
            .and_then(symbol_search::SymbolSearch::take)
            && rev == self.symbol_revision
        {
            self.symbol_hits = hits;
            self.symbol_pending = false;
        }
        if let Some(result) = self
            .analyzer_check
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.analyzer_status = result;
            self.analyzer_check = None;
        }
        // Diagnostic results belong to the project that launched the task.
        if self
            .analyzer_diagnostics
            .as_ref()
            .is_some_and(|task| self.root.as_ref() != Some(task.project_root()))
        {
            if let Some(task) = self.analyzer_diagnostics.take() {
                task.cancel();
            }
            self.analyzer_diagnostics_status = "项目已切换，旧诊断已取消。".into();
        }
        if let Some(result) = self
            .analyzer_diagnostics
            .as_ref()
            .and_then(|task| task.take_result())
        {
            self.analyzer_diagnostics_status = result;
            self.analyzer_diagnostics = None;
        }
        if self.lsp_task.as_ref().is_some_and(|_| !self.lsp_ready)
            || self.cargo_task.is_some()
            || self.opening.is_some()
            || self.saving.is_some()
            || self.symbol_pending
            || self.projection_pending
            || self.file_search_pending
            || self.analyzer_diagnostics.is_some()
            || self.analyzer_check.is_some()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.close_requires_confirmation() {
            if self.saving.is_some() {
                self.after_save_close = true;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if self.confirm_close {
            egui::Modal::new(egui::Id::new("confirm_close")).show(ctx, |ui| {
                let durability_uncertain = self.status.contains("持久性未确认");
                ui.heading(if durability_uncertain {
                    "内容已提交，但目录持久性未确认"
                } else {
                    "Unsaved changes"
                });
                ui.label(&self.status);
                ui.horizontal(|ui| {
                    if self.dirty
                        && ui
                            .add_enabled(self.saving.is_none(), egui::Button::new("Save and close"))
                            .clicked()
                    {
                        self.after_save_close = true;
                        self.save();
                    }
                    if durability_uncertain {
                        if ui.button("确认警告并关闭").clicked() {
                            self.confirm_close = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    } else if self.dirty
                        && ui
                            .add_enabled(
                                self.saving.is_none(),
                                egui::Button::new("Discard and close"),
                            )
                            .clicked()
                    {
                        self.discard_current_recovery();
                        self.dirty = false;
                        self.confirm_close = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Cancel").clicked() {
                        self.after_save_close = false;
                        self.confirm_close = false;
                    }
                });
            });
        }
        if self.pending_open.is_some() && !self.confirm_close {
            egui::Modal::new(egui::Id::new("confirm_open")).show(ctx, |ui| {
                ui.heading("Switch file with unsaved changes");
                ui.label("Save or discard your edits before switching files.");
                ui.label(&self.status);
                ui.horizontal(|ui| {
                    let durability_uncertain = self.status.contains("持久性未确认");
                    if durability_uncertain && !self.dirty {
                        if ui.button("继续打开（内容已写入，持久性未确认）").clicked()
                            && let Some(path) = self.pending_open.take()
                        {
                            self.open_file(path);
                        }
                    } else if self.dirty {
                        if ui
                            .add_enabled(self.saving.is_none(), egui::Button::new("Save and open"))
                            .clicked()
                        {
                            self.after_save_open = true;
                            self.save();
                        }
                        if ui
                            .add_enabled(
                                self.saving.is_none(),
                                egui::Button::new("Discard and open"),
                            )
                            .clicked()
                            && let Some(path) = self.pending_open.take()
                        {
                            self.discard_on_open = true;
                            self.open_file(path);
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_open = None;
                    }
                });
            });
        }
        if let Some(action) = self.cargo_confirm {
            egui::Modal::new(egui::Id::new("cargo_authorization")).show(ctx, |ui| {
                ui.heading(format!("确认执行 {}", action.label()));
                ui.label(
                    "Cargo 可能运行项目的 build.rs、测试代码或程序本身，并访问本机文件与网络。",
                );
                ui.label("只有在信任该项目时才继续；不会自动执行。");
                if let Some(root) = self.root.as_ref() {
                    ui.monospace(root.display().to_string());
                }
                ui.horizontal(|ui| {
                    if ui.button("我信任此项目，执行").clicked() {
                        self.cargo_confirm = None;
                        self.start_cargo_action(action);
                    }
                    if ui.button("取消").clicked() {
                        self.cargo_confirm = None;
                    }
                });
            });
        }
        if let Some(journal) = self.pending_recovery.clone() {
            egui::Modal::new(egui::Id::new("recover_unsaved")).show(ctx, |ui| {
                let unchanged = journal.baseline == self.saved_source;
                ui.heading("发现崩溃前未保存的 Rust 编辑");
                ui.label(format!("文件：{}", journal.path.display()));
                ui.label(if unchanged {
                    "原始磁盘内容与编辑前一致。可以安全恢复到编辑器，仍需手动保存。"
                } else {
                    "磁盘文件已变化！不能直接恢复覆盖。请先导出恢复副本，并自行比对。"
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(unchanged, egui::Button::new("恢复到编辑器"))
                        .clicked()
                    {
                        self.pending_recovery = None;
                        self.rust = journal.edited.clone();
                        self.update_projection();
                        self.dirty = true;
                        self.mark_dirty_for_recovery();
                        self.status = "已恢复未保存缓冲区；尚未覆盖磁盘文件。".into();
                    }
                    if ui.button("导出恢复副本").clicked() {
                        let destination = journal.path.with_extension("rustbridge-recovery.rs");
                        let result = self
                            .state_store
                            .as_ref()
                            .unwrap()
                            .export_recovery(&journal, &destination);
                        self.status = match result {
                            Ok(()) => format!("恢复副本已导出：{}", destination.display()),
                            Err(error) => format!("导出失败（不会覆盖）：{error}"),
                        };
                    }
                    if ui.button("忽略并清除快照").clicked() {
                        self.pending_recovery = None;
                        self.discard_current_recovery();
                    }
                });
            });
        }
        // Capture Ctrl+S before TextEdit can consume the event, but only save
        // AFTER the editor has applied all changes from this frame.
        let (save_shortcut, mode_shortcut, page_delta) = ctx.input(|i| {
            let mod_pressed = i.modifiers.ctrl || i.modifiers.command;
            let requested_mode = if !mod_pressed {
                None
            } else if i.key_pressed(egui::Key::Num1) {
                Some(Mode::Java)
            } else if i.key_pressed(egui::Key::Num2) {
                Some(Mode::Rust)
            } else if i.key_pressed(egui::Key::Num3) {
                Some(Mode::Compare)
            } else if i.key_pressed(egui::Key::Num4) {
                Some(Mode::Learn)
            } else {
                None
            };
            let page_delta = if !mod_pressed {
                0_i32
            } else if i.key_pressed(egui::Key::PageDown) {
                1
            } else if i.key_pressed(egui::Key::PageUp) {
                -1
            } else {
                0
            };
            (
                mod_pressed && i.key_pressed(egui::Key::S),
                requested_mode,
                page_delta,
            )
        });
        if let Some(mode) = mode_shortcut {
            self.mode = mode;
            self.last_scroll = None;
            self.jump_to = self.selected;
        }
        if page_delta != 0
            && self.mode == Mode::Rust
            && self.rust.len() >= editor_core::LARGE_EDITOR_BYTES
        {
            if page_delta > 0 {
                self.editor_page_start = (self.editor_page_start
                    + editor_core::EDITOR_PAGE_LINES / 2)
                    .min(self.editor_line_starts.len().saturating_sub(1));
            } else {
                self.editor_page_start = self
                    .editor_page_start
                    .saturating_sub(editor_core::EDITOR_PAGE_LINES);
            }
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("RustBridge");
                ui.separator();
                if ui.button("Open .rs").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Rust", &["rs"])
                        .pick_file()
                {
                    self.open(path);
                }
                if ui.button("Open folder").clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    if let Some(task)=self.lsp_task.take(){task.cancel();}
                    self.lsp_ready=false;
                    self.lsp_status="项目已切换；LSP 已停止，请明确重新启动。".into();
                    self.lsp_hover.clear();
                    self.lsp_completions.clear();
                    self.lsp_definitions.clear();
                    if let Some(task)=self.cargo_task.take(){task.cancel();}
                    self.cargo_diagnostics.clear();
                    self.cargo_log.clear();
                    self.cargo_status="新项目已打开；Cargo 任务需要重新授权。".into();
                    if let Some(task) = self.analyzer_diagnostics.take() { task.cancel(); }
                    self.analyzer_diagnostics_status = "已切换项目；原项目诊断已取消。".into();
                    self.dir_index = file_tree::DirectoryIndex::new();
                    self.directory_limits.clear();
                    self.symbol_revision+=1;
                    self.symbol_hits.clear();
                    self.symbol_pending=false;
                    self.cargo_project = is_cargo_root(&folder);
                    self.root = Some(folder);
                    self.refresh_file_search();
                    self.status = if self.cargo_project {
                        "Opened Cargo project directory. Rust source remains the only editable truth.".into()
                    } else {
                        "Opened directory without Cargo.toml. Browsing Rust files.".into()
                    };
                }
                ui.separator();
                for (mode, label) in [
                    (Mode::Java, "☕ Java 阅读"),
                    (Mode::Rust, "🦀 Rust 源码"),
                    (Mode::Compare, "◫ 双栏对照"),
                    (Mode::Learn, "分页学习"),
                ] {
                    if ui.selectable_label(self.mode == mode, label).clicked() {
                        self.mode = mode;
                        self.last_scroll = None;
                        self.jump_to = self.selected;
                    }
                }
                if self.dirty {
                    ui.colored_label(Color32::YELLOW, "● Unsaved");
                }
                if let Some(started)=self.saving_started{
                    ui.spinner();
                    ui.small(format!("安全保存中 {}s",started.elapsed().as_secs()));
                    if started.elapsed()>std::time::Duration::from_secs(20){
                        ui.colored_label(Color32::YELLOW,
                            "存储设备响应慢；后台保存不能安全强制终止，请勿退出。");
                    }
                }
                if self.external_change {
                    ui.colored_label(Color32::RED, "Disk changed outside the app");
                    if ui.button("Reload disk copy").clicked() {
                        self.reload_disk_copy();
                    }
                }
            });
        });
        egui::SidePanel::left("files")
            .resizable(true)
            .default_width(210.)
            .show(ctx, |ui| {
                ui.heading("真实项目文件 / Project");
                ui.small("目录直接来自文件系统；只编辑 .rs，其他文件仍显示。");
                let excluded_changed = ui
                    .checkbox(
                        &mut self.show_excluded,
                        "Show .git, target and node_modules",
                    )
                    .changed();
                ui.small("真实文件树；每页 180 项，可逐页展开，目录在后台读取。");
                if ui.button("刷新目录").clicked() {
                    self.dir_index = file_tree::DirectoryIndex::new();
                    self.directory_limits.clear();
                    self.refresh_file_search();
                }
                let changed = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.file_query)
                            .hint_text("Find Rust file…"),
                    )
                    .changed();
                if changed || excluded_changed {
                    self.refresh_file_search();
                }
                ui.separator();
                if let Some(root) = self.root.clone() {
                    ui.strong(root.file_name().unwrap_or_default().to_string_lossy());
                    ui.small(if self.cargo_project {
                        "Cargo.toml detected"
                    } else {
                        "No Cargo.toml at this directory root"
                    });
                    if !self.file_query.trim().is_empty() {
                        if self.file_search_pending {
                            ui.small("Searching project files in the background…");
                        } else {
                            ui.small(format!(
                                "{} result(s), limited to 100",
                                self.search_results.len()
                            ));
                        }
                        let results = self.search_results.clone();
                        egui::ScrollArea::vertical()
                            .max_height(180.)
                            .show(ui, |ui| {
                                for path in results {
                                    let label = path
                                        .strip_prefix(&root)
                                        .unwrap_or(&path)
                                        .display()
                                        .to_string();
                                    if ui
                                        .selectable_label(self.file.as_ref() == Some(&path), label)
                                        .clicked()
                                    {
                                        self.open(path);
                                    }
                                }
                            });
                        ui.separator();
                    }
                    egui::ScrollArea::vertical()
                        .max_height(300.)
                        .show(ui, |ui| self.directory(ui, &root, 0));
                } else {
                    ui.label("Open folder to browse the actual project.");
                }
                if !self.recent_files.is_empty() {
                    ui.separator();
                    ui.strong("Recent Rust files");
                    let recent = self.recent_files.clone();
                    for path in recent {
                        if ui
                            .selectable_label(
                                self.file.as_ref() == Some(&path),
                                path.display().to_string(),
                            )
                            .clicked()
                        {
                            self.open(path);
                        }
                    }
                }
                ui.separator();
                ui.strong("可选 rust-analyzer / 诊断");
                ui.small("手动触发；诊断磁盘中已保存的文件，不启动 LSP、补全或定义跳转。");
                ui.label(&self.analyzer_status);
                if ui
                    .add_enabled(
                        self.analyzer_check.is_none(),
                        egui::Button::new("Check availability"),
                    )
                    .clicked()
                {
                    self.check_analyzer();
                }
                ui.separator();
                ui.small(&self.analyzer_diagnostics_status);
                if let Some(task) = self.analyzer_diagnostics.as_ref() {
                    if ui.button("Cancel diagnostics").clicked() {
                        task.cancel();
                    }
                } else if ui
                    .add_enabled(
                        self.cargo_project && self.root.is_some(),
                        egui::Button::new("Check Cargo diagnostics"),
                    )
                    .clicked()
                {
                    self.start_analyzer_diagnostics();
                }
                ui.separator();
                ui.strong("rust-analyzer LSP · 按需启动");
                ui.small("真实 JSON-RPC：类型悬浮、补全、定义跳转；取消时终止会话。");
                ui.horizontal_wrapped(|ui| {
                    if self.lsp_task.is_none() {
                        if ui
                            .add_enabled(self.cargo_project, egui::Button::new("启动 LSP"))
                            .clicked()
                        {
                            self.start_lsp();
                        }
                    } else if ui.button("停止 LSP").clicked() {
                        if let Some(task) = self.lsp_task.take() {
                            task.cancel();
                        }
                        self.lsp_ready = false;
                        self.lsp_status = "rust-analyzer LSP 已停止。".into();
                    }
                    if self.lsp_ready {
                        for (kind, label) in [
                            (lsp::Kind::Hover, "类型提示"),
                            (lsp::Kind::Completion, "补全"),
                            (lsp::Kind::Definition, "跳转定义"),
                        ] {
                            if ui.button(label).clicked() {
                                self.lsp_query(kind);
                            }
                        }
                    }
                });
                ui.small(&self.lsp_status);
                if !self.lsp_hover.is_empty() {
                    egui::ScrollArea::vertical()
                        .max_height(120.)
                        .show(ui, |ui| {
                            ui.monospace(&self.lsp_hover);
                        });
                }
                if !self.lsp_completions.is_empty() {
                    let items = self.lsp_completions.clone();
                    ui.strong("可选补全（应用前请核对）");
                    egui::ScrollArea::vertical()
                        .max_height(120.)
                        .show(ui, |ui| {
                            for item in items {
                                if ui.selectable_label(false, &item.label).clicked() {
                                    self.insert_completion(item);
                                }
                            }
                        });
                }
                if !self.lsp_definitions.is_empty() {
                    let locations = self.lsp_definitions.clone();
                    ui.strong("真实定义位置");
                    for location in locations.into_iter().take(8) {
                        let label = format!(
                            "{}:{}:{}",
                            location.path.display(),
                            location.line + 1,
                            location.character + 1
                        );
                        if ui.selectable_label(false, label).clicked() {
                            self.jump_lsp_definition(location);
                        }
                    }
                }
                ui.separator();
                ui.strong("Cargo 工程 / Build · Test · Run");
                ui.small("手动确认后运行；cargo check 也可能执行 build.rs。");
                ui.horizontal_wrapped(|ui| {
                    for action in [
                        cargo_runner::Action::Check,
                        cargo_runner::Action::Test,
                        cargo_runner::Action::Run,
                    ] {
                        if ui
                            .add_enabled(
                                self.cargo_project
                                    && self.cargo_task.is_none()
                                    && !self.dirty
                                    && self.saving.is_none(),
                                egui::Button::new(action.label()),
                            )
                            .clicked()
                        {
                            self.cargo_confirm = Some(action);
                        }
                    }
                    if let Some(task) = self.cargo_task.as_ref()
                        && ui.button("取消任务").clicked()
                    {
                        task.cancel();
                    }
                });
                ui.small(&self.cargo_status);
                let diagnostics = self.cargo_diagnostics.clone();
                egui::ScrollArea::vertical()
                    .max_height(170.)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        for diag in diagnostics {
                            let label = format!(
                                "{} {}:{}:{} {}",
                                diag.level,
                                diag.path.file_name().unwrap_or_default().to_string_lossy(),
                                diag.line,
                                diag.column,
                                diag.message
                            );
                            if ui
                                .selectable_label(false, label)
                                .on_hover_text(diag.path.display().to_string())
                                .clicked()
                            {
                                self.jump_to_cargo_diagnostic(diag);
                            }
                        }
                        for line in self.cargo_log.iter().rev().take(25) {
                            ui.small(line);
                        }
                    });
                ui.separator();
                let complete_count = self.completed_lessons.iter().filter(|&&done| done).count();
                ui.strong(format!(
                    "Java 8 → Rust 课程 · {complete_count}/{}",
                    lessons::LESSONS.len()
                ));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (idx, lesson) in lessons::LESSONS.iter().enumerate() {
                        if ui
                            .selectable_label(
                                self.lesson == idx,
                                format!(
                                    "{} {:02} · {}",
                                    if self.completed_lessons.get(idx) == Some(&true) {
                                        "✓"
                                    } else {
                                        "·"
                                    },
                                    idx + 1,
                                    lesson.java
                                ),
                            )
                            .clicked()
                        {
                            self.lesson = idx;
                            self.quiz_choice = None;
                            self.mode = Mode::Learn;
                        }
                    }
                });
            });
        egui::SidePanel::right("explanation")
            .resizable(true)
            .default_width(290.)
            .show(ctx, |ui| {
                self.right_guide(ui);
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.small(&self.status);
                ui.separator();
                ui.small(format!(
                    "{} precise AST links",
                    self.projection.anchors.len()
                ));
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong(
                    self.file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "examples/basics.rs".into()),
                );
                ui.weak(format!(
                    " · {} lines · AST aligned",
                    self.editor_line_starts.len()
                ));
            });
            ui.separator();
            self.body(ui);
        });
        if save_shortcut {
            self.save();
            ctx.request_repaint();
        }
        self.poll_persistence();
        if self.recovery_due.is_some() || self.session_due.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn on_exit(&mut self, _gl: Option<&glow::Context>) {
        self.flush_shutdown_state();
    }
}

fn diagnostic_location(source: &str, line: usize, column: usize) -> parser::ByteSpan {
    let starts = parser::line_starts(source);
    let start = *starts.get(line.saturating_sub(1)).unwrap_or(&0);
    let end = *starts.get(line).unwrap_or(&source.len());
    let range = &source[start..end];
    let byte = range
        .char_indices()
        .nth(column.saturating_sub(1))
        .map_or(end, |(offset, _)| start + offset);
    parser::ByteSpan {
        start: byte,
        end: byte,
    }
}
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Java => "java",
        Mode::Rust => "rust",
        Mode::Compare => "compare",
        Mode::Learn => "learn",
    }
}
fn mode_from_name(value: &str) -> Mode {
    match value {
        "rust" => Mode::Rust,
        "compare" => Mode::Compare,
        "learn" => Mode::Learn,
        _ => Mode::Java,
    }
}
fn normalized_completion(list: &[bool]) -> Vec<bool> {
    (0..lessons::LESSONS.len())
        .map(|i| list.get(i).copied().unwrap_or(false))
        .collect()
}

fn is_cargo_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
}

fn main() -> eframe::Result {
    // Optional path is convenient for headless GUI acceptance and local workflows.
    // Only a real directory or existing Rust source file is accepted.
    let startup_path = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1320., 820.])
            .with_min_inner_size([800., 550.]),
        ..Default::default()
    };
    eframe::run_native(
        "RustBridge — Java 8 → Rust",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let _ = ui_fonts::install_system_cjk_fallback(&cc.egui_ctx);
            let mut app = App::default();
            app.initialize_state(startup_path.is_none());
            if let Some(path) = startup_path {
                if path.is_dir() {
                    app.cargo_project = is_cargo_root(&path);
                    app.root = Some(path);
                } else if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                    app.open_file(path);
                } else {
                    app.status = "参数必须是实际目录或可读的 .rs 文件。".into();
                }
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod file_tests {
    use super::*;
    use crate::editor_core::{append_rust_syntax, editor_page_bounds};

    #[test]
    fn restarting_restores_project_file_mode_lesson_cursor_and_progress() {
        let root =
            std::env::temp_dir().join(format!("rustbridge-session-root-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("main.rs");
        fs::write(&file, "fn main() {}").unwrap();
        let state_dir =
            std::env::temp_dir().join(format!("rustbridge-session-state-{}", std::process::id()));
        let store = persistence::Store::new(state_dir.clone()).unwrap();
        let session = persistence::Session {
            root: Some(root.clone()),
            file: Some(file.clone()),
            mode: "compare".into(),
            lesson: 6,
            cursor_byte: 3,
            completed_lessons: vec![true, false, true],
        };
        store.save_session(&session).unwrap();
        let mut app = App::default();
        app.initialize_state_from_store(store, true);
        app.wait_for_document_io();
        assert_eq!(app.root, Some(root.clone()));
        assert_eq!(app.file, Some(file));
        assert!(matches!(app.mode, Mode::Compare));
        assert_eq!(app.lesson, 6);
        assert_eq!(app.cursor_byte, 3);
        assert!(app.completed_lessons[0]);
        assert!(!app.completed_lessons[1]);
        assert!(app.completed_lessons[2]);
        drop(app);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(state_dir).unwrap();
    }

    #[test]
    fn graceful_shutdown_flushes_session_and_dirty_recovery_journal() {
        let state_dir =
            std::env::temp_dir().join(format!("rustbridge-app-drop-state-{}", std::process::id()));
        let source_dir =
            std::env::temp_dir().join(format!("rustbridge-app-drop-source-{}", std::process::id()));
        fs::create_dir_all(&source_dir).unwrap();
        let path = source_dir.join("main.rs");
        fs::write(&path, "disk source").unwrap();
        let store = persistence::Store::new(state_dir.clone()).unwrap();
        {
            let mut app = App {
                root: Some(source_dir.clone()),
                file: Some(path.clone()),
                saved_source: "disk source".into(),
                rust: "unsaved editor buffer".into(),
                dirty: true,
                mode: Mode::Rust,
                lesson: 5,
                cursor_byte: 4,
                state_store: Some(store.clone()),
                state_worker: Some(persistence::PersistenceWorker::spawn(store.clone())),
                ..App::default()
            };
            app.completed_lessons[2] = true;
            app.flush_shutdown_state();
        }
        let restored = store.load_session().unwrap();
        assert_eq!(restored.root, Some(source_dir.clone()));
        assert_eq!(restored.file, Some(path.clone()));
        assert_eq!(restored.mode, "rust");
        assert_eq!(restored.lesson, 5);
        assert_eq!(restored.cursor_byte, 4);
        assert!(restored.completed_lessons[2]);
        assert_eq!(
            store.load_journal(&path).unwrap().edited,
            "unsaved editor buffer"
        );
        fs::remove_dir_all(state_dir).unwrap();
        fs::remove_dir_all(source_dir).unwrap();
    }

    #[test]
    fn closing_a_background_save_always_requires_confirmation() {
        let mut app = App::default();
        assert!(!app.close_requires_confirmation());
        app.dirty = true;
        assert!(app.close_requires_confirmation());
        app.dirty = false;
        let path = std::env::temp_dir().join("rustbridge-lifecycle.rs");
        app.saving = Some((2, path, "new".into()));
        assert!(app.close_requires_confirmation());
    }
    #[test]
    fn failed_open_never_discards_the_last_dirty_buffer() {
        let mut app = App::default();
        app.rust.push_str("\n// unsaved");
        app.dirty = true;
        let before = app.rust.clone();
        app.discard_on_open = true;
        app.open_file(std::env::temp_dir().join("this-path-does-not-exist-failed-open.rs"));
        app.wait_for_document_io();
        assert_eq!(app.rust, before);
        assert!(app.dirty);
        assert!(!app.discard_on_open);
    }
    #[test]
    fn external_changes_and_deletion_block_save() {
        let path =
            std::env::temp_dir().join(format!("rustbridge-save-test-{}", std::process::id()));
        fs::write(&path, "original").unwrap();
        save_if_unchanged(&path, "original", "edited").unwrap();
        fs::write(&path, "external").unwrap();
        assert!(save_if_unchanged(&path, "edited", "mine").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
        fs::remove_file(&path).unwrap();
        assert!(save_if_unchanged(&path, "edited", "mine").is_err());
        assert!(!path.exists());
    }

    #[test]
    fn failed_discard_and_open_keeps_unsaved_buffer() {
        let mut app = App::default();
        app.rust.push_str("\n// unsaved");
        app.dirty = true;
        let previous = app.rust.clone();
        let missing =
            std::env::temp_dir().join(format!("rustbridge-missing-{}", std::process::id()));
        app.open_file(missing);
        app.wait_for_document_io();
        assert!(app.dirty);
        assert_eq!(app.rust, previous);
        assert!(app.status.starts_with("Open failed:"));
    }

    #[test]
    fn file_search_uses_real_paths_and_skips_build_output_by_default() {
        let root = std::env::temp_dir().join(format!("rustbridge-search-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("src/demo.rs"), "fn demo() {}").unwrap();
        fs::write(root.join("target/generated.rs"), "fn generated() {}").unwrap();
        let mut app = App {
            root: Some(root.clone()),
            file_query: "demo".into(),
            ..App::default()
        };
        app.refresh_file_search();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.file_search_pending {
            app.update_file_search_results();
            assert!(
                deadline > std::time::Instant::now(),
                "file search did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.search_results, vec![root.join("src/demo.rs")]);
        app.show_excluded = true;
        app.file_query = "generated".into();
        app.refresh_file_search();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.file_search_pending {
            app.update_file_search_results();
            assert!(
                deadline > std::time::Instant::now(),
                "file search did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.search_results, vec![root.join("target/generated.rs")]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clearing_file_search_discards_an_older_result() {
        let root =
            std::env::temp_dir().join(format!("rustbridge-clear-search-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("demo.rs"), "fn demo() {}").unwrap();
        let mut app = App {
            root: Some(root.clone()),
            file_query: "demo".into(),
            ..App::default()
        };
        app.refresh_file_search();
        app.file_query.clear();
        app.refresh_file_search();
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.update_file_search_results();
        assert!(app.search_results.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn known_ast_topics_open_the_matching_lesson() {
        let option = App::lesson_for_topic("Option<T> ↔ Optional<T>").unwrap();
        assert_eq!(lessons::LESSONS[option].rust, "Option<T>");
        let closure = App::lesson_for_topic("move closure capture").unwrap();
        assert_eq!(lessons::LESSONS[closure].rust, "Ownership + Borrowing");
        assert!(App::lesson_for_topic("Rust symbol occurrence").is_none());
    }

    #[test]
    fn every_lesson_has_a_common_mistake() {
        assert!(
            lessons::LESSONS
                .iter()
                .all(|lesson| !lesson.common_mistake.is_empty())
        );
        assert_eq!(lessons::LESSONS.len(), lessons::CHINESE_LESSONS.len());
        assert!(
            lessons::CHINESE_LESSONS
                .iter()
                .all(|lesson| !lesson.difference.is_empty() && !lesson.mistake.is_empty())
        );
    }

    #[test]
    fn external_file_stamp_is_detected_before_save() {
        let path = std::env::temp_dir().join(format!("rustbridge-stamp-{}", std::process::id()));
        fs::write(&path, "one").unwrap();
        let mut app = App {
            file: Some(path.clone()),
            disk_stamp: file_stamp(&path).ok(),
            ..App::default()
        };
        fs::write(&path, "two changed").unwrap();
        app.check_external_change();
        assert!(app.external_change);
        assert!(app.status.contains("changed outside"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reload_disk_copy_never_discards_unsaved_source_without_confirmation() {
        let path =
            std::env::temp_dir().join(format!("rustbridge-reload-{}.rs", std::process::id()));
        fs::write(&path, "disk version").unwrap();
        let mut app = App {
            file: Some(path.clone()),
            rust: "my unsaved version".into(),
            saved_source: "old version".into(),
            dirty: true,
            ..App::default()
        };
        app.reload_disk_copy();
        assert_eq!(app.rust, "my unsaved version");
        assert_eq!(app.pending_open, Some(path.clone()));
        app.dirty = false;
        app.pending_open = None;
        app.reload_disk_copy();
        app.wait_for_document_io();
        assert_eq!(app.rust, "disk version");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn cargo_root_detection_uses_the_real_manifest() {
        let root =
            std::env::temp_dir().join(format!("rustbridge-cargo-root-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        assert!(!is_cargo_root(&root));
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"sample\"").unwrap();
        assert!(is_cargo_root(&root));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recent_files_are_unique_and_bounded() {
        let mut app = App::default();
        for index in 0..10 {
            app.file = Some(PathBuf::from(format!("/tmp/rustbridge-{index}.rs")));
            app.record_recent_file();
        }
        assert_eq!(app.recent_files.len(), 8);
        assert_eq!(app.recent_files[0], PathBuf::from("/tmp/rustbridge-9.rs"));
        app.file = Some(PathBuf::from("/tmp/rustbridge-5.rs"));
        app.record_recent_file();
        assert_eq!(app.recent_files.len(), 8);
        assert_eq!(app.recent_files[0], PathBuf::from("/tmp/rustbridge-5.rs"));
    }

    #[test]
    fn editor_find_cycles_over_original_rust_text() {
        let mut app = App {
            rust: "fn one() {}\nfn two() {}\nfn one_again() {}".into(),
            editor_query: "one".into(),
            ..App::default()
        };
        app.find_next_in_editor();
        assert_eq!(app.editor_find, Some(parser::ByteSpan { start: 3, end: 6 }));
        app.find_next_in_editor();
        assert_eq!(
            app.editor_find,
            Some(parser::ByteSpan { start: 27, end: 30 })
        );
        app.find_next_in_editor();
        assert_eq!(app.editor_find, Some(parser::ByteSpan { start: 3, end: 6 }));
    }

    #[test]
    fn find_next_handles_relative_offsets_and_multibyte_text() {
        let mut app = App {
            rust: "foo foo".into(),
            editor_query: "foo".into(),
            ..App::default()
        };
        app.find_next_in_editor();
        assert_eq!(app.editor_find, Some(parser::ByteSpan { start: 0, end: 3 }));
        app.find_next_in_editor();
        assert_eq!(app.editor_find, Some(parser::ByteSpan { start: 4, end: 7 }));
        app.find_next_in_editor();
        assert_eq!(app.editor_find, Some(parser::ByteSpan { start: 0, end: 3 }));
        app.rust = "你好🦀 你好".into();
        app.editor_find = None;
        app.editor_query = "你好".into();
        app.find_next_in_editor();
        assert_eq!(app.editor_find.unwrap().start, 0);
        app.find_next_in_editor();
        assert_eq!(app.editor_find.unwrap().start, "你好🦀 ".len());
    }

    #[test]
    fn lexical_highlighter_keeps_source_bytes_and_marks_keywords() {
        let source = "fn main() { // note\nlet value = \"text\"; }";
        let mut job = egui::text::LayoutJob::default();
        append_rust_syntax(
            &mut job,
            source,
            &egui::FontId::monospace(12.),
            Color32::WHITE,
            Some(parser::ByteSpan { start: 3, end: 7 }),
        );
        let rendered: String = job
            .sections
            .iter()
            .map(|section| &source[section.byte_range.clone()])
            .collect();
        assert_eq!(rendered, source);
        assert!(
            job.sections
                .iter()
                .any(|section| section.format.color == Color32::from_rgb(126, 188, 247))
        );
        assert!(
            job.sections
                .iter()
                .any(|section| section.format.background == Color32::from_rgb(53, 103, 155))
        );
    }

    #[test]
    fn lexical_highlighter_recognizes_nested_comments_and_rust_literals() {
        let source = "/* outer /* inner */ done */\nlet raw = r#\"中文\"#;\nlet byte = b'a';";
        let mut job = egui::text::LayoutJob::default();
        append_rust_syntax(
            &mut job,
            source,
            &egui::FontId::monospace(12.),
            Color32::WHITE,
            None,
        );
        let rendered: String = job
            .sections
            .iter()
            .map(|section| &source[section.byte_range.clone()])
            .collect();
        assert_eq!(rendered, source);
        let comment = &source[..source.find('\n').unwrap()];
        assert!(job.sections.iter().any(|section| {
            &source[section.byte_range.clone()] == comment
                && section.format.color == Color32::from_rgb(112, 142, 112)
        }));
        assert!(job.sections.iter().any(|section| {
            &source[section.byte_range.clone()] == "r#\"中文\"#"
                && section.format.color == Color32::from_rgb(202, 162, 98)
        }));
        assert!(job.sections.iter().any(|section| {
            &source[section.byte_range.clone()] == "b'a'"
                && section.format.color == Color32::from_rgb(202, 162, 98)
        }));
    }

    #[test]
    fn page_bounds_preserve_crlf_utf8_and_trailing_lines() {
        let text = "你🦀\r\na\nb\n";
        let starts = parser::line_starts(text);
        assert_eq!(
            editor_page_bounds(&starts, 0, 1, text.len()),
            0..("你🦀\r\n".len())
        );
        assert_eq!(
            &text[editor_page_bounds(&starts, 1, 2, text.len())],
            "a\nb\n"
        );
        assert_eq!(&text[editor_page_bounds(&starts, 3, 1, text.len())], "");
    }

    #[test]
    fn line_number_gutter_preserves_physical_lines() {
        assert_eq!(line_number_gutter(""), "1");
        assert_eq!(
            line_number_gutter("let 中文 = 1;\r\nlet next = 2;\n"),
            "1\n2\n3"
        );
    }
}
