//! Reversible newline adapter for an egui TextEdit.
//! Rust source remains byte-for-byte unchanged outside the user's edit.
//! The editing widget displays CRLF as LF and translates edited spans back
//! into the original file's physical newline convention.

/// Translate CRLF pairs for display. Original code stays in the Document.
pub fn for_display(original: &str) -> String {
    original.replace("\r\n", "\n").replace('\r', "\n")
}

/// Map a UTF-8 byte boundary in original Rust into the normalized widget.
/// A removed carriage return maps to the position of its following newline.
pub fn original_to_display(original: &str, physical: usize) -> usize {
    let position = physical.min(original.len());
    let removed = original[..position].match_indices("\r\n").count();
    // If position is inside the CRLF pair at '\n', account for the CR.
    let interrupted = position > 0
        && original.as_bytes().get(position.wrapping_sub(1)) == Some(&b'\r')
        && original.as_bytes().get(position) == Some(&b'\n');
    position.saturating_sub(removed + usize::from(interrupted))
}

/// Map a widget byte offset to an insertion point in original source.
/// At a normalized '\n' boundary this points to CR (not inside CRLF).
pub fn display_to_original(original: &str, display: usize) -> usize {
    let mut removed = 0;
    for (index, _) in original.match_indices("\r\n") {
        let logical_newline = index - removed;
        if logical_newline < display {
            removed += 1;
        } else {
            break;
        }
    }
    (display + removed).min(original.len())
}

#[derive(Clone, Copy)]
enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

fn nearest_line_ending(original: &str, physical: usize) -> LineEnding {
    fn ending_at(text: &str, index: usize) -> Option<LineEnding> {
        match text.as_bytes().get(index) {
            Some(b'\r') if text.as_bytes().get(index + 1) == Some(&b'\n') => Some(LineEnding::CrLf),
            Some(b'\r') => Some(LineEnding::Cr),
            Some(b'\n') => Some(LineEnding::Lf),
            _ => None,
        }
    }
    if let Some(offset) = original[physical..].find(['\n', '\r']) {
        return ending_at(original, physical + offset).unwrap_or(LineEnding::Lf);
    }
    if let Some(offset) = original[..physical].rfind(['\n', '\r']) {
        // Prefer the CR of a CRLF pair (otherwise a preceding LF is ambiguous).
        if original.as_bytes()[offset] == b'\n'
            && offset > 0
            && original.as_bytes()[offset - 1] == b'\r'
        {
            return LineEnding::CrLf;
        }
        return ending_at(original, offset).unwrap_or(LineEnding::Lf);
    }
    LineEnding::Lf
}

