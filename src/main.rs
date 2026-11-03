mod analyzer;
mod code_view;
mod lessons;
mod parser;
mod projection_worker;

use eframe::egui::{self, Color32};
use std::{
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
}

fn append_rust_syntax(
    job: &mut egui::text::LayoutJob,
    text: &str,
    font: &egui::FontId,
    base: Color32,
    active: Option<parser::ByteSpan>,
) {
    let format = |color, start: usize, end: usize| egui::TextFormat {
        font_id: font.clone(),
        color,
        background: if active.is_some_and(|span| span.start < end && start < span.end) {
            Color32::from_rgb(53, 103, 155)
        } else {
            Color32::TRANSPARENT
        },
        ..Default::default()
    };
    let keyword = |word: &str| {
        matches!(
            word,
            "as" | "async"
                | "await"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "dyn"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
        )
    };
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        let end = if rest.starts_with("//") {
            index + rest.find('\n').unwrap_or(rest.len())
        } else if let Some(after_quote) = rest.strip_prefix('"') {
            let mut escaped = false;
            let mut length = 1;
            for character in after_quote.chars() {
                length += character.len_utf8();
                if character == '"' && !escaped {
                    break;
                }
                escaped = character == '\\' && !escaped;
                if character != '\\' {
                    escaped = false;
                }
            }
            index + length
        } else if rest
            .chars()
            .next()
            .is_some_and(|character| character.is_alphabetic() || character == '_')
        {
            index
                + rest
                    .char_indices()
                    .find(|(_, character)| !character.is_alphanumeric() && *character != '_')
                    .map_or(rest.len(), |(end, _)| end)
        } else {
            index + rest.chars().next().unwrap().len_utf8()
        };
        let token = &text[index..end];
        let color = if token.starts_with("//") {
            Color32::from_rgb(112, 142, 112)
        } else if token.starts_with('"') {
            Color32::from_rgb(202, 162, 98)
        } else if keyword(token) {
            Color32::from_rgb(126, 188, 247)
        } else {
            base
        };
        job.append(token, 0.0, format(color, index, end));
        index = end;
    }
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
    file_query: String,
    search_results: Vec<PathBuf>,
    disk_stamp: Option<FileStamp>,
    external_change: bool,
    next_disk_check: std::time::Instant,
    editor_query: String,
    editor_find: Option<parser::ByteSpan>,
    editor_jump: Option<parser::ByteSpan>,
    lesson_return: Option<(Mode, Option<usize>)>,
    analyzer_status: String,
    analyzer_check: Option<std::sync::mpsc::Receiver<String>>,
    status: String,
}

