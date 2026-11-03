mod lessons;
mod parser;
use eframe::egui::{self, Color32, RichText};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, PartialEq)]
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
    note: usize,
    dirty: bool,
    status: String,
}
impl Default for App {
    fn default() -> Self {
        let rust = include_str!("../examples/basics.rs").to_string();
        let projection = parser::project(&rust);
        Self {
            root: None,
            file: None,
            rust,
            projection,
            mode: Mode::Java,
            lesson: 0,
            note: 0,
            dirty: false,
            status: "Example loaded. Java view is educational pseudocode.".into(),
        }
    }
}
impl App {
    fn open(&mut self, path: PathBuf) {
        if self.dirty {
            self.status = "Save current changes before switching files".into();
            return;
        }
        match fs::read_to_string(&path) {
            Ok(s) => {
                self.rust = s;
                self.file = Some(path);
                self.projection = parser::project(&self.rust);
                self.note = 0;
                self.status = "Opened original Rust source".into()
            }
            Err(e) => self.status = format!("Open failed: {e}"),
        }
    }
    fn save(&mut self) {
        if let Some(p) = &self.file {
            match fs::write(p, &self.rust) {
                Ok(()) => {
                    self.dirty = false;
                    self.status = "Saved original .rs file".into()
                }
                Err(e) => self.status = format!("Save error: {e}"),
            }
        } else {
            self.status = "Built-in example is read-only. Open an existing .rs file first.".into()
        }
    }
    fn directory(&mut self, ui: &mut egui::Ui, dir: &Path, depth: usize) {
        if depth > 7 {
            return;
        }
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name()));
        for e in entries.into_iter().take(160) {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if [".git", "target", "node_modules"].contains(&name.as_str()) {
                    continue;
                }
                egui::CollapsingHeader::new(format!("▸ {name}"))
                    .id_salt(path.clone())
                    .show(ui, |ui| self.directory(ui, &path, depth + 1));
            } else if path.extension().is_some_and(|x| x == "rs")
                && ui
                    .selectable_label(self.file.as_ref() == Some(&path), format!("  {name}"))
                    .clicked()
            {
                self.open(path);
            }
        }
    }
    fn body(&mut self, ui: &mut egui::Ui) {
        match self.mode {
            Mode::Java => {
                ui.label(
                    RichText::new(
                        "Java mental model · read-only, approximate (not compilable Java)",
                    )
                    .color(Color32::YELLOW),
                );
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.projection.java.as_str())
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                });
            }
            Mode::Rust => {
                ui.horizontal(|ui| {
                    ui.label("Real Rust source · editable");
                    if ui.button("Save Ctrl+S").clicked() {
                        self.save();
                    }
                });
                let r = ui.add_sized(
                    ui.available_size(),
                    egui::TextEdit::multiline(&mut self.rust)
                        .font(egui::TextStyle::Monospace)
                        .code_editor()
                        .desired_width(f32::INFINITY),
                );
                if r.changed() {
                    self.projection = parser::project(&self.rust);
                    self.dirty = true;
                }
            }
            Mode::Compare => {
                ui.columns(2, |cols| {
                    cols[0].label("Java-style teaching view");
                    cols[1].label("Original Rust source");
                    egui::ScrollArea::both()
                        .id_salt("java")
                        .show(&mut cols[0], |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.projection.java.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        });
                    egui::ScrollArea::both()
                        .id_salt("rust")
                        .show(&mut cols[1], |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.rust.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        });
                });
            }
            Mode::Learn => {
                let l = &lessons::LESSONS[self.lesson];
                ui.heading(format!(
                    "Lesson {}/{}: {} → {}",
                    self.lesson + 1,
                    lessons::LESSONS.len(),
                    l.java,
                    l.rust
                ));
                ui.label(l.why);
                ui.separator();
                ui.columns(2, |cols| {
                    cols[0].heading("Java 8");
                    cols[0].monospace(l.java_code);
                    cols[1].heading("Rust");
                    cols[1].monospace(l.rust_code);
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
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save();
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("RustBridge");
                ui.separator();
                if ui.button("Open .rs").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Rust", &["rs"])
                        .pick_file()
                {
                    self.open(p);
                }
                if ui.button("Open folder").clicked()
                    && let Some(p) = rfd::FileDialog::new().pick_folder()
                {
                    self.root = Some(p);
                }
                ui.separator();
                for (mode, label) in [
                    (Mode::Java, "☕ Java View"),
                    (Mode::Rust, "🦀 Rust Source"),
                    (Mode::Compare, "◫ Compare"),
                    (Mode::Learn, "Lessons"),
                ] {
                    if ui.selectable_label(self.mode == mode, label).clicked() {
                        self.mode = mode;
                    }
                }
                if self.dirty {
                    ui.colored_label(Color32::YELLOW, "● Unsaved");
                }
            });
        });
        egui::SidePanel::left("files")
            .resizable(true)
            .default_width(200.)
            .show(ctx, |ui| {
                ui.heading("Real project files");
                ui.small("Read from filesystem; .rs only");
                ui.separator();
                if let Some(root) = self.root.clone() {
                    egui::ScrollArea::vertical()
                        .max_height(310.)
                        .show(ui, |ui| self.directory(ui, &root, 0));
                } else {
                    ui.label("Open folder to browse real source.");
                }
                ui.separator();
                ui.strong("Java 8 → Rust lessons");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, l) in lessons::LESSONS.iter().enumerate() {
                        if ui
                            .selectable_label(self.lesson == i, format!("{:02}  {}", i + 1, l.java))
                            .clicked()
                        {
                            self.lesson = i;
                            self.mode = Mode::Learn;
                        }
                    }
                });
            });
        egui::SidePanel::right("notes")
            .resizable(true)
            .default_width(240.)
            .show(ctx, |ui| {
                ui.heading("Syntax guide");
                ui.small("Tree-sitter AST · line numbers");
                ui.separator();
                if self.projection.warning {
                    ui.colored_label(
                        Color32::YELLOW,
                        "Syntax errors: projection may be incomplete",
                    );
                }
                let notes = self.projection.notes.clone();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, n) in notes.iter().enumerate() {
                        if ui
                            .selectable_label(
                                self.note == i,
                                format!("L{}  {}", n.line + 1, n.topic),
                            )
                            .clicked()
                        {
                            self.note = i;
                        }
                    }
                    ui.separator();
                    if let Some(n) = notes.get(self.note) {
                        ui.strong(n.topic);
                        ui.label(n.detail);
                    }
                });
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.small(&self.status);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong(
                    self.file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "examples/basics.rs".into()),
                );
                ui.weak(format!(" · {} lines", self.rust.lines().count()));
            });
            ui.separator();
            self.body(ui);
        });
    }
}
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200., 780.]),
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
