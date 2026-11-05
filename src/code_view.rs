//! Source-mapped, clickable, virtualized read-only code panes.
//! Render only visible rows; clickable spans are mapped to exact AST token occurrences.
use crate::parser::{Anchor, ByteSpan, Projection};
use eframe::egui::{self, Color32, RichText, Sense};

#[derive(Clone, Copy)]
pub enum Side {
    Java,
    Rust,
}

pub struct ViewResult {
    pub clicked: Option<usize>,
    pub offset_y: f32,
    pub hovered: bool,
}

#[derive(Clone, Copy)]
pub struct ViewOptions {
    pub id: &'static str,
    pub side: Side,
    pub selected: Option<usize>,
    pub jump_to: Option<usize>,
    pub scroll_to: Option<f32>,
}

/// Rendering deliberately preserves source line order. Do not use this as a Java code generator.
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
        scroll_to,
    } = options;
    // The parser computed these byte offsets once per document revision.
    // Avoid re-splitting a 40k-line file on every egui repaint.
    let line_starts = match side {
        Side::Java => &projection.java_line_starts,
        Side::Rust => &projection.rust_line_starts,
    };
    let lines = line_starts.len();
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 6.0;
    let jump_line = jump_to
        .and_then(|idx| projection.anchors.get(idx))
        .map(|a| match side {
            Side::Java => a.java_at.line,
            Side::Rust => a.rust_at.line,
        });
    // Stable scrollbar presence prevents alternating layout/repaint loops on
    // large source views: egui would otherwise flip show_scroll every frame.
    let mut scroll = egui::ScrollArea::both()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .id_salt(id)
        .auto_shrink([false, false]);
    if let Some(line) = jump_line {
        scroll = scroll.vertical_scroll_offset(
            line.saturating_sub(4) as f32 * (row_height + ui.spacing().item_spacing.y),
        );
    } else if let Some(y) = scroll_to {
        scroll = scroll.vertical_scroll_offset(y);
    }
    let mut clicked = None;
    let output = scroll.show_rows(ui, row_height, lines, |ui, visible| {
        for line_number in visible {
            let begin = line_starts[line_number];
            let end = line_starts
                .get(line_number + 1)
                .map(|next| next.saturating_sub(1))
                .unwrap_or(text.len());
            ui.horizontal(|row| {
                row.spacing_mut().item_spacing.x = 0.0;
                row.set_min_height(row_height);
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
            });
        }
    });
    ViewResult {
        clicked,
        offset_y: output.state.offset.y,
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
