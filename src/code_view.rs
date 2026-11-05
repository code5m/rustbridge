//! Source-mapped, clickable, virtualized read-only code panes.
//! Render only visible rows; clickable spans are mapped to exact AST token occurrences.
use crate::parser::{Anchor, ByteSpan, FoldRegion, Projection};
use eframe::egui::{self, Color32, RichText, Sense};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Copy)]
pub enum Side {
    Java,
    Rust,
}

pub struct ViewResult {
    pub clicked: Option<usize>,
    pub offset_y: f32,
    /// Corresponding original physical line, even if this pane is folded.
    pub top_source_line: usize,
    /// Used by linked scrolling to ignore layout changes caused by folds.
    pub fold_changed: bool,
    pub hovered: bool,
}

#[derive(Clone, Copy)]
pub struct ViewOptions {
    pub id: &'static str,
    pub side: Side,
    pub selected: Option<usize>,
    pub jump_to: Option<usize>,
    pub scroll_to_line: Option<usize>,
    /// Stable open document identity: each file and each pane owns fold state.
    pub document_id: u64,
}

/// Converts a scroll offset into an egui show_rows index. Its row height
/// includes item spacing; using text height alone causes growing drift.
fn source_row_for_offset(offset_y: f32, row_height: f32, spacing_y: f32) -> usize {
    let pitch = (row_height + spacing_y).max(1.0);
    (offset_y / pitch).floor().max(0.0) as usize
}

#[derive(Clone, Default)]
struct FoldState {
    source_hash: u64,
    collapsed: HashSet<usize>,
    /// None means every physical line is visible (zero per-frame row-map cost).
    visible_rows: Option<Vec<usize>>,
}

impl FoldState {
    fn update_rows(&mut self, projection: &Projection, total_lines: usize) {
        self.visible_rows = if self.collapsed.is_empty() {
            None
        } else {
            Some(visible_physical_lines(
                total_lines,
                &projection.folds,
                &projection.fold_starts,
                &self.collapsed,
            ))
        };
    }

    fn reveal(&mut self, projection: &Projection, line: usize, total_lines: usize) -> bool {
        let old_size = self.collapsed.len();
        self.collapsed.retain(|index| {
            projection
                .folds
                .get(*index)
                .is_none_or(|fold| !(fold.start_line < line && line <= fold.end_line))
        });
        if self.collapsed.len() != old_size {
            self.update_rows(projection, total_lines);
            true
        } else {
            false
        }
    }

    fn row_for_line(&self, line: usize) -> usize {
        self.visible_rows.as_ref().map_or(line, |rows| {
            rows.binary_search(&line)
                .unwrap_or_else(|index| index.saturating_sub(1))
        })
    }
}

/// Pure fold math used by both source panes; never drops/changes source bytes.
fn visible_physical_lines(
    total_lines: usize,
    folds: &[FoldRegion],
    fold_starts: &[Option<usize>],
    collapsed: &HashSet<usize>,
) -> Vec<usize> {
    let mut rows = Vec::with_capacity(total_lines);
    let mut line = 0;
    while line < total_lines {
        rows.push(line);
        if let Some(Some(index)) = fold_starts.get(line)
            && collapsed.contains(index)
            && let Some(fold) = folds.get(*index)
        {
            line = fold.end_line.saturating_add(1).max(line + 1);
        } else {
            line += 1;
        }
    }
    rows
}

