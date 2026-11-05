//! Editable Rust source panes; large files edit only a bounded page at once.
use super::editor_core::{
    EDITOR_PAGE_LINES, LARGE_EDITOR_BYTES, append_rust_syntax, editor_page_bounds,
    line_number_gutter,
};
use crate::{App, editor_eol, line_index, parser};
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
        if self.file.is_none() {
            ui.colored_label(
                Color32::YELLOW,
                "内置示例为只读学习材料。打开真实 .rs 文件后才能编辑和保存。",
            );
            ui.small("示例不会被当作项目文件，也不会出现看似可用但无法保存的编辑状态。");
            egui::ScrollArea::both()
                .id_salt("readonly_builtin_example")
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&self.rust).monospace())
                            .selectable(true)
                            .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
            return;
        }
        ui.horizontal(|ui| {
            ui.label("原始 Rust 源码 · 可编辑");
            if ui.button("保存 · Ctrl+S").clicked() {
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
            let updated = editor_eol::apply_display_edit(&self.rust, &original_view, &editor_text);
            let (physical_range, insert) = line_index::minimal_diff(&self.rust, &updated);
            let range = physical_range.clone();
            let inserted = insert.to_owned();
            self.rust = updated;
            self.source_edited(range, &inserted);
            self.dirty = self.rust != self.saved_source;
            self.mark_dirty_for_recovery();
            self.editor_find = None;
            self.editor_jump = None;
            self.selected = None;
            self.status = "Source edited; updating the reading projection.".into();
        }
        if let Some(cursor) = output.cursor_range {
            let displayed = parser::char_to_byte(&editor_text, cursor.primary.index);
            self.cursor_byte = editor_eol::display_to_original(&self.rust, displayed);
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
            self.editor_page_start = line.saturating_sub(EDITOR_PAGE_LINES / 2);
            self.editor_page_target = line + 1;
            self.editor_scroll_override = None;
        }
        self.editor_page_start = self
            .editor_page_start
            .min(lines.saturating_sub(EDITOR_PAGE_LINES));
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(Color32::YELLOW, "大文件连续浏览 · 只加载少量编辑行");
            ui.label(format!("全文件 {} 行", lines));
            let response = ui.add(
                egui::Slider::new(&mut self.editor_page_target, 1..=lines.max(1))
                    .text("全局行位置"),
            );
            if response.changed() {
                self.editor_page_start = self
                    .editor_page_target
                    .saturating_sub(1)
                    .min(lines.saturating_sub(EDITOR_PAGE_LINES));
                self.editor_scroll_override = Some(0.);
            }
        });
        let first = self.editor_page_start;
        let last = (first + EDITOR_PAGE_LINES).min(lines);
        ui.small(format!(
            "当前编辑窗口第 {}–{} 行 / 总计 {} 行；滚轮、Ctrl+PageDown、上方滑条可连续导航整份文件。",
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
        let mut scroll_area = egui::ScrollArea::both()
            .id_salt(("rust_virtual_scroll", &self.file, first))
            .auto_shrink([false, false]);
        if let Some(offset) = self.editor_scroll_override.take() {
            scroll_area = scroll_area.vertical_scroll_offset(offset);
        }
        let scroll_result = scroll_area.show(ui, |ui| {
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
        let wheel = ui.input(|input| input.raw_scroll_delta.y);
        let hovered = ui.rect_contains_pointer(scroll_result.inner_rect);
        let max_y = (scroll_result.content_size.y - scroll_result.inner_rect.height()).max(0.);
        let shift_lines = EDITOR_PAGE_LINES / 2;
        if hovered
            && wheel < -0.1
            && scroll_result.state.offset.y >= max_y - 8.
            && first + EDITOR_PAGE_LINES < lines
        {
            let next = (first + shift_lines).min(lines.saturating_sub(EDITOR_PAGE_LINES));
            let moved = next - first;
            self.editor_page_start = next;
            self.editor_page_target = next + 1;
            self.editor_scroll_override =
                Some((scroll_result.state.offset.y - moved as f32 * 18.).max(0.));
            ui.ctx().request_repaint();
        } else if hovered && wheel > 0.1 && scroll_result.state.offset.y <= 8. && first > 0 {
            let next = first.saturating_sub(shift_lines);
            let moved = first - next;
            self.editor_page_start = next;
            self.editor_page_target = next + 1;
            self.editor_scroll_override = Some(scroll_result.state.offset.y + moved as f32 * 18.);
            ui.ctx().request_repaint();
        }
        if let Some(mut output) = output {
            if output.response.changed() {
                let original_view = editor_eol::for_display(&physical_page);
                let patch = editor_eol::apply_display_edit(&physical_page, &original_view, &page);
                self.rust.replace_range(range.clone(), &patch);
                self.source_edited(range.clone(), &patch);
                if let Some(cursor) = output.cursor_range {
                    let visible = parser::char_to_byte(&page, cursor.primary.index);
                    self.cursor_byte =
                        range.start + editor_eol::display_to_original(&patch, visible);
                }
                self.dirty = self.rust != self.saved_source;
                self.mark_dirty_for_recovery();
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
            if !output.response.changed()
                && let Some(cursor) = output.cursor_range
            {
                let displayed = parser::char_to_byte(&page, cursor.primary.index);
                self.cursor_byte =
                    range.start + editor_eol::display_to_original(&physical_page, displayed);
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
