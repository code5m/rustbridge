mod analyzer;
mod cargo_runner;
mod code_view;
mod document_io;
mod document_manager;
mod editor_core;
mod editor_eol;
mod editor_ui;
mod file_search_worker;
mod file_tree;
mod file_watch;
mod learning_path;
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
mod ui_layout;

use editor_core::line_number_gutter;
use eframe::egui::{self, Color32};
use file_watch::FileStamp;
#[cfg(test)]
use file_watch::file_stamp;
#[cfg(test)]
use safe_save::save_if_unchanged;
#[cfg(test)]
use std::fs;
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Java,
    Rust,
    Compare,
    Learn,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarSection {
    Project,
    Structure,
    Search,
    Learn,
    Tools,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BottomTab {
    Problems,
    Build,
    Run,
    Output,
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
    sidebar_section: SidebarSection,
    show_left_sidebar: bool,
    show_right_guide: bool,
    show_bottom_panel: bool,
    bottom_tab: BottomTab,
    compare_split: f32,
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
    pending_project: Option<PathBuf>,
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
    documents: document_manager::DocumentManager,
    last_synced_path: Option<PathBuf>,
    last_synced_revision: u64,
    last_synced_cursor_byte: usize,
    last_synced_mode: Mode,
    last_synced_disk_stamp: Option<FileStamp>,
    last_synced_external_change: bool,
    restore_queue: VecDeque<PathBuf>,
    restore_states: HashMap<PathBuf, persistence::DocumentSession>,
    restore_active: Option<PathBuf>,
    orphan_recovery: Vec<persistence::RecoveryEntry>,
    orphan_recovery_offset: usize,
    recovery_scan_warning: Option<String>,
    recovery_export_receiver: Option<std::sync::mpsc::Receiver<String>>,
    startup_receiver: Option<std::sync::mpsc::Receiver<StartupResult>>,
    startup_restore_last_document: bool,
    startup_explicit_path: Option<PathBuf>,
    pending_close_document: Option<PathBuf>,
    save_queue: VecDeque<PathBuf>,
    save_errors: Vec<String>,
    io_revision: u64,
    opening: Option<(u64, PathBuf, u64)>,
    saving: Option<(u64, PathBuf, String)>,
    opening_started: Option<std::time::Instant>,
    saving_started: Option<std::time::Instant>,
    after_save_open: bool,
    after_save_project: bool,
    after_save_close: bool,
    discard_on_open: Option<PathBuf>,
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
    quiz_attempts: Vec<u32>,
    quiz_mistakes: Vec<u32>,
    practice_passed: Vec<bool>,
    practice_drafts: Vec<String>,
    practice_selected_stage: Option<usize>,
    practice_index: usize,
    practice_code: String,
    practice_confirm: bool,
    practice_task: Option<learning_path::PracticeTask>,
    practice_status: String,
    practice_diagnostics: Vec<cargo_runner::Diagnostic>,
    practice_log: Vec<String>,
    ownership_step: usize,
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
    pending_recovery: Option<std::sync::Arc<persistence::Journal>>,
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
    lsp_user_stopped: bool,
    lsp_hover: String,
    lsp_completions: Vec<lsp::Completion>,
    lsp_definitions: Vec<lsp::Location>,
    navigation_back: Vec<(PathBuf, lsp::SpanPosition)>,
    lsp_target: Option<(PathBuf, lsp::SpanPosition)>,
    editor_focused: bool,
    completion_due: Option<std::time::Instant>,
    completion_anchor: Option<egui::Pos2>,
    completion_selected: usize,
    completion_target: Option<(PathBuf, u64, u32, u32)>,
    definition_target: Option<(PathBuf, u64, u32, u32)>,
    hover_due: Option<std::time::Instant>,
    hover_target: Option<(PathBuf, u64, u32, u32)>,
    hover_pointer: Option<egui::Pos2>,
}

/// Startup disk I/O is deliberately kept off the egui render loop.
type StartupResult = Result<
    (
        persistence::Store,
        Option<persistence::Session>,
        persistence::RecoveryInventory,
    ),
    String,
>;

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
            sidebar_section: SidebarSection::Project,
            show_left_sidebar: true,
            show_right_guide: true,
            show_bottom_panel: false,
            bottom_tab: BottomTab::Problems,
            compare_split: 0.5,
            lesson: 0,
            selected: None,
            jump_to: None,
            sync_scroll: true,
            scroll_follow: None,
            last_scroll: None,
            dirty: false,
            confirm_close: false,
            pending_open: None,
            pending_project: None,
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
            documents: document_manager::DocumentManager::default(),
            last_synced_path: None,
            last_synced_revision: 0,
            last_synced_cursor_byte: 0,
            last_synced_mode: Mode::Java,
            last_synced_disk_stamp: None,
            last_synced_external_change: false,
            restore_queue: VecDeque::new(),
            restore_states: HashMap::new(),
            restore_active: None,
            orphan_recovery: Vec::new(),
            orphan_recovery_offset: 0,
            recovery_scan_warning: None,
            recovery_export_receiver: None,
            startup_receiver: None,
            startup_restore_last_document: false,
            startup_explicit_path: None,
            pending_close_document: None,
            save_queue: VecDeque::new(),
            save_errors: Vec::new(),
            io_revision: 0,
            opening: None,
            saving: None,
            opening_started: None,
            saving_started: None,
            after_save_open: false,
            after_save_project: false,
            after_save_close: false,
            discard_on_open: None,
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
            quiz_attempts: vec![0; lessons::LESSONS.len()],
            quiz_mistakes: vec![0; lessons::LESSONS.len()],
            practice_passed: vec![false; learning_path::EXERCISES.len()],
            practice_drafts: vec![String::new(); learning_path::EXERCISES.len()],
            practice_selected_stage: None,
            practice_index: 0,
            practice_code: String::new(),
            practice_confirm: false,
            practice_task: None,
            practice_status: String::new(),
            practice_diagnostics: Vec::new(),
            practice_log: Vec::new(),
            ownership_step: 0,
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
            lsp_status: "进入 Cargo 项目的 Rust 编辑器时，将按需启动 rust-analyzer。".into(),
            lsp_ready: false,
            lsp_user_stopped: false,
            lsp_hover: String::new(),
            lsp_completions: Vec::new(),
            lsp_definitions: Vec::new(),
            navigation_back: Vec::new(),
            lsp_target: None,
            editor_focused: false,
            completion_due: None,
            completion_anchor: None,
            completion_selected: 0,
            completion_target: None,
            definition_target: None,
            hover_due: None,
            hover_target: None,
            hover_pointer: None,
        }
    }
}

impl App {
    fn mark_active_document_synced(&mut self) {
        self.last_synced_path = self.file.clone();
        self.last_synced_revision = self.revision;
        self.last_synced_cursor_byte = self.cursor_byte;
        self.last_synced_mode = self.mode;
        self.last_synced_disk_stamp = self.disk_stamp.clone();
        self.last_synced_external_change = self.external_change;
    }

    /// Reflect metadata eagerly without copying an entire text buffer on every
    /// keystroke. The source is checkpointed by the 350 ms recovery debounce,
    /// or synchronously before save, switch, close and shutdown.
    fn sync_active_metadata_for_persistence(&mut self) {
        if self.file.as_ref() != self.documents.active().map(|doc| &doc.path) {
            self.sync_active_document();
            return;
        }
        if self.last_synced_cursor_byte != self.cursor_byte
            || self.last_synced_mode != self.mode
            || self.last_synced_disk_stamp != self.disk_stamp
            || self.last_synced_external_change != self.external_change
        {
            self.documents.update_active_metadata(
                self.cursor_byte,
                mode_name(self.mode).into(),
                self.disk_stamp.clone(),
                self.external_change,
            );
            self.last_synced_cursor_byte = self.cursor_byte;
            self.last_synced_mode = self.mode;
            self.last_synced_disk_stamp = self.disk_stamp.clone();
            self.last_synced_external_change = self.external_change;
        }
    }

    fn live_dirty_count(&self) -> usize {
        let count = self.documents.dirty_count();
        match self.documents.active() {
            Some(active) if self.file.as_ref() == Some(&active.path) => {
                if active.is_dirty() && !self.dirty {
                    count.saturating_sub(1)
                } else if !active.is_dirty() && self.dirty {
                    count + 1
                } else {
                    count
                }
            }
            _ => count + usize::from(self.file.is_some() && self.dirty),
        }
    }

    fn sync_active_document(&mut self) {
        let Some(path) = self.file.clone() else {
            self.mark_active_document_synced();
            return;
        };

        let mut inserted = false;
        let active_matches = self
            .documents
            .active()
            .is_some_and(|document| same_document_path(&document.path, &path));
        if !active_matches {
            if !self.documents.contains(&path) {
                self.documents.insert_loaded(
                    path.clone(),
                    self.saved_source.clone(),
                    self.cursor_byte,
                    mode_name(self.mode).into(),
                    self.disk_stamp.clone(),
                );
                inserted = true;
            }
            self.documents.set_active_path(&path);
        }

        let path_changed = self
            .last_synced_path
            .as_ref()
            .is_none_or(|previous| !same_document_path(previous, &path));
        let source_changed = inserted || path_changed || self.last_synced_revision != self.revision;
        let metadata_changed = path_changed
            || self.last_synced_cursor_byte != self.cursor_byte
            || self.last_synced_mode != self.mode
            || self.last_synced_disk_stamp != self.disk_stamp
            || self.last_synced_external_change != self.external_change;

        if source_changed {
            self.documents.update_active_source(
                self.rust.clone(),
                self.saved_source.clone(),
                self.cursor_byte,
                mode_name(self.mode).into(),
                self.disk_stamp.clone(),
                self.external_change,
            );
        } else if metadata_changed {
            // Cursor movement and mode changes must not clone large source buffers.
            self.documents.update_active_metadata(
                self.cursor_byte,
                mode_name(self.mode).into(),
                self.disk_stamp.clone(),
                self.external_change,
            );
        }
        if source_changed || metadata_changed {
            self.mark_active_document_synced();
        }
    }