/// Rendering deliberately preserves source order and original line numbers.
/// Hidden rows are a view-only AST fold, not deletions or a rewritten source.
pub fn show(
    ui: &mut egui::Ui,
    text: &str,
    projection: &Projection,
    options: ViewOptions,
) -> ViewResult {
    let ViewOptions {
        id,
        side,
        selected,
        jump_to,
        scroll_to_line,
        document_id,
    } = options;
    // The parser computed these byte offsets once per document revision.
    // Avoid re-splitting a 40k-line file on every egui repaint.
    let line_starts = match side {
        Side::Java => &projection.java_line_starts,
        Side::Rust => &projection.rust_line_starts,
    };
    let lines = line_starts.len();
    // Share folding per document and source side across read-only and Compare.
    // Each Rust/Java pane still has its own independent fold selections.
    let state_id = egui::Id::new(("rustbridge_fold", document_id, matches!(side, Side::Java)));
    let mut state = ui
        .ctx()
        .data(|data| data.get_temp::<Arc<FoldState>>(state_id))
        .unwrap_or_default();
    let mut state_changed = false;
    if state.source_hash != projection.source_hash {
        state = Arc::new(FoldState {
            source_hash: projection.source_hash,
            ..FoldState::default()
        });
        state_changed = true;
    }
    let mut command = None;
    ui.horizontal(|ui| {
        ui.small(format!(
            "可折叠结构 {}",
            projection.fold_starts.iter().flatten().count()
        ));
        if ui.small_button("全部折叠").clicked() {
            command = Some(true);
        }
        if ui.small_button("全部展开").clicked() {
            command = Some(false);
        }
    });
    if let Some(collapse) = command {
        let next = Arc::make_mut(&mut state);
        next.collapsed.clear();
        if collapse {
            for &index in projection.fold_starts.iter().flatten() {
                next.collapsed.insert(index);
            }
        }
        next.update_rows(projection, lines);
        state_changed = true;
    }
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 6.0;
    let jump_line = jump_to
        .and_then(|idx| projection.anchors.get(idx))
        .map(|a| match side {
            Side::Java => a.java_at.line,
            Side::Rust => a.rust_at.line,
        });
    if let Some(line) = jump_line
        && Arc::make_mut(&mut state).reveal(projection, line, lines)
    {
        state_changed = true;
    }
    // Stable scrollbar presence prevents alternating layout/repaint loops on
    // large source views: egui would otherwise flip show_scroll every frame.
    let mut scroll = egui::ScrollArea::both()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .id_salt(id)
        .auto_shrink([false, false]);
    if let Some(line) = jump_line {
        scroll = scroll.vertical_scroll_offset(
            state.row_for_line(line).saturating_sub(4) as f32
                * (row_height + ui.spacing().item_spacing.y),
        );
    } else if let Some(line) = scroll_to_line {
        scroll = scroll.vertical_scroll_offset(
            state.row_for_line(line) as f32 * (row_height + ui.spacing().item_spacing.y),
        );
    }
    let visible_count = state.visible_rows.as_ref().map_or(lines, Vec::len);
    let mut clicked = None;
    let mut toggle_fold = None;
    let output = scroll.show_rows(ui, row_height, visible_count, |ui, visible| {
        for physical_row in visible {
            let line_number = state.visible_rows.as_ref().map_or(physical_row, |rows| {
                rows.get(physical_row).copied().unwrap_or(physical_row)
            });
            let begin = line_starts[line_number];
            let end = line_starts
                .get(line_number + 1)
                .map(|next| next.saturating_sub(1))
                .unwrap_or(text.len());
            ui.horizontal(|row| {
                row.spacing_mut().item_spacing.x = 0.0;
                row.set_min_height(row_height);
                let fold_index = projection.fold_starts.get(line_number).copied().flatten();
                if let Some(index) = fold_index {
                    let folded = state.collapsed.contains(&index);
                    // Paint the fold triangle as vector geometry: unlike
                    // Unicode arrow glyphs it never becomes a missing-font box.
                    if row
                        .push_id(projection.folds[index].rust_byte, |row| {
                            let response =
                                row.allocate_response(egui::vec2(15.0, row_height), Sense::click());
                            let center = response.rect.center();
                            let points = if folded {
                                vec![
                                    center + egui::vec2(-3.0, -5.0),
                                    center + egui::vec2(4.0, 0.0),
                                    center + egui::vec2(-3.0, 5.0),
                                ]
                            } else {
                                vec![
                                    center + egui::vec2(-5.0, -3.0),
                                    center + egui::vec2(5.0, -3.0),
                                    center + egui::vec2(0.0, 4.0),
                                ]
                            };
                            row.painter().add(egui::Shape::convex_polygon(
                                points,
                                Color32::from_rgb(122, 177, 224),
                                egui::Stroke::NONE,
                            ));
                            response
                        })
                        .inner
                        .on_hover_text(format!(
                            "{} · {} · {} 行 · 点击{}",
                            projection.folds[index].kind,
                            projection.folds[index].title,
                            projection.folds[index].end_line - line_number,
                            if folded { "展开" } else { "折叠" },
                        ))
                        .clicked()
                    {
                        toggle_fold = Some(index);
                    }
                } else {
                    row.add(egui::Label::new("  ").sense(Sense::hover()));
                }
                let line_label = egui::Label::new(
                    RichText::new(format!("{:>5} │ ", line_number + 1))
                        .monospace()
                        .color(Color32::GRAY),
                )
                .sense(Sense::click());
                let line_response = row.add(line_label);
                if line_response.clicked() {
                    clicked = projection
                        .anchors_by_line
                        .get(line_number)
                        .and_then(|line| line.first().copied());
                }
                let mut cursor = begin;
                for &index in projection
                    .anchors_by_line
                    .get(line_number)
                    .into_iter()
                    .flatten()
                {
                    let anchor = &projection.anchors[index];
                    let token = span_for(anchor, side);
                    if token.start < cursor || token.end > end || token.end <= begin {
                        continue;
                    }
                    if token.start > cursor {
                        append_plain(row, &text[cursor..token.start], side);
                    }
                    let marked = selected == Some(index);
                    let styled = RichText::new(&text[token.start..token.end])
                        .monospace()
                        .color(if marked {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(126, 188, 247)
                        })
                        .background_color(if marked {
                            Color32::from_rgb(53, 103, 155)
                        } else {
                            Color32::TRANSPARENT
                        });
                    let response = row
                        .add(
                            egui::Label::new(styled)
                                .wrap_mode(egui::TextWrapMode::Extend)
                                .sense(Sense::click()),
                        )
                        .on_hover_text(format!("{}\n{}", anchor.topic, anchor.explanation));
                    if response.clicked() {
                        clicked = match side {
                            Side::Java => projection.java_anchor(token.start),
                            Side::Rust => projection.rust_anchor(token.start),
                        };
                    }
                    cursor = token.end;
                }
                if cursor < end {
                    append_plain(row, &text[cursor..end], side);
                } else if begin == end {
                    append_plain(row, " ", side);
                }
                if let Some(index) = fold_index
                    && state.collapsed.contains(&index)
                {
                    row.colored_label(
                        Color32::from_rgb(145, 153, 166),
                        format!("  … {} 行", projection.folds[index].end_line - line_number),
                    );
                }
            });
        }
    });
    if let Some(index) = toggle_fold {
        let next = Arc::make_mut(&mut state);
        if !next.collapsed.insert(index) {
            next.collapsed.remove(&index);
        }
        next.update_rows(projection, lines);
        state_changed = true;
    }
    if state_changed {
        ui.ctx()
            .data_mut(|data| data.insert_temp(state_id, Arc::clone(&state)));
        ui.ctx().request_repaint();
    }
    ViewResult {
        clicked,
        offset_y: output.state.offset.y,
        fold_changed: state_changed,
        top_source_line: {
            // egui::ScrollArea::show_rows adds vertical item spacing to the
            // supplied row height. Without this spacing, linked scrolling
            // drifts increasingly far from the matching original source line.
            let row = source_row_for_offset(
                output.state.offset.y,
                row_height,
                ui.spacing().item_spacing.y,
            );
            state.visible_rows.as_ref().map_or(row, |rows| {
                rows.get(row)
                    .copied()
                    .unwrap_or_else(|| *rows.last().unwrap_or(&0))
            })
        },
        hovered: output
            .inner_rect
            .contains(ui.input(|i| i.pointer.hover_pos().unwrap_or_default())),
    }
}

