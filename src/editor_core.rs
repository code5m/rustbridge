//! Lightweight UTF-8-safe lexical coloring and large editor page boundaries.
use crate::parser;
use eframe::egui::{self, Color32};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SyntaxToken {
    Comment,
    String,
    Word,
    Plain,
}

fn quoted_end(rest: &str, quote: char) -> usize {
    let mut escaped = false;
    for (offset, character) in rest[1..].char_indices() {
        if character == '\n' || character == '\r' {
            break;
        }
        if character == quote && !escaped {
            return offset + 2;
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    1
}

fn raw_string_end(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let prefix = if rest.starts_with("br") {
        2
    } else if rest.starts_with('r') {
        1
    } else {
        return None;
    };
    let hashes = bytes[prefix..]
        .iter()
        .take_while(|byte| **byte == b'#')
        .count();
    if bytes.get(prefix + hashes) != Some(&b'"') {
        return None;
    }
    let terminator = format!("\"{}", "#".repeat(hashes));
    rest[prefix + hashes + 1..]
        .find(&terminator)
        .map(|offset| prefix + hashes + 1 + offset + terminator.len())
        .or(Some(rest.len()))
}

fn block_comment_end(rest: &str) -> usize {
    // ASCII comment delimiters can be scanned bytewise without slicing across
    // UTF-8 code points. Unicode bytes never equal '/', '*'.
    let bytes = rest.as_bytes();
    let mut depth = 1_usize;
    let mut index = 2_usize;
    while index + 1 < bytes.len() {
        match (bytes[index], bytes[index + 1]) {
            (b'/', b'*') => {
                depth += 1;
                index += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                index += 2;
                if depth == 0 {
                    return index;
                }
            }
            _ => index += 1,
        }
    }
    rest.len()
}

fn next_rust_token(rest: &str) -> (usize, SyntaxToken) {
    if rest.starts_with("//") {
        return (rest.find('\n').unwrap_or(rest.len()), SyntaxToken::Comment);
    }
    if rest.starts_with("/*") {
        return (block_comment_end(rest), SyntaxToken::Comment);
    }
    if let Some(end) = raw_string_end(rest) {
        return (end, SyntaxToken::String);
    }
    if rest.starts_with("b\"") {
        return (1 + quoted_end(&rest[1..], '"'), SyntaxToken::String);
    }
    if rest.starts_with("b'") {
        return (1 + quoted_end(&rest[1..], '\''), SyntaxToken::String);
    }
    if rest.starts_with('"') {
        return (quoted_end(rest, '"'), SyntaxToken::String);
    }
    if rest.starts_with('\'') {
        return (quoted_end(rest, '\''), SyntaxToken::String);
    }
    if rest
        .chars()
        .next()
        .is_some_and(|character| character.is_alphabetic() || character == '_')
    {
        return (
            rest.char_indices()
                .find(|(_, character)| !character.is_alphanumeric() && *character != '_')
                .map_or(rest.len(), |(end, _)| end),
            SyntaxToken::Word,
        );
    }
    (rest.chars().next().unwrap().len_utf8(), SyntaxToken::Plain)
}

pub(crate) fn append_rust_syntax(
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
        let (length, kind) = next_rust_token(rest);
        let end = index + length;
        let token = &text[index..end];
        let color = if kind == SyntaxToken::Comment {
            Color32::from_rgb(112, 142, 112)
        } else if kind == SyntaxToken::String {
            Color32::from_rgb(202, 162, 98)
        } else if kind == SyntaxToken::Word && keyword(token) {
            Color32::from_rgb(126, 188, 247)
        } else {
            base
        };
        job.append(token, 0.0, format(color, index, end));
        index = end;
    }
}

pub(crate) fn line_number_gutter(text: &str) -> String {
    let line_count = text.bytes().filter(|byte| *byte == b'\n').count() + 1;
    (1..=line_count)
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) const LARGE_EDITOR_BYTES: usize = 512 * 1024;
pub(crate) const EDITOR_PAGE_LINES: usize = 240;

/// Page boundaries are byte offsets derived from actual UTF-8 newline locations.
pub(crate) fn editor_page_bounds(
    lines: &[usize],
    first: usize,
    count: usize,
    bytes: usize,
) -> std::ops::Range<usize> {
    let start = *lines.get(first).unwrap_or(&bytes);
    let end = lines
        .get(first.saturating_add(count))
        .copied()
        .unwrap_or(bytes);
    start..end
}

#[cfg(test)]
mod utf8_regressions {
    use super::*;
    #[test]
    fn chinese_and_emoji_block_comments_never_split_code_points() {
        for comment in [
            "/*中文*/",
            "/*🦀中文*/",
            "/* 中文 /* 内层 🦀 */ 外层 */",
            "/*没结束🦀",
            "/*a*/ fn main() {}",
        ] {
            let mut job = egui::text::LayoutJob::default();
            append_rust_syntax(
                &mut job,
                comment,
                &egui::FontId::monospace(12.),
                Color32::WHITE,
                None,
            );
            let assembled: String = job
                .sections
                .iter()
                .map(|s| &comment[s.byte_range.clone()])
                .collect();
            assert_eq!(assembled, comment);
            assert!(comment.is_char_boundary(next_rust_token(comment).0));
        }
    }
}