    fn persist_document_recovery(&self, path: &Path) {
        let (Some(worker), Some(doc)) = (self.state_worker.as_ref(), self.documents.get(path))
        else {
            return;
        };
        if doc.pending_recovery.is_some() {
            return;
        }
        if doc.is_dirty() {
            worker.send(persistence::Job::SaveJournal {
                path: doc.path.clone(),
                baseline: doc.saved_source.clone(),
                edited: doc.source.clone(),
            });
        } else {
            worker.send(persistence::Job::DeleteJournal(doc.path.clone()));
        }
    }

    fn flush_shutdown_state(&mut self) {
        self.sync_active_document();
        if let Some(worker) = self.state_worker.as_ref() {
            worker.send(persistence::Job::Session(self.session_snapshot()));
            for doc in self.documents.iter() {
                if doc.pending_recovery.is_some() {
                    continue;
                }
                if doc.is_dirty() {
                    worker.send(persistence::Job::SaveJournal {
                        path: doc.path.clone(),
                        baseline: doc.saved_source.clone(),
                        edited: doc.source.clone(),
                    });
                } else {
                    worker.send(persistence::Job::DeleteJournal(doc.path.clone()));
                }
            }
        }
    }

    fn initialize_state(&mut self, restore_last_document: bool) {
        self.startup_restore_last_document = restore_last_document;
        let (tx, rx) = std::sync::mpsc::channel();
        self.startup_receiver = Some(rx);
        self.status = "正在后台读取工作区状态及崩溃恢复索引…".into();
        std::thread::spawn(move || {
            let result: StartupResult = (|| {
                let store = persistence::Store::from_system().map_err(|error| error.to_string())?;
                let session = store.load_session();
                let inventory = store.scan_journals();
                Ok((store, session, inventory))
            })();
            let _ = tx.send(result);
        });
    }

