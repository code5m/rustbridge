mod analyzer;
mod code_view;
mod editor_core;
mod editor_eol;
mod editor_ui;
mod file_search_worker;
mod file_tree;
mod file_watch;
mod lessons;
mod parser;
mod projection_worker;
mod safe_save;
mod ui_fonts;

use editor_core::line_number_gutter;
use eframe::egui::{self, Color32};
use file_watch::{FileStamp, file_stamp};
use safe_save::save_if_unchanged;
use std::{
    collections::HashMap,
    fs,
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
    editor_query: String,
    editor_find: Option<parser::ByteSpan>,
    editor_jump: Option<parser::ByteSpan>,
    editor_line_starts: Vec<usize>,
    editor_gutter: String,
    editor_gutter_revision: u64,
    editor_page_start: usize,
    editor_page_target: usize,
    lesson_return: Option<(Mode, Option<usize>)>,
    analyzer_status: String,
    analyzer_check: Option<std::sync::mpsc::Receiver<String>>,
    analyzer_diagnostics: Option<analyzer::DiagnosticTask>,
    analyzer_diagnostics_status: String,
    status: String,
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
            editor_query: String::new(),
            editor_find: None,
            editor_jump: None,
            editor_line_starts,
            editor_gutter,
            editor_gutter_revision: 0,
            editor_page_start: 0,
            editor_page_target: 1,
            lesson_return: None,
            analyzer_status: "Not checked; core reading does not require rust-analyzer.".into(),
            analyzer_check: None,
            analyzer_diagnostics: None,
            analyzer_diagnostics_status: "Diagnostics are off until requested for a Cargo project."
                .into(),
            status: "Java view is an AST-aligned teaching aid, not compilable Java.".into(),
        }
    }
}

impl App {
    fn open(&mut self, path: PathBuf) {
        if self.dirty {
            self.pending_open = Some(path);
            return;
        }
        self.open_file(path);
    }

    fn open_file(&mut self, path: PathBuf) {
        match fs::read_to_string(&path) {
            Ok(text) => {
                self.file_watch = None;
                self.disk_stamp = file_stamp(&path).ok();
                self.external_change = false;
                self.saved_source = text.clone();
                self.rust = text;
                self.dirty = false;
                self.update_projection();
                self.file = Some(path);
                self.record_recent_file();
                self.selected = None;
                self.jump_to = None;
                self.editor_find = None;
                self.editor_jump = None;
                self.editor_page_start = 0;
                self.editor_page_target = 1;
                self.scroll_follow = None;
                self.last_scroll = None;
                self.mode = Mode::Java;
                self.status = "Opened actual Rust file. Java teaching view is read-only.".into();
            }
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    fn reload_disk_copy(&mut self) {
        if let Some(path) = self.file.clone() {
            self.open(path);
        }
    }

    fn update_projection(&mut self) {
        self.revision += 1;
        self.editor_line_starts = parser::line_starts(&self.rust);
        self.projection_pending = true;
        self.selected = None;
        self.jump_to = None;
        self.projection_due =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(200));
    }

    fn save(&mut self) {
        match &self.file {
            Some(path) => match save_if_unchanged(path, &self.saved_source, &self.rust) {
                Ok(()) => {
                    self.dirty = false;
                    self.saved_source.clone_from(&self.rust);
                    self.disk_stamp = file_stamp(path).ok();
                    self.external_change = false;
                    self.status = "Saved original Rust source".into();
                }
                Err(error) => self.status = format!("Save failed: {error}"),
            },
            None => self.status = "Example is not saved: open an existing .rs file first.".into(),
        }
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
            }
            if ui
                .add_enabled(
                    self.lesson + 1 < lessons::LESSONS.len(),
                    egui::Button::new("Next →"),
                )
                .clicked()
            {
                self.lesson += 1;
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
        } else {
            ui.label(
                "Choose a highlighted symbol in Java or Rust to see its exact paired occurrence.",
            );
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
        egui::ScrollArea::vertical().show(ui, |ui| {
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
        // Watch real disk metadata off the event thread. Idle windows do not
        // repeatedly repaint just to poll file stamps.
        if let Some(path) = self.file.clone() {
            if self
                .file_watch
                .as_ref()
                .is_none_or(|watch| watch.path() != path)
            {
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
        }
        if let Some((revision, projection)) = self.worker.as_ref().and_then(|worker| worker.take())
            && revision == self.revision
        {
            self.projection = projection;
            self.projection_pending = false;
            self.status = "Reading projection updated.".into();
        }
        self.update_file_search_results();
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
        if self.projection_pending
            || self.file_search_pending
            || self.analyzer_diagnostics.is_some()
            || self.analyzer_check.is_some()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if self.confirm_close {
            egui::Modal::new(egui::Id::new("confirm_close")).show(ctx, |ui| {
                ui.heading("Unsaved changes");
                ui.label("Your changes have not been saved.");
                ui.label(&self.status);
                ui.horizontal(|ui| {
                    if ui.button("Save and close").clicked() {
                        self.save();
                        if !self.dirty {
                            self.confirm_close = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    if ui.button("Discard and close").clicked() {
                        self.dirty = false;
                        self.confirm_close = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Cancel").clicked() {
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
                    if ui.button("Save and open").clicked() {
                        self.save();
                        if !self.dirty
                            && let Some(path) = self.pending_open.take()
                        {
                            self.open(path);
                        }
                    }
                    if ui.button("Discard and open").clicked()
                        && let Some(path) = self.pending_open.take()
                    {
                        self.open_file(path);
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_open = None;
                    }
                });
            });
        }
        // Capture Ctrl+S before TextEdit can consume the event, but only save
        // AFTER the editor has applied all changes from this frame.
        let save_shortcut =
            ctx.input(|i| (i.modifiers.ctrl || i.modifiers.command) && i.key_pressed(egui::Key::S));
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
                    if let Some(task) = self.analyzer_diagnostics.take() { task.cancel(); }
                    self.analyzer_diagnostics_status = "已切换项目；原项目诊断已取消。".into();
                    self.dir_index = file_tree::DirectoryIndex::new();
                    self.directory_limits.clear();
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
                ui.strong("Java 8 → Rust 对照课程");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (idx, lesson) in lessons::LESSONS.iter().enumerate() {
                        if ui
                            .selectable_label(
                                self.lesson == idx,
                                format!("{:02} · {}", idx + 1, lesson.java),
                            )
                            .clicked()
                        {
                            self.lesson = idx;
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
                    self.rust.lines().count()
                ));
            });
            ui.separator();
            self.body(ui);
        });
        if save_shortcut {
            self.save();
            ctx.request_repaint();
        }
    }
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
        let path = std::env::temp_dir().join(format!("rustbridge-reload-{}", std::process::id()));
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
