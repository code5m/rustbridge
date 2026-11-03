mod code_view;
mod lessons;
mod parser;

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

struct App {
    root: Option<PathBuf>,
    file: Option<PathBuf>,
    rust: String,
    projection: parser::Projection,
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
    status: String,
}

impl Default for App {
    fn default() -> Self {
        let rust = include_str!("../examples/basics.rs").to_owned();
        Self {
            root: None,
            file: None,
            projection: parser::project(&rust),
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
            status: "Java view is an AST-aligned teaching aid, not compilable Java.".into(),
        }
    }
}

impl App {
    fn open(&mut self, path: PathBuf) {
        if self.dirty {
            self.status = "Save your changes before switching files.".into();
            return;
        }
        match fs::read_to_string(&path) {
            Ok(text) => {
                self.saved_source = text.clone();
                self.rust = text;
                self.projection = parser::project(&self.rust);
                self.file = Some(path);
                self.selected = None;
                self.jump_to = None;
                self.scroll_follow = None;
                self.last_scroll = None;
                self.mode = Mode::Java;
                self.status = "Opened actual Rust file. Java teaching view is read-only.".into();
            }
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    fn save(&mut self) {
        match &self.file {
            Some(path) => match save_if_unchanged(path, &self.saved_source, &self.rust) {
                Ok(()) => {
                    self.dirty = false;
                    self.saved_source.clone_from(&self.rust);
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
        let Ok(dir_entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = dir_entries.flatten().collect();
        entries.sort_by_key(|entry| (!entry.path().is_dir(), entry.file_name()));
        for entry in entries.into_iter().take(180) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if [".git", "target", "node_modules"].contains(&name.as_str()) {
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
        let active = self
            .selected
            .and_then(|idx| self.projection.anchors.get(idx))
            .map(|anchor| anchor.rust);
        let jump = self
            .jump_to
            .take()
            .and_then(|idx| self.projection.anchors.get(idx))
            .map(|anchor| anchor.rust);

        // Highlight the selected source token without changing the file or transforming its syntax.
        let mut layouter = |ui: &egui::Ui, content: &dyn egui::TextBuffer, width: f32| {
            let text = content.as_str();
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = width;
            let font = egui::TextStyle::Monospace.resolve(ui.style());
            let format = |marked: bool| egui::TextFormat {
                font_id: font.clone(),
                color: ui.visuals().text_color(),
                background: if marked {
                    Color32::from_rgb(53, 103, 155)
                } else {
                    Color32::TRANSPARENT
                },
                ..Default::default()
            };
            if let Some(span) = active.filter(|span| {
                span.end <= text.len()
                    && text.is_char_boundary(span.start)
                    && text.is_char_boundary(span.end)
            }) {
                job.append(&text[..span.start], 0.0, format(false));
                job.append(&text[span.start..span.end], 0.0, format(true));
                job.append(&text[span.end..], 0.0, format(false));
            } else {
                job.append(text, 0.0, format(false));
            }
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
            self.projection = parser::project(&self.rust);
            self.dirty = self.rust != self.saved_source;
            self.selected = None;
            self.status = "Source edited; AST source map rebuilt.".into();
        }
        if output.response.clicked()
            && let Some(cursor) = output.cursor_range
        {
            let byte = parser::char_to_byte(&self.rust, cursor.primary.index);
            self.selected = self.projection.rust_anchor(byte).or_else(|| {
                byte.checked_sub(1)
                    .and_then(|before| self.projection.rust_anchor(before))
            });
        }
        if let Some(span) = jump
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
        ui.label(lesson.why);
        ui.separator();
        ui.columns(2, |columns| {
            columns[0].heading("Java 8");
            columns[0].monospace(lesson.java_code);
            columns[1].heading("Rust");
            columns[1].monospace(lesson.rust_code);
        });
        ui.separator();
        ui.horizontal(|ui| {
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
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if self.confirm_close {
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Your changes have not been saved.");
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
                    self.root = Some(folder);
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
            });
        });
        egui::SidePanel::left("files")
            .resizable(true)
            .default_width(210.)
            .show(ctx, |ui| {
                ui.heading("Real project files");
                ui.small("Filesystem truth; .rs filter");
                ui.separator();
                if let Some(root) = self.root.clone() {
                    ui.strong(root.file_name().unwrap_or_default().to_string_lossy());
                    egui::ScrollArea::vertical()
                        .max_height(300.)
                        .show(ui, |ui| self.directory(ui, &root, 0));
                } else {
                    ui.label("Open folder to browse the actual project.");
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
}