    fn poll_startup_state(&mut self, ctx: &egui::Context) -> bool {
        let Some(receiver) = self.startup_receiver.as_ref() else {
            return true;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.startup_receiver = None;
                match result {
                    Ok((store, session, inventory)) => {
                        self.initialize_state_from_scanned(
                            store,
                            session,
                            inventory,
                            self.startup_restore_last_document,
                        );
                    }
                    Err(error) => {
                        self.status = format!("工作区恢复存储不可用：{error}");
                    }
                }
                if let Some(path) = self.startup_explicit_path.take() {
                    self.apply_explicit_startup_path(path);
                }
                true
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.startup_receiver = None;
                self.status = "后台工作区恢复进程异常退出，未读取任何历史状态。".into();
                if let Some(path) = self.startup_explicit_path.take() {
                    self.apply_explicit_startup_path(path);
                }
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);
                        ui.heading("RustBridge");
                        ui.spinner();
                        ui.label("正在后台读取会话和崩溃恢复索引，界面仍可响应。");
                        ui.small("无文件覆盖；加载完成后恢复文件标签。");
                    });
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(80));
                false
            }
        }
    }

    fn apply_explicit_startup_path(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.cargo_project = is_cargo_root(&path);
            self.root = Some(path);
        } else if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            self.open_file(path);
        } else {
            self.status = "参数必须是实际目录或可读的 .rs 文件。".into();
        }
    }

    /// Synchronous path remains available to deterministic unit tests.
    #[cfg(test)]
    fn initialize_state_from_store(
        &mut self,
        store: persistence::Store,
        restore_last_document: bool,
    ) {
        let session = store.load_session();
        let inventory = store.scan_journals();
        self.initialize_state_from_scanned(store, session, inventory, restore_last_document);
    }

    fn initialize_state_from_scanned(
        &mut self,
        store: persistence::Store,
        session: Option<persistence::Session>,
        inventory: persistence::RecoveryInventory,
        restore_last_document: bool,
    ) {
        self.recovery_scan_warning = inventory.warning();
        let saved_journals = inventory.entries;
        self.orphan_recovery = saved_journals
            .iter()
            .filter(|entry| !entry.source_exists)
            .cloned()
            .collect();
        // Must install the recovery store *before* scheduling the first
        // asynchronous source open. Otherwise its Open job captures None
        // and the first tab silently loses its crash journal.
        self.state_worker = Some(persistence::PersistenceWorker::spawn(store.clone()));
        self.state_store = Some(store);
        if let Some(session) = session.or_else(|| {
            saved_journals
                .iter()
                .any(|entry| entry.source_exists)
                .then(persistence::Session::default)
        }) {
            self.lesson = session.lesson.min(lessons::LESSONS.len().saturating_sub(1));
            self.completed_lessons = normalized_completion(&session.completed_lessons);
            self.quiz_attempts = (0..lessons::LESSONS.len())
                .map(|i| session.quiz_attempts.get(i).copied().unwrap_or(0))
                .collect();
            self.quiz_mistakes = (0..lessons::LESSONS.len())
                .map(|i| session.quiz_mistakes.get(i).copied().unwrap_or(0))
                .collect();
            self.practice_passed = (0..learning_path::EXERCISES.len())
                .map(|i| session.practice_passed.get(i).copied().unwrap_or(false))
                .collect();
            self.practice_drafts = (0..learning_path::EXERCISES.len())
                .map(|i| session.practice_drafts.get(i).cloned().unwrap_or_default())
                .collect();
            self.cursor_byte = session.cursor_byte;
            self.recovered_mode = Some(mode_from_name(&session.mode));
            if restore_last_document {
                self.root = session.root.clone().filter(|p| p.is_dir());
                self.cargo_project = self.root.as_ref().is_some_and(|p| is_cargo_root(p));

                let mut documents = session.documents.clone();
                if documents.is_empty()
                    && let Some(path) = session.file.clone()
                {
                    documents.push(persistence::DocumentSession {
                        path,
                        cursor_byte: session.cursor_byte,
                        mode: session.mode.clone(),
                    });
                }
                if let Some(active) = session.file.clone()
                    && active.is_file()
                    && !documents
                        .iter()
                        .any(|document| same_document_path(&document.path, &active))
                {
                    documents.push(persistence::DocumentSession {
                        path: active,
                        cursor_byte: session.cursor_byte,
                        mode: session.mode.clone(),
                    });
                }
                for journal in saved_journals.iter().filter(|entry| entry.source_exists) {
                    if !documents
                        .iter()
                        .any(|document| same_document_path(&document.path, &journal.path))
                    {
                        documents.push(persistence::DocumentSession {
                            path: journal.path.clone(),
                            cursor_byte: 0,
                            mode: "rust".into(),
                        });
                    }
                }
                let mut seen = std::collections::HashSet::new();
                documents.retain(|document| {
                    document.path.is_file()
                        && seen.insert(
                            document
                                .path
                                .canonicalize()
                                .unwrap_or_else(|_| document.path.clone()),
                        )
                });
                if self.root.is_none() {
                    self.root = documents
                        .first()
                        .and_then(|document| document.path.parent())
                        .map(Path::to_path_buf);
                }
                self.cargo_project = self.root.as_ref().is_some_and(|p| is_cargo_root(p));
                self.restore_active = session
                    .file
                    .clone()
                    .filter(|path| path.is_file())
                    .or_else(|| documents.last().map(|document| document.path.clone()));
                for document in &documents {
                    self.restore_states
                        .insert(document.path.clone(), document.clone());
                    self.restore_queue.push_back(document.path.clone());
                }
                if let Some(first) = self.restore_queue.pop_front() {
                    self.open_file(first);
                } else if let Some(mode) = self.recovered_mode.take() {
                    self.mode = mode;
                }
            }
            self.last_session = Some(session);
        }
    }

    fn start_recovery_export(&mut self, source: PathBuf, destination: PathBuf) {
        if self.recovery_export_receiver.is_some() {
            self.status = "当前恢复导出仍在执行，请先查看其完成结果。".into();
            return;
        }
        let Some(store) = self.state_store.clone() else {
            self.status = "当前恢复存储不可用，无法导出。".into();
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.recovery_export_receiver = Some(rx);
        self.status = format!("正在后台导出恢复副本：{}", destination.display());
        std::thread::spawn(move || {
            let result = match store.load_journal(&source) {
                Some(journal) => match store.export_recovery(&journal, &destination) {
                    Ok(()) => format!("已导出恢复副本：{}", destination.display()),
                    Err(error) => format!("恢复导出失败；没有覆盖已有文件：{error}"),
                },
                None => "恢复快照已丢失、损坏或超过大小上限，未写入目标文件。".into(),
            };
            let _ = tx.send(result);
        });
    }

    fn poll_recovery_export(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.recovery_export_receiver.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(message) => {
                self.recovery_export_receiver = None;
                self.status = message;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.recovery_export_receiver = None;
                self.status = "恢复副本导出任务异常终止，目标文件需人工核查。".into();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            }
        }
    }

    fn session_snapshot(&self) -> persistence::Session {
        let mut documents = self
            .documents
            .iter()
            .map(|doc| persistence::DocumentSession {
                path: doc.path.clone(),
                cursor_byte: doc.cursor_byte,
                mode: doc.mode.clone(),
            })
            .collect::<Vec<_>>();

        // Session restoration is asynchronous. Persist the in-flight tab and
        // queued tabs too, so closing/crashing halfway through restore cannot
        // silently shrink the saved workspace to only the tabs already read.
        let mut pending_paths = Vec::new();
        if let Some((_, opening, _)) = self.opening.as_ref() {
            pending_paths.push(opening.clone());
        }
        pending_paths.extend(self.restore_queue.iter().cloned());
        for path in pending_paths {
            if documents
                .iter()
                .any(|document| same_document_path(&document.path, &path))
            {
                continue;
            }
            let saved = self.restore_states.get(&path);
            documents.push(persistence::DocumentSession {
                path,
                cursor_byte: saved.map_or(0, |state| state.cursor_byte),
                mode: saved
                    .map(|state| state.mode.clone())
                    .unwrap_or_else(|| mode_name(self.mode).into()),
            });
        }

        let active_file = self.restore_active.clone().or_else(|| self.file.clone());
        let active_state = active_file.as_ref().and_then(|path| {
            documents
                .iter()
                .find(|document| same_document_path(&document.path, path))
        });
        persistence::Session {
            root: self.root.clone(),
            file: active_file,
            mode: active_state
                .map(|document| document.mode.clone())
                .unwrap_or_else(|| mode_name(self.mode).into()),
            lesson: self.lesson,
            cursor_byte: active_state.map_or(self.cursor_byte, |document| document.cursor_byte),
            completed_lessons: self.completed_lessons.clone(),
            quiz_attempts: self.quiz_attempts.clone(),
            quiz_mistakes: self.quiz_mistakes.clone(),
            practice_passed: self.practice_passed.clone(),
            practice_drafts: self
                .practice_drafts
                .iter()
                .enumerate()
                .map(|(i, draft)| {
                    if self.practice_selected_stage.is_some() && i == self.practice_index {
                        self.practice_code.chars().take(64 * 1024).collect()
                    } else {
                        draft.clone()
                    }
                })
                .collect(),
            documents,
        }
    }

    fn mark_dirty_for_recovery(&mut self) {
        self.recovery_due = Some(std::time::Instant::now() + std::time::Duration::from_millis(350));
    }
    fn flush_recovery_journal(&mut self) {
        self.recovery_due = None;
        self.sync_active_document();
        if let Some(path) = self.file.clone() {
            self.persist_document_recovery(&path);
        }
    }
    fn discard_current_recovery(&mut self) {
        if let Some(path) = self.file.as_ref() {
            if let Some(worker) = self.state_worker.as_ref() {
                worker.send(persistence::Job::DeleteJournal(path.clone()));
            }
            self.documents.set_pending_recovery(path, None);
        }
        self.pending_recovery = None;
        self.recovery_due = None;
    }
    fn poll_persistence(&mut self) {
        self.sync_active_metadata_for_persistence();
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
        self.sync_active_document();
        let Some(root) = self.root.clone().and_then(|p| p.canonicalize().ok()) else {
            self.lsp_status = "请先打开实际 Cargo 项目目录。".into();
            return;
        };
        if !is_cargo_root(&root) {
            self.lsp_status = "当前目录缺少 Cargo.toml，无法初始化项目 LSP。".into();
            return;
        }
        self.lsp_task = None;
        self.lsp_user_stopped = false;
        // Normalize the project identity once rather than synchronously
        // canonicalizing the directory on every UI frame in poll_lsp().
        self.root = Some(root.clone());
        let worker = lsp::LspTask::start(root.clone());
        for doc in self.documents.iter() {
            if let Ok(path) = doc.path.canonicalize()
                && path.starts_with(&root)
            {
                worker.open(path, doc.source.clone(), self.revision.max(1));
            }
        }
        self.lsp_task = Some(worker);
        self.lsp_ready = false;
        self.lsp_hover.clear();
        self.lsp_completions.clear();
        self.lsp_definitions.clear();
        self.hover_target = None;
        self.hover_due = None;
        self.completion_due = None;
        self.lsp_status = "正在初始化 rust-analyzer LSP；所有已打开文档将保持独立同步。".into();
    }

    fn poll_lsp(&mut self) {
        let Some(task) = self.lsp_task.as_ref() else {
            return;
        };
        if self.root.as_ref() != Some(&task.root) {
            task.cancel();
            self.lsp_task = None;
            self.lsp_ready = false;
            return;
        }
        let events = task.poll();
        let mut stopped = false;
        for event in events {
            match event {
                lsp::Event::Stopped => {
                    stopped = true;
                }
                lsp::Event::Ready => {
                    self.lsp_ready = true;
                    self.lsp_status =
                        "rust-analyzer LSP 已初始化；当前项目的打开文档均已注册。".into();
                }
                lsp::Event::Status(text) => {
                    self.lsp_status = text;
                }
                lsp::Event::Hover {
                    path,
                    revision,
                    line,
                    character,
                    text,
                } if self.hover_target.as_ref().is_some_and(|target| {
                    target.0 == path
                        && target.1 == revision
                        && target.2 == line
                        && target.3 == character
                }) && revision == self.revision =>
                {
                    self.lsp_hover = text.chars().take(12000).collect();
                    self.lsp_status = if self.lsp_hover.is_empty() {
                        "当前位置没有可显示的悬浮信息。".into()
                    } else {
                        "rust-analyzer 悬浮信息已返回。".into()
                    };
                }
                lsp::Event::Completion {
                    path,
                    revision,
                    line,
                    character,
                    items,
                } if self.completion_target.as_ref().is_some_and(|target| {
                    target.0 == path
                        && target.1 == revision
                        && target.2 == line
                        && target.3 == character
                }) && revision == self.revision =>
                {
                    self.lsp_completions = items;
                    self.completion_selected = 0;
                    self.lsp_status = format!(
                        "rust-analyzer 返回 {} 项补全候选。",
                        self.lsp_completions.len()
                    );
                }
                lsp::Event::Definition {
                    path,
                    revision,
                    line,
                    character,
                    locations,
                } if self.definition_target.as_ref().is_some_and(|target| {
                    target.0 == path
                        && target.1 == revision
                        && target.2 == line
                        && target.3 == character
                }) && revision == self.revision =>
                {
                    self.lsp_definitions = locations;
                    if let Some(location) = self.lsp_definitions.first().cloned() {
                        self.jump_lsp_definition(location);
                    }
                    self.lsp_status = format!(
                        "rust-analyzer 返回 {} 个定义位置。",
                        self.lsp_definitions.len()
                    );
                }
                _ => {}
            }
        }
        if stopped {
            self.lsp_ready = false;
            self.lsp_task = None;
            // Stop an unlimited rapid-restart loop when analyzer is missing or
            // crashing. The Tools panel provides an explicit Retry action.
            self.lsp_user_stopped = true;
            self.lsp_status
                .push_str("；LSP 已退出，点击「启动 LSP」可以重试。");
            self.lsp_hover.clear();
            self.lsp_completions.clear();
        }
    }

    fn lsp_query_at(&mut self, kind: lsp::Kind, byte: usize) {
        let Some(task) = self.lsp_task.as_ref() else {
            return;
        };
        let Some(path) = self.file.as_ref().and_then(|path| path.canonicalize().ok()) else {
            return;
        };
        if !path.starts_with(&task.root) {
            self.lsp_status =
                "当前文件不属于已启动的 Cargo 工作区；请切换到对应项目后启动 LSP。".into();
            return;
        }
        let pos = lsp::position_for_byte(&self.rust, byte);
        let target = (path.clone(), self.revision, pos.line, pos.character);
        match kind {
            lsp::Kind::Hover => self.hover_target = Some(target),
            lsp::Kind::Completion => self.completion_target = Some(target),
            lsp::Kind::Definition => self.definition_target = Some(target),
        }
        task.query(path, kind, self.revision, pos.line, pos.character);
        self.lsp_status = format!("正在请求 {:?}，文档版本 {}…", kind, self.revision);
    }

    fn dispatch_lsp_debounced(&mut self) {
        let now = std::time::Instant::now();
        if self.hover_due.is_some_and(|due| now >= due) {
            self.hover_due = None;
            if let (Some(task), Some((path, revision, line, character))) =
                (self.lsp_task.as_ref(), self.hover_target.clone())
                && revision == self.revision
                && self.lsp_ready
            {
                task.query(path, lsp::Kind::Hover, revision, line, character);
            }
        }
        if self.completion_due.is_some_and(|due| now >= due) {
            self.completion_due = None;
            if self.lsp_ready {
                self.lsp_query_at(lsp::Kind::Completion, self.cursor_byte);
            }
        }
    }

    fn insert_completion(&mut self, completion: lsp::Completion) {
        if self.file.is_none() {
            return;
        }
        let text = if completion.is_snippet {
            lsp::expand_snippet(&completion.text)
        } else {
            completion.text.clone()
        };
        if text.is_empty() {
            self.lsp_status = "补全项没有可插入的文本。".into();
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
        self.rust.replace_range(range.clone(), &text);
        self.source_edited(range.clone(), &text);
        self.cursor_byte = range.start + text.len();
        self.editor_jump = Some(parser::ByteSpan {
            start: self.cursor_byte,
            end: self.cursor_byte,
        });
        self.dirty = self.rust != self.saved_source;
        self.mark_dirty_for_recovery();
        self.lsp_completions.clear();
        self.completion_due = None;
        self.completion_target = None;
        self.hover_target = None;
        self.hover_due = None;
        self.lsp_hover.clear();
        self.status = "已插入 rust-analyzer 补全文本；原始 Rust 文件尚未保存。".into();
    }

    fn jump_lsp_definition(&mut self, location: lsp::Location) {
        if let Some(from) = self.file.clone() {
            let from_pos = lsp::position_for_byte(&self.rust, self.cursor_byte);
            let target = lsp::SpanPosition {
                line: location.line,
                character: location.character,
            };
            if !same_document_path(&from, &location.path)
                || from_pos.line != target.line
                || from_pos.character != target.character
            {
                self.navigation_back.push((from, from_pos));
                if self.navigation_back.len() > 100 {
                    self.navigation_back.remove(0);
                }
            }
        }
        self.go_to_lsp_location(location);
    }

    fn navigate_back(&mut self) {
        let Some((path, position)) = self.navigation_back.pop() else {
            return;
        };
        self.go_to_lsp_location(lsp::Location {
            path,
            line: position.line,
            character: position.character,
        });
    }

    fn go_to_lsp_location(&mut self, location: lsp::Location) {
        let position = lsp::SpanPosition {
            line: location.line,
            character: location.character,
        };
        if self.file.as_ref().is_some_and(|path| {
            path.canonicalize().unwrap_or_else(|_| path.clone())
                == location
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| location.path.clone())
        }) {
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
            self.open(location.path);
        }
    }

    fn start_cargo_action(&mut self, action: cargo_runner::Action) {
        self.show_bottom_panel = true;
        self.bottom_tab = match action {
            cargo_runner::Action::Run => BottomTab::Run,
            cargo_runner::Action::Check
            | cargo_runner::Action::PracticeCheck
            | cargo_runner::Action::Test => BottomTab::Build,
        };
        let Some(root) = self.root.clone() else {
            return;
        };
        self.sync_active_document();
        if self.documents.dirty_count() > 0 || self.saving.is_some() {
            self.cargo_status = "有 Rust 文档尚未保存；请先全部保存再运行 Cargo。".into();
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
                cargo_runner::Event::Finished { message, .. } => {
                    self.cargo_status = message;
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
            self.open(diag.path);
        }
    }

    fn close_requires_confirmation(&self) -> bool {
        self.dirty || self.documents.dirty_count() > 0 || self.saving.is_some()
    }

    fn open(&mut self, path: PathBuf) {
        let key = path.canonicalize().unwrap_or_else(|_| path.clone());
        if self
            .file
            .as_ref()
            .is_some_and(|active| active.canonicalize().unwrap_or_else(|_| active.clone()) == key)
        {
            return;
        }
        if self.opening.as_ref().is_some_and(|(_, opening, _)| {
            opening.canonicalize().unwrap_or_else(|_| opening.clone()) == key
        }) {
            return;
        }

        self.sync_active_document();
        if let Some(active) = self.file.clone() {
            self.recovery_due = None;
            self.persist_document_recovery(&active);
        }

        if self.documents.contains(&path) {
            if !self.restore_queue.is_empty() {
                self.restore_active = Some(path.clone());
                self.activate_document(path);
                return;
            }
            if let Some(io) = self.document_io.as_ref() {
                io.cancel_open();
            }
            self.opening = None;
            self.opening_started = None;
            self.activate_document(path);
            return;
        }
        if !self.restore_queue.is_empty() {
            if let Some((_, opening, _)) = self.opening.as_ref()
                && self.restore_states.contains_key(opening)
                && !self.documents.contains(opening)
                && !self
                    .restore_queue
                    .iter()
                    .any(|queued| same_document_path(queued, opening))
            {
                self.restore_queue.push_front(opening.clone());
            }
            self.restore_active = Some(path.clone());
        }
        self.open_file(path);
    }

    fn open_project(&mut self, folder: PathBuf) {
        if !folder.is_dir() {
            self.status = format!("项目目录不可用：{}", folder.display());
            return;
        }
        self.sync_active_document();
        self.pending_open = None;
        if self.saving.is_some()
            || self.documents.dirty_count() > 0
            || self.documents.recovery_count() > 0
            || self.dirty
        {
            self.pending_project = Some(folder);
            self.after_save_open = false;
            self.after_save_project = false;
            self.status = format!(
                "切换项目之前有 {} 个未保存文档、{} 个待恢复文档。",
                self.documents.dirty_count(),
                self.documents.recovery_count()
            );
            return;
        }
        self.apply_project(folder);
    }

    fn apply_project(&mut self, folder: PathBuf) {
        self.sync_active_document();
        let old_documents = self.documents.clear();
        if let Some(worker) = self.state_worker.as_ref() {
            for document in old_documents {
                if document.pending_recovery.is_none() || document.is_dirty() {
                    worker.send(persistence::Job::DeleteJournal(document.path));
                }
            }
        }
        self.restore_queue.clear();
        self.restore_states.clear();
        self.restore_active = None;
        self.pending_close_document = None;
        self.save_queue.clear();
        self.save_errors.clear();
        if let Some(io) = self.document_io.as_ref() {
            io.cancel_open();
        }
        if let Some(task) = self.lsp_task.take() {
            task.cancel();
        }
        self.lsp_ready = false;
        self.lsp_user_stopped = false;
        self.lsp_status = "项目已切换；进入 Rust 编辑器时将按需启动新项目 LSP。".into();
        self.lsp_hover.clear();
        self.lsp_completions.clear();
        self.lsp_definitions.clear();
        self.navigation_back.clear();
        self.lsp_target = None;
        self.hover_target = None;
        self.hover_due = None;
        self.hover_pointer = None;
        self.completion_due = None;
        self.completion_anchor = None;
        self.completion_target = None;
        self.definition_target = None;

        if let Some(task) = self.cargo_task.take() {
            task.cancel();
        }
        self.cargo_diagnostics.clear();
        self.cargo_log.clear();
        self.cargo_status = "新项目已打开；Cargo 操作需要重新授权。".into();
        self.cargo_diag_target = None;

        if let Some(task) = self.analyzer_diagnostics.take() {
            task.cancel();
        }
        self.analyzer_diagnostics_status = "已切换项目；原项目诊断已取消。".into();

        self.symbol_revision += 1;
        self.symbol_hits.clear();
        self.symbol_pending = false;
        self.symbol_target = None;
        self.file_search_revision += 1;
        self.file_search_pending = false;
        self.search_results.clear();
        self.file_query.clear();

        self.dir_index = file_tree::DirectoryIndex::new();
        self.directory_limits.clear();
        self.root = Some(folder.clone());
        self.cargo_project = is_cargo_root(&folder);
        self.file = None;
        self.mark_active_document_synced();
        self.file_watch = None;
        self.disk_stamp = None;
        self.opening = None;
        self.opening_started = None;
        self.pending_open = None;
        self.pending_project = None;
        self.after_save_open = false;
        self.after_save_project = false;
        self.discard_on_open = None;
        self.pending_recovery = None;

        // A project switch closes the previous document context. The bundled
        // sample is restored as an explicitly read-only landing document.
        self.rust = include_str!("../examples/basics.rs").to_owned();
        self.saved_source = self.rust.clone();
        self.dirty = false;
        self.update_projection();
        self.editor_gutter_revision = 0;
        self.editor_find = None;
        self.editor_jump = None;
        self.editor_page_start = 0;
        self.editor_page_target = 1;
        self.editor_scroll_override = None;
        self.cursor_byte = 0;
        self.selected = None;
        self.jump_to = None;
        self.scroll_follow = None;
        self.last_scroll = None;
        self.symbol_query.clear();
        self.recent_files.retain(|path| path.starts_with(&folder));
        self.mode = Mode::Java;
        self.sidebar_section = SidebarSection::Project;
        self.lesson_return = None;
        self.quiz_choice = None;
        self.status = if self.cargo_project {
            format!(
                "已打开 Cargo 项目：{}。源码仍以真实 .rs 文件为准。",
                folder.display()
            )
        } else {
            format!(
                "已打开目录：{}。未发现根目录 Cargo.toml。",
                folder.display()
            )
        };
        self.session_due = Some(std::time::Instant::now() + std::time::Duration::from_millis(350));
    }

    fn open_file(&mut self, path: PathBuf) {
        if self.root.is_none()
            && let Some(root) = find_cargo_root(&path)
        {
            self.cargo_project = true;
            self.root = Some(root);
        }
        // A discard/reload intent is bound to one exact document path. If a
        // newer open supersedes it, never apply that intent to the new target.
        if self
            .discard_on_open
            .as_ref()
            .is_some_and(|pending| !same_document_path(pending, &path))
        {
            self.discard_on_open = None;
        }
        // Reading from slow disks/network mounts never blocks the rendering thread.
        self.io_revision += 1;
        let id = self.io_revision;
        self.opening = Some((id, path.clone(), self.revision));
        self.opening_started = Some(std::time::Instant::now());
        self.document_io
            .get_or_insert_with(document_io::DocumentIo::new)
            .open_with_recovery(id, path, self.state_store.clone());
        self.status = "正在后台读取 Rust 文件…".into();
    }

    fn complete_open(&mut self, path: PathBuf, loaded: document_io::Loaded) {
        self.sync_active_document();
        let discarding_recovery = self
            .discard_on_open
            .as_ref()
            .is_some_and(|pending| same_document_path(pending, &path));
        if discarding_recovery {
            self.discard_on_open = None;
        }
        if discarding_recovery {
            if self
                .file
                .as_ref()
                .is_some_and(|active| same_document_path(active, &path))
            {
                self.file = None;
                self.dirty = false;
            }
            self.documents.remove(&path);
            if let Some(worker) = self.state_worker.as_ref() {
                worker.send(persistence::Job::DeleteJournal(path.clone()));
            }
            self.pending_recovery = None;
            self.discard_on_open = None;
        }
        let restored = self.restore_states.remove(&path);
        let cursor_byte = restored
            .as_ref()
            .map_or(0, |state| state.cursor_byte.min(loaded.source.len()));
        let mode = restored
            .as_ref()
            .map_or_else(|| "java".to_owned(), |state| state.mode.clone());
        let recovery = if discarding_recovery {
            None
        } else {
            loaded
                .recovery
                .filter(|journal| journal.edited != loaded.source)
        };
        self.documents
            .insert_loaded(path.clone(), loaded.source, cursor_byte, mode, loaded.stamp);
        self.documents.set_pending_recovery(&path, recovery);
        self.activate_document(path);

        if let Some(next) = self.restore_queue.pop_front() {
            self.open_file(next);
            return;
        }
        if let Some(active) = self.restore_active.take()
            && self.documents.contains(&active)
        {
            self.activate_document(active);
        }
    }

    fn activate_document(&mut self, path: PathBuf) {
        self.sync_active_document();
        let Some(document) = self.documents.activate(&path) else {
            self.status = format!("文档尚未打开：{}", path.display());
            return;
        };

        self.file = Some(document.path.clone());
        self.rust = document.source;
        self.saved_source = document.saved_source;
        self.dirty = self.rust != self.saved_source;
        self.cursor_byte = document.cursor_byte.min(self.rust.len());
        if !self.rust.is_char_boundary(self.cursor_byte) {
            self.cursor_byte = self
                .rust
                .char_indices()
                .map(|(offset, _)| offset)
                .take_while(|offset| *offset <= self.cursor_byte)
                .last()
                .unwrap_or(0);
        }
        self.mode = mode_from_name(&document.mode);
        self.disk_stamp = document.disk_stamp;
        // Never synchronously query a slow filesystem during tab activation.
        // The reusable background watcher revalidates the switched-to path;
        // explicit saves independently re-check disk identity and contents.
        self.external_change = document.external_change;

        self.update_projection();
        self.editor_gutter_revision = 0;
        self.editor_find = None;
        self.editor_jump = None;
        self.editor_page_start = 0;
        self.editor_page_target = 1;
        self.editor_scroll_override = None;
        self.selected = None;
        self.jump_to = None;
        self.scroll_follow = None;
        self.last_scroll = None;
        self.lsp_hover.clear();
        self.lsp_completions.clear();
        self.lsp_definitions.clear();
        self.hover_target = None;
        self.hover_due = None;
        self.hover_pointer = None;
        self.completion_due = None;
        self.completion_anchor = None;
        self.completion_target = None;
        self.definition_target = None;
        self.completion_selected = 0;
        self.record_recent_file();

        self.pending_recovery = if self.restore_queue.is_empty() {
            document.pending_recovery.clone()
        } else {
            None
        };

        if let Some((target, span)) = self.symbol_target.take()
            && target == document.path
        {
            self.mode = Mode::Rust;
            self.editor_jump = Some(span);
        }
        if let Some((target, line, column)) = self.cargo_diag_target.take()
            && target == document.path
        {
            let span = diagnostic_location(&self.rust, line, column);
            self.editor_jump = Some(span);
            self.mode = Mode::Rust;
        }
        if let Some((target, pos)) = self.lsp_target.take()
            && target.canonicalize().unwrap_or_else(|_| target.clone())
                == document
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| document.path.clone())
            && let Some(byte) = lsp::byte_for_position(&self.rust, &pos)
        {
            self.editor_jump = Some(parser::ByteSpan {
                start: byte,
                end: byte,
            });
            self.cursor_byte = byte;
            self.mode = Mode::Rust;
        }

        if let Some(watch) = self.file_watch.as_mut() {
            watch.set_target(document.path.clone(), self.disk_stamp.clone());
        }
        if let Some(task) = self.lsp_task.as_ref()
            && let Ok(path) = document.path.canonicalize()
            && path.starts_with(&task.root)
        {
            task.open(path, self.rust.clone(), self.revision.max(1));
        }
        self.status = if self.external_change {
            "文档在 RustBridge 外部发生了变化；保存已阻止，请先重新加载或另存副本。".into()
        } else if self.dirty {
            "已恢复此文档的独立未保存缓冲区。".into()
        } else {
            "已切换到打开的 Rust 文档；每个标签保留自己的编辑状态。".into()
        };
        self.mark_active_document_synced();
        self.session_due = Some(std::time::Instant::now() + std::time::Duration::from_millis(350));
    }

    fn poll_document_io(&mut self, ctx: &egui::Context) {
        while let Some(reply) = self
            .document_io
            .as_ref()
            .and_then(document_io::DocumentIo::take)
        {
            match reply {
                document_io::Reply::Open { id, path, result } => {
                    let matching = self
                        .opening
                        .as_ref()
                        .is_some_and(|(pending, p, _)| *pending == id && p == &path);
                    if matching {
                        self.opening = None;
                        self.opening_started = None;
                        match result {
                            Ok(loaded) => self.complete_open(path, loaded),
                            Err(error) => {
                                self.discard_on_open = None;
                                self.status = format!("Open failed: {error}");
                                if let Some(next) = self.restore_queue.pop_front() {
                                    self.open_file(next);
                                } else if let Some(active) = self.restore_active.take()
                                    && self.documents.contains(&active)
                                {
                                    self.activate_document(active);
                                }
                            }
                        }
                    }
                }
                document_io::Reply::Save {
                    id,
                    path,
                    content,
                    result,
                    stamp,
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
                            // Metadata is already captured on the I/O worker.
                            // Do not block the GUI when the source sits on a
                            // slow filesystem or network mount.
                            self.documents
                                .update_saved(&path, content.clone(), stamp.clone());
                            let active_matches = self
                                .file
                                .as_ref()
                                .is_some_and(|active| same_document_path(active, &path));
                            if active_matches {
                                self.saved_source = content.clone();
                                self.disk_stamp = stamp.clone();
                                self.dirty = self.rust != self.saved_source;
                                self.external_change = false;
                                if let Some(watch) = self.file_watch.as_mut() {
                                    watch.set_target(path.clone(), stamp);
                                }
                                // The save only commits its captured snapshot. If the user
                                // typed again while I/O was in flight, the manager must
                                // receive the newer visible buffer before we decide whether
                                // the document is dirty or its recovery journal can be removed.
                                // Marking the revision as synced here would hide those edits.
                                self.sync_active_document();
                            }
                            self.persist_document_recovery(&path);
                            self.status = match outcome {
                                safe_save::SaveOutcome::Durable => {
                                    format!("已保存 {}", path.display())
                                }
                                safe_save::SaveOutcome::CommittedDurabilityUncertain(error) => {
                                    format!("内容已写入磁盘，但目录同步失败，持久性未确认：{error}")
                                }
                            };

                            if !durable {
                                self.after_save_close = false;
                                self.after_save_open = false;
                                self.after_save_project = false;
                                self.save_errors.push(self.status.clone());
                                self.start_next_queued_save();
                            } else {
                                if self.documents.get(&path).is_some_and(|doc| doc.is_dirty())
                                    && !self.save_queue.iter().any(|queued| queued == &path)
                                {
                                    self.save_queue.push_back(path.clone());
                                }
                                if self.start_next_queued_save() {
                                    // Continue the accepted save queue before closing/switching.
                                } else if self.pending_close_document.as_ref().is_some_and(
                                    |pending| {
                                        pending.canonicalize().unwrap_or_else(|_| pending.clone())
                                            == path.canonicalize().unwrap_or_else(|_| path.clone())
                                    },
                                ) && self
                                    .documents
                                    .get(&path)
                                    .is_some_and(|doc| !doc.is_dirty())
                                {
                                    self.close_document_now(path.clone(), false);
                                } else if self.pending_project.is_some()
                                    && self.save_errors.is_empty()
                                    && self.documents.dirty_count() == 0
                                    && let Some(next) = self.pending_project.take()
                                {
                                    self.apply_project(next);
                                }

                                if self.after_save_open
                                    && self.pending_open.as_ref().is_some_and(|pending| {
                                        pending.canonicalize().unwrap_or_else(|_| pending.clone())
                                            == path.canonicalize().unwrap_or_else(|_| path.clone())
                                    })
                                    && self.documents.get(&path).is_some_and(|doc| !doc.is_dirty())
                                {
                                    self.after_save_open = false;
                                    if let Some(next) = self.pending_open.take() {
                                        self.reload_file_from_disk(next);
                                    }
                                }
                                if self.after_save_close && !self.close_requires_confirmation() {
                                    self.after_save_close = false;
                                    self.confirm_close = false;
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            }
                            if active_matches && self.dirty {
                                self.status.push_str("；保存期间又有新修改，请再次保存。");
                            }
                        }
                        Err(error) => {
                            self.after_save_close = false;
                            self.after_save_open = false;
                            self.after_save_project = false;
                            self.status =
                                format!("Save failed (original source preserved): {error}");
                            self.save_errors
                                .push(format!("{}: {error}", path.display()));
                            self.start_next_queued_save();
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
        let Some(path) = self.file.clone() else {
            return;
        };
        self.sync_active_document();
        if self.dirty {
            self.pending_open = Some(path);
            self.status = "重新加载将替换当前缓冲区；请明确保存、放弃或取消。".into();
        } else {
            self.reload_file_from_disk(path);
        }
    }

    fn reload_file_from_disk(&mut self, path: PathBuf) {
        self.discard_on_open = Some(path.clone());
        self.open_file(path);
    }

    fn request_close_document(&mut self, path: PathBuf) {
        self.sync_active_document();
        if self.saving.as_ref().is_some_and(|(_, saving_path, _)| {
            saving_path
                .canonicalize()
                .unwrap_or_else(|_| saving_path.clone())
                == path.canonicalize().unwrap_or_else(|_| path.clone())
        }) {
            self.pending_close_document = Some(path);
            self.status = "该文档正在保存，保存完成后再关闭标签。".into();
            return;
        }
        if self.documents.get(&path).is_some_and(|doc| doc.is_dirty()) {
            self.pending_close_document = Some(path);
            return;
        }
        self.close_document_now(path, false);
    }

    fn close_document_now(&mut self, path: PathBuf, discard: bool) {
        if self.saving.as_ref().is_some_and(|(_, saving_path, _)| {
            saving_path
                .canonicalize()
                .unwrap_or_else(|_| saving_path.clone())
                == path.canonicalize().unwrap_or_else(|_| path.clone())
        }) {
            self.pending_close_document = Some(path);
            return;
        }
        self.sync_active_document();
        let was_active = self.file.as_ref().is_some_and(|active| {
            active.canonicalize().unwrap_or_else(|_| active.clone())
                == path.canonicalize().unwrap_or_else(|_| path.clone())
        });
        let has_pending_recovery = self
            .documents
            .get(&path)
            .is_some_and(|doc| doc.pending_recovery.is_some());
        if (discard
            || (!has_pending_recovery
                && self.documents.get(&path).is_some_and(|doc| !doc.is_dirty())))
            && let Some(worker) = self.state_worker.as_ref()
        {
            worker.send(persistence::Job::DeleteJournal(path.clone()));
        }
        self.documents.remove(&path);
        if let Some(task) = self.lsp_task.as_ref()
            && let Ok(canonical) = path.canonicalize()
        {
            task.close(canonical);
        }
        if was_active {
            self.hover_target = None;
            self.hover_due = None;
            self.hover_pointer = None;
            self.completion_target = None;
            self.completion_due = None;
            self.completion_anchor = None;
            self.definition_target = None;
            self.lsp_hover.clear();
            self.lsp_completions.clear();
            self.lsp_definitions.clear();
            self.editor_focused = false;
            self.file = None;
            self.mark_active_document_synced();
            self.file_watch = None;
            self.disk_stamp = None;
            self.external_change = false;
            self.pending_recovery = None;
            self.dirty = false;
            self.rust = include_str!("../examples/basics.rs").to_owned();
            self.saved_source = self.rust.clone();
            self.cursor_byte = 0;
            self.mode = Mode::Java;
            self.selected = None;
            self.jump_to = None;
            self.editor_jump = None;
            self.editor_find = None;
            self.update_projection();
            if let Some(next) = self.documents.active_path() {
                self.activate_document(next);
            }
        }
        self.pending_close_document = None;
        self.session_due = Some(std::time::Instant::now() + std::time::Duration::from_millis(200));
        self.status = format!("已关闭标签：{}", path.display());
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
        self.hover_target = None;
        self.hover_due = None;
        self.hover_pointer = None;
        self.completion_target = None;
        self.completion_due = None;
        self.definition_target = None;
        self.lsp_hover.clear();
        self.lsp_completions.clear();
        self.projection_due =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(200));
    }

    fn save(&mut self) {
        self.sync_active_document();
        let Some(path) = self.file.clone() else {
            self.status = "内置示例为只读材料，打开真实 .rs 文件后才能保存。".into();
            return;
        };
        self.start_save_path(path);
    }

    /// Returns true if a background save was accepted. False means the
    /// caller can continue with the next queued document.
    fn start_save_path(&mut self, path: PathBuf) -> bool {
        if self.saving.is_some() {
            self.status = "已有一个安全保存任务正在运行。".into();
            return false;
        }
        let Some(document) = self.documents.get(&path) else {
            self.status = format!("文档已关闭，无法保存：{}", path.display());
            return false;
        };
        if !document.is_dirty() {
            self.status = format!("{} 已经是最新版本。", path.display());
            return false;
        }
        if document.external_change {
            self.status = format!("{} 已在外部修改；为避免覆盖，拒绝保存。", path.display());
            return false;
        }
        self.io_revision += 1;
        let id = self.io_revision;
        let content = document.source.clone();
        self.document_io
            .get_or_insert_with(document_io::DocumentIo::new)
            .save(
                id,
                path.clone(),
                document.saved_source.clone(),
                content.clone(),
            );
        self.saving = Some((id, path.clone(), content));
        self.saving_started = Some(std::time::Instant::now());
        self.status = format!("正在后台安全保存 {}…", path.display());
        true
    }

    fn start_next_queued_save(&mut self) -> bool {
        while let Some(next) = self.save_queue.pop_front() {
            if self.start_save_path(next.clone()) {
                return true;
            }
            if self.documents.get(&next).is_some_and(|doc| doc.is_dirty()) {
                self.save_errors.push(self.status.clone());
            }
        }
        if !self.save_errors.is_empty() {
            self.status = format!(
                "保存全部已结束，但 {} 个文档存在冲突或错误：{}",
                self.save_errors.len(),
                self.save_errors.join("；")
            );
            self.after_save_close = false;
            self.after_save_project = false;
        }
        false
    }

    fn save_all_documents(&mut self) {
        self.sync_active_document();
        if self.saving.is_some() {
            self.status = "当前保存完成后再继续保存其他文档。".into();
            return;
        }
        self.save_errors.clear();
        self.save_queue = self.documents.dirty_paths().into();
        if self.save_queue.is_empty() {
            self.status = "所有打开的文档均已保存。".into();
        } else {
            self.start_next_queued_save();
        }
    }

    fn cycle_document(&mut self, direction: isize) {
        self.sync_active_document();
        let paths = self.documents.paths();
        if paths.len() < 2 {
            return;
        }
        let Some(active) = self.file.as_ref() else {
            return;
        };
        let Some(index) = paths.iter().position(|path| {
            path.canonicalize().unwrap_or_else(|_| path.clone())
                == active.canonicalize().unwrap_or_else(|_| active.clone())
        }) else {
            return;
        };
        let next = (index as isize + direction).rem_euclid(paths.len() as isize) as usize;
        self.open(paths[next].clone());
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

    fn apply_external_stamp(&mut self, current: Option<FileStamp>) {
        let Some(path) = self.file.as_ref() else {
            return;
        };
        let changed = current != self.disk_stamp;
        if changed && !self.external_change {
            self.status =
                "File changed outside RustBridge; save is blocked until you reopen it.".into();
        }
        self.external_change = changed;
        self.documents.set_external_change(path, changed);
    }

    #[cfg(test)]
    fn check_external_change(&mut self) {
        let stamp = self.file.as_ref().and_then(|path| file_stamp(path).ok());
        self.apply_external_stamp(stamp);
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
        self.sync_active_document();
        if self
            .documents
            .dirty_paths()
            .iter()
            .any(|path| path.starts_with(&root))
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
            ui.label("点击任意一侧的高亮符号，定位两侧同一个 AST 语法出现位置。");
            ui.checkbox(&mut self.sync_scroll, "联动滚动");
            ui.small(format!("左侧 {:.0}%", self.compare_split * 100.0));
        });
        ui.separator();

        let jump = self.jump_to.take();
        let pending = self.scroll_follow.take();
        let available = ui.available_rect_before_wrap();
        let available_width = available.width().max(300.0);
        let available_height = available.height().max(120.0);
        let divider_width = 6.0;
        let usable_width = (available_width - divider_width).max(280.0);
        let minimum_pane = (usable_width * 0.28).clamp(100.0, 180.0);
        let left_width =
            (usable_width * self.compare_split).clamp(minimum_pane, usable_width - minimum_pane);
        let right_width = usable_width - left_width;
        let (mut left, mut right) = (None, None);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                egui::vec2(left_width, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |pane| {
                    pane.strong("Java 心智模型 · 只读");
                    left = Some(code_view::show(
                        pane,
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
                },
            );

            let splitter = ui.allocate_response(
                egui::vec2(divider_width, available_height),
                egui::Sense::drag(),
            );
            ui.painter().rect_filled(
                splitter.rect,
                2.0,
                if splitter.dragged() || splitter.hovered() {
                    Color32::from_rgb(92, 145, 190)
                } else {
                    Color32::from_gray(70)
                },
            );
            if splitter.hovered() || splitter.dragged() {
                ui.output_mut(|output| {
                    output.cursor_icon = egui::CursorIcon::ResizeHorizontal;
                });
            }
            if splitter.dragged()
                && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
            {
                self.compare_split =
                    ((pointer.x - available.left()) / usable_width).clamp(0.25, 0.75);
                ui.ctx().request_repaint();
            }

            ui.allocate_ui_with_layout(
                egui::vec2(right_width, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |pane| {
                    pane.strong("原始 Rust 源码 · 只读对照");
                    right = Some(code_view::show(
                        pane,
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
                },
            );
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
        let stage = learning_path::stage_for_lesson(self.lesson);
        ui.small(format!(
            "{} · {}",
            learning_path::STAGES[stage].name,
            learning_path::STAGES[stage].objective
        ));
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
                if self.quiz_choice != Some(index) {
                    self.quiz_attempts[self.lesson] =
                        self.quiz_attempts[self.lesson].saturating_add(1);
                    if index != quiz.correct {
                        self.quiz_mistakes[self.lesson] =
                            self.quiz_mistakes[self.lesson].saturating_add(1);
                    }
                }
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
            "已完成 {completed}/{} 个知识测验（不是实战掌握证明）",
            lessons::LESSONS.len()
        ));
        let mistakes = self.quiz_mistakes[self.lesson];
        let attempts = self.quiz_attempts[self.lesson];
        if attempts > 0 {
            ui.small(format!(
                "本知识点答题 {attempts} 次，其中答错 {mistakes} 次；答错题建议继续练习。"
            ));
        }
        self.render_practice(ui);
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
                self.discard_on_open = None;
                self.restore_queue.clear();
                self.restore_states.clear();
                self.restore_active = None;
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
        if !self.poll_startup_state(ctx) {
            return;
        }
        self.poll_recovery_export(ctx);
        self.poll_document_io(ctx);
        self.poll_cargo();
        self.poll_practice();
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
            if let Some(stamp) = self
                .file_watch
                .as_ref()
                .and_then(|watch| watch.take_changed_stamp())
            {
                self.apply_external_stamp(stamp);
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
        self.dispatch_lsp_debounced();
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
            || self.practice_task.is_some()
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
        if ctx.input(|i| i.viewport().close_requested()) {
            self.sync_active_document();
            if self.close_requires_confirmation() {
                if self.saving.is_some() {
                    self.after_save_close = true;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_close = true;
            }
        }
        if self.confirm_close {
            egui::Modal::new(egui::Id::new("confirm_close")).show(ctx, |ui| {
                let durability_uncertain = self.status.contains("持久性未确认");
                let dirty_count = self.documents.dirty_count();
                ui.heading(if durability_uncertain {
                    "内容已提交，但目录持久性未确认"
                } else {
                    "关闭 RustBridge"
                });
                ui.label(format!(
                    "打开文档：{} 个；未保存文档：{} 个。",
                    self.documents.len(),
                    dirty_count
                ));
                ui.label(&self.status);
                if self.saving.is_some() {
                    ui.spinner();
                    ui.label("已接受的安全保存正在完成；关闭或放弃操作暂不可用。");
                } else if durability_uncertain {
                    if ui.button("确认风险并关闭").clicked() {
                        self.confirm_close = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                } else {
                    ui.horizontal(|ui| {
                        if dirty_count > 0 && ui.button("全部保存并关闭").clicked() {
                            self.after_save_close = true;
                            self.save_all_documents();
                        }
                        if dirty_count > 0 && ui.button("全部放弃并关闭").clicked() {
                            self.sync_active_document();
                            for path in self.documents.paths() {
                                if let Some(worker) = self.state_worker.as_ref() {
                                    worker.send(persistence::Job::DeleteJournal(path));
                                }
                            }
                            self.documents.clear();
                            self.file = None;
                            self.dirty = false;
                            self.confirm_close = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if dirty_count == 0 && ui.button("关闭").clicked() {
                            self.confirm_close = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("取消").clicked() {
                            self.after_save_close = false;
                            self.confirm_close = false;
                        }
                    });
                }
            });
        }

        if (self.pending_open.is_some() || self.pending_project.is_some()) && !self.confirm_close {
            egui::Modal::new(egui::Id::new("confirm_open")).show(ctx, |ui| {
                let switching_project = self.pending_project.is_some();
                let dirty_count = self.documents.dirty_count();
                let recovery_count = self.documents.recovery_count();
                ui.heading(if switching_project {
                    "切换项目"
                } else {
                    "重新加载磁盘版本"
                });
                ui.label(if switching_project {
                    format!(
                        "当前有 {dirty_count} 个未保存文档、{recovery_count} 个待恢复文档。切换项目会关闭这些标签；待恢复快照可以保留。"
                    )
                } else {
                    "重新加载将丢弃当前文档缓冲区；其他打开的文档不会受影响。".to_owned()
                });
                ui.label(&self.status);

                if self.saving.is_some() {
                    ui.spinner();
                    ui.label("已接受的保存任务仍在执行。");
                } else {
                    let durability_uncertain = self.status.contains("持久性未确认");
                    if durability_uncertain && dirty_count == 0 {
                        if ui.button("确认风险并继续").clicked() {
                            if let Some(folder) = self.pending_project.take() {
                                self.apply_project(folder);
                            } else if let Some(path) = self.pending_open.take() {
                                self.reload_file_from_disk(path);
                            }
                        }
                    } else {
                        ui.horizontal(|ui| {
                            if switching_project {
                                if dirty_count > 0
                                    && ui.button("全部保存并切换").clicked()
                                {
                                    self.save_all_documents();
                                }
                                if dirty_count > 0
                                    && ui.button("全部放弃并切换").clicked()
                                {
                                    for path in self.documents.paths() {
                                        if let Some(worker) = self.state_worker.as_ref() {
                                            worker.send(persistence::Job::DeleteJournal(path));
                                        }
                                    }
                                    if let Some(folder) = self.pending_project.take() {
                                        self.apply_project(folder);
                                    }
                                }
                                if dirty_count == 0 && recovery_count > 0 {
                                    if ui.button("保留恢复记录并切换").clicked()
                                        && let Some(folder) = self.pending_project.take()
                                    {
                                        self.apply_project(folder);
                                    }
                                    if ui.button("删除恢复记录并切换").clicked() {
                                        for path in self.documents.paths() {
                                            if let Some(worker) = self.state_worker.as_ref() {
                                                worker.send(persistence::Job::DeleteJournal(path));
                                            }
                                        }
                                        if let Some(folder) = self.pending_project.take() {
                                            self.apply_project(folder);
                                        }
                                    }
                                }
                                if dirty_count == 0
                                    && recovery_count == 0
                                    && let Some(folder) = self.pending_project.take()
                                {
                                    self.apply_project(folder);
                                }
                            } else if self.dirty {
                                if ui.button("保存并重新加载").clicked() {
                                    self.after_save_open = true;
                                    self.save();
                                }
                                if ui.button("放弃当前修改并重新加载").clicked()
                                    && let Some(path) = self.pending_open.take()
                                {
                                    self.reload_file_from_disk(path);
                                }
                            } else if let Some(path) = self.pending_open.take() {
                                self.reload_file_from_disk(path);
                            }
                            if ui.button("取消").clicked() {
                                self.pending_open = None;
                                self.pending_project = None;
                                self.after_save_open = false;
                                self.after_save_project = false;
                            }
                        });
                    }
                }
            });
        }

        if let Some(path) = self.pending_close_document.clone()
            && !self.confirm_close
            && self.pending_project.is_none()
            && self.pending_open.is_none()
        {
            egui::Modal::new(egui::Id::new("close_document")).show(ctx, |ui| {
                let dirty = self.documents.get(&path).is_some_and(|doc| doc.is_dirty());
                ui.heading("关闭文档标签");
                ui.label(path.display().to_string());
                if self
                    .saving
                    .as_ref()
                    .is_some_and(|(_, saving_path, _)| saving_path == &path)
                {
                    ui.spinner();
                    ui.label("正在保存；保存完成后会自动关闭标签。");
                } else if dirty {
                    ui.label("该文档有独立的未保存修改。");
                    ui.horizontal(|ui| {
                        if ui.button("保存并关闭").clicked() {
                            self.start_save_path(path.clone());
                        }
                        if ui.button("放弃并关闭").clicked() {
                            self.close_document_now(path.clone(), true);
                        }
                        if ui.button("取消").clicked() {
                            self.pending_close_document = None;
                        }
                    });
                } else {
                    self.close_document_now(path.clone(), false);
                }
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
                        self.documents.set_pending_recovery(&journal.path, None);
                        self.rust = journal.edited.clone();
                        self.update_projection();
                        // Restoring an editable buffer must actually enter the
                        // Rust editor rather than leave users in read-only Java mode.
                        self.mode = Mode::Rust;
                        self.cursor_byte = self.rust.len();
                        self.editor_jump = Some(parser::ByteSpan {
                            start: self.cursor_byte,
                            end: self.cursor_byte,
                        });
                        self.dirty = true;
                        self.mark_dirty_for_recovery();
                        self.status = "已恢复未保存缓冲区；尚未覆盖磁盘文件。".into();
                    }
                    if ui.button("导出恢复副本").clicked() {
                        let destination = journal.path.with_extension("rustbridge-recovery.rs");
                        self.start_recovery_export(journal.path.clone(), destination);
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
        let (
            save_shortcut,
            mode_shortcut,
            page_delta,
            close_tab_shortcut,
            document_cycle,
            back_shortcut,
        ) = ctx.input(|i| {
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
            let close_tab = mod_pressed && i.key_pressed(egui::Key::W);
            let cycle = if mod_pressed && i.key_pressed(egui::Key::Tab) {
                Some(if i.modifiers.shift { -1 } else { 1 })
            } else {
                None
            };
            (
                mod_pressed && i.key_pressed(egui::Key::S),
                requested_mode,
                page_delta,
                close_tab,
                cycle,
                i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft),
            )
        });
        if back_shortcut {
            self.navigate_back();
        }
        if let Some(mode) = mode_shortcut {
            self.mode = mode;
            self.last_scroll = None;
            self.jump_to = self.selected;
        }
        if close_tab_shortcut && let Some(path) = self.file.clone() {
            self.request_close_document(path);
        }
        if let Some(direction) = document_cycle {
            self.cycle_document(direction);
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
        self.render_workbench(ctx);
        if save_shortcut {
            self.save();
            ctx.request_repaint();
        }
        self.poll_persistence();
        if self.recovery_due.is_some()
            || self.session_due.is_some()
            || self.hover_due.is_some()
            || self.completion_due.is_some()
        {
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

fn same_document_path(left: &Path, right: &Path) -> bool {
    // Identical open-tab paths are common on every keystroke, save completion
    // and session update. Avoid touching the filesystem in the common case.
    if left == right {
        return true;
    }
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
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

fn find_cargo_root(path: &Path) -> Option<PathBuf> {
    let start = if path.is_dir() { path } else { path.parent()? };
    start
        .ancestors()
        .find(|candidate| candidate.join("Cargo.toml").is_file())
        .map(Path::to_path_buf)
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
            let mut app = App {
                startup_explicit_path: startup_path,
                ..App::default()
            };
            let restore_last_document = app.startup_explicit_path.is_none();
            app.initialize_state(restore_last_document);
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod file_tests {
    use super::*;
    use crate::editor_core::{append_rust_syntax, editor_page_bounds};

    #[test]
    fn project_switch_waits_for_unsaved_decision_and_resets_old_document_context() {
        let root =
            std::env::temp_dir().join(format!("rustbridge-project-switch-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"switch-target\"\n",
        )
        .unwrap();

        let mut app = App {
            file: Some(PathBuf::from("/tmp/old-project/src/main.rs")),
            ..App::default()
        };
        app.rust.push_str("\n// unsaved old project edit");
        app.dirty = true;
        app.open_project(root.clone());

        assert_eq!(app.root, None);
        assert_eq!(app.pending_project, Some(root.clone()));
        assert!(app.dirty);
        assert_eq!(
            app.file,
            Some(PathBuf::from("/tmp/old-project/src/main.rs"))
        );

        // This is the explicit Discard-and-continue branch used by the modal.
        app.discard_current_recovery();
        app.dirty = false;
        let next = app.pending_project.take().unwrap();
        app.apply_project(next);

        assert_eq!(app.root, Some(root.clone()));
        assert!(app.cargo_project);
        assert!(app.file.is_none());
        assert!(!app.dirty);
        assert!(app.rust.contains("fn main"));
        assert!(matches!(app.mode, Mode::Java));
        assert!(app.pending_project.is_none());
        fs::remove_dir_all(root).unwrap();
    }

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
            documents: vec![persistence::DocumentSession {
                path: file.clone(),
                cursor_byte: 3,
                mode: "compare".into(),
            }],
            ..persistence::Session::default()
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
    fn two_open_documents_keep_independent_unsaved_buffers_and_save_baselines() {
        let root = std::env::temp_dir().join(format!(
            "rustbridge-document-manager-app-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"doc-manager-test\"\n",
        )
        .unwrap();
        let a = root.join("src/a.rs");
        let b = root.join("src/b.rs");
        fs::write(&a, "fn a() {}").unwrap();
        fs::write(&b, "fn b() {}").unwrap();

        let mut app = App::default();
        app.open_file(a.clone());
        app.wait_for_document_io();
        let edit_a = "\n// unsaved A";
        let start_a = app.rust.len();
        app.rust.push_str(edit_a);
        app.source_edited(start_a..start_a, edit_a);
        app.dirty = true;
        app.sync_active_document();

        app.open(b.clone());
        app.wait_for_document_io();
        assert_eq!(app.rust, "fn b() {}");
        let edit_b = "\n// unsaved B";
        let start_b = app.rust.len();
        app.rust.push_str(edit_b);
        app.source_edited(start_b..start_b, edit_b);
        app.dirty = true;
        app.sync_active_document();

        app.open(a.clone());
        assert!(app.dirty);
        assert!(app.rust.contains("unsaved A"));
        app.open(b.clone());
        assert!(app.dirty);
        assert!(app.rust.contains("unsaved B"));

        app.save();
        app.wait_for_document_io();
        assert_eq!(fs::read_to_string(&b).unwrap(), "fn b() {}\n// unsaved B");
        assert!(app.documents.get(&a).unwrap().is_dirty());
        assert!(!app.documents.get(&b).unwrap().is_dirty());

        app.open(a.clone());
        assert!(app.dirty);
        assert!(app.rust.contains("unsaved A"));

        app.save_all_documents();
        app.wait_for_document_io();
        assert_eq!(fs::read_to_string(&a).unwrap(), "fn a() {}\n// unsaved A");
        assert!(!app.documents.get(&a).unwrap().is_dirty());
        assert!(!app.documents.get(&b).unwrap().is_dirty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn syncing_cursor_and_mode_does_not_clone_document_source_each_frame() {
        let root =
            std::env::temp_dir().join(format!("rustbridge-sync-no-clone-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"sync-no-clone\"\n",
        )
        .unwrap();
        let path = root.join("src/main.rs");
        fs::write(&path, "fn main() {}").unwrap();

        let mut app = App::default();
        app.open_file(path.clone());
        app.wait_for_document_io();
        let original_ptr = app.documents.get(&path).unwrap().source.as_ptr();

        app.sync_active_document();
        assert_eq!(
            app.documents.get(&path).unwrap().source.as_ptr(),
            original_ptr
        );

        app.cursor_byte = 4;
        app.sync_active_document();
        assert_eq!(
            app.documents.get(&path).unwrap().source.as_ptr(),
            original_ptr
        );

        app.mode = Mode::Compare;
        app.sync_active_document();
        assert_eq!(
            app.documents.get(&path).unwrap().source.as_ptr(),
            original_ptr
        );

        let insertion = " // edited";
        let offset = app.rust.len();
        app.rust.push_str(insertion);
        app.source_edited(offset..offset, insertion);
        app.dirty = true;
        app.sync_active_document();
        assert_ne!(
            app.documents.get(&path).unwrap().source.as_ptr(),
            original_ptr
        );
        assert!(app.documents.get(&path).unwrap().is_dirty());

        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initial_restored_document_recovers_even_without_a_session_file() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rustbridge-first-recovery-{}-{nonce}",
            std::process::id()
        ));
        let store = persistence::Store::new(root.join("state")).unwrap();
        let path = root.join("first.rs");
        fs::write(&path, "fn baseline() {}").unwrap();
        store
            .save_journal(&path, "fn baseline() {}", "fn edited_and_uncommitted() {}")
            .unwrap();
        // Intentional: no session.json exists. Startup must discover the
        // orphaned tab from its private recovery metadata.
        let mut app = App::default();
        app.initialize_state_from_store(store.clone(), true);
        app.wait_for_document_io();
        let recovery = app
            .documents
            .get(&path)
            .and_then(|doc| doc.pending_recovery.as_ref())
            .expect("first restored document must retain its journal");
        assert_eq!(recovery.edited, "fn edited_and_uncommitted() {}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "fn baseline() {}");
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restoring_a_session_reopens_every_document_and_keeps_recovery_per_tab() {
        let root =
            std::env::temp_dir().join(format!("rustbridge-session-docs-{}", std::process::id()));
        let state_dir = std::env::temp_dir().join(format!(
            "rustbridge-session-state-multi-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"session-docs-test\"\n",
        )
        .unwrap();
        let a = root.join("src/a.rs");
        let b = root.join("src/b.rs");
        let bad = root.join("src/not-rust.txt");
        fs::write(&a, "fn a() {}").unwrap();
        fs::write(&b, "fn b() {}").unwrap();
        fs::write(&bad, "not a Rust source file").unwrap();

        let store = persistence::Store::new(state_dir.clone()).unwrap();
        store
            .save_session(&persistence::Session {
                root: Some(root.clone()),
                file: Some(b.clone()),
                mode: "compare".into(),
                documents: vec![
                    persistence::DocumentSession {
                        path: bad,
                        cursor_byte: 0,
                        mode: "rust".into(),
                    },
                    persistence::DocumentSession {
                        path: a.clone(),
                        cursor_byte: 3,
                        mode: "rust".into(),
                    },
                    persistence::DocumentSession {
                        path: b.clone(),
                        cursor_byte: 5,
                        mode: "compare".into(),
                    },
                ],
                ..persistence::Session::default()
            })
            .unwrap();
        store
            .save_journal(&a, "fn a() {}", "fn a() { /* recovered A */ }")
            .unwrap();
        store
            .save_journal(&b, "fn b() {}", "fn b() { /* recovered B */ }")
            .unwrap();

        let mut app = App::default();
        app.initialize_state_from_store(store, true);
        let in_flight_session = app.session_snapshot();
        assert_eq!(in_flight_session.documents.len(), 3);
        assert_eq!(in_flight_session.file, Some(b.clone()));
        assert_eq!(in_flight_session.cursor_byte, 5);

        // Opening another file while the first restored tab is still loading
        // must not drop that in-flight tab from the restore queue.
        let c = root.join("src/c.rs");
        fs::write(&c, "fn c() {}").unwrap();
        app.open(c.clone());
        app.wait_for_document_io();

        assert_eq!(app.documents.len(), 3);
        assert_eq!(app.file.as_ref(), Some(&c));
        assert!(app.documents.get(&a).unwrap().pending_recovery.is_some());
        assert!(app.documents.get(&b).unwrap().pending_recovery.is_some());
        app.open(b.clone());
        assert_eq!(
            app.pending_recovery
                .as_ref()
                .map(|journal| journal.path.clone()),
            Some(b)
        );

        drop(app);
        fs::remove_dir_all(state_dir).unwrap();
        fs::remove_dir_all(root).unwrap();
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
        let target = std::env::temp_dir().join("this-path-does-not-exist-failed-open.rs");
        app.discard_on_open = Some(target.clone());
        app.open_file(target);
        app.wait_for_document_io();
        assert_eq!(app.rust, before);
        assert!(app.dirty);
        assert!(app.discard_on_open.is_none());
    }
    #[test]
    fn discard_intent_is_bound_to_its_original_document_path() {
        let mut app = App::default();
        let target = std::env::temp_dir().join("rustbridge-discard-target.rs");
        let other = std::env::temp_dir().join("rustbridge-other-target.rs");
        app.discard_on_open = Some(target);
        app.open_file(other);
        assert!(app.discard_on_open.is_none());
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
    #[test]
    fn live_dirty_count_updates_before_large_buffer_is_checkpointed() {
        let root =
            std::env::temp_dir().join(format!("rustbridge-live-dirty-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("main.rs");
        fs::write(&path, "fn main() {}").unwrap();
        let mut app = App::default();
        app.open_file(path.clone());
        app.wait_for_document_io();
        assert_eq!(app.live_dirty_count(), 0);
        let offset = app.rust.len();
        let patch = "\n// pending checkpoint";
        app.rust.push_str(patch);
        app.source_edited(offset..offset, patch);
        app.dirty = true;
        app.mark_dirty_for_recovery();
        assert_eq!(app.live_dirty_count(), 1);
        // Repaint/persistence polling must not copy the entire source just to
        // update cursor and tab metadata.
        app.poll_persistence();
        assert!(!app.documents.get(&path).unwrap().is_dirty());
        app.sync_active_document();
        assert_eq!(app.documents.dirty_count(), 1);
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_all_skips_conflict_and_keeps_saving_remaining_documents() {
        let root = std::env::temp_dir().join(format!(
            "rustbridge-save-all-continue-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let a = root.join("a.rs");
        let b = root.join("b.rs");
        fs::write(&a, "pub fn a() {}").unwrap();
        fs::write(&b, "pub fn b() {}").unwrap();
        let mut app = App::default();
        for (file, text) in [(&a, " // unsaved A"), (&b, " // unsaved B")] {
            app.open_file(file.clone());
            app.wait_for_document_io();
            let offset = app.rust.len();
            app.rust.push_str(text);
            app.source_edited(offset..offset, text);
            app.dirty = true;
            app.sync_active_document();
        }
        app.documents.set_external_change(&a, true);
        app.save_all_documents();
        app.wait_for_document_io();
        assert_eq!(fs::read_to_string(&a).unwrap(), "pub fn a() {}");
        assert_eq!(
            fs::read_to_string(&b).unwrap(),
            "pub fn b() {} // unsaved B"
        );
        assert!(app.documents.get(&a).unwrap().is_dirty());
        assert!(!app.documents.get(&b).unwrap().is_dirty());
        assert_eq!(app.save_errors.len(), 1, "one conflict should be reported");
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn edits_during_an_accepted_save_remain_dirty_and_can_be_saved_again() {
        let root = std::env::temp_dir().join(format!(
            "rustbridge-edits-while-saving-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("main.rs");
        fs::write(&path, "fn main() {}").unwrap();

        let mut app = App::default();
        app.open_file(path.clone());
        app.wait_for_document_io();

        let first = "\n// first revision";
        let start = app.rust.len();
        app.rust.push_str(first);
        app.source_edited(start..start, first);
        app.dirty = true;
        app.save();
        assert!(app.saving.is_some());

        // The I/O worker can finish before UI polling; editing in the meantime
        // must never become a phantom clean buffer or lose its recovery journal.
        let second = "\n// newer revision while save was in flight";
        let start = app.rust.len();
        app.rust.push_str(second);
        app.source_edited(start..start, second);
        app.dirty = true;
        app.mark_dirty_for_recovery();

        app.wait_for_document_io();
        // A successful first save can enqueue another save for the newer
        // revision. Either the later edit must remain dirty, or the automatic
        // second save must have durably persisted it. It must never disappear.
        let disk = fs::read_to_string(&path).unwrap();
        assert_eq!(
            app.documents.get(&path).unwrap().source,
            app.rust,
            "manager lost post-save edits"
        );
        if disk != app.rust {
            assert_eq!(disk, "fn main() {}\n// first revision");
            assert!(app.dirty);
            assert!(app.documents.get(&path).unwrap().is_dirty());
            app.save();
            app.wait_for_document_io();
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), app.rust);
        assert!(!app.dirty);
        assert!(!app.documents.get(&path).unwrap().is_dirty());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }
}