impl Default for App {
    fn default() -> Self {
        let rust = include_str!("../examples/basics.rs").to_owned();
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
            file_query: String::new(),
            search_results: Vec::new(),
            disk_stamp: None,
            external_change: false,
            next_disk_check: std::time::Instant::now(),
            editor_query: String::new(),
            editor_find: None,
            editor_jump: None,
            lesson_return: None,
            analyzer_status: "Not checked; core reading does not require rust-analyzer.".into(),
            analyzer_check: None,
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
                self.disk_stamp = file_stamp(&path).ok();
                self.external_change = false;
                self.saved_source = text.clone();
                self.rust = text;
                self.dirty = false;
                self.update_projection();
                self.file = Some(path);
                self.selected = None;
                self.jump_to = None;
                self.editor_find = None;
                self.editor_jump = None;
                self.scroll_follow = None;
                self.last_scroll = None;
                self.mode = Mode::Java;
                self.status = "Opened actual Rust file. Java teaching view is read-only.".into();
            }
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    fn update_projection(&mut self) {
        self.revision += 1;
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
        if depth > 7 {
            return;
        }
        let dir_entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => {
                ui.colored_label(Color32::YELLOW, format!("Cannot read folder: {error}"));
                return;
            }
        };
        let mut entries = Vec::new();
        for entry in dir_entries {
            match entry {
                Ok(entry) => entries.push(entry),
                Err(error) => {
                    ui.colored_label(Color32::YELLOW, format!("Cannot read entry: {error}"));
                }
            }
        }
        entries.sort_by_key(|entry| (!entry.path().is_dir(), entry.file_name()));
        for entry in entries.into_iter().take(180) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !self.show_excluded
                    && [".git", "target", "node_modules"].contains(&name.as_str())
                {
                    continue;
                }
                egui::CollapsingHeader::new(format!("▸ {name}"))
                    .id_salt(&path)
                    .show(ui, |ui| self.directory(ui, &path, depth + 1));
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && ui
                    .selectable_label(self.file.as_ref() == Some(&path), format!("  {name}"))
                    .clicked()
            {
                self.open(path);
            }
        }
    }

    fn refresh_file_search(&mut self) {
        self.search_results.clear();
        let Some(root) = self.root.clone() else {
            return;
        };
        let query = self.file_query.trim().to_lowercase();
        if !query.is_empty() {
            self.find_rust_files(&root, &query, 0);
        }
    }

    fn find_rust_files(&mut self, dir: &Path, query: &str, depth: usize) {
        if depth > 12 || self.search_results.len() >= 100 {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if self.search_results.len() >= 100 {
                break;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !self.show_excluded
                    && [".git", "target", "node_modules"].contains(&name.as_str())
                {
                    continue;
                }
                self.find_rust_files(&path, query, depth + 1);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && name.to_lowercase().contains(query)
            {
                self.search_results.push(path);
            }
        }
    }

    fn check_external_change(&mut self) {
        let Some(path) = self.file.as_ref() else {
            return;
        };
        let current = file_stamp(path).ok();
        if current != self.disk_stamp {
            self.external_change = true;
            self.status =
                "File changed outside RustBridge; save is blocked until you reopen it.".into();
        }
    }

    fn check_analyzer(&mut self) {
        if self.analyzer_check.is_none() {
            self.analyzer_status = "Checking rust-analyzer in the background…".into();
            self.analyzer_check = Some(analyzer::check_async());
        }
    }

    fn find_next_in_editor(&mut self) {
        let query = self.editor_query.trim();
        if query.is_empty() {
            self.editor_find = None;
            self.editor_jump = None;
            return;
        }
        let start = self.editor_find.map_or(0, |span| span.end);
        let offset = self.rust[start..]
            .find(query)
            .or_else(|| self.rust[..start].find(query));
        if let Some(offset) = offset {
            let span = parser::ByteSpan {
                start: if offset < start {
                    offset
                } else {
                    start + offset
                },
                end: if offset < start {
                    offset
                } else {
                    start + offset
                } + query.len(),
            };
            self.editor_find = Some(span);
            self.editor_jump = Some(span);
            self.status = format!("Found Rust text at byte {}.", span.start);
        } else {
            self.editor_find = None;
            self.status = "Text not found in the original Rust source.".into();
        }
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
            "Reference and borrowing" => "Ownership + Borrowing",
            _ => return None,
        };
        lessons::LESSONS
            .iter()
            .position(|lesson| lesson.rust == rust)
    }

    fn read_only(&mut self, ui: &mut egui::Ui, java: bool) {
        let jump = self.jump_to.take();
        let result =
            if java {
                ui.colored_label(Color32::YELLOW,
                "Java mental model · click a highlighted symbol to reveal the paired Rust source");
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

    fn editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Real Rust source · editable");
            if ui.button("Save · Ctrl+S").clicked() {
                self.save();
            }
        });
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.editor_query)
                    .hint_text("Find in original Rust…")
                    .desired_width(220.),
            );
            if ui.button("Find next").clicked()
                || (response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
            {
                self.find_next_in_editor();
            }
        });
        let active = self
            .selected
            .and_then(|idx| self.projection.anchors.get(idx))
            .map(|anchor| anchor.rust)
            .or(self.editor_find);
        let jump = self
            .jump_to
            .take()
            .and_then(|idx| self.projection.anchors.get(idx))
            .map(|anchor| anchor.rust);
        let cursor_target = jump.or(self.editor_jump.take());

        // This is deliberately lexical highlighting only. Tree-sitter remains the source of links.
        let mut layouter = |ui: &egui::Ui, content: &dyn egui::TextBuffer, width: f32| {
            let text = content.as_str();
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = width;
            let font = egui::TextStyle::Monospace.resolve(ui.style());
            let active = active.filter(|span| {
                span.end <= text.len()
                    && text.is_char_boundary(span.start)
                    && text.is_char_boundary(span.end)
            });
            append_rust_syntax(&mut job, text, &font, ui.visuals().text_color(), active);
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };
        let mut output = egui::TextEdit::multiline(&mut self.rust)
            .id_salt(("rust_source_editor", &self.file))
            .font(egui::TextStyle::Monospace)
            .code_editor()
            .desired_width(f32::INFINITY)
            .layouter(&mut layouter)
            .show(ui);
        if output.response.changed() {
            self.update_projection();
            self.dirty = self.rust != self.saved_source;
            self.editor_find = None;
            self.editor_jump = None;
            self.selected = None;
            self.status = "Source edited; updating the reading projection.".into();
        }
        if !self.projection_pending
            && output.response.clicked()
            && let Some(cursor) = output.cursor_range
        {
            let byte = parser::char_to_byte(&self.rust, cursor.primary.index);
            self.selected = self.projection.rust_anchor(byte).or_else(|| {
                byte.checked_sub(1)
                    .and_then(|before| self.projection.rust_anchor(before))
            });
        }
        if let Some(span) = cursor_target
            && span.end <= self.rust.len()
        {
            use egui::text::{CCursor, CCursorRange};
            let start = self.rust[..span.start].chars().count();
            let end = self.rust[..span.end].chars().count();
            output.state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(start),
                CCursor::new(end),
            )));
            output.state.store(ui.ctx(), output.response.id);
            ui.scroll_to_cursor(Some(egui::Align::Center));
            ui.ctx().request_repaint();
        }
    }

    fn lessons(&mut self, ui: &mut egui::Ui) {
        let lesson = &lessons::LESSONS[self.lesson];
        ui.heading(format!(
            "Lesson {} / {}: {} → {}",
            self.lesson + 1,
            lessons::LESSONS.len(),
            lesson.java,
            lesson.rust
        ));
        ui.strong("Key difference");
        ui.label(lesson.why);
        ui.small("Examples are teaching sketches. Check the real Rust source and compiler before treating them as production code.");
        ui.separator();
        ui.columns(2, |columns| {
            columns[0].heading("Java 8 mental model");
            columns[0].monospace(lesson.java_code);
            columns[1].heading("Rust example");
            columns[1].monospace(lesson.rust_code);
        });
        ui.separator();
        ui.strong("Common mistake");
        ui.label(lesson.common_mistake);
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
        ui.heading("AST correspondence");
        ui.small("Precise token spans · syntax-level, not symbol resolution");
        ui.separator();
        if self.projection_pending {
            ui.label("Updating projection; links are temporarily unavailable.");
            return;
        }
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
        if std::time::Instant::now() >= self.next_disk_check {
            self.next_disk_check = std::time::Instant::now() + std::time::Duration::from_secs(1);
            self.check_external_change();
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
        if let Some(result) = self
            .analyzer_check
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.analyzer_status = result;
            self.analyzer_check = None;
        }
        if self.projection_pending {
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
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save();
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
                    (Mode::Java, "☕ Java View"),
                    (Mode::Rust, "🦀 Rust Source"),
                    (Mode::Compare, "◫ Linked Compare"),
                    (Mode::Learn, "Lessons"),
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
                }
            });
        });
        egui::SidePanel::left("files")
            .resizable(true)
            .default_width(210.)
            .show(ctx, |ui| {
                ui.heading("Real project files");
                ui.small("Filesystem truth; .rs filter");
                let excluded_changed = ui
                    .checkbox(
                        &mut self.show_excluded,
                        "Show .git, target and node_modules",
                    )
                    .changed();
                ui.small("At most 180 entries per folder; maximum depth 8.");
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
                        ui.small(format!(
                            "{} result(s), limited to 100",
                            self.search_results.len()
                        ));
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
                ui.separator();
                ui.strong("Optional rust-analyzer");
                ui.small("Availability only; no LSP, diagnostics, completion, or definition lookup is started.");
                ui.label(&self.analyzer_status);
                if ui.add_enabled(self.analyzer_check.is_none(), egui::Button::new("Check availability")).clicked() {
                    self.check_analyzer();
                }
                ui.separator();
                ui.strong("Java 8 → Rust lessons");
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
    }
}