/// Patch just the changed interval; untouched mixed CRLF/LF sections stay intact.
/// For *new* line breaks use the nearest physical line-ending style.
pub fn apply_display_edit(original: &str, before: &str, edited: &str) -> String {
    if before == edited {
        return original.to_owned();
    }
    debug_assert_eq!(for_display(original), before);

    let mut prefix = before
        .bytes()
        .zip(edited.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(prefix) || !edited.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut old_end = before.len();
    let mut new_end = edited.len();
    while old_end > prefix
        && new_end > prefix
        && before.as_bytes()[old_end - 1] == edited.as_bytes()[new_end - 1]
    {
        old_end -= 1;
        new_end -= 1;
    }
    while !before.is_char_boundary(old_end) || !edited.is_char_boundary(new_end) {
        old_end += 1;
        new_end += 1;
    }

    let physical_start = display_to_original(original, prefix);
    let physical_end = display_to_original(original, old_end);
    let insertion = &edited[prefix..new_end];
    let replacement = match nearest_line_ending(original, physical_start) {
        LineEnding::CrLf => insertion.replace('\n', "\r\n"),
        LineEnding::Cr => insertion.replace('\n', "\r"),
        LineEnding::Lf => insertion.to_owned(),
    };
    let prefix = &original[..physical_start];
    let suffix = &original[physical_end..];
    let mut result = String::with_capacity(prefix.len() + replacement.len() + suffix.len());
    append_physical_atoms(&mut result, prefix);
    append_physical_atoms(&mut result, &replacement);
    append_physical_atoms(&mut result, suffix);
    debug_assert_eq!(
        for_display(&result),
        edited,
        "newline mapping must be reversible"
    );
    result
}

/// Preserve the original physical line endings as individual logical atoms.
/// If an edit joins an original lone CR and an independent LF, encode that LF
/// as CR rather than accidentally forming a single CRLF token. A chain of
/// adjacent line endings is repaired left-to-right without collapsing lines.
fn append_physical_atoms(out: &mut String, physical: &str) {
    let bytes = physical.as_bytes();
    let mut begin = 0;
    let mut index = 0;
    while index < bytes.len() {
        let value = bytes[index];
        if value != b'\n' && value != b'\r' {
            index += 1;
            continue;
        }
        out.push_str(&physical[begin..index]);
        if value == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            out.push_str("\r\n");
            index += 2;
        } else {
            out.push(if value == b'\n' && out.ends_with('\r') {
                '\r'
            } else {
                value as char
            });
            index += 1;
        }
        begin = index;
    }
    out.push_str(&physical[begin..]);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn insertion_before_crlf_preserves_unchanged_line_endings() {
        let original = "struct 中文 {\r\n field: String,\r\n}\r\n";
        let before = for_display(original);
        let edited = before.replace("field: String,", "field: String, // 🦀");
        let saved = apply_display_edit(original, &before, &edited);
        assert_eq!(saved, "struct 中文 {\r\n field: String, // 🦀\r\n}\r\n");
        assert_eq!(saved.matches("\r\n").count(), 3);
        assert_eq!(saved.matches('\n').count(), 3);
    }

    #[test]
    fn deleting_crlf_removes_both_physical_bytes() {
        let original = "a\r\nb\r\n";
        let before = for_display(original);
        let after = before.replace("a\nb", "ab");
        assert_eq!(apply_display_edit(original, &before, &after), "ab\r\n");
    }

    #[test]
    fn inserting_newlines_uses_local_crlf() {
        let original = "a\r\nb\r\n";
        let before = for_display(original);
        let after = before.replace("b", "b\nc");
        assert_eq!(
            apply_display_edit(original, &before, &after),
            "a\r\nb\r\nc\r\n"
        );
    }

    #[test]
    fn mixed_eol_unmodified_lines_are_preserved() {
        let original = "a\r\nb\nc\r\n";
        let before = for_display(original);
        assert_eq!(
            apply_display_edit(original, &before, &before.replace("b", "BB")),
            "a\r\nBB\nc\r\n"
        );
    }

    #[test]
    fn unicode_offsets_are_roundtrip_safe() {
        let original = "中文🦀\r\nlet data = \"Rust\";\r\n";
        let display = for_display(original);
        for position in display
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(display.len()))
        {
            let physical = display_to_original(original, position);
            assert!(original.is_char_boundary(physical));
            assert_eq!(original_to_display(original, physical), position);
        }
    }

    #[test]
    fn lone_cr_boundaries_are_not_eaten() {
        let original = "\r// \r";
        let before = for_display(original);
        assert_eq!(before, "\n// \n");
        let edited = "\n\n// \n";
        let saved = apply_display_edit(original, &before, edited);
        assert_eq!(for_display(&saved), edited);
        assert_eq!(saved, "\r\r// \r");
    }

    #[test]
    fn joining_adjacent_cr_and_lf_never_loses_a_newline() {
        let original = "\raa\r\n\n\r\n";
        let before = for_display(original);
        let edited = "\n\n\n";
        let result = apply_display_edit(original, &before, edited);
        assert_eq!(for_display(&result), edited);

        let original = "\n🦀b\r🦀\n/\n\n baa/x\n";
        let before = for_display(original);
        let edited = format!("{}\n{}", &before[..7], &before[13..]);
        let result = apply_display_edit(original, &before, &edited);
        assert_eq!(for_display(&result), edited);
    }

    #[test]
    fn randomized_mixed_eol_edits_are_reversible() {
        fn next(seed: &mut u64) -> u64 {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            *seed
        }
        let mut seed = 0x4138ae38bdffe21_u64;
        let pieces = ["a", "b", "中文", "🦀", "\r\n", "\n", "\r", "x", "/", " "];
        let inserts = ["", "a", "中", "🦀", "\n", "\na", "abc", "\n中", "a\n"];
        for trial in 0..6000 {
            let mut original = String::new();
            for _ in 0..(next(&mut seed) % 18 + 1) {
                original.push_str(pieces[(next(&mut seed) % pieces.len() as u64) as usize]);
            }
            let before = for_display(&original);
            let bounds: Vec<_> = before
                .char_indices()
                .map(|(i, _)| i)
                .chain([before.len()])
                .collect();
            let a = (next(&mut seed) % bounds.len() as u64) as usize;
            let b = (next(&mut seed) % bounds.len() as u64) as usize;
            let start = bounds[a.min(b)];
            let end = bounds[a.max(b)];
            let insert = inserts[(next(&mut seed) % inserts.len() as u64) as usize];
            let edited = format!("{}{}{}", &before[..start], insert, &before[end..]);
            let actual = apply_display_edit(&original, &before, &edited);
            assert_eq!(
                for_display(&actual),
                edited,
                "trial={trial} original={original:?}"
            );
        }
    }

    #[test]
    fn plain_lf_files_are_unmodified_except_actual_edit() {
        let original = "a\nb\n";
        let before = for_display(original);
        assert_eq!(apply_display_edit(original, &before, "a\nBB\n"), "a\nBB\n");
    }
}