fn span_for(anchor: &Anchor, side: Side) -> ByteSpan {
    match side {
        Side::Java => anchor.java,
        Side::Rust => anchor.rust,
    }
}

fn append_plain(ui: &mut egui::Ui, source: &str, side: Side) {
    let dark = ui.visuals().dark_mode;
    let keyword_color = if dark {
        Color32::from_rgb(198, 120, 221)
    } else {
        Color32::from_rgb(128, 56, 160)
    };
    let string_color = if dark {
        Color32::from_rgb(152, 195, 121)
    } else {
        Color32::from_rgb(44, 115, 70)
    };
    let comment_color = if dark {
        Color32::from_rgb(115, 145, 115)
    } else {
        Color32::from_rgb(90, 120, 90)
    };
    let number_color = if dark {
        Color32::from_rgb(209, 154, 102)
    } else {
        Color32::from_rgb(160, 92, 30)
    };
    let type_color = if dark {
        Color32::from_rgb(86, 182, 194)
    } else {
        Color32::from_rgb(0, 115, 130)
    };

    let bytes = source.as_bytes();
    let mut cursor = 0;
    while cursor < source.len() {
        let start = cursor;
        let ch = source[cursor..]
            .chars()
            .next()
            .expect("cursor is a char boundary");
        let mut color = None;

        if ch.is_whitespace() {
            cursor += ch.len_utf8();
            while cursor < source.len() {
                let next = source[cursor..].chars().next().unwrap();
                if !next.is_whitespace() {
                    break;
                }
                cursor += next.len_utf8();
            }
        } else if source[cursor..].starts_with("//") {
            cursor = source.len();
            color = Some(comment_color);
        } else if source[cursor..].starts_with("/*") {
            cursor += 2;
            if let Some(end) = source[cursor..].find("*/") {
                cursor += end + 2;
            } else {
                cursor = source.len();
            }
            color = Some(comment_color);
        } else if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut probe = cursor + ch.len_utf8();
            let mut escaped = false;
            let mut closed = false;
            while probe < source.len() {
                let next = source[probe..].chars().next().unwrap();
                probe += next.len_utf8();
                if escaped {
                    escaped = false;
                    continue;
                }
                if next == '\\' {
                    escaped = true;
                    continue;
                }
                if next == quote {
                    closed = true;
                    break;
                }
                if next == '\n' {
                    break;
                }
            }
            if quote == '"' || closed {
                cursor = probe;
                color = Some(string_color);
            } else {
                cursor += ch.len_utf8();
            }
        } else if ch.is_alphabetic() || ch == '_' {
            cursor += ch.len_utf8();
            while cursor < source.len() {
                let next = source[cursor..].chars().next().unwrap();
                if !(next.is_alphanumeric() || next == '_') {
                    break;
                }
                cursor += next.len_utf8();
            }
            let word = &source[start..cursor];
            if is_keyword(word, side) {
                color = Some(keyword_color);
            } else if word.chars().next().is_some_and(char::is_uppercase) {
                color = Some(type_color);
            }
        } else if ch.is_ascii_digit() {
            cursor += ch.len_utf8();
            while cursor < source.len() {
                let byte = bytes[cursor];
                if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' {
                    cursor += 1;
                } else {
                    break;
                }
            }
            color = Some(number_color);
        } else {
            cursor += ch.len_utf8();
        }

        let text = RichText::new(&source[start..cursor]).monospace();
        let text = color.map_or(text.clone(), |color| text.color(color));
        ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend));
    }
}

