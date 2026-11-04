//! Editable Rust source panes; large files edit only a bounded page at once.
use super::editor_core::{
    EDITOR_PAGE_LINES, LARGE_EDITOR_BYTES, append_rust_syntax, editor_page_bounds,
    line_number_gutter,
};
use crate::{App, editor_eol, parser};
use eframe::egui::{self, Color32};

impl App {
    pub(super) fn find_next_in_editor(&mut self) {
        let query = self.editor_query.trim();
        if query.is_empty() {
            self.editor_find = None;
            self.editor_jump = None;
            return;
        }
        let start = self.editor_find.map_or(0, |span| span.end);
        // find() returns an offset relative to the substring; convert it to
        // an absolute byte index before constructing the editor span.
        let offset = self.rust[start..]
            .find(query)
            .map(|relative| start + relative)
            .or_else(|| self.rust[..start].find(query));
        if let Some(offset) = offset {
            let span = parser::ByteSpan {
                start: offset,
                end: offset + query.len(),
            };
            self.editor_find = Some(span);
            self.editor_jump = Some(span);
            self.status = format!("Found Rust text at byte {}.", span.start);
        } else {
            self.editor_find = None;
            self.status = "Text not found in the original Rust source.".into();
        }
    }

    pub(super) fn editor(&mut self, ui: &mut egui::Ui) {
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
        if self.rust.len() >= LARGE_EDITOR_BYTES {
            self.paged_editor(ui);
            return;
        }
        // egui's TextEdit expects LF. Edit a normalized view, then apply only
        // the edited bytes back to the real source to retain CRLF/mixed EOL.
        let mut editor_text = editor_eol::for_display(&self.rust);
        let active = self
            .selected
            .and_then(|idx| self.projection.anchors.get(idx))
            .map(|anchor| anchor.rust)
            .or(self.editor_find)
            .map(|span| parser::ByteSpan {
                start: editor_eol::original_to_display(&self.rust, span.start),
                end: editor_eol::original_to_display(&self.rust, span.end),
            });
        let jump = self
            .jump_to
            .take()
            .and_then(|idx| self.projection.anchors.get(idx))
            .map(|anchor| anchor.rust);
        let cursor_target = jump
            .or(self.editor_jump.take())
            .map(|span| parser::ByteSpan {
                start: editor_eol::original_to_display(&self.rust, span.start),
                end: editor_eol::original_to_display(&self.rust, span.end),
            });

        // This is deliberately lexical highlighting only. Tree-sitter remains the source of links.
        let mut layouter = |ui: &egui::Ui, content: &dyn egui::TextBuffer, _width: f32| {
            let text = content.as_str();
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;
            let font = egui::TextStyle::Monospace.resolve(ui.style());
            let active = active.filter(|span| {
                span.end <= text.len()
                    && text.is_char_boundary(span.start)
                    && text.is_char_boundary(span.end)
            });
            append_rust_syntax(&mut job, text, &font, ui.visuals().text_color(), active);
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };
        if self.editor_gutter_revision != self.revision {
            self.editor_gutter = line_number_gutter(&self.rust);
            self.editor_gutter_revision = self.revision;
        }
        let line_numbers = &self.editor_gutter;
        let mut output = None;
        egui::ScrollArea::both()
            .id_salt(("rust_source_editor_scroll", &self.file))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(line_numbers).monospace().weak())
                            .selectable(false),
                    );
                    output = Some(
                        egui::TextEdit::multiline(&mut editor_text)
                            .id_salt(("rust_source_editor", &self.file))
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .layouter(&mut layouter)
                            .show(ui),
                    );
                });
            });
        let mut output = output.expect("Rust source editor always renders");
        if output.response.changed() {
            let original_view = editor_eol::for_display(&self.rust);
            self.rust = editor_eol::apply_display_edit(&self.rust, &original_view, &editor_text);
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
            let displayed = parser::char_to_byte(&editor_text, cursor.primary.index);
            let byte = editor_eol::display_to_original(&self.rust, displayed);
            self.selected = self.projection.rust_anchor(byte).or_else(|| {
                byte.checked_sub(1)
                    .and_then(|before| self.projection.rust_anchor(before))
            });
        }
        if let Some(span) = cursor_target
            && span.end <= editor_text.len()
        {
            use egui::text::{CCursor, CCursorRange};
            let start = editor_text[..span.start].chars().count();
            let end = editor_text[..span.end].chars().count();
            output.state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(start),
                CCursor::new(end),
            )));
            output.state.store(ui.ctx(), output.response.id);
            ui.scroll_to_cursor(Some(egui::Align::Center));
            ui.ctx().request_repaint();
        }
    }

    fn paged_editor(&mut self, ui: &mut egui::Ui) {
        let lines = self.editor_line_starts.len();
        let jump = self
            .jump_to
            .take()
            .and_then(|idx| self.projection.anchors.get(idx))
            .map(|anchor| anchor.rust);
        let target = jump.or(self.editor_jump.take());
        if let Some(span) = target {
            let line = self
                .editor_line_starts
                .partition_point(|&begin| begin <= span.start)
                .saturating_sub(1);
            self.editor_page_start = (line / EDITOR_PAGE_LINES) * EDITOR_PAGE_LINES;
            self.editor_page_target = line + 1;
        }
        self.editor_page_start = self.editor_page_start.min(lines.saturating_sub(1));
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                Color32::YELLOW,
                format!(
                    "大文件分页编辑 · 只渲染当前 {} 行，其余原始文件保持不变",
                    EDITOR_PAGE_LINES
                ),
            );
            if ui
                .add_enabled(self.editor_page_start > 0, egui::Button::new("上一页"))
                .clicked()
            {
                self.editor_page_start = self.editor_page_start.saturating_sub(EDITOR_PAGE_LINES);
            }
            if ui
                .add_enabled(
                    self.editor_page_start + EDITOR_PAGE_LINES < lines,
                    egui::Button::new("下一页"),
                )
                .clicked()
            {
                self.editor_page_start += EDITOR_PAGE_LINES;
            }
            ui.label("行:");
            ui.add(egui::DragValue::new(&mut self.editor_page_target).range(1..=lines.max(1)));
            if ui.button("跳转").clicked() {
                self.editor_page_start = ((self.editor_page_target.saturating_sub(1))
                    / EDITOR_PAGE_LINES)
                    * EDITOR_PAGE_LINES;
            }
        });
        let first = self.editor_page_start;
        let last = (first + EDITOR_PAGE_LINES).min(lines);
        ui.small(format!(
            "当前显示第 {}–{} 行 / 总计 {} 行；跨页修改仍保存到同一个真实 .rs 文件。",
            first + 1,
            last,
            lines
        ));
        let range = editor_page_bounds(
            &self.editor_line_starts,
            first,
            EDITOR_PAGE_LINES,
            self.rust.len(),
        );
        let physical_page = self.rust[range.clone()].to_owned();
        let mut page = editor_eol::for_display(&physical_page);
        let line_numbers = (first + 1..=last)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        let active = self
            .selected
            .and_then(|idx| self.projection.anchors.get(idx))
            .map(|anchor| anchor.rust)
            .or(self.editor_find)
            .and_then(|span| {
                if span.start >= range.start && span.end <= range.end {
                    Some(parser::ByteSpan {
                        start: editor_eol::original_to_display(
                            &physical_page,
                            span.start - range.start,
                        ),
                        end: editor_eol::original_to_display(
                            &physical_page,
                            span.end - range.start,
                        ),
                    })
                } else {
                    None
                }
            });
        let mut layouter = |ui: &egui::Ui, content: &dyn egui::TextBuffer, _width: f32| {
            let text = content.as_str();
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;
            let font = egui::TextStyle::Monospace.resolve(ui.style());
            append_rust_syntax(&mut job, text, &font, ui.visuals().text_color(), active);
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };
        let mut output = None;
        egui::ScrollArea::both()
            .id_salt(("rust_page_scroll", &self.file, first))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(line_numbers).monospace().weak())
                            .selectable(false),
                    );
                    output = Some(
                        egui::TextEdit::multiline(&mut page)
                            .id_salt(("rust_page_editor", &self.file, first))
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .layouter(&mut layouter)
                            .show(ui),
                    );
                });
            });
        if let Some(mut output) = output {
            if output.response.changed() {
                let original_view = editor_eol::for_display(&physical_page);
                let patch = editor_eol::apply_display_edit(&physical_page, &original_view, &page);
                self.rust.replace_range(range.clone(), &patch);
                self.update_projection();
                self.dirty = self.rust != self.saved_source;
                self.editor_find = None;
                self.selected = None;
                self.status = "大文件当前页已编辑；后台增量重建 AST 投影。".into();
            } else if !self.projection_pending
                && output.response.clicked()
                && let Some(cursor) = output.cursor_range
            {
                let displayed = parser::char_to_byte(&page, cursor.primary.index);
                let byte = range.start + editor_eol::display_to_original(&physical_page, displayed);
                self.selected = self.projection.rust_anchor(byte).or_else(|| {
                    byte.checked_sub(1)
                        .and_then(|b| self.projection.rust_anchor(b))
                });
            }
            if let Some(span) = target.filter(|s| s.start >= range.start && s.end <= range.end) {
                use egui::text::{CCursor, CCursorRange};
                let visible_start =
                    editor_eol::original_to_display(&physical_page, span.start - range.start);
                let visible_end =
                    editor_eol::original_to_display(&physical_page, span.end - range.start);
                let start = page[..visible_start].chars().count();
                let end = page[..visible_end].chars().count();
                output.state.cursor.set_char_range(Some(CCursorRange::two(
                    CCursor::new(start),
                    CCursor::new(end),
                )));
                output.state.store(ui.ctx(), output.response.id);
                ui.scroll_to_cursor(Some(egui::Align::Center));
            }
        }
    }
}