fn save_if_unchanged(path: &Path, expected: &str, text: &str) -> std::io::Result<()> {
    let current = fs::read_to_string(path)?;
    if current != expected {
        return Err(std::io::Error::other(
            "File changed externally; save blocked to protect both versions. Copy your edits before reopening.",
        ));
    }
    fs::write(path, text)
}

fn file_stamp(path: &Path) -> std::io::Result<FileStamp> {
    let metadata = fs::metadata(path)?;
    Ok(FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn is_cargo_root(path: &Path) -> bool {
    path.join("Cargo.toml").is_file()
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1320., 820.])
            .with_min_inner_size([800., 550.]),
        ..Default::default()
    };
    eframe::run_native(
        "RustBridge — Java 8 → Rust",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::<App>::default())
        }),
    )
}

#[cfg(test)]
mod file_tests {
    use super::*;

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
        assert_eq!(app.search_results, vec![root.join("src/demo.rs")]);
        app.show_excluded = true;
        app.file_query = "generated".into();
        app.refresh_file_search();
        assert_eq!(app.search_results, vec![root.join("target/generated.rs")]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn known_ast_topics_open_the_matching_lesson() {
        let option = App::lesson_for_topic("Option<T> ↔ Optional<T>").unwrap();
        assert_eq!(lessons::LESSONS[option].rust, "Option<T>");
        assert!(App::lesson_for_topic("Rust symbol occurrence").is_none());
    }

    #[test]
    fn every_lesson_has_a_common_mistake() {
        assert!(
            lessons::LESSONS
                .iter()
                .all(|lesson| !lesson.common_mistake.is_empty())
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
}