fn is_keyword(word: &str, side: Side) -> bool {
    match side {
        Side::Java => matches!(
            word,
            "abstract"
                | "assert"
                | "boolean"
                | "break"
                | "byte"
                | "case"
                | "catch"
                | "char"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "do"
                | "double"
                | "else"
                | "enum"
                | "extends"
                | "final"
                | "finally"
                | "float"
                | "for"
                | "if"
                | "implements"
                | "import"
                | "instanceof"
                | "int"
                | "interface"
                | "long"
                | "native"
                | "new"
                | "package"
                | "private"
                | "protected"
                | "public"
                | "return"
                | "short"
                | "static"
                | "strictfp"
                | "super"
                | "switch"
                | "synchronized"
                | "this"
                | "throw"
                | "throws"
                | "transient"
                | "try"
                | "void"
                | "volatile"
                | "while"
                | "true"
                | "false"
                | "null"
        ),
        Side::Rust => matches!(
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
        ),
    }
}

#[cfg(test)]
mod fold_tests {
    use super::*;
    use crate::parser;

    #[test]
    fn nested_ast_folds_hide_only_virtual_lines_and_keep_original_numbers() {
        let source = "fn outer() {\n  let one = 1;\n  if one > 0 {\n    println!(\"ok\");\n  }\n}\nfn next() {\n  println!(\"next\");\n}\n";
        let projection = parser::project(source);
        let outer = projection.fold_starts[0].expect("outer function must be foldable");
        let inner = projection.fold_starts[2].expect("if expression must be foldable");
        let mut state = FoldState {
            source_hash: projection.source_hash,
            ..FoldState::default()
        };
        state.collapsed.insert(inner);
        state.update_rows(&projection, projection.rust_line_starts.len());
        assert!(state.visible_rows.as_ref().unwrap().contains(&2));
        assert!(!state.visible_rows.as_ref().unwrap().contains(&3));
        assert!(state.visible_rows.as_ref().unwrap().contains(&6));
        state.collapsed.insert(outer);
        state.update_rows(&projection, projection.rust_line_starts.len());
        let rows = state.visible_rows.as_ref().unwrap();
        assert!(rows.contains(&0));
        assert!(!rows.contains(&2));
        assert!(rows.contains(&6), "other functions must remain visible");
        assert!(state.reveal(&projection, 3, projection.rust_line_starts.len()));
        assert!(state.visible_rows.as_ref().is_none());
    }

    #[test]
    fn linked_scroll_uses_virtual_row_pitch_including_egui_spacing() {
        // ScrollArea::show_rows uses row_height + item_spacing.y, not the
        // text row height alone. At row 1000 even 4px spacing accumulates.
        let physical_row_height: f32 = 21.0;
        let spacing: f32 = 4.0;
        let offset: f32 = 1000.0 * (physical_row_height + spacing);
        let linked_row = source_row_for_offset(offset, physical_row_height, spacing);
        assert_eq!(linked_row, 1000);
        assert_ne!((offset / physical_row_height).floor() as usize, 1000);
    }

    #[test]
    fn folding_a_large_source_uses_physical_line_indices_without_editing_text() {
        let source = format!("fn long() {{\n{} }}\n", "    let a = 1;\n".repeat(2000));
        let projection = parser::project(&source);
        let index = projection.fold_starts[0].unwrap();
        let collapsed: HashSet<usize> = [index].into_iter().collect();
        let visible = visible_physical_lines(
            projection.rust_line_starts.len(),
            &projection.folds,
            &projection.fold_starts,
            &collapsed,
        );
        assert!(
            visible.len() < 5,
            "large function collapses to its declaration"
        );
        assert_eq!(source.lines().count(), 2002, "folding never mutates source");
    }
}
